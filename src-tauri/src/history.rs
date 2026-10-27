//! The most recent events, kept in memory for `get_history`.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use crate::events::HistoryEntry;

const CAPACITY: usize = 500;

#[derive(Debug, Default)]
pub struct History {
    entries: Mutex<VecDeque<HistoryEntry>>,
}

impl History {
    pub fn push(&self, entry: HistoryEntry) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Up to `limit` entries, newest first.
    pub fn recent(&self, limit: usize) -> Vec<HistoryEntry> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::RuleNotice;
    use motor::Uuid;

    fn entry(n: u64) -> HistoryEntry {
        HistoryEntry::RuleError(RuleNotice {
            at: n,
            rule_id: Uuid::nil(),
            rule_name: "r".into(),
            message_pt: n.to_string(),
            can_resume: false,
        })
    }

    fn at(entry: &HistoryEntry) -> u64 {
        match entry {
            HistoryEntry::RuleError(n) => n.at,
            _ => unreachable!(),
        }
    }

    #[test]
    fn keeps_the_last_500_newest_first() {
        let history = History::default();
        for n in 0..600 {
            history.push(entry(n));
        }
        let all = history.recent(usize::MAX);
        assert_eq!(all.len(), 500);
        assert_eq!(at(&all[0]), 599);
        assert_eq!(at(&all[499]), 100);
        assert_eq!(
            history.recent(3).iter().map(at).collect::<Vec<_>>(),
            [599, 598, 597]
        );
    }
}
