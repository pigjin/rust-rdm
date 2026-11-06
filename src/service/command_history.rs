use super::CommandLog;
use std::{collections::VecDeque, sync::Arc};

const MAX_ENTRIES: usize = 5000;
const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct CommandHistory {
    entries: VecDeque<Arc<CommandLog>>,
    revision: u64,
    bytes: usize,
}

fn bytes(log: &CommandLog) -> usize {
    log.time.len() + log.server.len() + log.command.len() + log.error.len()
}

impl CommandHistory {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Arc<CommandLog>> {
        self.entries.iter()
    }

    pub fn push_back(&mut self, log: CommandLog) {
        self.bytes += bytes(&log);
        self.entries.push_back(Arc::new(log));
        // shortcut: keep one oversized command intact; use disk storage if a strict byte cap is needed.
        while self.entries.len() > MAX_ENTRIES || (self.bytes > MAX_BYTES && self.entries.len() > 1)
        {
            self.bytes -= bytes(&self.entries.pop_front().unwrap());
        }
        self.revision += 1;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(command: String) -> CommandLog {
        CommandLog {
            time: String::new(),
            server: String::new(),
            db: 0,
            command,
            elapsed_ms: 0.,
            error: String::new(),
        }
    }

    #[test]
    fn history_rollover_changes_revision_and_shares_immutable_records() {
        let mut history = CommandHistory::default();
        for i in 0..MAX_ENTRIES {
            history.push_back(log(i.to_string()));
        }
        let snapshot = history.clone();
        assert!(Arc::ptr_eq(
            history.iter().next().unwrap(),
            snapshot.iter().next().unwrap()
        ));
        let revision = history.revision();
        history.push_back(log("new".into()));
        assert_eq!(history.len(), MAX_ENTRIES);
        assert_ne!(history.revision(), revision);
        assert_eq!(history.iter().next().unwrap().command, "1");
        assert_eq!(snapshot.iter().next().unwrap().command, "0");
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.bytes, 0);
        assert_ne!(history.revision(), revision + 1);
    }

    #[test]
    fn byte_budget_evicts_old_entries_without_truncating_the_latest_command() {
        let mut history = CommandHistory::default();
        for _ in 0..3 {
            history.push_back(log("x".repeat(MAX_BYTES / 2)));
        }
        assert_eq!(history.len(), 2);
        assert_eq!(history.bytes, MAX_BYTES);
        history.push_back(log("中".repeat(MAX_BYTES / 3 + 1)));
        assert_eq!(history.len(), 1);
        assert!(history.iter().next().unwrap().command.len() > MAX_BYTES);
        history.push_back(log("PING".into()));
        assert_eq!(history.len(), 1);
        assert_eq!(history.bytes, 4);
    }
}
