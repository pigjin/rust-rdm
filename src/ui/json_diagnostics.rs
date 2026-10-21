//! JSON syntax diagnostics and tokenization following Monaco 0.47's scanner rules.
//! Adapted from Microsoft jsonc-parser/vscode-json-languageservice (MIT).
//! See assets/licenses/monaco-json-diagnostics.txt. Schema validation is separate.
use super::{EditorState, RdmApp};
use gpui::{Context, Entity, Window};
use gpui_base::input::{Diagnostic, DiagnosticSeverity, Position, Rope, RopeExt as _};
use std::{cell::Cell, rc::Rc, time::Duration};
use std::{collections::BTreeMap, ops::Range};

#[derive(Default)]
pub(super) struct Controller {
    json: Rc<Cell<bool>>,
    pending: Rc<Cell<usize>>,
    scheduled: Option<Rc<Cell<bool>>>,
    text: Option<String>,
}

impl Controller {
    #[cfg(feature = "ui-qa")]
    pub(super) fn is_pending(&self) -> bool {
        self.pending.get() > 0
    }

    pub(super) fn refresh(
        &mut self,
        editor: &Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<RdmApp>,
    ) {
        let text = editor.read(cx).value().to_string();
        let json = editor.read(cx).language_name() == "json";
        let changed_language = self.json.get() != json;
        if !changed_language && self.text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.json.set(json);
        self.text = Some(text);
        if changed_language {
            // Monaco disposes the content listener on a language change but
            // does not clear that listener's last timeout. Its old reply can
            // still apply if the same model has returned to JSON meanwhile.
            self.scheduled.take();
        } else if let Some(scheduled) = self.scheduled.take()
            && scheduled.replace(false)
        {
            self.pending.set(self.pending.get() - 1);
        }
        if !json {
            editor.update(cx, |state, cx| {
                let text = state.text().clone();
                if let Some(set) = state.diagnostics_mut() {
                    set.reset(&text);
                }
                state.clear_diagnostic_popover(cx);
                state.diagnostics_did_change(cx);
                state.refresh(cx);
            });
            return;
        }
        self.pending.set(self.pending.get() + 1);
        let scheduled = Rc::new(Cell::new(true));
        self.scheduled = Some(scheduled.clone());
        let json = self.json.clone();
        let pending = self.pending.clone();
        let editor = editor.downgrade();
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |_, cx| {
            if !changed_language {
                executor.timer(Duration::from_millis(500)).await;
            }
            if !scheduled.replace(false) {
                return;
            }
            // The original worker reads the model when validation starts,
            // not when the debounce was scheduled.
            let Ok(text) = editor.update(cx, |state, _| state.value().to_string()) else {
                pending.set(pending.get() - 1);
                return;
            };
            let issues = executor
                .spawn(async move {
                    #[cfg(feature = "ui-qa")]
                    super::dialogs::json_diagnostics_qa::hold(&text);
                    let lines = LineIndex::new(&text.encode_utf16().collect::<Vec<_>>());
                    diagnose(&text)
                        .into_iter()
                        .map(|issue| {
                            let start = lines.position(issue.range.start);
                            let end = lines.position(issue.range.end);
                            (issue, start, end)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            pending.set(pending.get() - 1);
            if !json.get() {
                return;
            }
            let _ = editor.update(cx, |state, cx| {
                let rope = state.text().clone();
                // Do not reject an already-running reply by model version.
                // Monaco applies it to the current model and clamps its ranges.
                let mut diagnostics: Vec<_> = issues
                    .into_iter()
                    .map(|(issue, start, end)| {
                        let start = model_position(&rope, start, false);
                        let end = model_position(&rope, end, true);
                        let mut diagnostic = Diagnostic::new(start..end, issue.message)
                            .with_code(issue.code.to_string())
                            .with_severity(if issue.warning {
                                DiagnosticSeverity::Warning
                            } else {
                                DiagnosticSeverity::Error
                            });
                        if issue.source {
                            diagnostic.source = Some("json".into());
                        }
                        diagnostic
                    })
                    .collect();
                diagnostics.sort_by_key(|diagnostic| {
                    (
                        diagnostic.range.start.line,
                        diagnostic.range.start.character,
                    )
                });
                if let Some(set) = state.diagnostics_mut() {
                    set.reset(&rope);
                    set.extend(diagnostics);
                }
                state.clear_diagnostic_popover(cx);
                state.diagnostics_did_change(cx);
                state.refresh(cx);
            });
        })
        .detach();
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.json.set(false);
    }
}

fn model_position(rope: &Rope, (line, column): (u32, u32), end: bool) -> Position {
    let last = rope.lines_len().saturating_sub(1);
    let row = (line as usize).min(last);
    let text = rope.slice_line(row);
    let text = if text.get_byte(text.len().saturating_sub(1)) == Some(b'\r') {
        text.slice(..text.len() - 1)
    } else {
        text
    };
    let column = if line as usize > last {
        usize::MAX
    } else {
        column as usize
    };
    let column = column.min(text.len_utf16());
    let byte = text.utf16_to_byte_idx(column);
    let scalar =
        text.byte_to_char_idx(byte) + usize::from(end && text.byte_to_utf16_idx(byte) < column);
    Position::new(row as u32, scalar as u32)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Issue {
    pub range: Range<usize>, // UTF-16 offsets, as in the original worker.
    pub code: u16,
    pub message: &'static str,
    pub warning: bool,
    pub source: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Object,
    EndObject,
    Array,
    EndArray,
    Comma,
    Colon,
    Null,
    True,
    False,
    String,
    Number,
    Comment,
    Space,
    Unknown,
    Eof,
}

struct Token {
    kind: Kind,
    range: Range<usize>,
    value: Vec<u16>,
    error: u16,
}
struct Scanner {
    text: Vec<u16>,
    pos: usize,
}

fn white(c: u16) -> bool {
    matches!(
        c,
        9 | 11 | 12 | 32 | 160 | 5760 | 8192..=8203 | 8239 | 8287 | 12288 | 65279
    )
}
fn line_break(c: u16) -> bool {
    matches!(c, 10 | 13 | 8232 | 8233)
}
fn js_white(c: u16) -> bool {
    (white(c) && c != 8203) || line_break(c)
}
fn digit(c: u16) -> bool {
    (48..=57).contains(&c)
}
fn hex(c: u16) -> Option<u16> {
    match c {
        48..=57 => Some(c - 48),
        65..=70 => Some(c - 55),
        97..=102 => Some(c - 87),
        _ => None,
    }
}
fn unknown(c: u16) -> bool {
    !white(c) && !line_break(c) && !matches!(c, 123 | 125 | 91 | 93 | 34 | 58 | 44 | 47)
}

impl Scanner {
    fn peek(&self) -> Option<u16> {
        self.text.get(self.pos).copied()
    }
    fn next(&mut self) -> Token {
        let start = self.pos.min(self.text.len());
        let mut error = 0;
        let mut value = Vec::new();
        let kind = match self.peek() {
            None => Kind::Eof,
            Some(c) if white(c) || line_break(c) => {
                self.pos += 1;
                if c == 13 && self.peek() == Some(10) {
                    self.pos += 1;
                }
                if white(c) {
                    while self.peek().is_some_and(white) {
                        self.pos += 1;
                    }
                }
                Kind::Space
            }
            Some(c @ (123 | 125 | 91 | 93 | 44 | 58)) => {
                self.pos += 1;
                match c {
                    123 => Kind::Object,
                    125 => Kind::EndObject,
                    91 => Kind::Array,
                    93 => Kind::EndArray,
                    44 => Kind::Comma,
                    _ => Kind::Colon,
                }
            }
            Some(34) => {
                self.pos += 1;
                loop {
                    match self.peek() {
                        None => {
                            error = 258;
                            break;
                        }
                        Some(34) => {
                            self.pos += 1;
                            break;
                        }
                        Some(92) => {
                            self.pos += 1;
                            let Some(c) = self.peek() else {
                                error = 258;
                                break;
                            };
                            self.pos += 1;
                            match c {
                                34 | 92 | 47 => value.push(c),
                                98 => value.push(8),
                                102 => value.push(12),
                                110 => value.push(10),
                                114 => value.push(13),
                                116 => value.push(9),
                                117 => {
                                    let (mut count, mut code) = (0, 0);
                                    while count < 4 {
                                        let Some(d) = self.peek().and_then(hex) else {
                                            break;
                                        };
                                        code = code * 16 + d;
                                        self.pos += 1;
                                        count += 1;
                                    }
                                    if count == 4 {
                                        value.push(code);
                                    } else {
                                        error = 260;
                                    }
                                }
                                _ => error = 261,
                            }
                        }
                        Some(c) if c <= 31 && line_break(c) => {
                            error = 258;
                            break;
                        }
                        Some(c) => {
                            if c <= 31 {
                                error = 262;
                            }
                            value.push(c);
                            self.pos += 1;
                        }
                    }
                }
                Kind::String
            }
            Some(47) if self.text.get(self.pos + 1) == Some(&47) => {
                self.pos += 2;
                while self.peek().is_some_and(|c| !line_break(c)) {
                    self.pos += 1;
                }
                Kind::Comment
            }
            Some(47) if self.text.get(self.pos + 1) == Some(&42) => {
                self.pos += 2;
                let mut closed = false;
                while self.pos < self.text.len().saturating_sub(1) {
                    if self.peek() == Some(42) && self.text.get(self.pos + 1) == Some(&47) {
                        self.pos += 2;
                        closed = true;
                        break;
                    }
                    self.pos += 1;
                }
                if !closed {
                    self.pos += 1;
                    error = 257;
                }
                Kind::Comment
            }
            Some(c) if digit(c) || c == 45 => {
                if c == 45 {
                    self.pos += 1;
                }
                if !self.peek().is_some_and(digit) {
                    value.push(45);
                    Kind::Unknown
                } else {
                    if self.peek() == Some(48) {
                        self.pos += 1;
                    } else {
                        while self.peek().is_some_and(digit) {
                            self.pos += 1;
                        }
                    }
                    if self.peek() == Some(46) {
                        self.pos += 1;
                        if self.peek().is_some_and(digit) {
                            while self.peek().is_some_and(digit) {
                                self.pos += 1;
                            }
                        } else {
                            error = 259;
                        }
                    }
                    if error == 0 && matches!(self.peek(), Some(69 | 101)) {
                        self.pos += 1;
                        if matches!(self.peek(), Some(43 | 45)) {
                            self.pos += 1;
                        }
                        if self.peek().is_some_and(digit) {
                            while self.peek().is_some_and(digit) {
                                self.pos += 1;
                            }
                        } else {
                            error = 259;
                        }
                    }
                    Kind::Number
                }
            }
            Some(_) => {
                while self.peek().is_some_and(unknown) {
                    self.pos += 1;
                }
                if self.pos == start {
                    self.pos += 1;
                }
                value = self.text[start..self.pos].to_vec();
                match value.as_slice() {
                    [116, 114, 117, 101] => Kind::True,
                    [102, 97, 108, 115, 101] => Kind::False,
                    [110, 117, 108, 108] => Kind::Null,
                    _ => Kind::Unknown,
                }
            }
        };
        Token {
            kind,
            range: start..self.pos,
            value,
            error,
        }
    }
}

pub(super) struct SyntaxToken {
    pub range: Range<usize>,
    pub scope: &'static str,
}

/// Monaco tokenizes one model line at a time. Incomplete strings and comments
/// carry their scanner state to the following line independently of validation.
pub(super) fn syntax_tokens(text: &str) -> Vec<SyntaxToken> {
    let mut result = Vec::new();
    let mut parents = Vec::new();
    let mut last_was_colon = false;
    let mut scan_error = 0;
    let mut line_start = 0;
    loop {
        let line_end = text.as_bytes()[line_start..]
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(text.len(), |offset| line_start + offset);
        let line = &text[line_start..line_end];
        let prefix = match scan_error {
            258 => "\"",
            257 => "/*",
            _ => "",
        };
        let prefix_len = prefix.len();
        let input = format!("{prefix}{line}");
        let mut scanner = Scanner {
            text: input.encode_utf16().collect(),
            pos: 0,
        };
        let byte_offsets: Vec<_> = line
            .char_indices()
            .flat_map(|(byte, character)| std::iter::repeat_n(byte, character.len_utf16()))
            .chain(std::iter::once(line.len()))
            .collect();
        loop {
            let token = scanner.next();
            if token.kind == Kind::Eof {
                break;
            }
            let scope = match token.kind {
                Kind::Object | Kind::Array => {
                    parents.push(token.kind);
                    last_was_colon = false;
                    if token.kind == Kind::Object {
                        "delimiter.bracket.json"
                    } else {
                        "delimiter.array.json"
                    }
                }
                Kind::EndObject | Kind::EndArray => {
                    parents.pop();
                    last_was_colon = false;
                    if token.kind == Kind::EndObject {
                        "delimiter.bracket.json"
                    } else {
                        "delimiter.array.json"
                    }
                }
                Kind::Colon => {
                    last_was_colon = true;
                    "delimiter.colon.json"
                }
                Kind::Comma => {
                    last_was_colon = false;
                    "delimiter.comma.json"
                }
                Kind::Null | Kind::True | Kind::False => {
                    last_was_colon = false;
                    "keyword.json"
                }
                Kind::String => {
                    let value = last_was_colon || parents.last() == Some(&Kind::Array);
                    last_was_colon = false;
                    if value {
                        "string.value.json"
                    } else {
                        "string.key.json"
                    }
                }
                Kind::Number => {
                    last_was_colon = false;
                    "number.json"
                }
                Kind::Comment => {
                    if scanner.text.get(token.range.start + 1) == Some(&47) {
                        "comment.line.json"
                    } else {
                        "comment.block.json"
                    }
                }
                _ => "",
            };
            scan_error = token.error;
            let start = token.range.start.saturating_sub(prefix_len);
            let end = token
                .range
                .end
                .min(scanner.text.len())
                .saturating_sub(prefix_len);
            let range = line_start + byte_offsets[start]..line_start + byte_offsets[end];
            if !range.is_empty() {
                result.push(SyntaxToken { range, scope });
            }
        }
        if line_end == text.len() {
            break;
        }
        line_start = line_end
            + if text.as_bytes()[line_end..].starts_with(b"\r\n") {
                2
            } else {
                1
            };
    }
    result
}

struct Parser {
    scanner: Scanner,
    lines: LineIndex,
    token: Token,
    issues: Vec<Issue>,
    comments: Vec<Range<usize>>,
    last_problem: Option<usize>,
}

impl Parser {
    fn advance(&mut self) {
        loop {
            self.token = self.scanner.next();
            let message = match self.token.error {
                257 => Some("Unexpected end of comment."),
                258 => Some("Unexpected end of string."),
                259 => Some("Unexpected end of number."),
                260 => Some("Invalid unicode sequence in string."),
                261 => Some("Invalid escape character in string."),
                262 => Some("Invalid characters in string. Control characters must be escaped."),
                _ => None,
            };
            if let Some(message) = message {
                self.error(message, self.token.error, &[]);
            }
            match self.token.kind {
                Kind::Comment => self.comments.push(self.token.range.clone()),
                Kind::Space => {}
                _ => return,
            }
        }
    }
    fn at(&mut self, range: Range<usize>, code: u16, message: &'static str, warning: bool) {
        if self.last_problem != Some(range.start) {
            self.last_problem = Some(range.start);
            self.issues.push(Issue {
                range,
                code,
                message,
                warning,
                source: true,
            });
        }
    }
    fn error(&mut self, message: &'static str, code: u16, until: &[Kind]) {
        let mut range = self.token.range.clone();
        if range.is_empty() && range.start > 0 {
            range.start -= 1;
            while range.start > 0 && js_white(self.scanner.text[range.start]) {
                range.start -= 1;
            }
            range.end = range.start + 1;
        }
        self.at(range, code, message, false);
        if !until.is_empty() {
            while self.token.kind != Kind::Eof && !until.contains(&self.token.kind) {
                self.advance();
            }
        }
    }
    fn value(&mut self) -> bool {
        match self.token.kind {
            Kind::Object => {
                self.object();
                true
            }
            Kind::Array => {
                self.array();
                true
            }
            Kind::String | Kind::Number | Kind::Null | Kind::True | Kind::False => {
                self.advance();
                true
            }
            _ => false,
        }
    }
    fn array(&mut self) {
        self.advance();
        let mut needs_comma = false;
        while !matches!(self.token.kind, Kind::EndArray | Kind::Eof) {
            if self.token.kind == Kind::Comma {
                if !needs_comma {
                    self.error("Value expected", 516, &[]);
                }
                let comma = self.token.range.start;
                self.advance();
                if self.token.kind == Kind::EndArray {
                    if needs_comma {
                        self.at(comma..comma + 1, 519, "Trailing comma", false);
                    }
                    continue;
                }
            } else if needs_comma {
                self.error("Expected comma", 514, &[]);
            }
            if !self.value() {
                self.error("Value expected", 516, &[Kind::EndArray, Kind::Comma]);
            }
            needs_comma = true;
        }
        if self.token.kind == Kind::EndArray {
            self.advance();
        } else {
            self.error("Expected comma or closing bracket", 517, &[]);
        }
    }
    fn object(&mut self) {
        self.advance();
        let mut needs_comma = false;
        let mut seen: BTreeMap<Vec<u16>, Option<Range<usize>>> = BTreeMap::new();
        while !matches!(self.token.kind, Kind::EndObject | Kind::Eof) {
            if self.token.kind == Kind::Comma {
                if !needs_comma {
                    self.error("Property expected", 513, &[]);
                }
                let comma = self.token.range.start;
                self.advance();
                if self.token.kind == Kind::EndObject {
                    if needs_comma {
                        self.at(comma..comma + 1, 519, "Trailing comma", false);
                    }
                    continue;
                }
            } else if needs_comma {
                self.error("Expected comma", 514, &[]);
            }
            if !self.property(&mut seen) {
                self.error("Property expected", 513, &[Kind::EndObject, Kind::Comma]);
            }
            needs_comma = true;
        }
        if self.token.kind == Kind::EndObject {
            self.advance();
        } else {
            self.error("Expected comma or closing brace", 518, &[]);
        }
    }
    fn property(&mut self, seen: &mut BTreeMap<Vec<u16>, Option<Range<usize>>>) -> bool {
        if !matches!(self.token.kind, Kind::String | Kind::Unknown) {
            return false;
        }
        if self.token.kind == Kind::Unknown {
            self.error("Property keys must be doublequoted", 0, &[]);
        }
        let range = self.token.range.clone();
        let key = self.token.value.clone();
        self.advance();
        if let Some(previous) = seen.get(&key).cloned() {
            self.at(range.clone(), 520, "Duplicate object key", true);
            if let Some(previous) = previous {
                self.at(previous, 520, "Duplicate object key", true);
            }
            seen.insert(key, None);
        } else {
            seen.insert(key, Some(range.clone()));
        }
        if self.token.kind == Kind::Colon {
            self.advance();
        } else {
            self.error("Colon expected", 515, &[]);
            if self.token.kind == Kind::String
                && self.lines.position(range.end).0 < self.lines.position(self.token.range.start).0
            {
                return true;
            }
        }
        if !self.value() {
            self.error("Value expected", 516, &[Kind::EndObject, Kind::Comma]);
        }
        true
    }
}

struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}
impl LineIndex {
    fn new(text: &[u16]) -> Self {
        let mut starts = vec![0];
        let mut i = 0;
        while i < text.len() {
            if text[i] == 13 {
                if text.get(i + 1) == Some(&10) {
                    i += 1;
                }
                starts.push(i + 1);
            } else if text[i] == 10 {
                starts.push(i + 1);
            }
            i += 1;
        }
        Self {
            starts,
            len: text.len(),
        }
    }
    fn position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        (line as u32, (offset - self.starts[line]) as u32)
    }
}

#[cfg(test)]
fn source_position(text: &[u16], offset: usize) -> (u32, u32) {
    LineIndex::new(text).position(offset)
}

pub(super) fn diagnose(text: &str) -> Vec<Issue> {
    let text: Vec<_> = text.encode_utf16().collect();
    let mut p = Parser {
        lines: LineIndex::new(&text),
        scanner: Scanner { text, pos: 0 },
        token: Token {
            kind: Kind::Eof,
            range: 0..0,
            value: vec![],
            error: 0,
        },
        issues: vec![],
        comments: vec![],
        last_problem: None,
    };
    p.advance();
    if p.token.kind != Kind::Eof {
        if !p.value() {
            p.error("Expected a JSON object, array or literal.", 0, &[]);
        } else if p.token.kind != Kind::Eof {
            p.error("End of file expected.", 0, &[]);
        }
    }
    for range in p.comments {
        p.issues.push(Issue {
            range,
            code: 521,
            message: "Comments are not permitted in JSON.",
            warning: false,
            source: false,
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    p.issues
        .retain(|issue| seen.insert((p.lines.position(issue.range.start), issue.message)));
    p.issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_reply_positions_clamp_to_current_model_without_splitting_surrogates() {
        let units: Vec<_> = "a\r\n🙂\n中".encode_utf16().collect();
        assert_eq!(source_position(&units, 2), (0, 2));
        assert_eq!(source_position(&units, 3), (1, 0));
        assert_eq!(source_position(&units, 5), (1, 2));
        let current = Rope::from("🙂\r\n中");
        assert_eq!(model_position(&current, (0, 1), false), Position::new(0, 0));
        assert_eq!(model_position(&current, (0, 1), true), Position::new(0, 1));
        assert_eq!(model_position(&current, (0, 99), true), Position::new(0, 1));
        assert_eq!(model_position(&current, (9, 0), false), Position::new(1, 1));
    }
    #[test]
    fn diagnostics_match_original_monaco_samples() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/entry-diagnostics-source.json"
        ))
        .unwrap();
        check_fixture(fixture);
    }

    #[test]
    fn extended_diagnostics_match_original_monaco_samples() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/json-diagnostics-extended-source.json"
        ))
        .unwrap();
        check_fixture(fixture);
    }

    fn check_fixture(fixture: serde_json::Value) {
        for record in fixture["records"].as_array().unwrap() {
            let text = record["value"].as_str().unwrap();
            let utf16: Vec<_> = text.encode_utf16().collect();
            let actual:Vec<_>=diagnose(text).into_iter().map(|issue|{let start=source_position(&utf16,issue.range.start);let end=source_position(&utf16,issue.range.end);serde_json::json!({"code":issue.code.to_string(),"message":issue.message,"severity":if issue.warning{4}else{8},"startLineNumber":start.0+1,"startColumn":start.1+1,"endLineNumber":end.0+1,"endColumn":end.1+1})}).collect();
            let expected: Vec<_> = record["markers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|marker| {
                    let mut copy = serde_json::Map::new();
                    for key in [
                        "code",
                        "message",
                        "severity",
                        "startLineNumber",
                        "startColumn",
                        "endLineNumber",
                        "endColumn",
                    ] {
                        copy.insert(key.into(), marker[key].clone());
                    }
                    serde_json::Value::Object(copy)
                })
                .collect();
            assert_eq!(actual, expected, "{text:?}");
        }
    }
}
