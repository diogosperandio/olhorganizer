//! Rule selection and the "Testar regra" dry run.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::MotorError;
use crate::filter::compile;
use crate::ops::{FileSystem, Outcome, StdFs, process_with};
use crate::rule::{Rule, is_ignored_folder};

/// Dry-runs `rule` on every file directly inside `dir` (not subfolders),
/// sorted by name. `dir` replaces the rule's source folder, so a folder can be
/// tested before the rule is saved. Nothing on disk is changed.
pub fn test_rule(rule: &Rule, dir: &Path) -> Result<Vec<(String, Outcome)>, MotorError> {
    test_rule_with(&StdFs, rule, dir)
}

pub fn test_rule_with(
    fs: &impl FileSystem,
    rule: &Rule,
    dir: &Path,
) -> Result<Vec<(String, Outcome)>, MotorError> {
    let rule = Rule {
        source_dir: dir.to_path_buf(),
        ..rule.clone()
    };
    let mut files = Vec::new();
    collect_files(fs, dir, rule.include_subfolders, &mut files).map_err(|source| {
        MotorError::Io {
            path: dir.to_path_buf(),
            source,
        }
    })?;

    let mut results: Vec<(String, Outcome)> = files
        .iter()
        .map(|path| {
            (
                relative_label(dir, path),
                process_with(fs, path, &rule, true),
            )
        })
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Files in `dir`, and below it when `recursive`, without following symlinked
/// folders or entering hidden and system folders. Unreadable subfolders are
/// skipped; only an unreadable `dir` is an error.
fn collect_files(
    fs: &impl FileSystem,
    dir: &Path,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for path in fs.list_dir(dir)? {
        let Ok(meta) = fs.symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let ignored = path
                .file_name()
                .is_some_and(|n| is_ignored_folder(&n.to_string_lossy()));
            if recursive && !ignored {
                let _ = collect_files(fs, &path, true, out);
            }
        } else if fs.metadata(&path).is_ok_and(|m| m.is_file()) {
            out.push(path);
        }
    }
    Ok(())
}

/// `path` relative to `dir`, with `/` separators: `2025/fotos/IMG_0001.jpg`.
fn relative_label(dir: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(dir).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The first enabled rule, lowest priority value first, that watches the
/// folder `file` is in (see [`Rule::watches`]) and whose filter matches its name.
pub fn pick_rule<'a>(rules: &'a [Rule], file: &Path) -> Option<&'a Rule> {
    let name = file.file_name()?.to_str()?;

    let mut candidates: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    candidates.sort_by_key(|r| r.priority);
    candidates
        .into_iter()
        .find(|r| r.watches(file) && compile(&r.filter).is_ok_and(|f| f.matches(name).is_some()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::rule::{Action, CheckMode, ConflictPolicy, DateSource};
    use crate::testutil::snapshot;

    fn rule(name: &str, source: &Path, filter: &str, priority: u32, enabled: bool) -> Rule {
        Rule {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            enabled,
            source_dir: source.to_path_buf(),
            include_subfolders: false,
            filter: filter.into(),
            destination: format!(
                "{}/{{YYYY}}/{{MM}}",
                source.with_file_name("Jornais").display()
            ),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            on_conflict: ConflictPolicy::Rename,
            priority,
        }
    }

    #[test]
    fn pick_rule_follows_priority() {
        let news = PathBuf::from("/home/u/NEWS");
        let other = PathBuf::from("/home/u/Outros");
        let rules = [
            rule("qualquer", &news, "*", 5, true),
            rule("desligada", &news, "*.pdf", 0, false),
            rule("outra pasta", &other, "*.pdf", 0, true),
            rule("pdf", &news, "*.pdf", 2, true),
            rule("data", &news, "{YYYY}{MM}{DD}.pdf", 1, true),
        ];
        let pick = |file: &str| pick_rule(&rules, Path::new(file)).map(|r| r.name.as_str());

        assert_eq!(pick("/home/u/NEWS/20261022.pdf"), Some("data"));
        assert_eq!(pick("/home/u/NEWS/relatório.pdf"), Some("pdf"));
        assert_eq!(pick("/home/u/NEWS/foto.png"), Some("qualquer"));
        assert_eq!(pick("/home/u/Outros/a.pdf"), Some("outra pasta"));
        assert_eq!(pick("/home/u/Outros/a.png"), None);
        assert_eq!(pick("/home/u/NEWS/sub/20261022.pdf"), None);
        assert_eq!(pick("/home/u/NEWS/a.pdf.part"), None);
    }

    #[test]
    fn pick_rule_with_subfolders() {
        let news = PathBuf::from("/home/u/NEWS");
        let mut tree = rule("árvore", &news, "*.pdf", 1, true);
        tree.include_subfolders = true;
        let rules = [rule("topo", &news, "*.pdf", 0, true), tree];
        let pick = |file: &str| pick_rule(&rules, Path::new(file)).map(|r| r.name.as_str());

        assert_eq!(pick("/home/u/NEWS/a.pdf"), Some("topo"));
        assert_eq!(pick("/home/u/NEWS/sub/a.pdf"), Some("árvore"));
        assert_eq!(pick("/home/u/NEWS/a/b/c.pdf"), Some("árvore"));
        assert_eq!(pick("/home/u/NEWS/.cache/a.pdf"), None);
    }

    #[test]
    fn test_rule_lists_subfolders_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("NEWS");
        for (folder, name) in [
            ("", "20261022.pdf"),
            ("2025/fotos", "20251130.pdf"),
            (".oculta", "20261023.pdf"),
            ("$RECYCLE.BIN", "20261024.pdf"),
        ] {
            fs::create_dir_all(src.join(folder)).unwrap();
            fs::write(src.join(folder).join(name), "x").unwrap();
        }
        let before = snapshot(dir.path());

        let mut r = rule("Jornais", &src, "{YYYY}{MM}{DD}.pdf", 0, true);
        let labels = |r: &Rule| -> Vec<String> {
            test_rule(r, &src)
                .unwrap()
                .into_iter()
                .map(|(label, _)| label)
                .collect()
        };
        assert_eq!(labels(&r), ["20261022.pdf"]);
        r.include_subfolders = true;
        assert_eq!(labels(&r), ["2025/fotos/20251130.pdf", "20261022.pdf"]);

        let results = test_rule(&r, &src).unwrap();
        assert_eq!(
            results[0].1,
            Outcome::WouldMove {
                to: dir.path().join("Jornais/2025/11/20251130.pdf")
            }
        );
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn test_rule_reports_every_file_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("NEWS");
        fs::create_dir_all(src.join("subpasta")).unwrap();
        fs::write(src.join("20261022.pdf"), "a").unwrap();
        fs::write(src.join("foto.png"), "b").unwrap();
        fs::write(src.join("subpasta/20261023.pdf"), "c").unwrap();
        let before = snapshot(dir.path());

        let r = rule("Jornais", &src, "{YYYY}{MM}{DD}.pdf", 0, true);
        let results = test_rule(&r, &src).unwrap();

        let expected_to = dir.path().join("Jornais/2026/10/20261022.pdf");
        assert_eq!(
            results,
            [
                (
                    "20261022.pdf".to_owned(),
                    Outcome::WouldMove { to: expected_to }
                ),
                ("foto.png".to_owned(), Outcome::NotMatched),
            ]
        );
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn test_rule_on_missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("não existe");
        let r = rule("x", &missing, "*", 0, true);
        assert!(matches!(
            test_rule(&r, &missing),
            Err(MotorError::Io { .. })
        ));
    }
}
