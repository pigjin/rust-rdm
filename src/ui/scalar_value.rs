use super::text::{ScriptLabel, script_width};
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use gpui_base::input::{
    EditorGutterStyle, HighlightStyleResolver, InputEditorStyle, TextDecoration,
};

struct JsonColors {
    dark: bool,
}

#[cfg(test)]
mod tests {
    use super::{bracket_decorations, json_decorations};
    use gpui::{HighlightStyle, Hsla, rgb};
    use gpui_base::input::TextDecoration;
    use serde_json::Value as Json;

    fn source_color(css: &str) -> Hsla {
        let components: Vec<f32> = css
            .split_once('(')
            .unwrap()
            .1
            .trim_end_matches(')')
            .split(',')
            .map(|value| value.trim().parse().unwrap())
            .collect();
        let mut color: Hsla = rgb(((components[0] as u32) << 16)
            | ((components[1] as u32) << 8)
            | components[2] as u32)
        .into();
        color.a = components.get(3).copied().unwrap_or(1.);
        color
    }

    #[test]
    fn json_syntax_matches_original_monaco_samples() {
        let fixture: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/json-syntax-source.json"
        ))
        .unwrap();
        let samples = fixture["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 36);
        for sample in samples {
            let text = sample["text"].as_str().unwrap();
            let dark = sample["dark"].as_bool().unwrap();
            let mut colors = vec![None; text.len()];
            for decoration in json_decorations(text, dark) {
                colors[decoration.range].fill(decoration.style.color);
            }
            let mut expected_tokens = Vec::new();
            let mut line_start = 0;
            for ((raw_line, line), tokens) in text
                .split_inclusive('\n')
                .zip(sample["lines"].as_array().unwrap())
                .zip(sample["tokens"].as_array().unwrap())
            {
                let content = raw_line.trim_end_matches(['\r', '\n']);
                let mut byte = line_start;
                for span in line["spans"].as_array().unwrap() {
                    let part = span["text"].as_str().unwrap();
                    assert_eq!(&text[byte..byte + part.len()], part);
                    let expected = Some(source_color(span["color"].as_str().unwrap()));
                    assert!(colors[byte..byte + part.len()].iter().all(|color| *color == expected),
                        "Original color of {part:?} in {text:?}, dark={dark}");
                    byte += part.len();
                }
                assert_eq!(byte, line_start + content.len());
                let utf16: Vec<_> = content.encode_utf16().collect();
                let tokens = tokens.as_array().unwrap();
                for (index, token) in tokens.iter().enumerate() {
                    let start = token["offset"].as_u64().unwrap() as usize;
                    let end = tokens.get(index + 1)
                        .map_or(utf16.len(), |next| next["offset"].as_u64().unwrap() as usize);
                    let start = line_start + String::from_utf16(&utf16[..start]).unwrap().len();
                    let end = line_start + String::from_utf16(&utf16[..end]).unwrap().len();
                    if start != end {
                        expected_tokens.push((start..end, token["type"].as_str().unwrap().to_owned()));
                    }
                }
                line_start += raw_line.len();
            }
            let actual_tokens: Vec<_> = super::super::json_diagnostics::syntax_tokens(text)
                .into_iter()
                .map(|token| (token.range, token.scope.to_owned()))
                .collect();
            assert_eq!(actual_tokens, expected_tokens, "Original tokens of {text:?}");
        }
    }

    #[test]
    fn json_brackets_match_original_samples() {
        let source: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/json-bracket-source.json"
        ))
        .unwrap();
        for sample in source["samples"].as_array().unwrap() {
            let text = sample["text"].as_str().unwrap();
            let actual = bracket_decorations(text, sample["dark"].as_bool().unwrap());
            let mut offset = 0;
            let mut expected = Vec::new();
            for span in sample["spans"].as_array().unwrap() {
                let part = span["text"].as_str().unwrap().replace('\u{a0}', " ");
                if part.len() == 1 && "{}[]".contains(&part) {
                    let css = span["color"].as_str().unwrap();
                    let components: Vec<f32> = css
                        .split_once('(')
                        .unwrap()
                        .1
                        .trim_end_matches(')')
                        .split(',')
                        .map(|v| v.trim().parse().unwrap())
                        .collect();
                    let mut color: Hsla = rgb(((components[0] as u32) << 16)
                        | ((components[1] as u32) << 8)
                        | components[2] as u32)
                    .into();
                    color.a = components.get(3).copied().unwrap_or(1.);
                    expected.push(TextDecoration::new(
                        offset..offset + 1,
                        HighlightStyle {
                            color: Some(color),
                            ..Default::default()
                        },
                    ));
                }
                offset += part.len();
            }
            assert_eq!(actual, expected, "original bracket colors for {sample}");
        }
    }
}
impl HighlightStyleResolver for JsonColors {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        let color = match (self.dark, name) {
            (false, "property") => 0xa31515,
            (true, "property") => 0x9cdcfe,
            (false, "string" | "string.escape" | "boolean" | "constant.builtin") => 0x0451a5,
            (true, "string" | "string.escape" | "boolean" | "constant.builtin") => 0xce9178,
            (false, "number") => 0x098658,
            (true, "number") => 0xb5cea8,
            (false, "comment") => 0x008000,
            (true, "comment") => 0x608b4e,
            (false, "punctuation") => 0x000000,
            (true, "punctuation") => 0xdcdcdc,
            _ => return None,
        };
        Some(HighlightStyle {
            color: Some(rgb(color).into()),
            ..Default::default()
        })
    }
}

