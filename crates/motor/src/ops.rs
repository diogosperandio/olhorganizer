//! File operations: moves or copies one file according to a rule.

use std::fmt::Display;
use std::fs::{self, Metadata, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::destination::{FileMeta, build_destination};
use crate::filter::compile;
use crate::rule::{Action, ConflictPolicy, Rule, resolve_with};

/// Most "name (n).ext" candidates tried before giving up.
const MAX_RENAME: u32 = 9999;

/// Suffix of the temporary file a copy is written to before being renamed into
/// place. It ends in `.tmp`, so watchers ignore it.
const COPY_SUFFIX: &str = ".olhorganizer.tmp";

/// Result of processing one file. Messages are in pt-BR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Moved { to: PathBuf },
    Copied { to: PathBuf },
    Skipped { reason: String },
    WouldMove { to: PathBuf },
    WouldCopy { to: PathBuf },
    NotMatched,
    Failed { error: String },
}

/// Every file system call the engine makes, so tests can inject failures.
pub trait FileSystem {
    /// Metadata, following symlinks.
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// Metadata of the link itself for symlinks.
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// True if anything (even a broken symlink) exists at `path`.
    fn exists(&self, path: &Path) -> bool;
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Copies `from` to `to` and flushes `to` to disk.
    fn copy_synced(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFs;

impl FileSystem for StdFs {
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::symlink_metadata(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn exists(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|e| e.path()))
            .collect()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn copy_synced(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::copy(from, to)?;
        OpenOptions::new().write(true).open(to)?.sync_all()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// Processes `file` with `rule` on the real file system. With `dry_run`,
/// nothing is created, moved, copied or deleted.
pub fn process(file: &Path, rule: &Rule, dry_run: bool) -> Outcome {
    process_with(&StdFs, file, rule, dry_run)
}

pub fn process_with(fs: &impl FileSystem, file: &Path, rule: &Rule, dry_run: bool) -> Outcome {
    run(fs, file, rule, dry_run).unwrap_or_else(|outcome| outcome)
}

/// `Err` carries an early outcome (no match, skipped, failed).
fn run(fs: &impl FileSystem, file: &Path, rule: &Rule, dry_run: bool) -> Result<Outcome, Outcome> {
    let filter = compile(&rule.filter).map_err(|e| Outcome::Failed {
        error: e.to_string(),
    })?;
    let Some(name) = file.file_name() else {
        return Ok(Outcome::NotMatched);
    };
    let name = name
        .to_str()
        .ok_or_else(|| skipped("O nome do arquivo tem caracteres que não puderam ser lidos."))?;
    let Some(captures) = filter.matches(name) else {
        return Ok(Outcome::NotMatched);
    };

    // Canonical paths resolve symlinks, so a link pointing outside the source
    // folder is caught here and never followed.
    let real_file = fs
        .canonicalize(file)
        .map_err(|e| failed("Não foi possível ler o arquivo", e))?;
    let real_source = fs
        .canonicalize(&rule.source_dir)
        .map_err(|e| failed("Não foi possível ler a pasta monitorada", e))?;
    if !real_file.starts_with(&real_source) {
        return Err(skipped(
            "O arquivo aponta para fora da pasta monitorada e foi ignorado.",
        ));
    }

    let meta = fs
        .metadata(file)
        .map_err(|e| failed("Não foi possível ler o arquivo", e))?;
    if !meta.is_file() {
        return Err(skipped("Não é um arquivo."));
    }

    let file_meta = FileMeta {
        modified: meta.modified().ok(),
        detected: SystemTime::now(),
    };
    let dir = build_destination(rule, &captures, &file_meta).map_err(|e| Outcome::Failed {
        error: e.to_string(),
    })?;
    guard_against_loops(fs, file, &dir, rule)?;
    let target = resolve_target(fs, &dir, &captures.name, &captures.ext, rule.on_conflict)?;

    if dry_run {
        return Ok(match rule.action {
            Action::Move => Outcome::WouldMove { to: target },
            Action::Copy => Outcome::WouldCopy { to: target },
        });
    }

    fs.create_dir_all(&dir)
        .map_err(|e| failed("Não foi possível criar a pasta de destino", e))?;

    match rule.action {
        Action::Copy => {
            copy_into(fs, file, &target, meta.len())?;
            Ok(Outcome::Copied { to: target })
        }
        Action::Move => match fs.rename(file, &target) {
            Ok(()) => Ok(Outcome::Moved { to: target }),
            Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
                // Different disk: copy, verify, and only then remove the source.
                copy_into(fs, file, &target, meta.len())?;
                fs.remove_file(file).map_err(|e| {
                    failed(
                        format!(
                            "O arquivo foi copiado para {}, mas não foi possível removê-lo da pasta de origem",
                            target.display()
                        ),
                        e,
                    )
                })?;
                Ok(Outcome::Moved { to: target })
            }
            Err(e) => Err(failed("Não foi possível mover o arquivo", e)),
        },
    }
}

/// Defense in depth against processing the same file forever, in case
/// validation was bypassed or folders changed after it ran: a file already in
/// its destination folder, or below a destination that lies in the watched
/// folder, is left alone. Must run before the conflict policy, which would
/// otherwise rename the file next to itself.
fn guard_against_loops(
    fs: &impl FileSystem,
    file: &Path,
    dir: &Path,
    rule: &Rule,
) -> Result<(), Outcome> {
    let resolve = |path: &Path| resolve_with(path, |p| fs.canonicalize(p));
    let here = resolve(file.parent().unwrap_or(Path::new("")));
    let there = resolve(dir);
    if here == there {
        return Err(skipped("O arquivo já está na pasta de destino."));
    }
    // Only when the destination is inside the source: a destination above the
    // source ("move up one folder") also contains the file, legitimately.
    if here.starts_with(&there) && there.starts_with(&resolve(&rule.source_dir)) {
        return Err(skipped(
            "O arquivo já está dentro da pasta de destino e não será processado de novo.",
        ));
    }
    Ok(())
}

/// Picks the final path in `dir` according to the conflict policy.
fn resolve_target(
    fs: &impl FileSystem,
    dir: &Path,
    stem: &str,
    ext: &str,
    policy: ConflictPolicy,
) -> Result<PathBuf, Outcome> {
    let file_name = |suffix: &str| {
        if ext.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{ext}")
        }
    };

    let name = file_name("");
    if !name_taken(fs, dir, &name) {
        return Ok(dir.join(name));
    }
    match policy {
        ConflictPolicy::Overwrite => Ok(dir.join(name)),
        ConflictPolicy::Skip => Err(skipped("Já existe um arquivo com o mesmo nome no destino.")),
        ConflictPolicy::Rename => (2..=MAX_RENAME)
            .map(|n| file_name(&format!(" ({n})")))
            .find(|candidate| !name_taken(fs, dir, candidate))
            .map(|candidate| dir.join(candidate))
            .ok_or_else(|| Outcome::Failed {
                error: "Há arquivos demais com o mesmo nome no destino.".into(),
            }),
    }
}

/// Windows file names are case-insensitive; the listing also catches folders
/// where the file system itself was made case-sensitive.
fn name_taken(fs: &impl FileSystem, dir: &Path, name: &str) -> bool {
    if fs.exists(&dir.join(name)) {
        return true;
    }
    if !cfg!(windows) {
        return false;
    }
    let name = name.to_lowercase();
    fs.list_dir(dir).is_ok_and(|entries| {
        entries.iter().any(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == name)
        })
    })
}

