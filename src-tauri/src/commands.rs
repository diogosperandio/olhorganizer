use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use motor::{
    Action, CheckMode, Config, ConflictPolicy, DateSource, FilterTemplate, MarkerDef, MarkerKind,
    Rule, Settings, Uuid,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::events::OutcomePayload;
use crate::history::HistoryEntry;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub source_dir: PathBuf,
    pub filter: String,
    pub destination: String,
    pub action: Action,
    pub date_source: DateSource,
    pub check_mode: CheckMode,
    pub include_subfolders: bool,
    pub on_conflict: ConflictPolicy,
    pub priority: u32,
}

impl From<Rule> for RuleDto {
    fn from(rule: Rule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            enabled: rule.enabled,
            source_dir: rule.source_dir,
            filter: rule.filter,
            destination: rule.destination,
            action: rule.action,
            date_source: rule.date_source,
            check_mode: rule.check_mode,
            include_subfolders: rule.include_subfolders,
            on_conflict: rule.on_conflict,
            priority: rule.priority,
        }
    }
}

impl From<RuleDto> for Rule {
    fn from(rule: RuleDto) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            enabled: rule.enabled,
            source_dir: rule.source_dir,
            filter: rule.filter,
            destination: rule.destination,
            action: rule.action,
            date_source: rule.date_source,
            check_mode: rule.check_mode,
            include_subfolders: rule.include_subfolders,
            on_conflict: rule.on_conflict,
            priority: rule.priority,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub check_updates_on_start: bool,
    pub skipped_version: Option<String>,
    pub notifications: bool,
    pub start_with_system: bool,
}

impl From<Settings> for SettingsDto {
    fn from(settings: Settings) -> Self {
        Self {
            check_updates_on_start: settings.check_updates_on_start,
            skipped_version: settings.skipped_version,
            notifications: settings.notifications,
            start_with_system: settings.start_with_system,
        }
    }
}

