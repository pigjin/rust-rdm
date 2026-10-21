//! Monaco 0.47 diagnostic hover and marker navigation for our editor models.
//! Layout and lifetime are intentionally separate from syntax validation.
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use gpui_base::input::{
    DiagnosticEntry, DiagnosticPresentation, DiagnosticSeverity, EditorViewZone, RopeExt as _,
};
use std::{cell::Cell, rc::Rc};

mod text;
use text::{MessageText, Part};

/// Monaco's editor commands remain available while one of its diagnostic
/// widgets has focus. Keep the editor identity when forwarding that command;
/// focusing the editor first would change the widget's selection/lifetime.
#[derive(Clone, gpui::Action, PartialEq, Eq, serde::Deserialize)]
#[action(namespace = rdm_diagnostic_message, no_json)]
pub(super) struct SaveFromMessage {
    pub editor: u64,
}

pub(super) fn init(cx: &mut App) {
    text::init(cx);
}

thread_local! { static NEXT_MODEL: Cell<usize> = const { Cell::new(0) }; }

#[cfg(feature = "ui-qa")]
pub(super) fn reset_model_counter() {
    NEXT_MODEL.with(|next| next.set(0));
}

#[derive(Default)]
struct Navigation {
    model: usize,
    marker: Option<DiagnosticEntry>,
    index: Option<usize>,
    title_index: usize,
    title_count: usize,
    stale: bool,
    scroll: ScrollHandle,
    scroll_drag: Option<(Pixels, Pixels)>,
    scrollbar_hover: bool,
    hover_scroll: ScrollHandle,
    hover_drag: Option<(Pixels, Pixels)>,
    hover_scrollbar_hover: bool,
}
type NavigationState = Rc<RefCell<Navigation>>;

pub(super) fn install(state: &mut EditorState) {
    let navigation = NavigationState::default();
    let attached = navigation.clone();
    let hover = navigation.clone();
    let next = navigation.clone();
    let close = navigation.clone();
    state.set_diagnostic_presentation(DiagnosticPresentation {
        attached: Rc::new(move || {
            attached.borrow_mut().model = NEXT_MODEL.with(|next| {
                let id = next.get() + 1;
                next.set(id);
                id
            });
        }),
        show_delay: Duration::from_millis(300),
        hide_delay: Duration::from_millis(300),
        render_hover: Rc::new(move |editor, entries, _, _| {
            HoverSurface {
                editor,
                entries,
                navigation: hover.clone(),
            }
            .into_any_element()
        }),
        navigate: Rc::new(move |state, forward, window, cx| {
            navigate(state, &next, forward, None, window, cx)
        }),
        dismiss: Rc::new(move |state, _, cx| dismiss(state, &close, cx)),
        changed: Rc::new(move |state, cx| {
            let mut nav = navigation.borrow_mut();
            nav.index = None;
            if nav.marker.is_none() {
                return;
            }
            let anchor = state.view_zone_after().unwrap_or(state.cursor());
            let marker = state
                .diagnostics()
                .and_then(|set| set.iter().find(|entry| contains(entry, anchor)))
                .cloned();
            nav.stale = marker.is_none();
            if let Some(marker) = marker {
                nav.marker = Some(marker);
            }
            drop(nav);
            set_zone(state, &navigation, anchor, cx);
        }),
    });
}

fn contains(entry: &DiagnosticEntry, offset: usize) -> bool {
    entry.range.start <= offset && offset <= entry.range.end
}

fn dismiss(
    state: &mut EditorState,
    navigation: &NavigationState,
    cx: &mut Context<EditorState>,
) -> bool {
    if navigation.borrow_mut().marker.take().is_none() {
        return false;
    }
    navigation.borrow_mut().index = None;
    state.set_view_zone(None, cx);
    true
}

