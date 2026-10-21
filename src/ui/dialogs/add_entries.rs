//! Original AddFieldsDialog: raw rows, operation radios and optional full reload.
use super::*;

impl RdmApp {
    fn add_entry_field(&self, index: usize, label: &str, control: AnyElement) -> AnyElement {
        div()
            .id(("add-entries-field", index))
            .test_support()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(if index % 2 == 0 { 21.5 } else { 21. }))
                    .flex_shrink_0()
                    .pb(px(5.))
                    .pl(px(2.))
                    .text_size(px(13.))
                    .line_height(px(16.25))
                    .font_weight(FontWeight(450.))
                    .text_color(rgb(0x717880))
                    .child(label.to_string()),
            )
            .child(control)
            .child(div().h(px(18.)).flex_shrink_0())
            .into_any_element()
    }

    fn add_entry_operations(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let list = dialog.value("type", cx) == "LIST";
        let value = dialog.value("op_type", cx);
        let radio_color:Hsla=rgb(0xd13b37).into();
        let focus = &dialog.new_key.as_ref().unwrap().type_focus;
        let mut group = div()
            .id("add-entries-operations")
            .test_support()
            .role(Role::RadioGroup)
            .track_focus(focus)
            .flex()
            .gap(px(1.))
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, w, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "left" | "right" | "up" | "down"
                ) {
                    let value = app.dialog.as_ref().unwrap().value("op_type", cx);
                    app.set_dialog_field("op_type", if value == "0" { "1" } else { "0" }, w, cx);
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }));
        for (index, key) in (if list {
            ["dialogue.field.append_item", "dialogue.field.prepend_item"]
        } else {
            [
                "dialogue.field.overwrite_field",
                "dialogue.field.ignore_field",
            ]
        })
        .into_iter()
        .enumerate()
        {
            let chosen = value == index.to_string();
            let dark = self.dark(cx);
            group = group.child(
                self.btn(
                    format!("add-entries-op-{index}"),
                    self.tr(key),
                    "",
                    cx,
                    move |app, w, cx| {
                        let focus = app
                            .dialog
                            .as_ref()
                            .unwrap()
                            .new_key
                            .as_ref()
                            .unwrap()
                            .type_focus
                            .clone();
                        focus.focus(w, cx);
                        app.set_dialog_field("op_type", index.to_string(), w, cx);
                    },
                )
                .px(px(13.5))
                .w(px(self.modal_label_width(&self.tr(key), window) + 29.))
                .rounded(px(0.))
                .when(index == 0, |button| button.rounded_l(px(3.)))
                .when(index == 1, |button| button.rounded_r(px(3.)))
                .bg::<Hsla>(if chosen && dark {
                    radio_color
                } else if dark {
                    Hsla::transparent_black()
                } else {
                    rgb(0xffffff).into()
                })
                .text_color(if chosen {
                    if dark {
                        rgb(0xffffff).into()
                    } else {
                        radio_color
                    }
                } else {
                    self.csv_text_color(cx)
                })
                .border_color(if chosen {
                    radio_color
                } else if dark {
                    rgba(0xffffff3d).into()
                } else {
                    rgb(0xe0e0e6).into()
                })
                .when(chosen && focus.is_focused(window), |button| {
                    button.shadow(vec![
                        BoxShadow::new(px(0.), px(0.), radio_color.opacity(0.3))
                            .spread_radius(px(2.)),
                    ])
                }),
            );
        }
        group.into_any_element()
    }

    pub(super) fn render_add_entries(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let kind = dialog.value("type", cx);
        let choice = matches!(kind.as_str(), "HASH" | "LIST" | "ZSET");
        let stream = kind == "STREAM";
        let count = if choice || stream { 3. } else { 2. };
        let content_height = count * 71.25 + 50. + dialog.rows.len().saturating_sub(1) as f32 * 42.;
        let height = content_height.min(500.);
        let mut form = div()
            .id("add-entries-form")
            .test_support()
            .w_full()
            .pr(px(15.))
            .flex()
            .flex_col()
            .child(
                self.add_entry_field(
                    0,
                    &self.tr("common.key"),
                    self.new_key_input("key", window, cx)
                        .id("add-entries-key")
                        .readonly(true)
                        .into_any_element(),
                ),
            );
        if choice {
            form = form.child(self.add_entry_field(
                1,
                &self.tr(if kind == "HASH" {
                    "dialogue.field.conflict_handle"
                } else {
                    "interface.type"
                }),
                self.add_entry_operations(window, cx),
            ));
        }
        if stream {
            form = form.child(
                self.add_entry_field(
                    1,
                    "ID",
                    self.new_key_input("stream_id", window, cx)
                        .id("add-entries-stream-id")
                        .into_any_element(),
                ),
            );
        }
        let label = if kind == "HASH" {
            format!(
                "{} ({}:{})",
                self.tr("dialogue.field.element"),
                self.tr("common.field"),
                self.tr("common.value")
            )
        } else if kind == "ZSET" {
            format!(
                "{} ({}:{})",
                self.tr("dialogue.field.element"),
                self.tr("common.value"),
                self.tr("common.score")
            )
        } else if stream {
            format!("{}:{}", self.tr("common.field"), self.tr("common.value"))
        } else {
            self.tr("dialogue.field.element")
        };
        form = form
            .child(self.add_entry_field(
                if choice || stream { 2 } else { 1 },
                &label,
                self.render_value_rows(window, cx),
            ))
            .child(
                div()
                    .id("add-entries-reload-field")
                    .test_support()
                    .h(px(50.))
                    .flex_shrink_0()
                    .child(
                        div()
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .child(self.dialog_checkbox(
                                "reload",
                                &self.tr("dialogue.field.reload_when_succ"),
                                cx,
                            )),
                    ),
            );
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .occlude()
            .child(
                div()
                    .id("dialog-card")
                    .test_support()
                    .w(px(600.))
                    .h(px(height + 119.))
                    .px(px(28.))
                    .pt(px(16.))
                    .pb(px(20.))
                    .rounded(px(4.))
                    .bg(rgb(if self.dark(cx) { 0x2c2c32 } else { 0xffffff }))
                    .text_color(self.csv_text_color(cx))
                    .shadow_lg()
                    .font({
                        let mut font = gpui::font(".SystemUIFont");
                        font.fallbacks = Some(FontFallbacks::default());
                        font
                    })
                    .text_size(px(14.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(35.))
                            .flex_shrink_0()
                            .text_size(px(18.))
                        .line_height(px(27.))
                        .font_weight(FontWeight::MEDIUM)
                            .when(self.dark(cx), |title| title.text_color(rgba(0xffffffe6)))
                            .child(dialog.title.clone()),
                    )
                    .child(
                        div()
                            .id("add-entries-scroll")
                            .test_support()
                            .h(px(height))
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .track_scroll(&dialog.new_key.as_ref().unwrap().scroll)
                            .child(form),
                    )
                    .child(
                        div()
                            .id("add-entries-actions")
                            .test_support()
                            .mt(px(16.))
                            .h(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .child(
                                self.csv_secondary_button(
                                    self.btn(
                                        "dialog-cancel",
                                        self.tr("common.cancel"),
                                        "",
                                        cx,
                                        |app, w, cx| app.close_dialog(w, cx),
                                    ),
                                    cx,
                                )
                                .px(px(11.))
                                .w(px(self
                                    .modal_label_width(&self.tr("common.cancel"), window)
                                    + 24.)),
                            )
                            .child(
                                self.btn(
                                    "dialog-save",
                                    self.tr("common.confirm"),
                                    "",
                                    cx,
                                    |app, w, cx| app.submit_dialog(w, cx),
                                )
                                .primary()
                                .px(px(11.))
                                .w(px(self
                                    .modal_label_width(&self.tr("common.confirm"), window)
                                    + 24.))
                                .when(self.dark(cx), |button| button.text_color(rgb(0x000000))),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn submit_add_entries(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let dialog = self.dialog.as_ref().unwrap();
        let target = dialog
            .target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
        let value = self.new_key_value(cx);
        let kind = target.kind.clone();
        if kind == KeyType::Stream && value.as_array().is_none_or(|values| values.len() <= 2) {
            self.dialog = None;
            return Ok(());
        }
        let op = dialog.value("op_type", cx).parse::<i32>().unwrap_or(0);
        let action = if kind == KeyType::List {
            if op == 1 { 0 } else { 1 }
        } else {
            op
        };
        let reload = dialog.boolean("reload");
        let saved = target.clone();
        self.job(
            window,
            cx,
            move |service| {
                Ok(service
                    .add_key_entries(&target.conn, target.db, &target.key, kind, action, &value)
                    .map_err(|error| error.to_string()))
            },
            move |app, result, w, cx| {
                match result {
                    Ok(data) => {
                        let refresh = app.apply_added_entries(&saved, &data, action, cx);
                        app.toast(app.tr("dialogue.handle_succ"), false);
                        if refresh {
                            if reload {
                                app.reload_detail(saved.id, true, w, cx);
                            } else {
                                app.reload_entry_summary(saved.clone(), w, cx);
                            }
                        }
                    }
                    Err(error) => app.toast(error, true),
                }
                // The original store closes its shared AddFields flag, even
                // if that dialog was canceled and reopened while awaiting IO.
                if app
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| matches!(dialog.kind, DialogKind::AddEntries))
                {
                    app.dialog = None;
                }
            },
        );
        Ok(())
    }

    pub(super) fn apply_added_entries(
        &mut self,
        target: &Target,
        data: &Json,
        action: i32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.tab_index(target.id) else {
            return false;
        };
        let tab = &mut self.tabs[index];
        if tab.db != target.db || tab.current_key.as_ref() != Some(&target.key) {
            return false;
        }
        let Some(detail) = &mut tab.detail else {
            return false;
        };
        // A list prepend changes each loaded row's index. Keep its displayed
        // value with the row, as the original tab store keeps the dv property.
        for row in &mut detail.rows {
            if row.display_value.is_none()
                && let Some(value) = tab.row_values.get(&row.id)
            {
                row.display_value = Some(value.text.clone());
            }
        }
        let before = detail.rows.len();
        let parse = |name: &str| {
            data[name]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|row| added_entry(&target.kind, row))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let added = parse("added");
        let updated = parse("updated");
        let left = parse("left");
        let right = parse("right");
        let insert = |rows: &mut Vec<Entry>, entries: Vec<Entry>, front: bool| {
            if front {
                rows.splice(0..0, entries);
            } else {
                rows.extend(entries);
            }
        };
        let update = |rows: &mut Vec<Entry>, entries: Vec<Entry>| {
            for entry in entries {
                if let Some(row) = rows.iter_mut().find(|row| row.id == entry.id) {
                    *row = entry;
                } else {
                    rows.push(entry);
                }
            }
        };
        let refresh = match target.kind {
            KeyType::Hash => {
                update(&mut detail.rows, updated);
                insert(&mut detail.rows, added, false);
                true
            }
            KeyType::Set => {
                insert(&mut detail.rows, added, false);
                true
            }
            KeyType::ZSet => {
                insert(&mut detail.rows, added, false);
                update(&mut detail.rows, updated);
                true
            }
            KeyType::List => {
                let entries = if action == 0 { left } else { right };
                let refresh = !entries.is_empty();
                insert(&mut detail.rows, entries, action == 0);
                for (index, row) in detail.rows.iter_mut().enumerate() {
                    row.id = index.to_string().into_bytes();
                }
                refresh
            }
            KeyType::Stream => {
                let refresh = !added.is_empty();
                insert(&mut detail.rows, added, true);
                refresh
            }
            _ => false,
        };
        detail.length += detail.rows.len().saturating_sub(before) as u64;
        for row in &detail.rows {
            if let Some(text) = &row.display_value {
                tab.row_values.insert(
                    row.id.clone(),
                    codec::Converted {
                        text: text.clone(),
                        decode: "None".into(),
                        format: "Raw".into(),
                    },
                );
            }
        }
        tab.entry_list
            .splice(0..tab.entry_list.item_count(), tab.entry_list.item_count());
        cx.notify();
        refresh
    }

    pub(super) fn reload_entry_summary(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = target.clone();
        self.job(
            window,
            cx,
            move |service| service.key_summary(&target.conn, target.db, &target.key),
            move |app, summary, _, cx| {
                if let Some(i) = app.tab_index(saved.id)
                    && app.tabs[i].db == saved.db
                    && app.tabs[i].current_key.as_ref() == Some(&saved.key)
                    && let Some(detail) = &mut app.tabs[i].detail
                {
                    detail.kind = summary.kind;
                    detail.ttl = summary.ttl;
                    detail.memory = summary.memory;
                    detail.encoding = summary.encoding;
                    detail.length = summary.length;
                    app.tabs[i].detail_loaded = Instant::now();
                    cx.notify();
                }
            },
        );
    }
}

pub(super) fn added_entry(kind: &KeyType, row: &Json) -> Entry {
    let text = |value: &Json| {
        value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string())
    };
    let value = text(&row["v"]);
    let id = if *kind == KeyType::Hash {
        text(&row["k"])
    } else if *kind == KeyType::Stream {
        text(&row["id"])
    } else {
        value.clone()
    };
    let fields = if *kind == KeyType::Stream {
        row["v"]
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .map(|(key, value)| (key.as_bytes().to_vec(), text(value).into_bytes()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };
    let display = row["dv"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&value)
        .to_string();
    Entry {
        id: id.into_bytes(),
        value: value.into_bytes(),
        score: row["s"]
            .as_f64()
            .map(|value| value.to_string())
            .unwrap_or_default(),
        fields,
        display_value: Some(display),
    }
}
