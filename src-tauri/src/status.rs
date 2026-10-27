//! Current state of each rule (ok / paused / error) and its last processed
//! file, so the main screen can show it on load, not only from live events.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use motor::Uuid;
use motor::ops::Outcome;
use serde::Serialize;

use crate::events::HistoryEntry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleState {
    #[default]
    Ok,
    Paused,
    Error,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleStatus {
    pub rule_id: Uuid,
    pub state: RuleState,
    /// Why the rule is paused or in error, in pt-BR.
    pub message_pt: Option<String>,
    /// Paused by the circuit breaker: only the user can resume it.
    pub can_resume: bool,
    /// Last file moved or copied, relative to the source folder.
    pub last_file: Option<String>,
    /// When `last_file` was processed, in milliseconds since the Unix epoch.
    pub last_at: Option<u64>,
}

#[derive(Debug, Default)]
pub struct StatusBoard {
    rules: Mutex<HashMap<Uuid, RuleStatus>>,
}

impl StatusBoard {
    /// Updates the state of the rule an event belongs to.
    pub fn apply(&self, entry: &HistoryEntry) {
        let mut rules = self.rules.lock().unwrap_or_else(PoisonError::into_inner);
        let (rule_id, change): (Uuid, &dyn Fn(&mut RuleStatus)) = match entry {
            HistoryEntry::FileProcessed(e) => (e.rule_id, &|s| match &e.outcome {
                Outcome::Moved { .. } | Outcome::Copied { .. } => {
                    s.last_file = Some(e.relative_path.clone());
                    s.last_at = Some(e.at);
                    if s.state == RuleState::Error {
                        clear(s);
                    }
                }
                Outcome::Failed { .. } => {
                    s.state = RuleState::Error;
                    s.message_pt = Some(e.message_pt.clone());
                }
                _ => {}
            }),
            HistoryEntry::RuleError(n) => (n.rule_id, &|s| {
                s.state = RuleState::Error;
                s.message_pt = Some(n.message_pt.clone());
            }),
            HistoryEntry::RulePaused(n) => (n.rule_id, &|s| {
                s.state = RuleState::Paused;
                s.message_pt = Some(n.message_pt.clone());
                s.can_resume = n.can_resume;
            }),
            HistoryEntry::RuleResumed(n) => (n.rule_id, &clear),
            HistoryEntry::RuleHint(_) => return,
        };
        let status = rules.entry(rule_id).or_insert_with(|| RuleStatus {
            rule_id,
            ..RuleStatus::default()
        });
        change(status);
    }

    /// Back to ok after the rule was restarted (saved, toggled or resumed);
    /// the last processed file is kept.
    pub fn reset(&self, rule_id: Uuid) {
        if let Some(status) = self
            .rules
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&rule_id)
        {
            clear(status);
        }
    }

    /// Status of each rule in `ids`, in that order.
    pub fn get(&self, ids: impl IntoIterator<Item = Uuid>) -> Vec<RuleStatus> {
        let rules = self.rules.lock().unwrap_or_else(PoisonError::into_inner);
        ids.into_iter()
            .map(|rule_id| {
                rules.get(&rule_id).cloned().unwrap_or(RuleStatus {
                    rule_id,
                    ..RuleStatus::default()
                })
            })
            .collect()
    }
}

fn clear(status: &mut RuleStatus) {
    status.state = RuleState::Ok;
    status.message_pt = None;
    status.can_resume = false;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::events::{FileProcessed, RuleNotice};

    fn notice(id: Uuid, can_resume: bool) -> RuleNotice {
        RuleNotice {
            at: 1,
            rule_id: id,
            rule_name: "r".into(),
            message_pt: "algo".into(),
            can_resume,
        }
    }

    fn processed(id: Uuid, outcome: Outcome) -> HistoryEntry {
        HistoryEntry::FileProcessed(FileProcessed {
            at: 42,
            rule_id: id,
            rule_name: "r".into(),
            file: PathBuf::from("/x/a.pdf"),
            relative_path: "sub/a.pdf".into(),
            outcome,
            message_pt: "msg".into(),
        })
    }

    #[test]
    fn tracks_state_and_last_file() {
        let board = StatusBoard::default();
        let id = Uuid::new_v4();
        assert_eq!(board.get([id])[0].state, RuleState::Ok);

        board.apply(&processed(
            id,
            Outcome::Moved {
                to: "/y/a.pdf".into(),
            },
        ));
        let s = &board.get([id])[0];
        assert_eq!(
            (s.last_file.as_deref(), s.last_at),
            (Some("sub/a.pdf"), Some(42))
        );

        board.apply(&HistoryEntry::RulePaused(notice(id, true)));
        let s = &board.get([id])[0];
        assert_eq!((s.state, s.can_resume), (RuleState::Paused, true));

        board.reset(id);
        let s = &board.get([id])[0];
        assert_eq!((s.state, s.can_resume), (RuleState::Ok, false));
        assert_eq!(s.last_file.as_deref(), Some("sub/a.pdf"));

        board.apply(&processed(id, Outcome::Failed { error: "x".into() }));
        assert_eq!(board.get([id])[0].state, RuleState::Error);
        board.apply(&processed(
            id,
            Outcome::Copied {
                to: "/y/b.pdf".into(),
            },
        ));
        assert_eq!(board.get([id])[0].state, RuleState::Ok);
    }
}