fn navigate(
    state: &mut EditorState,
    navigation: &NavigationState,
    forward: bool,
    at: Option<usize>,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let mut entries: Vec<_> = state
        .diagnostics()
        .map(|set| {
            set.iter()
                .filter(|entry| entry.severity != DiagnosticSeverity::Hint)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|entry| {
        (
            entry.range.start,
            entry.range.end,
            match entry.severity {
                DiagnosticSeverity::Error => 0,
                DiagnosticSeverity::Warning => 1,
                DiagnosticSeverity::Info => 2,
                DiagnosticSeverity::Hint => 3,
            },
        )
    });
    if entries.is_empty() {
        return;
    }
    let mut nav = navigation.borrow_mut();
    let cursor = at.unwrap_or(state.cursor());
    let index = if at.is_none() {
        nav.index.filter(|i| *i < entries.len())
    } else {
        None
    };
    let index = match index {
        Some(i) if forward => (i + 1) % entries.len(),
        Some(i) => (i + entries.len() - 1) % entries.len(),
        None => entries
            .iter()
            .position(|entry| contains(entry, cursor) || cursor <= entry.range.start)
            .unwrap_or(if forward { 0 } else { entries.len() - 1 }),
    };
    let marker = entries[index].clone();
    let anchor = if contains(&marker, state.cursor()) {
        state.cursor()
    } else {
        marker.range.start
    };
    nav.marker = Some(marker);
    nav.index = Some(index);
    nav.title_index = index + 1;
    nav.title_count = entries.len();
    nav.stale = false;
    nav.scroll.set_offset(point(px(0.), px(0.)));
    drop(nav);
    state.clear_diagnostic_popover(cx);
    state.set_selected_range(anchor..anchor, cx);
    state.focus(window, cx);
    set_zone(state, navigation, anchor, cx);
}

fn set_zone(
    state: &mut EditorState,
    navigation: &NavigationState,
    after: usize,
    cx: &mut Context<EditorState>,
) {
    let lines = navigation
        .borrow()
        .marker
        .as_ref()
        .map(|entry| entry.message.split('\n').count().clamp(1, 17))
        .unwrap_or(1);
    let height = state.line_height().unwrap_or(px(21.)) * (3 + lines);
    let editor = cx.entity().downgrade();
    let navigation = navigation.clone();
    state.set_view_zone(
        Some(EditorViewZone {
            after,
            height,
            right_inset: px(10.),
            render: Rc::new(move |window, cx| {
                let Some(editor) = editor.upgrade() else {
                    return Empty.into_any_element();
                };
                render_navigation(editor, navigation.clone(), height, window, cx)
            }),
        }),
        cx,
    );
}

fn suffix(entry: &DiagnosticEntry) -> String {
    let source = entry.source.as_deref().unwrap_or("");
    match &entry.code {
        Some(code) => format!("{source}({code})"),
        None => source.into(),
    }
}

fn shape(text: &str, font: &Font, font_size: Pixels, color: Hsla, window: &Window) -> ShapedLine {
    window.text_system().shape_line(
        text.to_owned().into(),
        font_size,
        &[TextRun {
            len: text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    )
}

fn editor_font(editor: &EditorState, cx: &App) -> (Font, Pixels) {
    editor.editor_font().unwrap_or_else(|| {
        (
            font(cx.theme().mono_font_family.clone()),
            cx.theme().mono_font_size,
        )
    })
}

// CSS pre-wrap uses eight-space tab stops. The blank tab fragment retains a
// single source byte for selection, rather than copying its visual spaces.
fn text_width(text: &str, font: &Font, font_size: Pixels, color: Hsla, window: &Window) -> Pixels {
    if !text.contains('\t') {
        return shape(text, font, font_size, color, window).width;
    }
    let interval = shape(" ", font, font_size, color, window).width * 8.;
    let mut x = px(0.);
    for (i, segment) in text.split('\t').enumerate() {
        if i > 0 {
            x = interval * ((x / interval).floor() + 1.);
        }
        x += shape(segment, font, font_size, color, window).width;
    }
    x
}

fn message_parts(
    text: &str,
    start: usize,
    font: &Font,
    font_size: Pixels,
    color: Hsla,
    window: &Window,
) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut previous = 0;
    let mut x = px(0.);
    let interval = shape(" ", font, font_size, color, window).width * 8.;
    for (index, _) in text.match_indices('\t') {
        if index > previous {
            let line = shape(&text[previous..index], font, font_size, color, window);
            let width = line.width;
            parts.push(Part {
                x,
                line,
                source: start + previous..start + index,
                selection_opacity: color.a,
                tab: false,
            });
            x += width;
        }
        let width = interval * ((x / interval).floor() + 1.) - x;
        let mut line = window.text_system().shape_line(
            " ".into(),
            font_size,
            &[TextRun {
                len: 1,
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            Some(width),
        );
        let measured = std::sync::Arc::clone(&line);
        *line = std::sync::Arc::new(LineLayout {
            width,
            font_size: measured.font_size,
            ascent: measured.ascent,
            descent: measured.descent,
            runs: measured.runs.clone(),
            len: 1,
        });
        line.text = "\t".into();
        parts.push(Part {
            x,
            line,
            source: start + index..start + index + 1,
            selection_opacity: color.a,
            tab: true,
        });
        x += width;
        previous = index + 1;
    }
    if previous < text.len() || parts.is_empty() {
        parts.push(Part {
            x,
            line: shape(&text[previous..], font, font_size, color, window),
            source: start + previous..start + text.len(),
            selection_opacity: color.a,
            tab: false,
        });
    }
    parts
}

// CSS inline wrapping keeps the last word attached to the adjacent code span.
// Whitespace at a soft break hangs beyond the painted line's ink.
fn message_lines(
    entry: &DiagnosticEntry,
    width: Pixels,
    font: &Font,
    font_size: Pixels,
    color: Hsla,
    window: &Window,
) -> Vec<Vec<Part>> {
    let suffix = suffix(entry);
    let code_width = shape(&suffix, font, font_size, color, window).width;
    let mut result = Vec::new();
    let mut paragraph_start = 0;
    let paragraphs: Vec<_> = entry.message.split('\n').collect();
    for (paragraph_index, paragraph) in paragraphs.iter().enumerate() {
        let mut rest = *paragraph;
        let mut start = paragraph_start;
        loop {
            let last_paragraph = paragraph_index + 1 == paragraphs.len();
            let all_width = text_width(rest, font, font_size, color, window)
                + if last_paragraph && !suffix.is_empty() {
                    px(6.) + code_width
                } else {
                    px(0.)
                };
            let end = if all_width <= width || rest.is_empty() {
                rest.len()
            } else {
                rest.char_indices()
                    .filter(|(_, ch)| ch.is_whitespace())
                    .map(|(i, ch)| i + ch.len_utf8())
                    .filter(|i| {
                        text_width(rest[..*i].trim_end(), font, font_size, color, window) <= width
                    })
                    .last()
                    .unwrap_or_else(|| {
                        rest.char_indices()
                            .skip(1)
                            .map(|(i, _)| i)
                            .take_while(|i| {
                                text_width(&rest[..*i], font, font_size, color, window) <= width
                            })
                            .last()
                            .unwrap_or(rest.len())
                    })
            };
            let text = &rest[..end];
            let x = text_width(text, font, font_size, color, window);
            let mut parts = message_parts(text, start, font, font_size, color, window);
            if end == rest.len() && last_paragraph && !suffix.is_empty() {
                parts.push(Part {
                    x: x + px(6.),
                    line: shape(
                        &suffix,
                        font,
                        font_size,
                        Hsla {
                            a: color.a * 0.6,
                            ..color
                        },
                        window,
                    ),
                    source: entry.message.len()..entry.message.len() + suffix.len(),
                    selection_opacity: color.a * 0.6,
                    tab: false,
                });
            }
            result.push(parts);
            if end == rest.len() {
                break;
            }
            let following = rest[end..].trim_start();
            start += rest.len() - following.len();
            rest = following;
        }
        paragraph_start += paragraph.len() + 1;
    }
    result
}

struct HoverSurface {
    editor: Entity<EditorState>,
    entries: Vec<DiagnosticEntry>,
    navigation: NavigationState,
}
impl IntoElement for HoverSurface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
struct HoverLayout {
    element: Option<AnyElement>,
    bounds: Bounds<Pixels>,
}
impl Element for HoverSurface {
    type RequestLayoutState = HoverLayout;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some("json-diagnostic-hover-surface".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, HoverLayout) {
        let state = self.editor.read(cx);
        let first = self.entries.first();
        let anchor = first.and_then(|entry| state.range_to_bounds(&entry.range));
        let Some(anchor) = anchor else {
            return (
                div()
                    .absolute()
                    .size_0()
                    .into_any_element()
                    .request_layout(window, cx),
                HoverLayout {
                    element: None,
                    bounds: Bounds::default(),
                },
            );
        };
        let editor_bounds = state.input_bounds();
        let (font, font_size) = editor_font(state, cx);
        let line_height = state.line_height().unwrap_or(font_size * 1.5);
        let dark = cx.theme().mode == ThemeMode::Dark;
        let fg = rgb(if dark { 0xcccccc } else { 0x616161 }).into();
        let border = rgb(if dark { 0x454545 } else { 0xc8c8c8 });
        let natural = self
            .entries
            .iter()
            .map(|entry| {
                let lines: Vec<_> = entry.message.split('\n').collect();
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        text_width(line, &font, font_size, fg, window)
                            + if i + 1 == lines.len() {
                                shape(&suffix(entry), &font, font_size, fg, window).width
                            } else {
                                px(0.)
                            }
                    })
                    .max()
                    .unwrap_or(px(0.))
                    + px(24.)
            })
            .max()
            .unwrap_or(px(0.));
        // The original keyboard-opened WebKit hover lays out its status bar
        // with both labels present. Keep the existing pointer path separate.
        let natural = if state.diagnostic_hover_from_keyboard() {
            let footer_font = gpui::font(".SystemUIFont");
            natural.max(
                shape("View Problem (⌥F8)", &footer_font, px(12.), fg, window).width
                    + shape(
                        "No quick fixes available",
                        &footer_font,
                        px(12.),
                        fg,
                        window,
                    )
                    .width
                    + px(34.),
            )
        } else {
            natural
        };
        let width = px(natural.as_f32().round())
            .min(editor_bounds.size.width)
            .min((editor_bounds.size.width * 0.66).max(px(500.)))
            .min(window.bounds().size.width - px(30.));
        let mut document_text = self
            .entries
            .iter()
            .map(|entry| format!("{}{}", entry.message, suffix(entry)))
            .collect::<Vec<_>>()
            .join("\n");
        document_text.push('\n');
        let footer_start = document_text.len();
        document_text.push_str("No quick fixes available\n");
        let document = text::Document::new(
            id.expect("diagnostic hover has an id"),
            document_text.into(),
            footer_start,
            window,
            cx,
        );
        let mut document_offset = 0;
        let mut content = div()
            .id("diagnostic-hover-content")
            .test_support()
            .w(width)
            .flex()
            .flex_col();
        let mut height = px(25.);
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                content = content.child(div().w(width).h(px(1.)).flex_shrink_0().bg(border));
                height += px(1.);
            }
            let lines = message_lines(entry, width - px(16.), &font, font_size, fg, window);
            let body_height = line_height * lines.len();
            height += body_height + px(8.);
            content = content.child(
                div()
                    .w(width)
                    .h(body_height + px(8.))
                    .flex_shrink_0()
                    .px(px(8.))
                    .py(px(4.))
                    .child(MessageText {
                        id: ("diagnostic-hover-message", index).into(),
                        order: index as u64 + 1,
                        document: Some((document.clone(), document_offset)),
                        inline_selection: false,
                        text: format!("{}{}", entry.message, suffix(entry)).into(),
                        lines,
                        size: size(width - px(16.), body_height),
                        line_height,
                        font: font.clone(),
                        font_size,
                        foreground: fg,
                        background: rgb(if dark { 0x252526 } else { 0xf3f3f3 }).into(),
                        editor: self.editor.clone(),
                        navigation: self.navigation.clone(),
                    }),
            );
            document_offset += entry.message.len() + suffix(entry).len() + 1;
        }
        let editor = self.editor.downgrade();
        let navigation = self.navigation.clone();
        let at = first.unwrap().range.start;
        content = content.child(
            div()
                .w(width)
                .h(px(23.))
                .flex_shrink_0()
                .border_t_1()
                .border_color(border)
                .bg(rgb(if dark { 0x2c2c2d } else { 0xe7e7e7 }))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .px(px(8.))
                        .text_size(px(12.))
                        .font_family(".SystemUIFont")
                        .line_height(px(22.))
                        .child(
                            div()
                                .id("diagnostic-view-problem")
                                .test_support()
                                .flex_shrink_0()
                                .mr(px(16.))
                                .cursor_pointer()
                                .text_color(rgb(if dark { 0x3794ff } else { 0x006ab1 }))
                                .child("View Problem (⌥F8)")
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    if let Some(editor) = editor.upgrade() {
                                        editor.update(cx, |state, cx| {
                                            navigate(state, &navigation, true, Some(at), window, cx)
                                        });
                                    }
                                }),
                        )
                        .child(MessageText {
                            id: "diagnostic-hover-footer".into(),
                            order: self.entries.len() as u64 + 1,
                            document: Some((document.clone(), footer_start)),
                            inline_selection: true,
                            text: "No quick fixes available".into(),
                            lines: vec![vec![Part {
                                x: px(0.),
                                line: shape(
                                    "No quick fixes available",
                                    &gpui::font(".SystemUIFont"),
                                    px(12.),
                                    fg,
                                    window,
                                ),
                                source: 0.."No quick fixes available".len(),
                                selection_opacity: 1.,
                                tab: false,
                            }]],
                            size: size(
                                shape(
                                    "No quick fixes available",
                                    &gpui::font(".SystemUIFont"),
                                    px(12.),
                                    fg,
                                    window,
                                )
                                .width,
                                px(22.),
                            ),
                            line_height: px(22.),
                            font: gpui::font(".SystemUIFont"),
                            font_size: px(12.),
                            foreground: fg,
                            background: rgb(if dark { 0x2c2c2d } else { 0xe7e7e7 }).into(),
                            editor: self.editor.clone(),
                            navigation: self.navigation.clone(),
                        }),
                ),
        );
        let max_height = (editor_bounds.size.height / 4.).max(px(250.));
        let content_height = height - px(2.);
        height = height.min(max_height);
        let scroll = self.navigation.borrow().hover_scroll.clone();
        let scrollbar =
            hover_scrollbar(&self.navigation, &self.editor, height, content_height, dark);
        let mut element = div()
            .id("diagnostic-hover-card")
            .test_support()
            .role(Role::Tooltip)
            .aria_label(
                self.entries
                    .iter()
                    .map(|entry| format!("{}{}", entry.message, suffix(entry)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .relative()
            .occlude()
            .w(width)
            .h(height)
            .rounded(px(3.))
            .border_1()
            .border_color(border)
            .bg(rgb(if dark { 0x252526 } else { 0xf3f3f3 }))
            .text_color(fg)
            .overflow_hidden()
            // WebKit scrolls the hover's client box inside its 1px border,
            // independently of the content scrollbar. Its child retains the
            // full declared viewport height, creating the measured overflow.
            .child(
                div()
                    .id("diagnostic-hover-reveal")
                    .test_support()
                    .w(width - px(2.))
                    .h(height - px(2.))
                    .overflow_hidden()
                    .track_scroll(&document.reveal_scroll())
                    .child(
                        div()
                            .id("diagnostic-hover-scroll")
                            .test_support()
                            .w(width)
                            .h(height)
                            .line_height(px(25.))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&scroll)
                            .when(content_height > height, |this| {
                                this.on_scroll_wheel(|_, window, cx| {
                                    // The original consumes wheel input even at either
                                    // boundary; it must never scroll the editor below.
                                    window.refresh();
                                    cx.stop_propagation();
                                })
                            })
                            .child(document.wrap(
                                content.into_any_element(),
                                self.editor.clone(),
                                self.navigation.clone(),
                            )),
                    ),
            )
            .child(scrollbar)
            .into_any_element();
        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
        let x = anchor
            .left()
            .min(window.bounds().size.width - px(15.) - size.width)
            .max(px(15.));
        let y = if anchor.top() >= height {
            anchor.top() - height
        } else {
            anchor.bottom()
        };
        let bounds = Bounds::new(point(x, y), size);
        (
            div()
                .absolute()
                .size_0()
                .into_any_element()
                .request_layout(window, cx),
            HoverLayout {
                element: Some(element),
                bounds,
            },
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        layout: &mut HoverLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(element) = layout.element.take() {
            let mask = window.content_mask();
            self.editor.update(cx, |state, _| {
                state.set_diagnostic_popover_bounds(layout.bounds.intersect(&mask.bounds))
            });
            // Monaco's overlay escapes the editor but remains clipped by its
            // content page. Preserve that actual ancestor mask across the
            // deferred draw, including after resize and full-editor changes.
            window.defer_draw(element, layout.bounds.origin, 0, Some(mask));
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut HoverLayout,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

fn codicon(code: char, color: Hsla) -> AnyElement {
    let name = match code {
        '\u{ea87}' => "error",
        '\u{ea6c}' => "warning",
        '\u{ea9a}' => "next",
        '\u{eaa1}' => "previous",
        '\u{ea76}' => "close",
        _ => unreachable!(),
    };
    icon(&format!("diagnostic-{name}"))
        .size(px(16.))
        .text_color(color)
        .into_any_element()
}

fn hover_scrollbar(
    navigation: &NavigationState,
    editor: &Entity<EditorState>,
    viewport: Pixels,
    content: Pixels,
    dark: bool,
) -> AnyElement {
    if content <= viewport {
        return div().into_any_element();
    }
    let geometry = Rc::new(Cell::new((Bounds::<Pixels>::default(), px(0.), px(0.))));
    let mouse_geometry = geometry.clone();
    let mouse_nav = navigation.clone();
    let hover_nav = navigation.clone();
    let paint_nav = navigation.clone();
    let editor = editor.downgrade();
    div()
        .id("diagnostic-hover-scrollbar")
        .test_support()
        .absolute()
        .top_0()
        .right_0()
        .w(px(10.))
        .h(viewport)
        .occlude()
        .on_hover(move |hovered, window, _| {
            hover_nav.borrow_mut().hover_scrollbar_hover = *hovered;
            window.refresh();
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (bounds, thumb, limit) = mouse_geometry.get();
            if limit <= px(0.) {
                return;
            }
            let mut nav = mouse_nav.borrow_mut();
            let mut offset = -nav.hover_scroll.offset().y;
            let start = px((offset / limit * (viewport - thumb)).as_f32().round());
            if event.position.y < bounds.top() + start
                || event.position.y > bounds.top() + start + thumb
            {
                offset = ((event.position.y - bounds.top() - thumb / 2.) / (viewport - thumb)
                    * limit)
                    .clamp(px(0.), limit);
                offset = px(offset.as_f32().round());
                nav.hover_scroll.set_offset(point(px(0.), -offset));
            }
            nav.hover_drag = Some((event.position.y, offset));
            window.prevent_default();
            window.refresh();
            cx.stop_propagation();
        })
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let nav = paint_nav.borrow();
                    let limit = nav.hover_scroll.max_offset().y;
                    let content = viewport + limit;
                    let thumb = px((viewport * (viewport / content)).as_f32().floor())
                        .max(px(20.))
                        .min(viewport);
                    geometry.set((bounds, thumb, limit));
                    if limit <= px(0.) {
                        return;
                    }
                    let y = px(((-nav.hover_scroll.offset().y / limit).clamp(0., 1.)
                        * (viewport - thumb))
                        .as_f32()
                        .round());
                    let alpha = if nav.hover_drag.is_some() {
                        0xcc
                    } else if nav.hover_scrollbar_hover {
                        0xb3
                    } else {
                        0x66
                    };
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin + point(px(0.), y), size(px(10.), thumb)),
                        rgba((if dark { 0x79797900 } else { 0x64646400 }) | alpha),
                    ));
                    drop(nav);
                    // Listen on the window while dragging, as Monaco does, so
                    // releasing beyond the narrow track also ends the drag.
                    let move_nav = paint_nav.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if !phase.bubble() || event.pressed_button != Some(MouseButton::Left) {
                            return;
                        }
                        let nav = move_nav.borrow();
                        let Some((start, offset)) = nav.hover_drag else {
                            return;
                        };
                        let offset = (offset
                            + (event.position.y - start) / (viewport - thumb) * limit)
                            .clamp(px(0.), limit);
                        nav.hover_scroll
                            .set_offset(point(px(0.), -px(offset.as_f32().round())));
                        window.prevent_default();
                        window.refresh();
                        cx.stop_propagation();
                    });
                    let up_nav = paint_nav.clone();
                    let editor = editor.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                        if phase.bubble()
                            && event.button == MouseButton::Left
                            && up_nav.borrow_mut().hover_drag.take().is_some()
                        {
                            if !up_nav
                                .borrow()
                                .hover_scroll
                                .bounds()
                                .contains(&event.position)
                            {
                                if let Some(editor) = editor.upgrade() {
                                    editor
                                        .update(cx, |state, cx| state.clear_diagnostic_popover(cx));
                                }
                            }
                            window.prevent_default();
                            window.refresh();
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .size_full(),
        )
        .into_any_element()
}