#[cfg(test)]
fn bracket_decorations(text: &str, dark: bool) -> Vec<TextDecoration> {
    brackets_from_tokens(text, dark, &super::json_diagnostics::syntax_tokens(text))
}

fn brackets_from_tokens(
    text: &str,
    dark: bool,
    tokens: &[super::json_diagnostics::SyntaxToken],
) -> Vec<TextDecoration> {
    let palette = if dark {
        [0xffd700, 0xda70d6, 0x179fff]
    } else {
        [0x0431fa, 0x319331, 0x7b3814]
    };
    let mut brackets: Vec<TextDecoration> = Vec::new();
    let mut stack: Vec<(char, usize)> = Vec::new();
    let invalid: Hsla = rgba(0xff1212cc).into();
    for token in tokens.iter().filter(|token| {
        matches!(token.scope, "delimiter.bracket.json" | "delimiter.array.json")
    }) {
        let offset = token.range.start;
        let c = text[offset..].chars().next().unwrap();
        let color = match c {
            '{' | '[' => {
                let color = rgb(palette[stack.len() % 3]).into();
                stack.push((c, brackets.len()));
                color
            }
            '}' | ']' => {
                let opener = if c == '}' { '{' } else { '[' };
                if let Some(index) = stack.iter().rposition(|(kind, _)| *kind == opener) {
                    for (_, unclosed) in &stack[index + 1..] {
                        brackets[*unclosed].style.color = Some(invalid);
                    }
                    let color = brackets[stack[index].1].style.color.unwrap();
                    stack.truncate(index);
                    color
                } else {
                    invalid
                }
            }
            _ => continue,
        };
        brackets.push(TextDecoration::new(
            offset..offset + 1,
            HighlightStyle {
                color: Some(color),
                ..Default::default()
            },
        ));
    }
    for (_, unclosed) in stack {
        brackets[unclosed].style.color = Some(invalid);
    }
    brackets
}

pub(super) fn json_decorations(text: &str, dark: bool) -> Vec<TextDecoration> {
    let tokens = super::json_diagnostics::syntax_tokens(text);
    let colors = JsonColors { dark };
    let mut decorations: Vec<_> = tokens
        .iter()
        .map(|token| {
            let name = match token.scope {
                "string.key.json" => "property",
                "string.value.json" | "keyword.json" => "string",
                "number.json" => "number",
                scope if scope.starts_with("delimiter.") => "punctuation",
                scope if scope.starts_with("comment.") => "comment",
                _ => "",
            };
            TextDecoration::new(
                token.range.clone(),
                colors.style(name).unwrap_or(HighlightStyle {
                    color: Some(rgb(if dark { 0xd4d4d4 } else { 0x000000 }).into()),
                    ..Default::default()
                }),
            )
        })
        .collect();
    decorations.extend(brackets_from_tokens(text, dark, &tokens));
    decorations
}

impl RdmApp {
    pub(super) fn update_scalar_diagnostics(&mut self,id:u64,window:&mut Window,cx:&mut Context<Self>){
        let Some(i)=self.tab_index(id) else{return;};
        let tab=&mut self.tabs[i];
        tab.json_diagnostics.refresh(&tab.editor,window,cx);
    }
    pub(super) fn scalar_format_disabled(&self, i: usize, cx: &App) -> bool {
        let tab = &self.tabs[i];
        tab.detail
            .as_ref()
            .is_some_and(|detail| detail.kind == KeyType::String)
            && !tab.values_loading
            && text_input(&tab.editor, cx) != tab.editor_original
    }
    pub(super) fn update_json_brackets(&self, id: u64, cx: &mut App) {
        let Some(i) = self.tab_index(id) else { return };
        let tab = &self.tabs[i];
        let decorations = if tab.detail.as_ref().is_some_and(|detail| {
            detail.kind == KeyType::Json
                || (detail.kind == KeyType::String
                    && matches!(tab.format.as_str(), "JSON" | "Unicode JSON"))
        }) {
            json_decorations(&text_input(&tab.editor, cx), self.dark(cx))
        } else {
            vec![]
        };
        tab.json_brackets.set(decorations, cx);
    }

