use std::path::PathBuf;
use std::sync::Arc;

use monitor::Monitor;
use motor::Config;
use tokio::sync::Mutex;

use crate::history::History;

pub struct AppState {
    pub config: Mutex<Config>,
    pub monitor: Mutex<Monitor>,
    pub mutation: Mutex<()>,
    pub config_path: PathBuf,
    pub log_dir: PathBuf,
    pub history: Arc<History>,
}

impl AppState {
    pub fn new(
        config: Config,
        monitor: Monitor,
        config_path: PathBuf,
        log_dir: PathBuf,
        history: Arc<History>,
    ) -> Self {
        Self {
            config: Mutex::new(config),
            monitor: Mutex::new(monitor),
            mutation: Mutex::new(()),
            config_path,
            log_dir,
            history,
        }
    }
}