impl From<SettingsDto> for Settings {
    fn from(settings: SettingsDto) -> Self {
        Self {
            check_updates_on_start: settings.check_updates_on_start,
            skipped_version: settings.skipped_version,
            notifications: settings.notifications,
            start_with_system: settings.start_with_system,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRow {
    pub relative_path: String,
    pub outcome: OutcomePayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    Replace,
    Add,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkerDto {
    pub token: String,
    pub kind: String,
    pub description_pt: String,
    pub example: String,
}

impl From<&MarkerDef> for MarkerDto {
    fn from(marker: &MarkerDef) -> Self {
        Self {
            token: marker.token.to_owned(),
            kind: marker_kind_name(marker.kind).to_owned(),
            description_pt: marker.description_pt.to_owned(),
            example: marker.example.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterTemplateDto {
    pub name_pt: String,
    pub filter: String,
}

impl From<&FilterTemplate> for FilterTemplateDto {
    fn from(template: &FilterTemplate) -> Self {
        Self {
            name_pt: template.name_pt.to_owned(),
            filter: template.filter.to_owned(),
        }
    }
}

#[tauri::command]
pub async fn list_rules(state: State<'_, AppState>) -> Result<Vec<RuleDto>, String> {
    Ok(state
        .config
        .lock()
        .await
        .rules
        .clone()
        .into_iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub async fn get_rule(id: Uuid, state: State<'_, AppState>) -> Result<RuleDto, String> {
    state
        .config
        .lock()
        .await
        .rules
        .iter()
        .find(|rule| rule.id == id)
        .cloned()
        .map(Into::into)
        .ok_or_else(|| "A regra solicitada não foi encontrada.".to_owned())
}

#[tauri::command]
pub async fn save_rule(rule: RuleDto, state: State<'_, AppState>) -> Result<RuleDto, String> {
    let _mutation = state.mutation.lock().await;
    let rule: Rule = rule.into();
    let problems = rule.validate();
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let mut candidate = state.config.lock().await.clone();
    if let Some(saved) = candidate.rules.iter_mut().find(|saved| saved.id == rule.id) {
        *saved = rule.clone();
    } else {
        candidate.rules.push(rule.clone());
    }
    ensure_no_cycles(&candidate)?;
    persist_and_reload(&state, candidate).await?;
    Ok(rule.into())
}

#[tauri::command]
pub async fn delete_rule(id: Uuid, state: State<'_, AppState>) -> Result<(), String> {
    let _mutation = state.mutation.lock().await;
    let mut candidate = state.config.lock().await.clone();
    let original_len = candidate.rules.len();
    candidate.rules.retain(|rule| rule.id != id);
    if candidate.rules.len() == original_len {
        return Err("A regra solicitada não foi encontrada.".to_owned());
    }
    persist_and_reload(&state, candidate).await
}

#[tauri::command]
pub async fn toggle_rule(
    id: Uuid,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<RuleDto, String> {
    let _mutation = state.mutation.lock().await;
    let mut candidate = state.config.lock().await.clone();
    let Some(rule) = candidate.rules.iter_mut().find(|rule| rule.id == id) else {
        return Err("A regra solicitada não foi encontrada.".to_owned());
    };
    rule.enabled = enabled;
    let changed = rule.clone();
    ensure_no_cycles(&candidate)?;
    persist_and_reload(&state, candidate).await?;
    Ok(changed.into())
}

#[tauri::command]
pub async fn reorder_rules(
    ids: Vec<Uuid>,
    state: State<'_, AppState>,
) -> Result<Vec<RuleDto>, String> {
    let _mutation = state.mutation.lock().await;
    let mut candidate = state.config.lock().await.clone();
    let unique: HashSet<_> = ids.iter().copied().collect();
    if ids.len() != candidate.rules.len()
        || unique.len() != ids.len()
        || candidate
            .rules
            .iter()
            .any(|rule| !unique.contains(&rule.id))
    {
        return Err("A nova ordem precisa conter cada regra exatamente uma vez.".to_owned());
    }
    let positions: std::collections::HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    candidate
        .rules
        .sort_by_key(|rule| positions.get(&rule.id).copied().unwrap_or(usize::MAX));
    for (index, rule) in candidate.rules.iter_mut().enumerate() {
        rule.priority =
            u32::try_from(index).map_err(|_| "Há regras demais para ordenar.".to_owned())?;
    }
    let response = candidate.rules.clone();
    persist_and_reload(&state, candidate).await?;
    Ok(response.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn test_rule(rule: RuleDto) -> Result<Vec<TestRow>, String> {
    let rule: Rule = rule.into();
    let directory = rule.source_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        motor::test_rule(&rule, &directory)
            .into_iter()
            .map(|(relative_path, outcome)| TestRow {
                relative_path,
                outcome: outcome.into(),
            })
            .collect()
    })
    .await
    .map_err(|error| format!("Não foi possível testar a regra: {error}"))
}

#[tauri::command]
pub async fn validate_rule(
    rule: RuleDto,
    state: State<'_, AppState>,
) -> Result<ValidationResult, String> {
    let rule: Rule = rule.into();
    let mut errors = rule.validate();
    let mut candidate = state.config.lock().await.clone();
    if let Some(saved) = candidate.rules.iter_mut().find(|saved| saved.id == rule.id) {
        *saved = rule;
    } else {
        candidate.rules.push(rule);
    }
    let cross_rule = candidate.validate_all();
    errors.extend(cross_rule.errors);
    Ok(ValidationResult {
        errors,
        warnings: cross_rule.warnings,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn preview_destination(
    rule: RuleDto,
    sample_file_name: String,
) -> Result<String, String> {
    motor::preview_destination(&rule.into(), &sample_file_name)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_markers() -> Result<Vec<MarkerDto>, String> {
    Ok(motor::list_markers().iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_filter_templates() -> Result<Vec<FilterTemplateDto>, String> {
    Ok(motor::list_filter_templates()
        .iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub async fn get_history(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<HistoryEntry>, String> {
    Ok(state.history.recent(limit))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsDto, String> {
    Ok(state.config.lock().await.settings.clone().into())
}

#[tauri::command]
pub async fn save_settings(
    settings: SettingsDto,
    state: State<'_, AppState>,
) -> Result<SettingsDto, String> {
    let _mutation = state.mutation.lock().await;
    let mut candidate = state.config.lock().await.clone();
    candidate.settings = settings.clone().into();
    persist_and_reload(&state, candidate).await?;
    Ok(settings)
}

#[tauri::command]
pub async fn export_rules(path: PathBuf, state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().await.clone();
    tauri::async_runtime::spawn_blocking(move || config.save(&path))
        .await
        .map_err(|error| format!("Não foi possível concluir a exportação: {error}"))?
        .map_err(|error| format!("Não foi possível exportar as regras: {error}"))
}

#[tauri::command]
pub async fn import_rules(
    path: PathBuf,
    mode: ImportMode,
    state: State<'_, AppState>,
) -> Result<Vec<RuleDto>, String> {
    let imported = tauri::async_runtime::spawn_blocking(move || {
        if !path.is_file() {
            return Err("O arquivo escolhido não existe ou não pode ser lido.".to_owned());
        }
        Config::import(&path)
            .map_err(|error| format!("Não foi possível importar as regras: {error}"))
    })
    .await
    .map_err(|error| format!("Não foi possível concluir a importação: {error}"))??;

    let _mutation = state.mutation.lock().await;
    let mut candidate = state.config.lock().await.clone();
    match mode {
        ImportMode::Replace => candidate.rules = imported.rules,
        ImportMode::Add => append_imported_rules(&mut candidate, imported.rules)?,
    }
    validate_candidate(&candidate)?;
    let response = candidate.rules.clone();
    persist_and_reload(&state, candidate).await?;
    Ok(response.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn get_log_path(state: State<'_, AppState>) -> Result<String, String> {
    let log_dir = state.log_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let entries = fs::read_dir(&log_dir)
            .map_err(|error| format!("Não foi possível acessar a pasta de logs: {error}"))?;
        let mut latest: Option<(SystemTime, PathBuf)> = None;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Não foi possível ler um arquivo de log: {error}"))?;
            let path = entry.path();
            let is_log = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("olhorganizer.log"));
            if !is_log {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if latest
                .as_ref()
                .is_none_or(|(current, _)| modified > *current)
            {
                latest = Some((modified, path));
            }
        }
        latest
            .map(|(_, path)| path.to_string_lossy().into_owned())
            .ok_or_else(|| "Ainda não há um arquivo de log para abrir.".to_owned())
    })
    .await
    .map_err(|error| format!("Não foi possível procurar o arquivo de log: {error}"))?
}

fn append_imported_rules(config: &mut Config, rules: Vec<Rule>) -> Result<(), String> {
    let mut ids: HashSet<_> = config.rules.iter().map(|rule| rule.id).collect();
    let mut priority = config
        .rules
        .iter()
        .map(|rule| rule.priority)
        .max()
        .unwrap_or(0);
    for mut rule in rules {
        if ids.contains(&rule.id) {
            rule.id = Uuid::new_v4();
        }
        ids.insert(rule.id);
        priority = priority
            .checked_add(1)
            .ok_or_else(|| "Há regras demais para importar.".to_owned())?;
        rule.priority = priority;
        config.rules.push(rule);
    }
    Ok(())
}

fn validate_candidate(config: &Config) -> Result<(), String> {
    for rule in &config.rules {
        let problems = rule.validate();
        if !problems.is_empty() {
            return Err(format!(
                "A regra '{}' do arquivo importado não é válida: {}",
                rule.name,
                problems.join(" ")
            ));
        }
    }
    ensure_no_cycles(config)
}

async fn persist_and_reload(state: &AppState, candidate: Config) -> Result<(), String> {
    candidate
        .save(&state.config_path)
        .map_err(|error| format!("Não foi possível salvar a configuração: {error}"))?;
    *state.config.lock().await = candidate.clone();
    state.monitor.lock().await.reload(candidate).await;
    Ok(())
}

fn ensure_no_cycles(config: &Config) -> Result<(), String> {
    let validation = config.validate_all();
    if validation.errors.is_empty() {
        Ok(())
    } else {
        Err(validation.errors.join("\n"))
    }
}

fn marker_kind_name(kind: MarkerKind) -> &'static str {
    match kind {
        MarkerKind::Year4 => "year4",
        MarkerKind::Year2 => "year2",
        MarkerKind::Month2 => "month2",
        MarkerKind::MonthName => "monthName",
        MarkerKind::Day2 => "day2",
        MarkerKind::Name => "name",
        MarkerKind::Ext => "ext",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: Uuid, name: &str, priority: u32) -> Rule {
        let root = std::env::temp_dir().join("olhorganizer-import-tests");
        Rule {
            id,
            name: name.to_owned(),
            enabled: true,
            source_dir: root.join(name).join("entrada"),
            filter: "*.pdf".to_owned(),
            destination: root.join(name).join("saida").to_string_lossy().into_owned(),
            action: Action::Move,
            date_source: DateSource::Modified,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority,
        }
    }

    #[test]
    fn adding_imported_rules_resolves_ids_and_priorities() {
        let shared_id = Uuid::new_v4();
        let mut config = Config::default();
        config.rules.push(rule(shared_id, "Atual", 7));

        append_imported_rules(&mut config, vec![rule(shared_id, "Importada", 0)])
            .expect("import should be merged");

        assert_eq!(config.rules.len(), 2);
        assert_ne!(config.rules[0].id, config.rules[1].id);
        assert_eq!(config.rules[1].priority, 8);
    }

    #[test]
    fn invalid_import_candidate_is_rejected() {
        let mut config = Config::default();
        let mut invalid = rule(Uuid::new_v4(), "Inválida", 0);
        invalid.source_dir = PathBuf::from("pasta-relativa");
        config.rules.push(invalid);

        let error = validate_candidate(&config).expect_err("invalid rule must be rejected");

        assert!(error.contains("não é válida"));
    }
}