// Monaco reserves six pixels for the other scrollbar even when it is hidden.
fn horizontal_scrollbar(
    navigation: &NavigationState,
    scroll_width: Pixels,
    dark: bool,
) -> AnyElement {
    let geometry = Rc::new(Cell::new((Bounds::<Pixels>::default(), px(0.), px(0.))));
    let paint_geometry = geometry.clone();
    let paint_nav = navigation.clone();
    let mouse_geometry = geometry.clone();
    let mouse_nav = navigation.clone();
    let move_geometry = geometry;
    let move_nav = navigation.clone();
    let up_nav = navigation.clone();
    let hover_nav = navigation.clone();
    div()
        .id("diagnostic-horizontal-scrollbar")
        .test_support()
        .absolute()
        .left_0()
        .right(px(6.))
        .bottom_0()
        .h(px(6.))
        .occlude()
        .on_hover(move |hovered, window, _| {
            hover_nav.borrow_mut().scrollbar_hover = *hovered;
            window.refresh();
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (bounds, thumb, limit) = mouse_geometry.get();
            if limit <= px(0.) || !bounds.contains(&event.position) {
                return;
            }
            let nav = mouse_nav.borrow();
            let mut offset = -nav.scroll.offset().x;
            let start = offset / limit * (bounds.size.width - thumb);
            if event.position.x < bounds.left() + start
                || event.position.x > bounds.left() + start + thumb
            {
                offset = ((event.position.x - bounds.left() - thumb / 2.)
                    / (bounds.size.width - thumb)
                    * limit)
                    .clamp(px(0.), limit);
                nav.scroll.set_offset(point(-offset, nav.scroll.offset().y));
            }
            drop(nav);
            mouse_nav.borrow_mut().scroll_drag = Some((event.position.x, offset));
            window.refresh();
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, window, cx| {
            let nav = move_nav.borrow();
            let Some((start, offset)) = nav.scroll_drag else {
                return;
            };
            if event.pressed_button != Some(MouseButton::Left) {
                return;
            }
            let (bounds, thumb, limit) = move_geometry.get();
            let offset = (offset
                + (event.position.x - start) / (bounds.size.width - thumb) * limit)
                .clamp(px(0.), limit);
            nav.scroll.set_offset(point(-offset, nav.scroll.offset().y));
            window.refresh();
            cx.stop_propagation();
        })
        .on_mouse_up(MouseButton::Left, move |_, window, _| {
            up_nav.borrow_mut().scroll_drag = None;
            window.refresh();
        })
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let viewport = bounds.size.width + px(6.);
                    let limit = (scroll_width - viewport).max(px(0.));
                    let thumb = px((bounds.size.width.as_f32() * viewport.as_f32()
                        / scroll_width.as_f32())
                    .floor())
                    .max(px(20.))
                    .min(bounds.size.width);
                    paint_geometry.set((bounds, thumb, limit));
                    if limit <= px(0.) {
                        return;
                    }
                    let nav = paint_nav.borrow();
                    let x = (-nav.scroll.offset().x / limit).clamp(0., 1.)
                        * (bounds.size.width - thumb);
                    let alpha = if nav.scroll_drag.is_some() {
                        0xcc
                    } else if nav.scrollbar_hover {
                        0xb3
                    } else {
                        0x66
                    };
                    let mut color =
                        Hsla::from(rgba((if dark { 0x79797900 } else { 0x64646400 }) | alpha));
                    if nav.stale {
                        color.a *= 0.6;
                    }
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(x, px(0.)),
                            size(thumb, bounds.size.height),
                        ),
                        color,
                    ));
                },
            )
            .size_full(),
        )
        .into_any_element()
}

