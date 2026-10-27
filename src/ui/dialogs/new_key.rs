//! The original NewKeyDialog has its own plain inputs and emitted value model.
use super::*;
use std::{cell::Cell, rc::Rc};

const TYPES: [&str; 7] = ["STRING", "HASH", "LIST", "SET", "ZSET", "STREAM", "JSON"];
const COLORS: [u32; 7] = [
    0x8b5cf6, 0x3b82f6, 0x10b981, 0xf59e0b, 0xef4444, 0xec4899, 0x828766,
];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Selector {
    Database,
    Type,
}

pub(super) struct NewKeyState {
    pub(super) value: Option<Json>,
    pub(super) errors: HashSet<String>,
    pub(super) selector: Option<Selector>,
    pending: usize,
    pub(super) type_focus: FocusHandle,
    selector_bounds: Rc<Cell<Bounds<Pixels>>>,
    token: Option<gpui_base::DeferredPopover>,
    pub(super) scroll: ScrollHandle,
    menu_scroll: ScrollHandle,
    pub(super) value_height: f32,
    resize: Option<(f32, f32)>,
}

pub(super) fn score_key(row: &Row) -> String {
    format!("new_score.{}", row.score.entity_id().as_u64())
}
pub(super) fn field_key(row: &Row) -> String {
    format!("new_field.{}", row.id.entity_id().as_u64())
}
pub(super) fn value_key(row: &Row) -> String {
    let RowValue::Single(value) = &row.value else {
        unreachable!()
    };
    format!("new_value.{}", value.entity_id().as_u64())
}

