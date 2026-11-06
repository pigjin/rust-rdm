use super::*;

fn merge_keys(old: Vec<KeyEntry>, incoming: Vec<KeyEntry>) -> Vec<KeyEntry> {
    if old.is_empty() {
        return incoming;
    }
    if incoming.is_empty() {
        return old;
    }
    let mut merged = Vec::with_capacity(old.len() + incoming.len());
    let mut old = old.into_iter().peekable();
    let mut incoming = incoming.into_iter().peekable();
    while let (Some(a), Some(b)) = (old.peek(), incoming.peek()) {
        if natural_compare(&display_bytes(&a.key), &display_bytes(&b.key)).is_le() {
            merged.push(old.next().unwrap());
        } else {
            merged.push(incoming.next().unwrap());
        }
    }
    merged.extend(old);
    merged.extend(incoming);
    merged
}

impl RdmApp {
    pub(super) fn cancel_key_scan(&mut self, id: u64) {
        if let Some(i) = self.tab_index(id) {
            self.tabs[i].scan_cancel.cancel();
            self.tabs[i].keys_loading = false;
        }
    }

    pub(super) fn scan_keys(
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
        if t.keys_loading && !reset {
            return;
        }
        t.scan_cancel.cancel();
        t.scan_cancel = Cancellation::default();
        let token = t.scan_cancel.clone();
        if reset {
            t.key_types.reset();
            t.scan = ScanState::default();
            t.scan_epoch += 1;
            t.key_revision += 1;
            t.keys.clear();
            t.selected.clear();
        }
        t.keys_loading = true;
        let epoch = t.scan_epoch;
        let config = t.conn.clone();
        let db = t.db;
        let mut state = t.scan.clone();
        state.total = None;
        let mut seen = t
            .keys
            .iter()
            .map(|entry| entry.key.clone())
            .collect::<HashSet<_>>();
        let pattern = if t.key_match.is_empty() && !t.exact {
            config.default_filter.clone()
        } else {
            t.key_match.clone()
        };
        let pattern = if !t.exact
            && !pattern
                .chars()
                .any(|c| matches!(c, '?' | '*' | '[' | ']' | '{' | '}'))
        {
            format!("*{pattern}*")
        } else {
            pattern
        };
        let kind = t.key_type.clone();
        let exact = t.exact;
        let load_size = config.load_size;
        let count = self.pref.general.scan_size.max(1);
        let count = if all || load_size == 0 {
            count
        } else {
            count.min(load_size)
        };
        let service = self.service.clone();
        self.busy += 1;
        self.key_scans += 1;
        cx.spawn_in(window, async move |this, cx| {
            let mut pending: Vec<KeyEntry> = vec![];
            let mut loaded = 0;
            let mut first = true;
            let mut emitted = Instant::now();
            loop {
                if token.canceled() {
                    break;
                }
                let (service, config, pattern, kind, cancel, scan) = (
                    service.clone(),
                    config.clone(),
                    pattern.clone(),
                    kind.clone(),
                    token.clone(),
                    state.clone(),
                );
                let (next_seen, batch, result, next_loaded, publish, done) = cx
                    .background_executor()
                    .spawn(async move {
                        let before = pending.len();
                        let result = service
                            .scan(&config, db, &pattern, &kind, exact, scan, count, &cancel)
                            .map(|mut page| {
                                pending.extend(
                                    page.keys
                                        .drain(..)
                                        .filter(|entry| seen.insert(entry.key.clone())),
                                );
                                page
                            });
                        let loaded = loaded + pending.len() - before;
                        let done = result.as_ref().map_or(true, |page| {
                            page.state.end || (!all && (load_size == 0 || loaded >= load_size))
                        });
                        let publish = done
                            || (!pending.is_empty() && first)
                            || emitted.elapsed() >= Duration::from_millis(100);
                        if publish {
                            pending.sort_by(|a, b| {
                                natural_compare(&display_bytes(&a.key), &display_bytes(&b.key))
                            });
                        }
                        (seen, pending, result, loaded, publish, done)
                    })
                    .await;
                seen = next_seen;
                pending = batch;
                loaded = next_loaded;
                if token.canceled() {
                    break;
                }
                let error = match result {
                    Ok(page) => {
                        state = page.state;
                        None
                    }
                    Err(error) => Some(error),
                };
                if publish {
                    first &= pending.is_empty();
                    let incoming = std::mem::take(&mut pending);
                    let applied = this
                        .update_in(cx, |app, _, cx| {
                            let Some(i) = app.tab_index(id) else {
                                return false;
                            };
                            if token.canceled()
                                || app.tabs[i].db != db
                                || app.tabs[i].scan_epoch != epoch
                            {
                                return false;
                            }
                            let t = &mut app.tabs[i];
                            if !incoming.is_empty() {
                                t.keys = merge_keys(std::mem::take(&mut t.keys), incoming);
                                t.key_revision += 1;
                            }
                            t.scan = state.clone();
                            if let Some(total) = state.total {
                                t.total = total;
                            }
                            if let Some(error) = error {
                                app.toast(error.to_string(), true);
                            }
                            cx.notify();
                            true
                        })
                        .unwrap_or(false);
                    emitted = Instant::now();
                    if !applied {
                        break;
                    }
                }
                if done {
                    break;
                }
            }
            let _ = this.update_in(cx, |app, _, cx| {
                app.busy = app.busy.saturating_sub(1);
                app.key_scans = app.key_scans.saturating_sub(1);
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].db == db
                    && app.tabs[i].scan_epoch == epoch
                    && !token.canceled()
                {
                    app.tabs[i].keys_loading = false;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::natural_compare;
    use super::merge_keys;
    use crate::model::{KeyEntry, KeyType, display_bytes};
    #[test]
    fn merging_batches_preserves_natural_order_and_binary_keys() {
        let entry = |key: &[u8]| KeyEntry {
            key: key.to_vec(),
            kind: KeyType::Unknown,
        };
        let old = vec![entry(b"a:1"), entry(b"a:10")];
        let incoming = vec![entry(b"\xff\0"), entry(b"a:2")];
        let mut expected = old.iter().chain(&incoming).cloned().collect::<Vec<_>>();
        expected.sort_by(|a, b| natural_compare(&display_bytes(&a.key), &display_bytes(&b.key)));
        assert_eq!(
            merge_keys(old, incoming)
                .iter()
                .map(|e| &e.key)
                .collect::<Vec<_>>(),
            expected.iter().map(|e| &e.key).collect::<Vec<_>>()
        );
        assert!(merge_keys(vec![], vec![]).is_empty());
    }
}
