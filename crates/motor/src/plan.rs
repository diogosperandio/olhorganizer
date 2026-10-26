//! Rule selection and the "Testar regra" dry run.

use std::path::Path;

use crate::error::MotorError;
use crate::filter::compile;
use crate::ops::{FileSystem, Outcome, StdFs, process_with};
use crate::rule::{Rule, normalize};

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
    let entries = fs.list_dir(dir).map_err(|source| MotorError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut results: Vec<(String, Outcome)> = entries
        .iter()
        .filter(|path| fs.metadata(path).is_ok_and(|m| m.is_file()))
        .map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, process_with(fs, path, &rule, true))
        })
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// The first enabled rule, lowest priority value first, whose source folder
/// directly contains `file` and whose filter matches its name.
pub fn pick_rule<'a>(rules: &'a [Rule], file: &Path) -> Option<&'a Rule> {
    let parent = normalize(file.parent()?);
    let name = file.file_name()?.to_str()?;

    let mut candidates: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    candidates.sort_by_key(|r| r.priority);
    candidates.into_iter().find(|r| {
        normalize(&r.source_dir) == parent
            && compile(&r.filter).is_ok_and(|f| f.matches(name).is_some())
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::rule::{Action, CheckMode, ConflictPolicy, DateSource};
    use crate::testutil::snapshot;

    fn rule(name: &str, source: &Path, filter: &str, priority: u32, enabled: bool) -> Rule {
        Rule {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            enabled,
            source_dir: source.to_path_buf(),
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
