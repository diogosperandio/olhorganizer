//! App-wide state: the config (memory and disk), the monitor and the history.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use monitor::{Monitor, MonitorEvent};
use motor::Config;
use tokio::sync::mpsc::UnboundedSender;

use crate::history::History;
use crate::status::StatusBoard;

pub struct AppState {
    config: Mutex<Config>,
    config_path: PathBuf,
    monitor: Mutex<Monitor>,
    pub history: History,
    pub statuses: StatusBoard,
}

impl AppState {
    /// Loads `config_path` (a missing file gives an empty config) and starts
    /// monitoring its rules.
    pub fn start(config_path: PathBuf, events: UnboundedSender<MonitorEvent>) -> Self {
        let config = match Config::load(&config_path) {
            Ok(config) => config,
            Err(e) => {
                // The broken file was backed up to config.json.bak by `load`.
                tracing::error!(error = %e, path = %config_path.display(), "config not loaded, starting empty");
                Config::default()
            }
        };
        // The monitor spawns its tasks on the runtime current at start.
        let monitor = {
            let _runtime = tauri::async_runtime::handle().inner().enter();
            Monitor::start(&config, events)
        };
        Self {
            config: Mutex::new(config),
            config_path,
            monitor: Mutex::new(monitor),
            history: History::default(),
            statuses: StatusBoard::default(),
        }
    }

    pub fn config(&self) -> MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes `candidate` to disk, then makes it the current config and
    /// restarts the rules that changed. On a write error nothing changes.
    /// Lock order: config, then monitor.
    pub fn commit(
        &self,
        config: &mut MutexGuard<'_, Config>,
        candidate: Config,
    ) -> Result<(), String> {
        candidate
            .save(&self.config_path)
            .map_err(|e| e.to_string())?;
        // Changed rules restart, so their pause or error is gone.
        for rule in &candidate.rules {
            if !config.rules.contains(rule) {
                self.statuses.reset(rule.id);
            }
        }
        **config = candidate;
        self.monitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reload(config);
        Ok(())
    }

    /// Restarts a rule paused by the circuit breaker.
    pub fn resume(&self, rule_id: motor::Uuid) {
        self.statuses.reset(rule_id);
        self.monitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .resume(rule_id);
    }
}
