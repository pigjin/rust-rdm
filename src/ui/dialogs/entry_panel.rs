use super::*;
use crate::ui::text::ScriptLabel;

impl RdmApp {
    pub(super) fn entry_panel_text(&self, cx: &App) -> Hsla {
        if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        }
    }

    fn entry_panel_icon(
        &self,
        id: &'static str,
        image: &'static str,
        size: f32,
        label: String,
        selected: bool,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        let selected_color: Hsla = if self.dark(cx) {
            rgba(0xffffff47).into()
        } else {
            rgb(0xd1d1d1).into()
        };
        Button::new(id)
            .ghost()
            .tab_stop(false)
            .w(px(26.))
            .h(px(26.))
            .p(px(0.))
            .rounded(px(3.))
            .border_1()
            .border_color(if selected {
                selected_color
            } else {
                Hsla::transparent_black()
            })
            .bg(if selected {
                selected_color
            } else {
                Hsla::transparent_black()
            })
            .text_color(self.entry_panel_text(cx))
            .tooltip(label)
            .child(
                div()
                    .id(format!("{id}-icon"))
                    .test_support()
                    .size(px(size))
                    .child(icon(image).size(px(size))),
            )
            .on_click(cx.listener(move |app, _, w, cx| handler(app, w, cx)))
    }

    pub(super) fn render_collection_entry_panel(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dialog = self.displayed_entry_dialog().unwrap();
        let dialog_id = dialog.id;
        let kind = dialog
            .target
            .as_ref()
            .map(|target| target.kind.clone())
            .unwrap_or_default();
        let dark = self.dark(cx);
        let text = self.entry_panel_text(cx);
        let label_color: Hsla = if dark {
            rgba(0xffffff85).into()
        } else {
            rgb(0x767c82).into()
        };
        let control_border: Hsla = if dark {
            rgba(0xffffff17).into()
        } else {
            rgb(0xe0e0e6).into()
        };
        let card_border: Hsla = if dark {
            rgba(0xffffff17).into()
        } else {
            rgb(0xefeff5).into()
        };
        let field_key = if kind == KeyType::Hash {
            "field"
        } else if kind == KeyType::ZSet {
            "score"
        } else {
            "row_index"
        };
        let field = self
            .styled_form_input(
                form_input(&dialog.fields[field_key]),
                &dialog.fields[field_key],
                window,
                cx,
            )
            .id("entry-field-input")
            .w_full()
            .readonly(matches!(kind, KeyType::List | KeyType::Set))
            .when(
                dark && dialog.fields[field_key].focus_handle(cx).is_focused(window),
                |field| field.bg(rgb(0x2b1b1e)),
            );
        let label = |id: &'static str, text: String| {
            div()
                .id(id)
                .test_support()
                .h(px(28.))
                .flex_shrink_0()
                .py(px(5.))
                .text_size(px(13.))
                .line_height(px(18.))
                .text_color(label_color)
                .child(ScriptLabel { text })
        };
        let mut selectors = div()
            .id("entry-format-selectors")
            .test_support()
            .flex()
            .items_center()
            .h(px(32.))
            .flex_shrink_0()
            .mt(px(5.));
        for kind in [
            value_footer::PickerKind::Format,
            value_footer::PickerKind::Decode,
        ] {
            if kind == value_footer::PickerKind::Decode {
                selectors = selectors.child(
                    div()
                        .id("entry-format-divider")
                        .test_support()
                        .mx(px(20.))
                        .h(px(16.))
                        .w(px(1.))
                        .flex_shrink_0()
                        .bg(rgb(0xaaaaab)),
                );
            }
            selectors = selectors.child(self.render_entry_picker(kind, window, cx));
        }
        let changed = dialog.entry_changed(cx);
        let enabled = changed;
        let primary: Hsla = rgb(0xd33a31).into();
        let save_label = self.tr("common.update");
        let save = Button::new("save-entry")
            .ghost()
            .w(px(
                self.footer_text_width_at(&save_label, 14., window, cx) + 48.
            ))
            .h(px(32.))
            .px(px(12.))
            .rounded(px(3.))
            .border_0()
            .disabled(!enabled)
            .bg(if enabled {
                primary.opacity(0.16)
            } else {
                primary.opacity(if dark {0.38} else {0.5})
            })
            .text_color(if enabled {
                primary
            } else if dark {
                rgb(0x0f0f11).into()
            } else {
                rgb(0xffffff).into()
            })
            .opacity(1.)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(14.))
                    .line_height(px(14.))
                    .child(icon("save").size(px(18.)))
                    .child(ScriptLabel { text: save_label }),
            )
            .on_click(cx.listener(move |app, _, w, cx| {
                if enabled
                    && app
                        .dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.id == dialog_id)
                {
                    app.submit_dialog(w, cx);
                }
            }));
        let editor_font = if self.pref.editor.font_family.is_empty() {
            "Menlo".into()
        } else {
            self.preference_font("editor", "Menlo", cx).family
        };
        let body = div()
            .id("entry-panel-content")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(16.))
            .pb(px(12.))
            .child(label(
                "entry-field-label",
                self.tr(if kind == KeyType::Hash {
                    "common.field"
                } else if kind == KeyType::ZSet {
                    "common.score"
                } else {
                    "common.index"
                }),
            ))
            .child(field)
            .child(div().h(px(16.)).flex_shrink_0())
            .child(label("entry-value-label", self.tr("common.value")))
            .child(
                div()
                    .id("entry-editor-border")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(control_border)
                    .rounded(px(3.))
                    .p(px(2.))
                    .child(
                        div()
                            .id("entry-value-editor")
                            .test_support()
                            .size_full()
                            .bg(rgb(if dark { 0x1e1e1e } else { 0xffffff }))
                            .text_size(px(self.editor_font_size()))
                            .child(
                                Editor::new(&dialog.editors["value"])
                                    .h_full()
                                    .appearance(false)
                                    .text_color(rgb(if dark {0xd4d4d4} else {0x000000}))
                                    .font_family(editor_font)
                                    .editor_style(
                                        self.scalar_editor_style(&dialog.value("format", cx), cx),
                                    )
                                    .editor_paddings(gpui::Edges::default())
                                    .text_size(px(self.editor_font_size()))
                                    .readonly(false),
                            ),
                    ),
            )
            .child(selectors);
        let header = div()
            .id("entry-panel-header")
            .test_support()
            .flex()
            .items_center()
            .h(px(51.))
            .flex_shrink_0()
            .px(px(16.))
            .child(
                div()
                    .id("entry-panel-title")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h(px(24.))
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if dark {
                        Hsla::from(rgba(0xffffffe6))
                    } else {
                        Hsla::from(rgb(0x1f2225))
                    })
                    .child(ScriptLabel {
                        text: self.tr("interface.edit_row"),
                    }),
            )
            .child(
                div()
                    .id("entry-panel-tools")
                    .test_support()
                    .flex()
                    .items_start()
                    .gap(px(5.))
                    .h(px(27.))
                    .child(self.entry_panel_icon(
                        "pin-entry",
                        "entry-pin",
                        19.,
                        self.tr(if dialog.boolean("pin") {
                            "interface.unpin_edit"
                        } else {
                            "interface.pin_edit"
                        }),
                        dialog.boolean("pin"),
                        cx,
                        move |app, _, cx| {
                            if let Some(dialog) = &mut app.dialog
                                && dialog.id == dialog_id
                            {
                                let pin = dialog.boolean("pin");
                                set_json_boolean(&mut dialog.data, "pin", !pin);
                                cx.notify();
                            }
                        },
                    ))
                    .child(self.entry_panel_icon(
                        "full-entry",
                        if self.entry_panel_fullscreen() {
                            "entry-offscreen"
                        } else {
                            "entry-fullscreen"
                        },
                        18.,
                        self.tr("interface.fullscreen"),
                        false,
                        cx,
                        move |app, _, cx| {
                            if app
                                .dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.id == dialog_id)
                            {
                                app.full_editor = !app.full_editor;
                                cx.notify();
                            }
                        },
                    ))
                    .child(self.entry_panel_icon(
                        "close-entry",
                        "entry-close",
                        18.,
                        self.tr("menu.close"),
                        false,
                        cx,
                        move |app, _, cx| {
                            if app
                                .dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.id == dialog_id)
                            {
                                app.dismiss_entry();
                                cx.notify();
                            }
                        },
                    )),
            );
        div()
            .id("entry-panel")
            .test_support()
            .size_full()
            .pl(px(2.))
            .child(
                div()
                    .id("entry-panel-card")
                    .test_support()
                    .size_full()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(card_border)
                    .rounded(px(3.))
                    .bg(rgb(if dark { 0x18181c } else { 0xffffff }))
                    .text_color(text)
                    .child(header)
                    .child(body)
                    .child(
                        div()
                            .id("entry-panel-actions")
                            .test_support()
                            .h(px(42.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_end()
                            .px(px(10.))
                            .py(px(5.))
                            .child(save),
                    ),
            )
            .into_any_element()
    }
}