    pub(super) fn scalar_editor_style(&self, format: &str, cx: &App) -> InputEditorStyle {
        let dark = self.dark(cx);
        InputEditorStyle {
            foreground: rgb(if dark { 0xd4d4d4 } else { 0x000000 }).into(),
            muted_foreground: rgb(if dark { 0x858585 } else { 0xbabbbd }).into(),
            background: rgb(if dark { 0x1e1e1e } else { 0xffffff }).into(),
            editor_gutter_background: Some(rgb(if dark { 0x1e1e1e } else { 0xfffffe }).into()),
            border: cx.theme().border,
            selection: rgb(if dark { 0x264f78 } else { 0xadd6ff }).into(),
            caret: rgb(if dark { 0xaeafad } else { 0x000000 }).into(),
            highlight_styles: if matches!(format, "JSON" | "Unicode JSON") {
                Arc::new(JsonColors { dark })
            } else {
                cx.theme().highlight_theme.clone()
            },
            gutter: Some(EditorGutterStyle {
                min_digits: 2,
                right_padding: px(0.),
                fold_width: px(16.),
                active_foreground: rgb(if dark { 0xc6c6c6 } else { 0x777d83 }).into(),
                active_line_border: Some(rgb(if dark { 0x282828 } else { 0xeeeeee }).into()),
                highlight_text_line: false,
                fold_icons_only_on_hover: true,
            }),
            diagnostics: gpui_base::input::DiagnosticColors {
                error: rgb(if dark { 0xf14c4c } else { 0xe51400 }).into(),
                warning: rgb(if dark { 0xcca700 } else { 0xbf8803 }).into(),
                info: cx.theme().primary,
                hint: cx.theme().muted_foreground,
            },
            overview_ruler: Some(gpui_base::input::EditorOverviewRulerStyle {
                width: px(10.),
                minimum_height: px(6.),
                diagnostics: gpui_base::input::DiagnosticColors {
                    error: rgba(0xff1212b3).into(),
                    warning: rgb(if dark { 0xcca700 } else { 0xbf8803 }).into(),
                    info: rgb(if dark { 0x3794ff } else { 0x1a85ff }).into(),
                    hint: Hsla::transparent_black(),
                },
                cursor: Some(rgba(if dark { 0xaeafadb3 } else { 0x000000b3 }).into()),
                border: Some(rgba(0x7f7f7f4d).into()),
            }),
            diagnostic_underline: Some(gpui_base::input::DiagnosticUnderlineStyle {
                path: "icons/editor-diagnostic-wave.svg".into(),
                svg: include_str!("../../assets/icons/editor-diagnostic-wave.svg"),
                size: size(px(6.), px(3.)),
            }),
            ..Default::default()
        }
    }

