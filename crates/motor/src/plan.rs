use std::path::Path;

use crate::{process_with_fs, FileSystem, Outcome, Rule, StdFileSystem};

pub fn test_rule(rule: &Rule, directory: &Path) -> Vec<(String, Outcome)> {
    test_rule_with_fs(&StdFileSystem, rule, directory)
}

pub fn test_rule_with_fs<F: FileSystem + ?Sized>(
    fs: &F,
    rule: &Rule,
    directory: &Path,
) -> Vec<(String, Outcome)> {
    let mut files = match collect_files(fs, directory, rule.include_subfolders) {
        Ok(files) => files,
        Err(error) => {
            return vec![(
                directory.to_string_lossy().into_owned(),
                Outcome::Failed {
                    error: format!("Não foi possível listar a pasta para o teste: {error}"),
                },
            )];
        }
    };
    files.sort();

    files
        .into_iter()
        .filter(|path| {
            fs.symlink_metadata(path)
                .map(|metadata| metadata.is_file() || metadata.file_type().is_symlink())
                .unwrap_or(true)
        })
        .map(|path| {
            let file_name = path
                .strip_prefix(directory)
                .ok()
                .filter(|relative| !relative.as_os_str().is_empty())
                .map(|relative| relative.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let outcome = process_with_fs(fs, &path, rule, true);
            (file_name, outcome)
        })
        .collect()
}

fn collect_files<F: FileSystem + ?Sized>(
    fs: &F,
    directory: &Path,
    recursive: bool,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_directory(fs, directory, recursive, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_directory<F: FileSystem + ?Sized>(
    fs: &F,
    directory: &Path,
    recursive: bool,
    files: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    let mut entries = fs.read_dir(directory)?;
    entries.sort();
    for path in entries {
        let metadata = fs.symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            files.push(path);
        } else if recursive && metadata.is_dir() && !is_ignored_directory(&path) {
            collect_directory(fs, &path, true, files)?;
        }
    }
    Ok(())
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with('.')
                || name.eq_ignore_ascii_case("$RECYCLE.BIN")
                || name.eq_ignore_ascii_case("System Volume Information")
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::{Action, CheckMode, ConflictPolicy, DateSource};

    fn snapshot(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn visit(root: &Path, directory: &Path, entries: &mut Vec<(PathBuf, Vec<u8>)>) {
            let mut paths: Vec<_> = fs::read_dir(directory)
                .expect("directory should be readable")
                .map(|entry| entry.expect("entry should be readable").path())
                .collect();
            paths.sort();
            for path in paths {
                let relative = path
                    .strip_prefix(root)
                    .expect("path should be under root")
                    .to_path_buf();
                if path.is_dir() {
                    entries.push((relative.clone(), Vec::new()));
                    visit(root, &path, entries);
                } else {
                    entries.push((relative, fs::read(path).expect("file should be readable")));
                }
            }
        }

        let mut entries = Vec::new();
        visit(directory, directory, &mut entries);
        entries
    }

    #[test]
    fn dry_run_lists_top_level_files_without_changing_a_byte() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        let nested = source.join("subpasta");
        fs::create_dir_all(&nested).expect("source should be created");
        fs::write(source.join("20261022.pdf"), b"um").expect("fixture should be written");
        fs::write(source.join("ignorar.txt"), b"dois").expect("fixture should be written");
        fs::write(nested.join("20261023.pdf"), b"tres").expect("fixture should be written");
        let rule = Rule {
            id: Uuid::new_v4(),
            name: "Teste".to_owned(),
            enabled: true,
            source_dir: source.clone(),
            filter: "{YYYY}{MM}{DD}.pdf".to_owned(),
            destination: output.join("{YYYY}/{MM}").to_string_lossy().into_owned(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        };
        let before = snapshot(directory.path());

        let outcomes = test_rule(&rule, &source);
        let after = snapshot(directory.path());

        assert_eq!(before, after);
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].0, "20261022.pdf");
        assert!(matches!(outcomes[0].1, Outcome::WouldMove { .. }));
        assert_eq!(outcomes[1], ("ignorar.txt".to_owned(), Outcome::NotMatched));
        assert!(!output.exists());
    }

    #[test]
    fn recursive_dry_run_lists_relative_paths_and_skips_hidden_folders() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir_all(source.join("2025/fotos")).expect("nested source should exist");
        fs::create_dir_all(source.join(".oculta")).expect("hidden folder should exist");
        fs::write(source.join("2025/fotos/20261022.pdf"), b"foto")
            .expect("fixture should be written");
        fs::write(source.join(".oculta/20261023.pdf"), b"oculto")
            .expect("fixture should be written");
        let mut configured_rule = Rule {
            id: Uuid::new_v4(),
            name: "Teste".to_owned(),
            enabled: true,
            source_dir: source.clone(),
            filter: "{YYYY}{MM}{DD}.pdf".to_owned(),
            destination: output.join("{YYYY}/{MM}").to_string_lossy().into_owned(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        };
        configured_rule.include_subfolders = true;
        let before = snapshot(directory.path());

        let outcomes = test_rule(&configured_rule, &source);

        assert_eq!(snapshot(directory.path()), before);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0].0,
            Path::new("2025/fotos/20261022.pdf")
                .to_string_lossy()
                .into_owned()
        );
        assert!(matches!(outcomes[0].1, Outcome::WouldMove { .. }));
    }
}