impl RdmApp {
    pub(super) fn modal_label_width(&self, label: &str, window: &Window) -> f32 {
        let mut font = gpui::font(".SystemUIFont");
        font.fallbacks = Some(FontFallbacks::default());
        window
            .text_system()
            .shape_line(
                label.to_string().into(),
                px(14.),
                &[TextRun {
                    len: label.len(),
                    font,
                    color: rgb(0x000000).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
            .as_f32()
    }
    pub(super) fn prepare_new_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = input("", "0", false, window, cx);
        let db = self.dialog.as_ref().unwrap().value("db", cx);
        query.update(cx, |state, cx| {
            state.set_placeholder(db, window, cx);
            state.set_placeholder_colors(rgb(0x333639).into(), rgba(0xffffffd1).into(), cx);
        });
        self.watch_dialog_input("new_db_query".into(), &query, window, cx);
        let stream_placeholder = self.control_placeholder(false);
        let dialog = self.dialog.as_mut().unwrap();
        dialog.fields.insert("new_db_query".into(), query);
        if let Some(ttl) = dialog.fields.get("ttl") {
            ttl.update(cx, |state, cx| state.set_placeholder("TTL", window, cx));
        }
        dialog.fields["stream_id"].update(cx, |state, cx| {
            state.set_placeholder(stream_placeholder, window, cx)
        });
        dialog.new_key = Some(NewKeyState {
            value: None,
            errors: HashSet::new(),
            selector: None,
            pending: 0,
            type_focus: cx.focus_handle().tab_stop(true),
            selector_bounds: Rc::new(Cell::new(Bounds::default())),
            token: None,
            scroll: ScrollHandle::new(),
            menu_scroll: ScrollHandle::new(),
            value_height: 137.,
            resize: None,
        });
        self.reset_new_key_rows(window, cx);
    }

    fn new_key_row(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Row {
        let kind = self.dialog.as_ref().unwrap().value("type", cx);
        let row = Row {
            id: input("", self.tr("dialogue.field.enter_field"), false, window, cx),
            value: RowValue::Single(input(
                "",
                self.tr(match kind.as_str() {
                    "LIST" | "SET" => "dialogue.field.enter_elem",
                    "ZSET" if matches!(self.dialog.as_ref().unwrap().kind, DialogKind::NewKey) => {
                        "dialogue.field.enter_member"
                    }
                    _ => "dialogue.field.enter_value",
                }),
                false,
                window,
                cx,
            )),
            score: input(
                "0",
                self.tr("dialogue.field.enter_score"),
                false,
                window,
                cx,
            ),
        };
        let RowValue::Single(value) = &row.value else {
            unreachable!()
        };
        for (key, state) in [
            (field_key(&row), &row.id),
            (value_key(&row), value),
            (score_key(&row), &row.score),
        ] {
            self.dialog
                .as_mut()
                .unwrap()
                .fields
                .insert(key.clone(), state.clone());
            self.watch_dialog_input(key, state, window, cx);
        }
        self.dialog
            .as_mut()
            .unwrap()
            .numbers
            .insert(score_key(&row), Some(0.));
        row
    }

    fn reset_new_key_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = self.dialog.as_mut().unwrap();
        dialog.fields.retain(|key, _| {
            !key.starts_with("new_field.")
                && !key.starts_with("new_value.")
                && !key.starts_with("new_score.")
        });
        dialog
            .numbers
            .retain(|key, _| !key.starts_with("new_score."));
        let row = self.new_key_row(window, cx);
        self.dialog.as_mut().unwrap().rows = vec![row];
    }

    pub(super) fn change_new_key_type(
        &mut self,
        kind: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        if dialog.value("type", cx) == kind {
            return;
        }
        set_input(&dialog.fields["type"], kind, window, cx);
        // A new component instance starts with null, even though its input is empty.
        let value = input("", "", true, window, cx);
        self.watch_dialog_input("value".into(), &value, window, cx);
        let dialog = self.dialog.as_mut().unwrap();
        dialog.fields.insert("value".into(), value);
        set_input(&dialog.fields["stream_id"], "*", window, cx);
        dialog.new_key.as_mut().unwrap().value = None;
        dialog.new_key.as_mut().unwrap().value_height = 137.;
        dialog
            .new_key
            .as_ref()
            .unwrap()
            .scroll
            .set_offset(point(px(0.), px(0.)));
        self.reset_new_key_rows(window, cx);
        cx.notify();
    }

    pub(super) fn new_key_input_changed(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        if !matches!(dialog.kind, DialogKind::NewKey | DialogKind::AddEntries) {
            return;
        }
        let Some(state) = &mut dialog.new_key else {
            return;
        };
        if key == "key" && matches!(dialog.kind, DialogKind::NewKey) {
            if dialog.fields["key"].read(cx).value().is_empty() {
                state.errors.insert("key".into());
            } else {
                state.errors.remove("key");
            }
        } else if key == "new_db_query" {
            if state.selector.is_none() && text_input(&dialog.fields["new_db_query"], cx).is_empty()
            {
                return;
            }
            let options = self.new_key_options(Selector::Database, cx);
            if let Some(state) = &mut self.dialog.as_mut().unwrap().new_key {
                state.selector = Some(Selector::Database);
                state
                    .token
                    .get_or_insert_with(|| gpui_base::GlobalState::register_deferred_popover(cx));
                state.pending = 0;
                state.menu_scroll.set_offset(point(px(0.), px(0.)));
                if options.is_empty() {
                    state.pending = 0;
                }
            }
        } else if key == "value" {
            state.value = Some(Json::String(text_input(&dialog.fields["value"], cx)));
        } else if key.starts_with("new_field.")
            || key.starts_with("new_value.")
            || key.starts_with("new_score.")
        {
            self.update_new_key_rows(cx);
        }
    }

    fn update_new_key_rows(&mut self, cx: &mut Context<Self>) {
        let dialog = self.dialog.as_ref().unwrap();
        let kind = dialog.value("type", cx);
        if kind == "ZSET" && matches!(dialog.kind, DialogKind::AddEntries) {
            let mut values = serde_json::Map::new();
            for row in &dialog.rows {
                let value = text_input(&row.value, cx);
                if value.is_empty() || value == "__proto__" {
                    continue;
                }
                let score = dialog.numbers.get(&score_key(row)).copied().flatten();
                values.insert(
                    value,
                    score
                        .and_then(serde_json::Number::from_f64)
                        .map(Json::Number)
                        .unwrap_or(Json::Null),
                );
            }
            self.dialog
                .as_mut()
                .unwrap()
                .new_key
                .as_mut()
                .unwrap()
                .value = Some(Json::Object(values));
            cx.notify();
            return;
        }
        let mut values = Vec::new();
        let mut seen = HashSet::new();
        for row in &dialog.rows {
            let value = text_input(&row.value, cx);
            match kind.as_str() {
                "HASH" | "STREAM" => {
                    let field = text_input(&row.id, cx);
                    if !field.is_empty() {
                        values.extend([Json::String(field), Json::String(value)]);
                    }
                }
                "LIST" | "SET" if !value.is_empty() => {
                    if kind == "LIST" || seen.insert(value.clone()) {
                        values.push(Json::String(value));
                    }
                }
                "ZSET" if !value.is_empty() => {
                    let Some(score) = dialog.numbers.get(&score_key(row)).copied().flatten() else {
                        // The Source handler throws at null.toString(); its last
                        // successfully emitted model remains the submit value.
                        return;
                    };
                    values.extend([
                        Json::String(value),
                        Json::String(numbers::display(Some(score))),
                    ]);
                }
                _ => {}
            }
        }
        if kind == "STREAM" {
            let id = dialog.value("stream_id", cx);
            values.insert(0, Json::String(if id.is_empty() { "*".into() } else { id }));
        }
        self.dialog
            .as_mut()
            .unwrap()
            .new_key
            .as_mut()
            .unwrap()
            .value = Some(Json::Array(values));
        cx.notify();
    }

    pub(super) fn append_new_key_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = self.new_key_row(window, cx);
        let dialog = self.dialog.as_mut().unwrap();
        dialog.rows.insert((index + 1).min(dialog.rows.len()), row);
        self.update_new_key_rows(cx);
        if matches!(self.dialog.as_ref().unwrap().kind, DialogKind::AddEntries) {
            return;
        }
        let id = self.dialog.as_ref().unwrap().id;
        cx.defer_in(window, move |app, _, cx| {
            if let Some(dialog) = &app.dialog
                && dialog.id == id
            {
                dialog.new_key.as_ref().unwrap().scroll.scroll_to_bottom();
                cx.notify();
            }
        });
    }

    pub(super) fn remove_new_key_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let dialog = self.dialog.as_mut().unwrap();
        if dialog.rows.len() <= 1 || index >= dialog.rows.len() {
            return;
        }
        let row = dialog.rows.remove(index);
        for key in [field_key(&row), value_key(&row), score_key(&row)] {
            dialog.fields.remove(&key);
            dialog.numbers.remove(&key);
        }
        self.update_new_key_rows(cx);
    }

