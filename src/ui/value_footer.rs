use super::text::{ScriptLabel as FooterLabel, script_runs as footer_text_runs};
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKind {
    Format,
    Decode,
}

#[derive(Clone, Copy)]
pub(super) enum PickerHost {
    Footer(u64),
    Entry(u64),
}

pub(super) struct FooterPicker {
    pub kind: PickerKind,
    pub cursor: Option<usize>,
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    _token: gpui_base::DeferredPopover,
}

impl FooterPicker {
    pub(super) fn new(kind: PickerKind, window: &mut Window, cx: &mut App) -> Self {
        let previous_focus = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            kind,
            cursor: None,
            focus,
            previous_focus,
            _token: gpui_base::GlobalState::register_deferred_popover(cx),
        }
    }

    pub(super) fn restore_focus(&self, fallback: &FocusHandle, window: &mut Window, cx: &mut App) {
        if self.focus.contains_focused(window, cx) {
            self.previous_focus
                .as_ref()
                .unwrap_or(fallback)
                .focus(window, cx);
        }
    }
}

impl RdmApp {
    fn value_picker(&self, host: PickerHost) -> Option<&FooterPicker> {
        match host {
            PickerHost::Footer(id) => self
                .tab_index(id)
                .and_then(|i| self.tabs[i].footer_picker.as_ref()),
            PickerHost::Entry(id) => self.entry_picker_open(id),
        }
    }

    fn value_picker_key(
        &mut self,
        host: PickerHost,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match host {
            PickerHost::Footer(id) => self.footer_picker_key(id, event, window, cx),
            PickerHost::Entry(id) => self.entry_picker_key(id, event, window, cx),
        }
    }

    fn value_picker_close(
        &mut self,
        host: PickerHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match host {
            PickerHost::Footer(id) => self.close_footer_picker(id, window, cx),
            PickerHost::Entry(id) => self.close_entry_picker(id, window, cx),
        }
    }

    fn value_picker_choose(
        &mut self,
        host: PickerHost,
        kind: PickerKind,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match host {
            PickerHost::Footer(id) => self.choose_footer_option(id, kind, index, window, cx),
            PickerHost::Entry(id) => self.choose_entry_option(id, kind, index, window, cx),
        }
    }

