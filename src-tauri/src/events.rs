use std::path::Path;

use monitor::MonitorEvent;
use motor::{Outcome, Uuid};
use serde::Serialize;

pub const FILE_PROCESSED_EVENT: &str = "file-processed";
pub const RULE_ERROR_EVENT: &str = "rule-error";
pub const RULE_PAUSED_EVENT: &str = "rule-paused";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProcessedPayload {
    pub rule_id: Uuid,
    pub file: String,
    pub outcome: OutcomePayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleErrorPayload {
    pub rule_id: Uuid,
    pub message_pt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePausedPayload {
    pub rule_id: Uuid,
    pub reason_pt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutcomePayload {
    Moved { to: String },
    Copied { to: String },
    Skipped { reason: String },
    WouldMove { to: String },
    WouldCopy { to: String },
    NotMatched,
    Failed { error: String },
}

impl From<Outcome> for OutcomePayload {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Moved { to } => Self::Moved { to: path_text(&to) },
            Outcome::Copied { to } => Self::Copied { to: path_text(&to) },
            Outcome::Skipped { reason } => Self::Skipped { reason },
            Outcome::WouldMove { to } => Self::WouldMove { to: path_text(&to) },
            Outcome::WouldCopy { to } => Self::WouldCopy { to: path_text(&to) },
            Outcome::NotMatched => Self::NotMatched,
            Outcome::Failed { error } => Self::Failed { error },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "name", content = "payload", rename_all = "kebab-case")]
pub enum AppEvent {
    FileProcessed(FileProcessedPayload),
    RuleError(RuleErrorPayload),
    RulePaused(RulePausedPayload),
}

impl From<MonitorEvent> for AppEvent {
    fn from(event: MonitorEvent) -> Self {
        match event {
            MonitorEvent::FileProcessed {
                rule_id,
                file,
                outcome,
            } => Self::FileProcessed(FileProcessedPayload {
                rule_id,
                file: path_text(&file),
                outcome: outcome.into(),
            }),
            MonitorEvent::RuleError {
                rule_id,
                message_pt,
            } => Self::RuleError(RuleErrorPayload {
                rule_id,
                message_pt,
            }),
            MonitorEvent::RulePaused { rule_id, reason_pt } => {
                Self::RulePaused(RulePausedPayload { rule_id, reason_pt })
            }
        }
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_frontend_payloads_in_camel_case() {
        let event = AppEvent::FileProcessed(FileProcessedPayload {
            rule_id: Uuid::new_v4(),
            file: "/entrada/arquivo.pdf".to_owned(),
            outcome: OutcomePayload::WouldMove {
                to: "/saida/arquivo.pdf".to_owned(),
            },
        });

        let json = serde_json::to_value(event).expect("event should serialize");

        assert_eq!(json["name"], "file-processed");
        assert!(json["payload"].get("ruleId").is_some());
        assert_eq!(json["payload"]["outcome"]["kind"], "wouldMove");
    }
}
