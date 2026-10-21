//! The original TtlInput uses an unsearchable Naive UI select, not a menu.
use super::*;
use std::{cell::Cell, rc::Rc};

const UNITS: [(u32, &str); 4] = [
    (1, "common.second"),
    (60, "common.minute"),
    (3600, "common.hour"),
    (86400, "common.day"),
];
pub(super) struct UnitPicker {
    pub(super) open: bool,
    pub(super) pending: usize,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    token: Option<gpui_base::DeferredPopover>,
}
impl UnitPicker {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            open: false,
            pending: 0,
            focus: cx.focus_handle().tab_stop(true),
            bounds: Rc::new(Cell::new(Bounds::default())),
            token: None,
        }
    }
}
impl RdmApp {
    fn csv_unit_index(&self, cx: &App) -> usize {
        let value = self
            .dialog
            .as_ref()
            .map(|d| d.value("unit", cx).parse::<u32>().unwrap_or(1))
            .unwrap_or(1);
        UNITS
            .iter()
            .position(|(unit, _)| *unit == value)
            .unwrap_or(0)
    }
    pub(super) fn open_csv_unit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.csv_unit_index(cx);
        if let Some(dialog) = &mut self.dialog {
            let picker = &mut dialog.csv_unit_picker;
            picker.open = true;
            picker.pending = selected;
            picker
                .token
                .get_or_insert_with(|| gpui_base::GlobalState::register_deferred_popover(cx));
            picker.focus.focus(window, cx);
            cx.notify();
        }
    }
    fn close_csv_unit(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.dialog {
            dialog.csv_unit_picker.open = false;
            dialog.csv_unit_picker.token = None;
            cx.notify();
        }
    }
    fn choose_csv_unit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.set_dialog_field("unit", UNITS[index].0.to_string(), window, cx);
        self.close_csv_unit(cx);
    }
    fn csv_unit_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let open = dialog.csv_unit_picker.open;
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                if !open {
                    // Naive Select only opens on ArrowDown; ArrowUp while
                    // closed is consumed without changing the form or value.
                    if event.keystroke.key == "down" {
                        self.open_csv_unit(window, cx);
                    }
                } else {
                    let picker = &mut self.dialog.as_mut().unwrap().csv_unit_picker;
                    picker.pending = if event.keystroke.key == "down" {
                        (picker.pending + 1) % 4
                    } else {
                        (picker.pending + 3) % 4
                    };
                    cx.notify();
                }
            }
            "enter" | "space" => {
                if open {
                    self.choose_csv_unit(dialog.csv_unit_picker.pending, window, cx);
                } else {
                    self.open_csv_unit(window, cx);
                }
            }
            "escape" => {
                self.close_dialog(window, cx);
            }
            "tab" => {
                self.close_csv_unit(cx);
                return;
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }
    pub(super) fn render_csv_unit(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let picker = &self.dialog.as_ref().unwrap().csv_unit_picker;
        let selected = self.csv_unit_index(cx);
        let focused = picker.focus.is_focused(window);
        let bounds = picker.bounds.clone();
        let dark = self.dark(cx);
        let text: Hsla = if dark {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let control = div()
            .id("csv-unit")
            .test_support()
            .role(Role::ComboBox)
            .aria_label(self.tr(UNITS[selected].1))
            .aria_expanded(picker.open)
            .track_focus(&picker.focus)
            .on_prepaint(move |area, _, _| bounds.set(area))
            .w(px(150.))
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .pl(px(11.))
            .pr(px(9.))
            .border_1()
            .rounded(px(3.))
            .rounded_l(px(0.))
            .cursor_pointer()
            .text_size(px(14.))
            .line_height(px(21.))
            .font_weight(FontWeight::NORMAL)
            .text_color(text)
            .bg::<Hsla>(if dark {
                if focused || picker.open {
                    // Keep the shadow under the active fill from tinting it twice.
                    rgb(0x3d2d32).into()
                } else {
                    rgba(0xffffff1a).into()
                }
            } else {
                rgb(0xffffff).into()
            })
            .border_color(if focused || picker.open {
                rgb(0xd33a31).into()
            } else if dark {
                Hsla::transparent_black()
            } else {
                rgb(0xe0e0e6).into()
            })
            .when(!focused && !picker.open, |control| {
                control.hover(|control| control.border_color(rgb(0xff6b6b)))
            })
            .when(focused || picker.open, |control| {
                control.shadow(vec![
                    BoxShadow::new(
                        px(0.),
                        px(0.),
                        Hsla::from(rgb(0xd33a31)).opacity(if dark { 0.4 } else { 0.2 }),
                    )
                    .blur_radius(px(if dark { 8. } else { 0. }))
                    .spread_radius(px(if dark { 0. } else { 2. })),
                ])
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    if app.dialog.as_ref().is_some_and(|d| d.csv_unit_picker.open) {
                        app.close_csv_unit(cx);
                    } else {
                        app.open_csv_unit(window, cx);
                    }
                }),
            )
            .capture_key_down(
                cx.listener(|app, event, window, cx| app.csv_unit_key(event, window, cx)),
            )
            .child(div().flex_1().min_w_0().child(self.tr(UNITS[selected].1)))
            .child(icon("select-chevron").size(px(14.)).text_color(if dark {
                rgba(0xffffff61)
            } else {
                rgb(0xc2c2c2)
            }));
        let mut popup = gpui_base::Popup::new("csv-unit-popup", control)
            .offset(px(4.))
            .w(px(150.))
            .flex_shrink_0()
            .ml(px(-1.));
        if picker.open {
            let mut menu = div()
                .id("csv-unit-options")
                .test_support()
                .role(Role::ListBox)
                .w(px(150.))
                .h(px(136.))
                .py(px(4.))
                .rounded(px(4.))
                .bg(rgb(if dark { 0x48484e } else { 0xffffff }))
                .text_color(text)
                .text_size(px(14.))
                .line_height(px(21.))
                .font_weight(FontWeight::NORMAL)
                .shadow(crate::ui::notices::shadows(dark));
            for (index, (_, label)) in UNITS.iter().enumerate() {
                let chosen = selected == index;
                menu = menu.child(
                    div()
                        .id(("csv-unit-option", index))
                        .test_support()
                        .role(Role::ListBoxOption)
                        .aria_label(self.tr(label))
                        .relative()
                        .h(px(32.))
                        .flex()
                        .items_center()
                        .pl(px(12.))
                        .pr(px(32.))
                        .cursor_pointer()
                        .when(picker.pending == index, |row| {
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
                        .text_color(if chosen { rgb(0xd33a31).into() } else { text })
                        .child(div().relative().child(self.tr(label)))
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
                            if *hover && let Some(dialog) = &mut app.dialog {
                                dialog.csv_unit_picker.pending = index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.choose_csv_unit(index, window, cx);
                            cx.stop_propagation();
                        })),
                );
            }
            let bounds = picker.bounds.clone();
            popup = popup.content(menu.on_mouse_down_out(cx.listener(
                move |app, event: &MouseDownEvent, _, cx| {
                    if !bounds.get().contains(&event.position) {
                        app.close_csv_unit(cx);
                    }
                },
            )));
        }
        popup.into_any_element()
    }
}
