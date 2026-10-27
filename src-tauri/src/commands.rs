//! Commands the UI calls. They hold no file logic: everything goes through
//! motor and monitor. Errors are pt-BR messages ready to show.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

use motor::{
    Config, FileMeta, Rule, RuleProblem, Uuid, build_destination, compile, filter_templates,
    list_markers as motor_markers,
};
use tauri::State;

use crate::dto::{
    FieldMessage, FilterTemplateDto, MarkerDto, RuleDto, RuleField, SaveRuleResult, SettingsDto,
    TestRow, ValidationResult,
};
use crate::events::HistoryEntry;
use crate::state::AppState;
use crate::status::RuleStatus;

type CmdResult<T> = Result<T, String>;

const RULE_NOT_FOUND: &str = "Regra não encontrada. Ela pode ter sido apagada.";

fn find(config: &Config, id: Uuid) -> CmdResult<&Rule> {
    config
        .rules
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| RULE_NOT_FOUND.to_owned())
}

/// All rules, in priority order.
#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> CmdResult<Vec<RuleDto>> {
    let config = state.config();
    let mut rules: Vec<&Rule> = config.rules.iter().collect();
    rules.sort_by_key(|r| r.priority);
    Ok(rules.into_iter().map(RuleDto::from).collect())
}

#[tauri::command]
pub fn get_rule(state: State<'_, AppState>, id: Uuid) -> CmdResult<RuleDto> {
    find(&state.config(), id).map(RuleDto::from)
}

/// Creates or updates a rule: validates it (including cycles with the other
/// rules), writes config.json and restarts its watcher.
#[tauri::command]
pub fn save_rule(state: State<'_, AppState>, rule: RuleDto) -> CmdResult<SaveRuleResult> {
    let mut config = state.config();
    let mut rule = rule;
    let exists = rule
        .id
        .is_some_and(|id| config.rules.iter().any(|r| r.id == id));
    if !exists {
        // New rules go last.
        rule.priority = config
            .rules
            .iter()
            .map(|r| r.priority + 1)
            .max()
            .unwrap_or(0);
    }
    let rule = rule.into_rule();

    let mut candidate = config.clone();
    let warnings = candidate
        .save_rule(rule.clone())
        .map_err(|e| e.to_string())?;
    state.commit(&mut config, candidate)?;
    Ok(SaveRuleResult {
        rule: RuleDto::from(&rule),
        warnings: warnings.into_iter().map(|w| w.message_pt).collect(),
    })
}

#[tauri::command]
pub fn delete_rule(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    let mut config = state.config();
    find(&config, id)?;
    let mut candidate = config.clone();
    candidate.rules.retain(|r| r.id != id);
    state.commit(&mut config, candidate)
}

/// Turning a rule on validates it like a save; turning it off always works.
#[tauri::command]
pub fn toggle_rule(state: State<'_, AppState>, id: Uuid, enabled: bool) -> CmdResult<RuleDto> {
    let mut config = state.config();
    let rule = Rule {
        enabled,
        ..find(&config, id)?.clone()
    };
    let mut candidate = config.clone();
    if enabled {
        candidate
            .save_rule(rule.clone())
            .map_err(|e| e.to_string())?;
    } else if let Some(r) = candidate.rules.iter_mut().find(|r| r.id == id) {
        r.enabled = false;
    }
    state.commit(&mut config, candidate)?;
    Ok(RuleDto::from(&rule))
}

/// Sets priorities from the order of `ids`, which must list every rule once.
#[tauri::command]
pub fn reorder_rules(state: State<'_, AppState>, ids: Vec<Uuid>) -> CmdResult<Vec<RuleDto>> {
    let mut config = state.config();
    let unique: HashSet<Uuid> = ids.iter().copied().collect();
    let known: HashSet<Uuid> = config.rules.iter().map(|r| r.id).collect();
    if unique.len() != ids.len() || unique != known {
        return Err("A lista de regras mudou. Recarregue a tela e tente de novo.".to_owned());
    }

    let mut candidate = config.clone();
    candidate
        .rules
        .sort_by_key(|r| ids.iter().position(|id| *id == r.id));
    for (priority, rule) in (0u32..).zip(candidate.rules.iter_mut()) {
        rule.priority = priority;
    }
    state.commit(&mut config, candidate)?;
    Ok(config.rules.iter().map(RuleDto::from).collect())
}

/// Dry run of a rule, saved or not, on the files in its source folder.
#[tauri::command]
pub async fn test_rule(rule: RuleDto) -> CmdResult<Vec<TestRow>> {
    let rule = rule.into_rule();
    if rule.source_dir.as_os_str().is_empty() {
        return Err(RuleProblem::SourceDirEmpty.to_string());
    }
    let rows =
        tauri::async_runtime::spawn_blocking(move || motor::test_rule(&rule, &rule.source_dir))
            .await
            .map_err(|e| format!("Falha interna ao testar a regra: {e}"))?
            .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|(relative_path, outcome)| TestRow {
            relative_path,
            outcome,
        })
        .collect())
}

