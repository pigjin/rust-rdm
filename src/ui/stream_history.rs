use super::*;
const MAX_LINES: usize = 10_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;
impl StreamLine {
    pub(super) fn retained_bytes(&self) -> usize {
        self.time.len() + self.channel.as_ref().map_or(0, String::len) + self.text.len()
    }
}
#[derive(Default)]
pub(super) struct Inbox {
    events: VecDeque<StreamEvent>,
    bytes: usize,
    pub(super) dropped: usize,
}
impl Inbox {
    pub(super) fn push(&mut self, event: StreamEvent) {
        if let StreamEvent::Line(line) = &event {
            let bytes = line.retained_bytes();
            if bytes > MAX_BYTES {
                self.dropped += 1;
                return;
            }
            while self.events.len() >= MAX_LINES || self.bytes + bytes > MAX_BYTES {
                if let Some(StreamEvent::Line(old)) = self.events.pop_front() {
                    self.bytes -= old.retained_bytes();
                    self.dropped += 1;
                } else {
                    break;
                }
            }
            self.bytes += bytes;
        }
        self.events.push_back(event);
    }
    pub(super) fn pop(&mut self) -> Option<StreamEvent> {
        let event = self.events.pop_front()?;
        if let StreamEvent::Line(line) = &event {
            self.bytes -= line.retained_bytes();
        }
        Some(event)
    }
}
impl StreamState {
    pub(super) fn push_line(&mut self, line: StreamLine) {
        let bytes = line.retained_bytes();
        if bytes > MAX_BYTES {
            self.dropped += 1;
            return;
        }
        let mut removed = 0;
        while self.lines.len() >= MAX_LINES || self.retained_bytes + bytes > MAX_BYTES {
            if let Some(old) = self.lines.pop_front() {
                self.retained_bytes -= old.retained_bytes();
                self.dropped += 1;
                removed += 1;
            } else {
                break;
            }
        }
        self.retained_bytes += bytes;
        self.lines.push_back(line);
        if removed > 0 {
            let mut offset = self.list.logical_scroll_top();
            offset.item_ix = offset.item_ix.saturating_sub(removed);
            self.list.reset(self.lines.len());
            if self.auto_last {
                self.list.scroll_to_reveal_item(self.lines.len() - 1);
            } else {
                self.list.scroll_to(offset);
            }
        }
    }
    pub(super) fn clear_lines(&mut self) {
        self.lines.clear();
        self.retained_bytes = 0;
        self.dropped = 0;
        self.list.reset(0);
        let mut inbox = self.receiver.lock();
        inbox.events.clear();
        inbox.bytes = 0;
        inbox.dropped = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::{Inbox, MAX_BYTES, MAX_LINES, StreamEvent, StreamLine, StreamState};
    #[test]
    fn history_evicts_by_count_and_bytes_and_clear_resets_accounting() {
        let mut stream = StreamState::idle();
        for index in 0..10_010 {
            stream.push_line(StreamLine {
                time: String::new(),
                channel: None,
                text: index.to_string(),
            });
        }
        assert_eq!(stream.lines.len(), MAX_LINES);
        assert_eq!(stream.dropped, 10);
        assert_eq!(stream.lines.front().unwrap().text, "10");
        stream.clear_lines();
        for _ in 0..3 {
            stream.push_line(StreamLine {
                time: String::new(),
                channel: Some("中文".into()),
                text: "x".repeat(MAX_BYTES / 2),
            });
        }
        assert_eq!(stream.lines.len(), 1);
        assert_eq!(stream.dropped, 2);
        assert!(stream.retained_bytes <= MAX_BYTES);
        stream.push_line(StreamLine {
            time: String::new(),
            channel: None,
            text: "x".repeat(MAX_BYTES + 1),
        });
        assert_eq!(stream.lines.len(), 1);
        assert_eq!(stream.dropped, 3);
        stream.clear_lines();
        assert_eq!(
            (stream.retained_bytes, stream.dropped, stream.lines.len()),
            (0, 0, 0)
        );
    }
    #[test]
    fn incoming_bursts_retain_the_latest_messages_without_blocking_the_reader() {
        let mut inbox = Inbox::default();
        for n in 0..10_010 {
            inbox.push(StreamEvent::Line(StreamLine {
                time: String::new(),
                channel: None,
                text: n.to_string(),
            }));
            if n == 1999 {
                assert_eq!(inbox.events.len(), 2000);
                assert_eq!(
                    inbox.dropped, 0,
                    "bursts within the history budget must not discard messages"
                );
            }
        }
        assert_eq!(inbox.events.len(), MAX_LINES);
        assert_eq!(inbox.dropped, 10);
        assert!(matches!(inbox.pop(),Some(StreamEvent::Line(line)) if line.text=="10"));
        inbox.push(StreamEvent::Stopped);
        while let Some(event) = inbox.pop() {
            if matches!(event, StreamEvent::Stopped) {
                assert_eq!(inbox.bytes, 0);
                return;
            }
        }
        panic!("terminal event must survive a full incoming queue");
    }
}
