#[cfg(feature = "ui-qa")]
pub(super) mod json_diagnostics_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod tab_close_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod connection_tabs_qa;
pub(super) mod numbers;
pub(super) mod csv;
mod new_key;
mod add_entries;
mod edit_entries;
mod entry_panel;
mod entry_picker;
mod entry_conversion;
#[cfg(feature = "ui-qa")]
pub(super) mod entry_conversion_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod entry_preferences_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod entry_picker_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod entry_panel_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod edit_entries_qa;
#[cfg(feature="ui-qa")]
pub(super) mod add_entries_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod new_key_qa;
mod csv_units;
mod csv_list;
#[cfg(feature = "ui-qa")]
pub(super) mod csv_units_qa;
pub(super) mod csv_transfer;
#[cfg(feature = "ui-qa")]
mod csv_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod csv_selection_qa;
#[cfg(feature = "ui-qa")]
pub(super) mod csv_list_qa;
use super::views::menu_action;
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use gpui_component::input::Textarea;
use serde::de::DeserializeOwned;

fn form_input(state: &Entity<InputState>) -> Input {
    // Input::h controls multiline editors. Single-line height belongs to Styled.
    Styled::h(Input::new(state), px(32.))
        .text_size(px(14.))
        .px(px(11.))
}

fn disabled_control_color(cx: &App) -> Hsla {
    if cx.theme().mode == ThemeMode::Dark {
        rgba(0xffffff61).into()
    } else {
        rgb(0xc2c2c2).into()
    }
}

fn disabled_control_background(cx: &App) -> Hsla {
    if cx.theme().mode == ThemeMode::Dark {
        rgba(0xffffff0f).into()
    } else {
        rgb(0xfafafc).into()
    }
}

