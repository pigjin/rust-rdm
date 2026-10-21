use super::*;
use gpui::prelude::*;
#[cfg(target_os = "macos")]
use gpui_base::TestSupportExt as _;
use unicode_script::{Script, UnicodeScript};

#[cfg(target_os = "macos")]
#[derive(IntoElement)]
pub(super) struct CssLabel {
    pub text: String,
    pub text_id: String,
}

#[cfg(target_os = "macos")]
impl RenderOnce for CssLabel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let line = window.text_system().shape_line(
            self.text.clone().into(), font_size, &[style.to_run(self.text.len())], None,
        );
        // CSS rounds font boxes and floors the leading above the baseline.
        // Keep the ordinary text element and its parent's clipping/hit bounds.
        let css_baseline = ((line_height.as_f32().floor() + line.ascent.as_f32().round()
            - line.descent.as_f32().round()) / 2.).floor();
        let native_baseline = (line_height + line.ascent - line.descent) * 0.5;
        // script_width measures separate scripts to avoid CoreText's added
        // CJK/Latin spacing. Paint the same runs, including around parentheses
        // and duration labels, while retaining the CSS baseline above.
        let content = if script_runs(&self.text).len() > 1 {
            ScriptLabel { text: self.text }.into_any_element()
        } else {
            self.text.into_any_element()
        };
        div().relative().size_full().child(
            div().id(self.text_id).test_support().absolute().left_0()
                .top(px(css_baseline) - native_baseline).size_full().child(content),
        )
    }
}

// Avoid CoreText's extra interscript space, including around ASCII slashes.
// Keep each script run intact so western kerning and CJK punctuation still work.
pub(super) fn script_runs(text: &str) -> Vec<&str> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut previous = None;
    for (offset, ch) in text.char_indices() {
        let cjk = match ch.script() {
            Script::Han
            | Script::Hiragana
            | Script::Katakana
            | Script::Hangul
            | Script::Bopomofo => true,
            Script::Inherited => previous.unwrap_or(false),
            _ => matches!(ch, '\u{3000}'..='\u{303f}' | '\u{ff01}'..='\u{ff60}'),
        };
        if previous.is_some_and(|previous| previous != cjk) {
            runs.push(&text[start..offset]);
            start = offset;
        }
        previous = Some(cjk);
    }
    if start < text.len() {
        runs.push(&text[start..]);
    }
    runs
}

#[derive(IntoElement)]
pub(super) struct ScriptLabel {
    pub text: String,
}

impl RenderOnce for ScriptLabel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let mut width = px(0.);
        let mut children = Vec::new();
        for text in script_runs(&self.text) {
            let run_width = window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    font_size,
                    &[TextRun {
                        len: text.len(),
                        font: style.font(),
                        color: style.color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width;
            width += run_width;
            // Text's intrinsic layout rounds each run up to a logical pixel.
            // Explicit widths keep that rounding from accumulating at boundaries.
            children.push(div().w(run_width).flex_shrink_0().child(text.to_owned()));
        }
        div()
            .flex()
            .w(width)
            .flex_shrink_0()
            .whitespace_nowrap()
            .children(children)
    }
}

pub(super) fn script_width(text: &str, font: Font, font_size: f32, window: &Window) -> f32 {
    script_runs(text)
        .into_iter()
        .map(|run| {
            window
                .text_system()
                .shape_line(
                    run.to_owned().into(),
                    px(font_size),
                    &[TextRun {
                        len: run.len(),
                        font: font.clone(),
                        color: Hsla::white(),
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
