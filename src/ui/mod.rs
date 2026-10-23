#[cfg(all(target_os = "macos", not(feature = "ui-qa")))]
mod app_icon;
mod connection_tree;
mod key_browser;
#[cfg(feature = "ui-qa")]
mod key_loading_qa;
mod key_selectors;
mod entries;
mod entry_delete;
mod value_footer;
mod text_control_width;
mod text;
mod content_header;
mod scalar_value;
mod json_diagnostics;
mod diagnostic_ui;
mod refresh;
mod dialogs;
mod file_dialogs;
mod notices;
mod tab_close;
mod connection_tabs;
#[cfg(feature = "ui-qa")]
mod qa;
#[cfg(feature = "ui-qa")]
mod entries_qa;
#[cfg(feature = "ui-qa")]
mod key_filter_qa;
#[cfg(feature = "ui-qa")]
mod qa_json;
#[cfg(feature = "ui-qa")]
mod qa_string;
#[cfg(feature = "ui-qa")]
mod qa_key_input;
#[cfg(feature = "ui-qa")]
mod qa_refresh;
#[cfg(feature = "ui-qa")]
mod qa_refresh_status;
mod terminal;
mod views;
mod sidebar;
mod status_values;
mod status_chart;
mod status_chart_text;
mod status_number;
mod status_info;
mod status_clients;
mod diagnostic_pages;
mod match_tooltip;
#[cfg(feature = "ui-qa")]
mod match_tooltip_qa;
#[cfg(feature = "ui-qa")]
mod status_values_qa;
#[cfg(feature = "ui-qa")]
mod status_layout_qa;
#[cfg(feature = "ui-qa")]
mod status_info_qa;
#[cfg(feature = "ui-qa")]
mod keypages_qa;
#[cfg(feature = "ui-qa")]
mod diagnostic_interactions;
#[cfg(feature = "ui-qa")]
mod status_clients_qa;
#[cfg(feature = "ui-qa")]
pub fn run_qa(data_dir: std::path::PathBuf, output_dir: std::path::PathBuf) {
    qa::run(data_dir, output_dir);
}
use crate::{
    cli, codec,
    model::*,
    redis::{Cancellation, Client, command},
    service::{KeyNotFound, ScanState, Service, SlowLog, info_value},
    storage::{Store, flatten},
};
use anyhow::Result;
use gpui::*;
use gpui_component::{
    button::*,
    input::{Editor, EditorState, Input, InputEvent, InputState, TextareaState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
    *,
};
use parking_lot::Mutex;
use indexmap::IndexSet;
use serde_json::Value as Json;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, HashSet, VecDeque},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
#[cfg(feature = "ui-qa")]
use std::path::PathBuf;
thread_local! {static TRANSLATIONS:RefCell<BTreeMap<String,String>>=RefCell::new(BTreeMap::new());}
const CONTENT_TABS_HEIGHT: f32 = 39.1875;
fn localized(text: &str) -> String {
    TRANSLATIONS.with(|map| {
        map.borrow()
            .get(text)
            .cloned()
            .unwrap_or_else(|| text.to_string())
    })
}
fn system_language() -> String {
    if let Ok(value) = std::env::var("REDISDOCK_LANGUAGE")
        .or_else(|_| std::env::var("RUST_RDM_LANGUAGE"))
    {
        return value;
    }
    #[cfg(target_os = "macos")]
    let locale = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
        .and_then(|value| {
            value
                .lines()
                .map(|line| line.trim().trim_matches(['"', ',', '(', ')', ' ']))
                .find(|line| !line.is_empty())
                .map(str::to_string)
        });
    #[cfg(not(target_os = "macos"))]
    let locale: Option<String> = None;
    let locale = locale
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en-us".into())
        .to_ascii_lowercase()
        .replace('_', "-");
    if locale.starts_with("zh") {
        if locale.contains("hant") || locale.contains("tw") || locale.contains("hk") {
            "zh-tw".into()
        } else {
            "zh-cn".into()
        }
    } else {
        let prefix = locale.split(['-', '.']).next().unwrap_or("en");
        match prefix {
            "ja" => "ja-jp",
            "ko" => "ko-kr",
            "fr" => "fr-fr",
            "es" => "es-es",
            "pt" => "pt-br",
            "ru" => "ru-ru",
            "tr" => "tr-tr",
            _ => "en-us",
        }
        .into()
    }
}

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/"]
struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = Self::get(path) {
            Ok(Some(f.data))
        } else {
            gpui_component_assets::Assets.load(path)
        }
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut a = gpui_component_assets::Assets.list(path)?;
        a.extend(
            Self::iter()
                .filter(|p| p.starts_with(path))
                .map(|p| SharedString::from(p.into_owned())),
        );
        Ok(a)
    }
}
actions!(
    rdm,
    [
        NewConnection,
        PreferencesAction,
        Refresh,
        SaveValue,
        CloseTab,
        CancelDialog,
        NewKey,
        FocusFilter,
        Quit
    ]
);
fn initialize_actions(cx: &mut App) {
    diagnostic_ui::init(cx);
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.set_menus(vec![
        Menu {
            name: "RedisDock".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Preferences…", PreferencesAction),
                MenuItem::Separator,
                MenuItem::action("Quit RedisDock", Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::os_action("Undo", gpui_component::input::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", gpui_component::input::Redo, OsAction::Redo),
                MenuItem::Separator,
                MenuItem::os_action("Cut", gpui_component::input::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", gpui_component::input::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", gpui_component::input::Paste, OsAction::Paste),
                MenuItem::os_action(
                    "Select All",
                    gpui_component::input::SelectAll,
                    OsAction::SelectAll,
                ),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![MenuItem::action("Close Tab", CloseTab)],
        },
    ]);
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewConnection, Some("Rdm")),
        KeyBinding::new("ctrl-n", NewConnection, Some("Rdm")),
        KeyBinding::new("cmd-,", PreferencesAction, Some("Rdm")),
        KeyBinding::new("ctrl-,", PreferencesAction, Some("Rdm")),
        KeyBinding::new("cmd-r", Refresh, Some("Rdm")),
        KeyBinding::new("f5", Refresh, Some("Rdm")),
        KeyBinding::new("cmd-s", SaveValue, Some("Rdm")),
        KeyBinding::new("ctrl-s", SaveValue, Some("Rdm")),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" }, CloseTab, Some("Rdm")),
        KeyBinding::new("escape", CancelDialog, Some("Rdm")),
        KeyBinding::new("cmd-f", FocusFilter, Some("Rdm")),
    ]);
    // The console handles these as terminal input before application/editor actions.
    cx.bind_keys(('a'..='z').map(|letter| {
        KeyBinding::new(&format!("ctrl-{letter}"), NoAction, Some("RdmCli > Input"))
    }));
    cx.bind_keys([KeyBinding::new("cmd-c", NoAction, Some("RdmCli > Input"))]);
    cx.bind_keys(
        [
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "backspace",
            "delete",
            "tab",
            "escape",
        ]
        .map(|key| KeyBinding::new(key, NoAction, Some("RdmCli > Input"))),
    );
}