#[derive(Clone)]
pub(super) enum DialogKind {
    Connection(Option<Connection>),
    ConnectionCopy(Connection),
    ConfirmEdit(Connection),
    Group(Option<String>),
    DeleteGroup(String),
    DeleteConnection(String),
    NewKey,
    AddEntries,
    EditEntry(Entry),
    Rename,
    Ttl,
    DeleteKeys,
    DeleteKey(Vec<u8>),
    Flush,
    Preferences,
    Decoder(Option<usize>),
    Import,
    Export,
    About,
    Community,
}
#[derive(Clone)]
struct Row {
    id: Entity<InputState>,
    value: RowValue,
    score: Entity<InputState>,
}
#[derive(Clone)]
enum RowValue {
    Single(Entity<InputState>),
    Multi(Entity<TextareaState>),
}
impl TextField for RowValue {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Single(state) => text_input(state, cx),
            Self::Multi(state) => text_input(state, cx),
        }
    }
    fn set(&self, value: SharedString, w: &mut Window, cx: &mut App) {
        match self {
            Self::Single(state) => set_input(state, value, w, cx),
            Self::Multi(state) => set_input(state, value, w, cx),
        }
    }
}
impl RowValue {
    fn render(&self) -> AnyElement {
        match self {
            Self::Single(state) => form_input(state).into_any_element(),
            Self::Multi(state) => Textarea::new(state).into_any_element(),
        }
    }
}
fn row_value(value: String, multi: bool, w: &mut Window, cx: &mut App) -> RowValue {
    if multi {
        RowValue::Multi(cx.new(|cx| {
            let mut state = TextareaState::new(w, cx)
                .auto_grow(1, 4)
                .placeholder(localized("值"));
            state.set_placeholder_colors(rgb(0xc2c2c2).into(), rgba(0xffffff61).into(), cx);
            state.set_value(value, w, cx);
            state
        }))
    } else {
        RowValue::Single(input(value, "", false, w, cx))
    }
}
#[derive(Clone)]
struct Target {
    id: u64,
    conn: Connection,
    db: u32,
    key: Vec<u8>,
    kind: KeyType,
    keys: Vec<Vec<u8>>,
}
pub(super) struct Dialog {
    id: u64,
    format_epoch: u64,
    format_loading: bool,
    kind: DialogKind,
    data: Json,
    fields: BTreeMap<String, Entity<InputState>>,
    numbers: BTreeMap<String, Option<f64>>,
    general_preview_size: f32,
    editors: BTreeMap<String, Entity<EditorState>>,
    arguments: BTreeMap<String, Vec<Entity<InputState>>>,
    rows: Vec<Row>,
    pub(super) tab: String,
    title: String,
    target: Option<Target>,
    progress: Arc<Mutex<String>>,
    previewed: bool,
    original_preferences: Option<Preferences>,
    return_focus: Option<FocusHandle>,
    font_menu: Option<String>,
    font_cursor: usize,
    font_scroll: ScrollHandle,
    connection_testing: bool,
    connection_test: Option<std::result::Result<(), String>>,
    connection_errors: BTreeMap<String, ConnectionFieldError>,
    sentinel_masters: Vec<String>,
    csv_filename: Option<file_dialogs::CsvFilename>,
    csv_unit_picker: csv_units::UnitPicker,
    csv_key_scroll: ScrollHandle,
    csv_scrollbar: csv_list::ScrollbarState,
    new_key: Option<new_key::NewKeyState>,
    entry_picker: entry_picker::EntryPickerState,
    entry_content: Option<entry_conversion::EntryContent>,
}
impl Dialog {
    fn entry_changed(&self, cx: &App) -> bool {
        ["value", "field", "score"].iter()
            .any(|key| self.value(key, cx) != self.data[*key].as_str().unwrap_or_default())
    }
    fn value(&self, key: &str, cx: &App) -> String {
        if key == "value" && let Some(content) = &self.entry_content {
            return content.editing.clone();
        }
        if let Some(f) = self.editors.get(key) {
            text_input(f, cx)
        } else {
            self.fields
                .get(key)
                .map(|f| text_input(f, cx))
                .unwrap_or_default()
        }
    }
    fn boolean(&self, key: &str) -> bool {
        self.data
            .pointer(&format!("/{}", key.replace('.', "/")))
            .and_then(Json::as_bool)
            .unwrap_or_default()
    }
    fn result<T: DeserializeOwned>(&self, cx: &App) -> Result<T> {
        let mut value = self.data.clone();
        let fields = self
            .fields
            .iter()
            .filter(|(key, _)| !self.numbers.contains_key(*key))
            .map(|(key, field)| (key.clone(), field.clone()))
            .collect();
        apply_fields(
            &mut value,
            "",
            &fields,
            matches!(
                self.kind,
                DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
            ),
            cx,
        )?;
        for (key, number) in &self.numbers {
            if let Some(value) = value.pointer_mut(&format!("/{}", key.replace('.', "/"))) {
                *value = numbers::json(*number);
            }
        }
        for (key, field) in &self.editors {
            if let Some(v) = value.pointer_mut(&format!("/{}", key.replace('.', "/"))) {
                let text = text_input(field, cx);
                *v = if matches!(v, Json::String(_)) {
                    Json::String(text)
                } else {
                    serde_json::from_str(&text)?
                };
            }
        }
        for (key, fields) in &self.arguments {
            if key == "db_filter_list" {
                let mut indices = fields
                    .iter()
                    .map(|field| text_input(field, cx).parse::<u32>())
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                indices.sort_unstable();
                indices.dedup();
                value[key] = serde_json::to_value(indices)?;
            } else {
                value[key] = Json::Array(
                    fields
                        .iter()
                        .map(|field| Json::String(text_input(field, cx)))
                        .collect(),
                );
            }
        }
        if matches!(
            self.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            let mut aliases = serde_json::Map::new();
            for row in &self.rows {
                let alias = text_input(&row.value, cx);
                if !alias.is_empty() {
                    let db = self
                        .numbers
                        .get(&numbers::alias_key(&row.id))
                        .copied()
                        .flatten();
                    aliases.insert(numbers::object_key(db), Json::String(alias));
                }
            }
            value["alias"] = Json::Object(aliases);
        }
        Ok(serde_json::from_value(value)?)
    }
}
fn flatten_fields(
    value: &Json,
    prefix: &str,
    map: &mut BTreeMap<String, Entity<InputState>>,
    window: &mut Window,
    cx: &mut App,
) {
    if prefix == "alias" {
        return;
    }
    match value {
        Json::Object(items) => {
            for (k, v) in items {
                let name = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_fields(v, &name, map, window, cx);
            }
        }
        Json::Bool(_) => {}
        _ => {
            let text = match value {
                Json::String(s) => s.clone(),
                Json::Null => String::new(),
                Json::Number(n) => n
                    .as_f64()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| n.to_string()),
                _ => value.to_string(),
            };
            let multiline = matches!(
                prefix,
                "value" | "message" | "alias" | "decode_args" | "encode_args"
            );
            let state = input(text, "", multiline, window, cx);
            if prefix.ends_with("password") || prefix.ends_with("passphrase") {
                state.update(cx, |i, cx| {
                    i.set_masked(true, window, cx);
                });
            }
            map.insert(prefix.into(), state);
        }
    }
}
fn apply_fields(
    value: &mut Json,
    prefix: &str,
    fields: &BTreeMap<String, Entity<InputState>>,
    nullable_numbers: bool,
    cx: &App,
) -> Result<()> {
    if prefix == "alias"
        && let Some(field) = fields.get(prefix)
    {
        *value = serde_json::from_str(&text_input(field, cx))?;
        return Ok(());
    }
    match value {
        Json::Object(items) => {
            for (k, v) in items {
                let name = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                apply_fields(v, &name, fields, nullable_numbers, cx)?;
            }
        }
        Json::Bool(_) => {}
        _ => {
            if let Some(input) = fields.get(prefix) {
                let text = text_input(input, cx);
                *value = match value {
                    Json::String(_) => Json::String(text),
                    Json::Number(_) | Json::Null if nullable_numbers && text.is_empty() => {
                        Json::from(0)
                    }
                    Json::Null if nullable_numbers => serde_json::from_str(&text)
                        .map_err(|_| anyhow::anyhow!("{prefix} 必须是数字"))?,
                    Json::Number(_) => serde_json::from_str(&text)
                        .map_err(|_| anyhow::anyhow!("{prefix} 必须是数字"))?,
                    Json::Array(_) => serde_json::from_str(&text)
                        .map_err(|_| anyhow::anyhow!("{prefix} 必须是 JSON 数组"))?,
                    _ => serde_json::from_str(&text).unwrap_or(Json::String(text)),
                };
            }
        }
    }
    Ok(())
}
fn set_json_boolean(value: &mut Json, key: &str, new: bool) {
    if let Some(v) = value.pointer_mut(&format!("/{}", key.replace('.', "/"))) {
        *v = Json::Bool(new);
    }
}
impl RdmApp {
    fn preference_dialog(&self) -> Option<&Dialog> {
        self.dialog
            .as_ref()
            .filter(|dialog| matches!(dialog.kind, DialogKind::Preferences))
            .or_else(|| {
                self.dialog_stack
                    .iter()
                    .rev()
                    .find(|dialog| matches!(dialog.kind, DialogKind::Preferences))
            })
    }
    pub(super) fn general_font_size(&self) -> f32 {
        self.preference_dialog()
            .map(|dialog| dialog.general_preview_size)
            .unwrap_or(self.general_css_size)
    }
    pub(super) fn editor_font_size(&self) -> f32 {
        let size = self
            .preference_dialog()
            .map(|dialog| {
                dialog
                    .numbers
                    .get("editor.font_size")
                    .copied()
                    .flatten()
                    .unwrap_or(0.) as f32
            })
            .unwrap_or(self.pref.editor.font_size);
        if size == 0. { 14. } else { size }
    }
    pub(super) fn cli_font_size(&self) -> f32 {
        let size = self
            .preference_dialog()
            .map(|dialog| {
                dialog
                    .numbers
                    .get("cli.font_size")
                    .copied()
                    .flatten()
                    .unwrap_or(0.) as f32
            })
            .unwrap_or(self.pref.cli.font_size);
        if size == 0. { 14. } else { size }
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_font_size(&self, expected: f32, cx: &App) {
        assert_eq!(
            self.general_font_size(),
            expected,
            "source CSS general font size"
        );
        assert_eq!(
            cx.theme().font_size,
            px(14.),
            "source modal and controls keep the common 14px font"
        );
    }

    fn watch_dialog_input(
        &mut self,
        key: String,
        state: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.subs.push(cx.subscribe_in(
            state,
            window,
            move |app, state, event: &InputEvent, window, cx| {
                let current = app
                    .dialog
                    .as_ref()
                    .and_then(|dialog| dialog.fields.get(&key))
                    .is_some_and(|field| field.entity_id() == state.entity_id());
                if current {
                    app.number_event(&key, event, window, cx);
                    if matches!(event, InputEvent::Change) {
                        app.new_key_input_changed(&key, cx);
                        app.validate_connection_input(&key, state.entity_id(), cx);
                        if key == "path" && let Some(dialog)=&mut app.dialog
                            && let Some(cache)=&mut dialog.csv_filename {
                            cache.path_changed(text_input(state,cx),chrono::Local::now());
                        }
                    }
                }
                cx.notify();
            },
        ));
    }

    pub(super) fn duplicate_connection(
        &mut self,
        mut connection: Connection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = connection.name.clone();
        let names = flatten(&self.connections)
            .into_iter()
            .map(|c| c.name)
            .collect::<HashSet<_>>();
        let mut suffix = 2;
        loop {
            let name = format!("{base}{suffix}");
            if !names.contains(&name) {
                connection.name = name;
                break;
            }
            suffix += 1;
        }
        self.open_dialog(DialogKind::ConnectionCopy(connection), window, cx);
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_focus_number(&self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog
            .as_ref()
            .unwrap()
            .fields
            .get(key)
            .unwrap()
            .update(cx, |input, cx| input.focus(window, cx));
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_number_text(&self, key: &str, expected: &str, cx: &App) {
        assert_eq!(
            self.dialog.as_ref().unwrap().value(key, cx),
            expected,
            "original numeric text for {key}"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_number_value(&self, key: &str, expected: &Json, cx: &App) {
        let value = self.dialog.as_ref().unwrap().result::<Json>(cx).unwrap();
        let actual = value
            .pointer(&format!("/{}", key.replace('.', "/")))
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(
            actual,
            expected.as_f64().unwrap_or(0.),
            "original numeric model for {key}"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_number_focus(
        &self,
        key: &str,
        focused: bool,
        window: &Window,
        cx: &App,
    ) {
        let state = self.dialog.as_ref().unwrap().fields.get(key).unwrap();
        assert_eq!(
            state.read(cx).focus_handle(cx).is_focused(window),
            focused,
            "original Enter focus for {key}"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_preference_rpc(&self, accepted: bool, cx: &App) {
        let value = self.dialog.as_ref().unwrap().result::<Json>(cx).unwrap();
        assert_eq!(
            Preferences::from_form_json(value).is_ok(),
            accepted,
            "original Go preference parameter parsing"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_prepare_alias_number(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        self.close_dialog(window, cx);
        self.open_dialog(DialogKind::Connection(None), window, cx);
        self.add_alias_row(None, window, cx);
        let dialog = self.dialog.as_mut().unwrap();
        dialog.tab = "alias".into();
        let row = &dialog.rows[0];
        set_input(&row.value, "fixture", window, cx);
        numbers::alias_key(&row.id)
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_alias_number(&self, expected: &Json, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        assert_eq!(
            dialog.result::<Json>(cx).unwrap()["alias"],
            *expected,
            "original alias numeric model"
        );
        let accepted = expected
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key.parse::<i64>().is_ok());
        assert_eq!(
            dialog.result::<Connection>(cx).is_ok(),
            accepted,
            "alias keys must match Go integer map parameters"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_connection_validation(
        &self,
        expected: &[(&str, ConnectionFieldError)],
        tab: &str,
    ) {
        let dialog = self.dialog.as_ref().unwrap();
        let errors = expected
            .iter()
            .map(|(key, error)| (key.to_string(), *error))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            dialog.connection_errors, errors,
            "source connection feedback"
        );
        assert_eq!(dialog.tab, tab, "source validation tab");
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_connection_tree_dialog(&self, kind: &str, name: &str, cx: &App) {
        let dialog = self.dialog.as_ref().expect("tree action opens its dialog");
        match (&dialog.kind, kind) {
            (DialogKind::Connection(Some(_)), "edit")
            | (DialogKind::ConnectionCopy(_), "copy")
            | (DialogKind::Group(Some(_)), "group") => assert_eq!(dialog.value("name", cx), name),
            (DialogKind::DeleteConnection(value), "delete")
            | (DialogKind::DeleteGroup(value), "delete-group") => assert_eq!(value, name),
            _ => panic!("unexpected connection tree dialog"),
        }
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_incomplete_connection_form(&self, ssh: bool, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        for key in [
            "addr",
            "port",
            "conn_timeout",
            "exec_timeout",
            "load_size",
            "ssh.port",
        ] {
            assert_eq!(dialog.value(key, cx), "", "source omitted field {key}");
        }
        assert_eq!(dialog.value("ssh.login_type", cx), "pwd");
        assert_eq!(dialog.boolean("ssh.enable"), ssh);
        let profile = dialog.result::<Connection>(cx).unwrap();
        assert_eq!(profile.port, 0);
        assert_eq!(profile.ssh.port, 0);
        assert_eq!(profile.conn_timeout, 0);
        assert_eq!(profile.ssh.username, if ssh { "user" } else { "" });
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_type_connection_input(
        &mut self,
        key: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self
            .dialog
            .as_ref()
            .unwrap()
            .fields
            .get(key)
            .unwrap()
            .clone();
        state.update(cx, |state, cx| {
            let end = state.value().encode_utf16().count();
            state.replace_text_in_range(Some(0..end), text, window, cx);
        });
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn general_font_stack(&self, cx: &App) -> Vec<String> {
        self.preference_font_stack("general", cx)
    }
    pub(super) fn preference_font_stack(&self, section: &str, cx: &App) -> Vec<String> {
        self.preference_dialog()
            .and_then(|dialog| {
                serde_json::from_str(&dialog.value(&format!("{section}.font_family"), cx)).ok()
            })
            .unwrap_or_else(|| match section {
                "editor" => self.pref.editor.font_family.clone(),
                "cli" => self.pref.cli.font_family.clone(),
                _ => self.pref.general.font_family.clone(),
            })
    }
    pub(super) fn preference_font(&self, section: &str, default: &str, cx: &App) -> gpui::Font {
        let families = self.preference_font_stack(section, cx);
        // CSS selects a resolvable family, including fonts omitted by inventories.
        let available = |family: &String| -> Option<String> {
            if let Some(name) = self.fonts.iter().find(|name| name.eq_ignore_ascii_case(family)) {
                return Some(name.clone());
            }
            #[cfg(target_os = "macos")]
            { super::text_control_width::available_family(family) }
            #[cfg(not(target_os = "macos"))]
            { None }
        };
        let primary = families.iter().find_map(&available);
        // A nonempty general stack replaces Naive's inherited system stack.
        // With no installed family, macOS WebKit uses its standard Times face
        // (verified against the original component, including its CJK hint).
        let default = if cfg!(target_os = "macos") && section == "general" && !families.is_empty() {
            "Times"
        } else {
            default
        };
        let mut font = gpui::font(primary.unwrap_or_else(|| default.into()));
        font.fallbacks = Some(FontFallbacks::from_fonts(
            families
                .into_iter()
                .filter(|family| !family.is_empty())
                // CSS skips unavailable entries in the general font cascade.
                // Keep the standard fallback identical to selecting it directly.
                .filter_map(|family| if section == "general" { available(&family) } else { Some(family) })
                .filter(|family| !family.eq_ignore_ascii_case(font.family.as_ref()))
                .collect(),
        ));
        font
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_type_font_query(
        &self,
        key: &str,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.dialog.as_ref().unwrap().fields[&format!("font_query.{key}")].update(
            cx,
            |input, cx| {
                input.focus(window, cx);
                input.replace_text_in_range(None, text, window, cx);
            },
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_font_filter(&self, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        assert_eq!(dialog.font_menu.as_deref(), Some("general.font_family"));
        assert_eq!(
            dialog.value("font_query.general.font_family", cx),
            "QA Custom Font"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_font_tags(&self, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        assert_eq!(
            dialog
                .result::<Preferences>(cx)
                .unwrap()
                .general
                .font_family,
            ["Monaco", "QA Custom Font"]
        );
        assert!(
            dialog.font_menu.is_none(),
            "Clicking outside dismisses the font list"
        );
        assert!(
            self.pref.general.font_family.is_empty(),
            "font preview must not change persisted preferences"
        );
        assert_eq!(
            self.preference_font("general", ".SystemUIFont", cx)
                .family
                .as_ref(),
            "Monaco"
        );
        assert_eq!(
            cx.theme().font_family.as_ref(),
            ".SystemUIFont",
            "source modal font stays unchanged"
        );
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_font_keyboard(&self, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        assert_eq!(
            dialog
                .result::<Preferences>(cx)
                .unwrap()
                .general
                .font_family,
            ["Monaco"]
        );
        assert!(dialog.font_menu.is_none());
        assert!(self.pref.general.font_family.is_empty());
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_focus_database_filter(&self, window: &mut Window, cx: &mut App) {
        self.dialog.as_ref().unwrap().fields["db_filter_input"]
            .update(cx, |input, cx| input.focus(window, cx));
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_database_tags(&self, cx: &App) {
        let dialog = self.dialog.as_ref().unwrap();
        let values = dialog.arguments["db_filter_list"]
            .iter()
            .map(|field| text_input(field, cx))
            .collect::<Vec<_>>();
        assert_eq!(values, ["0", "3"]);
        assert_eq!(dialog.value("db_filter_input", cx), "");
    }
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_assert_connection_result(&self, expected_success: bool) {
        let dialog = self.dialog.as_ref().unwrap();
        assert!(!dialog.connection_testing);
        assert_eq!(
            dialog.connection_test.as_ref().unwrap().is_ok(),
            expected_success
        );
    }
    pub(super) fn decoder_options(&self) -> Vec<(String, String)> {
        let mut options = codec::DECODERS
            .iter()
            .map(|name| (name.to_string(), name.to_string()))
            .collect::<Vec<_>>();
        for decoder in &self.pref.decoder {
            options.retain(|(name, _)| *name != decoder.name);
            options.push((decoder.name.clone(), decoder.name.clone()));
        }
        options
    }
    pub(super) fn edit_row(&mut self, row: Entry, window: &mut Window, cx: &mut Context<Self>) {
        let pinned = self.dialog.as_ref().is_some_and(|dialog| {
            matches!(dialog.kind, DialogKind::EditEntry(_)) && dialog.boolean("pin")
        });
        let full = self.full_editor;
        self.open_dialog(DialogKind::EditEntry(row), window, cx);
        if let Some(dialog) = &mut self.dialog {
            set_json_boolean(&mut dialog.data, "pin", pinned);
        }
        self.full_editor = full;
    }
    pub(super) fn editing_row(&self, id: &[u8]) -> bool {
        self.displayed_entry_dialog().is_some_and(
            |dialog| matches!(&dialog.kind, DialogKind::EditEntry(row) if row.id.as_slice() == id),
        )
    }
    pub(super) fn edit_connection(
        &mut self,
        connection: Connection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.iter().any(|tab| tab.conn.name == connection.name) {
            self.open_dialog(DialogKind::ConfirmEdit(connection), window, cx);
        } else {
            self.open_dialog(DialogKind::Connection(Some(connection)), window, cx);
        }
    }
    pub(super) fn dismiss_entry(&mut self) {
        if self
            .dialog
            .as_ref()
            .is_some_and(|d| matches!(d.kind, DialogKind::EditEntry(_)))
        {
            self.dialog = None;
            self.full_editor = false;
        }
    }
    pub(super) fn activate_connection_tab(&mut self, next: Option<u64>, cx: &mut Context<Self>) {
        if self.active == next { return; }
        let live = self.tabs.iter().map(|tab| tab.id).collect::<HashSet<_>>();
        self.cached_entry_panels.retain(|id, _| live.contains(id));
        // The original renders a separate ContentPane for each connection and
        // switches it with v-show. Keep the row editor and its existing model.
        if self.editing_entry() {
            let dialog = self.dialog.take().unwrap();
            if let Some(id) = dialog.target.as_ref().map(|target| target.id).filter(|id| live.contains(id)) {
                self.cached_entry_panels.insert(id, (dialog, self.full_editor));
            }
        }
        while let Some(index) = self.dialog_stack.iter().position(|dialog| matches!(dialog.kind, DialogKind::EditEntry(_))) {
            let dialog = self.dialog_stack.remove(index);
            let full = dialog.boolean("rdm_full_editor");
            if let Some(id) = dialog.target.as_ref().map(|target| target.id).filter(|id| live.contains(id)) {
                self.cached_entry_panels.insert(id, (dialog, full));
            }
        }
        self.active = next;
        if let Some(index) = self.index() { self.connection_tabs_scroll.scroll_to_item(index); }
        self.full_editor = false;
        if self.dialog.is_some() {
            // A modal may remain open after a tab closes. Its previous editor
            // can now be hidden or destroyed, so restore focus to the body.
            let body = self.focus.clone();
            for dialog in self.dialog.iter_mut().chain(self.dialog_stack.iter_mut()) {
                dialog.return_focus = Some(body.clone());
            }
        }
        if let Some((mut dialog, full)) = next.and_then(|id| self.cached_entry_panels.remove(&id)) {
            if self.dialog.is_none() {
                self.dialog = Some(dialog);
                self.full_editor = full;
            } else {
                dialog.data["rdm_full_editor"] = Json::Bool(full);
                self.dialog_stack.insert(0, dialog);
            }
        }
        cx.notify();
    }
    pub(super) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.as_ref().is_some_and(|dialog| matches!(dialog.kind, DialogKind::Export)) {
            gpui_base::TextSelection::clear(window, cx);
        }
        if let Some(token) = self.transfer.take() {
            token.cancel();
        }
        if let Some(original) = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.original_preferences.clone())
        {
            self.pref = original;
            self.general_css_size = self.pref.general.font_size;
        }
        let return_focus = self.dialog.as_ref().and_then(|dialog| dialog.return_focus.clone());
        self.dialog = self.dialog_stack.pop();
        self.full_editor = self
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.boolean("rdm_full_editor"));
        self.apply_theme(window, cx);
        self.load_language();
        if let Some(focus) = return_focus {
            focus.focus(window, cx);
        } else if let Some(editor) = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.editors.get("value"))
            .cloned()
        {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
    }
    fn open_nested_dialog(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let return_focus = window.focused(cx);
        if let Some(mut parent) = self.dialog.take() {
            parent.data["rdm_full_editor"] = Json::Bool(self.full_editor);
            self.dialog_stack.push(parent);
        }
        self.open_dialog(kind, window, cx);
        if let Some(dialog) = &mut self.dialog {
            dialog.return_focus = return_focus;
        }
    }
    pub(super) fn set_dialog_field(
        &mut self,
        key: &str,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if key == "type" && self.dialog.as_ref().is_some_and(|d| matches!(d.kind, DialogKind::NewKey)) {
            self.change_new_key_type(value.into().to_string(), window, cx);
            return;
        }
        if let Some(d) = &mut self.dialog {
            let value = value.into();
            if d.numbers.contains_key(key) {
                if let Ok(number) = numbers::parse(&value) {
                    d.numbers.insert(key.into(), number);
                    if key == "general.font_size"
                        && let Some(size) = number
                    {
                        d.general_preview_size = size as f32;
                    }
                }
            }
            if let Some(f) = d.editors.get(key) {
                if key == "value" && let Some(content) = &mut d.entry_content {
                    content.editing = value.to_string();
                }
                set_input(f, value.clone(), window, cx);
                if key == "value" && d.entry_content.is_some() {
                    d.refresh_entry_json(window,cx);
                }
            } else if let Some(f) = d.fields.get(key) {
                set_input(f, value.clone(), window, cx);
            } else if d
                .data
                .pointer(&format!("/{}", key.replace('.', "/")))
                .is_some_and(Json::is_boolean)
            {
                set_json_boolean(&mut d.data, key, value == "true");
            }
            if matches!(d.kind, DialogKind::Import) && key == "unit" {
                self.csv_import_unit = d.value("unit", cx).parse().unwrap_or(1);
            }
            if matches!(d.kind, DialogKind::Export) && key == "include_ttl" {
                self.csv_export_expire = d.boolean(key);
            }
            if key == "general.theme" {
                let current = self.pref.general.theme.clone();
                self.pref.general.theme = d.value(key, cx);
                self.apply_theme(window, cx);
                self.pref.general.theme = current;
            }
            if key == "general.language" {
                let current = self.pref.general.language.clone();
                self.pref.general.language = value.to_string();
                self.load_language();
                self.pref.general.language = current;
                let title = self.tr("menu.preferences");
                if let Some(dialog) = &mut self.dialog {
                    dialog.title = title;
                }
            }
            if matches!(
                key,
                "general.font_family"
                    | "general.font_size"
                    | "editor.font_family"
                    | "editor.font_size"
                    | "cli.font_family"
                    | "cli.font_size"
            ) {
                self.apply_theme(window, cx);
            }
            if key == "db_filter_type" && value != "none" {
                if let Some(dialog) = &mut self.dialog {
                    let fields = dialog.arguments.entry("db_filter_list".into()).or_default();
                    if fields.is_empty() {
                        fields.push(input("0", "", false, window, cx));
                    }
                }
            }
            cx.notify();
        }
    }
    pub(super) fn open_decoder_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The original dropdown action removes its focused menu and leaves
        // focus on the page; the preferences modal has auto-focus disabled.
        self.focus.focus(window, cx);
        if self.dialog.is_some() {
            self.open_nested_dialog(DialogKind::Preferences, window, cx);
        } else {
            self.open_dialog(DialogKind::Preferences, window, cx);
        }
        if let Some(dialog) = &mut self.dialog {
            dialog.tab = "decoder".into();
        }
        self.focus.focus(window, cx);
    }
    pub(super) fn editing_entry(&self) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|d| matches!(d.kind, DialogKind::EditEntry(_)))
    }
    // The inline editor is content behind a modal, rather than the modal itself.
    fn displayed_entry_dialog(&self) -> Option<&Dialog> {
        self.dialog.iter().chain(self.dialog_stack.iter().rev())
            .find(|dialog| matches!(dialog.kind, DialogKind::EditEntry(_)))
    }
    pub(super) fn entry_panel_visible(&self) -> bool {
        self.displayed_entry_dialog().is_some()
    }
    pub(super) fn entry_panel_fullscreen(&self) -> bool {
        if self.editing_entry() {
            self.full_editor
        } else {
            self.displayed_entry_dialog().is_some_and(|dialog| dialog.boolean("rdm_full_editor"))
        }
    }
    pub(super) fn entry_editor_focused(&self,window:&Window,cx:&App)->bool {
        self.dialog.as_ref().and_then(|dialog|dialog.editors.get("value")).is_some_and(|editor|editor.focus_handle(cx).contains_focused(window,cx))
    }
    pub(super) fn entry_editor_id(&self) -> Option<EntityId> {
        self.dialog.as_ref().and_then(|dialog| dialog.editors.get("value"))
            .map(|editor| editor.entity_id())
    }
    fn entry_changed(&self, cx: &App) -> bool {
        self.dialog.as_ref().is_some_and(|dialog| dialog.entry_changed(cx))
    }
    pub(super) fn render_entry_panel(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        self.render_collection_entry_panel(window,cx)
    }
    fn connection_general(&self, cx: &Context<Self>) -> AnyElement {
        let d = self.dialog.as_ref().unwrap();
        let weak = cx.weak_entity();
        let network = d.value("network", cx);
        let invalid_address = d.connection_errors.contains_key("addr");
        let group = d.value("group", cx);
        let groups = self
            .connections
            .iter()
            .filter(|c| c.category == "group")
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        let group_selector = self
            .btn("connection-group", "", "", cx, |_, _, _| {})
            .child(div().flex_1().text_left().child(if group.is_empty() {
                self.tr("dialogue.connection.no_group")
            } else {
                group
            }))
            .child(icon("down").size(px(12.)).text_color(rgb(0xc2c2c2)))
            .w_full()
            .dropdown_menu(move |mut m, _, _| {
                let mut groups = groups.clone();
                groups.insert(0, String::new());
                for g in groups {
                    let value = g.clone();
                    m = m.item(menu_action(
                        if g.is_empty() { "无分组".into() } else { g },
                        weak.clone(),
                        move |a, w, cx| {
                            if let Some(d) = &a.dialog
                                && let Some(f) = d.fields.get("group")
                            {
                                set_input(f, value.clone(), w, cx);
                                cx.notify();
                            }
                        },
                    ));
                }
                m
            });
        let weak = cx.weak_entity();
        let select = self
            .btn("connection-network", "", "", cx, |_, _, _| {})
            .child(
                div()
                    .flex_1()
                    .text_left()
                    .when(network.is_empty(), |label| label.text_color(rgb(0xc2c2c2)))
                    .child(match network.as_str() {
                        "unix" => "UNIX",
                        "tcp" => "TCP",
                        _ => self.control_placeholder(true),
                    }),
            )
            .child(icon("down").size(px(12.)).text_color(rgb(0xc2c2c2)))
            .w(px(100.))
            .flex_shrink_0()
            .when(invalid_address, |field| {
                field.border_color(self.form_error_color(cx))
            })
            .dropdown_menu(move |mut m, _, _| {
                for value in ["tcp", "unix"] {
                    m = m.item(menu_action(
                        value.to_uppercase(),
                        weak.clone(),
                        move |a, w, cx| {
                            if let Some(d) = &a.dialog
                                && let Some(f) = d.fields.get("network")
                            {
                                set_input(f, value, w, cx);
                                cx.notify();
                            }
                        },
                    ));
                }
                m
            });
        let address = div()
            .flex()
            .items_center()
            .child(select)
            .when(network == "unix", |row| {
                row.child(div().flex_1().min_w_0().child(
                    form_input(d.fields.get("sock").unwrap()).h(px(32.)).when(
                        invalid_address,
                        |field| {
                            field
                                .border_color(self.form_error_color(cx))
                                .focus_bordered(false)
                        },
                    ),
                ))
            })
            .when(network != "unix", |row| {
                row.child(
                    div().flex_1().min_w_0().child(
                        form_input(d.fields.get("addr").unwrap())
                            .h(px(32.))
                            .when(invalid_address, |field| {
                                field
                                    .border_color(self.form_error_color(cx))
                                    .focus_bordered(false)
                            })
                            .rounded_l(px(0.)),
                    ),
                )
                .child(div().w(px(18.)).flex_shrink_0().text_center().child(":"))
                .child(
                    self.number_input("port", cx)
                        .h(px(32.))
                        .when(invalid_address, |field| {
                            field
                                .border_color(self.form_error_color(cx))
                                .focus_bordered(false)
                        })
                        .w(px(100.))
                        .flex_shrink_0(),
                )
            });
        div()
            .flex()
            .flex_col()
            .child(self.dialog_input("name", &self.tr("dialogue.connection.conn_name"), false, cx))
            .when(!matches!(d.kind, DialogKind::Connection(Some(_))), |row| {
                row.child(
                    div()
                        .flex()
                        .flex_col()
                        .mb(px(18.))
                        .gap(px(5.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(16.25))
                                .font_weight(FontWeight(450.))
                                .pl(px(2.))
                                .text_color(rgb(0x717880))
                                .child(self.tr("dialogue.connection.group")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(div().flex_1().min_w_0().child(group_selector))
                                .child(
                                    self.icon_btn(
                                        "connection-new-group",
                                        "addgroup",
                                        self.tr("interface.new_group"),
                                        cx,
                                        |a, w, cx| {
                                            a.open_nested_dialog(DialogKind::Group(None), w, cx)
                                        },
                                    )
                                    .with_size(gpui_component::Size::Size(px(26.6667)))
                                    .size(px(20.))
                                    .p_0(),
                                ),
                        ),
                )
            })
            .child(self.form_field_for(
                "addr",
                &self.tr("dialogue.connection.addr"),
                address.into_any_element(),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(div().flex_1().min_w_0().child(self.dialog_input(
                        "password",
                        &self.tr("dialogue.connection.pwd"),
                        false,
                        cx,
                    )))
                    .child(div().flex_1().min_w_0().child(self.dialog_input(
                        "username",
                        &self.tr("dialogue.connection.usr"),
                        false,
                        cx,
                    ))),
            )
            .into_any_element()
    }
    #[cfg(feature="ui-qa")]
    pub(super) fn qa_assert_key_delete_warning(&self, key: &[u8]) {
        assert!(matches!(&self.dialog.as_ref().unwrap().kind, DialogKind::DeleteKey(target) if target==key));
    }
    pub(super) fn set_dialog_keys(&mut self, keys: Vec<Vec<u8>>) {
        if let Some(dialog) = &mut self.dialog && let Some(target) = &mut dialog.target {
            target.keys = keys;
            dialog.previewed = true;
        }
    }
    pub(super) fn open_dialog(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(kind, DialogKind::Preferences) && self.editing_entry() {
            let id = self.dialog.as_ref().unwrap().id;
            self.close_entry_picker(id, window, cx);
            self.open_nested_dialog(kind, window, cx);
            return;
        }
        if let DialogKind::Connection(Some(connection)) | DialogKind::ConnectionCopy(connection) = &kind {
            // The original GetConnection action replaces the frontend profile
            // without a refreshInterval field, whose getter then defaults to 5.
            self.status_refresh_profiles.insert(connection.name.clone(), 5.);
        }
        if matches!(kind, DialogKind::Preferences) {
            self.general_css_size = self.pref.general.font_size;
        }
        let target = self.index().map(|i| {
            let t = &self.tabs[i];
            let key = t.current_key.clone().unwrap_or_default();
            let mut keys = t.selected.iter().cloned().collect::<Vec<_>>();
            if keys.is_empty() && !key.is_empty() {
                keys.push(key.clone());
            }
            Target {
                id: t.id,
                conn: t.conn.clone(),
                db: t.db,
                key,
                kind: t
                    .detail
                    .as_ref()
                    .map(|d| d.kind.clone())
                    .unwrap_or_default(),
                keys,
            }
        });
        let (title, mut data, tab) = match &kind {
            DialogKind::Connection(old) => (
                self.tr(if old.is_some() {
                    "dialogue.connection.edit_title"
                } else {
                    "dialogue.connection.new_title"
                }),
                old.as_ref()
                    .map(Connection::form_data)
                    .unwrap_or_else(|| serde_json::to_value(Connection::form_default()).unwrap()),
                "general",
            ),
            DialogKind::ConnectionCopy(c) => (
                self.tr("dialogue.connection.new_title"),
                c.form_data(),
                "general",
            ),
            DialogKind::ConfirmEdit(_) | DialogKind::DeleteKey(_) => (self.tr("common.warning"), Json::Null, "general"),
            DialogKind::Group(old) => (
                if old.is_some() {
                    "重命名分组"
                } else {
                    "新建分组"
                }
                .into(),
                serde_json::json!({"name":old.clone().unwrap_or_default()}),
                "general",
            ),
            DialogKind::DeleteConnection(n) => {
                ("删除连接".into(), serde_json::json!({"name":n}), "general")
            }
            DialogKind::DeleteGroup(n) => (
                self.tr("interface.remove_conn_group"),
                serde_json::json!({"name":n}),
                "general",
            ),
            DialogKind::Preferences => (
                self.tr("menu.preferences"),
                serde_json::to_value(&self.pref).unwrap(),
                "general",
            ),
            DialogKind::Decoder(index) => (
                "自定义解码".into(),
                serde_json::to_value(
                    index
                        .and_then(|i| self.pref.decoder.get(i).cloned())
                        .unwrap_or_else(|| Decoder {
                            enable: true,
                            auto: true,
                            ..Default::default()
                        }),
                )
                .unwrap(),
                "decode",
            ),
            DialogKind::NewKey => (
                self.tr("dialogue.key.new"),
                serde_json::json!({"key":"","db":target.as_ref().map(|t|t.db).unwrap_or(0),"type":"STRING","ttl":-1,"value":"","stream_id":"*"}),
                "general",
            ),
            DialogKind::AddEntries => (
                self.tr(if target.as_ref().is_some_and(|t|t.kind==KeyType::List){"dialogue.field.new_item"}else{"dialogue.field.new"}),
                serde_json::json!({"key":target.as_ref().map(|t|display_bytes(&t.key)).unwrap_or_default(),"type":target.as_ref().map(|t|t.kind.name().to_uppercase()).unwrap_or_default(),"op_type":0,"value":"","stream_id":"*","reload":self.add_entries_reload}),
                "general",
            ),
            DialogKind::EditEntry(row) => {
                (
                    self.tr("interface.edit_row"),
                    serde_json::json!({"field":display_bytes(&row.id),"row_index":self.index().and_then(|i|self.tabs[i].detail.as_ref()).and_then(|detail|detail.rows.iter().position(|entry|entry.id == row.id)).map(|index|(index+1).to_string()).unwrap_or_default(),"value":"","score":row.score,"format":"Raw","decode":"None","pin":false}),
                    "general",
                )
            }
            DialogKind::Rename => (
                self.tr("interface.rename_key"),
                serde_json::json!({"key":target.as_ref().map(|t|display_bytes(&t.key)).unwrap_or_default(),"new_key":""}),
                "general",
            ),
            DialogKind::Ttl => (
                "设置 TTL".into(),
                serde_json::json!({"ttl":self.index().and_then(|i|self.tabs[i].detail.as_ref().map(|d|d.ttl)).unwrap_or(-1),"unit":1,"key":target.as_ref().map(|t|display_bytes(&t.key)).unwrap_or_default(),"batch":self.index().is_some_and(|i|self.tabs[i].check_mode && !self.tabs[i].selected.is_empty()),"pattern":"*"}),
                "general",
            ),
            DialogKind::DeleteKeys => (
                self.tr("interface.delete_key"),
                serde_json::json!({"pattern":"*","async":true,"direct":false}),
                "general",
            ),
            DialogKind::Flush => (
                self.tr("interface.flush_db"),
                serde_json::json!({"confirm":false,"async":true}),
                "general",
            ),
            DialogKind::Import => (
                self.tr("interface.import_key"),
                serde_json::json!({"server":target.as_ref().map(|t|t.conn.name.clone()).unwrap_or_default(),"db":target.as_ref().map(|t|t.db).unwrap_or_default(),"path":"","conflict":0,"ttl_type":0,"ttl":-1,"unit":self.csv_import_unit,"reload":true}),
                "general",
            ),
            DialogKind::Export => (
                "导出数据".into(),
                serde_json::json!({"server":target.as_ref().map(|t|t.conn.name.clone()).unwrap_or_default(),"db":target.as_ref().map(|t|t.db).unwrap_or_default(),"path":"","include_ttl":self.csv_export_expire}),
                "general",
            ),
            DialogKind::About => (self.tr("menu.about"), Json::Null, "general"),
            DialogKind::Community => ("Tiny RDM".into(), Json::Null, "general"),
        };
        if matches!(
            kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            data["proxy"]["auth"] = Json::Bool(
                data["proxy"]["username"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()),
            );
        }
        let mut fields = BTreeMap::new();
        flatten_fields(&data, "", &mut fields, window, cx);
        if matches!(kind, DialogKind::Preferences) {
            for key in [
                "general.font_family",
                "editor.font_family",
                "cli.font_family",
            ] {
                let query_key = format!("font_query.{key}");
                let selected = data
                    .pointer(&format!("/{}", key.replace('.', "/")))
                    .and_then(Json::as_array)
                    .is_some_and(|fonts| !fonts.is_empty());
                let query = input(
                    "",
                    if selected {
                        String::new()
                    } else {
                        self.tr("preferences.general.font_tip")
                    },
                    false,
                    window,
                    cx,
                );
                let field_key = key.to_string();
                let event_query = query_key.clone();
                self.subs.push(
                    cx.subscribe_in(&query, window, move |app, state, event, w, cx| {
                        let current = app.dialog.as_ref().is_some_and(|dialog| {
                            dialog
                                .fields
                                .get(&event_query)
                                .is_some_and(|field| field.entity_id() == state.entity_id())
                        });
                        if !current {
                            return;
                        }
                        match event {
                            InputEvent::Focus => {
                                app.dialog.as_mut().unwrap().font_menu = Some(field_key.clone());
                                app.dialog.as_mut().unwrap().font_cursor = 0;
                                app.dialog
                                    .as_mut()
                                    .unwrap()
                                    .font_scroll
                                    .set_offset(point(px(0.), px(0.)));
                                cx.notify();
                            }
                            InputEvent::Change => {
                                if !app
                                    .dialog
                                    .as_ref()
                                    .unwrap()
                                    .value(&event_query, cx)
                                    .is_empty()
                                {
                                    app.dialog.as_mut().unwrap().font_menu =
                                        Some(field_key.clone());
                                }
                                app.dialog.as_mut().unwrap().font_cursor = 0;
                                app.dialog
                                    .as_mut()
                                    .unwrap()
                                    .font_scroll
                                    .set_offset(point(px(0.), px(0.)));
                                cx.notify();
                            }
                            InputEvent::PressEnter { .. } => app.add_dialog_font(&field_key, w, cx),
                            InputEvent::Blur => {
                                let dialog = app.dialog.as_mut().unwrap();
                                if dialog.font_menu.as_deref() == Some(&field_key) {
                                    dialog.font_menu = None;
                                    cx.notify();
                                }
                            }
                        }
                    }),
                );
                fields.insert(query_key, query);
            }
        }
        if matches!(
            kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            for (key, tip) in [
                ("name", "dialogue.connection.name_tip"),
                ("password", "dialogue.connection.pwd_tip"),
                ("username", "dialogue.connection.usr_tip"),
                ("addr", "dialogue.connection.addr_tip"),
                ("sock", "dialogue.connection.sock_tip"),
                ("default_filter", "dialogue.connection.advn.filter_tip"),
                ("key_separator", "dialogue.connection.advn.separator_tip"),
                ("ssl.certfile", "dialogue.connection.ssl.cert_file_tip"),
                ("ssl.keyfile", "dialogue.connection.ssl.key_file_tip"),
                ("ssl.cafile", "dialogue.connection.ssl.ca_file_tip"),
                ("ssl.sni", "dialogue.connection.ssl.sni"),
                ("ssh.addr", "dialogue.connection.ssh.addr_tip"),
                ("ssh.username", "dialogue.connection.ssh.usr_tip"),
                ("ssh.password", "dialogue.connection.ssh.pwd_tip"),
                ("ssh.pk_file", "dialogue.connection.ssh.pkfile_tip"),
                ("ssh.passphrase", "dialogue.connection.ssh.passphrase_tip"),
                ("sentinel.username", "dialogue.connection.sentinel.usr_tip"),
                ("sentinel.password", "dialogue.connection.sentinel.pwd_tip"),
                ("proxy.addr", "dialogue.connection.proxy.host"),
                ("proxy.username", "dialogue.connection.proxy.usr_tip"),
                ("proxy.password", "dialogue.connection.proxy.pwd_tip"),
            ] {
                if let Some(field) = fields.get(key) {
                    field.update(cx, |state, cx| {
                        state.set_placeholder(self.tr(tip), window, cx)
                    });
                }
            }
            for key in [
                "port",
                "conn_timeout",
                "exec_timeout",
                "load_size",
                "ssh.port",
                "proxy.port",
            ] {
                if let Some(field) = fields.get(key) {
                    field.update(cx, |state, cx| {
                        state.set_placeholder(
                            if key == "port" {
                                "6379"
                            } else {
                                self.control_placeholder(false)
                            },
                            window,
                            cx,
                        )
                    });
                }
            }
        }
        let mut editors = BTreeMap::new();
        let mut arguments: BTreeMap<String, Vec<Entity<InputState>>> = BTreeMap::new();
        if matches!(
            kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            fields.remove("db_filter_list");
            let filter_input = input(
                "",
                self.tr("dialogue.connection.advn.dbfilter_input_tip"),
                false,
                window,
                cx,
            );
            self.subs.push(
                cx.subscribe_in(&filter_input, window, |app, _, event, w, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        app.add_database_tag(w, cx);
                    }
                }),
            );
            fields.insert("db_filter_input".into(), filter_input);
            arguments.insert(
                "db_filter_list".into(),
                data["db_filter_list"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| input(value.to_string(), "DB", false, window, cx))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        if matches!(kind, DialogKind::Decoder(_)) {
            for key in ["decode_args", "encode_args"] {
                fields.remove(key);
                arguments.insert(
                    key.into(),
                    data[key]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .map(|item| {
                                    input(
                                        item.as_str().unwrap_or_default().to_string(),
                                        "",
                                        false,
                                        window,
                                        cx,
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                );
            }
        }
        for key in ["value", "message", "alias", "decode_args", "encode_args"] {
            if matches!(kind, DialogKind::NewKey | DialogKind::AddEntries) { continue; }
            if let Some(state) = fields.remove(key) {
                let text = text_input(&state, cx);
                let language = if key == "value" && !matches!(kind, DialogKind::EditEntry(_)) {
                    editor_language(data["format"].as_str().unwrap_or("Raw"))
                } else {
                    "plaintext"
                };
                let state = editor(text, language, &self.pref, window, cx);
                let entry = (key == "value" && matches!(kind, DialogKind::EditEntry(_))).then_some(self.next_dialog_id + 1);
                self.subs
                    .push(cx.subscribe_in(&state, window, move |app, editor, event, window, cx| {
                        if matches!(event, InputEvent::Change) {
                            if let Some(id) = entry
                                && let Some(dialog) = app.entry_dialog_mut(id)
                            {
                                let content=dialog.entry_content.as_mut().unwrap();
                                content.editing = text_input(editor, cx);
                                dialog.refresh_entry_json(window,cx);
                            }
                            cx.notify();
                        }
                    }));
                editors.insert(key.to_string(), state);
            }
        }
        let row = Row {
            id: input("", "Field", false, window, cx),
            value: row_value("".into(), true, window, cx),
            score: input("0", "Score", false, window, cx),
        };
        let rows = if matches!(
            kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            data["alias"]
                .as_object()
                .map(|aliases| {
                    aliases
                        .iter()
                        .map(|(db, value)| Row {
                            id: input(db.clone(), "DB", false, window, cx),
                            value: row_value(
                                value.as_str().unwrap_or_default().to_string(),
                                false,
                                window,
                                cx,
                            ),
                            score: input("0", "", false, window, cx),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![row]
        };
        if matches!(kind, DialogKind::Import | DialogKind::Export) {
            fields.get("path").unwrap().update(cx, |field, cx| field.set_placeholder(self.tr(if matches!(kind, DialogKind::Import) {"dialogue.import.open_csv_file_tip"} else {"dialogue.export.save_file_tip"}), window, cx));
        }
        let focus = if matches!(kind, DialogKind::Import | DialogKind::Export) {fields.get("server").cloned()}
            else {fields.get("name").or_else(|| fields.get("key")).cloned()};
        if matches!(
            kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            for row in &rows {
                fields.insert(numbers::alias_key(&row.id), row.id.clone());
            }
        }
        for (key, state) in &fields {
            self.watch_dialog_input(key.clone(), state, window, cx);
        }
        for state in arguments.values().flatten() {
            self.subs
                .push(cx.subscribe_in(state, window, |_, _, _: &InputEvent, _, cx| cx.notify()));
        }
        let previewed = target.as_ref().is_some_and(|t| !t.keys.is_empty());
        let original_preferences =
            matches!(kind, DialogKind::Preferences).then(|| self.pref.clone());
        self.next_dialog_id += 1;
        let numbers = fields
            .keys()
            .filter(|key| numbers::bounds(&kind, key).is_some())
            .map(|key| {
                (
                    key.clone(),
                    if key.starts_with("alias_db.") {
                        numbers::parse(&text_input(fields.get(key).unwrap(), cx))
                            .ok()
                            .flatten()
                    } else {
                        data.pointer(&format!("/{}", key.replace('.', "/")))
                            .and_then(Json::as_f64)
                    },
                )
            })
            .collect();
        let csv_filename=matches!(kind,DialogKind::Export).then(||file_dialogs::CsvFilename::new(chrono::Local::now()));
        self.dialog = Some(Dialog {
            id: self.next_dialog_id,
            format_epoch: 0,
            format_loading: false,
            kind,
            data,
            fields,
            numbers,
            general_preview_size: self.pref.general.font_size,
            editors,
            arguments,
            rows,
            tab: tab.into(),
            title,
            target,
            progress: Arc::new(Mutex::new(String::new())),
            previewed,
            original_preferences,
            font_menu: None,
            font_cursor: 0,
            font_scroll: ScrollHandle::new(),
            connection_testing: false,
            connection_test: None,
            connection_errors: BTreeMap::new(),
            sentinel_masters: vec![],
            csv_filename,
            csv_unit_picker: csv_units::UnitPicker::new(cx),
            csv_key_scroll: ScrollHandle::new(),
            csv_scrollbar: csv_list::ScrollbarState::default(),
            new_key: None,
            entry_picker: Default::default(),
            entry_content: None,
            return_focus: None,
        });
        if self.dialog.as_ref().is_some_and(|d| matches!(d.kind, DialogKind::NewKey | DialogKind::AddEntries)) {
            self.prepare_new_key(window, cx);
        }
        let entry_binding = self.index().map(|i| (self.tabs[i].decode.clone(), self.tabs[i].format.clone())).unwrap_or(("None".into(),"Raw".into()));
        if let Some(dialog)=&mut self.dialog && let DialogKind::EditEntry(_)=&dialog.kind {
            let kind=&dialog.target.as_ref().unwrap().kind;
            let key=if *kind==KeyType::Hash{"field"}else if *kind==KeyType::ZSet{"score"}else{"row_index"};
            let state=&dialog.fields[key];
            let placeholder=text_input(state,cx);
            state.update(cx,|state,cx|{state.set_placeholder(placeholder,window,cx);});
            set_input(state,"",window,cx);
            dialog.entry_content=Some(entry_conversion::EntryContent::new(entry_binding.0.clone(),entry_binding.1.clone(),&dialog.editors["value"],cx));
        }
        if self.editing_entry() {
            self.focus.focus(window,cx);
        } else if let Some(f) = focus {
            f.update(cx, |i, cx| i.focus(window, cx));
        } else if let Some(editor) = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.editors.get("value"))
            .cloned()
        {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        }
        if self.editing_entry() {
            let id=self.dialog.as_ref().unwrap().id;
            self.convert_entry(id,entry_binding.0,entry_binding.1,window,cx);
        }
        cx.notify();
    }
    fn dialog_input(&self, key: &str, label: &str, multi: bool, cx: &Context<Self>) -> AnyElement {
        let Some(d) = &self.dialog else {
            return div().into_any_element();
        };
        let input = if let Some(state) = d.editors.get(key) {
            Editor::new(state)
                .h(px(160.))
                .aria_label(localized(label))
                .into_any_element()
        } else if let Some(state) = d.fields.get(key) {
            (if numbers::bounds(&d.kind, key).is_some() {
                self.number_input(key, cx)
            } else {
                form_input(state)
            })
            .h(px(if multi { 160. } else { 32. }))
            .disabled(self.connection_field_disabled(key, cx))
            .when(self.connection_field_disabled(key, cx), |field| {
                field.bg(disabled_control_background(cx))
            })
            .readonly(key == "key" && matches!(d.kind, DialogKind::Ttl | DialogKind::Rename))
            .aria_label(localized(label))
            .when(
                key.ends_with("password") || key.ends_with("passphrase"),
                |i| i.mask_toggle(),
            )
            .when(d.connection_errors.contains_key(key), |field| {
                field
                    .border_color(self.form_error_color(cx))
                    .focus_bordered(false)
            })
            .into_any_element()
        } else {
            return div()
                .child(format!("Unknown field: {key}"))
                .into_any_element();
        };
        self.form_field_for(key, label, input, cx)
    }
    fn form_field(&self, label: &str, control: AnyElement, cx: &Context<Self>) -> AnyElement {
        self.form_field_for("", label, control, cx)
    }
    fn control_placeholder(&self, select: bool) -> &'static str {
        match (
            matches!(self.current_language().as_str(), "zh" | "zh-cn"),
            select,
        ) {
            (true, true) => "请选择",
            (true, false) => "请输入",
            (false, true) => "Please Select",
            (false, false) => "Please Input",
        }
    }
    fn form_error_color(&self, cx: &App) -> Hsla {
        rgb(if cx.theme().mode == ThemeMode::Dark {
            0xe88080
        } else {
            0xd03050
        })
        .into()
    }
    fn form_field_for(
        &self,
        key: &str,
        label: &str,
        control: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let error = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.connection_errors.get(key))
            .copied();
        let help = if self
            .dialog
            .as_ref()
            .is_some_and(|dialog| matches!(dialog.kind, DialogKind::Preferences))
        {
            if label == self.tr("preferences.general.font") {
                Some("preferences.font_tip")
            } else if label == self.tr("preferences.general.scan_size") {
                Some("preferences.general.scan_size_tip")
            } else {
                None
            }
        } else {
            None
        };
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .mb(px(if error.is_some() { 0. } else { 18. }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(13.))
                    .line_height(px(16.25))
                    .font_weight(FontWeight(450.))
                    .text_color(rgb(0x717880))
                    .pl(px(2.))
                    .child(localized(label))
                    .when_some(help, |heading, tip| {
                        heading.child(
                            self.icon_btn(
                                format!("help-{tip}"),
                                "help",
                                self.tr(tip),
                                cx,
                                |_, _, _| {},
                            )
                            .with_size(gpui_component::Size::Size(px(18.6667)))
                            .size(px(14.))
                            .p_0()
                            .tab_stop(false)
                            .text_color(rgb(0x717880)),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(control)
                    .when_some(error, |field, error| {
                        field.child(
                            div()
                                .pt(px(4.))
                                .pl(px(2.))
                                .text_size(px(12.))
                                .line_height(px(15.))
                                .text_color(self.form_error_color(cx))
                                .child(self.tr(match error {
                                    ConnectionFieldError::Required => "dialogue.field_required",
                                    ConnectionFieldError::IllegalCharacters => {
                                        "dialogue.illegal_characters"
                                    }
                                })),
                        )
                    }),
            )
            .into_any_element()
    }
    fn dialog_radios(
        &self,
        key: &str,
        label: &str,
        options: &[(&str, String)],
        cx: &Context<Self>,
    ) -> AnyElement {
        let value = self.dialog.as_ref().unwrap().value(key, cx);
        let disabled = self.connection_field_disabled(key, cx);
        let mut choices = div().flex().items_center();
        for (index, (candidate, label)) in options.iter().enumerate() {
            let selected = value == *candidate;
            let key = key.to_string();
            let candidate = candidate.to_string();
            choices = choices.child(
                self.btn(
                    format!("radio-{key}-{candidate}"),
                    label.clone(),
                    "",
                    cx,
                    move |app, w, cx| app.set_dialog_field(&key, candidate.clone(), w, cx),
                )
                .when(selected, |button| button.primary().bg(rgb(0xd13b37)))
                .disabled(disabled)
                .when(!selected, |button| button.outline())
                .rounded(px(0.))
                .when(index == 0, |button| {
                    button.rounded_tl(px(4.)).rounded_bl(px(4.))
                })
                .when(index + 1 == options.len(), |button| {
                    button.rounded_tr(px(4.)).rounded_br(px(4.))
                }),
            );
        }
        self.form_field(label, choices.into_any_element(), cx)
    }
    fn dialog_number(&self, key: &str, label: &str, cx: &Context<Self>) -> AnyElement {
        let mut control = div()
            .relative()
            .w(px(214.))
            .h(px(32.))
            .child(self.number_input(key, cx).h(px(32.)).pr(px(50.)));
        let mut buttons = div()
            .absolute()
            .right(px(5.))
            .top_0()
            .h_full()
            .flex()
            .items_center();
        for (symbol, delta) in [("−", -1i64), ("+", 1)] {
            let key = key.to_string();
            buttons = buttons.child(
                self.btn(
                    format!("number-{key}-{delta}"),
                    symbol,
                    "",
                    cx,
                    move |app, w, cx| {
                        app.number_step(&key, delta as f64, w, cx);
                    },
                )
                .ghost()
                .w(px(20.))
                .px_0(),
            );
        }
        control = control.child(buttons);
        self.form_field(label, control.into_any_element(), cx)
    }
    fn dialog_ttl(&self, cx: &Context<Self>) -> AnyElement {
        let d = self.dialog.as_ref().unwrap();
        let unit = d.value("unit", cx).parse::<i64>().unwrap_or(1);
        let unit_name = match unit {
            60 => "common.minute",
            3600 => "common.hour",
            86400 => "common.day",
            _ => "common.second",
        };
        let weak = cx.weak_entity();
        let units = [
            (1, self.tr("common.second")),
            (60, self.tr("common.minute")),
            (3600, self.tr("common.hour")),
            (86400, self.tr("common.day")),
        ];
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mb_4()
            .child(self.tr("interface.ttl"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(form_input(d.fields.get("ttl").unwrap()).w_full())
                    .child(
                        self.btn("ttl-unit", self.tr(unit_name), "down", cx, |_, _, _| {})
                            .w(px(110.))
                            .flex_shrink_0()
                            .dropdown_menu(move |mut menu, _, _| {
                                for (unit, label) in &units {
                                    let unit = *unit;
                                    menu = menu.item(menu_action(
                                        label.clone(),
                                        weak.clone(),
                                        move |a, w, cx| {
                                            a.set_dialog_field("unit", unit.to_string(), w, cx)
                                        },
                                    ));
                                }
                                menu
                            }),
                    ),
            )
            .into_any_element()
    }
    fn toggle_dialog_font(
        &mut self,
        key: &str,
        font: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let mut selected =
            serde_json::from_str::<Vec<String>>(&dialog.value(key, cx)).unwrap_or_default();
        if let Some(index) = selected.iter().position(|value| value == font) {
            selected.remove(index);
        } else {
            selected.push(font.to_string());
        }
        self.set_dialog_field(key, serde_json::to_string(&selected).unwrap(), w, cx);
        let placeholder = if selected.is_empty() {
            self.tr("preferences.general.font_tip")
        } else {
            String::new()
        };
        if let Some(query) = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.fields.get(&format!("font_query.{key}")))
            .cloned()
        {
            query.update(cx, |input, cx| {
                input.set_value("", w, cx);
                input.set_placeholder(placeholder, w, cx);
            });
        }
        if let Some(dialog) = &mut self.dialog {
            dialog.font_cursor = 0;
            dialog.font_scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }
    fn add_dialog_font(&mut self, key: &str, w: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let cursor = dialog.font_cursor;
        if let Some(font) = self.font_candidates(key, cx).get(cursor).cloned() {
            self.toggle_dialog_font(key, &font, w, cx);
        }
    }
    fn font_candidates(&self, key: &str, cx: &App) -> Vec<String> {
        let query = self
            .dialog
            .as_ref()
            .unwrap()
            .value(&format!("font_query.{key}"), cx);
        let search = query.to_lowercase();
        let mut options = self
            .fonts
            .iter()
            .filter(|font| font.to_lowercase().contains(&search))
            .cloned()
            .collect::<Vec<_>>();
        let manual = query.trim().to_string();
        if !manual.is_empty() && !self.fonts.contains(&manual) {
            options.insert(0, manual);
        }
        options
    }
    pub(super) fn dialog_key(
        &mut self,
        event: &KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = &self.dialog else {
            return false;
        };
        let Some(key) = dialog.fields.iter().find_map(|(key, field)| {
            key.strip_prefix("font_query.")
                .filter(|_| field.focus_handle(cx).is_focused(w))
                .map(str::to_string)
        }) else {
            return false;
        };
        if event.keystroke.key == "backspace"
            && dialog.value(&format!("font_query.{key}"), cx).is_empty()
        {
            if let Some(font) = serde_json::from_str::<Vec<String>>(&dialog.value(&key, cx))
                .ok()
                .and_then(|fonts| fonts.last().cloned())
            {
                self.toggle_dialog_font(&key, &font, w, cx);
                cx.stop_propagation();
                return true;
            }
        }
        if dialog.font_menu.as_deref() != Some(&key) {
            return false;
        }
        if !matches!(event.keystroke.key.as_str(), "up" | "down") {
            return false;
        }
        let count = self.font_candidates(&key, cx).len();
        if count > 0 {
            let dialog = self.dialog.as_mut().unwrap();
            dialog.font_cursor = if event.keystroke.key == "up" {
                (dialog.font_cursor + count - 1) % count
            } else {
                (dialog.font_cursor + 1) % count
            };
            dialog.font_scroll.set_offset(point(
                px(0.),
                px(-(dialog.font_cursor.saturating_sub(6) as f32 * 32.)),
            ));
            cx.notify();
        }
        cx.stop_propagation();
        true
    }
    fn dialog_font(&self, key: &str, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let selected =
            serde_json::from_str::<Vec<String>>(&dialog.value(key, cx)).unwrap_or_default();
        let query_key = format!("font_query.{key}");
        let query = dialog.fields.get(&query_key).unwrap();
        let mut tags = div().flex().items_center().gap(px(4.));
        for (index, font) in selected.iter().enumerate() {
            let value = font.clone();
            let field = key.to_string();
            tags = tags.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .px(px(6.))
                    .h(px(24.))
                    .rounded(px(2.))
                    .bg(cx.theme().muted)
                    .child(if font.is_empty() {
                        String::new()
                    } else {
                        font.clone()
                    })
                    .child(
                        self.btn(
                            format!("remove-font-{key}-{index}"),
                            "",
                            "",
                            cx,
                            move |app, w, cx| app.toggle_dialog_font(&field, &value, w, cx),
                        )
                        .child(icon("close").size(px(12.)))
                        .ghost()
                        .size(px(14.))
                        .px_0(),
                    ),
            );
        }
        let field = key.to_string();
        let input = form_input(query)
            .when(!selected.is_empty(), |input| input.prefix(tags))
            .suffix(
                self.btn(
                    format!("font-caret-{key}"),
                    "",
                    "",
                    cx,
                    move |app, w, cx| {
                        if let Some(dialog) = &mut app.dialog {
                            if dialog.font_menu.as_deref() == Some(&field) {
                                dialog.font_menu = None;
                            } else {
                                dialog.font_menu = Some(field.clone());
                                dialog.fields[&format!("font_query.{field}")]
                                    .update(cx, |input, cx| input.focus(w, cx));
                            }
                            cx.notify();
                        }
                    },
                )
                .child(icon("down").size(px(12.)))
                .ghost()
                .size(px(18.))
                .px_0(),
            );
        let mut popup = gpui_base::Popup::new(format!("font-picker-{key}"), input)
            .w_full()
            .offset(px(4.));
        if dialog.font_menu.as_deref() == Some(key) {
            let options = self.font_candidates(key, cx);
            let menu_height = (options.len() as f32 * 32. + 8.).min(260.);
            let mut menu = div()
                .id(format!("font-options-{key}"))
                .flex()
                .flex_col()
                .w(px(469.53125))
                .h(px(menu_height))
                .overflow_y_scroll()
                .track_scroll(&dialog.font_scroll)
                .p(px(4.))
                .bg(cx.theme().background)
                .rounded(px(4.))
                .shadow_lg()
                .occlude();
            for (index, font) in options.into_iter().enumerate() {
                let field = key.to_string();
                let value = font.clone();
                let label = if font.is_empty() {
                    String::new()
                } else {
                    font.clone()
                };
                menu = menu.child(
                    self.btn(
                        format!("font-option-{key}-{font}"),
                        "",
                        "",
                        cx,
                        move |app, w, cx| app.toggle_dialog_font(&field, &value, w, cx),
                    )
                    .w_full()
                    .flex_shrink_0()
                    .text_left()
                    .ghost()
                    .tab_stop(false)
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.prevent_default())
                    .when(index == dialog.font_cursor, |row| row.bg(cx.theme().muted))
                    .justify_between()
                    .child(div().flex_1().child(label))
                    .when(selected.contains(&font), |row| {
                        row.child(
                            Icon::new(IconName::Check)
                                .size(px(16.))
                                .text_color(cx.theme().primary),
                        )
                    }),
                );
            }
            let trigger_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
            let observed = trigger_bounds.clone();
            popup = popup
                .on_position(move |_, bounds| observed.set(bounds))
                .content(menu.on_mouse_down_out(cx.listener(
                    move |app, event: &MouseDownEvent, _, cx| {
                        if !trigger_bounds.get().contains(&event.position) {
                            if let Some(dialog) = &mut app.dialog {
                                dialog.font_menu = None;
                            }
                            cx.notify();
                        }
                    },
                )));
        }
        self.form_field(
            &self.tr("preferences.general.font"),
            popup.into_any_element(),
            cx,
        )
    }
    fn dialog_checkbox(&self, key: &str, label: &str, cx: &Context<Self>) -> AnyElement {
        let checked = self.dialog.as_ref().is_some_and(|d| d.boolean(key));
        let csv = self.dialog.as_ref().is_some_and(|d| matches!(d.kind, DialogKind::Import|DialogKind::Export|DialogKind::AddEntries));
        let csv_dark = self.dark(cx) && csv;
        let disabled = self.connection_field_disabled(key, cx);
        let label = localized(label);
        let weak = cx.weak_entity();
        let key = key.to_string();
        gpui_base::Checkbox::new(SharedString::from(format!("check-{key}")))
            .disabled(disabled)
            .accessibility_label(label.clone())
            .checked(checked)
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(14.))
            .line_height(px(21.))
            .text_color(if disabled {
                disabled_control_color(cx)
            } else if csv_dark {
                rgba(0xffffffd1).into()
            } else {
                cx.theme().foreground
            })
            .when(!disabled, |control| control.cursor_pointer())
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .rounded(px(3.))
                    .border_1()
                    .border_color(if checked && !disabled {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .bg(if disabled {
                        disabled_control_background(cx)
                    } else if checked {
                        cx.theme().primary
                    } else if csv_dark {
                        Hsla::transparent_black()
                    } else {
                        cx.theme().background
                    })
                    .when(checked, |indicator| {
                        indicator.child((if csv {icon("key-checkbox-check")} else {Icon::new(IconName::Check)}).size(px(14.)).text_color(
                            if disabled {
                                disabled_control_color(cx)
                            } else if csv_dark {
                                rgb(0x18181c).into()
                            } else {
                                rgb(0xffffff).into()
                            },
                        ))
                    }),
            )
            .child(label)
            .on_change(move |state, _, _, cx| {
                let _ = weak.update(cx, |app, cx| {
                    if let Some(dialog) = &mut app.dialog {
                        set_json_boolean(
                            &mut dialog.data,
                            &key,
                            state == gpui_base::CheckboxState::Checked,
                        );
                        if matches!(dialog.kind, DialogKind::Export) && key == "include_ttl" {
                            app.csv_export_expire = dialog.boolean(&key);
                        }
                        if matches!(dialog.kind, DialogKind::AddEntries) && key == "reload" {
                            app.add_entries_reload = dialog.boolean(&key);
                        }
                        cx.notify();
                    }
                });
            })
            .into_any_element()
    }
    fn preferences_decoders(&self, cx: &Context<Self>) -> AnyElement {
        let help_url = if self.lang["name"] == "简体中文" {
            "https://tinyrdm.com/zh/guide/custom-decoder.html"
        } else {
            "https://tinyrdm.com/guide/custom-decoder.html"
        };
        let toolbar = div()
            .flex()
            .items_center()
            .mb(px(8.))
            .child(self.btn(
                "add-decoder",
                self.tr("preferences.decoder.new"),
                "add",
                cx,
                |app, w, cx| app.open_nested_dialog(DialogKind::Decoder(None), w, cx),
            ))
            .child(div().flex_1())
            .child(self.btn(
                "decoder-help",
                self.tr("preferences.decoder.help"),
                "help",
                cx,
                move |_, _, cx| cx.open_url(help_url),
            ));
        let heading = div()
            .flex()
            .h(px(46.))
            .items_center()
            .bg(cx.theme().muted)
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                div()
                    .w(px(120.))
                    .text_center()
                    .child(self.tr("preferences.decoder.decoder_name")),
            )
            .child(
                div()
                    .flex_1()
                    .text_center()
                    .child(self.tr("preferences.decoder.cmd_preview")),
            )
            .child(
                div()
                    .w(px(80.))
                    .text_center()
                    .child(self.tr("preferences.decoder.status")),
            )
            .child(
                div()
                    .w(px(80.))
                    .text_center()
                    .child(self.tr("interface.action")),
            );
        let mut rows = div()
            .id("decoder-table-scroll")
            .flex()
            .flex_col()
            .max_h(px(304.))
            .overflow_y_scroll();
        if self.pref.decoder.is_empty() {
            rows = rows.child(
                div()
                    .h(px(168.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_color(rgb(if self.dark(cx) { 0x606060 } else { 0xc2c2c2 }))
                    .child(icon("empty").size(px(40.)))
                    .child(if self.lang["name"] == "简体中文" {
                        "无数据"
                    } else {
                        "No data"
                    }),
            );
        }
        let command_text = |path: &str, args: &[String]| {
            std::iter::once(path.to_string())
                .chain(args.iter().map(|arg| {
                    if arg.chars().any(char::is_whitespace) {
                        format!("\"{}\"", arg.replace('"', "\\\""))
                    } else {
                        arg.clone()
                    }
                }))
                .collect::<Vec<_>>()
                .join(" ")
        };
        for (index, decoder) in self.pref.decoder.iter().enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .min_h(px(74.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .w(px(120.))
                            .px(px(8.))
                            .text_center()
                            .truncate()
                            .child(decoder.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(15.))
                            .px(px(8.))
                            .child(
                                div().truncate().child(command_text(
                                    &decoder.decode_path,
                                    &decoder.decode_args,
                                )),
                            )
                            .child(
                                div().truncate().child(command_text(
                                    &decoder.encode_path,
                                    &decoder.encode_args,
                                )),
                            ),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(decoder.auto, |cell| {
                                cell.child(icon("checked").size(px(16.)))
                            })
                            .when(!decoder.auto, |cell| cell.child("-")),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(self.icon_btn(
                                format!("decoder-delete-{index}"),
                                "delete",
                                self.tr("interface.delete_row"),
                                cx,
                                move |app, _, cx| {
                                    app.pref.decoder.remove(index);
                                    cx.notify();
                                },
                            ))
                            .child(self.icon_btn(
                                format!("decoder-edit-{index}"),
                                "edit",
                                self.tr("interface.edit_row"),
                                cx,
                                move |app, w, cx| {
                                    app.open_nested_dialog(DialogKind::Decoder(Some(index)), w, cx)
                                },
                            )),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(
                div()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(4.))
                    .overflow_hidden()
                    .child(heading)
                    .child(rows),
            )
            .into_any_element()
    }
    fn change_entry_format(
        &mut self,
        field: &str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = &self.dialog
            && matches!(dialog.kind, DialogKind::EditEntry(_))
        {
            let id = dialog.id;
            let decode = if field == "decode" { value.clone() } else { dialog.value("decode", cx) };
            let format = if field == "format" { value } else { String::new() };
            self.convert_entry(id, decode, format, window, cx);
            return;
        }
        let Some(d) = &mut self.dialog else {
            return;
        };
        let old_format = d.value("format", cx);
        let old_decode = d.value("decode", cx);
        let text = d.value("value", cx);
        let format = if field == "format" {
            value.clone()
        } else {
            String::new()
        };
        let decode = if field == "decode" {
            value
        } else {
            old_decode.clone()
        };
        let id = d.id;
        d.format_epoch += 1;
        let epoch = d.format_epoch;
        d.format_loading = true;
        let custom = self.pref.decoder.clone();
        self.cpu_job(
            window,
            cx,
            move || {
                Ok((|| -> Result<codec::Converted> {
                    let raw = codec::encode(&text,&old_decode,&old_format,&custom)?;
                    codec::convert(&raw, &decode, &format, &custom)
                })())
            },
            move |a, result, w, cx| {
                if let Some(d) = &mut a.dialog
                    && d.id == id
                    && d.format_epoch == epoch
                {
                    d.format_loading = false;
                    match result {
                        Ok(value) => {
                            if let Some(state) = d.fields.get("format") {
                                set_input(state, value.format.clone(), w, cx);
                            }
                            if let Some(state) = d.fields.get("decode") {
                                set_input(state, value.decode, w, cx);
                            }
                            if let Some(editor) = d.editors.get("value") {
                                editor.update(cx, |editor, cx| {
                                    editor.set_highlighter(editor_language(&value.format), cx);
                                    editor.set_value(value.text, w, cx);
                                });
                            }
                        }
                        Err(error) => a.toast(error.to_string(), true),
                    }
                }
            },
        );
    }
    fn dialog_choice(
        &self,
        key: &str,
        label: &str,
        options: Vec<(String, String)>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let d = self.dialog.as_ref().unwrap();
        let value = d.value(key, cx);
        let value = if key == "general.language" {
            match value.as_str() {
                "en-us" => "en",
                "zh-cn" => "zh",
                "zh-tw" => "tw",
                "ja-jp" => "ja",
                "ko-kr" => "ko",
                "fr-fr" => "fr",
                "es-es" => "es",
                "pt-br" => "pt",
                "ru-ru" => "ru",
                "tr-tr" => "tr",
                _ => &value,
            }
            .to_string()
        } else {
            value
        };
        let shown = options
            .iter()
            .find(|(v, _)| *v == value)
            .map(|(_, l)| l.clone())
            .unwrap_or(value);
        let weak = cx.weak_entity();
        let key = key.to_string();
        let button = self
            .btn(format!("choice-{key}"), "", "", cx, |_, _, _| {})
            .child(div().flex_1().text_left().child(shown))
            .child(icon("down").size(px(12.)))
            .w_full()
            .disabled(self.connection_field_disabled(&key, cx))
            .dropdown_menu(move |mut menu, _, _| {
                for (value, label) in &options {
                    let value = value.clone();
                    let field = key.clone();
                    menu = menu.item(menu_action(label.clone(), weak.clone(), move |a, w, cx| {
                        if a.editing_entry() && matches!(field.as_str(), "format" | "decode") {
                            a.change_entry_format(&field, value.clone(), w, cx);
                            return;
                        }
                        if let Some(d) = &mut a.dialog
                            && let Some(state) = d.fields.get(&field)
                        {
                            set_input(state, value.clone(), w, cx);
                            if field == "db_filter_type" && value != "none" {
                                let indices =
                                    d.arguments.entry("db_filter_list".into()).or_default();
                                if indices.is_empty() {
                                    indices.push(input("0", "DB", false, w, cx));
                                }
                            }
                            cx.notify();
                        }
                    }));
                }
                menu
            });
        self.form_field(label, button.into_any_element(), cx)
    }
    fn dialog_file(&self, key: &str, label: &str, save: bool, cx: &Context<Self>) -> AnyElement {
        self.form_field(label, self.file_control(key, save, cx), cx)
    }
    fn connection_field_disabled(&self, key: &str, cx: &App) -> bool {
        let Some(dialog) = &self.dialog else {
            return false;
        };
        if !matches!(
            dialog.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            return false;
        }
        for section in ["ssl", "ssh", "sentinel"] {
            if key.starts_with(&format!("{section}.")) && key != format!("{section}.enable") {
                return !dialog.boolean(&format!("{section}.enable"));
            }
        }
        if key.starts_with("proxy.") && key != "proxy.type" {
            return dialog.value("proxy.type", cx) != "2"
                || (matches!(key, "proxy.username" | "proxy.password")
                    && !dialog.boolean("proxy.auth"));
        }
        false
    }
    fn file_control(&self, key: &str, save: bool, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let field = key.to_string();
        let disabled = self.connection_field_disabled(key, cx);
        div()
            .flex()
            .items_center()
            .child(
                form_input(dialog.fields.get(key).unwrap())
                    .when(matches!(dialog.kind, DialogKind::Import | DialogKind::Export), |field| field.id("csv-path-input"))
                    .h(px(32.))
                    .rounded_r(px(0.))
                    .cleanable(true)
                    .disabled(disabled)
                    .when(disabled, |field| field.bg(disabled_control_background(cx))),
            )
            .child(
                self.btn(format!("file-{key}"), "...", "", cx, move |app, w, cx| {
                    app.choose_dialog_file(&field, save, w, cx)
                })
                .w(px(36.))
                .rounded_l(px(0.))
                .ml(px(-1.))
                .disabled(disabled),
            )
            .into_any_element()
    }
    fn connection_enable(&self, key: &str, label: &str, cx: &Context<Self>) -> AnyElement {
        div()
            .h(px(32.))
            .flex()
            .items_center()
            .mb(px(18.))
            .child(self.dialog_checkbox(key, label, cx))
            .into_any_element()
    }
    pub(super) fn add_database_tag(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let text = dialog.value("db_filter_input", cx);
        let Ok(index) = text.trim().parse::<u32>() else {
            self.toast("Invalid database index", true);
            return;
        };
        let fields = dialog.arguments.entry("db_filter_list".into()).or_default();
        if !fields
            .iter()
            .any(|field| text_input(field, cx) == index.to_string())
        {
            fields.push(input(index.to_string(), "", false, w, cx));
        }
        if let Some(field) = dialog.fields.get("db_filter_input") {
            set_input(field, "", w, cx);
        }
        cx.notify();
    }
    fn connection_database_tags(&self, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let mut tags = div().flex().items_center().gap(px(4.));
        for (index, field) in dialog
            .arguments
            .get("db_filter_list")
            .into_iter()
            .flatten()
            .enumerate()
        {
            tags = tags.child(
                div()
                    .flex()
                    .items_center()
                    .bg(cx.theme().muted)
                    .rounded(px(3.))
                    .px(px(5.))
                    .child(text_input(field, cx))
                    .child(
                        self.icon_btn(
                            format!("remove-db-{index}"),
                            "close",
                            self.tr("interface.delete_row"),
                            cx,
                            move |app, _, cx| {
                                if let Some(dialog) = &mut app.dialog {
                                    if let Some(fields) = dialog.arguments.get_mut("db_filter_list")
                                    {
                                        fields.remove(index);
                                    }
                                }
                                cx.notify();
                            },
                        )
                        .size(px(20.)),
                    ),
            );
        }
        self.form_field(
            &self.tr("dialogue.connection.advn.dbfilter_input"),
            form_input(dialog.fields.get("db_filter_input").unwrap())
                .h(px(32.))
                .prefix(tags)
                .into_any_element(),
            cx,
        )
    }
    pub(super) fn add_alias_row(
        &mut self,
        after: Option<usize>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = Row {
            id: input("0", self.tr("dialogue.connection.alias.db"), false, w, cx),
            value: RowValue::Single(input(
                "",
                self.tr("dialogue.connection.alias.value"),
                false,
                w,
                cx,
            )),
            score: input("0", "", false, w, cx),
        };
        let key = numbers::alias_key(&row.id);
        self.watch_dialog_input(key.clone(), &row.id, w, cx);
        if let Some(dialog) = &mut self.dialog {
            let index = after
                .map(|index| index + 1)
                .unwrap_or(dialog.rows.len())
                .min(dialog.rows.len());
            dialog.fields.insert(key.clone(), row.id.clone());
            dialog.numbers.insert(key, Some(0.));
            dialog.rows.insert(index, row);
            cx.notify();
        }
    }
    fn form_pair(&self, left: AnyElement, right: AnyElement) -> AnyElement {
        div()
            .flex()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().child(left))
            .child(div().flex_1().min_w_0().child(right))
            .into_any_element()
    }
    fn connection_timeout(&self, key: &str, label: &str, cx: &Context<Self>) -> AnyElement {
        self.form_field(
            label,
            self.number_input(key, cx)
                .h(px(32.))
                .suffix(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.tr("common.second")),
                )
                .into_any_element(),
            cx,
        )
    }
    fn connection_pane(&self, cx: &Context<Self>) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let mut pane = div().flex().flex_col();
        match dialog.tab.as_str() {
            "general" => return self.connection_general(cx),
            "advanced" => {
                pane = pane
                    .child(self.form_pair(
                        self.dialog_input(
                            "default_filter",
                            &self.tr("dialogue.connection.advn.filter"),
                            false,
                            cx,
                        ),
                        self.dialog_input(
                            "key_separator",
                            &self.tr("dialogue.connection.advn.separator"),
                            false,
                            cx,
                        ),
                    ))
                    .child(self.form_pair(
                        self.connection_timeout(
                            "conn_timeout",
                            &self.tr("dialogue.connection.advn.conn_timeout"),
                            cx,
                        ),
                        self.connection_timeout(
                            "exec_timeout",
                            &self.tr("dialogue.connection.advn.exec_timeout"),
                            cx,
                        ),
                    ))
                    .child(self.form_pair(
                        self.dialog_radios(
                            "key_view",
                            &self.tr("dialogue.connection.advn.key_view"),
                            &[
                                ("0", self.tr("dialogue.connection.advn.key_view_tree")),
                                ("1", self.tr("dialogue.connection.advn.key_view_list")),
                            ],
                            cx,
                        ),
                        self.dialog_input(
                            "load_size",
                            &self.tr("dialogue.connection.advn.load_size"),
                            false,
                            cx,
                        ),
                    ))
                    .child(self.dialog_radios(
                        "db_filter_type",
                        &self.tr("dialogue.connection.advn.dbfilter_type"),
                        &[
                            ("none", self.tr("dialogue.connection.advn.dbfilter_all")),
                            ("show", self.tr("dialogue.connection.advn.dbfilter_show")),
                            ("hide", self.tr("dialogue.connection.advn.dbfilter_hide")),
                        ],
                        cx,
                    ));
                if dialog.value("db_filter_type", cx) != "none" {
                    pane = pane.child(self.connection_database_tags(cx));
                }
                let selected = dialog.value("mark_color", cx);
                let mut colors = div().flex().items_center().h(px(24.));
                for color in [
                    "", "#F75B52", "#F7A234", "#F7CE33", "#4ECF60", "#348CF7", "#B270D3",
                ] {
                    let value = color.to_string();
                    colors = colors.child(
                        div()
                            .id(format!("color-{color}"))
                            .size(px(24.))
                            .mr(px(2.))
                            .border(px(3.))
                            .rounded_full()
                            .border_color(if selected == color {
                                cx.theme().foreground
                            } else {
                                cx.theme().border
                            })
                            .when(!color.is_empty(), |swatch| {
                                swatch.bg(rgb(u32::from_str_radix(&color[1..], 16).unwrap()))
                            })
                            .when(color.is_empty(), |swatch| {
                                swatch.child(icon("close").size(px(24.)))
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |app, _, w, cx| {
                                app.set_dialog_field("mark_color", value.clone(), w, cx)
                            })),
                    );
                }
                pane = pane.child(self.form_field(
                    &self.tr("dialogue.connection.advn.mark_color"),
                    colors.into_any_element(),
                    cx,
                ));
            }
            "alias" => {
                for (index, row) in dialog.rows.iter().enumerate() {
                    pane = pane.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .mb(px(if index + 1 == dialog.rows.len() {
                                18.
                            } else {
                                10.
                            }))
                            .child(
                                self.number_input(&numbers::alias_key(&row.id), cx)
                                    .w(px(100.765625))
                                    .flex_shrink_0(),
                            )
                            .child(":")
                            .child(div().flex_1().min_w_0().child(row.value.render()))
                            .child(
                                self.icon_btn(
                                    format!("remove-alias-{index}"),
                                    "delete",
                                    self.tr("interface.delete_row"),
                                    cx,
                                    move |app, _, cx| {
                                        if let Some(dialog) = &mut app.dialog {
                                            let row = dialog.rows.remove(index);
                                            let key = numbers::alias_key(&row.id);
                                            dialog.fields.remove(&key);
                                            dialog.numbers.remove(&key);
                                            cx.notify();
                                        }
                                    },
                                )
                                .with_size(gpui_component::Size::Size(px(24.)))
                                .size(px(18.))
                                .p_0(),
                            )
                            .child(
                                self.icon_btn(
                                    format!("insert-alias-{index}"),
                                    "add",
                                    if self.lang["name"] == "简体中文" {
                                        "添加"
                                    } else {
                                        "Create"
                                    },
                                    cx,
                                    move |app, w, cx| app.add_alias_row(Some(index), w, cx),
                                )
                                .with_size(gpui_component::Size::Size(px(24.)))
                                .size(px(18.))
                                .p_0(),
                            ),
                    );
                }
                if dialog.rows.is_empty() {
                    pane = pane.child(
                        self.btn(
                            "add-alias",
                            if self.lang["name"] == "简体中文" {
                                "添加"
                            } else {
                                "Create"
                            },
                            "add",
                            cx,
                            |app, w, cx| app.add_alias_row(None, w, cx),
                        )
                        .w_full()
                        .border_dashed(),
                    );
                }
            }
            "ssl" => {
                pane = pane
                    .child(self.connection_enable(
                        "ssl.enable",
                        &self.tr("dialogue.connection.ssl.enable"),
                        cx,
                    ))
                    .child(self.dialog_file(
                        "ssl.certfile",
                        &self.tr("dialogue.connection.ssl.cert_file"),
                        false,
                        cx,
                    ))
                    .child(self.dialog_file(
                        "ssl.keyfile",
                        &self.tr("dialogue.connection.ssl.key_file"),
                        false,
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(16.25))
                                    .font_weight(FontWeight(450.))
                                    .text_color(cx.theme().muted_foreground)
                                    .pl(px(2.))
                                    .child(self.tr("dialogue.connection.ssl.ca_file")),
                            )
                            .child(self.file_control("ssl.cafile", false, cx)),
                    )
                    .child(div().pt(px(18.)).child(self.connection_enable(
                        "ssl.allow_insecure",
                        &self.tr("dialogue.connection.ssl.allow_insecure"),
                        cx,
                    )))
                    .child(self.dialog_input(
                        "ssl.sni",
                        &self.tr("dialogue.connection.ssl.sni"),
                        false,
                        cx,
                    ));
            }
            "ssh" => {
                let disabled = !dialog.boolean("ssh.enable");
                pane = pane
                    .child(self.connection_enable(
                        "ssh.enable",
                        &self.tr("dialogue.connection.ssh.enable"),
                        cx,
                    ))
                    .child(
                        self.form_field(
                            &self.tr("dialogue.connection.addr"),
                            div()
                                .flex()
                                .items_center()
                                .child(
                                    form_input(dialog.fields.get("ssh.addr").unwrap())
                                        .h(px(32.))
                                        .disabled(disabled)
                                        .when(disabled, |field| {
                                            field.bg(disabled_control_background(cx))
                                        }),
                                )
                                .child(
                                    div()
                                        .w(px(25.65625))
                                        .flex_shrink_0()
                                        .text_center()
                                        .child(":"),
                                )
                                .child(
                                    self.number_input("ssh.port", cx)
                                        .h(px(32.))
                                        .w(px(128.3125))
                                        .flex_shrink_0()
                                        .disabled(disabled)
                                        .when(disabled, |field| {
                                            field.bg(disabled_control_background(cx))
                                        }),
                                )
                                .into_any_element(),
                            cx,
                        ),
                    )
                    .child(self.dialog_radios(
                        "ssh.login_type",
                        &self.tr("dialogue.connection.ssh.login_type"),
                        &[
                            ("pwd", self.tr("dialogue.connection.pwd")),
                            ("pkfile", self.tr("dialogue.connection.ssh.pkfile")),
                            ("agent", self.tr("dialogue.connection.ssh.agent")),
                        ],
                        cx,
                    ))
                    .child(self.dialog_input(
                        "ssh.username",
                        &self.tr("dialogue.connection.usr"),
                        false,
                        cx,
                    ));
                if dialog.value("ssh.login_type", cx) == "pkfile" {
                    pane = pane
                        .child(self.dialog_file(
                            "ssh.pk_file",
                            &self.tr("dialogue.connection.ssh.pkfile"),
                            false,
                            cx,
                        ))
                        .child(self.dialog_input(
                            "ssh.passphrase",
                            &self.tr("dialogue.connection.ssh.passphrase"),
                            false,
                            cx,
                        ));
                } else if dialog.value("ssh.login_type", cx) == "pwd" {
                    pane = pane.child(self.dialog_input(
                        "ssh.password",
                        &self.tr("dialogue.connection.pwd"),
                        false,
                        cx,
                    ));
                }
            }
            "sentinel" => {
                let weak = cx.weak_entity();
                let masters = dialog.sentinel_masters.clone();
                let dropdown = self
                    .icon_btn(
                        "sentinel-master-choices",
                        "down",
                        self.tr("dialogue.connection.sentinel.master"),
                        cx,
                        |_, _, _| {},
                    )
                    .disabled(!dialog.boolean("sentinel.enable"))
                    .dropdown_menu(move |mut menu, _, _| {
                        for master in &masters {
                            let value = master.clone();
                            menu = menu.item(menu_action(
                                master.clone(),
                                weak.clone(),
                                move |app, w, cx| {
                                    app.set_dialog_field("sentinel.master", value.clone(), w, cx)
                                },
                            ));
                        }
                        menu
                    });
                pane = pane
                    .child(self.connection_enable(
                        "sentinel.enable",
                        &self.tr("dialogue.connection.sentinel.enable"),
                        cx,
                    ))
                    .child(
                        self.form_field(
                            &self.tr("dialogue.connection.sentinel.master"),
                            div()
                                .flex()
                                .items_center()
                                .child(
                                    form_input(dialog.fields.get("sentinel.master").unwrap())
                                        .h(px(32.))
                                        .disabled(!dialog.boolean("sentinel.enable"))
                                        .suffix(dropdown),
                                )
                                .child(
                                    self.btn(
                                        "sentinel-discovery",
                                        self.tr("dialogue.connection.sentinel.auto_discover"),
                                        "",
                                        cx,
                                        |app, w, cx| app.test_connection(true, w, cx),
                                    )
                                    .disabled(!dialog.boolean("sentinel.enable"))
                                    .loading(dialog.connection_testing),
                                )
                                .into_any_element(),
                            cx,
                        ),
                    )
                    .child(self.dialog_input(
                        "sentinel.password",
                        &self.tr("dialogue.connection.sentinel.password"),
                        false,
                        cx,
                    ))
                    .child(self.dialog_input(
                        "sentinel.username",
                        &self.tr("dialogue.connection.sentinel.username"),
                        false,
                        cx,
                    ));
            }
            "cluster" => {
                pane = pane.child(self.connection_enable(
                    "cluster.enable",
                    &self.tr("dialogue.connection.cluster.enable"),
                    cx,
                ))
            }
            "proxy" => {
                for (value, label) in [
                    ("0", "dialogue.connection.proxy.type_none"),
                    ("1", "dialogue.connection.proxy.type_system"),
                    ("2", "dialogue.connection.proxy.type_custom"),
                ] {
                    pane = pane.child(
                        div().h(px(21.)).mb(px(12.)).child(
                            gpui_base::Radio::new(format!("proxy-mode-{value}"))
                                .accessibility_label(self.tr(label))
                                .checked(dialog.value("proxy.type", cx) == value)
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .h(px(21.))
                                .text_size(px(14.))
                                .line_height(px(21.))
                                .cursor_pointer()
                                .child(
                                    div()
                                        .size(px(16.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .border_1()
                                        .border_color(if dialog.value("proxy.type", cx) == value {
                                            cx.theme().primary
                                        } else {
                                            cx.theme().border
                                        })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(dialog.value("proxy.type", cx) == value, |circle| {
                                            circle.child(
                                                div()
                                                    .size(px(10.))
                                                    .rounded_full()
                                                    .bg(cx.theme().primary),
                                            )
                                        }),
                                )
                                .child(self.tr(label))
                                .on_change({
                                    let weak = cx.weak_entity();
                                    move |_, _, w, cx| {
                                        let _ = weak.update(cx, |app, cx| {
                                            app.set_dialog_field("proxy.type", value, w, cx)
                                        });
                                    }
                                }),
                        ),
                    );
                }
                let weak = cx.weak_entity();
                let options = ["http", "https", "socks5", "socks5h"];
                let schema = self
                    .btn("proxy-schema", "", "", cx, |_, _, _| {})
                    .child(
                        div()
                            .flex_1()
                            .text_left()
                            .child(dialog.value("proxy.schema", cx).to_uppercase()),
                    )
                    .child(icon("down").size(px(12.)))
                    .w(px(100.))
                    .disabled(dialog.value("proxy.type", cx) != "2")
                    .dropdown_menu(move |mut menu, _, _| {
                        for value in options {
                            menu = menu.item(menu_action(
                                value.to_uppercase(),
                                weak.clone(),
                                move |app, w, cx| {
                                    app.set_dialog_field("proxy.schema", value, w, cx)
                                },
                            ));
                        }
                        menu
                    });
                pane = pane
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .mb(px(18.))
                            .child(schema)
                            .child(
                                form_input(dialog.fields.get("proxy.addr").unwrap())
                                    .h(px(32.))
                                    .rounded(px(0.))
                                    .disabled(self.connection_field_disabled("proxy.addr", cx))
                                    .when(
                                        self.connection_field_disabled("proxy.addr", cx),
                                        |field| field.bg(disabled_control_background(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(17.859375))
                                    .flex_shrink_0()
                                    .text_center()
                                    .child(":"),
                            )
                            .child(
                                self.number_input("proxy.port", cx)
                                    .h(px(32.))
                                    .w(px(99.328125))
                                    .flex_shrink_0()
                                    .disabled(self.connection_field_disabled("proxy.port", cx))
                                    .when(
                                        self.connection_field_disabled("proxy.port", cx),
                                        |field| field.bg(disabled_control_background(cx)),
                                    ),
                            ),
                    )
                    .child(self.connection_enable(
                        "proxy.auth",
                        &self.tr("dialogue.connection.proxy.auth"),
                        cx,
                    ))
                    .child(self.form_pair(
                        self.dialog_input(
                            "proxy.username",
                            &self.tr("dialogue.connection.usr"),
                            false,
                            cx,
                        ),
                        self.dialog_input(
                            "proxy.password",
                            &self.tr("dialogue.connection.pwd"),
                            false,
                            cx,
                        ),
                    ));
            }
            _ => {}
        }
        pane.into_any_element()
    }
    pub(super) fn render_dialog(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let d = self.dialog.as_ref().unwrap();
        if matches!(d.kind, DialogKind::NewKey) { return self.render_new_key(window, cx); }
        if matches!(d.kind, DialogKind::AddEntries) { return self.render_add_entries(window, cx); }
        if matches!(d.kind, DialogKind::Import | DialogKind::Export) {return self.render_csv_dialog(window,cx);}
        if let DialogKind::DeleteKey(key) = &d.kind { return self.render_key_delete_warning(key, cx); }
        if matches!(d.kind, DialogKind::Community) {
            let images = div()
                .id("community-images")
                .test_support()
                .w(px(400.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(
                    [
                        ("wechat_official.png", 137.5),
                        ("bilibili_official.png", 400. * 730. / 960.),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (name, height))| {
                        div()
                            .id(("community-image", index))
                            .test_support()
                            .w(px(400.))
                            .h(px(height))
                            .flex_shrink_0()
                            .child(
                                img(ImageSource::Resource(Resource::Embedded(
                                    format!("images/{name}").into(),
                                )))
                                .w(px(400.))
                                .h(px(height)),
                            )
                    }),
                );
            return div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000066))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|app, _, window, cx| app.close_dialog(window, cx)),
                )
                .child(images)
                .into_any_element();
        }
        let special = matches!(d.kind, DialogKind::About);
        let mut body = div().flex().flex_col();
        let mut sections = vec![];
        match &d.kind {
            DialogKind::ConfirmEdit(_) => body = body.child(self.tr("dialogue.edit_close_confirm")),
            DialogKind::DeleteKey(_) => unreachable!("single key removal uses its warning dialog"),
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_) => {
                sections = vec![
                    ("general", "dialogue.connection.general"),
                    ("advanced", "dialogue.connection.advn.title"),
                    ("alias", "dialogue.connection.alias.title"),
                    ("ssl", "dialogue.connection.ssl.title"),
                    ("ssh", "dialogue.connection.ssh.title"),
                    ("sentinel", "dialogue.connection.sentinel.title"),
                    ("cluster", "dialogue.connection.cluster.title"),
                    ("proxy", "dialogue.connection.proxy.title"),
                ];
                body = body.child(self.connection_pane(cx));
            }
            DialogKind::Preferences => {
                sections = vec![
                    ("general", "preferences.general.name"),
                    ("editor", "preferences.editor.name"),
                    ("cli", "preferences.cli.name"),
                    ("decoder", "preferences.decoder.name"),
                ];
                match d.tab.as_str() {
                    "editor" => {
                        body = body.child(self.dialog_font("editor.font_family", cx));
                        for (key, _label) in [("editor.font_size", "字号")] {
                            body = body.child(self.dialog_number(
                                key,
                                &self.tr("preferences.general.font_size"),
                                cx,
                            ));
                        }
                        for (key, label) in [
                            ("editor.show_line_num", "显示行号"),
                            ("editor.show_folding", "代码折叠"),
                            ("editor.drop_text", "拖动文本"),
                            ("editor.links", "链接检测"),
                        ] {
                            body = body.child(
                                div()
                                    .h(px(32.))
                                    .flex()
                                    .items_center()
                                    .child(self.dialog_checkbox(key, label, cx)),
                            );
                        }
                    }
                    "cli" => {
                        body = body
                            .child(self.dialog_font("cli.font_family", cx))
                            .child(self.dialog_number(
                                "cli.font_size",
                                &self.tr("preferences.general.font_size"),
                                cx,
                            ))
                            .child(self.dialog_radios(
                                "cli.cursor_style",
                                &self.tr("preferences.cli.cursor_style"),
                                &[
                                    ("block", self.tr("preferences.cli.cursor_style_block")),
                                    (
                                        "underline",
                                        self.tr("preferences.cli.cursor_style_underline"),
                                    ),
                                    ("bar", self.tr("preferences.cli.cursor_style_bar")),
                                ],
                                cx,
                            ));
                    }
                    "decoder" => body = body.child(self.preferences_decoders(cx)),
                    _ => {
                        body = body
                            .child(self.dialog_radios("general.theme", &self.tr("preferences.general.theme"), &[
                                ("light", self.tr("preferences.general.theme_light")),
                                ("dark", self.tr("preferences.general.theme_dark")),
                                ("auto", self.tr("preferences.general.theme_auto")),
                            ], cx))
                            .child(self.dialog_choice("general.language", &self.tr("preferences.general.language"),
                                vec![("auto", self.tr("preferences.general.system_lang")), ("en", "English".into()), ("zh", "简体中文".into()), ("tw", "繁體中文".into()), ("ja", "日本語".into()), ("ko", "한국어".into()), ("fr", "Français".into()), ("es", "Español".into()), ("pt", "Português (BR)".into()), ("ru", "Русский".into()), ("tr", "Türkçe".into())].into_iter().map(|(code, label)| (code.to_string(), label)).collect(), cx))
                            .child(self.dialog_font("general.font_family", cx))
                            .child(self.dialog_number("general.font_size", &self.tr("preferences.general.font_size"), cx))
                            .child(div().flex().gap(px(10.))
                                .child(div().flex_1().min_w_0().child(self.dialog_input("general.scan_size", &self.tr("preferences.general.scan_size"), false, cx)))
                                .child(div().flex_1().min_w_0().child(self.dialog_choice("general.key_icon_style", &self.tr("preferences.general.key_icon_style"), (0..4).map(|style| (style.to_string(), self.tr(&format!("preferences.general.key_icon_style{style}")))).collect(), cx))))
                            .child(self.form_field(&self.tr("preferences.general.update"), div().h(px(32.)).flex().items_center().child(self.dialog_checkbox("general.check_update", &self.tr("preferences.general.auto_check_update"), cx)).into_any_element(), cx))
                            .child(self.form_field(&self.tr("preferences.general.privacy"), div().h(px(32.)).flex().items_center()
                                .child(self.dialog_checkbox("general.allow_track", &self.tr("preferences.general.allow_track"), cx))
                                .child(self.btn("privacy-policy", self.tr("preferences.general.privacy"), "", cx, |app, _, cx| cx.open_url(if app.lang["name"] == "简体中文" { "https://tinyrdm.com/zh/guide/privacy.html" } else { "https://tinyrdm.com/guide/privacy.html" })).text().underline().text_color(cx.theme().primary).px_0()).into_any_element(), cx));
                    }
                }
            }
            DialogKind::Decoder(_) => {
                body = body.child(self.dialog_input(
                    "name",
                    &self.tr("dialogue.decoder.decoder_name"),
                    false,
                    cx,
                ));
                let mut tabs = div()
                    .flex()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .mb_3();
                for (key, label) in [
                    ("decode", "dialogue.decoder.decoder"),
                    ("encode", "dialogue.decoder.encoder"),
                ] {
                    tabs = tabs.child(
                        self.btn(
                            format!("decoder-{key}"),
                            self.tr(label),
                            "",
                            cx,
                            move |a, _, cx| {
                                if let Some(d) = &mut a.dialog {
                                    d.tab = key.into();
                                    cx.notify();
                                }
                            },
                        )
                        .ghost()
                        .selected(d.tab == key),
                    );
                }
                body = body.child(tabs);
                let encode = d.tab == "encode";
                let path = if encode { "encode_path" } else { "decode_path" };
                let key = if encode { "encode_args" } else { "decode_args" };
                body = body
                    .child(self.dialog_file(
                        path,
                        &self.tr(if encode {
                            "dialogue.decoder.encode_path"
                        } else {
                            "dialogue.decoder.decode_path"
                        }),
                        false,
                        cx,
                    ))
                    .child(div().mb_2().child(self.tr("dialogue.decoder.args")));
                if let Some(arguments) = d.arguments.get(key) {
                    for (index, field) in arguments.iter().enumerate() {
                        let key = key.to_string();
                        body = body.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .mb_2()
                                .child(form_input(field))
                                .child(self.icon_btn(
                                    format!("remove-arg-{index}"),
                                    "delete",
                                    self.tr("interface.delete_row"),
                                    cx,
                                    move |a, _, cx| {
                                        if let Some(d) = &mut a.dialog
                                            && let Some(fields) = d.arguments.get_mut(&key)
                                        {
                                            fields.remove(index);
                                            cx.notify();
                                        }
                                    },
                                )),
                        );
                    }
                }
                let field_key = key.to_string();
                body = body.child(self.btn(
                    "add-decoder-argument",
                    self.tr("interface.add_row"),
                    "add",
                    cx,
                    move |a, w, cx| {
                        if let Some(d) = &mut a.dialog {
                            d.arguments
                                .entry(field_key.clone())
                                .or_default()
                                .push(input("", "", false, w, cx));
                            cx.notify();
                        }
                    },
                ));
                let mut preview = vec![d.value(path, cx)];
                if let Some(arguments) = d.arguments.get(key) {
                    preview.extend(arguments.iter().map(|field| text_input(field, cx)));
                }
                if !preview.iter().any(|arg| arg.contains("{VALUE}")) {
                    preview.push("{VALUE}".into());
                }
                body = body.child(
                    div()
                        .mt_3()
                        .text_size(px(12.))
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(preview.join(" ")),
                );
            }
            DialogKind::NewKey | DialogKind::AddEntries => {
                let kind = if matches!(d.kind, DialogKind::NewKey) {
                    KeyType::parse(&d.value("type", cx))
                } else {
                    d.target
                        .as_ref()
                        .map(|t| t.kind.clone())
                        .unwrap_or_default()
                };
                if matches!(d.kind, DialogKind::NewKey) {
                    body = body
                        .child(self.dialog_input("key", "键名", false, cx))
                        .child(
                            self.dialog_choice(
                                "type",
                                "类型",
                                ["STRING", "HASH", "LIST", "SET", "ZSET", "STREAM", "JSON"]
                                    .iter()
                                    .map(|s| (s.to_string(), s.to_string()))
                                    .collect(),
                                cx,
                            ),
                        )
                        .child(self.dialog_ttl(cx));
                }
                if matches!(kind, KeyType::String | KeyType::Json) {
                    body = body.child(self.dialog_input("value", "值", true, cx));
                } else {
                    if kind == KeyType::Stream {
                        body = body.child(self.dialog_input(
                            "stream_id",
                            "ID（* 自动生成）",
                            false,
                            cx,
                        ));
                    }
                    for (index, row) in d.rows.iter().enumerate() {
                        let mut line = div().flex().gap_2().mb_2();
                        if matches!(kind, KeyType::Hash | KeyType::Stream) {
                            line = line.child(form_input(&row.id));
                        }
                        if kind == KeyType::ZSet {
                            line = line.child(form_input(&row.score).w(px(100.)));
                        }
                        line = line.child(row.value.render()).child(self.icon_btn(
                            format!("remove-row-{index}"),
                            "delete",
                            "Delete",
                            cx,
                            move |a, _, cx| {
                                if let Some(d) = &mut a.dialog {
                                    d.rows.remove(index);
                                    cx.notify();
                                }
                            },
                        ));
                        body = body.child(line);
                    }
                    body = body.child(self.btn("add-row", "添加", "add", cx, |a, w, cx| {
                        let row = Row {
                            id: input("", "Field", false, w, cx),
                            value: row_value("".into(), true, w, cx),
                            score: input("0", "Score", false, w, cx),
                        };
                        if let Some(d) = &mut a.dialog {
                            d.rows.push(row);
                            cx.notify();
                        }
                    }));
                }
                body = body
                    .child(
                        self.dialog_choice(
                            "format",
                            "查看方式",
                            codec::FORMATS
                                .iter()
                                .map(|s| (s.to_string(), s.to_string()))
                                .collect(),
                            cx,
                        ),
                    )
                    .child(self.dialog_choice(
                        "decode",
                        "解码 / 解压方式",
                        self.decoder_options(),
                        cx,
                    ));
                if matches!(d.kind, DialogKind::AddEntries) {
                    body = body
                        .child(self.dialog_checkbox("head", "插入列表头部", cx))
                        .child(self.dialog_checkbox("only_new", "只添加新字段 / 成员", cx));
                }
            }
            DialogKind::EditEntry(_) => {
                body = body
                    .child(self.dialog_input("field", "字段 / 索引 / 成员", false, cx))
                    .child(self.dialog_input("value", "值", true, cx));
                if d.target.as_ref().is_some_and(|t| t.kind == KeyType::ZSet) {
                    body = body.child(self.dialog_input("score", "分值", false, cx));
                }
            }
            DialogKind::Rename => {
                body = body
                    .child(self.dialog_input("key", "原键名", false, cx))
                    .child(self.dialog_input("new_key", "新键名", false, cx))
            }
            DialogKind::Ttl => {
                body = body
                    .when(!d.boolean("batch"), |body| {
                        body.child(self.dialog_input("key", &self.tr("common.key"), false, cx))
                    })
                    .child(self.dialog_ttl(cx));
                let mut quick = div().flex().flex_wrap().gap_2();
                for (value, unit, label) in [
                    (-1, 1, "interface.forever"),
                    (10, 1, "common.second"),
                    (1, 60, "common.minute"),
                    (1, 3600, "common.hour"),
                    (1, 86400, "common.day"),
                ] {
                    let text = if value < 0 {
                        self.tr(label)
                    } else {
                        format!("{value} {}", self.tr(label))
                    };
                    quick = quick.child(
                        self.btn(
                            format!("ttl-quick-{value}-{unit}"),
                            text,
                            "",
                            cx,
                            move |a, w, cx| {
                                a.set_dialog_field("ttl", value.to_string(), w, cx);
                                a.set_dialog_field("unit", unit.to_string(), w, cx);
                            },
                        )
                        .small()
                        .rounded_full(),
                    );
                }
                body = body
                    .child(div().mb_2().child(self.tr("dialogue.ttl.quick_set")))
                    .child(quick);
            }
            DialogKind::DeleteKeys => {
                if !d.previewed {
                    body = body
                        .child(self.dialog_input(
                            "pattern",
                            &self.tr("dialogue.key.key_expression"),
                            false,
                            cx,
                        ))
                        .child(self.dialog_checkbox(
                            "direct",
                            &self.tr("dialogue.key.direct_delete"),
                            cx,
                        ));
                } else if let Some(target) = &d.target {
                    body = body
                        .child(div().mb_2().child(format!(
                            "{} ({})",
                            self.tr("dialogue.key.affected_key"),
                            target.keys.len()
                        )))
                        .child(
                            div()
                                .id("affected-keys")
                                .max_h(px(180.))
                                .overflow_y_scroll()
                                .children(
                                    target
                                        .keys
                                        .iter()
                                        .map(|k| div().h(px(25.)).child(display_bytes(k))),
                                ),
                        );
                }
            }
            DialogKind::Flush => {
                body = body
                    .child(div().mb_4().child(format!(
                            "{}: {} / db{}",
                            self.tr("dialogue.key.server"),
                            d.target
                                .as_ref()
                                .map(|t| t.conn.name.clone())
                                .unwrap_or_default(),
                            d.target.as_ref().map(|t| t.db).unwrap_or_default()
                        )))
                    .child(self.dialog_checkbox(
                        "async",
                        &self.tr("dialogue.key.async_delete_title"),
                        cx,
                    ))
                    .child(self.dialog_checkbox(
                        "confirm",
                        &self.tr("dialogue.key.confirm_flush"),
                        cx,
                    ))
            }
            DialogKind::Import => {
                body = body
                    .child(self.dialog_file("path", "CSV 文件", false, cx))
                    .child(self.dialog_checkbox("replace", "覆盖同名键", cx))
                    .child(self.dialog_input("ttl", "TTL（-1 保留原过期时间，0 永久）", false, cx))
            }
            DialogKind::Export => {
                body = body
                    .child(self.dialog_file("path", "导出文件", true, cx))
                    .child(self.dialog_checkbox("include_ttl", "包含过期时间", cx))
                    .child(self.dialog_checkbox("all", "导出匹配表达式的全部键", cx))
                    .child(self.dialog_input("pattern", "键名匹配表达式", false, cx))
            }
            DialogKind::Group(_) => {
                body = body.child(self.dialog_input("name", "分组名", false, cx))
            }
            DialogKind::DeleteConnection(n) => body = body.child(format!("确认删除连接 {n}？")),
            DialogKind::DeleteGroup(n) => {
                body = body.child(self.tr("dialogue.remove_group_tip").replace("{name}", n))
            }
            DialogKind::About => {
                body = body
                    .items_center()
                    .gap_3()
                    .child(
                        img(ImageSource::Resource(Resource::Embedded(
                            "images/icon.png".into(),
                        )))
                        .size(px(120.)),
                    )
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child("Tiny RDM"),
                    )
                    .child(format!("v{}", crate::update::CURRENT_VERSION))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                self.btn(
                                    "about-source",
                                    self.tr("dialogue.about.source"),
                                    "",
                                    cx,
                                    |_, _, cx| {
                                        cx.open_url("https://github.com/tiny-craft/tiny-rdm")
                                    },
                                )
                                .ghost(),
                            )
                            .child("|")
                            .child(
                                self.btn(
                                    "about-website",
                                    self.tr("dialogue.about.website"),
                                    "",
                                    cx,
                                    |_, _, cx| cx.open_url("https://tinyrdm.com/"),
                                )
                                .ghost(),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "Copyright © {} Tinycraft.cc All rights reserved",
                                chrono::Local::now().format("%Y")
                            )),
                    )
            }
            DialogKind::Community => unreachable!("community uses the original image-only modal"),
        }
        let has_sections = !sections.is_empty();
        let mut nav = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .w(px(102.46875))
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .pr_0();
        for (key, label) in sections {
            let selected = d.tab == key;
            let key = key.to_string();
            nav = nav.child(
                div()
                    .border_r(px(if selected { 2. } else { 0. }))
                    .border_color(cx.theme().primary)
                    .child(
                        self.btn(
                            format!("dialog-section-{key}"),
                            "",
                            "",
                            cx,
                            move |a, _, cx| {
                                if let Some(d) = &mut a.dialog {
                                    d.tab = key.clone();
                                    d.font_menu = None;
                                    cx.notify();
                                }
                            },
                        )
                        .child(
                            div()
                                .w_full()
                                .text_right()
                                .font_weight(FontWeight(420.))
                                .child(self.tr(label)),
                        )
                        .ghost()
                        .rounded(px(0.))
                        .h(px(34.1875))
                        .w_full()
                        .text_color(if selected {
                            cx.theme().primary
                        } else {
                            cx.theme().foreground
                        }),
                    ),
            );
        }
        let connection_footer = matches!(
            d.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        );
        let mut footer = div().flex().items_center().gap(px(12.)).pt(px(16.));
        if matches!(
            d.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            footer = footer
                .child(
                    self.btn(
                        "test-connection",
                        self.tr("dialogue.connection.test"),
                        "",
                        cx,
                        |a, w, cx| a.test_connection(false, w, cx),
                    )
                    .loading(d.connection_testing),
                )
                .child(div().flex_1())
                .child(self.btn(
                    "parse-url",
                    self.tr("dialogue.connection.parse_url_clipboard"),
                    "",
                    cx,
                    |a, w, cx| a.parse_clipboard_url(w, cx),
                ));
        }
        if matches!(d.kind, DialogKind::Preferences) {
            footer = footer.child(self.btn(
                "restore-preferences",
                self.tr("preferences.restore_defaults"),
                "",
                cx,
                |a, w, cx| {
                    a.pref = Preferences::default();
                    a.save_preferences();
                    a.apply_theme(w, cx);
                    a.load_language();
                    a.open_dialog(DialogKind::Preferences, w, cx);
                },
            ));
        }
        footer = footer
            .when(!connection_footer, |footer| footer.child(div().flex_1()))
            .child(self.btn(
                "dialog-cancel",
                if special { "关闭" } else { "取消" },
                "",
                cx,
                |a, w, cx| {
                    a.close_dialog(w, cx);
                    cx.notify();
                },
            ));
        if !special {
            let label =
                if matches!(d.kind, DialogKind::DeleteKeys) && !d.previewed && !d.boolean("direct")
                {
                    self.tr("dialogue.key.show_affected_key")
                } else if matches!(d.kind, DialogKind::Preferences | DialogKind::Ttl) {
                    self.tr("common.save")
                } else if matches!(d.kind, DialogKind::Connection(Some(_))) {
                    self.tr("preferences.general.update")
                } else {
                    self.tr("common.confirm")
                };
            footer = footer.child(
                self.btn("dialog-save", label, "", cx, |a, w, cx| {
                    a.submit_dialog(w, cx)
                })
                .primary()
                .disabled(
                    self.transfer.is_some()
                        || matches!(d.kind, DialogKind::Flush) && !d.boolean("confirm")
                        || matches!(d.kind, DialogKind::DeleteKeys)
                            && d.previewed
                            && d.target.as_ref().is_some_and(|t| t.keys.is_empty()),
                ),
            );
        }
        let progress = d.progress.lock().clone();
        div()
            .id("dialog-mask")
            .test_support()
            .absolute()
            // The original modal is teleported above the application's 1px frame.
            .inset(px(-1.))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .occlude()
            .child(
                div()
                    .id("dialog-card")
                    .test_support()
                    .w(px(if matches!(d.kind, DialogKind::Preferences) {
                        640.
                    } else if has_sections {
                        600.
                    } else {
                        450.
                    }))
                    .max_h(relative(0.9))
                    .px(px(28.))
                    .pt(px(16.))
                    .pb(px(20.))
                    .rounded(px(4.))
                    .bg(cx.theme().background)
                    .font({
                        let mut font = gpui::font(".SystemUIFont");
                        font.fallbacks = Some(FontFallbacks::default());
                        font
                    })
                    .text_size(px(14.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .mb(px(12.5))
                            .line_height(px(22.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(18.))
                            .child(d.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .min_h_0()
                            .when(has_sections, |body| {
                                body.min_h(px(if matches!(d.kind, DialogKind::Preferences) {
                                    300.
                                } else {
                                    window.bounds().size.height.as_f32() * 0.5
                                }))
                            })
                            .max_h(px((window.bounds().size.height.as_f32() - 151.).max(300.)))
                            .when(has_sections, |d| d.child(nav))
                            .child(
                                div()
                                    .id("dialog-body")
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_y_scroll()
                                    .when(has_sections, |d| d.pl(px(12.)))
                                    .child(body),
                            ),
                    )
                    .when_some(d.connection_test.as_ref(), |card, result| {
                        let success = result.is_ok();
                        let color = rgb(if success { 0x18a058 } else { 0xd03050 });
                        card.child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(10.))
                                .p(px(12.))
                                .rounded(px(4.))
                                .bg(color.opacity(0.1))
                                .child(
                                    icon(if success { "checked" } else { "close" })
                                        .size(px(20.))
                                        .text_color(color),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .when(!success, |content| {
                                            content.child(
                                                div().font_weight(FontWeight::MEDIUM).child(
                                                    self.tr("dialogue.connection.test_fail"),
                                                ),
                                            )
                                        })
                                        .child(match result {
                                            Ok(()) => self.tr("dialogue.connection.test_succ"),
                                            Err(error) => error.clone(),
                                        }),
                                )
                                .child(self.icon_btn(
                                    "close-connection-test",
                                    "close",
                                    self.tr("menu.close"),
                                    cx,
                                    |app, _, cx| {
                                        if let Some(dialog) = &mut app.dialog {
                                            dialog.connection_test = None;
                                            cx.notify();
                                        }
                                    },
                                )),
                        )
                    })
                    .when(!progress.is_empty(), |d| d.child(progress))
                    .child(footer),
            )
            .into_any_element()
    }
    pub(super) fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.validate_connection_form(cx) {
            cx.notify();
            return;
        }
        if let Err(e) = self.submit_inner(window, cx) {
            if self.dialog.as_ref().is_some_and(|dialog| matches!(dialog.kind, DialogKind::NewKey)) {
                self.toast(e.to_string(), true);
            } else if e.is::<serde_json::Error>() || e.to_string().starts_with("json: cannot unmarshal") {
                self.error_notification(e.to_string());
            } else {
                self.toast(e.to_string(), true);
            }
        }
        cx.notify();
    }
    fn validate_connection_input(&mut self, key: &str, entity: EntityId, cx: &App) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        if !matches!(
            dialog.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) || dialog
            .fields
            .get(key)
            .is_none_or(|state| state.entity_id() != entity)
        {
            return;
        }
        let field = if matches!(key, "sock" | "port") {
            "addr"
        } else {
            key
        };
        if let Some(error) = connection_form_error(field, &dialog.value(field, cx)) {
            dialog.connection_errors.insert(field.into(), error);
        } else {
            dialog.connection_errors.remove(field);
        }
    }
    fn validate_connection_form(&mut self, cx: &App) -> bool {
        let Some(dialog) = &mut self.dialog else {
            return true;
        };
        if !matches!(
            dialog.kind,
            DialogKind::Connection(_) | DialogKind::ConnectionCopy(_)
        ) {
            return true;
        }
        for (tab, fields) in [
            ("general", ["name", "addr"]),
            ("advanced", ["default_filter", "key_separator"]),
        ] {
            let mut invalid = false;
            for field in fields {
                if let Some(error) = connection_form_error(field, &dialog.value(field, cx)) {
                    dialog.connection_errors.insert(field.into(), error);
                    invalid = true;
                } else {
                    dialog.connection_errors.remove(field);
                }
            }
            if invalid {
                dialog.tab = tab.into();
                return false;
            }
        }
        true
    }
    pub(super) fn save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &self.dialog {
            if matches!(
                dialog.kind,
                DialogKind::Connection(_)
                    | DialogKind::ConnectionCopy(_)
                    | DialogKind::Group(_)
                    | DialogKind::NewKey
                    | DialogKind::AddEntries
                    | DialogKind::EditEntry(_)
                    | DialogKind::Rename
                    | DialogKind::Ttl
                    | DialogKind::Preferences
                    | DialogKind::Decoder(_)
            ) {
                self.submit_dialog(window, cx);
            }
        } else {
            self.save_value(window, cx);
        }
    }
    fn submit_inner(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        self.finish_focused_number(window, cx);
        let d = self.dialog.as_ref().unwrap();
        if d.format_loading && !matches!(d.kind, DialogKind::EditEntry(_)) {
            return Ok(());
        }
        if matches!(d.kind, DialogKind::Import | DialogKind::Export) {
            self.submit_csv_transfer(window, cx);
            return Ok(());
        }
        if matches!(d.kind, DialogKind::NewKey) {
            return self.submit_new_key(window, cx);
        }
        if matches!(d.kind, DialogKind::AddEntries) {
            return self.submit_add_entries(window, cx);
        }
        if matches!(d.kind, DialogKind::EditEntry(_)) {
            return self.submit_edit_entry(window, cx);
        }
        let dialog_id = d.id;
        let kind = d.kind.clone();
        let target = d.target.clone();
        let progress = d.progress.clone();
        if matches!(kind, DialogKind::DeleteKeys) && !d.previewed && !d.boolean("direct") {
            let target = target.ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
            let pattern = d.value("pattern", cx);
            let token = Cancellation::default();
            self.job(
                window,
                cx,
                move |s| s.all_keys(&target.conn, target.db, &pattern, &token),
                move |a, keys, _, _| {
                    if let Some(d) = &mut a.dialog
                        && d.id == dialog_id
                        && matches!(d.kind, DialogKind::DeleteKeys)
                    {
                        d.previewed = true;
                        if let Some(t) = &mut d.target {
                            t.keys = keys;
                        }
                    }
                },
            );
            return Ok(());
        }
        match &kind {
            DialogKind::ConfirmEdit(connection) => {
                let connection = connection.clone();
                if let Some(tab) = self
                    .tabs
                    .iter()
                    .find(|tab| tab.conn.name == connection.name)
                {
                    self.close_tab(tab.id, window, cx);
                }
                self.open_dialog(DialogKind::Connection(Some(connection)), window, cx);
            }
            DialogKind::Connection(old) => {
                let mut c: Connection = d.result(cx)?;
                c.prepare_form_save(d.boolean("proxy.auth"));
                let name = old.as_ref().map(|c| c.name.as_str());
                crate::storage::save_connection(&mut self.connections, name, c)?;
                self.status_refresh_profiles.clear();
                self.save_connections();
                self.dialog = None;
            }
            DialogKind::ConnectionCopy(_) => {
                let mut c: Connection = d.result(cx)?;
                c.prepare_form_save(d.boolean("proxy.auth"));
                crate::storage::save_connection(&mut self.connections, None, c)?;
                self.status_refresh_profiles.clear();
                self.save_connections();
                self.dialog = None;
            }
            DialogKind::Group(old) => {
                let name = d.value("name", cx);
                if name.trim().is_empty() {
                    anyhow::bail!("分组名不能为空");
                }
                if self.connections.iter().any(|c| {
                    c.category == "group" && c.name == name && Some(&c.name) != old.as_ref()
                }) {
                    anyhow::bail!("同名分组已存在");
                }
                if let Some(old) = old {
                    if self.groups_open.remove(old) {
                        self.groups_open.insert(name.clone());
                    }
                    if let Some(c) = self
                        .connections
                        .iter_mut()
                        .find(|c| c.category == "group" && c.name == *old)
                    {
                        c.name = name.clone();
                    }
                    for tab in &mut self.tabs {
                        if tab.conn.group == *old {
                            tab.conn.group = name.clone();
                        }
                    }
                } else {
                    self.connections.push(Connection {
                        name: name.clone(),
                        category: "group".into(),
                        ..Default::default()
                    });
                }
                self.status_refresh_profiles.clear();
                self.save_connections();
                self.dialog = self.dialog_stack.pop();
                if let Some(parent) = &self.dialog
                    && let Some(field) = parent.fields.get("group")
                {
                    set_input(field, name, window, cx);
                }
            }
            DialogKind::DeleteKey(key) => {
                let target = target.ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
                let key = key.clone(); let deleted_key = key.clone(); let id = target.id;
                let label = display_bytes(&key);
                self.dialog = None;
                self.job(window,cx,move|service|Ok(service.run(&target.conn,target.db,vec![b"DEL".to_vec(),key])?.integer()?.max(0) as u64),move|app,count,_,cx|{
                    if let Some(i)=app.tab_index(id) {
                        let tab=&mut app.tabs[i];
                        tab.keys.retain(|entry|entry.key!=deleted_key);
                        tab.total=tab.total.saturating_sub(count);
                        tab.selected.clear();tab.key_selection=None;tab.current_key=None;tab.detail=None;tab.detail_epoch+=1;tab.pane=Pane::Status;
                    }
                    app.toast(app.tr("dialogue.delete.success").replace("{key}",&label),false);cx.notify();
                });
            }
            DialogKind::DeleteConnection(name) => {
                crate::storage::remove_connection(&mut self.connections, name);
                self.status_refresh_profiles.clear();
                self.save_connections();
                self.dialog = None;
                if let Some(t) = self.tabs.iter().find(|t| t.conn.name == *name) {
                    self.close_tab(t.id, window, cx);
                }
            }
            DialogKind::DeleteGroup(name) => {
                if let Some(i) = self
                    .connections
                    .iter()
                    .position(|g| g.category == "group" && g.name == *name)
                {
                    let group = self.connections.remove(i);
                    let names = flatten(&group.connections)
                        .into_iter()
                        .map(|c| c.name)
                        .collect::<HashSet<_>>();
                    let ids = self
                        .tabs
                        .iter()
                        .filter(|t| names.contains(&t.conn.name))
                        .map(|t| t.id)
                        .collect::<Vec<_>>();
                    for id in ids {
                        self.close_tab(id, window, cx);
                    }
                    self.status_refresh_profiles.clear();
                    self.save_connections();
                }
                self.dialog = None;
            }
            DialogKind::Preferences => {
                let return_focus = d.return_focus.clone();
                let mut pref = Preferences::from_form_json(d.result::<Json>(cx)?)?;
                pref.decoder = self.pref.decoder.clone();
                self.general_css_size = d.general_preview_size;
                self.pref = pref;
                self.save_preferences();
                self.apply_theme(window, cx);
                self.load_language();
                for tab in &self.tabs {
                    tab.editor.update(cx, |editor, cx| {
                        editor.set_line_number(self.pref.editor.show_line_num, window, cx);
                        editor.set_folding(self.pref.editor.show_folding, window, cx);
                        editor.set_drag_text(self.pref.editor.drop_text, cx);
                        editor.set_links_enabled(self.pref.editor.links, cx);
                    });
                    tab.cli_input.update(cx, |input, cx| {
                        input.set_caret_shape(self.pref.cli.cursor_style.clone(), cx)
                    });
                }
                for dialog in self.dialog_stack.iter()
                    .chain(self.cached_entry_panels.values().map(|(dialog, _)| dialog))
                {
                    for editor in dialog.editors.values() {
                        editor.update(cx, |editor, cx| {
                            editor.set_line_number(self.pref.editor.show_line_num, window, cx);
                            editor.set_folding(self.pref.editor.show_folding, window, cx);
                            editor.set_drag_text(self.pref.editor.drop_text, cx);
                            editor.set_links_enabled(self.pref.editor.links, cx);
                        });
                    }
                }
                self.dialog = self.dialog_stack.pop();
                self.full_editor = self.dialog.as_ref().is_some_and(|dialog| dialog.boolean("rdm_full_editor"));
                return_focus.unwrap_or_else(|| self.focus.clone()).focus(window, cx);
            }
            DialogKind::Decoder(index) => {
                let decoder: Decoder = d.result(cx)?;
                if decoder.name.trim().is_empty() {
                    anyhow::bail!("解码器名称不能为空");
                }
                if let Some(i) = index {
                    self.pref.decoder[*i] = decoder;
                } else {
                    self.pref.decoder.push(decoder);
                }
                if let Some(parent) = self.dialog_stack.pop() {
                    self.dialog = Some(parent);
                } else {
                    self.open_dialog(DialogKind::Preferences, window, cx);
                }
            }
            DialogKind::NewKey
            | DialogKind::AddEntries
            | DialogKind::EditEntry(_)
            | DialogKind::Rename
            | DialogKind::Ttl
            | DialogKind::DeleteKeys
            | DialogKind::Flush => {
                let target = target.ok_or_else(|| anyhow::anyhow!("请先打开 Redis 连接"))?;
                let id = target.id;
                let c = target.conn.clone();
                let db = target.db;
                let key = target.key.clone();
                let field = |name: &str| d.value(name, cx);
                let boolval = |name: &str| d.boolean(name);
                match kind {
                    DialogKind::NewKey | DialogKind::AddEntries => {
                        let new = matches!(kind, DialogKind::NewKey);
                        let kind = if new {
                            KeyType::parse(&field("type"))
                        } else {
                            target.kind
                        };
                        let key = if new { field("key").into_bytes() } else { key };
                        let decode = field("decode");
                        let format = field("format");
                        let value =
                            codec::encode(&field("value"), &decode, &format, &self.pref.decoder)?;
                        let rows = d
                            .rows
                            .iter()
                            .map(|r| {
                                Ok(Entry {
                                    id: text_input(&r.id, cx).into_bytes(),
                                    value: codec::encode(
                                        &text_input(&r.value, cx),
                                        &decode,
                                        &format,
                                        &self.pref.decoder,
                                    )?,
                                    score: text_input(&r.score, cx),
                                    ..Default::default()
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let ttl = if new {
                            field("ttl")
                                .parse::<i64>()?
                                .checked_mul(field("unit").parse::<i64>().unwrap_or(1))
                                .ok_or_else(|| anyhow::anyhow!("TTL 超出范围"))?
                        } else {
                            -1
                        };
                        let head = boolval("head");
                        let only_new = boolval("only_new");
                        let stream_id = field("stream_id");
                        self.dialog = None;
                        self.job(
                            window,
                            cx,
                            move |s| {
                                if new {
                                    let bytes = if kind == KeyType::Stream {
                                        stream_id.as_bytes()
                                    } else {
                                        &value
                                    };
                                    s.create(&c, db, &key, kind, bytes, &rows, ttl)
                                } else {
                                    s.add_entries(
                                        &c, db, &key, kind, &rows, head, only_new, &stream_id,
                                    )
                                }
                            },
                            move |a, _, w, cx| {
                                a.scan_keys(id, true, false, w, cx);
                                a.reload_detail(id, true, w, cx);
                            },
                        );
                    }
                    DialogKind::EditEntry(old) => {
                        let new_field = field("field");
                        let dialog_id = d.id;
                        let saved_field = new_field.clone();
                        let saved_value = field("value");
                        let saved_score = field("score");
                        let pin = boolval("pin");
                        let new = Entry {
                            id: if new_field == display_bytes(&old.id) {
                                old.id.clone()
                            } else {
                                new_field.into_bytes()
                            },
                            value: codec::encode(
                                &field("value"),
                                &field("decode"),
                                &field("format"),
                                &self.pref.decoder,
                            )?,
                            score: field("score"),
                            ..Default::default()
                        };
                        let kind = target.kind;
                        let updated = new.clone();
                        if !pin {
                            self.dialog = None;
                            self.full_editor = false;
                        }
                        self.job(
                            window,
                            cx,
                            move |s| s.edit_entry(&c, db, &key, &kind, &old, &new, false),
                            move |a, _, w, cx| {
                                if pin
                                    && let Some(d) = &mut a.dialog
                                    && d.id == dialog_id
                                    && matches!(d.kind, DialogKind::EditEntry(_))
                                {
                                    d.kind = DialogKind::EditEntry(updated);
                                    d.data["field"] = Json::String(saved_field);
                                    d.data["value"] = Json::String(saved_value);
                                    d.data["score"] = Json::String(saved_score);
                                }
                                a.reload_detail(id, true, w, cx);
                            },
                        );
                    }
                    DialogKind::Rename => {
                        let new = field("new_key").into_bytes();
                        if new.is_empty() {
                            anyhow::bail!("新键名不能为空");
                        }
                        self.dialog = None;
                        let new2 = new.clone();
                        self.job(
                            window,
                            cx,
                            move |s| s.rename(&c, db, &key, &new),
                            move |a, _, w, cx| {
                                a.select_key(id, new2, w, cx);
                                a.scan_keys(id, true, false, w, cx);
                            },
                        );
                    }
                    DialogKind::Flush => {
                        if !boolval("confirm") {
                            anyhow::bail!("请勾选确认清空数据库");
                        }
                        let asynchronous = boolval("async");
                        self.dialog = None;
                        self.job(
                            window,
                            cx,
                            move |s| s.flush(&c, db, asynchronous),
                            move |a, _, w, cx| {
                                if let Some(i) = a.tab_index(id) {
                                    a.tabs[i].detail = None;
                                    a.tabs[i].current_key = None;
                                }
                                a.scan_keys(id, true, false, w, cx);
                            },
                        );
                    }
                    DialogKind::Ttl | DialogKind::DeleteKeys => {
                        let pattern = field("pattern");
                        let ttl = if matches!(kind, DialogKind::Ttl) {
                            field("ttl").parse::<i64>()?.checked_mul(field("unit").parse::<i64>().unwrap_or(1))
                                .ok_or_else(|| anyhow::anyhow!("TTL 超出范围"))?
                        } else {-1};
                        let all = boolval("all");
                        let batch_ttl = boolval("batch");
                        if matches!(kind, DialogKind::Ttl) && target.keys.is_empty() {
                            anyhow::bail!("请先选择要设置 TTL 的键");
                        }
                        let cancel = Cancellation::default();
                        let apply_cancel = cancel.clone();
                        self.transfer = Some(cancel.clone());
                        let mut keys = target.keys;
                        let mode = kind.clone();
                        self.job(
                            window,
                            cx,
                            move |s| {
                                Ok((|| -> Result<crate::service::TransferProgress> {
                                    if keys.is_empty() || all {
                                        keys = s.all_keys(&c, db, &pattern, &cancel)?;
                                    }
                                    let update = |r: crate::service::TransferProgress| {
                                        *progress.lock() = format!(
                                            "{} / {} · failed {} · skipped {}",
                                            r.done, r.total, r.failed, r.skipped
                                        );
                                    };
                                    match mode {
                                        DialogKind::Ttl => {
                                            if batch_ttl {
                                                s.batch(&c, db, &keys, Some(ttl), &cancel, update)
                                            } else {
                                                let key = keys
                                                    .first()
                                                    .ok_or_else(|| anyhow::anyhow!("请先选择键"))?;
                                                if cancel.canceled() {
                                                    return Ok(crate::service::TransferProgress {
                                                        total: 1,
                                                        canceled: true,
                                                        ..Default::default()
                                                    });
                                                }
                                                s.ttl(&c, db, key, ttl)?;
                                                let result = crate::service::TransferProgress {
                                                    done: 1,
                                                    total: 1,
                                                    ..Default::default()
                                                };
                                                update(result.clone());
                                                Ok(result)
                                            }
                                        }
                                        _ => s.batch(&c, db, &keys, None, &cancel, update),
                                    }
                                })())
                            },
                            move |a, result, w, cx| {
                                if a.transfer
                                    .as_ref()
                                    .is_some_and(|token| Arc::ptr_eq(&token.0, &apply_cancel.0))
                                {
                                    a.transfer = None;
                                }
                                let current = a
                                    .dialog
                                    .as_ref()
                                    .is_some_and(|dialog| dialog.id == dialog_id);
                                match result {
                                    Ok(r) => {
                                        if current {
                                            a.dialog = None;
                                        }
                                        if !r.canceled && !apply_cancel.canceled() {
                                            a.toast(
                                                format!(
                                                    "{} completed, {} failed, {} skipped",
                                                    r.done, r.failed, r.skipped
                                                ),
                                                r.failed > 0,
                                            );
                                        }
                                        a.scan_keys(id, true, false, w, cx);
                                        a.reload_detail(id, true, w, cx);
                                    }
                                    Err(e) => {
                                        if current {
                                            a.toast(e.to_string(), true);
                                        }
                                    }
                                }
                            },
                        );
                    }
                    _ => {}
                }
            }
            _ => {
                self.dialog = None;
            }
        }
        cx.notify();
        Ok(())
    }
    pub(super) fn test_connection(
        &mut self,
        sentinel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let config = match dialog.result::<Connection>(cx) {
            Ok(config) => config,
            Err(error) => {
                self.toast(error.to_string(), true);
                return;
            }
        };
        let dialog_id = dialog.id;
        dialog.connection_testing = true;
        dialog.connection_test = None;
        self.job(
            window,
            cx,
            move |service| {
                Ok((|| -> Result<Vec<String>> {
                    if sentinel {
                        let response = service.sentinel_masters(&config)?;
                        let names = response
                            .array()?
                            .iter()
                            .filter_map(|row| {
                                row.array().ok().and_then(|fields| {
                                    fields
                                        .chunks(2)
                                        .find(|pair| pair.len() == 2 && pair[0].text() == "name")
                                        .map(|pair| pair[1].text())
                                })
                            })
                            .collect::<Vec<_>>();
                        if names.is_empty() {
                            anyhow::bail!("list sentinel master fail");
                        }
                        Ok(names)
                    } else {
                        let mut client = Client::connect(&config, 0)?;
                        if client.query(&command(&["PING"]))?.checked()?.text() != "PONG" {
                            anyhow::bail!("invalid Redis PING response");
                        }
                        Ok(vec![])
                    }
                })()
                .map_err(|error| error.to_string()))
            },
            move |app, result, w, cx| {
                let Some(dialog) = &mut app.dialog else {
                    return;
                };
                if dialog.id != dialog_id {
                    return;
                }
                dialog.connection_testing = false;
                if sentinel {
                    match result {
                        Ok(names) => {
                            if let Some(field) = dialog.fields.get("sentinel.master") {
                                set_input(field, names[0].clone(), w, cx);
                            }
                            dialog.sentinel_masters = names;
                        }
                        Err(error) => app.toast(error, true),
                    }
                } else {
                    dialog.connection_test = Some(result.map(|_| ()));
                }
            },
        );
    }
    fn parse_clipboard_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) else {
            return;
        };
        match Connection::from_url(&text) {
            Ok(c) => self.open_dialog(DialogKind::ConnectionCopy(c), window, cx),
            Err(e) => self.toast(e.to_string(), true),
        }
    }
    fn choose_dialog_file(
        &mut self,
        key: &str,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::file_dialogs::{self, Purpose, Request};
        let Some(dialog) = self.dialog.as_ref() else {
            return;
        };
        let dialog_id = dialog.id;
        let purpose = match dialog.kind {
            DialogKind::Import => Purpose::OpenCsv,
            DialogKind::Export => Purpose::SaveCsv,
            _ => Purpose::OpenFile,
        };
        let mut request=Request::new(purpose,chrono::Local::now());
        if purpose==Purpose::SaveCsv && let Some(cache)=&dialog.csv_filename {request.filename=cache.name.clone();}
        let key = key.to_string();
        let task = file_dialogs::prompt(request, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await.unwrap_or_else(|error| Err(error.into()));
            if let Some(value) = file_dialogs::field_value(result, save) {
                let _ = this.update_in(cx, |a, w, cx| {
                    if let Some(d) = &a.dialog
                        && d.id == dialog_id
                        && let Some(f) = d.fields.get(&key)
                    {
                        set_input(f, value, w, cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn connection_file(
        &mut self,
        export: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::file_dialogs::{self, Purpose, Request};
        let store = self.store.clone();
        let purpose = if export {
            Purpose::SaveConnectionBackup
        } else {
            Purpose::OpenConnectionBackup
        };
        let task = file_dialogs::prompt(Request::new(purpose, chrono::Local::now()), window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = match task.await.unwrap_or_else(|error| Err(error.into())) {
                Ok(Some(path)) if export => store.export_connections(&path).map(|_| None),
                Ok(Some(path)) => store.import_connections(&path).map(Some),
                Ok(None) => Err(file_dialogs::cancelled_backup_error()),
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |a, _, cx| {
                match result {
                    Ok(Some(c)) => {
                        a.connections = c;
                        a.status_refresh_profiles.clear();
                        a.toast(a.tr("dialogue.handle_succ"), false);
                    }
                    Ok(None) => a.toast(a.tr("dialogue.handle_succ"), false),
                    Err(e) => a.toast(e.to_string(), true),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn export_stream(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::file_dialogs::{self, Purpose, Request};
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let text = self.tabs[i].monitor.as_ref().map(|stream| {
            stream.lines.iter().map(|line| format!("{}\n", line.export())).collect::<String>()
        }).unwrap_or_default();
        let task = file_dialogs::prompt(
            Request::new(Purpose::SaveMonitorLog, chrono::Local::now()), window, cx,
        );
        // The original monitor ignores ExportLog's return value, including cancel.
        cx.background_executor().spawn(async move {
            if let Ok(Ok(Some(path))) = task.await {
                let _ = crate::storage::atomic_write(&path, text.as_bytes());
            }
        }).detach();
    }
    pub(super) fn check_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fetch_updates(true, window, cx);
    }
    pub(super) fn fetch_updates(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.update_loading {
            return;
        }
        self.update_loading = true;
        let loading = manual.then(|| self.loading_message(self.tr("interface.retrieving_version")));
        let task = cx
            .background_executor()
            .spawn(async { crate::update::fetch() });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |a, _, cx| {
                a.update_loading = false;
                if let Some(id) = loading {
                    a.destroy_message(id);
                }
                match result {
                    Ok(release) => {
                        a.cache_sponsors(&release.sponsor);
                        let language = if a.pref.general.language == "auto" {
                            system_language()
                        } else {
                            a.pref.general.language.clone()
                        };
                        let url = release.text(&release.download_page, &language).to_string();
                        if release.available(
                            crate::update::CURRENT_VERSION,
                            &a.pref.general.skip_version,
                            manual,
                        ) && !url.is_empty()
                        {
                            let description =
                                release.text(&release.description, &language).to_string();
                            let description = if description.is_empty() {
                                a.tr("dialogue.upgrade.new_version_tip")
                                    .replace("{ver}", &release.version)
                            } else {
                                description
                            };
                            a.update_notice = Some((release, description, url));
                        } else if manual {
                            a.toast(a.tr("dialogue.upgrade.no_update"), false);
                        }
                    }
                    Err(error) => {
                        if manual {
                            a.toast(error.to_string(), true);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
