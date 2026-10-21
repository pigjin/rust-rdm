//! Editing and reply formatting used by the desktop Redis console.
use crate::redis::Value;
pub mod terminal;

/// The desktop console uses Tiny RDM's SplitCmd rules, including empty arguments.
pub fn parse(input: &str) -> Vec<Vec<u8>> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut previous = '\0';
    let mut quote = '\0';
    for character in input.trim().chars() {
        if matches!(character, '\'' | '"')
            && previous != '\\'
            && (quote == '\0' || quote == character)
        {
            quote = if quote == '\0' { character } else { '\0' };
        } else if character == ' ' && quote == '\0' {
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(character);
        }
        previous = character;
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
        .into_iter()
        .enumerate()
        .filter_map(|(index, part)| {
            if index == 0 && part.is_empty() {
                return None;
            }
            Some(if part.contains('\\') {
                unquote(&part).unwrap_or_else(|| part.into_bytes())
            } else {
                part.into_bytes()
            })
        })
        .collect()
}

// strconv.Unquote applied to a double-quoted argument; byte escapes stay binary.
fn unquote(input: &str) -> Option<Vec<u8>> {
    let input = input.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        index += 1;
        if byte == b'"' || byte == b'\n' {
            return None;
        }
        if byte != b'\\' {
            result.push(byte);
            continue;
        }
        let escaped = *input.get(index)?;
        index += 1;
        match escaped {
            b'a' => result.push(7),
            b'b' => result.push(8),
            b'f' => result.push(12),
            b'n' => result.push(10),
            b'r' => result.push(13),
            b't' => result.push(9),
            b'v' => result.push(11),
            b'\\' | b'"' => result.push(escaped),
            b'x' | b'u' | b'U' => {
                let digits = match escaped {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let bytes = input.get(index..index + digits)?;
                if !bytes.iter().all(u8::is_ascii_hexdigit) {
                    return None;
                }
                let code = u32::from_str_radix(std::str::from_utf8(bytes).ok()?, 16).ok()?;
                index += digits;
                if escaped == b'x' {
                    result.push(code as u8);
                } else {
                    let mut buffer = [0; 4];
                    result.extend_from_slice(
                        char::from_u32(code)?.encode_utf8(&mut buffer).as_bytes(),
                    );
                }
            }
            b'0'..=b'7' => {
                let digits = input.get(index..index + 2)?;
                if !digits.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
                    return None;
                }
                let code = (u16::from(escaped - b'0') << 6)
                    | (u16::from(digits[0] - b'0') << 3)
                    | u16::from(digits[1] - b'0');
                result.push(u8::try_from(code).ok()?);
                index += 2;
            }
            _ => return None,
        }
    }
    Some(result)
}

#[derive(Default)]
pub struct History {
    items: Vec<String>,
    index: usize,
}

impl History {
    pub fn update(&mut self, input: &str) {
        if self.items.is_empty() {
            self.items.push(String::new());
        }
        self.items[self.index] = input.into();
    }

    pub fn browse(&mut self, previous: bool, input: &str) -> String {
        self.update(input);
        self.index = if previous {
            self.index.saturating_sub(1)
        } else {
            (self.index + 1).min(self.items.len() - 1)
        };
        self.items[self.index].clone()
    }

    /// Edited history moves to the last slot; adjacent duplicates collapse.
    pub fn finish(&mut self, input: &str) {
        self.update(input);
        if self.index < self.items.len() - 1 {
            let selected = self.items.remove(self.index);
            *self.items.last_mut().unwrap() = selected;
        }
        let len = self.items.len();
        if len > 1 && self.items[len - 1] == self.items[len - 2] {
            self.items.pop();
        }
        if self.items.last().is_some_and(|item| !item.is_empty()) {
            self.items.push(String::new());
        }
        self.index = self.items.len().saturating_sub(1);
    }
}

#[derive(Clone, Copy)]
pub enum Edit {
    Home,
    End,
    Left,
    Right,
    Delete,
    Backspace,
    KillBefore,
    KillAfter,
    KillWord,
}

