//! Rule data model and validation.

use std::io;
use std::path::{Component, Path, PathBuf, Prefix};

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
    /// Also watch every subfolder of `source_dir` (except hidden and system
    /// folders). The destination must then be outside `source_dir`.
    #[serde(default)]
    pub include_subfolders: bool,
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
    #[error(
        "O destino precisa começar por uma pasta fixa, escolhida com o botão 'Escolher pasta base'."
    )]
    DestinationNeedsFixedFolder,
    #[error("A pasta de destino não pode ser a própria pasta monitorada.")]
    DestinationIsSource,
    #[error(
        "Com 'Incluir subpastas' ativado, a pasta de destino precisa ficar fora da pasta monitorada."
    )]
    DestinationInsideWatchedTree,
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
        } else if !self.destination_prefix().is_absolute() {
            problems.push(RuleProblem::DestinationNeedsFixedFolder);
        } else if source_ok {
            problems.extend(self.destination_overlap());
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

impl Rule {
    /// The fixed part of the destination, before the first marker.
    pub(crate) fn destination_prefix(&self) -> &Path {
        Path::new(fixed_prefix(&self.destination))
    }

    pub(crate) fn destination_has_markers(&self) -> bool {
        self.destination.contains('{')
    }

    /// Whether files sent to this rule's destination could land in the area
    /// watched by `watcher` (which may be this same rule). Textual: only the
    /// fixed prefix is known before marker expansion.
    pub(crate) fn destination_reaches(&self, watcher: &Rule) -> bool {
        let source = resolve(&watcher.source_dir);
        let prefix = resolve(self.destination_prefix());
        if watcher.include_subfolders {
            prefix.starts_with(&source)
        } else {
            // Only the top level is watched: subfolders created by markers are safe.
            !self.destination_has_markers() && prefix == source
        }
    }

    fn destination_overlap(&self) -> Option<RuleProblem> {
        if !self.destination_reaches(self) {
            return None;
        }
        Some(if self.include_subfolders {
            RuleProblem::DestinationInsideWatchedTree
        } else {
            RuleProblem::DestinationIsSource
        })
    }

    /// Whether `file` is in this rule's watched area: directly inside
    /// `source_dir` or, with `include_subfolders`, anywhere below it outside
    /// hidden and system folders. Lexical, so it costs no disk access.
    pub fn watches(&self, file: &Path) -> bool {
        let Some(parent) = file.parent() else {
            return false;
        };
        let source = normalize(&self.source_dir);
        let parent = normalize(parent);
        if parent == source {
            return true;
        }
        self.include_subfolders
            && parent.starts_with(&source)
            && parent[source.len()..]
                .iter()
                .all(|name| !is_ignored_folder(name))
    }
}

/// Folders never entered when watching subfolders: hidden ones and the
/// Windows system folders.
pub fn is_ignored_folder(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
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

/// Resolves `.` and `..` without touching the disk.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Comparable components of `path` after resolving `.` and `..`, case-folded
/// on Windows where paths are case-insensitive.
pub(crate) fn normalize(path: &Path) -> Vec<String> {
    key(&lexical(path))
}

/// Like [`normalize`], but the longest existing part of an absolute path is
/// canonicalized first, so symlinks compare equal to their targets.
pub(crate) fn resolve(path: &Path) -> Vec<String> {
    resolve_with(path, |p| std::fs::canonicalize(p))
}

pub(crate) fn resolve_with(
    path: &Path,
    canonicalize: impl Fn(&Path) -> io::Result<PathBuf>,
) -> Vec<String> {
    let path = lexical(path);
    if path.is_absolute() {
        for ancestor in path.ancestors() {
            if let Ok(real) = canonicalize(ancestor) {
                let rest = path.strip_prefix(ancestor).unwrap_or(Path::new(""));
                return key(&lexical(&real.join(rest)));
            }
        }
    }
    key(&path)
}

fn key(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| match component {
            Component::Prefix(prefix) => prefix_key(prefix.kind()),
            other => fold_case(&other.as_os_str().to_string_lossy()),
        })
        .collect()
}

/// `C:` and `\\?\C:` (what canonicalize returns on Windows) are the same drive.
fn prefix_key(prefix: Prefix) -> String {
    match prefix {
        Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
            format!("{}:", char::from(drive).to_ascii_lowercase())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => fold_case(&format!(
            r"\\{}\{}",
            server.to_string_lossy(),
            share.to_string_lossy()
        )),
        other => fold_case(&format!("{other:?}")),
    }
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
            include_subfolders: false,
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
    fn destination_must_start_with_a_fixed_folder() {
        for destination in [
            "{cliente}/{YYYY}",
            "{YYYY}/{MM}",
            "Jornais/{YYYY}",
            "Arquivo",
            "./Jornais",
        ] {
            assert_eq!(
                problems(&abs("home/u/NEWS"), destination),
                [RuleProblem::DestinationNeedsFixedFolder],
                "{destination}"
            );
        }
    }

    /// (include_subfolders, destination, expected problem)
    fn check_destinations(source: &str, cases: &[(bool, String, Option<RuleProblem>)]) {
        for (include_subfolders, destination, expected) in cases {
            let mut r = rule(source, destination);
            r.include_subfolders = *include_subfolders;
            assert_eq!(
                r.validate(),
                expected.iter().cloned().collect::<Vec<_>>(),
                "subpastas={include_subfolders} destino={destination}"
            );
        }
    }

    #[test]
    fn destination_against_source() {
        use RuleProblem::{DestinationInsideWatchedTree as Inside, DestinationIsSource as Same};
        let news = |rest: &str| abs(&format!("home/u/NEWS{rest}"));
        check_destinations(
            &news(""),
            &[
                // Top level only: subfolders are fine, the folder itself is not.
                (false, news("/{YYYY}/{MM}"), None),
                (false, news("/arquivo"), None),
                (false, news(""), Some(Same)),
                (false, news("/"), Some(Same)),
                (false, news("/sub/.."), Some(Same)),
                (false, abs("home/u/Outros/../NEWS"), Some(Same)),
                (false, news("{YYYY}"), None),
                // With subfolders: anything inside the tree is a loop.
                (true, news("/{YYYY}"), Some(Inside)),
                (true, news(""), Some(Inside)),
                (true, news("/a/b"), Some(Inside)),
                (true, abs("home/u/Fora/{YYYY}"), None),
                (true, news("/../Fora"), None),
                (true, news("2/{YYYY}"), None),
                (true, news("{YYYY}"), None),
                (true, abs("home/u/{YYYY}"), None),
            ],
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_destination_is_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let news = dir.path().join("NEWS");
        std::fs::create_dir(&news).unwrap();
        let link = dir.path().join("atalho");
        std::os::unix::fs::symlink(&news, &link).unwrap();

        let mut r = rule(
            news.to_str().unwrap(),
            &format!("{}/{{YYYY}}", link.display()),
        );
        r.include_subfolders = true;
        assert_eq!(r.validate(), [RuleProblem::DestinationInsideWatchedTree]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_ignore_case() {
        use RuleProblem::{DestinationInsideWatchedTree as Inside, DestinationIsSource as Same};
        check_destinations(
            r"C:\Users\Ana\NEWS",
            &[
                (false, r"c:\users\ana\news".to_owned(), Some(Same)),
                (true, r"C:\USERS\ANA\News\{YYYY}".to_owned(), Some(Inside)),
                (true, r"C:\Users\Ana\Jornais\{YYYY}".to_owned(), None),
            ],
        );
    }

    #[test]
    fn watches_top_level_or_tree() {
        let mut r = rule(&abs("home/u/NEWS"), &abs("home/u/Jornais"));
        let file = |rest: &str| PathBuf::from(abs(&format!("home/u/NEWS/{rest}")));
        assert!(r.watches(&file("a.pdf")));
        assert!(!r.watches(&file("sub/a.pdf")));
        r.include_subfolders = true;
        assert!(r.watches(&file("sub/a.pdf")));
        assert!(r.watches(&file("a/b/c.pdf")));
        assert!(!r.watches(&file(".oculta/a.pdf")));
        assert!(!r.watches(&file("$RECYCLE.BIN/a.pdf")));
        assert!(!r.watches(&file("a/System Volume Information/b.pdf")));
        assert!(!r.watches(&PathBuf::from(abs("home/u/Outros/a.pdf"))));
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
            RuleProblem::DestinationInsideWatchedTree.to_string(),
            "Com 'Incluir subpastas' ativado, a pasta de destino precisa ficar fora da pasta monitorada."
        );
    }
}
