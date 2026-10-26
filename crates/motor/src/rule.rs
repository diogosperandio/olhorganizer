use std::ffi::OsString;
use std::fs;
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
    pub include_subfolders: bool,
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
        } else {
            let prefix = destination_fixed_prefix(&self.destination);
            if !destination_has_fixed_absolute_base(&prefix) {
                problems.push(
                    "O destino precisa começar por uma pasta fixa, escolhida com o botão 'Escolher pasta base'."
                        .to_owned(),
                );
            } else if self.include_subfolders && path_is_same_or_inside(&prefix, &self.source_dir) {
                problems.push(
                    "Com 'Incluir subpastas' ativado, a pasta de destino precisa ficar fora da pasta monitorada."
                        .to_owned(),
                );
            } else if !self.include_subfolders
                && !self.destination.contains('{')
                && paths_equal(&prefix, &self.source_dir)
            {
                problems
                    .push("A pasta de destino não pode ser a própria pasta monitorada.".to_owned());
            }
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

pub(crate) fn destination_fixed_prefix(destination: &str) -> PathBuf {
    PathBuf::from(
        destination
            .split_once('{')
            .map_or(destination, |(prefix, _)| prefix),
    )
}

fn destination_has_fixed_absolute_base(prefix: &Path) -> bool {
    prefix.is_absolute()
        && prefix
            .components()
            .any(|component| matches!(component, Component::Normal(_)))
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
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

    canonicalize_existing_prefix(normalized)
}

fn canonicalize_existing_prefix(path: PathBuf) -> PathBuf {
    let mut existing = path.clone();
    let mut missing = Vec::<OsString>::new();

    while !existing.as_os_str().is_empty() && !existing.exists() {
        let Some(name) = existing.file_name().map(ToOwned::to_owned) else {
            break;
        };
        missing.push(name);
        if !existing.pop() {
            break;
        }
    }

    let Ok(mut canonical) = fs::canonicalize(&existing) else {
        return path;
    };
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    canonical
}

pub(crate) fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = normalize_path(left);
    let right = normalize_path(right);
    components_equal(&left, &right)
}

pub(crate) fn path_is_same_or_inside(path: &Path, directory: &Path) -> bool {
    if !path.is_absolute() || !directory.is_absolute() {
        return false;
    }
    let path = normalize_path(path);
    let directory = normalize_path(directory);
    let path_components: Vec<_> = path.components().collect();
    let directory_components: Vec<_> = directory.components().collect();
    path_components.len() >= directory_components.len()
        && path_components
            .iter()
            .zip(directory_components.iter())
            .all(|(left, right)| component_equal(left, right))
}

fn components_equal(left: &Path, right: &Path) -> bool {
    let left: Vec<_> = left.components().collect();
    let right: Vec<_> = right.components().collect();
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| component_equal(left, right))
}

fn component_equal(left: &Component<'_>, right: &Component<'_>) -> bool {
    #[cfg(windows)]
    {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
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
            include_subfolders: false,
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
    fn rejects_destination_starting_with_a_marker() {
        let mut rule = valid_rule();
        rule.destination = "{cliente}/{YYYY}".to_owned();

        assert_eq!(
            rule.validate(),
            ["O destino precisa começar por uma pasta fixa, escolhida com o botão 'Escolher pasta base'."]
        );
    }

    #[test]
    fn rejects_relative_destination() {
        let mut rule = valid_rule();
        rule.destination = "Jornais/{YYYY}".to_owned();

        assert_eq!(
            rule.validate(),
            ["O destino precisa começar por uma pasta fixa, escolhida com o botão 'Escolher pasta base'."]
        );
    }

    #[test]
    fn allows_destination_inside_source_when_subfolders_are_off() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada/organizados/{YYYY}".to_owned();

        assert!(rule.validate().is_empty());
    }

    #[test]
    fn rejects_source_itself_when_subfolders_are_off() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada".to_owned();

        assert_eq!(
            rule.validate(),
            ["A pasta de destino não pode ser a própria pasta monitorada."]
        );
    }

    #[test]
    fn rejects_destination_inside_source_when_subfolders_are_on() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada/{YYYY}".to_owned();
        rule.include_subfolders = true;

        assert_eq!(
            rule.validate(),
            ["Com 'Incluir subpastas' ativado, a pasta de destino precisa ficar fora da pasta monitorada."]
        );
    }

    #[test]
    fn allows_destination_outside_source_when_subfolders_are_on() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/saida/{YYYY}".to_owned();
        rule.include_subfolders = true;

        assert!(rule.validate().is_empty());
    }

    #[test]
    fn normalizes_parent_components_before_comparing_paths() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from("/entrada");
        rule.destination = "/entrada/../saida/{YYYY}".to_owned();
        rule.include_subfolders = true;

        assert!(rule.validate().is_empty());
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

    #[cfg(windows)]
    #[test]
    fn compares_windows_paths_without_case_sensitivity() {
        let mut rule = valid_rule();
        rule.source_dir = PathBuf::from(r"C:\NEWS");
        rule.destination = r"c:\news\{YYYY}".to_owned();
        rule.include_subfolders = true;

        assert_eq!(rule.validate().len(), 1);
    }
}
