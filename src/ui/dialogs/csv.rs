//! CSV forms follow the original ImportKeyDialog / ExportKeyDialog components.
use super::*;

fn csv_key_label(key: &[u8]) -> String {
    if !contains_binary(key) {
        return display_bytes(key);
    }
    // decodeRedisKey(array) keeps every printable ASCII byte, including '\\'.
    // This is display text; transport and DUMP export still use the raw key.
    key.iter()
        .map(|byte| {
            if (32..=126).contains(byte) {
                (*byte as char).to_string()
            } else {
                format!("\\x{byte:02X}")
            }
        })
        .collect()
}

impl RdmApp {
    pub(super) fn csv_text_color(&self, cx: &App) -> Hsla {
        if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        }
    }
    pub(super) fn csv_secondary_button(&self, button: Button, cx: &App) -> Button {
        button.when(self.dark(cx), |button| {
            button
                .bg(Hsla::transparent_black())
                .text_color(rgba(0xffffffd1))
                .border_color(rgb(0x515151))
        })
    }
    pub(in crate::ui) fn copy_csv_selection(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .dialog
            .as_ref()
            .is_some_and(|dialog| matches!(dialog.kind, DialogKind::Export))
            || !gpui_base::TextSelection::has_selection(window, cx)
        {
            return false;
        }
        let text = gpui_base::TextSelection::selected_text(window, cx);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }
    fn csv_form_field(&self, label: &str, control: AnyElement, _cx: &Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .mb(px(18.))
            .child(
                div()
                    .h(px(21.25))
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
            .into_any_element()
    }

    pub(super) fn csv_input_style(
        &self,
        field: Input,
        key: &str,
        window: &Window,
        cx: &Context<Self>,
    ) -> Input {
        self.styled_form_input(field, &self.dialog.as_ref().unwrap().fields[key], window, cx)
    }

    pub(super) fn styled_form_input(
        &self,
        field: Input,
        state: &Entity<InputState>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Input {
        let focused = state.focus_handle(cx).is_focused(window);
        field
            .focus_bordered(false)
            .when(self.dark(cx), |field| {
                field
                    .editor_style(gpui_base::input::InputEditorStyle {
                        foreground: rgba(0xffffffd1).into(),
                        muted_foreground: rgba(0xffffff61).into(),
                        selection: rgb(0x607ea5).into(),
                        caret: cx.theme().caret,
                        ..Default::default()
                    })
                    .bg(if focused {
                        // GPUI's shadow also paints under the control interior.
                        // Composite the Source tint over the modal first so the
                        // inset stays opaque, as CSS box-shadow does.
                        rgb(0x3d2d32)
                    } else {
                        rgba(0xffffff1a)
                    })
            })
            .border_color(if focused {
                cx.theme().primary_hover
            } else if self.dark(cx) {
                Hsla::transparent_black()
            } else {
                rgb(0xe0e0e6).into()
            })
            .when(focused, |field| {
                field.shadow(vec![
                    BoxShadow::new(px(0.), px(0.), cx.theme().primary.opacity(0.2))
                        .blur_radius(px(0.))
                        .spread_radius(px(2.)),
                ])
            })
    }

    fn csv_file_control(&self, save: bool, window: &Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        div()
            .flex()
            .items_center()
            .child(
                self.csv_input_style(form_input(&dialog.fields["path"]), "path", window, cx)
                    .id("csv-path-input")
                    .h(px(32.))
                    .rounded_r(px(0.))
                    .cleanable(true),
            )
            .child(
                self.csv_secondary_button(
                    self.btn("file-path", "...", "", cx, move |app, w, cx| {
                        app.choose_dialog_file("path", save, w, cx)
                    }),
                    cx,
                )
                .w(px(36.))
                .rounded_l(px(0.))
                .ml(px(-1.)),
            )
            .into_any_element()
    }

    fn csv_readonly(
        &self,
        key: &str,
        label: &str,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        self.csv_form_field(
            label,
            self.csv_input_style(form_input(&dialog.fields[key]), key, window, cx)
                .id(SharedString::from(format!("csv-{key}")))
                .aria_label(label.to_string())
                .readonly(true)
                .into_any_element(),
            cx,
        )
    }

    fn csv_radios(&self, key: &str, options: &[(&str, String)], cx: &Context<Self>) -> AnyElement {
        let value = self.dialog.as_ref().unwrap().value(key, cx);
        let mut row = div()
            .id(SharedString::from(format!("csv-{key}-choices")))
            .test_support()
            .flex()
            .self_start()
            .gap(px(1.))
            .h(px(32.));
        for (index, (candidate, label)) in options.iter().enumerate() {
            let selected = value == *candidate;
            let field = key.to_string();
            let choice = candidate.to_string();
            row = row.child(
                self.btn(
                    format!("radio-{key}-{candidate}"),
                    label.clone(),
                    "",
                    cx,
                    move |app, window, cx| app.set_dialog_field(&field, choice.clone(), window, cx),
                )
                .px(px(14.))
                .border_l_0()
                .border_r_0()
                .rounded(px(0.))
                .when(selected, |button| button.primary().bg(rgb(0xd13b37)))
                .when(!selected, |button| button.outline())
                .when(!selected && self.dark(cx), |button| {
                    button
                        .bg(Hsla::transparent_black())
                        .text_color(rgba(0xffffffd1))
                        .border_color(rgba(0xffffff17))
                })
                .when(index == 0, |button| {
                    button.border_l_1().rounded_tl(px(3.)).rounded_bl(px(3.))
                })
                .when(index + 1 == options.len(), |button| {
                    button.border_r_1().rounded_tr(px(3.)).rounded_br(px(3.))
                }),
            );
        }
        row.into_any_element()
    }

    fn csv_ttl_control(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        div()
            .id("csv-custom-ttl")
            .test_support()
            .flex()
            .h(px(32.))
            .mt(px(8.))
            .child(
                self.csv_input_style(self.number_input("ttl", cx), "ttl", window, cx)
                    .id("csv-ttl-input")
                    .h(px(32.))
                    .flex_1()
                    .min_w_0()
                    .rounded_r(px(0.)),
            )
            .child(self.render_csv_unit(window, cx))
            .into_any_element()
    }

    fn csv_affected_keys(&self, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let keys = dialog
            .target
            .as_ref()
            .map(|target| target.keys.as_slice())
            .unwrap_or(&[]);
        let height = (keys.len() as f32 * 25.).min(180.);
        // Original vueuc keeps a 25px estimate, with one preceding row and
        // ceil(viewport / estimate + 1) following rows. Natural row height is
        // still 1.6em, so this deliberately preserves its gaps after scrolling.
        let top = (-dialog.csv_key_scroll.offset().y.as_f32()).max(0.);
        let start = ((top / 25.).floor() as usize)
            .saturating_sub(1)
            .min(keys.len());
        let end = (start + (height / 25. + 1.).ceil() as usize + 1).min(keys.len());
        let rows = keys[start..end]
            .iter()
            .enumerate()
            .map(|(slot, key)| {
                let label = csv_key_label(key);
                div()
                    .id(("csv-affected-row", slot))
                    .test_support()
                    .aria_label(label.clone())
                    .min_h(px(22.4))
                    .line_height(px(22.4))
                    .cursor_text()
                    .child(
                        gpui_base::SelectableText::new(("csv-key-text", slot), label)
                            .document_order(slot as u64)
                            .reset_selection_on_text_change(true)
                            .preserve_empty_copy_fragments(true)
                            .selection_line_end_padding(px(4.))
                            .selection_color(
                            rgb(if self.dark(cx) { 0x5f83ac } else { 0xb1d5fd }).into(),
                            ),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("csv-affected-keys")
            .test_support()
            .border_1()
            .border_color(if self.dark(cx) {
                rgba(0xffffff17).into()
            } else {
                cx.theme().border
            })
            .rounded(px(3.))
            .bg(if self.dark(cx) {
                rgb(0x2c2c32)
            } else {
                rgb(0xfafafc)
            })
            .child(
                div()
                    .id("csv-affected-heading")
                    .test_support()
                    .h(px(48.))
                    .px(px(16.))
                    .py(px(12.))
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::MEDIUM)
                    .when(self.dark(cx), |heading| {
                        heading.text_color(rgba(0xffffffe6))
                    })
                    .child(format!(
                        "{}({})",
                        self.tr("dialogue.key.affected_key"),
                        keys.len()
                    )),
            )
            .child(
                div().px(px(16.)).pb(px(12.)).child(
                    div()
                        .id("csv-key-scroll-area")
                        .relative()
                        .h(px(height))
                        .on_hover(cx.listener(|app, hover, _, cx| app.csv_list_hover(*hover, cx)))
                        .child(
                            div()
                                .id("csv-affected-list")
                                .test_support()
                                .h(px(height))
                                .track_scroll(&dialog.csv_key_scroll)
                                .overflow_y_scroll()
                                .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|app, _, window, cx| {
                                        app.focus.focus(window, cx);
                                    }),
                                )
                                .child(
                                    div().relative().h(px(keys.len() as f32 * 25.)).child(
                                        div()
                                            .absolute()
                                            .top(px(start as f32 * 25.))
                                            .left_0()
                                            .right_0()
                                            .flex()
                                            .flex_col()
                                            .children(rows),
                                    ),
                                ),
                        )
                        .child(self.render_csv_scrollbar(height, keys.len(), cx)),
                ),
            )
            .into_any_element()
    }

    pub(super) fn csv_confirm_enabled(&self, cx: &App) -> bool {
        self.dialog.as_ref().is_some_and(|dialog| {
            !dialog.value("path", cx).is_empty()
                && (!matches!(dialog.kind, DialogKind::Export)
                    || dialog
                        .target
                        .as_ref()
                        .is_some_and(|target| !target.keys.is_empty()))
        })
    }

    pub(super) fn render_csv_dialog(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let import = matches!(dialog.kind, DialogKind::Import);
        let mut form = div()
            .id("csv-form")
            .test_support()
            .flex()
            .flex_1()
            .min_h_0()
            .flex_col()
            .justify_between()
            .child(self.form_pair(
                self.csv_readonly("server", &self.tr("dialogue.key.server"), window, cx),
                self.csv_readonly("db", &self.tr("dialogue.key.db_index"), window, cx),
            ));
        if import {
            let mut ttl = div().flex().flex_col().child(self.csv_radios(
                "ttl_type",
                &[
                    ("0", self.tr("dialogue.import.ttl_include")),
                    ("1", self.tr("dialogue.import.ttl_ignore")),
                    ("2", self.tr("dialogue.import.ttl_custom")),
                ],
                cx,
            ));
            if dialog.value("ttl_type", cx) == "2" {
                ttl = ttl.child(self.csv_ttl_control(window, cx));
            }
            form = form
                .child(self.csv_form_field(
                    &self.tr("dialogue.import.open_csv_file"),
                    self.csv_file_control(false, window, cx),
                    cx,
                ))
                .child(self.csv_form_field(
                    &self.tr("dialogue.import.conflict_handle"),
                    self.csv_radios(
                        "conflict",
                        &[
                            ("0", self.tr("dialogue.import.conflict_overwrite")),
                            ("1", self.tr("dialogue.import.conflict_ignore")),
                        ],
                        cx,
                    ),
                    cx,
                ))
                .child(self.csv_form_field(
                    &self.tr("dialogue.import.import_expire_title"),
                    ttl.into_any_element(),
                    cx,
                ))
                .child(
                    div()
                        .h(px(32.))
                        .mb(px(18.))
                        .flex()
                        .items_center()
                        .child(self.dialog_checkbox(
                            "reload",
                            &self.tr("dialogue.import.reload"),
                            cx,
                        )),
                );
        } else {
            form = form
                .child(
                    self.csv_form_field(
                        &self.tr("dialogue.export.export_expire_title"),
                        div()
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .child(self.dialog_checkbox(
                                "include_ttl",
                                &self.tr("dialogue.export.export_expire"),
                                cx,
                            ))
                            .into_any_element(),
                        cx,
                    ),
                )
                .child(self.csv_form_field(
                    &self.tr("dialogue.export.save_file"),
                    self.csv_file_control(true, window, cx),
                    cx,
                ))
                .child(self.csv_affected_keys(cx));
        }
        // GPUI snaps authored lengths before layout; preserve the original
        // total height and distribute the subpixel remainder between form rows.
        let height = if import {
            454. + if dialog.value("ttl_type", cx) == "2" {
                40.
            } else {
                0.
            }
        } else {
            394.75
                + dialog
                    .target
                    .as_ref()
                    .map(|target| (target.keys.len() as f32 * 25.).min(180.))
                    .unwrap_or(0.)
        };
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
                    .w(px(446.))
                    .h(px(height))
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
                            .mb(px(8.))
                            .text_size(px(18.))
                            .line_height(px(27.))
                            .font_weight(FontWeight::MEDIUM)
                            .when(self.dark(cx), |title| title.text_color(rgba(0xffffffe6)))
                            .child(dialog.title.clone()),
                    )
                    .child(form)
                    .child(
                        div()
                            .id("csv-actions")
                            .test_support()
                            .mt(px(16.))
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
                                        |app, w, cx| {
                                            app.close_dialog(w, cx);
                                            cx.notify();
                                        },
                                    ),
                                    cx,
                                )
                                .px(px(11.)),
                            )
                            .child(
                                self.btn(
                                    "dialog-save",
                                    self.tr(if import {
                                        "dialogue.import.import"
                                    } else {
                                        "dialogue.export.export"
                                    }),
                                    "",
                                    cx,
                                    |app, w, cx| app.submit_dialog(w, cx),
                                )
                                .primary()
                                .when(self.dark(cx), |button| button.text_color(rgb(0x000000)))
                                .px(px(11.))
                                .when(!self.csv_confirm_enabled(cx), |button| {
                                    button
                                        .bg(cx.theme().primary.opacity(if self.dark(cx) {
                                            0.38
                                        } else {
                                            0.5
                                        }))
                                        .border_color(
                                            cx.theme().primary.opacity(if self.dark(cx) {
                                                0.38
                                            } else {
                                                0.5
                                            }),
                                        )
                                        .text_color(if self.dark(cx) {
                                            rgb(0x1b1b1f)
                                        } else {
                                            rgb(0xffffff)
                                        })
                                })
                                .disabled(!self.csv_confirm_enabled(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
