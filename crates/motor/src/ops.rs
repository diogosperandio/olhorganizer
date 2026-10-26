use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use crate::{build_destination, compile, Action, ConflictPolicy, Rule};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Moved { to: PathBuf },
    Copied { to: PathBuf },
    Skipped { reason: String },
    WouldMove { to: PathBuf },
    WouldCopy { to: PathBuf },
    NotMatched,
    Failed { error: String },
}

pub trait FileSystem {
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64>;
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> io::Result<bool>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::symlink_metadata(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        fs::copy(from, to)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn exists(&self, path: &Path) -> io::Result<bool> {
        path.try_exists()
    }
}

pub fn process(file: &Path, rule: &Rule, dry_run: bool) -> Outcome {
    process_with_fs(&StdFileSystem, file, rule, dry_run)
}

pub fn process_with_fs<F: FileSystem + ?Sized>(
    fs: &F,
    file: &Path,
    rule: &Rule,
    dry_run: bool,
) -> Outcome {
    let source_metadata = match fs.symlink_metadata(file) {
        Ok(metadata) => metadata,
        Err(error) => return failed("ler os dados do arquivo de origem", error),
    };
    if source_metadata.file_type().is_symlink() {
        return Outcome::Skipped {
            reason: "Links simbólicos não são processados por segurança.".to_owned(),
        };
    }
    if !source_metadata.is_file() {
        return Outcome::Skipped {
            reason: "O item de origem não é um arquivo comum.".to_owned(),
        };
    }

    if let Err(outcome) = ensure_source_contains_file(fs, &rule.source_dir, file) {
        return outcome;
    }

    let file_name = match file.file_name().and_then(|name| name.to_str()) {
        Some(file_name) => file_name,
        None => {
            return Outcome::Failed {
                error: "O nome do arquivo não é um texto UTF-8 válido.".to_owned(),
            };
        }
    };
    let filter = match compile(&rule.filter) {
        Ok(filter) => filter,
        Err(error) => {
            return Outcome::Failed {
                error: error.to_string(),
            };
        }
    };
    let Some(captures) = filter.matches(file_name) else {
        return Outcome::NotMatched;
    };
    let destination = match build_destination(rule, &captures, &source_metadata) {
        Ok(destination) => destination,
        Err(error) => {
            return Outcome::Failed {
                error: error.to_string(),
            };
        }
    };
    let destination = match destination_for_conflict(fs, destination, rule.on_conflict) {
        Ok(Some(destination)) => destination,
        Ok(None) => {
            return Outcome::Skipped {
                reason: "Já existe um arquivo com o mesmo nome no destino.".to_owned(),
            };
        }
        Err(error) => return failed("verificar conflitos no destino", error),
    };

    if dry_run {
        return match rule.action {
            Action::Move => Outcome::WouldMove { to: destination },
            Action::Copy => Outcome::WouldCopy { to: destination },
        };
    }

    let Some(parent) = destination.parent() else {
        return Outcome::Failed {
            error: "O caminho de destino não possui uma pasta válida.".to_owned(),
        };
    };
    if let Err(error) = fs.create_dir_all(parent) {
        return failed("criar as pastas de destino", error);
    }

    match rule.action {
        Action::Copy => match fs.copy(file, &destination) {
            Ok(_) => Outcome::Copied { to: destination },
            Err(error) => failed("copiar o arquivo", error),
        },
        Action::Move => {
            if let Err(error) = prepare_move_destination(fs, &destination, rule.on_conflict) {
                return failed("preparar a substituição no destino", error);
            }
            move_file(fs, file, destination, source_metadata.len())
        }
    }
}

pub fn pick_rule(rules: &[Rule], file: impl AsRef<Path>) -> Option<&Rule> {
    let file = file.as_ref();
    let file_name = file.file_name()?.to_str()?;
    let mut candidates: Vec<_> = rules.iter().filter(|rule| rule.enabled).collect();
    candidates.sort_by_key(|rule| rule.priority);

    candidates.into_iter().find(|rule| {
        path_is_inside(file, &rule.source_dir)
            && compile(&rule.filter)
                .ok()
                .and_then(|filter| filter.matches(file_name))
                .is_some()
    })
}

fn ensure_source_contains_file<F: FileSystem + ?Sized>(
    fs: &F,
    source: &Path,
    file: &Path,
) -> Result<(), Outcome> {
    let source = fs
        .canonicalize(source)
        .map_err(|error| failed("localizar a pasta monitorada", error))?;
    let file = fs
        .canonicalize(file)
        .map_err(|error| failed("localizar o arquivo de origem", error))?;
    if path_is_inside(&file, &source) {
        Ok(())
    } else {
        Err(Outcome::Skipped {
            reason: "O arquivo não está dentro da pasta monitorada.".to_owned(),
        })
    }
}

fn destination_for_conflict<F: FileSystem + ?Sized>(
    fs: &F,
    destination: PathBuf,
    policy: ConflictPolicy,
) -> io::Result<Option<PathBuf>> {
    if !destination_exists(fs, &destination)? {
        return Ok(Some(destination));
    }

    match policy {
        ConflictPolicy::Skip => Ok(None),
        ConflictPolicy::Overwrite => Ok(Some(destination)),
        ConflictPolicy::Rename => {
            let parent = destination.parent().unwrap_or_else(|| Path::new(""));
            let stem = destination
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("arquivo");
            let extension = destination.extension().and_then(|value| value.to_str());
            let mut number = 2_u32;
            loop {
                let file_name = match extension {
                    Some(extension) if !extension.is_empty() => {
                        format!("{stem} ({number}).{extension}")
                    }
                    _ => format!("{stem} ({number})"),
                };
                let candidate = parent.join(file_name);
                if !destination_exists(fs, &candidate)? {
                    return Ok(Some(candidate));
                }
                number = number.checked_add(1).ok_or_else(|| {
                    io::Error::other("não há numeração disponível para o arquivo")
                })?;
            }
        }
    }
}

fn destination_exists<F: FileSystem + ?Sized>(fs: &F, path: &Path) -> io::Result<bool> {
    #[cfg(windows)]
    {
        let Some(parent) = path.parent() else {
            return fs.exists(path);
        };
        let Some(wanted_name) = path.file_name().and_then(|name| name.to_str()) else {
            return fs.exists(path);
        };
        if !fs.exists(parent)? {
            return Ok(false);
        }
        return Ok(fs.read_dir(parent)?.iter().any(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted_name))
        }));
    }

    #[cfg(not(windows))]
    fs.exists(path)
}

