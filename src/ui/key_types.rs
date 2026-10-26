use super::*;
use gpui::prelude::*;
use std::collections::HashMap;

pub(super) struct VisibleTypes {
    pub view: WeakEntity<RdmApp>,
    pub id: u64,
    pub rows: Arc<Vec<views::TreeRow>>,
}
impl UniformListDecoration for VisibleTypes {
    fn compute(
        &self,
        range: std::ops::Range<usize>,
        _: Bounds<Pixels>,
        _: Point<Pixels>,
        _: Pixels,
        _: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        // Item renderers also run for offscreen size measurement; this range is actually visible.
        let _ = self.view.update(cx, |app, cx| {
            app.load_visible_key_types(self.id, &self.rows[range], window, cx)
        });
        div().into_any_element()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum State {
    Loading,
    Ready(KeyType),
    Failed,
}

#[derive(Default)]
pub(super) struct Cache {
    pub states: HashMap<Vec<u8>, State>,
    pub in_flight: bool,
    pub(super) cancel: Cancellation,
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Cache {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn remember(&mut self, key: Vec<u8>, kind: KeyType) {
        self.states.insert(key, State::Ready(kind));
    }
    fn begin(&mut self, rows: &[views::TreeRow]) -> Vec<Vec<u8>> {
        if self.in_flight {
            return vec![];
        }
        let mut keys = vec![];
        for row in rows {
            if row.kind == KeyType::Unknown
                && let Some(key) = &row.key
            {
                if !self.states.contains_key(key) {
                    self.states.insert(key.clone(), State::Loading);
                    keys.push(key.clone());
                    if keys.len() == 64 {
                        break;
                    }
                }
            }
        }
        self.in_flight = !keys.is_empty();
        keys
    }
    fn finish(&mut self, keys: Vec<Vec<u8>>, result: Result<Vec<KeyType>>) {
        self.in_flight = false;
        let mut types = result.ok().map(Vec::into_iter);
        for key in keys {
            let kind = types.as_mut().and_then(Iterator::next);
            // A detail read or deletion may have superseded this request.
            if self.states.get(&key) == Some(&State::Loading) {
                self.states
                    .insert(key, kind.map(State::Ready).unwrap_or(State::Failed));
            }
        }
    }
}

impl RdmApp {
    pub(super) fn load_visible_key_types(
        &mut self,
        id: u64,
        rows: &[views::TreeRow],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.tabs[i];
        if tab.keys_loading {
            return;
        }
        let keys = tab.key_types.begin(rows);
        if keys.is_empty() {
            return;
        }
        let (config, db, epoch, cancel) = (
            tab.conn.clone(),
            tab.db,
            tab.scan_epoch,
            tab.key_types.cancel.clone(),
        );
        let service = self.service.clone();
        cx.spawn_in(window, async move |this, cx| {
            let request_keys = keys.clone();
            let token = cancel.clone();
            let result = cx
                .background_executor()
                .spawn(async move { service.key_types(&config, db, &request_keys, &token) })
                .await;
            if cancel.canceled() {
                return;
            }
            let _ = this.update_in(cx, |app, _, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].db == db
                    && app.tabs[i].scan_epoch == epoch
                {
                    app.tabs[i].key_types.finish(keys, result);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Cache, State};
    use crate::{model::KeyType, ui::views};
    fn row(index: usize) -> views::TreeRow {
        views::TreeRow {
            key: Some(index.to_string().into_bytes()),
            label: index.to_string(),
            path: String::new(),
            depth: 0,
            kind: KeyType::Unknown,
        }
    }
    #[test]
    fn visible_batches_are_bounded_deduplicated_and_retry_only_after_reset() {
        let mut cache = Cache::default();
        let rows = (0..100).map(row).collect::<Vec<_>>();
        let first = cache.begin(&rows);
        assert_eq!(first.len(), 64);
        assert!(cache.begin(&rows[64..]).is_empty());
        cache.finish(first, Err(anyhow::anyhow!("test failure")));
        assert!(cache.begin(&rows[..64]).is_empty());
        let second = cache.begin(&rows[90..]);
        assert_eq!(
            second.len(),
            10,
            "scrolling does not queue the skipped offscreen rows"
        );
        cache.remember(b"90".to_vec(), KeyType::Hash);
        cache.states.remove(b"91".as_slice());
        cache.finish(second, Ok(vec![KeyType::String; 10]));
        assert_eq!(
            cache.states.get(b"90".as_slice()),
            Some(&State::Ready(KeyType::Hash))
        );
        assert!(!cache.states.contains_key(b"91".as_slice()));
        let token = cache.cancel.clone();
        cache.reset();
        assert!(token.canceled());
        assert_eq!(cache.begin(&rows[..5]).len(), 5);
        let token = cache.cancel.clone();
        drop(cache);
        assert!(token.canceled());
    }
}
