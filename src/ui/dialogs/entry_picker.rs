use super::*;
use crate::ui::text::ScriptLabel;
use crate::ui::value_footer::{FooterPicker, PickerHost, PickerKind};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub(super) struct EntryPickerState {
    pub(super) open: Option<FooterPicker>,
    pub(super) hover: Option<PickerKind>,
    pub(super) tooltip: Option<PickerKind>,
    epoch: u64,
    tooltip_bounds: Rc<Cell<Bounds<Pixels>>>,
    arrow_left: Rc<Cell<Pixels>>,
    triggers: [Rc<Cell<Bounds<Pixels>>>; 2],
    cursors: [Option<usize>; 2],
}

impl RdmApp {
    pub(in crate::ui) fn dismiss_entry_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let id = self
            .dialog
            .as_ref()
            .filter(|dialog| dialog.entry_picker.open.is_some())
            .map(|dialog| dialog.id);
        if let Some(id) = id {
            self.close_entry_picker(id, window, cx);
            true
        } else {
            false
        }
    }
    pub(in crate::ui) fn entry_picker_open(&self, id: u64) -> Option<&FooterPicker> {
        self.dialog
            .as_ref()
            .filter(|dialog| dialog.id == id)
            .and_then(|dialog| dialog.entry_picker.open.as_ref())
    }

    pub(in crate::ui) fn close_entry_picker(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(picker) = self
            .dialog
            .as_mut()
            .filter(|dialog| dialog.id == id)
            .and_then(|dialog| {
                let picker = dialog.entry_picker.open.take()?;
                let index = if picker.kind == PickerKind::Format {
                    0
                } else {
                    1
                };
                dialog.entry_picker.cursors[index] = picker.cursor;
                Some(picker)
            })
        {
            picker.restore_focus(&self.focus, window, cx);
            cx.notify();
        }
    }

    fn toggle_entry_picker(
        &mut self,
        id: u64,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same = self
            .entry_picker_open(id)
            .is_some_and(|picker| picker.kind == kind);
        self.close_entry_picker(id, window, cx);
        if let Some(dialog) = &mut self.dialog
            && dialog.id == id
        {
            let state = &mut dialog.entry_picker;
            state.tooltip = None;
            state.hover = None;
            state.epoch += 1;
            if !same {
                let mut picker = FooterPicker::new(kind, window, cx);
                picker.cursor = state.cursors[if kind == PickerKind::Format { 0 } else { 1 }];
                state.open = Some(picker);
            }
            cx.notify();
        }
    }

    pub(in crate::ui) fn entry_picker_cursor(
        &mut self,
        id: u64,
        index: usize,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = &mut self.dialog
            && dialog.id == id
            && let Some(picker) = &mut dialog.entry_picker.open
        {
            if hovered {
                picker.cursor = Some(index);
            } else if !window.last_input_was_keyboard() && picker.cursor == Some(index) {
                picker.cursor = None;
            }
            cx.notify();
        }
    }

    pub(in crate::ui) fn choose_entry_option(
        &mut self,
        id: u64,
        kind: PickerKind,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dialog.as_ref().is_some_and(|dialog| dialog.id == id) {
            return;
        }
        let options = self.footer_options(kind);
        let Some(Some(value)) = options.get(index) else {
            return;
        };
        self.close_entry_picker(id, window, cx);
        if kind == PickerKind::Decode && index == options.len() - 1 {
            self.open_decoder_settings(window, cx);
        } else {
            self.change_entry_format(
                if kind == PickerKind::Format {
                    "format"
                } else {
                    "decode"
                },
                value.clone(),
                window,
                cx,
            );
        }
    }

    pub(in crate::ui) fn entry_picker_key(
        &mut self,
        id: u64,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.entry_picker_open(id) else {
            return;
        };
        let kind = picker.kind;
        let cursor = picker.cursor;
        let options = self.footer_options(kind);
        let indices = options
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.as_ref().map(|_| index))
            .collect::<Vec<_>>();
        match event.keystroke.key.as_str() {
            "up" | "down" => {
                let current =
                    cursor.and_then(|cursor| indices.iter().position(|index| *index == cursor));
                let next = if event.keystroke.key == "down" {
                    current.map_or(0, |current| (current + 1) % indices.len())
                } else {
                    current.map_or(indices.len() - 1, |current| {
                        (current + indices.len() - 1) % indices.len()
                    })
                };
                self.dialog
                    .as_mut()
                    .unwrap()
                    .entry_picker
                    .open
                    .as_mut()
                    .unwrap()
                    .cursor = Some(indices[next]);
            }
            "enter" => {
                if let Some(cursor) = cursor {
                    self.choose_entry_option(id, kind, cursor, window, cx);
                }
            }
            "escape" => self.close_entry_picker(id, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn entry_picker_hover(
        &mut self,
        id: u64,
        kind: PickerKind,
        entered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        if dialog.id != id {
            return;
        }
        let state = &mut dialog.entry_picker;
        state.epoch += 1;
        let epoch = state.epoch;
        if entered {
            state.hover = Some(kind);
        } else if state.hover == Some(kind) {
            state.hover = None;
        }
        if state.open.is_some() {
            state.tooltip = None;
            cx.notify();
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                if let Some(dialog) = &mut app.dialog
                    && dialog.id == id
                    && dialog.entry_picker.epoch == epoch
                    && dialog.entry_picker.open.is_none()
                {
                    let state = &mut dialog.entry_picker;
                    if state.hover.is_some()
                        || !state
                            .tooltip_bounds
                            .get()
                            .contains(&window.mouse_position())
                    {
                        state.tooltip = state.hover;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn render_entry_picker(
        &self,
        kind: PickerKind,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dialog = self.displayed_entry_dialog().unwrap();
        let id = dialog.id;
        let name = if kind == PickerKind::Format {
            "format"
        } else {
            "decode"
        };
        let value = dialog.value(name, cx);
        let label = if value.is_empty() || value == "Auto" {
            if kind == PickerKind::Format {
                "Raw"
            } else {
                "None"
            }
        } else {
            &value
        };
        let width = self.footer_text_width_at(label, 14., window, cx) + 48.;
        let picker = dialog
            .entry_picker
            .open
            .as_ref()
            .filter(|picker| picker.kind == kind);
        let hover: Hsla = if self.dark(cx) {
            rgba(0xffffff1f).into()
        } else {
            rgba(0x2e333817).into()
        };
        let trigger_bounds =
            dialog.entry_picker.triggers[if kind == PickerKind::Format { 0 } else { 1 }].clone();
        let measured_trigger = trigger_bounds.clone();
        let button = div()
            .id(format!("entry-select-{name}"))
            .test_support()
            .on_prepaint(move |mut bounds, window, _| {
                // Div reports its content box; the follower anchors to the
                // complete control, including its symmetric horizontal padding.
                bounds.origin.x -= px(12.);
                bounds.size.width += px(24.);
                if measured_trigger.get() != bounds {
                    measured_trigger.set(bounds);
                    window.refresh();
                }
            })
            .role(Role::Button)
            .aria_label(label.to_owned())
            .w(px(width))
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(12.))
            .gap(px(6.))
            .rounded(px(3.))
            .text_size(px(14.))
            .line_height(px(14.))
            .text_color(self.entry_panel_text(cx))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_hover(cx.listener(move |app, entered, w, cx| {
                app.entry_picker_hover(id, kind, *entered, w, cx)
            }))
            .on_click(cx.listener(move |app, _, w, cx| {
                app.toggle_entry_picker(id, kind, w, cx);
                cx.stop_propagation();
            }))
            .child(
                icon(if kind == PickerKind::Format {
                    "code"
                } else {
                    "conversion"
                })
                .size(px(18.)),
            )
            .child(ScriptLabel {
                text: label.to_owned(),
            });
        let menu_width = self
            .footer_options(kind)
            .iter()
            .flatten()
            .map(|label| self.footer_text_width_at(label, 14., window, cx) + 34.)
            .fold(134_f32, f32::max);
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let measured = bounds.clone();
        let anchor_for = |surface_width: f32| {
            let trigger = trigger_bounds.get();
            if trigger.center().x.as_f32() + surface_width / 2.
                > window.viewport_size().width.as_f32()
            {
                Anchor::BottomRight
            } else if trigger.center().x.as_f32() - surface_width / 2. < 0. {
                Anchor::BottomLeft
            } else {
                Anchor::BottomCenter
            }
        };
        let anchor = anchor_for(menu_width);
        let mut popup = gpui_base::Popup::new(format!("entry-picker-popup-{name}"), button)
            .snap_to_device_pixels()
            .preserve_horizontal_anchor()
            .w(px(width))
            .h(px(32.))
            .flex_shrink_0()
            .anchor(anchor)
            .offset(px(10.))
            .on_position(move |_, trigger| measured.set(trigger));
        if picker.is_some() {
            popup = popup.content(self.render_value_picker_menu(
                PickerHost::Entry(id),
                kind,
                label,
                width,
                menu_width,
                anchor,
                bounds,
                Some(dialog.entry_picker.arrow_left.clone()),
                cx,
            ));
        } else if dialog.entry_picker.tooltip == Some(kind) {
            let text = self.tr(if kind == PickerKind::Format {
                "interface.view_as"
            } else {
                "interface.decode_with"
            });
            let tooltip_width = self.footer_text_width_at(&text, 14., window, cx) + 28.;
            let tooltip_anchor = anchor_for(tooltip_width);
            let tooltip_bounds = dialog.entry_picker.tooltip_bounds.clone();
            // The original follower rounds the CSS anchor before centering its
            // fractional tooltip width. Reconstruct it before GPUI's half-pixel
            // trigger snapping changes the rounding tie.
            let trigger_left = trigger_bounds.get().origin.x.as_f32().floor();
            let tooltip_x = match tooltip_anchor {
                Anchor::BottomRight => trigger_left + width,
                Anchor::BottomLeft => trigger_left,
                _ => trigger_left + width / 2.,
            };
            popup = popup
                .anchor(tooltip_anchor)
                .anchor_x(px(tooltip_x.round()))
                .offset(px(6.))
                .content(
                    div()
                        .id("entry-picker-tooltip")
                        .test_support()
                        .role(Role::Tooltip)
                        .occlude()
                        .on_prepaint(move |bounds, _, _| tooltip_bounds.set(bounds))
                        .w(px(tooltip_width))
                        .h(px(37.))
                        .px(px(14.))
                        .py(px(8.))
                        .text_size(px(14.))
                        .line_height(px(21.))
                        .whitespace_nowrap()
                        .rounded(px(4.))
                        .bg(rgb(if self.dark(cx) { 0x2c2c32 } else { 0x262626 }))
                        .text_color(if self.dark(cx) {
                            rgba(0xffffffd1).into()
                        } else {
                            Hsla::white()
                        })
                        .shadow(super::super::notices::shadows(self.dark(cx)))
                        .on_hover(cx.listener(move |app, entered, w, cx| {
                            app.entry_picker_hover(id, kind, *entered, w, cx)
                        }))
                        .child(ScriptLabel { text }),
                );
        }
        popup.into_any_element()
    }
}
