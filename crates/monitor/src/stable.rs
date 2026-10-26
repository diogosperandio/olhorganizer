//! Waits for a file to stop changing before it is processed.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableConfig {
    /// How long size and modification time must stay the same.
    pub quiet: Duration,
    /// How many checks to make before giving up.
    pub attempts: u32,
}

impl Default for StableConfig {
    fn default() -> Self {
        Self {
            quiet: Duration::from_secs(2),
            attempts: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unstable {
    /// The file disappeared (moved or deleted by someone else).
    Gone,
    /// It was still changing, or locked, after every attempt.
    StillChanging,
}

/// Returns once size and modification time stay the same for `quiet` and,
/// on Windows, the file can be opened for reading (no other program holds it).
pub async fn wait_until_stable(path: &Path, config: StableConfig) -> Result<(), Unstable> {
    let mut last = state(path)?;
    for _ in 0..config.attempts {
        tokio::time::sleep(config.quiet).await;
        let now = state(path)?;
        if now == last && can_open(path) {
            return Ok(());
        }
        last = now;
    }
    Err(Unstable::StillChanging)
}

fn state(path: &Path) -> Result<(u64, Option<SystemTime>), Unstable> {
    let meta = fs::metadata(path).map_err(|_| Unstable::Gone)?;
    Ok((meta.len(), meta.modified().ok()))
}

#[cfg(windows)]
fn can_open(path: &Path) -> bool {
    fs::File::open(path).is_ok()
}

#[cfg(not(windows))]
fn can_open(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const FAST: StableConfig = StableConfig {
        quiet: Duration::from_millis(100),
        attempts: 3,
    };

    #[tokio::test]
    async fn quiet_file_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        fs::write(&path, "x").unwrap();
        assert_eq!(wait_until_stable(&path, FAST).await, Ok(()));
    }

    #[tokio::test]
    async fn missing_file_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        assert_eq!(wait_until_stable(&path, FAST).await, Err(Unstable::Gone));
    }

    #[tokio::test]
    async fn growing_file_gives_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        let mut file = fs::File::create(&path).unwrap();
        let writer = tokio::spawn(async move {
            for _ in 0..20 {
                file.write_all(b"pedaco").unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
        assert_eq!(
            wait_until_stable(&path, FAST).await,
            Err(Unstable::StillChanging)
        );
        writer.await.unwrap();
    }
}