/// Cursor offsets are UTF-8 bytes, as required by GPUI's input state.
pub fn edit(input: &str, cursor: usize, action: Edit) -> (String, usize) {
    let mut cursor = cursor.min(input.len());
    while !input.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let previous = |offset| input[..offset].char_indices().last().map_or(0, |(i, _)| i);
    let next = |offset| offset + input[offset..].chars().next().map_or(0, char::len_utf8);
    let mut output = input.to_string();
    match action {
        Edit::Home => cursor = 0,
        Edit::End => cursor = input.len(),
        Edit::Left => cursor = previous(cursor),
        Edit::Right => cursor = next(cursor),
        Edit::Delete => output.replace_range(cursor..next(cursor), ""),
        Edit::Backspace => {
            let start = previous(cursor);
            output.replace_range(start..cursor, "");
            cursor = start;
        }
        Edit::KillBefore => {
            output.replace_range(..cursor, "");
            cursor = 0;
        }
        Edit::KillAfter => output.truncate(cursor),
        Edit::KillWord => {
            let end = cursor;
            let mut separator_seen = false;
            loop {
                let alphanumeric = input[cursor..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
                if separator_seen && !alphanumeric {
                    break;
                }
                separator_seen |= !alphanumeric;
                if cursor == 0 {
                    break;
                }
                cursor = previous(cursor);
            }
            output.replace_range(cursor..end, "");
        }
    }
    (output, cursor)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Text,
    Prompt(usize),
    Error,
    Nil,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub kind: LineKind,
}

impl Line {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: LineKind::Text,
        }
    }

    pub fn prompt(prefix: &str, input: &str) -> Self {
        Self {
            text: format!("{prefix}{input}"),
            kind: LineKind::Prompt(prefix.len()),
        }
    }

    pub fn reply(value: &Value) -> Vec<Self> {
        let kind = match value {
            Value::Null => LineKind::Nil,
            Value::Error(_) => LineKind::Error,
            _ => LineKind::Text,
        };
        reply_text(value, 0)
            .split('\n')
            .map(|text| Self {
                text: text.trim_end_matches('\r').into(),
                kind,
            })
            .collect()
    }
}

/// Mirrors the original Go AnyToString: only strings inside arrays are quoted.
pub fn reply_text(value: &Value, depth: usize) -> String {
    match value {
        Value::Null => {
            if depth == 0 {
                "(nil)".into()
            } else {
                String::new()
            }
        }
        Value::Int(number) => number.to_string(),
        Value::Bytes(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            if depth == 0 {
                text.into_owned()
            } else {
                format!("\"{text}\"")
            }
        }
        Value::Error(message) => message.clone(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| format!("{}) {}", index + 1, reply_text(item, depth + 1)))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_match_original_go_outputs_including_binary_and_empty_arguments() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/cli-command-source.json"))
                .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 115);
        for case in cases {
            let input = case["input"].as_str().unwrap();
            let expected = case["args_hex"]
                .as_array()
                .unwrap()
                .iter()
                .map(|argument| hex::decode(argument.as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                parse(input),
                expected,
                "original Go SplitCmd input {input:?}"
            );
        }
    }

    #[test]
    fn history_keeps_drafts_edits_and_collapses_adjacent_duplicates() {
        let mut history = History::default();
        history.finish("PING");
        history.finish("PING");
        history.finish("GET key");
        assert_eq!(history.browse(true, "unfinished draft"), "GET key");
        assert_eq!(history.browse(false, "GET key"), "unfinished draft");
        assert_eq!(history.browse(true, "unfinished draft"), "GET key");
        assert_eq!(history.browse(true, "GET other"), "PING");
        assert_eq!(history.browse(false, "PING"), "GET other");
        history.finish("GET other");
        assert_eq!(history.items, ["PING", "GET other", ""]);
        assert_eq!(history.browse(true, ""), "GET other");
    }

    #[test]
    fn terminal_edits_preserve_unicode_and_delete_the_correct_side() {
        let input = "SET 用户 值";
        let cursor = input.find('值').unwrap();
        assert_eq!(
            edit(input, cursor, Edit::Delete),
            ("SET 用户 ".into(), cursor)
        );
        assert_eq!(
            edit(input, cursor, Edit::Backspace),
            ("SET 用户值".into(), cursor - 1)
        );
        assert_eq!(edit(input, cursor, Edit::Left), (input.into(), cursor - 1));
        assert_eq!(
            edit(input, cursor, Edit::Right),
            (input.into(), input.len())
        );
        assert_eq!(edit(input, cursor, Edit::KillBefore), ("值".into(), 0));
        assert_eq!(
            edit(input, cursor, Edit::KillAfter),
            ("SET 用户 ".into(), cursor)
        );
        assert_eq!(
            edit("SET foo bar", 11, Edit::KillWord),
            ("SET foo".into(), 7)
        );
        assert_eq!(edit(input, 0, Edit::Backspace), (input.into(), 0));
    }

    #[test]
    fn replies_match_original_nested_strings_nil_and_error_text() {
        let value = Value::Array(vec![
            Value::Bytes(b"name".to_vec()),
            Value::Array(vec![
                Value::Bytes(b"a\"b".to_vec()),
                Value::Null,
                Value::Int(7),
            ]),
        ]);
        assert_eq!(
            reply_text(&value, 0),
            "1) \"name\"\n2) 1) \"a\"b\"\n2) \n3) 7"
        );
        assert_eq!(
            reply_text(&Value::Bytes(b"line\nnext".to_vec()), 0),
            "line\nnext"
        );
        assert_eq!(reply_text(&Value::Null, 0), "(nil)");
        assert_eq!(
            reply_text(&Value::Error("ERR example".into()), 0),
            "ERR example"
        );
    }
}
