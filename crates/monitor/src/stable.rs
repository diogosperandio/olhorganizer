use std::fmt;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy)]
pub(crate) struct StableOptions {
    pub stable_for: Duration,
    pub attempts: usize,
}

#[derive(Debug)]
pub(crate) struct StableError {
    message: String,
}

impl fmt::Display for StableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StableError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    size: u64,
    modified: SystemTime,
}

pub(crate) async fn wait_until_stable(
    path: &Path,
    options: StableOptions,
) -> Result<(), StableError> {
    let attempts = options.attempts.max(1);
    let mut last_problem = String::from("o arquivo continuou mudando");

    for _ in 0..attempts {
        let before = match snapshot(path) {
            Ok(snapshot) => snapshot,
            Err(message) => {
                last_problem = message;
                tokio::time::sleep(options.stable_for).await;
                continue;
            }
        };

        tokio::time::sleep(options.stable_for).await;

        let after = match snapshot(path) {
            Ok(snapshot) => snapshot,
            Err(message) => {
                last_problem = message;
                continue;
            }
        };

        if before != after {
            last_problem = "o tamanho ou a data de modificação ainda está mudando".to_owned();
            continue;
        }

        if can_open_for_reading(path) {
            return Ok(());
        }
        last_problem = "o arquivo ainda está bloqueado para leitura".to_owned();
    }

    Err(StableError {
        message: format!(
            "O arquivo '{}' não ficou estável após {attempts} tentativas: {last_problem}.",
            path.display()
        ),
    })
}

fn snapshot(path: &Path) -> Result<Snapshot, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("não foi possível consultar o arquivo: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("links simbólicos não são processados".to_owned());
    }
    if !metadata.is_file() {
        return Err("o item não é um arquivo comum".to_owned());
    }
    let modified = metadata
        .modified()
        .map_err(|error| format!("não foi possível ler a data de modificação: {error}"))?;
    Ok(Snapshot {
        size: metadata.len(),
        modified,
    })
}

#[cfg(windows)]
fn can_open_for_reading(path: &Path) -> bool {
    fs::File::open(path).is_ok()
}

#[cfg(not(windows))]
fn can_open_for_reading(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn waits_until_size_stops_changing() {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("arquivo.bin");
        fs::write(&path, b"a").expect("fixture should be written");
        let writer_path = path.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(writer_path)
                .expect("fixture should open");
            file.write_all(b"b").expect("chunk should be written");
        });

        wait_until_stable(
            &path,
            StableOptions {
                stable_for: Duration::from_millis(50),
                attempts: 4,
            },
        )
        .await
        .expect("file should become stable");
        writer.await.expect("writer should finish");
    }
}