fn move_file<F: FileSystem + ?Sized>(
    fs: &F,
    source: &Path,
    destination: PathBuf,
    source_size: u64,
) -> Outcome {
    match fs.rename(source, &destination) {
        Ok(()) => Outcome::Moved { to: destination },
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            move_across_devices(fs, source, destination, source_size)
        }
        Err(error) => failed("mover o arquivo", error),
    }
}

fn prepare_move_destination<F: FileSystem + ?Sized>(
    fs: &F,
    destination: &Path,
    policy: ConflictPolicy,
) -> io::Result<()> {
    #[cfg(windows)]
    if policy == ConflictPolicy::Overwrite && destination_exists(fs, destination)? {
        fs.remove_file(destination)?;
    }

    #[cfg(not(windows))]
    let _ = (fs, destination, policy);

    Ok(())
}

fn move_across_devices<F: FileSystem + ?Sized>(
    fs: &F,
    source: &Path,
    destination: PathBuf,
    source_size: u64,
) -> Outcome {
    if let Err(error) = fs.copy(source, &destination) {
        return failed("copiar o arquivo entre discos", error);
    }
    if let Err(error) = fs.sync_file(&destination) {
        return failed("sincronizar a cópia no destino", error);
    }
    let destination_size = match fs.metadata(&destination) {
        Ok(metadata) => metadata.len(),
        Err(error) => return failed("verificar a cópia no destino", error),
    };
    if destination_size != source_size {
        return Outcome::Failed {
            error: format!(
                "A cópia no destino tem tamanho diferente da origem ({destination_size} de {source_size} bytes)."
            ),
        };
    }
    if let Err(error) = fs.remove_file(source) {
        return failed("remover a origem após verificar a cópia", error);
    }
    Outcome::Moved { to: destination }
}

