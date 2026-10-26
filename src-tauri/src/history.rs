use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::events::AppEvent;

const HISTORY_LIMIT: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub timestamp_ms: u128,
    pub event: AppEvent,
}

#[derive(Debug, Default)]
pub struct History {
    entries: Mutex<VecDeque<HistoryEntry>>,
}

impl History {
    pub fn push(&self, event: AppEvent) {
        let timestamp_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            Err(_) => 0,
        };
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        entries.push_back(HistoryEntry {
            timestamp_ms,
            event,
        });
        while entries.len() > HISTORY_LIMIT {
            entries.pop_front();
        }
    }

    pub fn recent(&self, limit: usize) -> Vec<HistoryEntry> {
        let entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        entries
            .iter()
            .rev()
            .take(limit.min(HISTORY_LIMIT))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use motor::Uuid;

    use super::*;
    use crate::events::{AppEvent, RuleErrorPayload};

    #[test]
    fn keeps_only_the_last_five_hundred_events() {
        let history = History::default();
        for index in 0..510 {
            history.push(AppEvent::RuleError(RuleErrorPayload {
                rule_id: Uuid::new_v4(),
                message_pt: index.to_string(),
            }));
        }

        let entries = history.recent(1_000);

        assert_eq!(entries.len(), 500);
        assert!(matches!(
            &entries[0].event,
            AppEvent::RuleError(payload) if payload.message_pt == "509"
        ));
        assert!(matches!(
            &entries[499].event,
            AppEvent::RuleError(payload) if payload.message_pt == "10"
        ));
    }
}
