//! Shapes exchanged with the UI. config.json keeps motor's snake_case field
//! names; the UI sees camelCase. src/lib/types.ts mirrors these by hand and
//! must be kept in sync.

use std::path::PathBuf;

use motor::ops::Outcome;
use motor::{
    Action, CheckMode, ConflictPolicy, DateSource, FilterTemplate, MarkerDef, MarkerKind, Rule,
    Settings, Uuid,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    /// `None` for a rule the UI has not saved yet.
    #[serde(default)]
    pub id: Option<Uuid>,
    pub name: String,
    pub enabled: bool,
    pub source_dir: PathBuf,
    #[serde(default)]
    pub include_subfolders: bool,
    pub filter: String,
    pub destination: String,
    pub action: Action,
    pub date_source: DateSource,
    pub check_mode: CheckMode,
    #[serde(default)]
    pub on_conflict: ConflictPolicy,
    #[serde(default)]
    pub priority: u32,
}

impl From<&Rule> for RuleDto {
    fn from(rule: &Rule) -> Self {
        Self {
            id: Some(rule.id),
            name: rule.name.clone(),
            enabled: rule.enabled,
            source_dir: rule.source_dir.clone(),
            include_subfolders: rule.include_subfolders,
            filter: rule.filter.clone(),
            destination: rule.destination.clone(),
            action: rule.action,
            date_source: rule.date_source,
            check_mode: rule.check_mode,
            on_conflict: rule.on_conflict,
            priority: rule.priority,
        }
    }
}

impl RuleDto {
    /// The engine rule; an unsaved rule gets a fresh id.
    pub fn into_rule(self) -> Rule {
        Rule {
            id: self.id.unwrap_or_else(Uuid::new_v4),
            name: self.name,
            enabled: self.enabled,
            source_dir: self.source_dir,
            include_subfolders: self.include_subfolders,
            filter: self.filter,
            destination: self.destination,
            action: self.action,
            date_source: self.date_source,
            check_mode: self.check_mode,
            on_conflict: self.on_conflict,
            priority: self.priority,
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

impl From<&Settings> for SettingsDto {
    fn from(s: &Settings) -> Self {
        Self {
            check_updates_on_start: s.check_updates_on_start,
            skipped_version: s.skipped_version.clone(),
            notifications: s.notifications,
            start_with_system: s.start_with_system,
        }
    }
}

impl From<SettingsDto> for Settings {
    fn from(s: SettingsDto) -> Self {
        Self {
            check_updates_on_start: s.check_updates_on_start,
            skipped_version: s.skipped_version,
            notifications: s.notifications,
            start_with_system: s.start_with_system,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleResult {
    pub rule: RuleDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// One file of a "Testar regra" dry run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRow {
    /// Relative to the source folder, `/`-separated: `2025/fotos/IMG_0001.jpg`.
    pub relative_path: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkerDto {
    pub token: &'static str,
    pub kind: MarkerKind,
    pub description_pt: &'static str,
    pub example: &'static str,
    pub usable_in_filter: bool,
}

impl From<&MarkerDef> for MarkerDto {
    fn from(m: &MarkerDef) -> Self {
        Self {
            token: m.token,
            kind: m.kind,
            description_pt: m.description_pt,
            example: m.example,
            usable_in_filter: m.kind.usable_in_filter(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterTemplateDto {
    pub name_pt: &'static str,
    pub filter: &'static str,
}

impl From<&FilterTemplate> for FilterTemplateDto {
    fn from(t: &FilterTemplate) -> Self {
        Self {
            name_pt: t.name_pt,
            filter: t.filter,
        }
    }
}
