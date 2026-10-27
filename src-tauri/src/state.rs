use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use monitor::Monitor;
use motor::Config;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::history::History;

pub fn config_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join("config.json")
}

pub struct AppState {
    pub app: AppHandle,
    pub config: Mutex<Config>,
    pub monitor: Mutex<Monitor>,
    pub mutation: Mutex<()>,
    pub config_path: PathBuf,
    pub log_dir: PathBuf,
    pub history: Arc<History>,
    pub all_rules_paused: AtomicBool,
}

impl AppState {
    pub fn new(
        app: AppHandle,
        config: Config,
        monitor: Monitor,
        config_path: PathBuf,
        log_dir: PathBuf,
        history: Arc<History>,
    ) -> Self {
        Self {
            app,
            config: Mutex::new(config),
            monitor: Mutex::new(monitor),
            mutation: Mutex::new(()),
            config_path,
            log_dir,
            history,
            all_rules_paused: AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use motor::Config;

    use super::*;

    #[test]
    fn config_survives_an_application_version_bump() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = config_file_path(directory.path());
        let mut expected = Config::default();
        expected.settings.notifications = false;
        expected.settings.skipped_version = Some("0.1.0".to_owned());
        expected.save(&path).expect("config should be saved");

        for _installed_version in ["0.1.0", "0.1.1"] {
            let loaded = Config::load(&path).expect("config should survive the version bump");
            assert_eq!(loaded, expected);
        }
    }
}
