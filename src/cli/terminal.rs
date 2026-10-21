use super::{Line, LineKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}

pub struct Row {
    pub cells: Vec<vt100::Cell>,
    pub wrapped: bool,
}

pub struct Frame {
    pub rows: Vec<Row>,
    pub cols: usize,
    pub cursor: Point,
}

impl Frame {
    pub fn output(bytes: &[u8], rows: u16, cols: u16, history: bool) -> Self {
        let mut parser = vt100::Parser::new(rows.max(1), cols.max(1), 1000);
        parser.process(bytes);
        Self::screen(parser.screen(), history)
    }

    pub fn console(
        lines: &[Line],
        prefix: &str,
        input: &str,
        cursor: usize,
        rows: u16,
        cols: u16,
    ) -> Self {
        let mut bytes = Vec::new();
        for line in lines {
            match line.kind {
                LineKind::Prompt(length) => {
                    bytes.extend_from_slice(b"\x1b[33m");
                    bytes.extend_from_slice(&line.text.as_bytes()[..length]);
                    bytes.extend_from_slice(b"\x1b[0m");
                    bytes.extend_from_slice(&line.text.as_bytes()[length..]);
                }
                LineKind::Error => {
                    bytes.extend_from_slice(b"\x1b[31m");
                    bytes.extend_from_slice(line.text.as_bytes());
                    bytes.extend_from_slice(b"\x1b[0m");
                }
                LineKind::Nil => {
                    bytes.extend_from_slice(b"\x1b[38;5;244m");
                    bytes.extend_from_slice(line.text.as_bytes());
                    bytes.extend_from_slice(b"\x1b[0m");
                }
                LineKind::Text => bytes.extend_from_slice(line.text.as_bytes()),
            }
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(b"\x1b[33m");
        bytes.extend_from_slice(prefix.as_bytes());
        bytes.extend_from_slice(b"\x1b[0m");
        bytes.extend_from_slice(input.as_bytes());
        if cursor < input.len() && input.is_char_boundary(cursor) {
            let prefix_width = prefix
                .encode_utf16()
                .map(|c| if c <= 255 { 1 } else { 2 })
                .sum::<usize>();
            let width = unicode_width::UnicodeWidthStr::width(&input[..cursor]);
            bytes.extend_from_slice(format!("\x1b[{}G", prefix_width + width + 1).as_bytes());
        }
        Self::output(&bytes, rows, cols, true)
    }

    fn screen(source: &vt100::Screen, history: bool) -> Self {
        let (rows, cols) = source.size();
        let (cursor_row, cursor_col) = source.cursor_position();
        let mut source = source.clone();
        if history {
            source.set_scrollback(usize::MAX);
        }
        let scrollback = source.scrollback();
        let mut result = Vec::new();
        for row in 0..scrollback + usize::from(rows) {
            source.set_scrollback(scrollback.saturating_sub(row));
            let source_row = row.saturating_sub(scrollback) as u16;
            result.push(Row {
                cells: (0..cols)
                    .map(|col| source.cell(source_row, col).unwrap().clone())
                    .collect(),
                wrapped: source.row_wrapped(source_row),
            });
        }
        Self {
            rows: result,
            cols: usize::from(cols),
            cursor: Point {
                row: scrollback + usize::from(cursor_row),
                col: usize::from(cursor_col),
            },
        }
    }

    fn word_on_row(&self, point: Point) -> Option<(Point, Point)> {
        let row = self.rows.get(point.row)?;
        if point.col >= self.cols {
            return None;
        }
        let mut col = point.col;
        if row.cells[col].is_wide_continuation() {
            col = col.saturating_sub(1);
        }
        let space = |cell: &vt100::Cell| {
            !cell.is_wide_continuation() && (!cell.has_contents() || cell.contents() == " ")
        };
        let separator = |cell: &vt100::Cell| {
            !cell.is_wide_continuation() && r#" ()[]{}',"`"#.contains(cell.contents())
        };
        let whitespace = space(&row.cells[col]);
        let mut first = col;
        let mut last = (col + if row.cells[col].is_wide() { 2 } else { 1 }).min(self.cols);
        while first > 0
            && if whitespace {
                space(&row.cells[first - 1])
            } else {
                !separator(&row.cells[first - 1])
            }
        {
            first -= 1;
        }
        while last < self.cols
            && if whitespace {
                space(&row.cells[last])
            } else {
                !separator(&row.cells[last])
            }
        {
            last += 1;
        }
        Some((
            Point {
                row: point.row,
                col: first,
            },
            Point {
                row: point.row,
                col: last,
            },
        ))
    }
    pub fn word_selection(&self, point: Point) -> Option<(Point, Point)> {
        let (mut first, mut last) = self.word_on_row(point)?;
        while first.col == 0
            && first.row > 0
            && self.rows[first.row - 1].wrapped
            && self.rows[first.row].cells[0].contents() != " "
            && self.rows[first.row - 1].cells[self.cols - 1].contents() != " "
        {
            let Some((previous, end)) = self.word_on_row(Point {
                row: first.row - 1,
                col: self.cols - 1,
            }) else {
                break;
            };
            if self.selected_text(previous, end).trim().is_empty() {
                break;
            }
            first = previous;
        }
        while last.col == self.cols
            && last.row + 1 < self.rows.len()
            && self.rows[last.row].wrapped
            && self.rows[last.row].cells[self.cols - 1].contents() != " "
            && self.rows[last.row + 1].cells[0].contents() != " "
        {
            let Some((start, next)) = self.word_on_row(Point {
                row: last.row + 1,
                col: 0,
            }) else {
                break;
            };
            if self.selected_text(start, next).trim().is_empty() {
                break;
            }
            last = next;
        }
        Some((first, last))
    }
    pub fn line_selection(&self, row: usize) -> Option<(Point, Point)> {
        if row >= self.rows.len() {
            return None;
        }
        let mut first = row;
        let mut last = row;
        while first > 0 && self.rows[first - 1].wrapped {
            first -= 1;
        }
        while last + 1 < self.rows.len() && self.rows[last].wrapped {
            last += 1;
        }
        Some((
            Point { row: first, col: 0 },
            Point {
                row: last,
                col: self.cols,
            },
        ))
    }

    pub fn selected_text(&self, start: Point, end: Point) -> String {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut result = String::new();
        for row in start.row..=end.row.min(self.rows.len().saturating_sub(1)) {
            let line = &self.rows[row];
            let first = if row == start.row {
                start.col.min(self.cols)
            } else {
                0
            };
            let trimmed = row_trimmed_len(line);
            let last = if row == end.row {
                end.col.min(trimmed)
            } else {
                trimmed
            };
            let mut text = String::new();
            for cell in line.cells.iter().take(last).skip(first) {
                if cell.is_wide_continuation() {
                    continue;
                }
                text.push_str(if cell.has_contents() {
                    cell.contents()
                } else {
                    " "
                });
            }
            result.push_str(&text.replace('\u{a0}', " "));
            if row < end.row && !line.wrapped {
                result.push('\n');
            }
        }
        result
    }
}

fn row_trimmed_len(row: &Row) -> usize {
    row.cells
        .iter()
        .rposition(vt100::Cell::has_contents)
        .map_or(0, |col| col + if row.cells[col].is_wide() { 2 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_selection_matches_original_xterm() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/cli-word-source.json"))
                .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let frame = Frame::output(
                case["input"].as_str().unwrap().as_bytes(),
                case["rows"].as_u64().unwrap() as u16,
                case["cols"].as_u64().unwrap() as u16,
                false,
            );
            let point = Point {
                row: case["row"].as_u64().unwrap() as usize,
                col: case["col"].as_u64().unwrap() as usize,
            };
            let selection = if case["selection"] == "line" {
                frame.line_selection(point.row)
            } else {
                frame.word_selection(point)
            };
            assert_eq!(
                selection
                    .map(|(start, end)| frame.selected_text(start, end))
                    .unwrap_or_default(),
                case["text"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            let from_json = |value: &serde_json::Value| Point {
                col: value[0].as_u64().unwrap() as usize,
                row: value[1].as_u64().unwrap() as usize,
            };
            assert_eq!(
                selection,
                case["start"]
                    .is_array()
                    .then(|| (from_json(&case["start"]), from_json(&case["end"]))),
                "{} selection bounds",
                case["name"]
            );
        }
    }
    fn color(color: vt100::Color) -> serde_json::Value {
        match color {
            vt100::Color::Default => serde_json::json!({"type":"default"}),
            vt100::Color::Idx(index) => serde_json::json!({"type":"indexed","index":index}),
            vt100::Color::Rgb(r, g, b) => serde_json::json!({"type":"rgb","r":r,"g":g,"b":b}),
        }
    }
    #[test]
    fn terminal_cells_match_original_xterm_output() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/cli-terminal-source.json"
        ))
        .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let frame = Frame::output(
                case["input"].as_str().unwrap().as_bytes(),
                case["rows"].as_u64().unwrap() as u16,
                case["cols"].as_u64().unwrap() as u16,
                false,
            );
            let lines = frame.rows.iter().map(|row| serde_json::json!({
                "wrapped":row.wrapped,
                "cells":row.cells.iter().map(|cell| serde_json::json!({
                    "text":cell.contents(), "width":if cell.is_wide_continuation() {0} else if cell.is_wide(){2}else{1},
                    "fg":color(cell.fgcolor()),"bg":color(cell.bgcolor()),
                    "bold":cell.bold(),"dim":cell.dim(),"italic":cell.italic(),"underline":cell.underline(),"inverse":cell.inverse(),
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>();
            assert_eq!(
                serde_json::json!(lines),
                case["lines"],
                "xterm cells: {}",
                case["name"]
            );
            assert_eq!(
                serde_json::json!([frame.cursor.row, frame.cursor.col]),
                case["cursor"],
                "xterm cursor: {}",
                case["name"]
            );
        }
    }
    #[test]
    fn selecting_wrapped_output_preserves_unicode_without_inserted_newlines() {
        let frame = Frame::output("用户abcdefghijk\r\nnext".as_bytes(), 5, 8, true);
        assert_eq!(
            frame.selected_text(Point { row: 0, col: 0 }, Point { row: 1, col: 7 }),
            "用户abcdefghijk"
        );
        assert_eq!(
            frame.selected_text(Point { row: 0, col: 0 }, Point { row: 2, col: 4 }),
            "用户abcdefghijk\nnext"
        );
    }
}