fn path_is_inside(path: &Path, directory: &Path) -> bool {
    #[cfg(windows)]
    {
        let mut path_components = path.components();
        return directory.components().all(|directory_component| {
            path_components.next().is_some_and(|path_component| {
                path_component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&directory_component.as_os_str().to_string_lossy())
            })
        });
    }

    #[cfg(not(windows))]
    path.starts_with(directory)
}

fn failed(operation: &str, error: io::Error) -> Outcome {
    Outcome::Failed {
        error: format!("Não foi possível {operation}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::{CheckMode, DateSource};

    struct CrossDeviceFileSystem {
        fail_sync: bool,
    }

    impl FileSystem for CrossDeviceFileSystem {
        fn metadata(&self, path: &Path) -> io::Result<Metadata> {
            StdFileSystem.metadata(path)
        }

        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            StdFileSystem.symlink_metadata(path)
        }

        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            StdFileSystem.canonicalize(path)
        }

        fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
            StdFileSystem.read_dir(path)
        }

        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            StdFileSystem.create_dir_all(path)
        }

        fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::CrossesDevices))
        }

        fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
            StdFileSystem.copy(from, to)
        }

        fn sync_file(&self, path: &Path) -> io::Result<()> {
            if self.fail_sync {
                Err(io::Error::other("falha de sincronização simulada"))
            } else {
                StdFileSystem.sync_file(path)
            }
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            StdFileSystem.remove_file(path)
        }

        fn exists(&self, path: &Path) -> io::Result<bool> {
            StdFileSystem.exists(path)
        }
    }

    fn rule(source: &Path, destination: &Path, action: Action) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: "Teste".to_owned(),
            enabled: true,
            source_dir: source.to_path_buf(),
            filter: "{YYYY}{MM}{DD}.pdf".to_owned(),
            destination: destination
                .join("{YYYY}/{MM}")
                .to_string_lossy()
                .into_owned(),
            action,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        }
    }

    #[test]
    fn moves_file_into_date_folders() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir(&source).expect("source should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"jornal").expect("fixture should be written");

        let outcome = process(&file, &rule(&source, &output, Action::Move), false);
        let destination = output.join("2026/10/20261022.pdf");

        assert_eq!(
            outcome,
            Outcome::Moved {
                to: destination.clone()
            }
        );
        assert!(!file.exists());
        assert_eq!(
            fs::read(destination).expect("destination should exist"),
            b"jornal"
        );
    }

    #[test]
    fn copies_without_touching_source() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir(&source).expect("source should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"imagem").expect("fixture should be written");

        let outcome = process(&file, &rule(&source, &output, Action::Copy), false);

        assert!(matches!(outcome, Outcome::Copied { .. }));
        assert_eq!(fs::read(&file).expect("source should remain"), b"imagem");
        assert_eq!(
            fs::read(output.join("2026/10/20261022.pdf")).expect("destination should exist"),
            b"imagem"
        );
    }

    #[test]
    fn applies_rename_conflict_numbering_through_three() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida/2026/10");
        fs::create_dir_all(&source).expect("source should be created");
        fs::create_dir_all(&output).expect("output should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"novo").expect("fixture should be written");
        fs::write(output.join("20261022.pdf"), b"um").expect("conflict should be written");
        fs::write(output.join("20261022 (2).pdf"), b"dois")
            .expect("second conflict should be written");

        let outcome = process(
            &file,
            &rule(&source, &directory.path().join("saida"), Action::Move),
            false,
        );
        let expected = output.join("20261022 (3).pdf");

        assert_eq!(
            outcome,
            Outcome::Moved {
                to: expected.clone()
            }
        );
        assert_eq!(
            fs::read(expected).expect("renamed file should exist"),
            b"novo"
        );
    }

    #[test]
    fn overwrites_when_policy_allows_it() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output_root = directory.path().join("saida");
        let output = output_root.join("2026/10");
        fs::create_dir_all(&source).expect("source should be created");
        fs::create_dir_all(&output).expect("output should be created");
        let file = source.join("20261022.pdf");
        let destination = output.join("20261022.pdf");
        fs::write(&file, b"novo").expect("fixture should be written");
        fs::write(&destination, b"antigo").expect("conflict should be written");
        let mut rule = rule(&source, &output_root, Action::Copy);
        rule.on_conflict = ConflictPolicy::Overwrite;

        let outcome = process(&file, &rule, false);

        assert_eq!(
            outcome,
            Outcome::Copied {
                to: destination.clone()
            }
        );
        assert_eq!(
            fs::read(destination).expect("destination should exist"),
            b"novo"
        );
        assert_eq!(fs::read(file).expect("source should remain"), b"novo");
    }

    #[test]
    fn skips_existing_destination_when_policy_requires_it() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output_root = directory.path().join("saida");
        let output = output_root.join("2026/10");
        fs::create_dir_all(&source).expect("source should be created");
        fs::create_dir_all(&output).expect("output should be created");
        let file = source.join("20261022.pdf");
        let destination = output.join("20261022.pdf");
        fs::write(&file, b"novo").expect("fixture should be written");
        fs::write(&destination, b"antigo").expect("conflict should be written");
        let mut rule = rule(&source, &output_root, Action::Move);
        rule.on_conflict = ConflictPolicy::Skip;

        let outcome = process(&file, &rule, false);

        assert!(matches!(outcome, Outcome::Skipped { .. }));
        assert_eq!(fs::read(file).expect("source should remain"), b"novo");
        assert_eq!(
            fs::read(destination).expect("destination should remain"),
            b"antigo"
        );
    }

    #[test]
    fn picks_first_enabled_matching_rule_by_priority() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        fs::create_dir(&source).expect("source should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"pdf").expect("fixture should be written");
        let output = directory.path().join("saida");
        let mut later = rule(&source, &output, Action::Copy);
        later.name = "Posterior".to_owned();
        later.priority = 20;
        let mut disabled = rule(&source, &output, Action::Copy);
        disabled.name = "Desativada".to_owned();
        disabled.priority = 1;
        disabled.enabled = false;
        let mut first = rule(&source, &output, Action::Copy);
        first.name = "Primeira".to_owned();
        first.priority = 10;
        let rules = [later, disabled, first];

        let selected = pick_rule(&rules, &file).expect("a rule should match");

        assert_eq!(selected.name, "Primeira");
    }

    #[test]
    fn cross_device_move_copies_syncs_verifies_and_removes_source() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir(&source).expect("source should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"conteudo verificado").expect("fixture should be written");
        let filesystem = CrossDeviceFileSystem { fail_sync: false };

        let outcome = process_with_fs(
            &filesystem,
            &file,
            &rule(&source, &output, Action::Move),
            false,
        );
        let destination = output.join("2026/10/20261022.pdf");

        assert_eq!(
            outcome,
            Outcome::Moved {
                to: destination.clone()
            }
        );
        assert!(!file.exists());
        assert_eq!(
            fs::read(destination).expect("destination should exist"),
            b"conteudo verificado"
        );
    }

    #[test]
    fn cross_device_failure_keeps_source() {
        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir(&source).expect("source should be created");
        let file = source.join("20261022.pdf");
        fs::write(&file, b"origem preservada").expect("fixture should be written");
        let filesystem = CrossDeviceFileSystem { fail_sync: true };

        let outcome = process_with_fs(
            &filesystem,
            &file,
            &rule(&source, &output, Action::Move),
            false,
        );

        assert!(matches!(outcome, Outcome::Failed { .. }));
        assert_eq!(
            fs::read(file).expect("source should remain"),
            b"origem preservada"
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().expect("temp directory should be created");
        let source = directory.path().join("entrada");
        let output = directory.path().join("saida");
        fs::create_dir(&source).expect("source should be created");
        let outside = directory.path().join("20261022.pdf");
        let link = source.join("20261022.pdf");
        fs::write(&outside, b"fora").expect("outside file should be written");
        symlink(&outside, &link).expect("symlink should be created");

        let outcome = process(&link, &rule(&source, &output, Action::Move), false);

        assert!(matches!(outcome, Outcome::Skipped { .. }));
        assert_eq!(
            fs::read(outside).expect("outside file should remain"),
            b"fora"
        );
        assert!(!output.exists());
    }
}
