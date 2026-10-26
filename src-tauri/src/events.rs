//! Monitor events, forwarded to the UI, the in-memory history and the log.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use monitor::MonitorEvent;
use motor::ops::Outcome;
use motor::plan::relative_label;
use motor::{Config, Uuid};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::state::AppState;

/// Payload of "file-processed".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProcessed {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub rule_id: Uuid,
    pub rule_name: String,
    pub file: PathBuf,
    /// Relative to the rule's source folder: `2025/fotos/IMG_0001.jpg`.
    pub relative_path: String,
    pub outcome: Outcome,
    /// One pt-BR line, e.g. "20261022.pdf movido para D:\Jornais\2026\10".
    pub message_pt: String,
}

/// Payload of "rule-error", "rule-paused", "rule-resumed" and "rule-hint".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleNotice {
    pub at: u64,
    pub rule_id: Uuid,
    pub rule_name: String,
    pub message_pt: String,
}

/// One event as kept in the history; the tag is also the event name.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum HistoryEntry {
    FileProcessed(FileProcessed),
    RuleError(RuleNotice),
    RulePaused(RuleNotice),
    RuleResumed(RuleNotice),
    /// Advice, such as preferring interval mode on network folders.
    RuleHint(RuleNotice),
}

impl HistoryEntry {
    fn from_monitor(event: &MonitorEvent, config: &Config) -> Self {
        let rule = config.rules.iter().find(|r| r.id == event.rule_id());
        let rule_name = rule.map_or_else(|| "(regra removida)".to_owned(), |r| r.name.clone());
        let source_dir = rule.map(|r| r.source_dir.as_path());
        let message_pt = match source_dir {
            Some(dir) => event.describe_pt_from(dir),
            None => event.describe_pt(),
        };
        let notice = || RuleNotice {
            at: now_ms(),
            rule_id: event.rule_id(),
            rule_name: rule_name.clone(),
            message_pt: message_pt.clone(),
        };

        match event {
            MonitorEvent::FileProcessed { file, outcome, .. } => {
                Self::FileProcessed(FileProcessed {
                    at: now_ms(),
                    rule_id: event.rule_id(),
                    rule_name: rule_name.clone(),
                    file: file.clone(),
                    relative_path: relative_path(source_dir, file),
                    outcome: outcome.clone(),
                    message_pt: message_pt.clone(),
                })
            }
            MonitorEvent::RuleError { .. } => Self::RuleError(notice()),
            MonitorEvent::RulePaused { .. } => Self::RulePaused(notice()),
            MonitorEvent::RuleResumed { .. } => Self::RuleResumed(notice()),
            MonitorEvent::NetworkHint { .. } => Self::RuleHint(notice()),
        }
    }

    fn emit(&self, app: &AppHandle) -> tauri::Result<()> {
        match self {
            Self::FileProcessed(p) => app.emit("file-processed", p),
            Self::RuleError(p) => app.emit("rule-error", p),
            Self::RulePaused(p) => app.emit("rule-paused", p),
            Self::RuleResumed(p) => app.emit("rule-resumed", p),
            Self::RuleHint(p) => app.emit("rule-hint", p),
        }
    }

    fn log(&self) {
        match self {
            Self::FileProcessed(p) => {
                if matches!(p.outcome, Outcome::Failed { .. }) {
                    tracing::warn!(target: "olhorganizer::eventos", regra = %p.rule_name, "{}", p.message_pt);
                } else {
                    tracing::info!(target: "olhorganizer::eventos", regra = %p.rule_name, "{}", p.message_pt);
                }
            }
            Self::RuleError(p) | Self::RulePaused(p) => {
                tracing::warn!(target: "olhorganizer::eventos", regra = %p.rule_name, "{}", p.message_pt);
            }
            Self::RuleResumed(p) | Self::RuleHint(p) => {
                tracing::info!(target: "olhorganizer::eventos", regra = %p.rule_name, "{}", p.message_pt);
            }
        }
    }
}

fn relative_path(source_dir: Option<&Path>, file: &Path) -> String {
    match source_dir {
        Some(dir) if file.starts_with(dir) => relative_label(dir, file),
        _ => file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Forwards every monitor event to the log, the history and the UI.
pub fn spawn_forwarder(app: AppHandle, mut events: UnboundedReceiver<MonitorEvent>) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            let state = app.state::<AppState>();
            let entry = HistoryEntry::from_monitor(&event, &state.config());
            entry.log();
            state.history.push(entry.clone());
            if let Err(e) = entry.emit(&app) {
                tracing::warn!(error = %e, "could not send event to the window");
            }
        }
    });
}
