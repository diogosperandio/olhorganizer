//! Rule data model and validation.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::filter::compile;

/// One "watch this folder and send matching files there" rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    /// Folder being watched. Must be absolute.
    pub source_dir: PathBuf,
    /// File name filter, e.g. `*.{pdf,jpg}`.
    pub filter: String,
    /// Destination folder template with markers, e.g. `D:\Jornais\{YYYY}\{MM}`.
    pub destination: String,
    pub action: Action,
    pub date_source: DateSource,
    pub check_mode: CheckMode,
    #[serde(default)]
    pub on_conflict: ConflictPolicy,
    /// Lower values run first.
    pub priority: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Move,
    Copy,
}

/// Where the date used by date markers ({YYYY}, {MM}, ...) comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateSource {
    /// Parsed from the file name.
    FileName,
    /// The file's modification time.
    Modified,
    /// The moment the file was detected (now).
    Detected,
}

/// How the source folder is checked for new files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CheckMode {
    /// React to file system events as they happen.
    Instant,
    /// Scan the folder every `minutes` minutes.
    Interval { minutes: u32 },
}

/// What to do when a file with the same name already exists at the destination.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Keep both files by giving the new one a different name.
    #[default]
    Rename,
    Overwrite,
    Skip,
}

/// A problem found by [`Rule::validate`]. Display messages are shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleProblem {
    #[error("Escolha a pasta que será monitorada.")]
    SourceDirEmpty,
    #[error(
        "A pasta monitorada precisa ser um caminho completo, começando pela unidade ou pela raiz."
    )]
    SourceDirNotAbsolute,
    #[error("Informe a pasta de destino.")]
    DestinationEmpty,
    #[error("A pasta de destino não pode ficar dentro da pasta monitorada.")]
    DestinationInsideSource,
    #[error("O intervalo de verificação precisa ser de pelo menos 1 minuto.")]
    IntervalZero,
    #[error(
        "A data vem do nome do arquivo, mas o filtro não tem data. Use {{YYYY}}, {{MM}} ou {{DD}} no filtro ou escolha outra origem para a data."
    )]
    FilterHasNoDate,
}

impl Rule {
    /// Returns every problem with the rule; an empty list means it is valid.
    pub fn validate(&self) -> Vec<RuleProblem> {
        let mut problems = Vec::new();

        let source_ok = if self.source_dir.to_string_lossy().trim().is_empty() {
            problems.push(RuleProblem::SourceDirEmpty);
            false
        } else if !self.source_dir.is_absolute() {
            problems.push(RuleProblem::SourceDirNotAbsolute);
            false
        } else {
            true
        };

        if self.destination.trim().is_empty() {
            problems.push(RuleProblem::DestinationEmpty);
        } else if source_ok && destination_inside(&self.source_dir, &self.destination) {
            problems.push(RuleProblem::DestinationInsideSource);
        }

        if self.check_mode == (CheckMode::Interval { minutes: 0 }) {
            problems.push(RuleProblem::IntervalZero);
        }

        // A filter that does not compile is reported by the filter compiler.
        if self.date_source == DateSource::FileName
            && compile(&self.filter).is_ok_and(|f| !f.has_date())
        {
            problems.push(RuleProblem::FilterHasNoDate);
        }

        problems
    }
}

/// Textual check done before marker expansion: the fixed part of the
/// destination is compared with the source folder. A relative destination is
/// taken as relative to the source folder, so it always counts as inside.
fn destination_inside(source: &Path, destination: &str) -> bool {
    let fixed = Path::new(fixed_prefix(destination));
    let resolved = if fixed.is_absolute() {
        fixed.to_path_buf()
    } else {
        source.join(fixed)
    };
    normalize(&resolved).starts_with(&normalize(source))
}

/// The destination up to the first marker, cut back to the last whole folder:
/// in `/a/b{YYYY}` the folder `b{YYYY}` is variable, so only `/a/` is fixed.
fn fixed_prefix(destination: &str) -> &str {
    let Some(marker) = destination.find('{') else {
        return destination;
    };
    let before = &destination[..marker];
    match before.rfind(std::path::is_separator) {
        Some(sep) => &before[..=sep],
        None => "",
    }
}