fn render_navigation(
    editor: Entity<EditorState>,
    navigation: NavigationState,
    height: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let nav = navigation.borrow();
    let Some(marker) = nav.marker.as_ref() else {
        return Empty.into_any_element();
    };
    let (font, font_size) = editor_font(editor.read(cx), cx);
    let line_height = editor.read(cx).line_height().unwrap_or(px(21.));
    let arrow = px((line_height.as_f32() / 3.).round());
    let frame = px((line_height.as_f32() / 9.).round());
    let container_height = height - arrow * 2. - frame * 2. + px(2.);
    let header_height = px((line_height.as_f32() * 1.2).ceil());
    let dark = cx.theme().mode == ThemeMode::Dark;
    let fg: Hsla = rgb(if dark { 0xd4d4d4 } else { 0x000000 }).into();
    let warning = marker.severity == DiagnosticSeverity::Warning;
    let color: Hsla = rgb(match (dark, warning) {
        (false, false) => 0xe51400,
        (true, false) => 0xf14c4c,
        (false, true) => 0xbf8803,
        (true, true) => 0xcca700,
    })
    .into();
    let bg = rgb(if dark { 0x1e1e1e } else { 0xfffffe });
    let title = div()
        .flex()
        .items_baseline()
        .text_size(px(13.))
        .line_height(header_height)
        .font_family(".SystemUIFont")
        .child(
            div()
                .flex_shrink_0()
                .self_center()
                .mr(px(4.))
                .child(codicon(
                    if warning { '\u{ea6c}' } else { '\u{ea87}' },
                    color,
                )),
        )
        .child(div().child(nav.model.to_string()))
        .child(
            div()
                .ml(px(5.85))
                .text_size(px(11.7))
                .text_color(rgb(if dark { 0xcccccc } else { 0x616161 }))
                .child(format!(
                    "{} of {} problem{}",
                    nav.title_index,
                    nav.title_count,
                    if nav.title_count > 1 { "s" } else { "" }
                )),
        );
    let mut actions = div().ml_auto().flex().items_center().pr(px(2.));
    for (id, code, forward) in [
        ("diagnostic-next", '\u{ea9a}', Some(true)),
        ("diagnostic-previous", '\u{eaa1}', Some(false)),
        ("diagnostic-close", '\u{ea76}', None),
    ] {
        let editor = editor.downgrade();
        let navigation = navigation.clone();
        actions = actions.child(
            div()
                .id(id)
                .test_support()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .rounded(px(5.))
                .hover(|style| style.bg(rgba(if dark { 0x5a5d5e50 } else { 0xb8b8b850 })))
                .child(codicon(code, fg))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(editor) = editor.upgrade() {
                        editor.update(cx, |state, cx| {
                            if let Some(forward) = forward {
                                navigate(state, &navigation, forward, None, window, cx);
                            } else {
                                dismiss(state, &navigation, cx);
                                state.focus(window, cx);
                            }
                        });
                    }
                }),
        );
    }
    let body_height = container_height - px(3.) - header_height;
    let mut body_font = font.clone();
    body_font.style = if nav.stale {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let body_color = if nav.stale {
        Hsla {
            a: fg.a * 0.6,
            ..fg
        }
    } else {
        fg
    };
    let paragraphs: Vec<_> = marker.message.split('\n').collect();
    let content_width = paragraphs
        .iter()
        .enumerate()
        .map(|(i, line)| {
            text_width(line, &body_font, font_size, body_color, window)
                + if i + 1 == paragraphs.len() {
                    shape(&suffix(marker), &body_font, font_size, body_color, window).width + px(6.)
                } else {
                    px(0.)
                }
        })
        .max()
        .unwrap_or(px(0.));
    let lines = message_lines(
        marker,
        px(f32::MAX),
        &body_font,
        font_size,
        body_color,
        window,
    );
    let message = MessageText {
        id: "diagnostic-navigation-text".into(),
        order: 1,
        document: None,
        inline_selection: false,
        text: format!("{}{}", marker.message, suffix(marker)).into(),
        lines,
        size: size(
            content_width,
            line_height * marker.message.split('\n').count().max(1),
        ),
        line_height,
        font: body_font,
        font_size,
        foreground: body_color,
        background: bg.into(),
        editor: editor.clone(),
        navigation: navigation.clone(),
    };
    let scroll_width = px((shape("ｍ", &font, font_size, fg, window).width.as_f32()
        * (marker
            .message
            .split('\n')
            .map(|line| line.encode_utf16().count())
            .max()
            .unwrap_or(0)
            + suffix(marker).len()) as f32
        * 0.75)
        .ceil());
    let scrollbar = horizontal_scrollbar(&navigation, scroll_width, dark);
    let body = div()
        .id("diagnostic-navigation-body")
        .test_support()
        .relative()
        .w_full()
        .h(body_height)
        .border_t_1()
        .border_color(color)
        .bg(bg)
        .overflow_hidden()
        .child(
            div()
                .id("diagnostic-navigation-message-scroll")
                .size_full()
                .overflow_x_scroll()
                .overflow_y_scroll()
                .track_scroll(&nav.scroll)
                .child(
                    div()
                        .w(scroll_width)
                        .pt(px(8.))
                        .pl(px(20.))
                        .pr(px(12.))
                        .child(message),
                ),
        )
        .child(scrollbar);
    let state = editor.read(cx);
    let after = state
        .view_zone_after()
        .unwrap_or(state.cursor())
        .min(state.text().len());
    let buffer_point = state.text().offset_to_point(after);
    // Monaco nudges an arrow at column 1 to column 2; it stays attached to
    // the zone's tracked model position when the user moves the caret.
    let arrow_offset = if buffer_point.column == 0 {
        after
            + state
                .text()
                .slice(after..)
                .chars()
                .next()
                .filter(|ch| ch.len_utf16() == 1)
                .map_or(0, char::len_utf8)
    } else {
        after
    };
    let cursor_x = state
        .range_to_bounds(&(arrow_offset..arrow_offset))
        .map(|bounds| bounds.left() - state.input_bounds().left())
        .unwrap_or(px(0.));

    div()
        .id("diagnostic-navigation-zone")
        .test_support()
        .role(Role::Tooltip)
        .aria_label(format!(
            "{}\n{} of {} problem{}\n{}{}",
            nav.model,
            nav.title_index,
            nav.title_count,
            if nav.title_count > 1 { "s" } else { "" },
            marker.message,
            suffix(marker)
        ))
        .relative()
        .w_full()
        .h(height)
        .occlude()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let x = bounds.left() + cursor_x.max(arrow);
                    let mut path = PathBuilder::fill();
                    path.move_to(point(x, bounds.top()));
                    path.line_to(point(x - arrow, bounds.top() + arrow));
                    path.line_to(point(x + arrow, bounds.top() + arrow));
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(arrow),
        )
        .child(
            div()
                .absolute()
                .top(arrow)
                .left_0()
                .right_0()
                .h(container_height)
                .border_y_1()
                .border_color(color)
                .child(
                    div()
                        .w_full()
                        .h(header_height)
                        .flex()
                        .items_center()
                        .pl(px(20.))
                        .bg(Hsla { a: 0.1, ..color })
                        .text_color(fg)
                        .child(title)
                        .child(actions),
                )
                .child(body),
        )
        .into_any_element()
}
