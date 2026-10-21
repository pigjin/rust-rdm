use super::*;
use crate::cli::terminal::{Frame, Point as CellPoint};
use std::rc::Rc;

pub(super) struct CliView {
    pub bounds: Bounds<Pixels>,
    pub cell_width: f32,
    pub line_height: f32,
    pub visible_rows: usize,
    pub scroll_back: usize,
    pub frame: Option<Rc<Frame>>,
    pub selection: Option<(CellPoint, CellPoint)>,
    selecting: bool,
    selection_mode: u8,
    selection_anchor: Option<(CellPoint, CellPoint)>,
    lines: Vec<cli::Line>,
    prefix: String,
    input: String,
    cursor: usize,
    dimensions: (u16, u16),
    dirty: bool,
    caret_epoch: Instant,
}
impl Default for CliView {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            cell_width: 8.4,
            line_height: 16.,
            visible_rows: 42,
            scroll_back: 0,
            frame: None,
            selection: None,
            selecting: false,
            selection_mode: 1,
            selection_anchor: None,
            lines: vec![],
            prefix: String::new(),
            input: String::new(),
            cursor: 0,
            dimensions: (0, 0),
            dirty: true,
            caret_epoch: Instant::now(),
        }
    }
}
impl CliView {
    fn first_row(&self) -> usize {
        self.frame.as_ref().map_or(0, |frame| {
            frame
                .rows
                .len()
                .saturating_sub(self.visible_rows + self.scroll_back)
        })
    }
    fn position(&self, position: gpui::Point<Pixels>) -> CellPoint {
        CellPoint {
            row: self.first_row()
                + ((position.y - self.bounds.origin.y).as_f32() / self.line_height)
                    .floor()
                    .max(0.) as usize,
            col: ((position.x - self.bounds.origin.x - px(10.)).as_f32() / self.cell_width)
                .round()
                .max(0.) as usize,
        }
    }
    fn cell_position(&self, position: gpui::Point<Pixels>) -> CellPoint {
        CellPoint {
            row: self.first_row()
                + ((position.y - self.bounds.origin.y).as_f32() / self.line_height)
                    .floor()
                    .max(0.) as usize,
            col: ((position.x - self.bounds.origin.x - px(10.)).as_f32() / self.cell_width)
                .floor()
                .max(0.) as usize,
        }
    }
    pub fn selected_text(&self) -> Option<String> {
        let (first, last) = self.selection?;
        (first != last)
            .then(|| {
                self.frame
                    .as_ref()
                    .map(|frame| frame.selected_text(first, last))
            })
            .flatten()
    }
}

fn color(value: vt100::Color, background: bool, bold: bool) -> Hsla {
    const PALETTE: [u32; 16] = [
        0x2e3436, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf, 0x555753,
        0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
    ];
    match value {
        vt100::Color::Default => rgb(if background { 0 } else { 0xffffff }).into(),
        vt100::Color::Rgb(r, g, b) => {
            rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into()
        }
        vt100::Color::Idx(index) => {
            let index = if bold && !background && index < 8 {
                index + 8
            } else {
                index
            };
            let value = if index < 16 {
                PALETTE[usize::from(index)]
            } else if index < 232 {
                let index = index - 16;
                let component = |v: u8| if v == 0 { 0 } else { u32::from(v) * 40 + 55 };
                (component(index / 36) << 16)
                    | (component((index / 6) % 6) << 8)
                    | component(index % 6)
            } else {
                let grey = u32::from(index - 232) * 10 + 8;
                (grey << 16) | (grey << 8) | grey
            };
            rgb(value).into()
        }
    }
}