/// Lexically resolves `.` and `..` and returns the components, case-folded on
/// Windows where paths are case-insensitive.
pub(crate) fn normalize(path: &Path) -> Vec<String> {
    let mut parts = Vec::new();
    let mut root_len = 0;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.len() > root_len {
                    parts.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir => {
                parts.push(fold_case(&component.as_os_str().to_string_lossy()));
                root_len = parts.len();
            }
            Component::Normal(name) => parts.push(fold_case(&name.to_string_lossy())),
        }
    }
    parts
}

#[cfg(windows)]
fn fold_case(s: &str) -> String {
    s.to_lowercase()
}

#[cfg(not(windows))]
fn fold_case(s: &str) -> String {
    s.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an absolute path on the current platform from a `/`-separated one.
    fn abs(rel: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{}", rel.replace('/', "\\"))
        } else {
            format!("/{rel}")
        }
    }

    fn rule(source: &str, destination: &str) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: "Jornais".into(),
            enabled: true,
            source_dir: PathBuf::from(source),
            filter: "{YYYY}{MM}{DD}.pdf".into(),
            destination: destination.into(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        }
    }

    fn problems(source: &str, destination: &str) -> Vec<RuleProblem> {
        rule(source, destination).validate()
    }

    #[test]
    fn valid_rule_has_no_problems() {
        assert!(problems(&abs("home/u/NEWS"), &abs("home/u/Jornais/{YYYY}/{MM}")).is_empty());
    }

    #[test]
    fn empty_source_dir() {
        assert_eq!(
            problems("", &abs("home/u/Jornais")),
            [RuleProblem::SourceDirEmpty]
        );
        assert_eq!(
            problems("   ", &abs("home/u/Jornais")),
            [RuleProblem::SourceDirEmpty]
        );
    }

    #[test]
    fn relative_source_dir() {
        assert_eq!(
            problems("NEWS", &abs("home/u/Jornais")),
            [RuleProblem::SourceDirNotAbsolute]
        );
    }

    #[test]
    fn empty_destination() {
        assert_eq!(
            problems(&abs("home/u/NEWS"), "  "),
            [RuleProblem::DestinationEmpty]
        );
    }

    #[test]
    fn destination_inside_source() {
        let source = abs("home/u/NEWS");
        for destination in [
            abs("home/u/NEWS/{YYYY}/{MM}"),
            abs("home/u/NEWS/arquivo"),
            abs("home/u/NEWS"),
            abs("home/u/Outros/../NEWS/{YYYY}"),
            "{YYYY}/{MM}".to_owned(),
            "Arquivo".to_owned(),
        ] {
            assert_eq!(
                problems(&source, &destination),
                [RuleProblem::DestinationInsideSource],
                "{destination}"
            );
        }
    }

    #[test]
    fn destination_outside_source() {
        let source = abs("home/u/NEWS");
        for destination in [
            abs("home/u/NEWS{YYYY}"),
            abs("home/u/NEWS2/{YYYY}"),
            abs("home/u/NEWS/../Jornais/{YYYY}"),
            abs("home/u/{YYYY}"),
        ] {
            assert!(problems(&source, &destination).is_empty(), "{destination}");
        }
    }

    #[test]
    fn interval_zero() {
        let mut r = rule(&abs("home/u/NEWS"), &abs("home/u/Jornais"));
        r.check_mode = CheckMode::Interval { minutes: 0 };
        assert_eq!(r.validate(), [RuleProblem::IntervalZero]);
        r.check_mode = CheckMode::Interval { minutes: 1 };
        assert!(r.validate().is_empty());
    }

    #[test]
    fn file_name_date_needs_a_date_in_the_filter() {
        let mut r = rule(&abs("home/u/NEWS"), &abs("home/u/Jornais"));
        r.filter = "*.pdf".into();
        assert_eq!(r.validate(), [RuleProblem::FilterHasNoDate]);
        r.date_source = DateSource::Modified;
        assert!(r.validate().is_empty());
        r.date_source = DateSource::FileName;
        r.filter = "{YYYY}_*.pdf".into();
        assert!(r.validate().is_empty());
    }

    #[test]
    fn reports_all_problems() {
        let mut r = rule("", "");
        r.check_mode = CheckMode::Interval { minutes: 0 };
        assert_eq!(
            r.validate(),
            [
                RuleProblem::SourceDirEmpty,
                RuleProblem::DestinationEmpty,
                RuleProblem::IntervalZero,
            ]
        );
    }

    #[test]
    fn messages_are_in_portuguese() {
        assert_eq!(
            RuleProblem::DestinationInsideSource.to_string(),
            "A pasta de destino não pode ficar dentro da pasta monitorada."
        );
    }
}
