use super::key_browser::database_label;
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use std::{cell::Cell, rc::Rc};

pub(super) struct DatabasePicker {
    pub query: Entity<InputState>,
    pub open: bool,
    pub cursor: Option<u32>,
    pub scroll: ScrollHandle,
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    label: String,
    palette: Option<bool>,
    token: Option<gpui_base::DeferredPopover>,
}
impl DatabasePicker {
    pub fn new(query: Entity<InputState>) -> Self {
        Self {
            query,
            open: false,
            cursor: None,
            scroll: ScrollHandle::default(),
            bounds: Rc::new(Cell::new(Bounds::default())),
            label: String::new(),
            palette: None,
            token: None,
        }
    }
}
#[derive(Default)]
pub(super) struct TypePicker {
    pub open: bool,
    pub hover_epoch: u64,
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    menu_bounds: Rc<Cell<Bounds<Pixels>>>,
    token: Option<gpui_base::DeferredPopover>,
}
const TYPES: [&str; 8] = [
    "", "STRING", "HASH", "LIST", "SET", "ZSET", "STREAM", "JSON",
];
const DATABASE_MENU_MAX_HEIGHT: f32 = 243.1875;

impl RdmApp {
    pub(super) fn prepare_key_selectors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.index() else {
            return;
        };
        let label = database_label(&self.tabs[i], self.tabs[i].db);
        let dark = self.dark(cx);
        let opened = self.tabs[i].database_picker.open;
        let picker = &mut self.tabs[i].database_picker;
        if picker.label != label || picker.palette != Some(dark) {
            picker.label = label.clone();
            picker.palette = Some(dark);
            picker.query.update(cx, |input, cx| {
                input.set_placeholder(label, window, cx);
                input.set_placeholder_colors(
                    if opened {
                        rgb(0xc2c2c2).into()
                    } else {
                        rgb(0x333639).into()
                    },
                    if opened {
                        rgba(0xffffff61).into()
                    } else {
                        rgba(0xffffffd1).into()
                    },
                    cx,
                );
            });
        }
    }
    fn database_options(&self, id: u64, cx: &App) -> Vec<(u32, String)> {
        let Some(i) = self.tab_index(id) else {
            return Vec::new();
        };
        let tab = &self.tabs[i];
        let query = text_input(&tab.database_picker.query, cx);
        tab.databases
            .iter()
            .filter(|db| query.is_empty() || db.index.to_string() == query)
            .map(|db| (db.index, database_label(tab, db.index)))
            .collect()
    }
    fn open_database_picker(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let picker = &mut self.tabs[i].database_picker;
        if !picker.open {
            picker.open = true;
            picker.token = Some(gpui_base::GlobalState::register_deferred_popover(cx));
            picker.query.update(cx, |input, cx| {
                input.set_placeholder_colors(rgb(0xc2c2c2).into(), rgba(0xffffff61).into(), cx)
            });
        }
        picker.query.update(cx, |input, cx| input.focus(window, cx));
        let options = self.database_options(id, cx);
        let db = self.tabs[i].db;
        self.tabs[i].database_picker.cursor = options
            .iter()
            .find(|(index, _)| *index == db)
            .or_else(|| options.first())
            .map(|(index, _)| *index);
        self.reveal_database_cursor(id, cx);
        cx.notify();
    }
    pub(super) fn close_database_picker(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let picker = &mut self.tabs[i].database_picker;
        picker.open = false;
        picker.token = None;
        picker.cursor = None;
        set_input(&picker.query, "", window, cx);
        picker.query.update(cx, |input, cx| {
            input.set_placeholder_colors(rgb(0x333639).into(), rgba(0xffffffd1).into(), cx)
        });
        cx.notify();
    }
    pub(super) fn database_query_changed(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        if !self.tabs[i].database_picker.open
            && text_input(&self.tabs[i].database_picker.query, cx).is_empty()
        {
            return;
        }
        if !self.tabs[i].database_picker.open {
            self.open_database_picker(id, window, cx);
        }
        let options = self.database_options(id, cx);
        self.tabs[i].database_picker.cursor = options.first().map(|(index, _)| *index);
        self.tabs[i]
            .database_picker
            .scroll
            .set_offset(point(px(0.), px(0.)));
        cx.notify();
    }
    fn choose_database(&mut self, id: u64, db: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.close_database_picker(id, window, cx);
        if self.tab_index(id).is_some_and(|i| self.tabs[i].db != db) {
            self.change_database(id, db, window, cx);
        }
    }
    fn database_picker_key(
        &mut self,
        id: u64,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if !matches!(key, "up" | "down" | "enter" | "escape") {
            return;
        }
        if key == "escape" {
            self.close_database_picker(id, window, cx);
        } else if !self.tabs[i].database_picker.open {
            self.open_database_picker(id, window, cx);
        } else {
            let options = self.database_options(id, cx);
            let index = options
                .iter()
                .position(|(db, _)| Some(*db) == self.tabs[i].database_picker.cursor);
            match key {
                "enter" => {
                    if let Some(db) = self.tabs[i].database_picker.cursor {
                        self.choose_database(id, db, window, cx);
                    }
                }
                "down" => {
                    let next = index.map_or(0, |index| (index + 1) % options.len().max(1));
                    self.tabs[i].database_picker.cursor = options.get(next).map(|(db, _)| *db);
                }
                "up" => {
                    let next = index.map_or(options.len().saturating_sub(1), |index| {
                        if index == 0 {
                            options.len().saturating_sub(1)
                        } else {
                            index - 1
                        }
                    });
                    self.tabs[i].database_picker.cursor = options.get(next).map(|(db, _)| *db);
                }
                _ => {}
            }
            self.reveal_database_cursor(id, cx);
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    fn reveal_database_cursor(&self, id: u64, cx: &App) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let picker = &self.tabs[i].database_picker;
        let options = self.database_options(id, cx);
        let Some(index) = options
            .iter()
            .position(|(db, _)| Some(*db) == picker.cursor)
        else {
            return;
        };
        let viewport = (options.len() as f32 * 32.).min(DATABASE_MENU_MAX_HEIGHT - 8.);
        let top = -picker.scroll.offset().y.as_f32();
        let row = index as f32 * 32.;
        let target = if row < top {
            row
        } else if row + 32. > top + viewport {
            row + 32. - viewport
        } else {
            top
        };
        picker.scroll.set_offset(point(px(0.), px(-target)));
    }
    pub(super) fn type_picker_hover(
        &mut self,
        id: u64,
        hover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let picker = &mut self.tabs[i].type_picker;
        picker.hover_epoch += 1;
        let epoch = picker.hover_epoch;
        if hover {
            picker.open = true;
            picker
                .token
                .get_or_insert_with(|| gpui_base::GlobalState::register_deferred_popover(cx));
            cx.notify();
        } else {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let _ = this.update_in(cx, |app, window, cx| {
                    if let Some(i) = app.tab_index(id)
                        && app.tabs[i].type_picker.hover_epoch == epoch
                        && !app.tabs[i]
                            .type_picker
                            .bounds
                            .get()
                            .contains(&window.mouse_position())
                        && !app.tabs[i]
                            .type_picker
                            .menu_bounds
                            .get()
                            .contains(&window.mouse_position())
                    {
                        app.tabs[i].type_picker.open = false;
                        app.tabs[i].type_picker.token = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
    pub(super) fn close_type_picker(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(i) = self.tab_index(id) {
            let picker = &mut self.tabs[i].type_picker;
            picker.open = false;
            picker.token = None;
            picker.hover_epoch += 1;
            cx.notify();
        }
    }
    fn choose_key_type(
        &mut self,
        id: u64,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let changed = self.tabs[i].key_type != kind;
        self.tabs[i].key_type = kind.to_string();
        self.close_type_picker(id, cx);
        if changed {
            self.scan_keys(id, true, false, window, cx);
        }
    }
    fn selector_surface(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0x272727 } else { 0xffffff }).into()
    }
    fn selector_text(&self, cx: &App) -> Hsla {
        if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        }
    }
    pub(super) fn render_type_selector(
        &self,
        id: u64,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> (AnyElement, f32) {
        let tab = &self.tabs[self.tab_index(id).unwrap()];
        let kind = KeyType::parse(&tab.key_type);
        let (fg, bg) = if tab.key_type.is_empty() {
            (
                self.selector_text(cx),
                if self.dark(cx) {
                    Hsla::transparent_black()
                } else {
                    rgb(0xfafafc).into()
                },
            )
        } else {
            let (fg, bg) = kind.colors();
            (rgb(fg).into(), rgb(bg).into())
        };
        let enabled = info_value(&tab.info, "Server", "redis_version")
            .split('.')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|v| v > 5);
        let label = if tab.key_type.is_empty() {
            "A"
        } else {
            kind.short()
        };
        // The original medium, strong NTag uses 14px/500 text and 10px
        // horizontal padding. Measure before the text element's intrinsic
        // width is rounded up, so the adjacent input keeps that fraction.
        let mut font = self.preference_font("general", ".SystemUIFont", cx);
        font.weight = FontWeight::MEDIUM;
        let width = window
            .text_system()
            .shape_line(
                label.to_owned().into(),
                px(14.),
                &[TextRun {
                    len: label.len(),
                    font,
                    color: fg,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
            .as_f32()
            + 20.;
        let button = Button::new("type-filter")
            .outline()
            .label(label.to_string())
            .with_size(gpui_component::Size::Size(px(26.6667)))
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(28.))
            .px(px(9.))
            .rounded_l(px(4.))
            .rounded_r(px(0.))
            .content_style(
                StyleRefinement::default().text_size(px(14.)),
                gpui_component::Size::Size(px(14.)),
            )
            .font_weight(FontWeight::MEDIUM)
            .border_color(cx.theme().border)
            .bg(bg)
            .text_color(fg)
            .disabled(!enabled)
            .flex_shrink_0()
            .on_hover(cx.listener(move |app, hover, w, cx| {
                if enabled {
                    app.type_picker_hover(id, *hover, w, cx);
                }
            }))
            .on_click(cx.listener(move |app, _, w, cx| {
                if enabled {
                    app.type_picker_hover(id, true, w, cx);
                }
            }));
        let trigger = div()
            .relative()
            .h(px(28.))
            // Derive the width during layout. Authored px widths and measured
            // leaves are snapped early by GPUI, losing the original fraction.
            .aspect_ratio(width / 28.)
            .flex_shrink_0()
            .child(button);
        let bounds = tab.type_picker.bounds.clone();
        let mut popup = gpui_base::Popup::new("type-selector-popup", trigger)
            .offset(px(10.))
            .on_position(move |_, trigger| bounds.set(trigger));
        if tab.type_picker.open && enabled {
            let font = self.preference_font("general", ".SystemUIFont", cx);
            let mut width = 140.0625f32;
            for value in TYPES {
                let title = if value.is_empty() { "ALL" } else { value };
                let mut font = font.clone();
                if value == tab.key_type {
                    font.weight = FontWeight::BOLD;
                }
                let run = TextRun {
                    len: title.len(),
                    font,
                    color: fg,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                width = width.max(
                    window
                        .text_system()
                        .shape_line(title.into(), px(14.), &[run], None)
                        .width
                        .as_f32()
                        + 84.,
                );
            }
            let menu_bounds = tab.type_picker.menu_bounds.clone();
            let mut menu = div()
                .id("key-type-options")
                .test_support()
                .occlude()
                .on_prepaint(move |bounds, _, _| menu_bounds.set(bounds))
                .relative()
                .w(px(width))
                .py(px(6.))
                .px(px(2.))
                .bg(self.selector_surface(cx))
                .text_size(px(14.))
                .text_color(self.selector_text(cx))
                .rounded(px(3.))
                .shadow(super::notices::shadows(self.dark(cx)))
                .on_hover(
                    cx.listener(move |app, hover, w, cx| app.type_picker_hover(id, *hover, w, cx)),
                )
                .child(
                    icon("selector-arrow")
                        .size(px(14.))
                        .h(px(6.))
                        .absolute()
                        .top(px(-6.))
                        .left(px(8.))
                        .text_color(self.selector_surface(cx)),
                );
            for (index, value) in TYPES.into_iter().enumerate() {
                let chosen = value == tab.key_type;
                let typ = KeyType::parse(value);
                let (color, background) = typ.colors();
                let tag = if value.is_empty() {
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .rounded(px(4.))
                        .border_1()
                        .border_color(rgba(0xddddddcc))
                        .bg(if self.dark(cx) {
                            Hsla::transparent_black()
                        } else {
                            rgb(0xfafafc).into()
                        })
                        .text_color(self.selector_text(cx))
                        .child("A")
                        .into_any_element()
                } else {
                    super::views::type_mark(&typ, true, true, chosen)
                };
                menu =
                    menu.child(
                        div()
                            .id(("key-type-option", index))
                            .test_support()
                            .role(Role::MenuItem)
                            .aria_label(if value.is_empty() { "ALL" } else { value })
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .rounded(px(3.))
                            .hover(|style| style.bg(self.connection_hover_color(cx)))
                            .on_click(cx.listener(move |app, _, w, cx| {
                                app.choose_key_type(id, value, w, cx);
                                cx.stop_propagation();
                            }))
                            .child(
                                div()
                                    .id(("key-type-prefix", index))
                                    .test_support()
                                    .w(px(36.))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .child(tag),
                            )
                            .child(
                                div()
                                    .id(("key-type-label", index))
                                    .test_support()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .font_weight(if chosen {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(if value.is_empty() { "ALL" } else { value })
                                    .child(div().w(px(16.)).flex_shrink_0().when(
                                        chosen && !value.is_empty(),
                                        |slot| {
                                            slot.child(
                                                div().size(px(14.)).rounded_full().bg(rgb(
                                                    if chosen { color } else { background },
                                                )),
                                            )
                                        },
                                    )),
                            )
                            .child(div().w(px(16.)).h_full().flex_shrink_0()),
                    );
            }
            let bounds = tab.type_picker.bounds.clone();
            popup = popup.content(menu.on_mouse_down_out(cx.listener(
                move |app, event: &MouseDownEvent, _, cx| {
                    if !bounds.get().contains(&event.position) {
                        app.close_type_picker(id, cx);
                    }
                },
            )));
        }
        (popup.into_any_element(), width)
    }
    pub(super) fn render_database_selector(
        &self,
        id: u64,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[self.tab_index(id).unwrap()];
        let picker = &tab.database_picker;
        let caret = self
            .connection_icon_button(
                "database-caret",
                "down",
                String::new(),
                16.,
                cx,
                move |app, w, cx| {
                    if app
                        .tab_index(id)
                        .is_some_and(|i| app.tabs[i].database_picker.open)
                    {
                        app.close_database_picker(id, w, cx);
                    } else {
                        app.open_database_picker(id, w, cx);
                    }
                },
            )
            .icon(icon("down").size(px(16.)))
            .size(px(16.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let measured_bounds = picker.bounds.clone();
        let control = div()
            .id("database-select")
            .test_support()
            .role(Role::EditableComboBox)
            .aria_label(picker.label.clone())
            .aria_expanded(picker.open)
            .on_prepaint(move |bounds, _, _| measured_bounds.set(bounds))
            .track_focus(&picker.query.focus_handle(cx))
            .h(px(28.))
            .w_full()
            .relative()
            .rounded(px(3.))
            .border_1()
            .border_color(
                if picker.open || picker.query.focus_handle(cx).is_focused(window) {
                    rgb(0xd33a31).into()
                } else {
                    cx.theme().input
                },
            )
            .bg::<Hsla>(if self.dark(cx) {
                rgba(0xffffff1a).into()
            } else {
                rgb(0xffffff).into()
            })
            .text_size(px(14.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, w, cx| app.open_database_picker(id, w, cx)),
            )
            .capture_key_down(
                cx.listener(move |app, event, w, cx| app.database_picker_key(id, event, w, cx)),
            )
            .child(Styled::h(
                Input::new(&picker.query)
                    .appearance(false)
                    .bordered(false)
                    .px(px(11.))
                    .text_size(px(14.))
                    .suffix(
                        div()
                            .mr(px(8.) - gpui_component::Size::Medium.input_px())
                            .child(caret),
                    ),
                px(26.),
            ));
        let bounds = picker.bounds.clone();
        let mut popup = gpui_base::Popup::new("database-selector-popup", control)
            .w_full()
            .anchor(Anchor::BottomLeft)
            .offset(px(4.))
            .on_position(move |_, trigger| bounds.set(trigger));
        if picker.open {
            let options = self.database_options(id, cx);
            let all = tab
                .databases
                .iter()
                .map(|db| (db.index, database_label(tab, db.index)))
                .collect::<Vec<_>>();
            let measured_width = picker.bounds.get().size.width.as_f32();
            let trigger_width = if measured_width == 0. {
                (self.pref.behavior.aside_width - 138.).clamp(100., 200.)
            } else {
                measured_width
            };
            let mut width = trigger_width;
            for (_, label) in &all {
                let run = TextRun {
                    len: label.len(),
                    font: self.preference_font("general", ".SystemUIFont", cx),
                    color: self.selector_text(cx),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                width = width.max(
                    window
                        .text_system()
                        .shape_line(label.clone().into(), px(14.), &[run], None)
                        .width
                        .as_f32()
                        + 44.,
                );
            }
            let height = if options.is_empty() {
                93.
            } else {
                (options.len() as f32 * 32. + 8.).min(DATABASE_MENU_MAX_HEIGHT)
            };
            let mut menu = div()
                .id("key-database-options")
                .test_support()
                .w(px(width))
                .h(px(height))
                .py(px(if options.is_empty() { 12. } else { 4. }))
                .rounded(px(3.))
                .bg(self.selector_surface(cx))
                .text_size(px(14.))
                .text_color(self.selector_text(cx))
                .shadow(super::notices::shadows(self.dark(cx)));
            if options.is_empty() {
                menu = menu
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(icon("empty").size(px(40.)).text_color(if self.dark(cx) {
                        rgb(0x606060)
                    } else {
                        rgb(0xc2c2c2)
                    }))
                    .child(
                        div()
                            .mt(px(8.))
                            .h(px(21.))
                            .line_height(px(21.))
                            .text_color(if self.dark(cx) {
                                Hsla::from(rgba(0xffffff61))
                            } else {
                                Hsla::from(rgb(0xc2c2c2))
                            })
                            .child(if self.current_language().starts_with("zh") {
                                "无数据"
                            } else {
                                "No Data"
                            }),
                    );
            } else {
                let mut rows = div()
                    .id("key-database-scroll")
                    .test_support()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&picker.scroll);
                for (index, label) in options {
                    let chosen = index == tab.db;
                    let pending = Some(index) == picker.cursor;
                    rows = rows.child(
                        div()
                            .id(("key-database-option", index as usize))
                            .test_support()
                            .role(Role::ListBoxOption)
                            .aria_label(label.clone())
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .pl(px(12.))
                            .pr(px(12.))
                            .cursor_pointer()
                            .when(pending, |row| row.bg(self.connection_hover_color(cx)))
                            .text_color(if chosen {
                                rgb(0xd33a31).into()
                            } else {
                                self.selector_text(cx)
                            })
                            .on_hover(cx.listener(move |app, hover, _, cx| {
                                if *hover && let Some(i) = app.tab_index(id) {
                                    app.tabs[i].database_picker.cursor = Some(index);
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |app, _, w, cx| {
                                app.choose_database(id, index, w, cx);
                                cx.stop_propagation();
                            }))
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .child(div().w(px(20.)).flex_shrink_0().when(chosen, |slot| {
                                slot.child(icon("key-checkbox-check").size(px(16.)))
                            })),
                    );
                }
                menu = menu.child(rows);
            }
            let bounds = picker.bounds.clone();
            popup = popup.content(menu.on_mouse_down_out(cx.listener(
                move |app, event: &MouseDownEvent, w, cx| {
                    if !bounds.get().contains(&event.position) {
                        app.close_database_picker(id, w, cx);
                    }
                },
            )));
        }
        div()
            .w(px(200.))
            .min_w(px(100.))
            .max_w(px(200.))
            .flex_shrink_1()
            .child(popup)
            .into_any_element()
    }
}