    fn value_picker_cursor(
        &mut self,
        host: PickerHost,
        index: usize,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match host {
            PickerHost::Footer(id) => {
                if let Some(i) = self.tab_index(id)
                    && let Some(picker) = &mut self.tabs[i].footer_picker
                {
                    if hovered {
                        picker.cursor = Some(index);
                    } else if !window.last_input_was_keyboard() && picker.cursor == Some(index) {
                        picker.cursor = None;
                    }
                    cx.notify();
                }
            }
            PickerHost::Entry(id) => self.entry_picker_cursor(id, index, hovered, window, cx),
        }
    }
    fn footer_picker_hover(
        &mut self,
        id: u64,
        kind: PickerKind,
        entered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else { return };
        let tab = &mut self.tabs[i];
        tab.footer_hover_epoch += 1;
        let epoch = tab.footer_hover_epoch;
        if entered {
            tab.footer_hover = Some(kind);
        } else if tab.footer_hover == Some(kind) {
            tab.footer_hover = None;
        }
        if tab.footer_picker.is_some() {
            tab.footer_tooltip = None;
            cx.notify();
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].footer_hover_epoch == epoch
                    && app.tabs[i].footer_picker.is_none()
                {
                    let tab = &mut app.tabs[i];
                    if tab.footer_hover.is_some()
                        || !tab
                            .footer_tooltip_bounds
                            .get()
                            .contains(&window.mouse_position())
                    {
                        tab.footer_tooltip = tab.footer_hover;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn close_footer_picker(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self.tab_index(id)
            && let Some(picker) = self.tabs[i].footer_picker.take()
        {
            if picker.focus.contains_focused(window, cx) {
                picker
                    .previous_focus
                    .unwrap_or_else(|| self.focus.clone())
                    .focus(window, cx);
            }
            cx.notify();
        }
    }

    fn toggle_footer_picker(
        &mut self,
        id: u64,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else { return };
        if self.scalar_format_disabled(i, cx) {
            return;
        }
        let same = self.tabs[i]
            .footer_picker
            .as_ref()
            .is_some_and(|picker| picker.kind == kind);
        self.tabs[i].footer_tooltip = None;
        self.tabs[i].footer_hover = None;
        self.tabs[i].footer_hover_epoch += 1;
        self.close_footer_picker(id, window, cx);
        if !same {
            let previous_focus = window.focused(cx);
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            self.tabs[i].footer_picker = Some(FooterPicker {
                kind,
                cursor: None,
                focus,
                previous_focus,
                _token: gpui_base::GlobalState::register_deferred_popover(cx),
            });
        }
        cx.notify();
    }

    pub(super) fn footer_options(&self, kind: PickerKind) -> Vec<Option<String>> {
        if kind == PickerKind::Format {
            return codec::FORMATS
                .iter()
                .map(|value| Some(value.to_string()))
                .collect();
        }
        let mut options = codec::DECODERS
            .iter()
            .filter(|name| {
                !self
                    .pref
                    .decoder
                    .iter()
                    .any(|decoder| decoder.name == **name)
            })
            .map(|name| Some(name.to_string()))
            .collect::<Vec<_>>();
        if !self.pref.decoder.is_empty() {
            options.push(None);
            options.extend(
                self.pref
                    .decoder
                    .iter()
                    .map(|decoder| Some(decoder.name.clone())),
            );
        }
        options.push(None);
        options.push(Some(self.tr("interface.custom_decoder")));
        options
    }

    fn choose_footer_option(
        &mut self,
        id: u64,
        kind: PickerKind,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = self.footer_options(kind);
        let Some(Some(value)) = options.get(index) else {
            return;
        };
        self.close_footer_picker(id, window, cx);
        if kind == PickerKind::Decode && index == options.len() - 1 {
            self.open_decoder_settings(window, cx);
        } else if kind == PickerKind::Format {
            self.change_format(id, None, Some(value.clone()), window, cx);
        } else {
            self.change_format(id, Some(value.clone()), None, window, cx);
        }
    }

    fn footer_picker_key(
        &mut self,
        id: u64,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else { return };
        let Some(picker) = &self.tabs[i].footer_picker else {
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
                self.tabs[i].footer_picker.as_mut().unwrap().cursor = Some(indices[next]);
            }
            "enter" => {
                if let Some(cursor) = cursor {
                    self.choose_footer_option(id, kind, cursor, window, cx);
                }
            }
            "escape" => self.close_footer_picker(id, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn footer_text_color(&self, cx: &App) -> Hsla {
        if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        }
    }

    fn footer_text_width(&self, text: &str, window: &Window, cx: &App) -> f32 {
        self.footer_text_width_at(text, self.general_font_size(), window, cx)
    }

    pub(super) fn footer_text_width_at(
        &self,
        text: &str,
        font_size: f32,
        window: &Window,
        cx: &App,
    ) -> f32 {
        footer_text_runs(text)
            .into_iter()
            .map(|text| {
                window
                    .text_system()
                    .shape_line(
                        text.to_owned().into(),
                        px(font_size),
                        &[TextRun {
                            len: text.len(),
                            font: self.preference_font("general", ".SystemUIFont", cx),
                            color: self.footer_text_color(cx),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width
                    .as_f32()
            })
            .sum()
    }

    fn footer_label(&self, text: &str) -> FooterLabel {
        FooterLabel {
            text: text.to_owned(),
        }
    }

    fn footer_divider(&self, name: &'static str, cx: &App) -> AnyElement {
        div()
            .id(name)
            .test_support()
            .w(px(1.))
            .h(px(16.))
            .mx(px(8.))
            .flex_shrink_0()
            .bg(if self.dark(cx) {
                rgba(0xffffff17)
            } else {
                rgb(0xaaaaab)
            })
            .into_any_element()
    }

    fn render_footer_picker(
        &self,
        i: usize,
        kind: PickerKind,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let disabled = self.scalar_format_disabled(i, cx);
        let name = if kind == PickerKind::Format {
            "format"
        } else {
            "decode"
        };
        let label = if kind == PickerKind::Format {
            if tab.format == "Auto" || tab.format.is_empty() {
                "Raw"
            } else {
                &tab.format
            }
        } else if tab.decode == "Auto" || tab.decode.is_empty() {
            "None"
        } else {
            &tab.decode
        };
        // Naive's medium button keeps its own 14px font when the body size changes.
        let width = self.footer_text_width_at(label, 14., window, cx) + 48.;
        let picker = tab
            .footer_picker
            .as_ref()
            .filter(|picker| picker.kind == kind);
        let hover: Hsla = if self.dark(cx) {
            rgba(0xffffff1f).into()
        } else {
            rgba(0x2e333817).into()
        };
        let button = div()
            .id(name)
            .test_support()
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
            .text_color(self.footer_text_color(cx))
            .when(!disabled, |button| {
                button.cursor_pointer().hover(move |style| style.bg(hover))
            })
            .when(disabled, |button| button.opacity(0.5))
            .on_hover(cx.listener(move |app, entered, window, cx| {
                if !disabled {
                    app.footer_picker_hover(id, kind, *entered, window, cx)
                }
            }))
            .on_click(cx.listener(move |app, _, window, cx| {
                app.toggle_footer_picker(id, kind, window, cx);
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
            .child(self.footer_label(label));
        let options = self.footer_options(kind);
        let menu_width = options
            .iter()
            .flatten()
            .map(|label| self.footer_text_width_at(label, 14., window, cx) + 34.)
            .fold(134_f32, f32::max);
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let measured = bounds.clone();
        let anchor = if kind == PickerKind::Decode && menu_width > width {
            Anchor::BottomRight
        } else {
            Anchor::BottomCenter
        };
        let mut popup = gpui_base::Popup::new(format!("footer-popup-{name}"), button)
            .snap_to_device_pixels()
            .w(px(width))
            .h(px(32.))
            .flex_shrink_0()
            .anchor(anchor)
            .offset(px(10.))
            .on_position(move |_, trigger| measured.set(trigger));
        if picker.is_some() {
            popup = popup.content(self.render_value_picker_menu(
                PickerHost::Footer(id),
                kind,
                label,
                width,
                menu_width,
                anchor,
                bounds.clone(),
                None,
                cx,
            ));
        }
        if picker.is_none() && tab.footer_tooltip == Some(kind) {
            let tooltip_bounds = tab.footer_tooltip_bounds.clone();
            let text = self.tr(if kind == PickerKind::Format {
                "interface.view_as"
            } else {
                "interface.decode_with"
            });
            let tooltip_width = self.footer_text_width_at(&text, 14., window, cx) + 28.;
            let tooltip_anchor = if kind == PickerKind::Decode && tooltip_width > width {
                Anchor::BottomRight
            } else {
                Anchor::BottomCenter
            };
            // The CSS follower rounds its anchor before applying its percentage
            // transform. Reconstruct the footer's center from the styled widths;
            // GPUI's already-snapped trigger can move a half-pixel tie the other way.
            if kind == PickerKind::Format {
                let decode_label = if tab.decode == "Auto" || tab.decode.is_empty() {
                    "None"
                } else {
                    &tab.decode
                };
                let decode_width = self.footer_text_width_at(decode_label, 14., window, cx) + 48.;
                let center =
                    window.viewport_size().width.as_f32() - 11. - decode_width - 41. - width / 2.;
                popup = popup.anchor_x(px(center.round()));
            }
            popup = popup.anchor(tooltip_anchor).offset(px(6.)).content(
                div()
                    .id("footer-tooltip")
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
                    .shadow(super::notices::shadows(self.dark(cx)))
                    .on_hover(cx.listener(move |app, entered, window, cx| {
                        app.footer_picker_hover(id, kind, *entered, window, cx)
                    }))
                    .child(self.footer_label(&text)),
            );
        }
        popup.into_any_element()
    }

    pub(super) fn render_value_picker_menu(
        &self,
        host: PickerHost,
        kind: PickerKind,
        label: &str,
        width: f32,
        menu_width: f32,
        anchor: Anchor,
        bounds: Rc<Cell<Bounds<Pixels>>>,
        arrow_position: Option<Rc<Cell<Pixels>>>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let picker = self.value_picker(host).unwrap();
        let outside_bounds = bounds.clone();
        let options = self.footer_options(kind);
        let (prefix, option_id) = match host {
            PickerHost::Footer(_) => ("footer", "footer-option"),
            PickerHost::Entry(_) => ("entry-picker", "entry-picker-option"),
        };
        let surface: Hsla = rgb(if self.dark(cx) { 0x272727 } else { 0xffffff }).into();
        let title = self.tr(if kind == PickerKind::Format {
            "interface.view_as"
        } else {
            "interface.decode_with"
        });
        let mut menu = div()
            .id(format!("{prefix}-options"))
            .test_support()
            .role(Role::Menu)
            .track_focus(&picker.focus)
            .relative()
            .w(px(menu_width))
            .py(px(6.))
            .px(px(2.))
            .rounded(px(5.))
            .bg(surface)
            .shadow(super::notices::shadows(self.dark(cx)))
            .text_size(px(14.))
            .text_color(self.footer_text_color(cx))
            .on_key_down(cx.listener(move |app, event, window, cx| {
                app.value_picker_key(host, event, window, cx)
            }))
            .on_mouse_down_out(cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                if !outside_bounds.get().contains(&event.position) {
                    app.value_picker_close(host, window, cx);
                }
            }))
            .child(
                div()
                    .id(format!("{prefix}-menu-title"))
                    .test_support()
                    .h(px(30.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(15.))
                    .font_weight(FontWeight::BOLD)
                    .child(
                        div()
                            .id(format!("{prefix}-menu-title-label"))
                            .test_support()
                            .child(self.footer_label(&title)),
                    ),
            )
            .child(self.footer_menu_separator(format!("{prefix}-menu-title-divider"), cx));
        for (index, option) in options.iter().enumerate() {
            let Some(option) = option else {
                menu = menu.child(
                    self.footer_menu_separator(format!("{prefix}-menu-divider-{index}"), cx),
                );
                continue;
            };
            let active =
                option == label && !(kind == PickerKind::Decode && index == options.len() - 1);
            let pending = picker.cursor == Some(index);
            menu = menu.child(
                div()
                    .id((option_id, index))
                    .test_support()
                    .role(Role::MenuItem)
                    .aria_label(option.clone())
                    .relative()
                    .h(px(28.))
                    .w_full()
                    .flex()
                    .items_center()
                    .pl(px(14.))
                    .pr(px(16.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(if active {
                        rgb(0xd33a31).into()
                    } else if pending {
                        Hsla::white()
                    } else {
                        self.footer_text_color(cx)
                    })
                    .on_hover(cx.listener(move |app, hovered, window, cx| {
                        app.value_picker_cursor(host, index, *hovered, window, cx);
                    }))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.value_picker_choose(host, kind, index, window, cx);
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(4.))
                            .right(px(4.))
                            .rounded(px(5.))
                            .bg(if active {
                                Hsla::from(rgb(0xd33a31)).opacity(if self.dark(cx) {
                                    0.15
                                } else {
                                    0.1
                                })
                            } else if pending {
                                rgb(0xd33a31).into()
                            } else {
                                Hsla::transparent_black()
                            }),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(28.))
                            .line_height(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(self.footer_label(option)),
                    ),
            );
        }
        let arrow_left = if anchor == Anchor::BottomRight {
            menu_width - width / 2. - 6.
        } else {
            menu_width / 2. - 6.
        };
        let arrow_left = arrow_position
            .as_ref()
            .map_or(px(arrow_left), |position| position.get());
        if let Some(position) = arrow_position {
            menu = menu.on_prepaint(move |menu_bounds, window, _| {
                let next = bounds.get().center().x - menu_bounds.origin.x - px(6.);
                if (next - position.get()).abs() > px(0.01) {
                    position.set(next);
                    window.refresh();
                }
            });
        }
        menu = menu.child(
            icon("entry-delete-arrow")
                .size(px(12.))
                .h(px(6.))
                .absolute()
                .bottom(px(-6.))
                .left(arrow_left)
                .text_color(surface),
        );
        menu.into_any_element()
    }

    fn footer_menu_separator(&self, name: impl Into<ElementId>, cx: &App) -> AnyElement {
        div()
            .id(name)
            .test_support()
            .h(px(1.))
            .w_full()
            .my(px(4.))
            .flex_shrink_0()
            .bg(if self.dark(cx) {
                rgba(0xffffff17)
            } else {
                rgb(0xefeff5)
            })
            .into_any_element()
    }

    pub(super) fn render_value_footer(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let Some(detail) = &tab.detail else {
            return div().into_any_element();
        };
        let json = detail.kind == KeyType::Json;
        let scalar = detail.kind == KeyType::String;
        let summary = if scalar {
            detail.length.to_string()
        } else {
            format!(
                "{} / {}",
                detail.rows.len(),
                detail.length.max(detail.rows.len() as u64)
            )
        };
        let summary = format!(
            "{}: {summary}",
            self.tr(if scalar {
                "interface.length"
            } else {
                "interface.entries"
            })
        );
        let mut footer = div()
            .id("value-footer")
            .test_support()
            .h(px(37.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(10.))
            .text_size(px(self.general_font_size()))
            .line_height(px(self.general_font_size() * 1.5))
            .whitespace_nowrap()
            .border_t_1()
            .border_color(if json || scalar {
                cx.theme().border
            } else {
                self.entry_border(cx)
            })
            .text_color(self.footer_text_color(cx))
            .bg(rgb(if self.dark(cx) { 0x2b2b2b } else { 0xfafafc }))
            .shadow(vec![
                BoxShadow::new(px(0.), px(-5.), rgba(0x0000001a).into())
                    .blur_radius(px(5.))
                    .spread_radius(px(-5.)),
            ])
            .when(!json, |footer| {
                footer.child(
                    div()
                        .id("footer-summary")
                        .test_support()
                        .w(px(self.footer_text_width(&summary, window, cx)))
                        .flex_shrink_0()
                        .child(self.footer_label(&summary)),
                )
            });
        if detail.memory > 0 {
            let memory = format!(
                "{}: {}",
                self.tr("interface.memory_usage"),
                footer_bytes(detail.memory)
            );
            footer = footer
                .when(!json, |footer| {
                    footer.child(self.footer_divider("footer-memory-divider", cx))
                })
                .child(
                    div()
                        .id("footer-memory")
                        .test_support()
                        .w(px(self.footer_text_width(&memory, window, cx)))
                        .flex_shrink_0()
                        .child(self.footer_label(&memory)),
                );
        }
        footer = footer.child(div().flex_1());
        if !matches!(detail.kind, KeyType::Stream | KeyType::Json) && !self.entry_panel_visible() {
            footer = footer.child(
                div()
                    .id("footer-selectors")
                    .test_support()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(self.render_footer_picker(i, PickerKind::Format, window, cx))
                    .child(self.footer_divider("footer-format-divider", cx))
                    .child(self.render_footer_picker(i, PickerKind::Decode, window, cx)),
            );
        }
        footer.into_any_element()
    }
}

fn footer_bytes(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B";
    for candidate in ["KB", "MB", "GB", "TB"] {
        if value < 1024. {
            break;
        }
        value /= 1024.;
        unit = candidate;
    }
    let value = format!("{value:.2}");
    format!(
        "{}{unit}",
        value.trim_end_matches('0').trim_end_matches('.')
    )
}
