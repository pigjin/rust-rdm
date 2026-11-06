use super::*;
use crate::service::CommandLog;

pub(super) struct State {
    pub list: ListState,
    pub revision: Option<u64>,
    source: Vec<Arc<CommandLog>>,
    pub rows: Arc<Vec<Arc<CommandLog>>>,
    pub servers: Arc<Vec<String>>,
    filter: String,
    server: String,
    ascending: Option<bool>,
    command_width: Option<f32>,
    pub jump_to_end: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            list: ListState::new(0, ListAlignment::Top, px(100.)),
            revision: None,
            source: vec![],
            rows: Arc::new(vec![]),
            servers: Arc::new(vec![]),
            filter: String::new(),
            server: String::new(),
            ascending: None,
            command_width: None,
            jump_to_end: false,
        }
    }
}

impl State {
    pub fn update(
        &mut self,
        service: &Service,
        filter: String,
        server: &str,
        ascending: Option<bool>,
        command_width: Option<f32>,
    ) {
        let mut changed = false;
        if let Some(history) = service.history.try_lock()
            && self.revision != Some(history.revision())
        {
            self.source = history.iter().cloned().collect();
            self.revision = Some(history.revision());
            changed = true;
        }
        if changed {
            let mut seen = HashSet::new();
            self.servers = Arc::new(
                self.source
                    .iter()
                    .filter_map(|log| {
                        (!log.server.is_empty() && seen.insert(&log.server))
                            .then(|| log.server.clone())
                    })
                    .collect(),
            );
        }
        let query_changed =
            self.filter != filter || self.server != server || self.ascending != ascending;
        if changed || query_changed {
            let mut offset = self.list.logical_scroll_top();
            let anchor = self.rows.get(offset.item_ix);
            let mut rows = self
                .source
                .iter()
                .filter(|log| {
                    (server.is_empty() || log.server == server)
                        && (filter.is_empty() || log.command.contains(&filter))
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(ascending) = ascending {
                rows.sort_by(|a, b| a.time.cmp(&b.time));
                if !ascending {
                    rows.reverse();
                }
            }
            self.list.reset_with_uniform_height(rows.len(), px(45.));
            if !query_changed {
                if let Some(index) =
                    anchor.and_then(|anchor| rows.iter().position(|log| Arc::ptr_eq(log, anchor)))
                {
                    offset.item_ix = index;
                } else {
                    offset.item_ix = offset.item_ix.min(rows.len().saturating_sub(1));
                    offset.offset_in_item = px(0.);
                }
                self.list.scroll_to(offset);
            }
            self.rows = Arc::new(rows);
            self.filter = filter;
            self.server = server.into();
            self.ascending = ascending;
        }
        if self.command_width != command_width {
            self.list.remeasure();
            self.command_width = command_width;
        }
        if std::mem::take(&mut self.jump_to_end) {
            self.list.scroll_to_end();
        }
    }
}

pub(super) fn command_preview(text: &str) -> Cow<'_, str> {
    let mut lines = 1;
    for (count, (offset, ch)) in text.char_indices().take(513).enumerate() {
        if ch == '\n' {
            lines += 1;
        }
        if count == 512 || lines > 4 {
            return format!("{}…", &text[..offset]).into();
        }
    }
    text.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn log(time: &str, server: &str, command: &str) -> CommandLog {
        CommandLog {
            time: time.into(),
            server: server.into(),
            db: 0,
            command: command.into(),
            elapsed_ms: 0.,
            error: String::new(),
        }
    }

    #[test]
    fn preview_bounds_multiline_and_unicode_without_changing_short_commands() {
        assert_eq!(command_preview("GET key"), "GET key");
        assert_eq!(command_preview("1\n2\n3\n4\n5"), "1\n2\n3\n4…");
        let text = "中😀".repeat(400);
        let preview = command_preview(&text);
        assert_eq!(preview.chars().count(), 513);
        assert!(preview.ends_with('…'));
        assert_eq!(command_preview(&"x".repeat(512)).len(), 512);
    }

    #[test]
    fn cache_reuses_rows_handles_contention_and_tracks_clear_and_rollover() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::new(Store::new(dir.path()));
        service
            .history
            .lock()
            .push_back(log("2", "a", "GET second"));
        service.history.lock().push_back(log("1", "b", "SET first"));
        let mut state = State::default();
        state.update(&service, "".into(), "", Some(true), None);
        let rows = state.rows.clone();
        assert_eq!(rows[0].command, "SET first");
        state.update(&service, "".into(), "", Some(true), None);
        assert!(Arc::ptr_eq(&rows, &state.rows));
        let guard = service.history.lock();
        state.update(&service, "GET".into(), "a", Some(false), None);
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.rows[0].command, "GET second");
        drop(guard);
        state.update(&service, "get".into(), "", None, None);
        assert!(state.rows.is_empty());
        for i in 0..5001 {
            service
                .history
                .lock()
                .push_back(log("3", "a", &format!("GET {i}")));
        }
        state.update(&service, "".into(), "", None, None);
        assert_eq!(state.rows.len(), 5000);
        assert_eq!(state.rows[0].command, "GET 1");
        state.list.scroll_to(ListOffset {
            item_ix: 100,
            offset_in_item: px(0.),
        });
        let anchor = state.rows[100].clone();
        service.history.lock().push_back(log("4", "a", "new"));
        state.update(&service, "".into(), "", None, None);
        assert!(Arc::ptr_eq(
            &state.rows[state.list.logical_scroll_top().item_ix],
            &anchor
        ));
        service.history.lock().clear();
        state.update(&service, "".into(), "", None, None);
        assert!(state.rows.is_empty());
        assert!(state.servers.is_empty());
        assert_eq!(state.list.item_count(), 0);
    }
}
