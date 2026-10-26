use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub source_dir: PathBuf,
    pub filter: String,
    pub destination: String,
    pub action: Action,
    pub date_source: DateSource,
    pub check_mode: CheckMode,
    #[serde(default)]
    pub on_conflict: ConflictPolicy,
    pub priority: u32,
}

impl Rule {
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();

        if self.source_dir.as_os_str().is_empty() {
            problems.push("A pasta monitorada não pode ficar vazia.".to_owned());
        } else if !self.source_dir.is_absolute() {
            problems.push("A pasta monitorada deve usar um caminho absoluto.".to_owned());
        }

        if self.destination.trim().is_empty() {
            problems.push("A pasta de destino não pode ficar vazia.".to_owned());
        } else if destination_is_inside_source(&self.destination, &self.source_dir) {
            problems
                .push("A pasta de destino não pode ficar dentro da pasta monitorada.".to_owned());
        }

        if matches!(self.check_mode, CheckMode::Interval { minutes: 0 }) {
            problems.push("O intervalo de verificação deve ser maior que zero.".to_owned());
        }

        problems
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Move,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateSource {
    FileName,
    Modified,
    Detected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckMode {
    Instant,
    Interval { minutes: u32 },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    #[default]
    Rename,
    Overwrite,
    Skip,
}

fn destination_is_inside_source(destination: &str, source: &Path) -> bool {
    if !source.is_absolute() {
        return false;
    }

    let fixed_prefix = destination
        .split_once('{')
        .map_or(destination, |(prefix, _)| prefix);
    let destination_prefix = Path::new(fixed_prefix);
    if !destination_prefix.is_absolute() {
        return false;
    }

    normalize_path(destination_prefix).starts_with(normalize_path(source))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rule() -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: "Documentos".to_owned(),
            enabled: true,
            source_dir: PathBuf::from("/entrada"),
            filter: "*.pdf".to_owned(),
            destination: "/saida/{YYYY}/{MM}".to_owned(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            on_conflict: ConflictPolicy::default(),
            priority: 0,
        }
    }

    #[test]
    fn rejects_empty_source_directory() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::new();

        assert_eq!(
            rule.validate(),
            ["A pasta monitorada não pode ficar vazia."]
        );
    }

    #[test]
    fn rejects_relative_source_directory() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("entrada");

        assert_eq!(
            rule.validate(),
            ["A pasta monitorada deve usar um caminho absoluto."]
        );
    }

    #[test]
    fn rejects_empty_destination() {
        let mut rule = valid_rule();
        rule.destination = "  ".to_owned();

        assert_eq!(
            rule.validate(),
            ["A pasta de destino não pode ficar vazia."]
        );
    }

    #[test]
    fn rejects_destination_inside_source_directory() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada/organizados/{YYYY}".to_owned();

        assert_eq!(
            rule.validate(),
            ["A pasta de destino não pode ficar dentro da pasta monitorada."]
        );
    }

    #[test]
    fn allows_destination_with_similar_directory_name() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada-organizada/{YYYY}".to_owned();

        assert!(rule.validate().is_empty());
    }

    #[test]
    fn rejects_zero_minute_interval() {
        let mut rule = valid_rule();
        rule.check_mode = CheckMode::Interval { minutes: 0 };

        assert_eq!(
            rule.validate(),
            ["O intervalo de verificação deve ser maior que zero."]
        );
    }

    #[test]
    fn reports_all_validation_problems() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::new();
        rule.destination = String::new();
        rule.check_mode = CheckMode::Interval { minutes: 0 };

        assert_eq!(rule.validate().len(), 3);
    }
}