/// Live feedback for the rule editor: the rule's own problems, plus cycles
/// (errors) and chains (warnings) with the other saved rules.
#[tauri::command]
pub fn validate_rule(state: State<'_, AppState>, rule: RuleDto) -> CmdResult<ValidationResult> {
    let rule = rule.into_rule();
    let message = |field, text: String| FieldMessage {
        field,
        message: text,
    };
    let mut result = ValidationResult::default();
    if rule.name.trim().is_empty() {
        result.errors.push(message(
            RuleField::Name,
            "Dê um nome para a regra.".to_owned(),
        ));
    }
    result.errors.extend(
        rule.validate()
            .iter()
            .map(|p| message(problem_field(p), p.to_string())),
    );
    if let Err(e) = compile(&rule.filter) {
        result
            .errors
            .push(message(RuleField::Filter, e.to_string()));
    }

    let mut candidate = state.config().clone();
    match candidate.rules.iter_mut().find(|r| r.id == rule.id) {
        Some(existing) => *existing = rule.clone(),
        None => candidate.rules.push(rule.clone()),
    }
    let report = candidate.validate_all();
    result.errors.extend(
        report
            .cycles
            .into_iter()
            .filter(|c| c.rule_ids.contains(&rule.id))
            .map(|c| message(RuleField::Destination, c.message_pt)),
    );
    result.warnings.extend(
        report
            .warnings
            .into_iter()
            .filter(|w| w.rule_id == rule.id || w.other_rule_id == rule.id)
            .map(|w| message(RuleField::Destination, w.message_pt)),
    );
    Ok(result)
}

fn problem_field(problem: &RuleProblem) -> RuleField {
    match problem {
        RuleProblem::SourceDirEmpty | RuleProblem::SourceDirNotAbsolute => RuleField::SourceDir,
        RuleProblem::DestinationEmpty
        | RuleProblem::DestinationNeedsFixedFolder
        | RuleProblem::DestinationIsSource
        | RuleProblem::DestinationInsideWatchedTree => RuleField::Destination,
        RuleProblem::IntervalZero => RuleField::CheckMode,
        RuleProblem::FilterHasNoDate => RuleField::DateSource,
    }
}

/// Files in `dir` (and its subfolders if asked), as paths relative to it,
/// for sample names in the editor. With `filter`, only matching files.
#[tauri::command]
pub async fn list_folder_files(
    dir: PathBuf,
    include_subfolders: bool,
    filter: Option<String>,
    limit: Option<usize>,
) -> CmdResult<Vec<String>> {
    if dir.as_os_str().is_empty() {
        return Err(RuleProblem::SourceDirEmpty.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let filter = filter
            .map(|f| compile(&f))
            .transpose()
            .map_err(|e| e.to_string())?;
        let files = motor::plan::list_files(&dir, include_subfolders).map_err(|e| e.to_string())?;
        Ok(files
            .into_iter()
            .filter(|label| filter.as_ref().is_none_or(|f| f.matches(label).is_some()))
            .take(limit.unwrap_or(200))
            .collect())
    })
    .await
    .map_err(|e| format!("Falha interna ao ler a pasta: {e}"))?
}

/// Where a file with this name would go, e.g. "D:\Jornais\2026\10\20261022.pdf".
/// Dates from the file's modification time use today in the preview.
#[tauri::command]
pub fn preview_destination(rule: RuleDto, sample_file_name: String) -> CmdResult<String> {
    let rule = rule.into_rule();
    let filter = compile(&rule.filter).map_err(|e| e.to_string())?;
    let captures = filter
        .matches(&sample_file_name)
        .ok_or_else(|| "O nome de exemplo não corresponde ao filtro.".to_owned())?;
    let now = SystemTime::now();
    let meta = FileMeta {
        modified: Some(now),
        detected: now,
    };
    let dir = build_destination(&rule, &captures, &meta).map_err(|e| e.to_string())?;
    let name = sample_file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&sample_file_name);
    Ok(dir.join(name).display().to_string())
}

#[tauri::command]
pub fn list_markers() -> CmdResult<Vec<MarkerDto>> {
    Ok(motor_markers().iter().map(MarkerDto::from).collect())
}

#[tauri::command]
pub fn list_filter_templates() -> CmdResult<Vec<FilterTemplateDto>> {
    Ok(filter_templates()
        .iter()
        .map(FilterTemplateDto::from)
        .collect())
}

/// The latest events, newest first (at most 500 are kept).
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CmdResult<Vec<HistoryEntry>> {
    Ok(state.history.recent(limit.unwrap_or(100)))
}

/// Ok / paused / error and last processed file of every rule, in rule order.
#[tauri::command]
pub fn get_rule_statuses(state: State<'_, AppState>) -> CmdResult<Vec<RuleStatus>> {
    let ids: Vec<Uuid> = state.config().rules.iter().map(|r| r.id).collect();
    Ok(state.statuses.get(ids))
}

/// Restarts a rule the circuit breaker paused.
#[tauri::command]
pub fn resume_rule(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    find(&state.config(), id)?;
    state.resume(id);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsDto> {
    Ok(SettingsDto::from(&state.config().settings))
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: SettingsDto) -> CmdResult<SettingsDto> {
    let mut config = state.config();
    let mut candidate = config.clone();
    candidate.settings = settings.into();
    state.commit(&mut config, candidate)?;
    Ok(SettingsDto::from(&config.settings))
}