    fn new_key_options(&self, selector: Selector, cx: &App) -> Vec<String> {
        if selector == Selector::Type {
            return TYPES.iter().map(|v| v.to_string()).collect();
        }
        let dialog = self.dialog.as_ref().unwrap();
        let query = dialog.value("new_db_query", cx).to_lowercase();
        dialog
            .target
            .as_ref()
            .and_then(|target| self.tab_index(target.id))
            .map(|i| {
                self.tabs[i]
                    .databases
                    .iter()
                    .map(|db| db.index.to_string())
                    .filter(|db| db.contains(&query))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn open_new_key_selector(
        &mut self,
        selector: Selector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = self.new_key_options(selector, cx);
        let dialog = self.dialog.as_mut().unwrap();
        let value = dialog.value(
            if selector == Selector::Type {
                "type"
            } else {
                "db"
            },
            cx,
        );
        let state = dialog.new_key.as_mut().unwrap();
        state.selector = Some(selector);
        state.pending = options.iter().position(|v| *v == value).unwrap_or(0);
        state
            .token
            .get_or_insert_with(|| gpui_base::GlobalState::register_deferred_popover(cx));
        state.menu_scroll.set_offset(point(px(0.), px(0.)));
        if selector == Selector::Type {
            state.type_focus.focus(window, cx);
        } else {
            dialog.fields["new_db_query"].update(cx, |field, cx| {
                field.set_placeholder_colors(rgb(0xc2c2c2).into(), rgba(0xffffff61).into(), cx);
                field.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn close_new_key_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.dialog
            && let Some(state) = &mut dialog.new_key
        {
            state.selector = None;
            state.token = None;
            let db = text_input(&dialog.fields["db"], cx);
            dialog.fields["new_db_query"].update(cx, |field, cx| {
                field.set_value("", window, cx);
                field.set_placeholder(db, window, cx);
                field.set_placeholder_colors(rgb(0x333639).into(), rgba(0xffffffd1).into(), cx);
            });
        }
        cx.notify();
    }

    fn choose_new_key_option(
        &mut self,
        selector: Selector,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_dialog_field(
            if selector == Selector::Type {
                "type"
            } else {
                "db"
            },
            value,
            window,
            cx,
        );
        self.close_new_key_selector(window, cx);
    }

    fn new_key_selector_key(
        &mut self,
        selector: Selector,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !matches!(key, "down" | "up" | "enter" | "escape" | "tab" | "space") {
            return;
        }
        if key == "escape" {
            self.close_dialog(window, cx);
        } else if key == "tab" {
            self.close_new_key_selector(window, cx);
            return;
        } else {
            let open = self
                .dialog
                .as_ref()
                .unwrap()
                .new_key
                .as_ref()
                .unwrap()
                .selector
                == Some(selector);
            if !open {
                if key != "up" {
                    self.open_new_key_selector(selector, window, cx);
                }
            } else {
                let options = self.new_key_options(selector, cx);
                let state = self.dialog.as_mut().unwrap().new_key.as_mut().unwrap();
                match key {
                    "enter" | "space" if selector == Selector::Type || key == "enter" => {
                        if let Some(value) = options.get(state.pending).cloned() {
                            self.choose_new_key_option(selector, value, window, cx);
                        }
                    }
                    "down" => state.pending = (state.pending + 1) % options.len().max(1),
                    "up" => {
                        state.pending =
                            (state.pending + options.len().max(1) - 1) % options.len().max(1)
                    }
                    _ => return,
                }
                if let Some(state) = self.dialog.as_ref().and_then(|d| d.new_key.as_ref()) {
                    let top = -state.menu_scroll.offset().y.as_f32();
                    let row = state.pending as f32 * 32.;
                    let height = (options.len() as f32 * 32.).min(235.1875);
                    let next = if row < top {
                        row
                    } else if row + 32. > top + height {
                        row + 32. - height
                    } else {
                        top
                    };
                    state.menu_scroll.set_offset(point(px(0.), px(-next)));
                }
                cx.notify();
            }
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn new_key_type_label(&self, kind: &str) -> AnyElement {
        let index = TYPES.iter().position(|v| *v == kind).unwrap_or(0);
        div()
            .flex()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .size(px(17.))
                    .rounded_full()
                    .border_2()
                    .border_color(rgb(0xffffff))
                    .bg(rgb(COLORS[index])),
            )
            .child(kind.to_string())
            .into_any_element()
    }

    fn new_key_select(
        &self,
        selector: Selector,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let state = dialog.new_key.as_ref().unwrap();
        let open = state.selector == Some(selector);
        let focused = if selector == Selector::Type {
            state.type_focus.is_focused(window)
        } else {
            dialog.fields["new_db_query"]
                .focus_handle(cx)
                .is_focused(window)
        };
        let dark = self.dark(cx);
        let mut control = div()
            .id(if selector == Selector::Type {
                "new-key-type"
            } else {
                "new-key-db"
            })
            .test_support()
            .role(if selector == Selector::Type {
                Role::ComboBox
            } else {
                Role::EditableComboBox
            })
            .aria_label(dialog.value(
                if selector == Selector::Type {
                    "type"
                } else {
                    "db"
                },
                cx,
            ))
            .aria_expanded(open)
            .h(px(32.))
            .w_full()
            .flex()
            .items_center()
            .relative()
            .border_1()
            .rounded(px(3.))
            .border_color(if open || focused {
                cx.theme().primary_hover
            } else if dark {
                Hsla::transparent_black()
            } else {
                rgb(0xe0e0e6).into()
            })
            .bg::<Hsla>(if dark {
                if focused {
                    rgb(0x3d2d32).into()
                } else {
                    rgba(0xffffff1a).into()
                }
            } else {
                rgb(0xffffff).into()
            })
            .when(open || focused, |control| {
                control.shadow(vec![
                    BoxShadow::new(px(0.), px(0.), cx.theme().primary.opacity(0.2))
                        .spread_radius(px(2.)),
                ])
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, window, cx| {
                    app.open_new_key_selector(selector, window, cx)
                }),
            )
            .capture_key_down(cx.listener(move |app, event, window, cx| {
                app.new_key_selector_key(selector, event, window, cx)
            }));
        if selector == Selector::Type {
            control = control
                .track_focus(&state.type_focus)
                .cursor_pointer()
                .px(px(11.))
                .child(
                    div()
                        .flex_1()
                        .child(self.new_key_type_label(&dialog.value("type", cx))),
                )
                .child(icon("select-chevron").size(px(16.)).text_color(if dark {
                    rgba(0xffffff61)
                } else {
                    rgb(0xc2c2c2)
                }));
        } else {
            control = control
                .child(
                    Styled::h(
                        Input::new(&dialog.fields["new_db_query"])
                            .appearance(false)
                            .bordered(false)
                            .px(px(11.))
                            .text_size(px(14.)),
                        px(30.),
                    )
                    .w_full(),
                )
                .child(div().absolute().right(px(10.)).top(px(7.)).child(
                    icon("select-chevron").size(px(16.)).text_color(if dark {
                        rgba(0xffffff61)
                    } else {
                        rgb(0xc2c2c2)
                    }),
                ));
        }
        let mut popup = gpui_base::Popup::new(
            if selector == Selector::Type {
                "new-key-type-popup"
            } else {
                "new-key-db-popup"
            },
            control,
        )
        .w_full()
        .anchor(Anchor::TopLeft)
        .offset(px(4.));
        if open {
            let options = self.new_key_options(selector, cx);
            let width = if state.selector_bounds.get().size.width == px(0.) {
                529.
            } else {
                state.selector_bounds.get().size.width.as_f32()
            };
            let height = if options.is_empty() {
                93.
            } else {
                (options.len() as f32 * 32. + 8.).min(243.1875)
            };
            let mut menu = div()
                .id("new-key-options")
                .test_support()
                .w(px(width))
                .h(px(height))
                .rounded(px(3.))
                .bg(rgb(if dark { 0x272727 } else { 0xffffff }))
                .shadow(super::super::notices::shadows(dark))
                .occlude()
                .text_color(self.csv_text_color(cx))
                .text_size(px(14.))
                .py(px(4.));
            if options.is_empty() {
                menu = menu
                    .py(px(12.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(icon("empty").size(px(40.)).text_color(if dark {
                        rgb(0x606060)
                    } else {
                        rgb(0xc2c2c2)
                    }))
                    .child(
                        div()
                            .mt(px(8.))
                            .h(px(21.))
                            .line_height(px(21.))
                            .text_color(if dark {
                                rgba(0xffffff61)
                            } else {
                                rgb(0xc2c2c2)
                            })
                            .child(if self.current_language().starts_with("zh") {
                                "无数据"
                            } else {
                                "No Data"
                            }),
                    );
            } else {
                let selected = dialog.value(
                    if selector == Selector::Type {
                        "type"
                    } else {
                        "db"
                    },
                    cx,
                );
                let mut rows = div()
                    .id("new-key-menu-scroll")
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&state.menu_scroll);
                for (index, option) in options.into_iter().enumerate() {
                    let chosen = selected == option;
                    let pending = index == state.pending;
                    let value = option.clone();
                    rows = rows.child(
                        div()
                            .id(("new-key-option", index))
                            .test_support()
                            .role(Role::ListBoxOption)
                            .relative()
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .pl(px(12.))
                            .pr(px(32.))
                            .cursor_pointer()
                            .text_color(if chosen {
                                cx.theme().primary
                            } else {
                                self.csv_text_color(cx)
                            })
                            .when(pending, |row| {
                                row.child(
                                    div()
                                        .absolute()
                                        .left(px(4.))
                                        .right(px(4.))
                                        .top_0()
                                        .bottom_0()
                                        .rounded(px(3.))
                                        .bg::<Hsla>(if dark {
                                            rgba(0xffffff17).into()
                                        } else {
                                            rgb(0xf3f3f5).into()
                                        }),
                                )
                            })
                            .child(div().relative().child(if selector == Selector::Type {
                                self.new_key_type_label(&option)
                            } else {
                                div().child(option).into_any_element()
                            }))
                            .when(chosen, |row| {
                                row.child(
                                    div()
                                        .absolute()
                                        .right(px(8.))
                                        .top(px(9.))
                                        .child(icon("select-check").size(px(16.))),
                                )
                            })
                            .on_hover(cx.listener(move |app, hover, _, cx| {
                                if *hover {
                                    app.dialog
                                        .as_mut()
                                        .unwrap()
                                        .new_key
                                        .as_mut()
                                        .unwrap()
                                        .pending = index;
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.choose_new_key_option(selector, value.clone(), window, cx);
                                cx.stop_propagation();
                            })),
                    );
                }
                menu = menu.child(rows);
            }
            let bounds = state.selector_bounds.clone();
            popup = popup.content(menu.on_mouse_down_out(cx.listener(
                move |app, event: &MouseDownEvent, window, cx| {
                    if !bounds.get().contains(&event.position) {
                        app.close_new_key_selector(window, cx);
                    }
                },
            )));
        }
        let bounds = state.selector_bounds.clone();
        popup
            .on_position(move |_, trigger| {
                if open {
                    bounds.set(trigger);
                }
            })
            .into_any_element()
    }

    fn new_key_field(
        &self,
        key: &str,
        label: &str,
        control: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let error = self
            .dialog
            .as_ref()
            .unwrap()
            .new_key
            .as_ref()
            .unwrap()
            .errors
            .contains(key);
        // Distribute the quarter-pixel CSS labels across the half-pixel GPUI
        // grid, keeping cumulative input positions within one device pixel.
        let label_height = if matches!(key, "db" | "ttl")
            || (key == "rows" && self.dialog.as_ref().unwrap().value("type", cx) == "STREAM")
        {
            21.
        } else {
            21.5
        };
        div()
            .id(SharedString::from(format!("new-key-field-{key}")))
            .test_support()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(label_height))
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
            .child(
                div()
                    .h(px(if error { 19. } else { 18. }))
                    .flex_shrink_0()
                    .text_size(px(14.))
                    .line_height(px(19.))
                    .text_color(self.form_error_color(cx))
                    .when(error, |feedback| {
                        feedback.child(self.tr("dialogue.field_required"))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn new_key_input(&self, key: &str, window: &Window, cx: &Context<Self>) -> Input {
        let dialog = self.dialog.as_ref().unwrap();
        let error = dialog.new_key.as_ref().unwrap().errors.contains(key);
        self.csv_input_style(self.number_input(key, cx), key, window, cx)
            .w_full()
            .when(key.starts_with("new_score."), |field| field.pr(px(7.)))
            .when(error, |field| field.border_color(self.form_error_color(cx)))
    }

    fn new_key_resize_handle(&self, cx: &Context<Self>) -> AnyElement {
        let move_view = cx.weak_entity();
        let up_view = move_view.clone();
        div()
            .absolute()
            .inset_0()
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let weak = move_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.bubble() || !event.dragging() {
                                return;
                            }
                            let Some(view) = weak.upgrade() else {
                                return;
                            };
                            let drag = view
                                .read(cx)
                                .dialog
                                .as_ref()
                                .and_then(|d| d.new_key.as_ref())
                                .and_then(|state| state.resize);
                            if let Some((y, height)) = drag {
                                window.prevent_default();
                                cx.stop_propagation();
                                view.update(cx, |app, cx| {
                                    app.dialog
                                        .as_mut()
                                        .unwrap()
                                        .new_key
                                        .as_mut()
                                        .unwrap()
                                        .value_height =
                                        (height + event.position.y.as_f32() - y).max(32.);
                                    cx.notify();
                                });
                            }
                        });
                        let weak = up_view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if !phase.bubble() || event.button != MouseButton::Left {
                                return;
                            }
                            let Some(view) = weak.upgrade() else {
                                return;
                            };
                            if view
                                .read(cx)
                                .dialog
                                .as_ref()
                                .and_then(|d| d.new_key.as_ref())
                                .is_some_and(|state| state.resize.is_some())
                            {
                                cx.stop_propagation();
                                view.update(cx, |app, cx| {
                                    app.dialog
                                        .as_mut()
                                        .unwrap()
                                        .new_key
                                        .as_mut()
                                        .unwrap()
                                        .resize = None;
                                    cx.notify();
                                });
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .id("new-key-value-resize")
                    .test_support()
                    .absolute()
                    .bottom(px(1.))
                    .right(px(1.))
                    .size(px(12.))
                    .cursor(CursorStyle::ResizeUpDown)
                    .child(
                        icon("textarea-resize")
                            .size(px(12.))
                            .text_color(rgb(0x808080)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|app, event: &MouseDownEvent, window, cx| {
                            let state = app.dialog.as_mut().unwrap().new_key.as_mut().unwrap();
                            state.resize = Some((event.position.y.as_f32(), state.value_height));
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_value_rows(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let kind = dialog.value("type", cx);
        let add = matches!(dialog.kind, DialogKind::AddEntries);
        let mut rows = div().flex().flex_col().gap(px(10.));
        for (index, row) in dialog.rows.iter().enumerate() {
            let mut controls = div()
                .id(("new-key-row", index))
                .test_support()
                .flex()
                .items_center();
            if matches!(kind.as_str(), "HASH" | "STREAM") {
                controls = controls.gap(px(12.)).child(
                    div().flex_1().min_w_0().child(
                        self.new_key_input(&field_key(row), window, cx)
                            .id(("new-key-row-field", index)),
                    ),
                );
            }
            controls = controls.child(
                div().flex_1().min_w_0().child(
                    self.new_key_input(&value_key(row), window, cx)
                        .id(("new-key-row-value", index)),
                ),
            );
            if kind == "ZSET" {
                if add {
                    controls = controls.child(div().w(px(4.015625)).flex_shrink_0().child(":"));
                }
                controls = controls.gap(px(10.)).child(
                    self.new_key_input(&score_key(row), window, cx)
                        .id(("new-key-row-score", index))
                        // The original flex row shrinks the 174px numeric
                        // intrinsic width against the 529px member input.
                        .w(px((529.
                            - 20.
                            - if dialog.rows.len() > 1 { 46. } else { 18. }
                            - if add { 14.015625 } else { 0. })
                            * 174.
                            / 703.))
                        .flex_shrink_0(),
                );
            }
            let mut actions = div().flex().items_center().gap(px(10.)).ml(px(
                if matches!(kind.as_str(), "HASH" | "STREAM") {
                    -2.
                } else if kind == "ZSET" {
                    0.
                } else {
                    10.
                },
            ));
            if dialog.rows.len() > 1 {
                actions = actions.child(
                    self.btn(
                        format!("new-key-remove-{index}"),
                        "",
                        "",
                        cx,
                        move |app, _, cx| app.remove_new_key_row(index, cx),
                    )
                    .ghost()
                    .size(px(18.))
                    .p_0()
                    .child(icon("delete").size(px(18.))),
                );
            }
            actions = actions.child(
                self.btn(
                    format!("new-key-add-{index}"),
                    "",
                    "",
                    cx,
                    move |app, window, cx| app.append_new_key_row(index, window, cx),
                )
                .ghost()
                .size(px(18.))
                .p_0()
                .child(icon("add").size(px(18.))),
            );
            rows = rows.child(controls.child(actions));
        }
        rows.into_any_element()
    }

    pub(super) fn render_new_key(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let state = dialog.new_key.as_ref().unwrap();
        let kind = dialog.value("type", cx);
        let mut footer_content = div().gap(px(6.));
        let scalar = matches!(kind.as_str(), "STRING" | "JSON");
        let content_height = 4. * 71.25
            + if scalar {
                state.value_height + 39.25
            } else {
                71.25 + (dialog.rows.len().saturating_sub(1) as f32 * 42.)
            }
            + if kind == "STREAM" { 71.25 } else { 0. }
            + if state.errors.contains("key") { 1. } else { 0. };
        let form_height = content_height.min(500.);
        let mut form = div()
            .id("new-key-form")
            .test_support()
            .w_full()
            .pr(px(15.))
            .flex()
            .flex_col()
            .child(
                self.new_key_field(
                    "key",
                    &self.tr("common.key"),
                    self.new_key_input("key", window, cx)
                        .id("new-key-key")
                        .into_any_element(),
                    cx,
                ),
            )
            .child(self.new_key_field(
                "db",
                &self.tr("dialogue.key.db_index"),
                self.new_key_select(Selector::Database, window, cx),
                cx,
            ))
            .child(self.new_key_field(
                "type",
                &self.tr("interface.type"),
                self.new_key_select(Selector::Type, window, cx),
                cx,
            ));
        let ttl = div()
            .flex()
            .h(px(32.))
            .child(
                self.new_key_input("ttl", window, cx)
                    .id("new-key-ttl")
                    .w(px(190.90625))
                    .rounded_r(px(0.))
                    .suffix(
                        div()
                            .mr(px(-4.))
                            .text_color(self.csv_text_color(cx))
                            .child(self.tr("common.second")),
                    ),
            )
            .child(
                self.btn(
                    "new-key-forever",
                    self.tr("interface.forever"),
                    "",
                    cx,
                    |app, window, cx| app.set_dialog_field("ttl", "-1", window, cx),
                )
                .w(px(self
                    .modal_label_width(&self.tr("interface.forever"), window)
                    + 24.))
                .px(px(11.))
                .rounded_l(px(0.))
                .border_color(Hsla::transparent_black())
                .text_color(cx.theme().primary)
                .bg(cx.theme().primary.opacity(0.16)),
            );
        form = form.child(self.new_key_field(
            "ttl",
            &self.tr("interface.ttl"),
            ttl.into_any_element(),
            cx,
        ));
        if scalar {
            form = form.child(
                self.new_key_field(
                    "value",
                    &self.tr("common.value"),
                    div()
                        .relative()
                        .child(Styled::h(
                            self.new_key_input("value", window, cx)
                                .id("new-key-value")
                                .h(px(state.value_height)),
                            px(state.value_height),
                        ))
                        .child(self.new_key_resize_handle(cx))
                        .into_any_element(),
                    cx,
                ),
            );
        } else {
            if kind == "STREAM" {
                form = form.child(
                    self.new_key_field(
                        "stream_id",
                        "ID",
                        self.new_key_input("stream_id", window, cx)
                            .id("new-key-stream-id")
                            .into_any_element(),
                        cx,
                    ),
                );
            }
            let rows = self.render_value_rows(window, cx);
            let label = if kind == "ZSET" {
                self.tr("dialogue.field.conflict_handle")
            } else if kind == "STREAM" {
                format!("{}:{}", self.tr("common.field"), self.tr("common.value"))
            } else {
                self.tr("dialogue.field.element")
            };
            form = form.child(self.new_key_field("rows", &label, rows.into_any_element(), cx));
        }
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
                    .h(px(form_height + 119.))
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
                            .id("new-key-scroll")
                            .test_support()
                            .h(px(form_height))
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .track_scroll(&state.scroll)
                            .child(form),
                    )
                    .child(
                        div()
                            .id("new-key-actions")
                            .test_support()
                            .mt(px(16.))
                            .h(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                self.csv_secondary_button(
                                    self.btn(
                                        "new-key-import",
                                        self.tr("interface.import_key"),
                                        "import",
                                        cx,
                                        |app, window, cx| app.import_from_new_key(window, cx),
                                    ),
                                    cx,
                                )
                                .px(px(11.))
                                .w(px(self.modal_label_width(
                                    &self.tr("interface.import_key"),
                                    window,
                                ) + 48.))
                                .content_style(
                                    footer_content.style().clone(),
                                    gpui_component::Size::Size(px(18.)),
                                ),
                            )
                            .child(div().flex_1())
                            .child(
                                self.csv_secondary_button(
                                    self.btn(
                                        "dialog-cancel",
                                        self.tr("common.cancel"),
                                        "",
                                        cx,
                                        |app, window, cx| app.close_dialog(window, cx),
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
                                    |app, window, cx| app.submit_dialog(window, cx),
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

    pub(super) fn new_key_value(&self, cx: &App) -> Json {
        let dialog = self.dialog.as_ref().unwrap();
        dialog
            .new_key
            .as_ref()
            .unwrap()
            .value
            .clone()
            .unwrap_or_else(|| match dialog.value("type", cx).as_str() {
                "STREAM" if matches!(dialog.kind, DialogKind::AddEntries) => {
                    serde_json::json!(["*"])
                }
                "STRING" => Json::String(String::new()),
                "JSON" => Json::String("{}".into()),
                _ => Json::Array(vec![]),
            })
    }

    fn import_from_new_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = self.dialog.as_ref().unwrap();
        let target = dialog.target.clone();
        let db = dialog.value("db", cx);
        self.open_dialog(DialogKind::Import, window, cx);
        if let Some(mut target) = target {
            target.db = db.parse().unwrap_or(0);
            self.set_dialog_field("server", target.conn.name.clone(), window, cx);
            self.set_dialog_field("db", db, window, cx);
            self.dialog.as_mut().unwrap().target = Some(target);
        }
    }

    pub(super) fn submit_new_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let dialog = self.dialog.as_ref().unwrap();
        let key = dialog.value("key", cx);
        if key.is_empty() {
            self.dialog
                .as_mut()
                .unwrap()
                .new_key
                .as_mut()
                .unwrap()
                .errors
                .insert("key".into());
            self.toast(self.tr("dialogue.field_required"), true);
            return Ok(());
        }
        let target = dialog
            .target
            .clone()
            .ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
        let kind = KeyType::parse(&dialog.value("type", cx));
        let db = dialog.value("db", cx).parse::<u32>()?;
        let ttl_value = dialog.numbers.get("ttl").copied().flatten();
        if let Some(ttl) = ttl_value
            && ttl.fract() != 0.
        {
            anyhow::bail!(
                "json: cannot unmarshal number {} into Go struct field SetKeyParam.ttl of type int64",
                numbers::display(Some(ttl))
            );
        }
        let ttl = ttl_value.unwrap_or(0.) as i64;
        let value = self.new_key_value(cx);
        let dialog_id = dialog.id;
        let saved_kind = kind.clone();
        self.job(
            window,
            cx,
            move |service| {
                service
                    .create_key(&target.conn, db, key.as_bytes(), kind, &value, ttl)
                    .map(|_| (target.id, db, key))
            },
            move |app, (id, db, key), window, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].db == db
                {
                    // The original inserts this node directly, including when
                    // a filter would exclude it; a full scan would lose it.
                    let bytes = key.into_bytes();
                    app.tabs[i].key_types.remember(bytes.clone(), saved_kind.clone());
                    if !app.tabs[i].keys.iter().any(|entry| entry.key == bytes) {
                        app.tabs[i].keys.push(KeyEntry {
                            key: bytes.clone(),
                            kind: saved_kind.clone(),
                        });
                        app.tabs[i].keys.sort_by(|a, b| {
                            natural_compare(&display_bytes(&a.key), &display_bytes(&b.key))
                        });
                        app.tabs[i].total += 1;
                        if let Some(database) = app.tabs[i]
                            .databases
                            .iter_mut()
                            .find(|database| database.index == db)
                        {
                            database.keys += 1;
                        }
                    }
                    let current = app.tabs[i].current_key.as_ref() == Some(&bytes);
                    app.select_key(id, bytes, window, cx);
                    if current {
                        app.reload_detail(id, true, window, cx);
                    }
                }
                if app.dialog.as_ref().is_some_and(|d| d.id == dialog_id) {
                    app.dialog = None;
                }
            },
        );
        Ok(())
    }
}