pub fn run() {
    let store = Store::system();
    let pref = store.preferences().unwrap_or_default();
    let bounds = size(
        px(pref.behavior.window_width.max(960.)),
        px(pref.behavior.window_height.max(640.)),
    );
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            initialize_actions(cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some({
                        let saved = Bounds {
                            origin: point(
                                px(pref.behavior.window_pos_x),
                                px(pref.behavior.window_pos_y),
                            ),
                            size: bounds,
                        };
                        let valid = pref.behavior.window_pos_x > 0.
                            && pref.behavior.window_pos_y > 0.
                            && cx.displays().iter().any(|display| {
                                let screen = display.bounds();
                                saved.origin.x >= screen.origin.x
                                    && saved.origin.y >= screen.origin.y
                                    && saved.origin.x + saved.size.width
                                        <= screen.origin.x + screen.size.width
                                    && saved.origin.y + saved.size.height
                                        <= screen.origin.y + screen.size.height
                            });
                        let saved = if valid {
                            saved
                        } else {
                            Bounds::centered(None, bounds, cx)
                        };
                        if pref.behavior.window_maximised {
                            WindowBounds::Maximized(saved)
                        } else {
                            WindowBounds::Windowed(saved)
                        }
                    }),
                    window_min_size: Some(size(px(960.), px(640.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("RedisDock".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| RdmApp::new(store, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("无法创建窗口");
            cx.activate(true);
        });
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nav {
    Browser,
    Server,
    Log,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Status,
    Detail,
    Cli,
    Slow,
    Monitor,
    Pubsub,
}
#[derive(Clone)]
struct ConnectionDrag {
    name: String,
    group: bool,
}
impl Render for ConnectionDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded(px(4.))
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_md()
            .child(self.name.clone())
    }
}
struct StreamLine {
    time: String,
    channel: Option<String>,
    text: String,
}
impl StreamLine {
    fn export(&self) -> String {
        if let Some(channel) = &self.channel {
            format!("{}\t{}\t{}", self.time, channel, self.text)
        } else {
            self.text.clone()
        }
    }
}
enum StreamEvent {
    Line(StreamLine),
    Error(String),
    Stopped,
}
struct StreamState {
    cancel: Cancellation,
    receiver: mpsc::Receiver<StreamEvent>,
    lines: VecDeque<StreamLine>,
    running: bool,
    auto_last: bool,
    list: ListState,
}
impl StreamState {
    fn idle() -> Self {
        let (_, receiver) = mpsc::sync_channel(1);
        Self {
            cancel: Cancellation::default(),
            receiver,
            lines: VecDeque::new(),
            running: false,
            auto_last: true,
            list: ListState::new(0, ListAlignment::Top, px(500.)),
        }
    }
}
impl Drop for StreamState {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
struct Sample {
    time: String,
    ops: f64,
    clients: f64,
    memory: f64,
    input: f64,
    output: f64,
}
#[derive(Clone, Copy)]
enum RefreshTarget {
    Status,
    Detail,
    Slow,
}
struct Tab {
    id: u64,
    conn: Connection,
    db: u32,
    pane: Pane,
    content_nav_offset: std::cell::Cell<f32>,
    content_nav_layout: std::cell::Cell<Option<(Pane, f32, [f32; 6])>>,
    databases: Vec<Database>,
    info: Info,
    samples: VecDeque<Sample>,
    keys: Vec<KeyEntry>,
    scan: ScanState,
    scan_epoch: u64,
    keys_loading: bool,
    total: u64,
    selected: IndexSet<Vec<u8>>,
    check_mode: bool,
    key_selection: Option<key_browser::Selection>,
    key_focus: FocusHandle,
    key_scroll: UniformListScrollHandle,
    key_match: String,
    key_filter: String,
    key_filter_epoch: u64,
    key_filter_changed: Option<Instant>,
    key_root_expanded: bool,
    database_picker: key_selectors::DatabasePicker,
    type_picker: key_selectors::TypePicker,
    expanded: HashSet<String>,
    flat: bool,
    filter: Entity<InputState>,
    key_type: String,
    exact: bool,
    detail: Option<KeyDetail>,
    key_name: Entity<InputState>,
    current_key: Option<Vec<u8>>,
    detail_epoch: u64,
    editor: Entity<EditorState>,
    editor_original: String,
    json_brackets: gpui_component::input::TextDecorationCollection,
    json_diagnostics: json_diagnostics::Controller,
    row_values: BTreeMap<Vec<u8>, codec::Converted>,
    removed_fields: HashSet<Vec<u8>>,
    format_epoch: u64,
    values_loading: bool,
    value_saving: bool,
    value_save_epoch: u64,
    row_filter: Entity<InputState>,
    row_match: String,
    row_filter_value: String,
    row_filter_epoch: u64,
    row_filter_changed: Option<Instant>,
    entry_list: ListState,
    format: String,
    decode: String,
    reverse: bool,
    field_sort: Option<bool>,
    score_sort: Option<bool>,
    entry_delete: Option<entry_delete::EntryDelete>,
    footer_picker: Option<value_footer::FooterPicker>,
    footer_hover: Option<value_footer::PickerKind>,
    footer_tooltip: Option<value_footer::PickerKind>,
    footer_hover_epoch: u64,
    footer_tooltip_bounds: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    match_tooltips: match_tooltip::MatchTooltips,
    column_widths: BTreeMap<String, f32>,
    editing: bool,
    auto_refresh: bool,
    refresh_seconds: f64,
    refresh_numbers: [Option<f64>; 3],
    status_seconds: f64,
    refresh_interval: Entity<InputState>,
    detail_loaded: Instant,
    cli_input: Entity<InputState>,
    cli_output: Vec<cli::Line>,
    cli_history: cli::History,
    cli_db: u32,
    cli_scroll: ScrollHandle,
    cli_view: std::rc::Rc<RefCell<terminal::CliView>>,
    slow: Vec<SlowLog>,
    slow_limit: Entity<InputState>,
    slow_keyword: Entity<InputState>,
    slow_client: Entity<InputState>,
    slow_ascending: Option<bool>,
    slow_auto: bool,
    slow_seconds: f64,
    slow_interval: Entity<InputState>,
    slow_polled: Instant,
    clients: Vec<BTreeMap<String, String>>,
    clients_loading: bool,
    client_load_started: Instant,
    #[cfg(feature = "ui-qa")]
    client_load_gate: Option<std::sync::Arc<status_clients_qa::LoadGate>>,
    client_sort: status_clients::ClientSort,
    client_trigger_bounds: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    client_grid_bounds: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    info_group: String,
    info_filter: Entity<InputState>,
    status_auto: bool,
    info_details: bool,
    status_interval: Entity<InputState>,
    monitor: Option<StreamState>,
    pubsub: Option<StreamState>,
    stream_filter: Entity<InputState>,
    pub_channel: Entity<InputState>,
    pub_message: Entity<InputState>,
    pub_pattern: Entity<InputState>,
    pub_history: Vec<String>,
    pub_received: Option<(i64, Instant)>,
}
impl Tab {
    fn record_info(&mut self, info: Info) {
        let value = |section, key| status_values::info_integer(&info, section, key);
        self.samples.push_back(Sample {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            ops: value("Stats", "instantaneous_ops_per_sec"),
            clients: value("Clients", "connected_clients"),
            memory: value("Memory", "used_memory"),
            input: value("Stats", "instantaneous_input_kbps"),
            output: value("Stats", "instantaneous_output_kbps"),
        });
        self.info = info;
        while self.samples.len() > 5 {
            self.samples.pop_front();
        }
    }
}
struct RdmApp {
    store: Store,
    service: Arc<Service>,
    connections: Vec<Connection>,
    status_refresh_profiles: BTreeMap<String, f64>,
    pref: Preferences,
    general_css_size: f32,
    lang: Json,
    nav: Nav,
    tabs: Vec<Tab>,
    active: Option<u64>,
    connection_tabs_scroll: ScrollHandle,
    next_id: u64,
    dialog: Option<dialogs::Dialog>,
    dialog_stack: Vec<dialogs::Dialog>,
    cached_entry_panels: BTreeMap<u64, (dialogs::Dialog, bool)>,
    tab_close_warnings: Vec<tab_close::CloseWarning>,
    next_dialog_id: u64,
    conn_filter: Entity<InputState>,
    log_filter: Entity<InputState>,
    focus: FocusHandle,
    subs: Vec<Subscription>,
    busy: usize,
    entry_conversions: usize,
    messages: Vec<notices::MessageNotice>,
    notifications: Vec<notices::ErrorNotification>,
    next_notice_id: u64,
    last_poll: Instant,
    last_auto: Instant,
    log_server: String,
    log_ascending: Option<bool>,
    log_command_width: Option<f32>,
    log_scroll: ScrollHandle,
    sidebar_resizing: bool,
    sidebar_hover: bool,
    sidebar_save_epoch: u64,
    transfer: Option<Cancellation>,
    csv_import_unit: u32,
    csv_export_expire: bool,
    add_entries_reload: bool,
    csv_transfers: BTreeMap<u64, dialogs::csv_transfer::CsvTransfer>,
    csv_progress: (mpsc::Sender<dialogs::csv_transfer::CsvProgressEvent>, mpsc::Receiver<dialogs::csv_transfer::CsvProgressEvent>),
    full_editor: bool,
    groups_open: HashSet<String>,
    connection_selected: Option<String>,
    connection_pending: Option<String>,
    connection_focus: FocusHandle,
    connection_scroll: ScrollHandle,
    fonts: Vec<String>,
    viewport_width: f32,
    column_drag: Option<(u64, String, f32, f32)>,
}
fn input(
    text: impl Into<SharedString>,
    placeholder: impl Into<SharedString>,
    _multi: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let text = text.into();
    let placeholder = placeholder.into();
    cx.new(|cx| {
        let mut i = InputState::new(window, cx).placeholder(placeholder);
        i.set_placeholder_colors(rgb(0xc2c2c2).into(), rgba(0xffffff61).into(), cx);
        i.set_value(text, window, cx);
        i
    })
}
trait TextField {
    fn value(&self, cx: &App) -> String;
    fn set(&self, text: SharedString, window: &mut Window, cx: &mut App);
}
macro_rules! text_field {
    ($kind:ty) => {
        impl TextField for Entity<$kind> {
            fn value(&self, cx: &App) -> String {
                self.read(cx).value().to_string()
            }
            fn set(&self, text: SharedString, window: &mut Window, cx: &mut App) {
                self.update(cx, |i, cx| i.set_value(text, window, cx));
            }
        }
    };
}
text_field!(InputState);
text_field!(EditorState);
text_field!(TextareaState);
fn set_input(
    state: &impl TextField,
    text: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    state.set(text.into(), window, cx);
}
fn text_input(state: &impl TextField, cx: &App) -> String {
    state.value(cx)
}
fn editor(
    text: String,
    language: &str,
    pref: &Preferences,
    window: &mut Window,
    cx: &mut App,
) -> Entity<EditorState> {
    cx.new(|cx| {
        let mut state = EditorState::new(window, cx)
            .language(language.to_string())
            .line_number(pref.editor.show_line_num)
            .folding(pref.editor.show_folding);
        state.set_drag_text(pref.editor.drop_text, cx);
        state.set_placeholder_colors(rgb(0xc2c2c2).into(), rgba(0xffffff61).into(), cx);
        state.set_links_enabled(pref.editor.links, cx);
        state.set_value(text, window, cx);
        diagnostic_ui::install(&mut state);
        state
    })
}
fn icon(name: &str) -> Icon {
    Icon::default().path(SharedString::from(format!("icons/{name}.svg")))
}
fn folder_icon(open: bool, cx: &App) -> Img {
    let name = format!(
        "icons/folder-{}-{}.svg",
        if open { "open" } else { "closed" },
        if cx.theme().mode == ThemeMode::Dark {
            "dark"
        } else {
            "light"
        }
    );
    img(ImageSource::Resource(Resource::Embedded(name.into()))).size(px(20.))
}
fn natural_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_a, start_b) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let mut da = &a[start_a..i];
            let mut db = &b[start_b..j];
            while da.first() == Some(&b'0') {
                da = &da[1..];
            }
            while db.first() == Some(&b'0') {
                db = &db[1..];
            }
            let order = da.len().cmp(&db.len()).then(da.cmp(db));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    a.len().saturating_sub(i).cmp(&b.len().saturating_sub(j))
}
fn editor_language(format: &str) -> &'static str {
    match format {
        "JSON" | "Unicode JSON" => "json",
        "YAML" => "yaml",
        "XML" => "html",
        _ => "plaintext",
    }
}
impl RdmApp {
    fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let connections = store.connections();
        let preferences = store.preferences();
        let pref = preferences.as_ref().cloned().unwrap_or_default();
        let mut app = Self {
            service: Arc::new(Service::new(store.clone())),
            store,
            connections: connections.as_ref().cloned().unwrap_or_default(),
            status_refresh_profiles: BTreeMap::new(),
            general_css_size: pref.general.font_size,
            pref,
            lang: Json::Null,
            nav: Nav::Server,
            tabs: vec![],
            active: None,
            connection_tabs_scroll: ScrollHandle::new(),
            next_id: 0,
            dialog: None,
            dialog_stack: vec![],
            cached_entry_panels: BTreeMap::new(),
            tab_close_warnings: vec![],
            next_dialog_id: 0,
            conn_filter: input("", "筛选", false, window, cx),
            log_filter: input("", "", false, window, cx),
            focus: cx.focus_handle(),
            subs: vec![],
            busy: 0,
            entry_conversions: 0,
            messages: vec![],
            notifications: vec![],
            next_notice_id: 0,
            last_poll: Instant::now(),
            last_auto: Instant::now(),
            log_server: String::new(),
            log_ascending: Some(true),
            log_command_width: None,
            log_scroll: ScrollHandle::new(),
            sidebar_resizing: false,
            sidebar_hover: false,
            sidebar_save_epoch: 0,
            transfer: None,
            csv_import_unit: 1,
            csv_export_expire: false,
            add_entries_reload: false,
            csv_transfers: BTreeMap::new(),
            csv_progress: mpsc::channel(),
            full_editor: false,
            groups_open: HashSet::new(),
            connection_selected: None,
            connection_pending: None,
            connection_focus: cx.focus_handle(),
            connection_scroll: ScrollHandle::new(),
            fonts: cx.text_system().all_font_names(),
            viewport_width: window.bounds().size.width.as_f32(),
            column_drag: None,
        };
        if let Err(e) = connections {
            app.toast(e.to_string(), true);
        }
        if let Err(e) = preferences {
            app.toast(e.to_string(), true);
        }
        app.fonts
            .retain(|font| !font.is_empty() && !font.starts_with('.'));
        app.fonts.sort();
        app.fonts.dedup();
        app.apply_theme(window, cx);
        #[cfg(all(target_os = "macos", not(feature = "ui-qa")))]
        app_icon::sync(window.appearance());
        app.load_language();
        app.subs
            .push(cx.on_blur(&app.connection_focus, window, |app, _, cx| {
                app.connection_pending = None;
                cx.notify();
            }));
        app.subs.push(cx.subscribe_in(
            &app.conn_filter,
            window,
            |app, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    app.update_connection_filter(cx);
                }
                cx.notify();
            },
        ));
        for state in [app.log_filter.clone()] {
            app.subs
                .push(cx.subscribe_in(&state, window, |_, _, _: &InputEvent, _, cx| cx.notify()));
        }
        app.subs
            .push(cx.observe_window_bounds(window, |app, window, _| {
                let bounds = window.bounds();
                app.viewport_width = bounds.size.width.as_f32();
                if !window.is_maximized() {
                    app.pref.behavior.window_width = bounds.size.width.as_f32();
                    app.pref.behavior.window_height = bounds.size.height.as_f32();
                }
                let bounds = window.window_bounds().get_bounds();
                app.pref.behavior.window_pos_x = bounds.origin.x.as_f32();
                app.pref.behavior.window_pos_y = bounds.origin.y.as_f32();
                app.pref.behavior.window_maximised = window.is_maximized();
                app.save_preferences();
            }));
        app.subs
            .push(cx.observe_window_appearance(window, |app, window, cx| {
                #[cfg(all(target_os = "macos", not(feature = "ui-qa")))]
                app_icon::sync(window.appearance());
                if app.pref.general.theme == "auto" {
                    app.apply_theme(window, cx);
                    cx.notify();
                }
            }));
        app.subs.push(cx.on_app_quit(|app, _| {
            app.save_preferences();
            app.tabs.clear();
            async {}
        }));
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if this
                    .update_in(cx, |app, window, cx| app.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        app
    }
    fn load_language(&mut self) {
        let code = if self.pref.general.language == "auto" {
            system_language()
        } else {
            self.pref.general.language.clone()
        };
        let code = match code.as_str() {
            "zh" => "zh-cn",
            "en" => "en-us",
            "tw" => "zh-tw",
            "ja" => "ja-jp",
            "ko" => "ko-kr",
            "fr" => "fr-fr",
            "es" => "es-es",
            "pt" => "pt-br",
            "ru" => "ru-ru",
            "tr" => "tr-tr",
            _ => &code,
        };
        self.lang = Assets::get(&format!("langs/{code}.json"))
            .and_then(|f| serde_json::from_slice(&f.data).ok())
            .unwrap_or_else(|| {
                serde_json::from_slice(&Assets::get("langs/en-us.json").unwrap().data).unwrap()
            });
        let chinese: Json =
            serde_json::from_slice(&Assets::get("langs/zh-cn.json").unwrap().data).unwrap();
        let english: Json =
            serde_json::from_slice(&Assets::get("langs/en-us.json").unwrap().data).unwrap();
        fn walk(
            chinese: &Json,
            english: &Json,
            current: &Json,
            map: &mut BTreeMap<String, String>,
        ) {
            if let Json::Object(values) = chinese {
                for (k, v) in values {
                    walk(v, &english[k], &current[k], map);
                }
            } else if let (Some(c), Some(v)) = (chinese.as_str(), current.as_str()) {
                map.insert(c.into(), v.into());
                if let Some(e) = english.as_str() {
                    map.insert(e.into(), v.into());
                }
            }
        }
        TRANSLATIONS.with(|map| {
            let mut map = map.borrow_mut();
            map.clear();
            walk(&chinese, &english, &self.lang, &mut map);
        });
    }
    fn tr(&self, key: &str) -> String {
        self.lang
            .pointer(&format!("/{}", key.replace('.', "/")))
            .and_then(Json::as_str)
            .unwrap_or(key)
            .into()
    }
    fn apply_theme(&self, window: &mut Window, cx: &mut App) {
        let mode = match self.pref.general.theme.as_str() {
            "dark" => ThemeMode::Dark,
            "light" => ThemeMode::Light,
            _ => match window.appearance() {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
                _ => ThemeMode::Light,
            },
        };
        Theme::change(mode, Some(window), cx);
        let editor_font = self.preference_font("editor", "Monaco", cx);
        Theme::update(cx, |t| {
            t.font_size = px(14.);
            t.radius = px(4.);
            t.primary = rgb(0xd33a31).into();
            t.primary_foreground = rgb(0xffffff).into();
            t.primary_hover = rgb(0xff6b6b).into();
            t.primary_active = rgb(0xd5271c).into();
            t.button_primary = rgb(0xd33a31).into();
            t.button_primary_hover = rgb(0xff6b6b).into();
            t.button_primary_active = rgb(0xd5271c).into();
            t.button_primary_foreground = rgb(0xffffff).into();
            t.ring = rgb(0xd33a31).into();
            t.foreground = rgb(if mode == ThemeMode::Dark {
                0xe6e6e6
            } else {
                0x333639
            })
            .into();
            t.muted_foreground = rgb(if mode == ThemeMode::Dark {
                0x999999
            } else {
                0x767c82
            })
            .into();
            t.background = rgb(if mode == ThemeMode::Dark {
                0x1e1e1e
            } else {
                0xffffff
            })
            .into();
            t.mono_font_size = px(self.editor_font_size());
            t.mono_font_family = editor_font.family;
            t.border = rgb(if mode == ThemeMode::Dark {
                0x515151
            } else {
                0xe0e0e6
            })
            .into();
            t.font_family = ".SystemUIFont".into();
        });
        for tab in &self.tabs { self.update_json_brackets(tab.id, cx); }
        for dialog in self.dialog.iter().chain(self.dialog_stack.iter())
            .chain(self.cached_entry_panels.values().map(|(dialog, _)| dialog))
        { dialog.update_entry_brackets(cx); }
    }
    fn dark(&self, cx: &App) -> bool {
        cx.theme().mode == ThemeMode::Dark
    }
    fn sidebar_color(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0x262626 } else { 0xf2f2f2 }).into()
    }
    fn ribbon_color(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0x2c2c2c } else { 0xf9f9f9 }).into()
    }
    fn split_color(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0x474747 } else { 0xdadada }).into()
    }
    fn save_preferences(&mut self) {
        if let Err(e) = self.store.save("preferences.yaml", &self.pref) {
            self.toast(e.to_string(), true);
        }
    }
    fn choose_welcome(&mut self, allow_tracking: bool, cx: &mut Context<Self>) {
        self.pref.welcome_choice(allow_tracking);
        self.save_preferences();
        cx.notify();
    }
    fn current_language(&self) -> String {
        if self.pref.general.language == "auto" {
            system_language()
        } else {
            self.pref.general.language.clone()
        }
    }
    fn save_connections(&mut self) {
        if let Err(e) = self.store.save("connections.yaml", &self.connections) {
            self.toast(e.to_string(), true);
        }
    }
    fn index(&self) -> Option<usize> {
        let id = self.active?;
        self.tabs.iter().position(|t| t.id == id)
    }
    fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
    fn job<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        work: impl FnOnce(&Service) -> Result<T> + Send + 'static,
        apply: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.busy += 1;
        let service = self.service.clone();
        let task = cx
            .background_executor()
            .spawn(async move { work(&service) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.busy = app.busy.saturating_sub(1);
                match result {
                    Ok(value) => apply(app, value, window, cx),
                    Err(e) => app.toast(e.to_string(), true),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn open_connection(&mut self, c: Connection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.tabs.iter().find(|t| t.conn.name == c.name).map(|t| t.id) {
            self.activate_connection_tab(Some(id), cx);
            if self.dialog.is_none() || self.editing_entry() { self.focus.focus(window, cx); }
            self.nav = Nav::Browser;
            cx.notify();
            return;
        }
        self.next_id += 1;
        let id = self.next_id;
        let status_profile = self.status_refresh_profiles.get(&c.name)
            .copied().unwrap_or(c.refresh_interval as f64);
        let status_seconds = if status_profile > 0. { status_profile } else { 5. };
        let db = if c.cluster.enable { 0 } else { c.last_db };
        let editor = editor(String::new(), "json", &self.pref, window, cx);
        let json_brackets = editor.update(cx, |editor, cx| editor.create_decorations_collection(vec![], cx));
        self.subs
            .push(cx.subscribe_in(&editor, window, move |app, _, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    app.update_json_brackets(id, cx);
                    app.update_scalar_diagnostics(id, window, cx);
                    cx.notify();
                }
            }));
        let filter = input("", self.tr("interface.filter"), false, window, cx);
        let key_name = input("", "", false, window, cx);
        self.subs.push(cx.subscribe_in(&key_name, window, |_, _, _: &InputEvent, _, cx| {
            cx.notify();
        }));
        let database_query = input("", "", false, window, cx);
        self.subs.push(cx.subscribe_in(&database_query,window,move|app,_,event,window,cx|{
            if matches!(event,InputEvent::Change){app.database_query_changed(id,window,cx);}
        }));
        let row_filter = input("", self.tr("interface.filter"), false, window, cx);
        let cli_input = input("", "", false, window, cx);
        cli_input.update(cx, |input, cx| {
            input.set_caret_shape(self.pref.cli.cursor_style.clone(), cx);
            input.set_editor_style(gpui_base::input::InputEditorStyle {
                foreground: rgb(0xffffff).into(),
                background: rgb(0x000000).into(),
                caret: rgb(0xffffff).into(),
                selection: rgba(0xffffff4d).into(),
                ..Default::default()
            });
        });
        for (field, state) in [
            ("filter", filter.clone()),
            ("row", row_filter.clone()),
            ("cli", cli_input.clone()),
        ] {
            self.subs.push(
                cx.subscribe_in(&state, window, move |app, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        match field {
                            "filter" => app.search_keys(id, window, cx),
                            "row" => app.search_entries(id, window, cx),
                            _ => app.execute_cli(id, window, cx),
                        }
                    } else if matches!(event, InputEvent::Change) {
                        if field == "filter" {
                            app.update_key_filter(id, window, cx);
                        }
                        if field == "row" {
                            app.update_entry_filter(id, window, cx);
                        }
                        if field == "cli"
                            && let Some(index) = app.tab_index(id)
                        {
                            let line = text_input(&app.tabs[index].cli_input, cx);
                            app.tabs[index].cli_history.update(&line);
                            app.tabs[index].cli_view.borrow_mut().scroll_back = 0;
                        }
                        cx.notify();
                    }
                }),
            );
        }
        let tab = Tab {
            id,
            conn: c.clone(),
            db,
            pane: Pane::Status,
            content_nav_offset: Default::default(),
            content_nav_layout: Default::default(),
            databases: vec![],
            info: Info::default(),
            samples: VecDeque::new(),
            keys: vec![],
            scan: ScanState::default(),
            scan_epoch: 0,
            keys_loading: false,
            total: 0,
            selected: IndexSet::new(),
            check_mode: false,
            key_selection: None,
            key_focus: cx.focus_handle(),
            key_scroll: UniformListScrollHandle::new(),
            key_match: String::new(),
            key_filter: String::new(),
            key_filter_epoch: 0,
            key_filter_changed: None,
            key_root_expanded: true,
            database_picker: key_selectors::DatabasePicker::new(database_query),
            type_picker: key_selectors::TypePicker::default(),
            expanded: HashSet::new(),
            flat: c.key_view == 1,
            filter,
            key_type: String::new(),
            exact: false,
            detail: None,
            key_name,
            current_key: None,
            detail_epoch: 0,
            editor,
            editor_original: String::new(),
            json_brackets,
            json_diagnostics: Default::default(),
            row_values: BTreeMap::new(),
            removed_fields: HashSet::new(),
            format_epoch: 0,
            values_loading: false,
            value_saving: false,
            value_save_epoch: 0,
            row_filter,
            row_match: String::new(),
            row_filter_value: String::new(),
            row_filter_epoch: 0,
            row_filter_changed: None,
            entry_list: ListState::new(0, ListAlignment::Top, px(500.)),
            format: "Auto".into(),
            decode: "Auto".into(),
            reverse: false,
            field_sort: None,
            score_sort: None,
            entry_delete: None,
            footer_picker: None,
            footer_hover: None,
            footer_tooltip: None,
            footer_hover_epoch: 0,
            footer_tooltip_bounds: Default::default(),
            match_tooltips: Default::default(),
            column_widths: BTreeMap::new(),
            editing: false,
            auto_refresh: false,
            refresh_seconds: 2.,
            refresh_numbers: [Some(status_seconds), Some(2.), Some(5.)],
            status_seconds,
            refresh_interval: input("2", "", false, window, cx),
            detail_loaded: Instant::now(),
            cli_input,
            cli_output: vec![
                cli::Line::text(""),
                cli::Line::text(self.tr("interface.cli_welcome")),
                cli::Line::text(""),
                cli::Line::text(""),
            ],
            cli_history: cli::History::default(),
            cli_db: 0,
            cli_scroll: ScrollHandle::new(),
            cli_view: Default::default(),
            slow: vec![],
            slow_limit: input("20", "", false, window, cx),
            slow_keyword: input("", "", false, window, cx),
            slow_client: input("", "", false, window, cx),
            slow_ascending: Some(false),
            slow_auto: false,
            slow_seconds: 5.,
            slow_interval: input("5", "", false, window, cx),
            slow_polled: Instant::now(),
            clients: vec![],
            clients_loading: false,
            client_load_started: Instant::now(),
            #[cfg(feature = "ui-qa")]
            client_load_gate: None,
            client_sort: Default::default(),
            client_trigger_bounds: Default::default(),
            client_grid_bounds: Default::default(),
            info_group: "CPU".into(),
            info_filter: input("", "", false, window, cx),
            status_auto: status_profile >= 0.,
            info_details: false,
            status_interval: input(dialogs::numbers::display(Some(status_seconds)), "", false, window, cx),
            monitor: Some(StreamState::idle()),
            pubsub: Some(StreamState::idle()),
            stream_filter: input("", "", false, window, cx),
            pub_channel: input("", self.tr("pubsub.channel"), false, window, cx),
            pub_message: input("", self.tr("pubsub.message"), false, window, cx),
            pub_pattern: input("*", "Pattern", false, window, cx),
            pub_history: vec![],
            pub_received: None,
        };
        for state in [tab.pub_channel.clone(), tab.pub_message.clone()] {
            self.subs.push(
                cx.subscribe_in(&state, window, move |app, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        app.publish(id, window, cx);
                    }
                    cx.notify();
                }),
            );
        }
        self.subs.push(cx.subscribe_in(
            &tab.stream_filter,
            window,
            |_, _, _: &InputEvent, _, cx| cx.notify(),
        ));
        self.subs.push(
            cx.subscribe_in(&tab.info_filter, window, |_, _, _: &InputEvent, _, cx| {
                cx.notify()
            }),
        );
        for (field, target) in [(&tab.status_interval, RefreshTarget::Status), (&tab.refresh_interval, RefreshTarget::Detail), (&tab.slow_interval, RefreshTarget::Slow)] {
            self.subs.push(cx.subscribe_in(field, window, move |app, _, event, window, cx| {
                app.refresh_number_event(id, target, event, window, cx);
            }));
        }
        for field in [&tab.slow_keyword, &tab.slow_client] {
            self.subs
                .push(cx.subscribe_in(field, window, |_, _, _: &InputEvent, _, cx| cx.notify()));
        }
        self.subs.push(
            cx.subscribe_in(&tab.slow_limit, window, move |app, _, event, w, cx| {
                if matches!(event, InputEvent::Change) {
                    app.load_slow(id, w, cx);
                }
                cx.notify();
            }),
        );
        self.tabs.push(tab);
        self.activate_connection_tab(Some(id), cx);
        if self.dialog.is_none() || self.editing_entry() { self.focus.focus(window, cx); }
        self.nav = Nav::Browser;
        self.job(
            window,
            cx,
            move |s| {
                s.connect(&c, db)?;
                Ok((s.databases(&c)?, s.info(&c)?))
            },
            move |app, (databases, info), window, cx| {
                if let Some(i) = app.tab_index(id) {
                    app.tabs[i].databases = databases;
                    app.tabs[i].record_info(info);
                    if app.tabs[i].status_auto {
                        app.save_status_interval(id, window, cx);
                    }
                    app.scan_keys(id, true, false, window, cx);
                }
            },
        );
    }
    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let name = self.tabs[i].conn.name.clone();
        self.remove_closed_tab(&name, cx);
        self.job(
            window,
            cx,
            move |s| {
                s.close(&name);
                Ok(())
            },
            |_, _, _, _| {},
        );
    }
    fn scan_keys(
        &mut self,
        id: u64,
        reset: bool,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        if t.keys_loading && !reset {
            return;
        }
        if reset {
            t.scan = ScanState::default();
            t.scan_epoch += 1;
            t.keys.clear();
            t.selected.clear();
        }
        t.keys_loading = true;
        let epoch = t.scan_epoch;
        let c = t.conn.clone();
        let db = t.db;
        let state = t.scan.clone();
        let input_pattern = t.key_match.clone();
        let pattern = if input_pattern.is_empty() && !t.exact {
            c.default_filter.clone()
        } else {
            input_pattern
        };
        let pattern = if !t.exact && !pattern.chars().any(|c| matches!(c, '?' | '*' | '[' | ']' | '{' | '}')) {
            format!("*{pattern}*")
        } else { pattern };
        let kind = t.key_type.clone();
        let exact = t.exact;
        let count = self.pref.general.scan_size;
        let load_size = c.load_size;
        self.job(
            window,
            cx,
            move |s| {
                Ok((|| -> Result<_> {
                    let mut page = s.scan(&c, db, &pattern, &kind, exact, state, count)?;
                    while !page.state.end && (all || page.keys.len() < load_size) {
                        let next = s.scan(&c, db, &pattern, &kind, exact, page.state.clone(), count)?;
                        page.keys.extend(next.keys);
                        page.state = next.state;
                    }
                    Ok(page)
                })())
            },
            move |app, page, _, _| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].db == db
                    && app.tabs[i].scan_epoch == epoch
                {
                    app.tabs[i].keys_loading = false;
                    let page = match page {
                        Ok(page) => page,
                        Err(error) => {
                            app.toast(error.to_string(), true);
                            return;
                        }
                    };
                    let t = &mut app.tabs[i];
                    let mut seen = t.keys.iter().map(|k| k.key.clone()).collect::<HashSet<_>>();
                    t.keys
                        .extend(page.keys.into_iter().filter(|k| seen.insert(k.key.clone())));
                    t.keys.sort_by(|a, b| {
                        natural_compare(&display_bytes(&a.key), &display_bytes(&b.key))
                    });
                    t.scan = page.state;
                    t.total = page.total;
                }
            },
        );
    }
    fn change_database(&mut self, id: u64, db: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_entry();
        if let Some(i) = self.tab_index(id) {
            let t = &mut self.tabs[i];
            t.db = db;
            t.detail = None;
            t.current_key = None;
            t.entry_delete = None;
            t.footer_picker = None;
            t.footer_hover = None;
            t.footer_tooltip = None;
            t.footer_hover_epoch += 1;
            t.key_selection = None;
            t.key_match.clear();
            t.key_filter.clear();
            t.key_type.clear();
            t.exact = false;
            t.expanded.clear();
            set_input(&t.filter, "", window, cx);
            t.detail_epoch += 1;
            t.conn.last_db = db;
            t.value_saving = false;
            t.value_save_epoch += 1;
            let name = t.conn.name.clone();
            if let Some(mut connection) = flatten(&self.connections)
                .into_iter()
                .find(|connection| connection.name == name)
                && connection.last_db != db
            {
                connection.last_db = db;
                let _ = crate::storage::save_connection(
                    &mut self.connections,
                    Some(&name),
                    connection,
                );
                self.save_connections();
            }
            self.scan_keys(id, true, false, window, cx);
        }
    }
    fn select_key(&mut self, id: u64, key: Vec<u8>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tab_index(id) {
            let t = &mut self.tabs[i];
            t.key_selection = Some(key_browser::Selection::Key(key.clone()));
            if t.current_key.as_ref() == Some(&key) {
                if t.detail_loaded.elapsed() > Duration::from_secs(1) {
                    self.reload_detail(id, true, window, cx);
                }
                cx.notify();
                return;
            }
            self.dismiss_entry();
            let t = &mut self.tabs[i];
            t.pane = Pane::Detail;
            t.current_key = Some(key);
            t.value_saving = false;
            t.value_save_epoch += 1;
            t.entry_delete = None;
            t.footer_picker = None;
            t.footer_hover = None;
            t.footer_tooltip = None;
            t.footer_hover_epoch += 1;
            t.detail = None;
            t.decode = "Auto".into();
            t.row_values.clear();
            t.removed_fields.clear();
            t.format = "Auto".into();
            t.auto_refresh = false;
            t.row_match.clear();
            t.row_filter_value.clear();
            t.row_filter_epoch += 1;
            t.row_filter_changed = None;
            t.field_sort = None;
            t.score_sort = None;
            t.column_widths.retain(|key, _| !key.starts_with("entry-"));
            set_input(&t.row_filter, "", window, cx);
            self.reload_detail(id, true, window, cx);
        }
    }
    fn reload_detail(&mut self, id: u64, reset: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        let Some(key) = t.current_key.clone() else {
            return;
        };
        if reset {
            t.removed_fields.clear();
        }
        t.detail_epoch += 1;
        let epoch = t.detail_epoch;
        let c = t.conn.clone();
        let db = t.db;
        let cursor = if reset {
            0
        } else {
            t.detail.as_ref().map(|d| d.cursor).unwrap_or(0)
        };
        let after = if reset {
            String::new()
        } else {
            t.detail
                .as_ref()
                .and_then(|d| d.rows.last())
                .map(|r| display_bytes(&r.id))
                .unwrap_or_default()
        };
        let pattern = t.row_match.clone();
        let reverse = t.reverse;
        let page = self.pref.general.scan_size;
        self.job(
            window,
            cx,
            move |s| match s.detail(&c, db, &key, cursor, &pattern, page, &after, reverse) {
                Ok(detail) => Ok(Some(detail)),
                Err(error) if error.is::<KeyNotFound>() => Ok(None),
                Err(error) => Err(error),
            },
            move |app, detail, window, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].detail_epoch == epoch
                {
                    let Some(mut detail) = detail else {
                        let t = &mut app.tabs[i];
                        t.pane = Pane::Status;
                        t.current_key = None;
                        t.detail = None;
                        t.entry_delete = None;
                        t.footer_picker = None;
                        t.footer_hover = None;
                        t.footer_tooltip = None;
                        t.footer_hover_epoch += 1;
                        t.editing = false;
                        t.auto_refresh = false;
                        t.row_values.clear();
                        t.format_epoch += 1;
                        t.values_loading = false;
                        app.toast("load key summary fail: key not exists", true);
                        return;
                    };
                    let t = &mut app.tabs[i];
                    if !reset && let Some(old) = &t.detail {
                        let mut rows = old.rows.clone();
                        let mut seen = rows.iter().map(|r| r.id.clone()).collect::<HashSet<_>>();
                        rows.extend(
                            detail
                                .rows
                                .into_iter()
                                .filter(|r| seen.insert(r.id.clone())),
                        );
                        detail.rows = rows;
                    }
                    if !matches!(detail.kind, KeyType::String | KeyType::Json) {
                        if t.decode == "Auto" {
                            t.decode = "None".into();
                        }
                        if t.format == "Auto" {
                            t.format = "Raw".into();
                        }
                    }
                    app.tabs[i].detail = Some(detail);
                    app.tabs[i].detail_loaded = Instant::now();
                    app.tabs[i].editing = false;
                    app.prepare_values(id, window, cx);
                }
            },
        );
    }
    fn cpu_job<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<T> + Send + 'static,
        apply: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.busy += 1;
        let task = cx.background_executor().spawn(async move { work() });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.busy = app.busy.saturating_sub(1);
                match result {
                    Ok(value) => apply(app, value, window, cx),
                    Err(error) => app.toast(error.to_string(), true),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn prepare_values(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        if let Some(detail)=&mut t.detail {for row in &mut detail.rows {row.display_value=None;}}
        let Some(detail) = t.detail.clone() else {
            return;
        };
        let name = display_bytes(&detail.key);
        if text_input(&t.key_name, cx) != name {
            t.key_name.set(name.into(), window, cx);
        }
        t.format_epoch += 1;
        t.values_loading = true;
        let epoch = t.format_epoch;
        let detail_epoch = t.detail_epoch;
        let decode = if detail.kind == KeyType::Json { "None".into() } else { t.decode.clone() };
        let format = if detail.kind == KeyType::Json { "JSON".into() } else { t.format.clone() };
        let custom = self.pref.decoder.clone();
        self.cpu_job(
            window,
            cx,
            move || {
                Ok((|| -> Result<_> {
                    let scalar = if matches!(detail.kind, KeyType::String | KeyType::Json) {
                        Some(codec::convert(&detail.value, &decode, &format, &custom)?)
                    } else {
                        None
                    };
                    let rows = detail
                        .rows
                        .iter()
                        .map(|row| {
                            let format =
                                if format == "Raw" && std::str::from_utf8(&row.value).is_err() {
                                    "Hex"
                                } else {
                                    &format
                                };
                            Ok((
                                row.id.clone(),
                                codec::convert(&row.value, &decode, format, &custom)?,
                            ))
                        })
                        .collect::<Result<BTreeMap<_, _>>>()?;
                    Ok((scalar, rows))
                })())
            },
            move |a, result, window, cx| {
                if let Some(i) = a.tab_index(id)
                    && a.tabs[i].format_epoch == epoch
                    && a.tabs[i].detail_epoch == detail_epoch
                {
                    a.tabs[i].values_loading = false;
                    let (value, rows) = match result {
                        Ok(value) => value,
                        Err(error) => {
                            a.toast(error.to_string(), true);
                            return;
                        }
                    };
                    let t = &mut a.tabs[i];
                    t.row_values = rows;
                    t.entry_list
                        .splice(0..t.entry_list.item_count(), t.entry_list.item_count());
                    if let Some(value) = value {
                        t.decode = value.decode;
                        t.format = value.format;
                        t.editor_original = value.text.clone();
                        let offset = t.editor.read(cx).scroll_offset();
                        t.editor.update(cx, |editor, cx| {
                            editor.set_highlighter(editor_language(&t.format), cx);
                            editor.set_value(value.text, window, cx);
                            editor.set_scroll_offset(offset, cx);
                        });
                    }
                    a.update_json_brackets(id, cx);
                    a.update_scalar_diagnostics(id, window, cx);
                }
            },
        );
    }
    fn search_entries(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.tab_index(id) {
            let t = &mut self.tabs[i];
            let value = text_input(&t.row_filter, cx).trim().to_string();
            if value.is_empty() {
                set_input(&t.row_filter, "", window, cx);
                return;
            }
            t.row_match = value;
            t.row_filter_value.clear();
            t.row_filter_epoch += 1;
            set_input(&t.row_filter, "", window, cx);
            self.reload_detail(id, true, window, cx);
        }
    }
    fn load_all_entries(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        let Some(key) = t.current_key.clone() else {
            return;
        };
        let c = t.conn.clone();
        let db = t.db;
        let pattern = t.row_match.clone();
        let reverse = t.reverse;
        let page = self.pref.general.scan_size;
        t.detail_epoch += 1;
        let epoch = t.detail_epoch;
        self.job(
            window,
            cx,
            move |s| {
                let mut d = s.detail(&c, db, &key, 0, &pattern, page, "", reverse)?;
                let mut seen = d.rows.iter().map(|r| r.id.clone()).collect::<HashSet<_>>();
                while !d.end {
                    let after = d
                        .rows
                        .last()
                        .map(|r| display_bytes(&r.id))
                        .unwrap_or_default();
                    let next = s.detail(&c, db, &key, d.cursor, &pattern, page, &after, reverse)?;
                    d.rows
                        .extend(next.rows.into_iter().filter(|r| seen.insert(r.id.clone())));
                    d.cursor = next.cursor;
                    d.end = next.end;
                }
                Ok(d)
            },
            move |a, d, w, cx| {
                if let Some(i) = a.tab_index(id)
                    && a.tabs[i].detail_epoch == epoch
                {
                    a.tabs[i].detail = Some(d);
                    a.prepare_values(id, w, cx);
                }
            },
        );
    }
    fn cli_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_key(event, window, cx) || self.dialog.is_some() {
            return;
        }
        let Some(i) = self.index() else {
            return;
        };
        let t = &mut self.tabs[i];
        if t.pane != Pane::Cli || !t.cli_input.focus_handle(cx).is_focused(window) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform && key == "c" {
            if let Some(text) = t.cli_view.borrow().selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            cx.stop_propagation();
            return;
        }
        let control = event.keystroke.modifiers.control;
        let input = text_input(&t.cli_input, cx);
        if (!control && matches!(key, "up" | "down" | "pageup" | "pagedown"))
            || (control && matches!(key, "p" | "n"))
        {
            let previous = matches!(key, "up" | "pageup" | "p");
            let line = t.cli_history.browse(previous, &input);
            set_input(&t.cli_input, line, window, cx);
        } else if control && matches!(key, "l" | "c") {
            if key == "l" {
                t.cli_output.clear();
            } else {
                t.cli_output.push(cli::Line::prompt(
                    &format!("{}:db{}> ", t.conn.name, t.cli_db),
                    &input,
                ));
            }
            t.cli_history.finish(&input);
            set_input(&t.cli_input, "", window, cx);
            t.cli_scroll.scroll_to_bottom();
        } else {
            use cli::Edit;
            let action = match (control, key) {
                (true, "a") | (false, "home") => Some(Edit::Home),
                (true, "e") | (false, "end") => Some(Edit::End),
                (true, "b") | (false, "left") => Some(Edit::Left),
                (true, "f") | (false, "right") => Some(Edit::Right),
                (true, "d") | (false, "delete") => Some(Edit::Delete),
                (true, "h") | (false, "backspace") => Some(Edit::Backspace),
                (true, "u") => Some(Edit::KillBefore),
                (true, "k") => Some(Edit::KillAfter),
                (true, "w") => Some(Edit::KillWord),
                _ => None,
            };
            if let Some(action) = action {
                let cursor = t.cli_input.read(cx).cursor();
                let (line, cursor) = cli::edit(&input, cursor, action);
                t.cli_history.update(&line);
                t.cli_input.update(cx, |state, cx| {
                    state.set_value(line, window, cx);
                    state.set_selected_range(cursor..cursor, cx);
                });
            } else if !control && key != "tab" {
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn save_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.index() else { return };
        let tab = &self.tabs[i];
        if tab.pane == Pane::Detail && !tab.values_loading && !tab.value_saving
            && tab.detail.as_ref().is_some_and(|detail| matches!(detail.kind, KeyType::String | KeyType::Json)) {
            self.save_scalar_value(i, window, cx);
        }
    }
    fn refresh_info(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let c = self.tabs[i].conn.clone();
        self.job(
            window,
            cx,
            move |s| s.info(&c),
            move |app, info, _, _| {
                if let Some(i) = app.tab_index(id) {
                    app.tabs[i].record_info(info);
                }
            },
        );
    }
    fn switch_pane(&mut self, id: u64, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        self.close_footer_picker(id, window, cx);
        if pane != Pane::Detail {
            self.dismiss_entry();
        }
        let Some(i) = self.tab_index(id) else {
            return;
        };
        self.tabs[i].pane = pane;
        self.tabs[i].footer_hover = None;
        self.tabs[i].footer_tooltip = None;
        self.tabs[i].footer_hover_epoch += 1;
        match pane {
            Pane::Status => self.refresh_info(id, window, cx),
            Pane::Slow => self.load_slow(id, window, cx),
            Pane::Cli => self.tabs[i].cli_input.focus_handle(cx).focus(window, cx),
            _ => {}
        }
        cx.notify();
    }
    fn load_slow(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let c = self.tabs[i].conn.clone();
        let count = text_input(&self.tabs[i].slow_limit, cx)
            .parse::<usize>()
            .unwrap_or(20)
            .clamp(1, 9999);
        self.tabs[i].slow_polled = Instant::now();
        self.job(
            window,
            cx,
            move |s| s.slow_logs(&c, count),
            move |app, logs, _, _| {
                if let Some(i) = app.tab_index(id) {
                    app.tabs[i].slow = logs;
                }
            },
        );
    }
    fn refresh_hash_field(
        &mut self,
        id: u64,
        field: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &self.tabs[i];
        let Some(key) = t.current_key.clone() else {
            return;
        };
        let (c, db, epoch) = (t.conn.clone(), t.db, t.detail_epoch);
        let queried = field.clone();
        self.job(
            window,
            cx,
            move |s| s.run(&c, db, vec![b"HGET".to_vec(), key, queried]),
            move |a, value, w, cx| {
                let Some(i) = a.tab_index(id) else {
                    return;
                };
                if a.tabs[i].detail_epoch != epoch {
                    return;
                }
                if let Some(detail) = &mut a.tabs[i].detail {
                    if value == crate::redis::Value::Null {
                        a.tabs[i].removed_fields.insert(field.clone());
                    } else if let Some(row) = detail.rows.iter_mut().find(|row| row.id == field) {
                        row.value = value.bytes();
                        a.tabs[i].removed_fields.remove(&field);
                    }
                }
                a.prepare_values(id, w, cx);
            },
        );
    }
    fn execute_cli(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        let line = text_input(&t.cli_input, cx);
        t.cli_history.finish(&line);
        t.cli_view.borrow_mut().scroll_back = 0;
        set_input(&t.cli_input, "", window, cx);
        if matches!(line.as_str(), "clear" | "clr") {
            t.cli_output.clear();
            cx.notify();
            return;
        }
        t.cli_output.push(cli::Line::prompt(
            &format!("{}:db{}> ", t.conn.name, t.cli_db),
            &line,
        ));
        t.cli_scroll.scroll_to_bottom();
        let args = cli::parse(&line);
        if args.is_empty() {
            t.cli_output.push(cli::Line::text(""));
            cx.notify();
            return;
        }
        let c = t.conn.clone();
        let db = t.cli_db;
        let selected_db = if args
            .first()
            .is_some_and(|arg| arg.eq_ignore_ascii_case(b"SELECT"))
        {
            args.get(1)
                .and_then(|n| std::str::from_utf8(n).ok())
                .and_then(|n| n.parse::<u32>().ok())
        } else {
            None
        };
        self.job(
            window,
            cx,
            move |service| {
                Ok(match service.run_cli(&c, db, args) {
                    Ok(value) => (value, selected_db),
                    Err(error) => (crate::redis::Value::Error(error.to_string()), None),
                })
            },
            move |app, (value, selected_db), _, _| {
                if let Some(i) = app.tab_index(id) {
                    if let Some(db) = selected_db {
                        app.tabs[i].cli_db = db;
                    }
                    app.tabs[i].cli_output.extend(cli::Line::reply(&value));
                    app.tabs[i].cli_scroll.scroll_to_bottom();
                }
            },
        );
    }
    fn stream_toggle(
        &mut self,
        id: u64,
        pubsub: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &mut self.tabs[i];
        let current = if pubsub {
            &mut t.pubsub
        } else {
            &mut t.monitor
        };
        if let Some(stream) = current
            && stream.running
        {
            stream.cancel.cancel();
            stream.running = false;
            cx.notify();
            return;
        }
        let mut c = t.conn.clone();
        c.exec_timeout = 1;
        let db = t.db;
        let pattern = text_input(&t.pub_pattern, cx);
        let cancel = Cancellation::default();
        let token = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1024);
        let (lines, auto_last, list) = current.take().map_or_else(
            || (VecDeque::new(), true, ListState::new(0, ListAlignment::Top, px(500.))),
            |mut state| (std::mem::take(&mut state.lines), state.auto_last, state.list.clone()),
        );
        *current = Some(StreamState {
            cancel,
            receiver,
            lines,
            running: true,
            auto_last,
            list,
        });
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let client = Client::connect(&c, db)?;
                let mut wire = client.dedicated()?;
                if pubsub {
                    wire.send(&command(&[
                        "PSUBSCRIBE",
                        if pattern.is_empty() { "*" } else { &pattern },
                    ]))?;
                } else {
                    wire.query(&command(&["MONITOR"]))?.checked()?;
                }
                while !token.canceled() {
                    match wire.read() {
                        Ok(v) => {
                            let time = chrono::Local::now()
                                .format("%Y-%m-%d %H:%M:%S%.3f")
                                .to_string();
                            let line = if pubsub {
                                let values = v.array()?;
                                let Some(kind) = values.first() else {
                                    continue;
                                };
                                let (channel, message) = match kind.bytes().as_slice() {
                                    b"pmessage" if values.len() >= 4 => (&values[2], &values[3]),
                                    b"message" if values.len() >= 3 => (&values[1], &values[2]),
                                    _ => continue,
                                };
                                StreamLine {
                                    time,
                                    channel: Some(display_bytes(&channel.bytes())),
                                    text: display_bytes(&message.bytes()),
                                }
                            } else {
                                StreamLine {
                                    time,
                                    channel: None,
                                    text: v.text(),
                                }
                            };
                            if sender.send(StreamEvent::Line(line)).is_err() {
                                break;
                            }
                        }
                        Err(e) if crate::redis::is_timeout(&e) => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                let _ = sender.send(StreamEvent::Error(e.to_string()));
            }
            let _ = sender.send(StreamEvent::Stopped);
        });
        cx.notify();
    }
    fn publish(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &self.tabs[i];
        let channel = text_input(&t.pub_channel, cx);
        if channel.is_empty() {
            return;
        }
        let message = text_input(&t.pub_message, cx);
        let c = t.conn.clone();
        let db = t.db;
        let sent_channel = channel.clone();
        self.job(
            window,
            cx,
            move |s| s.run(&c, db, command(&["PUBLISH", &sent_channel, &message])),
            move |app, value, window, cx| {
                if let Some(i) = app.tab_index(id) {
                    let t = &mut app.tabs[i];
                    t.pub_received = Some((value.integer().unwrap_or(0), Instant::now()));
                    if !t.pub_history.contains(&channel) {
                        t.pub_history.push(channel);
                    }
                    set_input(&t.pub_message, "", window, cx);
                }
            },
        );
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut errors = vec![];
        for tab in &mut self.tabs {
            for stream in [&mut tab.monitor, &mut tab.pubsub].into_iter().flatten() {
                for _ in 0..500 {
                    match stream.receiver.try_recv() {
                        Ok(StreamEvent::Line(line)) => {
                            stream.lines.push_back(line);
                            changed = true;
                        }
                        Ok(StreamEvent::Error(error)) => {
                            errors.push(error);
                            stream.running = false;
                            changed = true;
                        }
                        Ok(StreamEvent::Stopped) => {
                            stream.running = false;
                            changed = true;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            if stream.running {
                                stream.running = false;
                                changed = true;
                            }
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                    }
                }
            }
            if tab
                .pub_received
                .as_ref()
                .is_some_and(|(_, time)| time.elapsed() > Duration::from_secs(2))
            {
                tab.pub_received = None;
                changed = true;
            }
        }
        for error in errors {
            self.toast(error, true);
        }
        if self.expire_messages() {
            changed = true;
        }
        changed |= self.poll_csv_transfers();
        if let Some(i) = self.index() {
            if self.last_poll.elapsed()
                > Duration::from_secs_f64(self.tabs[i].status_seconds.max(1.))
                && self.busy == 0
                && self.nav == Nav::Browser
                && self.tabs[i].pane == Pane::Status
                && self.tabs[i].status_auto
            {
                self.last_poll = Instant::now();
                self.refresh_info(self.tabs[i].id, window, cx);
            }
            if self.last_auto.elapsed() > Duration::from_secs_f64(self.tabs[i].refresh_seconds.max(1.))
                && self.busy == 0
                && self.tabs[i].auto_refresh
                && self.tabs[i].pane == Pane::Detail
            {
                self.last_auto = Instant::now();
                self.reload_detail(self.tabs[i].id, true, window, cx);
            }
            if self.tabs[i].slow_auto
                && self.tabs[i].pane == Pane::Slow
                && self.tabs[i].slow_polled.elapsed()
                    > Duration::from_secs_f64(self.tabs[i].slow_seconds.max(1.))
                && self.busy == 0
                && self.nav == Nav::Browser
            {
                self.load_slow(self.tabs[i].id, window, cx);
            }
        }
        if self.transfer.is_some() {
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
}
impl Render for RdmApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_app(window, cx)
    }
}