/// Copies to a temporary name next to `to`, checks the size, then renames it
/// into place, so a half-copied file never appears under the final name.
fn copy_into(
    fs: &impl FileSystem,
    from: &Path,
    to: &Path,
    expected_len: u64,
) -> Result<(), Outcome> {
    let mut tmp_name = to.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(COPY_SUFFIX);
    let tmp = to.with_file_name(tmp_name);

    let result = fs
        .copy_synced(from, &tmp)
        .and_then(|()| {
            let len = fs.metadata(&tmp)?.len();
            if len == expected_len {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "a cópia tem {len} bytes, mas o original tem {expected_len}"
                )))
            }
        })
        .and_then(|()| fs.rename(&tmp, to));
    if let Err(e) = result {
        let _ = fs.remove_file(&tmp);
        return Err(failed("Não foi possível copiar o arquivo", e));
    }
    Ok(())
}

fn skipped(reason: &str) -> Outcome {
    Outcome::Skipped {
        reason: reason.into(),
    }
}

fn failed(context: impl Display, error: io::Error) -> Outcome {
    Outcome::Failed {
        error: format!("{context}: {error}."),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::rule::{CheckMode, DateSource};
    use crate::testutil::snapshot;

    struct Env {
        _dir: tempfile::TempDir,
        src: PathBuf,
        dst: PathBuf,
        root: PathBuf,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let src = root.join("NEWS");
        let dst = root.join("Jornais");
        fs::create_dir_all(&src).unwrap();
        Env {
            _dir: dir,
            src,
            dst,
            root,
        }
    }

    fn rule(env: &Env, action: Action, on_conflict: ConflictPolicy) -> Rule {
        Rule {
            id: uuid::Uuid::new_v4(),
            name: "Jornais".into(),
            enabled: true,
            source_dir: env.src.clone(),
            include_subfolders: false,
            filter: "{YYYY}{MM}{DD}.pdf".into(),
            destination: format!("{}/{{YYYY}}/{{MM}}", env.dst.display()),
            action,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            on_conflict,
            priority: 0,
        }
    }

    fn put(dir: &Path, name: &str, content: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn moves_into_year_month_folders() {
        let env = env();
        let file = put(&env.src, "20261022.pdf", "jornal");
        let outcome = process(
            &file,
            &rule(&env, Action::Move, ConflictPolicy::Rename),
            false,
        );

        let to = env.dst.join("2026").join("10").join("20261022.pdf");
        assert_eq!(outcome, Outcome::Moved { to: to.clone() });
        assert_eq!(read(&to), "jornal");
        assert!(!file.exists());
    }

    #[test]
    fn moves_into_a_subfolder_of_the_source() {
        let env = env();
        let file = put(&env.src, "20261022.pdf", "jornal");
        let mut r = rule(&env, Action::Move, ConflictPolicy::Rename);
        r.destination = format!("{}/{{YYYY}}/{{MM}}", env.src.display());
        let to = env.src.join("2026/10/20261022.pdf");
        assert_eq!(process(&file, &r, false), Outcome::Moved { to: to.clone() });

        // Processing the moved file again changes nothing, and no "(2)" copy appears.
        let before = snapshot(&env.root);
        let outcome = process(&to, &r, false);
        assert!(
            matches!(&outcome, Outcome::Skipped { reason } if reason.contains("já está na pasta de destino")),
            "{outcome:?}"
        );
        assert_eq!(snapshot(&env.root), before);
    }

    #[test]
    fn file_below_a_destination_inside_the_source_is_skipped() {
        let env = env();
        let file = put(&env.src.join("2026/outra"), "20261022.pdf", "jornal");
        let mut r = rule(&env, Action::Move, ConflictPolicy::Rename);
        r.destination = format!("{}/{{YYYY}}", env.src.display());
        let before = snapshot(&env.root);
        let outcome = process(&file, &r, false);
        assert!(
            matches!(&outcome, Outcome::Skipped { reason } if reason.contains("dentro da pasta de destino")),
            "{outcome:?}"
        );
        assert_eq!(snapshot(&env.root), before);
    }

    #[test]
    fn destination_above_the_source_still_works() {
        let env = env();
        let file = put(&env.src, "20261022.pdf", "jornal");
        let mut r = rule(&env, Action::Move, ConflictPolicy::Rename);
        r.destination = env.root.display().to_string();
        assert_eq!(
            process(&file, &r, false),
            Outcome::Moved {
                to: env.root.join("20261022.pdf")
            }
        );
    }

    #[test]
    fn copy_keeps_the_source() {
        let env = env();
        let file = put(&env.src, "20261022.pdf", "jornal");
        let outcome = process(
            &file,
            &rule(&env, Action::Copy, ConflictPolicy::Rename),
            false,
        );

        let to = env.dst.join("2026/10/20261022.pdf");
        assert_eq!(outcome, Outcome::Copied { to: to.clone() });
        assert_eq!(read(&to), "jornal");
        assert_eq!(read(&file), "jornal");
        assert!(
            !env.dst
                .join("2026/10/20261022.pdf.olhorganizer.tmp")
                .exists()
        );
    }

    #[test]
    fn not_matching_file_is_untouched() {
        let env = env();
        let file = put(&env.src, "foto.png", "x");
        let outcome = process(
            &file,
            &rule(&env, Action::Move, ConflictPolicy::Rename),
            false,
        );
        assert_eq!(outcome, Outcome::NotMatched);
        assert!(file.exists());
    }

    #[test]
    fn conflict_skip_leaves_both_files() {
        let env = env();
        let existing = put(&env.dst.join("2026/10"), "20261022.pdf", "antigo");
        let file = put(&env.src, "20261022.pdf", "novo");
        let outcome = process(
            &file,
            &rule(&env, Action::Move, ConflictPolicy::Skip),
            false,
        );

        assert!(matches!(outcome, Outcome::Skipped { .. }), "{outcome:?}");
        assert_eq!(read(&existing), "antigo");
        assert_eq!(read(&file), "novo");
    }

    #[test]
    fn conflict_overwrite_replaces_the_file() {
        let env = env();
        let existing = put(&env.dst.join("2026/10"), "20261022.pdf", "antigo");
        let file = put(&env.src, "20261022.pdf", "novo");
        let outcome = process(
            &file,
            &rule(&env, Action::Move, ConflictPolicy::Overwrite),
            false,
        );

        assert_eq!(
            outcome,
            Outcome::Moved {
                to: existing.clone()
            }
        );
        assert_eq!(read(&existing), "novo");
        assert!(!file.exists());
    }

    #[test]
    fn conflict_rename_numbers_up_to_three() {
        let env = env();
        let r = rule(&env, Action::Move, ConflictPolicy::Rename);
        let dir = env.dst.join("2026/10");
        for (i, expected) in ["20261022.pdf", "20261022 (2).pdf", "20261022 (3).pdf"]
            .into_iter()
            .enumerate()
        {
            let file = put(&env.src, "20261022.pdf", &format!("versão {i}"));
            assert_eq!(
                process(&file, &r, false),
                Outcome::Moved {
                    to: dir.join(expected)
                }
            );
        }
        assert_eq!(read(&dir.join("20261022.pdf")), "versão 0");
        assert_eq!(read(&dir.join("20261022 (2).pdf")), "versão 1");
        assert_eq!(read(&dir.join("20261022 (3).pdf")), "versão 2");
    }

    #[test]
    fn dry_run_changes_nothing() {
        let env = env();
        put(&env.dst.join("2026/10"), "20261022.pdf", "antigo");
        let file = put(&env.src, "20261022.pdf", "novo");
        put(&env.src, "20261023.pdf", "outro");
        let before = snapshot(&env.root);

        let dir = env.dst.join("2026/10");
        let cases = [
            (
                Action::Move,
                ConflictPolicy::Rename,
                Outcome::WouldMove {
                    to: dir.join("20261022 (2).pdf"),
                },
            ),
            (
                Action::Copy,
                ConflictPolicy::Rename,
                Outcome::WouldCopy {
                    to: dir.join("20261022 (2).pdf"),
                },
            ),
            (
                Action::Move,
                ConflictPolicy::Overwrite,
                Outcome::WouldMove {
                    to: dir.join("20261022.pdf"),
                },
            ),
        ];
        for (action, policy, expected) in cases {
            assert_eq!(process(&file, &rule(&env, action, policy), true), expected);
        }
        // A destination folder that does not exist yet is not created.
        let other = env.src.join("20261023.pdf");
        assert!(matches!(
            process(
                &other,
                &rule(&env, Action::Move, ConflictPolicy::Rename),
                true
            ),
            Outcome::WouldMove { .. }
        ));
        assert!(matches!(
            process(&file, &rule(&env, Action::Move, ConflictPolicy::Skip), true),
            Outcome::Skipped { .. }
        ));

        assert_eq!(snapshot(&env.root), before);
    }

    #[test]
    fn missing_date_fails_without_touching_the_file() {
        let env = env();
        let file = put(&env.src, "relatório.pdf", "x");
        let mut r = rule(&env, Action::Move, ConflictPolicy::Rename);
        r.filter = "*.pdf".into();
        let outcome = process(&file, &r, false);
        assert!(
            matches!(&outcome, Outcome::Failed { error } if error.contains("não tem data")),
            "{outcome:?}"
        );
        assert!(file.exists());
    }

    #[test]
    fn invalid_characters_in_names_become_underscores() {
        let env = env();
        let file = put(&env.src, "a:b?.pdf", "x");
        let mut r = rule(&env, Action::Move, ConflictPolicy::Rename);
        r.filter = "*.pdf".into();
        r.date_source = DateSource::Detected;
        r.destination = format!("{}/{{NOME}}", env.dst.display());
        let outcome = process(&file, &r, false);
        assert_eq!(
            outcome,
            Outcome::Moved {
                to: env.dst.join("a_b_").join("a:b?.pdf")
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_the_source_is_skipped() {
        let env = env();
        let outside = put(&env.root.join("fora"), "20261022.pdf", "segredo");
        let link = env.src.join("20261022.pdf");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let before = snapshot(&env.root);

        let outcome = process(
            &link,
            &rule(&env, Action::Move, ConflictPolicy::Rename),
            false,
        );
        assert!(matches!(outcome, Outcome::Skipped { .. }), "{outcome:?}");
        assert_eq!(snapshot(&env.root), before);
    }

    /// Real file system with injected failures.
    #[derive(Default)]
    struct Faulty {
        /// `rename` from this path fails as if crossing disks.
        cross_device_from: Option<PathBuf>,
        /// `copy_synced` writes only half of the file.
        short_copy: bool,
        /// `copy_synced` fails.
        copy_fails: bool,
        copies: Cell<u32>,
    }

    impl FileSystem for Faulty {
        fn metadata(&self, path: &Path) -> io::Result<Metadata> {
            StdFs.metadata(path)
        }
        fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
            StdFs.symlink_metadata(path)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            StdFs.canonicalize(path)
        }
        fn exists(&self, path: &Path) -> bool {
            StdFs.exists(path)
        }
        fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
            StdFs.list_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            StdFs.create_dir_all(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.cross_device_from.as_deref() == Some(from) {
                return Err(io::ErrorKind::CrossesDevices.into());
            }
            StdFs.rename(from, to)
        }
        fn copy_synced(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.copies.set(self.copies.get() + 1);
            if self.copy_fails {
                fs::write(to, "pedaço")?;
                return Err(io::Error::other("disco cheio"));
            }
            if self.short_copy {
                let bytes = fs::read(from)?;
                return fs::write(to, &bytes[..bytes.len() / 2]);
            }
            StdFs.copy_synced(from, to)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            StdFs.remove_file(path)
        }
    }

    #[test]
    fn cross_device_move_copies_then_removes_source() {
        let env = env();
        let file = put(&env.src, "20261022.pdf", "jornal");
        let fs = Faulty {
            cross_device_from: Some(file.clone()),
            ..Faulty::default()
        };
        let outcome = process_with(
            &fs,
            &file,
            &rule(&env, Action::Move, ConflictPolicy::Rename),
            false,
        );

        let to = env.dst.join("2026/10/20261022.pdf");
        assert_eq!(outcome, Outcome::Moved { to: to.clone() });
        assert_eq!(fs.copies.get(), 1);
        assert_eq!(read(&to), "jornal");
        assert!(!file.exists());
    }

    #[test]
    fn failed_cross_device_copy_keeps_the_source() {
        for fs in [
            Faulty {
                copy_fails: true,
                ..Faulty::default()
            },
            Faulty {
                short_copy: true,
                ..Faulty::default()
            },
        ] {
            let env = env();
            let file = put(&env.src, "20261022.pdf", "jornal completo");
            let fs = Faulty {
                cross_device_from: Some(file.clone()),
                ..fs
            };
            let outcome = process_with(
                &fs,
                &file,
                &rule(&env, Action::Move, ConflictPolicy::Rename),
                false,
            );

            assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
            assert_eq!(read(&file), "jornal completo");
            let dir = env.dst.join("2026/10");
            assert_eq!(StdFs.list_dir(&dir).unwrap(), Vec::<PathBuf>::new());
        }
    }
}