    pub(super) fn render_scalar_value(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let json = tab.detail.as_ref().unwrap().kind == KeyType::Json;
        let enabled = !tab.values_loading && text_input(&tab.editor, cx) != tab.editor_original;
        let copy_label = self.tr("interface.copy_value");
        let save_label = self.tr("common.save");
        let font = window.text_style().font();
        // The original passes an array for its empty preference. Monaco falls
        // back to its macOS Menlo stack; explicit preferences are strings.
        let editor_font = if self.pref.editor.font_family.is_empty() {
            "Menlo".into()
        } else {
            self.preference_font("editor", "Menlo", cx).family
        };
        let button = |name: &'static str, label: String, image: &str| {
            let width = script_width(&label, font.clone(), 14., window) + 48.;
            let mut content = div().text_size(px(14.)).line_height(px(14.)).gap(px(6.));
            Button::new(name)
                .icon(icon(image).size(px(18.)))
                .child(ScriptLabel {
                    text: label.clone(),
                })
                .accessibility_label(label)
                .content_style(content.style().clone(), gpui_component::Size::Size(px(18.)))
                .w(px(width))
                .h(px(32.))
                .px(px(11.))
                .text_size(px(14.))
                .line_height(px(14.))
                .font_weight(FontWeight::NORMAL)
                .outline()
                .map(|button| self.key_button_style(button, cx))
        };
        let copy = button("copy-value", copy_label, "copy")
            .rounded_l(px(4.))
            .rounded_r(px(0.))
            .disabled(tab.value_saving)
            .when(tab.value_saving, |button| button.opacity(0.5))
            .on_click(cx.listener(move |app, _, _, cx| {
                if let Some(i) = app.tab_index(id) {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        app.tabs[i].editor_original.clone(),
                    ));
                    app.toast(app.tr("interface.copy_succ"), false);
                    cx.notify();
                }
            }));
        let save = button("save-value", save_label, "save")
            .rounded_l(px(0.))
            .rounded_r(px(4.))
            .disabled(!enabled)
            .when(!enabled, |button| button.opacity(0.5))
            .when(enabled, |button| {
                let mut hover = div()
                    .bg(cx.theme().primary.opacity(0.22))
                    .border_color(Hsla::transparent_black());
                let mut active = div()
                    .bg(cx.theme().primary.opacity(0.28))
                    .border_color(Hsla::transparent_black());
                button
                    .text_color(cx.theme().primary)
                    .bg(cx.theme().primary.opacity(0.16))
                    .border_color(Hsla::transparent_black())
                    .hover_style(hover.style().clone())
                    .active_style(active.style().clone())
            })
            .loading(tab.value_saving)
            .on_click(cx.listener(|app, _, window, cx| app.save_value(window, cx)));
        div()
            .flex()
            .flex_col()
            .size_full()
            .pt(px(5.))
            .gap(px(5.))
            .child(
                div()
                    .id(if json {
                        "json-toolbar"
                    } else {
                        "string-toolbar"
                    })
                    .test_support()
                    .h(px(32.))
                    .flex_shrink_0()
                    .px(px(5.))
                    .flex()
                    .justify_end()
                    .items_center()
                    .child(copy)
                    .child(save),
            )
            .child(
                div()
                    .id(if json {
                        "json-value-wrapper"
                    } else {
                        "string-value-wrapper"
                    })
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .px(px(5.))
                    .child(
                        div().size_full().p(px(2.)).child(
                            div()
                                .id(if json { "json-editor" } else { "string-editor" })
                                .test_support()
                                .size_full()
                                .text_size(px(self.editor_font_size()))
                                .child(
                                    Editor::new(&tab.editor)
                                        .h_full()
                                        .appearance(false)
                                        .text_color(rgb(if self.dark(cx) {0xd4d4d4} else {0x000000}))
                                        .font_family(editor_font)
                                        .editor_style(self.scalar_editor_style(&tab.format, cx))
                                        .editor_paddings(gpui::Edges::default())
                                        .text_size(px(self.editor_font_size()))
                                        .readonly(tab.values_loading),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn save_scalar_value(
        &mut self,
        i: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.tabs[i];
        let Some(detail) = &tab.detail else { return };
        let (id, db, connection, key, detail_epoch) = (
            tab.id,
            tab.db,
            tab.conn.clone(),
            detail.key.clone(),
            tab.detail_epoch,
        );
        let json = detail.kind == KeyType::Json;
        let draft = text_input(&tab.editor, cx);
        let value = if json {
            draft.as_bytes().to_vec()
        } else {
            match codec::encode(&draft, &tab.decode, &tab.format, &self.pref.decoder) {
                Ok(value) => value,
                Err(error) => {
                    self.toast(error.to_string(), true);
                    return;
                }
            }
        };
        let saved_bytes = value.clone();
        tab.value_saving = true;
        tab.value_save_epoch += 1;
        let save_epoch = tab.value_save_epoch;
        self.job(
            window,
            cx,
            move |service| Ok(service.set_string(&connection, db, &key, value, json)),
            move |app, result, window, cx| {
                let mut convert_saved = false;
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].value_save_epoch == save_epoch
                {
                    let tab = &mut app.tabs[i];
                    tab.value_saving = false;
                    if result.is_ok() && tab.detail_epoch == detail_epoch {
                        convert_saved = !json;
                        tab.editor_original = draft.clone();
                        if let Some(detail) = &mut tab.detail {
                            detail.value = saved_bytes;
                        }
                        tab.editor
                            .update(cx, |editor, cx| editor.set_value(draft, window, cx));
                    }
                }
                if convert_saved {
                    app.prepare_values(id, window, cx);
                }
                app.update_json_brackets(id, cx);
                app.update_scalar_diagnostics(id, window, cx);
                match result {
                    Ok(()) => app.toast(app.tr("interface.save_value_succ"), false),
                    Err(error) => app.toast(error.to_string(), true),
                }
            },
        );
    }
}
