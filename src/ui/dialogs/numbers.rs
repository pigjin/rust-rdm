use super::*;

pub(super) fn bounds(kind: &DialogKind, key: &str) -> Option<(f64, Option<f64>)> {
    match kind {
        DialogKind::Connection(_) | DialogKind::ConnectionCopy(_) => match key {
            "port" | "ssh.port" => Some((1., Some(65535.))),
            "proxy.port" => Some((0., Some(65535.))),
            "conn_timeout" | "exec_timeout" => Some((1., Some(999999.))),
            "load_size" => Some((0., None)),
            _ if key.starts_with("alias_db.") => Some((0., None)),
            _ => None,
        },
        DialogKind::Import if key == "ttl" => Some((-1., Some(9007199254740991.))),
        DialogKind::NewKey if key == "ttl" => Some((-1., Some(9007199254740991.))),
        DialogKind::NewKey | DialogKind::AddEntries if key.starts_with("new_score.") => Some((f64::NEG_INFINITY, None)),
        DialogKind::Preferences => match key {
            "general.font_size" | "editor.font_size" | "cli.font_size" => Some((1., Some(65535.))),
            "general.scan_size" => Some((1., None)),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn parse(text: &str) -> std::result::Result<Option<f64>, ()> {
    let text = text.trim_matches(|ch: char| matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'));
    if text.is_empty() {
        return Ok(None);
    }
    let value = if matches!(text, "Infinity" | "+Infinity") {
        f64::INFINITY
    } else if text == "-Infinity" {
        f64::NEG_INFINITY
    } else if let Some((digits, radix)) = [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ]
    .into_iter()
    .find_map(|(prefix, radix)| text.strip_prefix(prefix).map(|digits| (digits, radix)))
    {
        if digits.is_empty() {
            return Err(());
        }
        let mut value = 0.;
        for ch in digits.chars() {
            value = value * radix as f64 + ch.to_digit(radix).ok_or(())? as f64;
        }
        value
    } else {
        if !text.bytes().all(|ch| b"0123456789+-.eE".contains(&ch)) {
            return Err(());
        }
        text.parse::<f64>().map_err(|_| ())?
    };
    if value.is_nan() {
        Err(())
    } else {
        Ok(Some(value))
    }
}
pub(crate) fn unfinished(text: &str) -> bool {
    if matches!(text, "-" | "-0") {
        return true;
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    text.contains('.')
        && digits.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && (text.ends_with('.') || text.ends_with('0'))
}
pub(crate) fn display(value: Option<f64>) -> String {
    match value {
        None => String::new(),
        Some(value) if value == 0. => "0".into(),
        Some(value) if value == f64::INFINITY => "Infinity".into(),
        Some(value) if value == f64::NEG_INFINITY => "-Infinity".into(),
        Some(value) if value.abs() >= 1e21 || value.abs() < 1e-6 => {
            let text = format!("{value:e}");
            let (mantissa, exponent) = text.split_once('e').unwrap();
            let exponent = exponent.parse::<i32>().unwrap();
            format!(
                "{mantissa}e{}{exponent}",
                if exponent >= 0 { "+" } else { "" }
            )
        }
        Some(value) => value.to_string(),
    }
}
pub(super) fn alias_key(state: &Entity<InputState>) -> String {
    format!("alias_db.{}", state.entity_id().as_u64())
}
pub(super) fn object_key(value: Option<f64>) -> String {
    value
        .map(|value| display(Some(value)))
        .unwrap_or_else(|| "null".into())
}
pub(super) fn json(value: Option<f64>) -> Json {
    match value {
        Some(value)
            if value.is_finite()
                && value.fract() == 0.
                && value >= i64::MIN as f64
                && value < i64::MAX as f64 =>
        {
            Json::Number((value as i64).into())
        }
        Some(value) if value.is_finite() => serde_json::Number::from_f64(value)
            .map(Json::Number)
            .unwrap_or_else(|| Json::from(0)),
        _ => Json::from(0),
    }
}

impl RdmApp {
    pub(crate) fn number_input(&self, key: &str, cx: &Context<Self>) -> Input {
        let dialog = self.dialog.as_ref().unwrap();
        let state = dialog.fields.get(key).unwrap();
        let invalid = bounds(&dialog.kind, key).is_some_and(|(min, max)| {
            match parse(&text_input(state, cx)) {
                Ok(None) => false,
                Ok(Some(value)) => value < min || max.is_some_and(|max| value > max),
                Err(()) => true,
            }
        });
        form_input(state).when(invalid, |input| {
            input
                .border_color(self.form_error_color(cx))
                .focus_bordered(false)
        })
    }
    pub(crate) fn number_event(
        &mut self,
        key: &str,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            event,
            InputEvent::Change | InputEvent::Blur | InputEvent::PressEnter { .. }
        ) {
            return;
        }
        let Some(dialog) = &self.dialog else {
            return;
        };
        let Some((min, max)) = bounds(&dialog.kind, key) else {
            return;
        };
        let Some(state) = dialog.fields.get(key).cloned() else {
            return;
        };
        let raw = text_input(&state, cx);
        if matches!(event, InputEvent::Change) && unfinished(&raw) {
            return;
        }
        let parsed = parse(&raw);
        let value = match parsed {
            Ok(Some(value))
                if matches!(event, InputEvent::Change)
                    && (value < min || max.is_some_and(|max| value > max)) =>
            {
                return;
            }
            Ok(Some(value)) => Some(value.max(min).min(max.unwrap_or(f64::INFINITY))),
            Ok(None) => None,
            Err(()) if matches!(event, InputEvent::Blur) => {
                dialog.numbers.get(key).copied().flatten()
            }
            Err(()) => return,
        };
        if let Some(dialog) = &mut self.dialog {
            dialog.numbers.insert(key.into(), value);
            if key == "general.font_size"
                && let Some(size) = value
            {
                dialog.general_preview_size = size as f32;
            }
        }
        let formatted = display(value);
        if formatted != raw {
            self.set_dialog_field(key, formatted, window, cx);
        }
        if key == "editor.font_size" {
            self.apply_theme(window, cx);
        }
        cx.notify();
    }
    pub(crate) fn number_step(
        &mut self,
        key: &str,
        delta: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let Some((min, max)) = bounds(&dialog.kind, key) else {
            return;
        };
        let Some(state) = dialog.fields.get(key) else {
            return;
        };
        let value = match parse(&text_input(state, cx)) {
            Ok(None) => min.max(0.),
            Ok(Some(value)) => {
                let next = value + delta;
                if next < min || max.is_some_and(|max| next > max) {
                    return;
                }
                next
            }
            Err(()) => return,
        };
        self.set_dialog_field(key, display(Some(value)), window, cx);
    }
    pub(crate) fn number_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event.keystroke.key.as_str(), "up" | "down" | "enter") {
            return;
        }
        let Some(key) = self.dialog.as_ref().and_then(|dialog| {
            dialog
                .fields
                .iter()
                .find(|(key, state)| {
                    bounds(&dialog.kind, key).is_some()
                        && state.read(cx).focus_handle(cx).is_focused(window)
                })
                .map(|(key, _)| key.clone())
        }) else {
            return;
        };
        match event.keystroke.key.as_str() {
            "up" => self.number_step(&key, 1., window, cx),
            "down" => self.number_step(&key, -1., window, cx),
            _ => self.number_event(
                &key,
                &InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                },
                window,
                cx,
            ),
        }
        window.prevent_default();
        cx.stop_propagation();
    }
    pub(crate) fn finish_focused_number(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.dialog.as_ref().and_then(|dialog| {
            dialog
                .fields
                .iter()
                .find(|(key, state)| {
                    bounds(&dialog.kind, key).is_some()
                        && state.read(cx).focus_handle(cx).is_focused(window)
                })
                .map(|(key, _)| key.clone())
        }) {
            self.number_event(&key, &InputEvent::Blur, window, cx);
        }
    }
}
