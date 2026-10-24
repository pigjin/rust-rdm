use super::*;

impl Drop for Tab {
    fn drop(&mut self) {
        self.detail_cancel.cancel();
    }
}
impl RdmApp {
    pub(super) fn cancel_entry_load(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(i) = self.tab_index(id) {
            self.tabs[i].detail_cancel.cancel();
            self.tabs[i].entries_loading = false;
            self.tabs[i].values_loading = false;
            self.tabs[i].format_epoch += 1;
            self.tabs[i].detail_epoch += 1;
            cx.notify();
        }
    }
    pub(super) fn reload_detail(
        &mut self,
        id: u64,
        reset: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_entry_pages(id, reset, false, window, cx);
    }
    pub(super) fn load_all_entries(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_entry_pages(id, true, true, window, cx);
    }
    fn load_entry_pages(
        &mut self,
        id: u64,
        reset: bool,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        let Some(key) = t.current_key.clone() else {
            return;
        };
        if t.entries_loading && !reset {
            return;
        }
        t.detail_cancel.cancel();
        t.detail_cancel = Cancellation::default();
        t.entries_loading = true;
        t.detail_epoch += 1;
        let token = t.detail_cancel.clone();
        let epoch = t.detail_epoch;
        if reset {
            t.removed_fields.clear();
        }
        let config = t.conn.clone();
        let db = t.db;
        let pattern = t.row_match.clone();
        let reverse = t.reverse;
        let page = self.pref.general.scan_size.max(1);
        let mut cursor = if reset {
            0
        } else {
            t.detail.as_ref().map_or(0, |d| d.cursor)
        };
        let mut after = if reset {
            String::new()
        } else {
            t.detail
                .as_ref()
                .map_or_else(String::new, |d| d.stream_cursor.clone())
        };
        let mut seen = if reset {
            HashSet::new()
        } else {
            t.detail.as_ref().map_or_else(HashSet::new, |d| {
                d.rows.iter().map(|r| r.id.clone()).collect()
            })
        };
        let service = self.service.clone();
        self.busy += 1;
        cx.spawn_in(window, async move |this, cx| {
            let mut first = reset;
            let mut loaded = 0;
            loop {
                if token.canceled() {
                    break;
                }
                let (service_arg, config_arg, key_arg, pattern_arg, after_arg) = (
                    service.clone(),
                    config.clone(),
                    key.clone(),
                    pattern.clone(),
                    after.clone(),
                );
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        service_arg.detail(
                            &config_arg,
                            db,
                            &key_arg,
                            cursor,
                            &pattern_arg,
                            page,
                            &after_arg,
                            reverse,
                        )
                    })
                    .await;
                if token.canceled() {
                    break;
                }
                let mut detail = match result {
                    Ok(detail) => detail,
                    Err(error) => {
                        let _ = this.update_in(cx, |app, _, cx| {
                            if let Some(i) = app.tab_index(id)
                                && app.tabs[i].detail_epoch == epoch
                            {
                                let message = if error.is::<KeyNotFound>() {
                                    let t = &mut app.tabs[i];
                                    t.current_key = None;
                                    t.detail = None;
                                    t.pane = Pane::Status;
                                    t.auto_refresh = false;
                                    t.row_values.clear();
                                    t.entry_delete = None;
                                    t.footer_picker = None;
                                    t.footer_hover = None;
                                    t.footer_tooltip = None;
                                    t.footer_hover_epoch += 1;
                                    t.editing = false;
                                    t.format_epoch += 1;
                                    t.values_loading = false;
                                    "load key summary fail: key not exists".to_string()
                                } else {
                                    error.to_string()
                                };
                                app.toast(message, true);
                                cx.notify();
                            }
                        });
                        break;
                    }
                };
                cursor = detail.cursor;
                after = detail.stream_cursor.clone();
                detail.rows.retain(|row| seen.insert(row.id.clone()));
                loaded += detail.rows.len();
                let done = detail.end || (!all && loaded >= page);
                let applied = this
                    .update_in(cx, |app, w, cx| {
                        let Some(i) = app.tab_index(id) else {
                            return false;
                        };
                        if app.tabs[i].detail_epoch != epoch
                            || app.tabs[i].db != db
                            || app.tabs[i].current_key.as_ref() != Some(&key)
                        {
                            return false;
                        }
                        let t = &mut app.tabs[i];
                        let mut detail = detail;
                        if !first && let Some(old) = &mut t.detail {
                            let mut rows = std::mem::take(&mut old.rows);
                            rows.extend(detail.rows);
                            detail.rows = rows;
                        }
                        if detail.kind == KeyType::ZSet {
                            detail.rows.sort_by(|a, b| {
                                a.score
                                    .parse::<f64>()
                                    .unwrap_or_default()
                                    .total_cmp(&b.score.parse::<f64>().unwrap_or_default())
                                    .then_with(|| a.id.cmp(&b.id))
                            });
                            if reverse {
                                detail.rows.reverse();
                            }
                        }
                        if !matches!(detail.kind, KeyType::String | KeyType::Json) {
                            if t.decode == "Auto" {
                                t.decode = "None".into();
                            }
                            if t.format == "Auto" {
                                t.format = "Raw".into();
                            }
                        }
                        if first {
                            t.row_values.clear();
                        }
                        t.detail = Some(detail);
                        t.detail_loaded = Instant::now();
                        t.editing = false;
                        app.prepare_incremental_values(id, w, cx);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                first = false;
                if !applied || done {
                    break;
                }
                // Finish this batch's decoding before queuing another batch.
                while !token.canceled()
                    && this
                        .update_in(cx, |app, _, _| {
                            app.tab_index(id).is_some_and(|i| {
                                app.tabs[i].detail_epoch == epoch && app.tabs[i].values_loading
                            })
                        })
                        .unwrap_or(false)
                {
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
            }
            let _ = this.update_in(cx, |app, _, cx| {
                app.busy = app.busy.saturating_sub(1);
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].detail_epoch == epoch
                {
                    app.tabs[i].entries_loading = false;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
