//! Selection geometry for the already-shaped Monaco message fragments.
//! Soft wraps and inline padding are visual; copying uses original byte ranges.
use super::*;
use gpui_base::{
    TextSelection, TextSelectionContentKey, TextSelectionHandle, TextSelectionRegistration,
    TextSelectionRun, TextSelectionSnapshot,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation as _;

pub(super) struct Part {
    pub x: Pixels,
    pub line: ShapedLine,
    pub source: Range<usize>,
    pub selection_opacity: f32,
    pub tab: bool,
}

#[derive(Default)]
struct Geometry {
    text: SharedString,
    origin: Point<Pixels>,
    runs: Vec<TextSelectionRun>,
    sources: Vec<Range<usize>>,
    snapshot: Option<TextSelectionSnapshot>,
    footer_start: Option<usize>,
    tail: Option<Tail>,
}
impl Geometry {
    fn selected_text(&self, owner: EntityId) -> String {
        let Some(snapshot) = self.snapshot else {
            return String::new();
        };
        if snapshot.anchor().entity_id() == Some(owner)
            && snapshot.cursor().entity_id() == Some(owner)
            && let (Some(anchor), Some(cursor)) = (
                snapshot.anchor().content_key(),
                snapshot.cursor().content_key(),
            )
        {
            let (anchor, cursor) = (anchor.value() as usize, cursor.value() as usize);
            if let Some(text) = self.text.get(anchor.min(cursor)..anchor.max(cursor)) {
                return text.to_owned();
            }
        }
        let projection = snapshot.project_runs(&self.runs);
        let mut start = usize::MAX;
        let mut end = 0;
        for (range, source) in projection.ranges().iter().zip(&self.sources) {
            if let Some(range) = range {
                start = start.min(source.start + range.start);
                end = end.max(source.start + range.end);
            }
        }
        self.text.get(start..end).unwrap_or_default().to_owned()
    }

    fn offset_at(&self, content_point: Point<Pixels>) -> Option<usize> {
        let position = content_point + self.origin;
        let distance = |run: &TextSelectionRun| {
            let bounds = run.bounds();
            let dx = (bounds.left() - position.x)
                .max(position.x - bounds.right())
                .max(px(0.));
            let dy = (bounds.top() - position.y)
                .max(position.y - bounds.bottom())
                .max(px(0.));
            (f32::from(dy), f32::from(dx))
        };
        let (run, source) = self
            .runs
            .iter()
            .zip(&self.sources)
            .min_by(|(a, _), (b, _)| {
                let (ay, ax) = distance(a);
                let (by, bx) = distance(b);
                ay.total_cmp(&by).then(ax.total_cmp(&bx))
            })?;
        let index = if run.text().as_ref() == "\t" {
            usize::from(position.x >= run.bounds().center().x)
        } else {
            run.layout()
                .line_layouts()
                .first()?
                .closest_index_for_position(
                    position - run.layout().bounds().origin,
                    run.layout().line_height(),
                )
                .unwrap_or_else(|index| index)
        };
        Some(source.start + index.min(source.len()))
    }

    fn point_at(&self, offset: usize) -> Option<Point<Pixels>> {
        if self.footer_start.is_some() && offset == self.text.len() {
            let last = self.runs.last()?.bounds();
            return Some(last.bottom_right() + point(px(0.), last.size.height / 2.) - self.origin);
        }
        let (run, source) = self
            .runs
            .iter()
            .zip(&self.sources)
            .rev()
            .find(|(_, source)| source.start <= offset && offset <= source.end)
            .or_else(|| {
                self.runs
                    .iter()
                    .zip(&self.sources)
                    .find(|(_, source)| source.start >= offset)
            })?;
        let index = offset.saturating_sub(source.start).min(source.len());
        let position = if run.text().as_ref() == "\t" {
            point(
                if index == 0 {
                    run.bounds().left()
                } else {
                    run.bounds().right()
                },
                run.bounds().top(),
            )
        } else {
            run.layout().position_for_index(index)?
        };
        Some(position + point(px(0.), run.bounds().size.height / 2.) - self.origin)
    }

    fn horizontal_target(&mut self, offset: usize, forward: bool, word: bool) -> usize {
        let mut next = horizontal_offset(&self.text, offset, forward, word);
        if let Some(footer) = self.footer_start {
            let end = self.text.len();
            if word {
                // WebKit stops at the nonselectable action link before the
                // footer. Moving by character can cross this boundary back.
                if forward && offset < footer {
                    next = next.min(footer);
                }
                if !forward && offset >= footer {
                    next = next.max(footer);
                }
            }
            if offset == end && !word && !forward && self.tail == Some(Tail::Vertical) {
                self.tail = Some(Tail::Horizontal);
                return end;
            }
            if forward && next == end {
                // Two empty scrollbar nodes are distinct DOM caret stops but
                // copy the same final newline. Word movement skips the first.
                self.tail = Some(if word || offset == end {
                    Tail::Vertical
                } else {
                    Tail::Horizontal
                });
                return end;
            }
        }
        self.tail = None;
        next
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tail {
    Horizontal,
    Vertical,
}

struct Retained {
    selection: TextSelectionHandle,
    focus: FocusHandle,
    geometry: Rc<RefCell<Geometry>>,
    _subscription: Subscription,
    reveal_scroll: ScrollHandle,
}
struct LayoutPart {
    element: AnyElement,
    layout: TextLayout,
    origin: Point<Pixels>,
    bounds: Bounds<Pixels>,
}
pub(super) struct Layout {
    element: AnyElement,
    retained: Rc<Retained>,
    parts: Vec<LayoutPart>,
}

pub(super) struct MessageText {
    pub id: ElementId,
    pub order: u64,
    pub document: Option<(Document, usize)>,
    pub inline_selection: bool,
    pub text: SharedString,
    pub lines: Vec<Vec<Part>>,
    pub size: gpui::Size<Pixels>,
    pub line_height: Pixels,
    pub font: Font,
    pub font_size: Pixels,
    pub foreground: Hsla,
    pub background: Hsla,
    pub editor: Entity<EditorState>,
    pub navigation: NavigationState,
}
impl IntoElement for MessageText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
gpui::actions!(
    rdm_diagnostic_message,
    [
        NextProblem,
        PreviousProblem,
        ExtendLeft,
        ExtendRight,
        ExtendWordLeft,
        ExtendWordRight,
        KeepSelection,
    ]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-left", ExtendLeft, Some("RdmDiagnosticMessage")),
        KeyBinding::new("shift-right", ExtendRight, Some("RdmDiagnosticMessage")),
        KeyBinding::new("cmd-a", KeepSelection, Some("RdmDiagnosticMessage")),
        KeyBinding::new("alt-f8", NextProblem, Some("RdmDiagnosticMessage")),
        KeyBinding::new(
            "alt-shift-f8",
            PreviousProblem,
            Some("RdmDiagnosticMessage"),
        ),
        KeyBinding::new("shift-escape", CancelDialog, Some("RdmDiagnosticMessage")),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new(
            "alt-shift-left",
            ExtendWordLeft,
            Some("RdmDiagnosticMessage"),
        ),
        KeyBinding::new(
            "alt-shift-right",
            ExtendWordRight,
            Some("RdmDiagnosticMessage"),
        ),
    ]);
}

fn horizontal_offset(text: &str, offset: usize, forward: bool, word: bool) -> usize {
    if word {
        let ranges = word_ranges(text);
        if forward {
            ranges
                .iter()
                .map(|word| word.end)
                .find(|end| *end > offset)
                .unwrap_or(text.len())
        } else {
            ranges
                .iter()
                .map(|word| word.start)
                .take_while(|start| *start < offset)
                .last()
                .unwrap_or(0)
        }
    } else if forward {
        text.grapheme_indices(true)
            .map(|(start, _)| start)
            .find(|start| *start > offset)
            .unwrap_or(text.len())
    } else {
        text.grapheme_indices(true)
            .map(|(start, _)| start)
            .take_while(|start| *start < offset)
            .last()
            .unwrap_or(0)
    }
}

#[cfg(target_os = "macos")]
fn word_ranges(text: &str) -> Vec<Range<usize>> {
    use objc2::runtime::Bool;
    use objc2_foundation::{NSRange, NSString, NSStringEnumerationOptions};
    use std::ptr::NonNull;

    // Foundation supplies macOS word segmentation, including dictionary word
    // boundaries for Chinese. UAX #29 alone splits the source fixture's 中文.
    let mut utf16 = 0;
    let mut boundaries = vec![(0, 0)];
    for (offset, character) in text.char_indices() {
        utf16 += character.len_utf16();
        boundaries.push((utf16, offset + character.len_utf8()));
    }
    let ranges = Rc::new(RefCell::new(Vec::new()));
    let collected = ranges.clone();
    let block = block2::StackBlock::new(
        move |_: *mut NSString, range: NSRange, _: NSRange, _: NonNull<Bool>| {
            let start = boundaries
                .binary_search_by_key(&range.location, |(index, _)| *index)
                .ok();
            let end = boundaries
                .binary_search_by_key(&(range.location + range.length), |(index, _)| *index)
                .ok();
            if let (Some(start), Some(end)) = (start, end) {
                collected
                    .borrow_mut()
                    .push(boundaries[start].1..boundaries[end].1);
            }
        },
    );
    NSString::from_str(text).enumerateSubstringsInRange_options_usingBlock(
        NSRange::new(0, utf16),
        NSStringEnumerationOptions::ByWords | NSStringEnumerationOptions::SubstringNotRequired,
        &block,
    );
    drop(block);
    let result = ranges.borrow().clone();
    result
}

#[cfg(not(target_os = "macos"))]
fn word_ranges(text: &str) -> Vec<Range<usize>> {
    text.unicode_word_indices()
        .map(|(start, word)| start..start + word.len())
        .collect()
}

fn extend(retained: &Retained, forward: bool, word: bool, window: &mut Window, cx: &mut App) {
    let Some((_, cursor)) = retained.selection.selection_endpoints(window, cx) else {
        return;
    };
    let target = {
        let mut geometry = retained.geometry.borrow_mut();
        // WebKit's noneditable diagnostic stops extending once its DOM
        // selection collapses; it does not behave like an editable caret.
        if geometry
            .selected_text(retained.selection.entity_id())
            .is_empty()
        {
            return;
        }
        let offset = cursor
            .content_key()
            .map(|key| key.value() as usize)
            .or_else(|| geometry.offset_at(cursor.content_point()));
        let Some(offset) = offset else {
            return;
        };
        let next = geometry.horizontal_target(offset, forward, word);
        if geometry.tail == Some(Tail::Horizontal) {
            // The original hidden-overflow hover reveals its horizontal
            // scrollbar node. Let the layout's actual overflow set the offset.
            retained.reveal_scroll.scroll_to_bottom();
        }
        geometry.point_at(next).map(|point| (next, point))
    };
    if let Some((offset, point)) = target {
        retained.selection.extend_to(
            point,
            Some(TextSelectionContentKey::new(offset as u64)),
            window,
            cx,
        );
        window.refresh();
    }
}
fn retain(
    id: &GlobalElementId,
    text: &SharedString,
    window: &mut Window,
    cx: &mut App,
) -> Rc<Retained> {
    window.with_element_state(id, |old: Option<Rc<Retained>>, window| {
        let retained = old.unwrap_or_else(|| {
            let selection = TextSelectionHandle::new(text.to_string(), cx);
            let focus = cx.focus_handle();
            let focus_on_select = focus.clone();
            selection.focus_with(move |window, cx| focus_on_select.focus(window, cx), cx);
            let geometry = Rc::new(RefCell::new(Geometry::default()));
            let copy = geometry.clone();
            let owner = selection.entity_id();
            selection.copy_with(move |_| copy.borrow().selected_text(owner), cx);
            let offsets = geometry.clone();
            selection.resolve_content_key_with(
                move |point, _| {
                    offsets
                        .borrow()
                        .offset_at(point)
                        .map(|offset| TextSelectionContentKey::new(offset as u64))
                },
                cx,
            );
            let selected = geometry.clone();
            let handle = window.window_handle();
            let subscription = selection.subscribe(
                move |event, cx| {
                    if let gpui_base::TextSelectionEvent::SelectionChanged(snapshot) = event {
                        selected.borrow_mut().snapshot = *snapshot;
                        let _ = handle.update(cx, |_, window, _| window.refresh());
                    }
                },
                cx,
            );
            Rc::new(Retained {
                selection,
                focus,
                geometry,
                _subscription: subscription,
                reveal_scroll: ScrollHandle::new(),
            })
        });
        (retained.clone(), retained)
    })
}

fn interactive<E: InteractiveElement + Styled>(
    element: E,
    retained: &Rc<Retained>,
    editor: &Entity<EditorState>,
    navigation: &NavigationState,
) -> E {
    let editor = editor.downgrade();
    let navigation = navigation.clone();
    let next_editor = editor.clone();
    let next_navigation = navigation.clone();
    let previous_editor = editor.clone();
    let previous_navigation = navigation.clone();
    let save_editor = editor.clone();
    let extend_left = retained.clone();
    let extend_right = retained.clone();
    let word_left = retained.clone();
    let word_right = retained.clone();
    element
        .track_focus(&retained.focus)
        .key_context("RdmDiagnosticMessage")
        .cursor_text()
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_action(move |_: &ExtendLeft, window, cx| extend(&extend_left, false, false, window, cx))
        .on_action(move |_: &ExtendRight, window, cx| {
            extend(&extend_right, true, false, window, cx)
        })
        .on_action(move |_: &ExtendWordLeft, window, cx| {
            extend(&word_left, false, true, window, cx)
        })
        .on_action(move |_: &ExtendWordRight, window, cx| {
            extend(&word_right, true, true, window, cx)
        })
        .on_action(|_: &KeepSelection, _, cx| cx.stop_propagation())
        .on_action(|_: &gpui_base::input::Copy, window, cx| {
            let text = TextSelection::selected_text(window, cx);
            if text.is_empty() {
                cx.propagate();
            } else {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        })
        .on_action(move |_: &SaveValue, window, cx| {
            if let Some(editor) = save_editor.upgrade() {
                window.dispatch_action(
                    Box::new(SaveFromMessage {
                        editor: editor.entity_id().as_u64(),
                    }),
                    cx,
                );
            }
            cx.stop_propagation();
        })
        .on_action(move |_: &CancelDialog, window, cx| {
            TextSelection::clear(window, cx);
            if let Some(editor) = editor.upgrade() {
                editor.update(cx, |state, cx| {
                    state.clear_diagnostic_popover(cx);
                    dismiss(state, &navigation, cx);
                    state.focus(window, cx);
                });
            }
            cx.stop_propagation();
        })
        .on_action(move |_: &NextProblem, window, cx| {
            TextSelection::clear(window, cx);
            if let Some(editor) = next_editor.upgrade() {
                editor.update(cx, |state, cx| {
                    navigate(state, &next_navigation, true, None, window, cx)
                });
            }
        })
        .on_action(move |_: &PreviousProblem, window, cx| {
            TextSelection::clear(window, cx);
            if let Some(editor) = previous_editor.upgrade() {
                editor.update(cx, |state, cx| {
                    navigate(state, &previous_navigation, false, None, window, cx)
                });
            }
        })
}

impl Element for MessageText {
    type RequestLayoutState = Layout;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
    ) -> (LayoutId, Layout) {
        let retained = if let Some((document, _)) = &self.document {
            document.0.clone()
        } else {
            let retained = retain(
                id.expect("diagnostic message has an id"),
                &self.text,
                window,
                cx,
            );
            retained.geometry.borrow_mut().text = self.text.clone();
            retained
        };
        let element = div()
            .id(("diagnostic-message-layout", self.order as usize))
            .test_support()
            .w(self.size.width)
            .h(self.size.height)
            .flex_shrink_0();
        let mut element = if self.document.is_some() {
            element.cursor_text().into_any_element()
        } else {
            interactive(element, &retained, &self.editor, &self.navigation).into_any_element()
        };
        let layout = element.request_layout(window, cx);
        let mut parts = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            for part in line {
                let display = if part.tab {
                    " ".into()
                } else {
                    part.line.text.clone()
                };
                let measured_font_size = if part.tab {
                    let space =
                        shape(" ", &self.font, self.font_size, self.foreground, window).width;
                    self.font_size * (part.line.width / space)
                } else {
                    self.font_size
                };
                let styled = StyledText::new(display).with_runs(vec![TextRun {
                    len: part.line.text.len(),
                    font: self.font.clone(),
                    color: transparent_black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }]);
                let text_layout = styled.layout().clone();
                let mut styled = styled.into_any_element();
                window.with_text_style(
                    Some(TextStyleRefinement {
                        font_size: Some(measured_font_size.into()),
                        line_height: Some(self.line_height.into()),
                        white_space: Some(WhiteSpace::Nowrap),
                        ..Default::default()
                    }),
                    |window| {
                        styled.layout_as_root(AvailableSpace::min_size(), window, cx);
                    },
                );
                parts.push(LayoutPart {
                    element: styled,
                    layout: text_layout,
                    origin: point(part.x, self.line_height * row),
                    bounds: Bounds::default(),
                });
            }
        }
        (
            layout,
            Layout {
                element,
                retained,
                parts,
            },
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Layout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.document.is_none() {
            layout.retained.geometry.borrow_mut().origin = bounds.origin;
        }
        layout.element.prepaint(window, cx);
        let mut text_bounds = Vec::new();
        for (part, source) in layout.parts.iter_mut().zip(self.lines.iter().flatten()) {
            let origin = bounds.origin + part.origin;
            window.with_absolute_element_offset(origin, |window| part.element.prepaint(window, cx));
            part.bounds = Bounds::new(origin, size(source.line.width, self.line_height));
            text_bounds.push(part.bounds);
        }
        let runs = message_runs(&layout.parts, &self.lines);
        let base = self.document.as_ref().map_or(0, |(_, base)| *base);
        let mut geometry = layout.retained.geometry.borrow_mut();
        let sources = self
            .lines
            .iter()
            .flatten()
            .map(|part| (part.source.start + base)..(part.source.end + base));
        if self.document.is_none() {
            geometry.runs = runs;
            geometry.sources = sources.collect();
        } else {
            geometry.runs.extend(runs);
            geometry.sources.extend(sources);
        }
        drop(geometry);
        if self.document.is_none() {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let registration = TextSelectionRegistration::new(hitbox, bounds)
                .with_document_order(self.order)
                .with_text_bounds(text_bounds)
                .with_rendered_element(&layout.retained.selection, window, cx);
            layout.retained.selection.register(registration, window, cx);
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        layout: &mut Layout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        layout.element.paint(window, cx);
        let runs = message_runs(&layout.parts, &self.lines);
        let snapshot = layout.retained.selection.snapshot(cx);
        let ranges = if self.document.is_none() {
            layout
                .retained
                .selection
                .update_runs(&runs, cx)
                .ranges()
                .to_vec()
        } else {
            snapshot
                .map(|snapshot| snapshot.project_runs(&runs).ranges().to_vec())
                .unwrap_or_else(|| vec![None; runs.len()])
        };
        layout.retained.geometry.borrow_mut().snapshot = snapshot;
        let space = shape(" ", &self.font, self.font_size, self.foreground, window).width;
        for (index, ((part, source), selected)) in layout
            .parts
            .iter()
            .zip(self.lines.iter().flatten())
            .zip(&ranges)
            .enumerate()
        {
            if let Some(range) = selected {
                let start = if source.tab {
                    px(0.)
                } else {
                    source.line.x_for_index(range.start)
                };
                let mut end = if source.tab {
                    source.line.width
                } else {
                    source.line.x_for_index(range.end)
                };
                let row_end = layout
                    .parts
                    .get(index + 1)
                    .is_none_or(|next| next.origin.y != part.origin.y);
                let continues = snapshot.is_some_and(|snapshot| {
                    matches!(
                        snapshot.coverage(),
                        gpui_base::TextSelectionCoverage::Full
                            | gpui_base::TextSelectionCoverage::ToEnd
                    ) || snapshot.window_points().is_some_and(|points| {
                        points.anchor().y.max(points.cursor().y) >= part.bounds.bottom()
                    })
                });
                // Chromium paints a space-sized cap for a selected visual line
                // break and for a selected block separator. It is not copied.
                if !self.inline_selection
                    && row_end
                    && range.end == source.line.text.len()
                    && continues
                {
                    end += space;
                }
                let mut selection_color: Hsla = rgba(0x80BCFF99).into();
                selection_color.a *= source.selection_opacity;
                let (selection_top, selection_height) = if self.inline_selection {
                    // A shaped line normalizes descent and includes fallback
                    // faces. Raw FontKit metrics retain a negative descent.
                    let height = (source.line.ascent + source.line.descent).ceil();
                    (((self.line_height - height) / 2.).floor(), height)
                } else {
                    (px(0.), self.line_height)
                };
                window.paint_quad(fill(
                    Bounds::new(
                        part.bounds.origin + point(start, selection_top),
                        size(end - start, selection_height),
                    ),
                    selection_color,
                ));
            }
            // CSS applies inline opacity to the composed glyph and selection
            // group. Precomposing its foreground against the normal surface
            // keeps a selected translucent suffix from being tinted twice.
            if source.tab {
                continue;
            }
            let composed;
            let line = if source.selection_opacity < 1. {
                composed = shape(
                    &source.line.text,
                    &self.font,
                    self.font_size,
                    self.background.blend(Hsla {
                        a: source.selection_opacity,
                        ..self.foreground
                    }),
                    window,
                );
                &composed
            } else {
                &source.line
            };
            let _ = line.paint(
                part.bounds.origin,
                self.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}

fn message_runs(parts: &[LayoutPart], lines: &[Vec<Part>]) -> Vec<TextSelectionRun> {
    parts
        .iter()
        .zip(lines.iter().flatten())
        .enumerate()
        .map(|(i, (part, source))| {
            TextSelectionRun::new(source.line.text.clone(), part.layout.clone(), part.bounds)
                .with_document_order(i as u64)
        })
        .collect()
}

/// One noneditable DOM selection spans the hover's message rows and footer.
/// The action link is deliberately absent from both the runs and copy text.
#[derive(Clone)]
pub(super) struct Document(Rc<Retained>);
impl Document {
    pub fn new(
        id: &GlobalElementId,
        text: SharedString,
        footer_start: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let retained = retain(id, &text, window, cx);
        let mut geometry = retained.geometry.borrow_mut();
        geometry.text = text;
        geometry.footer_start = Some(footer_start);
        drop(geometry);
        Self(retained)
    }
    pub fn reveal_scroll(&self) -> ScrollHandle {
        self.0.reveal_scroll.clone()
    }
    pub fn wrap(
        &self,
        content: AnyElement,
        editor: Entity<EditorState>,
        navigation: NavigationState,
    ) -> DocumentRoot {
        DocumentRoot {
            document: self.clone(),
            content: Some(content),
            editor,
            navigation,
        }
    }
}
pub(super) struct DocumentRoot {
    document: Document,
    content: Option<AnyElement>,
    editor: Entity<EditorState>,
    navigation: NavigationState,
}
impl IntoElement for DocumentRoot {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for DocumentRoot {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some("diagnostic-hover-document".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut element = interactive(
            div().id("diagnostic-document-layout").test_support(),
            &self.document.0,
            &self.editor,
            &self.navigation,
        )
        .child(self.content.take().expect("hover content is laid out once"))
        .into_any_element();
        let layout = element.request_layout(window, cx);
        (layout, element)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        element: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self
            .editor
            .update(cx, |state, _| state.take_diagnostic_hover_focus_request())
        {
            self.document.0.focus.focus(window, cx);
        }
        {
            let mut geometry = self.document.0.geometry.borrow_mut();
            geometry.origin = bounds.origin;
            geometry.runs.clear();
            geometry.sources.clear();
        }
        element.prepaint(window, cx);
        let text_bounds = self
            .document
            .0
            .geometry
            .borrow()
            .runs
            .iter()
            .map(TextSelectionRun::bounds)
            .collect();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox, bounds)
            .with_document_order(1)
            .with_text_bounds(text_bounds)
            .with_rendered_element(&self.document.0.selection, window, cx);
        self.document.0.selection.register(registration, window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        element: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        {
            let mut geometry = self.document.0.geometry.borrow_mut();
            self.document.0.selection.update_runs(&geometry.runs, cx);
            geometry.snapshot = self.document.0.selection.snapshot(cx);
        }
        element.paint(window, cx);
    }
}