impl RdmApp {
    pub(super) fn render_cli(&self, i: usize, cx: &Context<Self>) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let view = tab.cli_view.clone();
        let input = text_input(&tab.cli_input, cx);
        let cursor = tab.cli_input.read(cx).cursor();
        let prefix = format!("{}:db{}> ", tab.conn.name, tab.cli_db);
        {
            let mut state = view.borrow_mut();
            if state.lines != tab.cli_output
                || state.input != input
                || state.prefix != prefix
                || state.cursor != cursor
            {
                state.lines = tab.cli_output.clone();
                state.input = input;
                state.prefix = prefix;
                state.cursor = cursor;
                state.dirty = true;
                state.caret_epoch = Instant::now();
            }
        }
        let font = self.preference_font("cli", "Courier New", cx);
        let container_font = font.clone();
        let font_size = px(self.cli_font_size());
        let line_height = view.borrow().line_height;
        let layout_view = view.clone();
        let layout_font = font.clone();
        let focus = tab.cli_input.focus_handle(cx);
        let cursor_style = self.pref.cli.cursor_style.clone();
        let drawing = canvas(
            move |bounds, window, _| {
                let font_id = window.text_system().resolve_font(&layout_font);
                let run = TextRun {
                    len: 1,
                    font: layout_font,
                    color: rgb(0xffffff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let measured_width = window
                    .text_system()
                    .shape_line("W".into(), font_size, &[run], None)
                    .width
                    .as_f32()
                    .max(1.);
                // xterm's DOM strategy measures 32 W characters using offsetWidth.
                let char_width = (measured_width * 32.).round() / 32.;
                let line_height = (window
                    .text_system()
                    .ascent(font_id, font_size)
                    .as_f32()
                    .round()
                    - window
                        .text_system()
                        .descent(font_id, font_size)
                        .as_f32()
                        .round())
                .max(1.);
                // Preserve the source's fractional tab height when GPUI snaps its layout.
                let rounding = CONTENT_TABS_HEIGHT - CONTENT_TABS_HEIGHT.round();
                let mut state = layout_view.borrow_mut();
                let prior_rows = if state.dimensions.0 == 0 {
                    24
                } else {
                    state.dimensions.0
                };
                let prior_cols = if state.dimensions.1 == 0 {
                    80
                } else {
                    state.dimensions.1
                };
                let fitting_cell_width =
                    (f64::from(char_width) * f64::from(prior_cols)).round() / f64::from(prior_cols);
                let fitting_height = (bounds.size.height.as_f32() - rounding)
                    .max(f32::from(prior_rows) * line_height);
                let rows = (fitting_height / line_height).floor().max(1.) as u16;
                let cols = (f64::from(bounds.size.width.as_f32() - 25.) / fitting_cell_width)
                    .floor()
                    .max(2.) as u16;
                let cell_width = (char_width * f32::from(cols)).round() / f32::from(cols);
                state.bounds = bounds;
                state.cell_width = cell_width;
                state.line_height = line_height;
                state.visible_rows = usize::from(rows);
                if state.dirty || state.dimensions != (rows, cols) {
                    state.frame = Some(Rc::new(Frame::console(
                        &state.lines,
                        &state.prefix,
                        &state.input,
                        state.cursor,
                        rows,
                        cols,
                    )));
                    state.dimensions = (rows, cols);
                    state.dirty = false;
                }
                let frame = state.frame.as_ref().unwrap().clone();
                let first = state.first_row();
                (
                    frame,
                    first,
                    state.selection,
                    cell_width,
                    line_height,
                    state.caret_epoch,
                )
            },
            move |bounds,
                  (frame, first, selection, cell_width, line_height, caret_epoch),
                  window,
                  cx| {
                for (visible, row) in frame
                    .rows
                    .iter()
                    .skip(first)
                    .take((bounds.size.height.as_f32() / line_height).floor() as usize)
                    .enumerate()
                {
                    for (col, cell) in row.cells.iter().enumerate() {
                        if cell.is_wide_continuation() {
                            continue;
                        }
                        let mut foreground = color(cell.fgcolor(), false, cell.bold());
                        let mut background = color(cell.bgcolor(), true, false);
                        if cell.inverse() {
                            std::mem::swap(&mut foreground, &mut background);
                        }
                        if cell.dim() {
                            foreground = foreground.opacity(0.5);
                        }
                        let origin = bounds.origin
                            + point(
                                px(10. + col as f32 * cell_width),
                                px(visible as f32 * line_height),
                            );
                        let width = cell_width * if cell.is_wide() { 2. } else { 1. };
                        let cell_bounds = Bounds {
                            origin,
                            size: size(px(width), px(line_height)),
                        };
                        if cell.bgcolor() != vt100::Color::Default || cell.inverse() {
                            window.paint_quad(fill(cell_bounds, background));
                        }
                        if let Some((mut start, mut end)) = selection {
                            if start > end {
                                std::mem::swap(&mut start, &mut end);
                            }
                            let point = CellPoint {
                                row: first + visible,
                                col,
                            };
                            if point >= start && point < end {
                                window.paint_quad(fill(cell_bounds, rgba(0xffffff4d)));
                            }
                        }
                        if cell.has_contents() {
                            let mut cell_font = font.clone();
                            if cell.bold() {
                                cell_font.weight = FontWeight::BOLD;
                            }
                            if cell.italic() {
                                cell_font.style = FontStyle::Italic;
                            }
                            let run = TextRun {
                                len: cell.contents().len(),
                                font: cell_font,
                                color: foreground,
                                background_color: None,
                                underline: cell.underline().then_some(UnderlineStyle {
                                    thickness: px(1.),
                                    color: Some(foreground),
                                    wavy: false,
                                }),
                                strikethrough: None,
                            };
                            let line = window.text_system().shape_line(
                                cell.contents().to_string().into(),
                                font_size,
                                &[run],
                                None,
                            );
                            let _ = line.paint(
                                origin,
                                px(line_height),
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                    }
                }
                if focus.is_focused(window)
                    && frame.cursor.row >= first
                    && frame.cursor.row
                        < first + (bounds.size.height.as_f32() / line_height).floor() as usize
                {
                    let origin = bounds.origin
                        + point(
                            px(10. + frame.cursor.col.min(frame.cols - 1) as f32 * cell_width),
                            px((frame.cursor.row - first) as f32 * line_height),
                        );
                    let cursor = match cursor_style.as_str() {
                        "bar" => Bounds {
                            origin,
                            size: size(px(1.), px(line_height)),
                        },
                        "underline" => Bounds {
                            origin: origin + point(px(0.), px(line_height - 1.)),
                            size: size(px(cell_width), px(1.)),
                        },
                        _ => Bounds {
                            origin,
                            size: size(px(cell_width), px(line_height)),
                        },
                    };
                    if !window.is_window_active() {
                        window.paint_quad(quad(
                            Bounds {
                                origin,
                                size: size(px(cell_width), px(line_height)),
                            },
                            px(0.),
                            rgba(0),
                            px(1.),
                            rgb(0xffffff),
                            BorderStyle::Solid,
                        ));
                    } else {
                        window.request_animation_frame();
                        if caret_epoch.elapsed().as_millis() % 1200 < 600 {
                            window.paint_quad(fill(cursor, rgb(0xffffff)));
                            if cursor_style == "block"
                                && let Some(cell) =
                                    frame.rows[frame.cursor.row].cells.get(frame.cursor.col)
                                && cell.has_contents()
                            {
                                let run = TextRun {
                                    len: cell.contents().len(),
                                    font: font.clone(),
                                    color: rgb(0).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                let line = window.text_system().shape_line(
                                    cell.contents().to_string().into(),
                                    font_size,
                                    &[run],
                                    None,
                                );
                                let _ = line.paint(
                                    origin,
                                    px(line_height),
                                    TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                }
            },
        )
        .size_full();
        let candidate = {
            let state = view.borrow();
            state
                .frame
                .as_ref()
                .map(|frame| {
                    (
                        10. + frame.cursor.col as f32 * state.cell_width,
                        frame.cursor.row.saturating_sub(state.first_row()) as f32
                            * state.line_height,
                    )
                })
                .unwrap_or((10., 0.))
        };
        div()
            .id("cli-terminal")
            .key_context("RdmCli")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0))
            .font(container_font)
            .text_size(font_size)
            .line_height(px(line_height))
            .child(drawing)
            .child(
                div()
                    .absolute()
                    .left(px(candidate.0))
                    .top(px(candidate.1))
                    .w(px(1.))
                    .h(px(line_height))
                    .opacity(0.)
                    .child(gpui_base::input::Input::new(&tab.cli_input)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                    if let Some(i) = app.tab_index(id) {
                        let mut state = app.tabs[i].cli_view.borrow_mut();
                        let point = if event.click_count > 1 {
                            state.cell_position(event.position)
                        } else {
                            state.position(event.position)
                        };
                        state.selection_mode = event.click_count.min(3) as u8;
                        state.selection = match state.selection_mode {
                            2 => state
                                .frame
                                .as_ref()
                                .and_then(|frame| frame.word_selection(point)),
                            3 => state
                                .frame
                                .as_ref()
                                .and_then(|frame| frame.line_selection(point.row)),
                            _ => Some((point, point)),
                        };
                        state.selection_anchor = state.selection;
                        state.selecting = true;
                        app.tabs[i].cli_input.focus_handle(cx).focus(window, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |app, event: &MouseMoveEvent, _, cx| {
                if let Some(i) = app.tab_index(id) {
                    let mut state = app.tabs[i].cli_view.borrow_mut();
                    if event.pressed_button != Some(MouseButton::Left) {
                        state.selecting = false;
                    }
                    if state.selecting {
                        let end = if state.selection_mode > 1 {
                            state.cell_position(event.position)
                        } else {
                            state.position(event.position)
                        };
                        if let Some((first, last)) = state.selection_anchor {
                            let range = match state.selection_mode {
                                2 => state
                                    .frame
                                    .as_ref()
                                    .and_then(|frame| frame.word_selection(end)),
                                3 => state
                                    .frame
                                    .as_ref()
                                    .and_then(|frame| frame.line_selection(end.row)),
                                _ => Some((end, end)),
                            };
                            if let Some((start, end)) = range {
                                state.selection = Some(if start < first {
                                    (last, start)
                                } else {
                                    (first, end.max(last))
                                });
                            }
                        }
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _, _, _| {
                    if let Some(i) = app.tab_index(id) {
                        app.tabs[i].cli_view.borrow_mut().selecting = false;
                    }
                }),
            )
            .capture_action(
                cx.listener(move |app, _: &gpui_component::input::Copy, _, cx| {
                    if let Some(i) = app.tab_index(id)
                        && let Some(text) = app.tabs[i].cli_view.borrow().selected_text()
                    {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        cx.stop_propagation();
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(move |app, event: &ScrollWheelEvent, _, cx| {
                if let Some(i) = app.tab_index(id) {
                    let mut state = app.tabs[i].cli_view.borrow_mut();
                    let delta = event.delta.pixel_delta(px(state.line_height)).y.as_f32();
                    let max = state
                        .frame
                        .as_ref()
                        .map_or(0, |f| f.rows.len().saturating_sub(state.visible_rows));
                    state.scroll_back = (state.scroll_back as f32 + delta / state.line_height)
                        .round()
                        .clamp(0., max as f32) as usize;
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}
