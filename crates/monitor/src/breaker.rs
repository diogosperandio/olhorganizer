//! Loop protection: the app remembers what it wrote itself, and a rule that
//! keeps processing the same file is stopped. Nothing here deletes anything.

use std::collections::{HashMap, VecDeque};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::Notify;

/// Shown when the breaker pauses a rule.
pub const BREAKER_MESSAGE: &str = "Regra pausada: o mesmo arquivo foi processado várias vezes seguidas. Verifique se outra regra está devolvendo os arquivos para esta pasta.";

/// Files the app moved or copied recently, by destination path.
#[derive(Debug, Default)]
pub struct WriteLog {
    entries: HashMap<PathBuf, Written>,
}

#[derive(Debug)]
struct Written {
    at: Instant,
    size: u64,
    modified: Option<SystemTime>,
}

impl WriteLog {
    /// Remembers a file the app just wrote; entries older than `keep` are dropped.
    pub fn record(&mut self, path: PathBuf, meta: &Metadata, keep: Duration) {
        let now = Instant::now();
        self.entries.retain(|_, w| now.duration_since(w.at) < keep);
        self.entries.insert(
            path,
            Written {
                at: now,
                size: meta.len(),
                modified: meta.modified().ok(),
            },
        );
    }

    /// Whether a watcher event for `path` is the echo of the app's own write:
    /// written less than `grace` ago and unchanged since.
    pub fn is_own_write(&self, path: &Path, meta: &Metadata, grace: Duration) -> bool {
        self.entries.get(path).is_some_and(|w| {
            w.at.elapsed() < grace && w.size == meta.len() && w.modified == meta.modified().ok()
        })
    }
}

/// A file as the breaker sees it: its name without the " (2)" counters the
/// conflict policy adds, its size and its modification time. The folder is
/// ignored, so a file bouncing between folders keeps its identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identity {
    name: String,
    size: u64,
    modified: Option<SystemTime>,
}

impl Identity {
    pub fn of(path: &Path, meta: &Metadata) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            name: base_name(&name),
            size: meta.len(),
            modified: meta.modified().ok(),
        }
    }
}

/// `relatório (2) (3).pdf` -> `relatório.pdf`.
fn base_name(name: &str) -> String {
    let (mut stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    while let Some(shorter) = strip_counter(stem) {
        stem = shorter;
    }
    format!("{stem}{ext}")
}

fn strip_counter(stem: &str) -> Option<&str> {
    let inner = stem.strip_suffix(')')?;
    let open = inner.rfind(" (")?;
    let digits = &inner[open + 2..];
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| &stem[..open])
}

/// Counts, for one rule, how often each identity was processed. Once an
/// identity has had `max_passes` passes within `window`, the next attempt
/// trips the breaker instead of processing the file.
#[derive(Debug)]
pub struct Breaker {
    window: Duration,
    max_passes: u32,
    passes: Mutex<HashMap<Identity, VecDeque<Instant>>>,
    tripped: Notify,
}

impl Breaker {
    pub fn new(window: Duration, max_passes: u32) -> Self {
        Self {
            window,
            max_passes,
            passes: Mutex::default(),
            tripped: Notify::new(),
        }
    }

    /// False, and the breaker trips, when `identity` has used up its passes.
    pub fn allows(&self, identity: &Identity) -> bool {
        let mut passes = self.passes.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        passes.retain(|_, times| {
            while times
                .front()
                .is_some_and(|t| now.duration_since(*t) >= self.window)
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        let used = passes.get(identity).map_or(0, VecDeque::len);
        if used >= self.max_passes as usize {
            self.tripped.notify_one();
            return false;
        }
        true
    }

    /// Counts one completed pass (a move or copy) of `identity`.
    pub fn record(&self, identity: Identity) {
        self.passes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(identity)
            .or_default()
            .push_back(Instant::now());
    }

    /// Resolves once the breaker has tripped.
    pub async fn tripped(&self) {
        self.tripped.notified().await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn base_name_drops_conflict_counters() {
        assert_eq!(base_name("relatório (2) (3).pdf"), "relatório.pdf");
        assert_eq!(base_name("foto (12).jpg"), "foto.jpg");
        assert_eq!(base_name("LEIAME (2)"), "LEIAME");
        assert_eq!(base_name("ata (rascunho).pdf"), "ata (rascunho).pdf");
        assert_eq!(base_name("(2).pdf"), "(2).pdf");
    }

    #[test]
    fn identity_ignores_folder_and_counter() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("b")).unwrap();
        let a = dir.path().join("x.pdf");
        let b = dir.path().join("b/x (2).pdf");
        fs::write(&a, "conteúdo").unwrap();
        fs::copy(&a, &b).unwrap();
        let modified = fs::metadata(&a).unwrap().modified().unwrap();
        fs::File::options()
            .write(true)
            .open(&b)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(
            Identity::of(&a, &fs::metadata(&a).unwrap()),
            Identity::of(&b, &fs::metadata(&b).unwrap())
        );
    }

    #[tokio::test]
    async fn trips_after_max_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        fs::write(&path, "x").unwrap();
        let id = Identity::of(&path, &fs::metadata(&path).unwrap());

        let breaker = Breaker::new(Duration::from_secs(600), 3);
        for _ in 0..3 {
            assert!(breaker.allows(&id));
            breaker.record(id.clone());
        }
        assert!(!breaker.allows(&id));
        tokio::time::timeout(Duration::from_secs(1), breaker.tripped())
            .await
            .expect("breaker should have tripped");
    }

    #[test]
    fn passes_expire_after_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        fs::write(&path, "x").unwrap();
        let id = Identity::of(&path, &fs::metadata(&path).unwrap());

        let breaker = Breaker::new(Duration::from_millis(50), 1);
        breaker.record(id.clone());
        assert!(!breaker.allows(&id));
        std::thread::sleep(Duration::from_millis(80));
        assert!(breaker.allows(&id));
    }

    #[test]
    fn own_writes_are_recognized_only_briefly_and_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.pdf");
        fs::write(&path, "x").unwrap();
        let mut log = WriteLog::default();
        log.record(
            path.clone(),
            &fs::metadata(&path).unwrap(),
            Duration::from_secs(600),
        );

        let grace = Duration::from_millis(100);
        assert!(log.is_own_write(&path, &fs::metadata(&path).unwrap(), grace));
        fs::write(&path, "outro conteúdo").unwrap();
        assert!(!log.is_own_write(&path, &fs::metadata(&path).unwrap(), grace));

        log.record(
            path.clone(),
            &fs::metadata(&path).unwrap(),
            Duration::from_secs(600),
        );
        std::thread::sleep(Duration::from_millis(150));
        assert!(!log.is_own_write(&path, &fs::metadata(&path).unwrap(), grace));
    }
}
