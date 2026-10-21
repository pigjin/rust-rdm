use super::*;

pub(super) struct EntryContent {
    displayed: String,
    pub(super) editing: String,
    pub(super) bound_decode: String,
    pub(super) bound_format: String,
    pub(super) diagnostics: json_diagnostics::Controller,
    brackets: gpui_base::input::TextDecorationCollection,
}

impl EntryContent {
    pub(super) fn new(
        decode: String,
        format: String,
        editor: &Entity<EditorState>,
        cx: &mut App,
    ) -> Self {
        Self {
            displayed: String::new(),
            editing: String::new(),
            bound_decode: decode,
            bound_format: format,
            diagnostics: Default::default(),
            brackets: editor.update(cx, |editor, cx| {
                editor.create_decorations_collection(vec![], cx)
            }),
        }
    }
}

impl Dialog {
    pub(in crate::ui) fn update_entry_brackets(&self, cx: &mut App) {
        let Some(content) = &self.entry_content else {
            return;
        };
        let editor = &self.editors["value"];
        let decorations = if editor.read(cx).language_name() == "json" {
            crate::ui::scalar_value::json_decorations(
                &text_input(editor, cx),
                cx.theme().mode == ThemeMode::Dark,
            )
        } else {
            vec![]
        };
        content.brackets.set(decorations, cx);
    }

    pub(super) fn refresh_entry_json(&mut self, window: &mut Window, cx: &mut Context<RdmApp>) {
        self.update_entry_brackets(cx);
        self.entry_content.as_mut().unwrap().diagnostics.refresh(
            &self.editors["value"],
            window,
            cx,
        );
    }

    pub(super) fn set_converted_entry_content(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.data["value"] = Json::String(text.clone());
        self.entry_content.as_mut().unwrap().editing = text.clone();
        self.display_entry_content(text, window, cx);
    }

    fn display_entry_content(&mut self, text: String, window: &mut Window, cx: &mut App) {
        let content = self.entry_content.as_mut().unwrap();
        if content.displayed != text {
            content.displayed = text.clone();
            content.editing = text.clone();
            set_input(&self.editors["value"], text, window, cx);
        }
    }
}

impl RdmApp {
    pub(super) fn entry_dialog_mut(&mut self, id: u64) -> Option<&mut Dialog> {
        self.dialog
            .iter_mut()
            .chain(self.dialog_stack.iter_mut())
            .chain(self.cached_entry_panels.values_mut().map(|(dialog, _)| dialog))
            .find(|dialog| dialog.id == id && matches!(dialog.kind, DialogKind::EditEntry(_)))
    }

    pub(super) fn convert_entry(
        &mut self,
        id: u64,
        decode: String,
        format: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.entry_dialog_mut(id) else {
            return;
        };
        let DialogKind::EditEntry(row) = &dialog.kind else {
            unreachable!();
        };
        let raw = row.value.clone();
        // Only changes to the display prop reset Monaco. A repeated empty prop
        // can retain visible text while editingContent is reset independently.
        dialog.display_entry_content(String::new(), window, cx);
        dialog.refresh_entry_json(window, cx);
        dialog.format_loading = true;
        dialog.format_epoch += 1;
        let custom = self.pref.decoder.clone();
        let requested_decode = decode.clone();
        let requested_format = format.clone();
        let fallback = codec::source_string(&raw);
        self.entry_conversions += 1;
        self.cpu_job(
            window,
            cx,
            move || {
                #[cfg(feature = "ui-qa")]
                if let Some(value) = super::entry_conversion_qa::controlled(&raw, &decode, &format)
                {
                    return Ok(value);
                }
                Ok(codec::convert(&raw, &decode, &format, &custom))
            },
            move |app, value, window, cx| {
                app.entry_conversions = app.entry_conversions.saturating_sub(1);
                // The original component applies replies in completion order,
                // including replies received while preferences cover it.
                let Some(dialog) = app.entry_dialog_mut(id) else {
                    return;
                };
                let content = dialog.entry_content.as_ref().unwrap();
                let old_decode = content.bound_decode.clone();
                let old_format = content.bound_format.clone();
                let mut value = value.unwrap_or_else(|_| codec::Converted {
                    text: fallback,
                    decode: requested_decode.clone(),
                    format: requested_format.clone(),
                });
                if !requested_decode.is_empty() {
                    value.decode = requested_decode;
                }
                if !requested_format.is_empty() {
                    value.format = requested_format;
                }
                let DialogKind::EditEntry(row) = &dialog.kind else {
                    unreachable!();
                };
                let (field, text) = match dialog.target.as_ref().unwrap().kind {
                    KeyType::Hash => ("field", display_bytes(&row.id)),
                    KeyType::ZSet => ("score", row.score.clone()),
                    _ => (
                        "row_index",
                        dialog.data["row_index"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ),
                };
                set_input(&dialog.fields[field], text.clone(), window, cx);
                dialog.data[field] = Json::String(text);
                let content = dialog.entry_content.as_mut().unwrap();
                content.bound_decode = value.decode.clone();
                content.bound_format = value.format.clone();
                set_input(&dialog.fields["decode"], value.decode.clone(), window, cx);
                set_input(&dialog.fields["format"], value.format.clone(), window, cx);
                dialog.editors["value"].update(cx, |editor, cx| {
                    editor.set_highlighter(editor_language(&value.format), cx);
                });
                dialog.set_converted_entry_content(value.text, window, cx);
                dialog.refresh_entry_json(window, cx);
                dialog.format_loading = false;
                // v-model updates trigger the original post-flush watchEffect.
                // Repeating an unchanged selection performs just one conversion.
                if old_decode != value.decode || old_format != value.format {
                    app.convert_entry(id, value.decode, value.format, window, cx);
                }
            },
        );
    }
}
