//! Offscreen rendering of this application's own views with disposable data.
//! This renders GPUI scenes; it never captures the user's desktop.
use super::*;
use std::rc::Rc;

#[cfg(target_os = "macos")]
fn verify_macos_font_advances(window: &Window) {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/font-layout-macos-source.json"
    ))
    .expect("read original macOS WKWebView font widths");
    let samples = fixture["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 231);
    for sample in samples {
        let family = sample["family"].as_str().unwrap();
        let text = sample["text"].as_str().unwrap();
        let font_size = sample["fontSize"].as_f64().unwrap() as f32;
        let expected = sample["width"].as_f64().unwrap() as f32;
        let shaped = window.text_system().shape_line(
            text.to_owned().into(),
            px(font_size),
            &[TextRun {
                len: text.len(),
                font: gpui::font(family),
                ..Default::default()
            }],
            None,
        );
        let actual = f32::from(shaped.width());
        assert!(
            (actual - expected).abs() < 0.02,
            "macOS WKWebView font advance {family} {font_size}px {text:?}: {actual} != {expected}"
        );
    }
    println!("MACOS FONT QA PASS: 231 original WKWebView/Monaco advance widths; three families, seven sizes, native CoreText tracking retained");
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/font-weight-macos-source.json"
    )).expect("read original macOS WKWebView CSS font weights");
    let samples = fixture["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 252);
    for sample in samples {
        let text = sample["text"].as_str().unwrap();
        let size = sample["fontSize"].as_f64().unwrap() as f32;
        let weight = sample["weight"].as_f64().unwrap() as f32;
        let expected = sample["width"].as_f64().unwrap() as f32;
        let mut font = gpui::font(".SystemUIFont");
        font.weight = FontWeight(weight);
        let shaped = window.text_system().shape_line(text.to_owned().into(), px(size),
            &[TextRun { len: text.len(), font, ..Default::default() }], None);
        let actual = f32::from(shaped.width());
        assert!((actual - expected).abs() < 0.02,
            "macOS WKWebView CSS weight {weight}, {size}px {text:?}: {actual} != {expected}");
    }
    let mixed = fixture["mixedSamples"].as_array().unwrap();
    assert_eq!(mixed.len(), 14);
    for sample in mixed {
        let size = sample["fontSize"].as_f64().unwrap() as f32;
        let expected = sample["width"].as_f64().unwrap() as f32;
        let mut text = String::new();
        let runs: Vec<_> = sample["runs"].as_array().unwrap().iter().map(|run| {
            let segment = run["text"].as_str().unwrap();
            text.push_str(segment);
            let mut font = gpui::font(".SystemUIFont");
            font.weight = FontWeight(run["weight"].as_f64().unwrap() as f32);
            TextRun { len: segment.len(), font, ..Default::default() }
        }).collect();
        let shaped = window.text_system().shape_line(text.clone().into(), px(size), &runs, None);
        let actual = f32::from(shaped.width());
        assert!((actual - expected).abs() < 0.02,
            "macOS WKWebView mixed CSS weight, {size}px {text:?}: {actual} != {expected}");
    }
    println!("MACOS CSS WEIGHT QA PASS: 252 single-font +14 mixed-weight original body-CSS advances; seven sizes, nine weights, ASCII/Chinese/Emoji; native instances and optical tracking retained");
}

pub(super) fn copy_result(original: Option<ClipboardItem>, cx: &mut App) -> Option<String> {
    let result=cx.read_from_clipboard().and_then(|item|item.text());
    let original=original.unwrap_or_else(||ClipboardItem::new_string(String::new()));
    let expected=original.text();
    cx.write_to_clipboard(original);
    assert!(cx.read_from_clipboard().and_then(|item|item.text())==expected,"QA must restore the previous clipboard text before checking the copied fixture");
    result
}

pub(super) fn run(data_dir: PathBuf, output_dir: PathBuf) {
    assert_eq!(
        std::fs::read_to_string(data_dir.join("RDM-QA-FIXTURE"))
            .expect("run seed_demo in a disposable Redis directory before rendering"),
        "rust-rdm-render-fixture-v1\n"
    );
    let configs = Store::new(&data_dir)
        .connections()
        .expect("load disposable QA connections");
    assert_eq!(configs.len(), 1, "QA requires one disposable connection");
    assert_eq!(configs[0].network, "unix");
    assert_eq!(PathBuf::from(&configs[0].sock), data_dir.join("redis.sock"));
    std::fs::create_dir_all(&output_dir).expect("create QA output directory");
    gpui_platform::headless()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            initialize_actions(cx);
            #[cfg(target_os = "macos")]
            super::file_dialogs::verify_native_panels(&output_dir);
            let view_slot = Rc::new(RefCell::new(None));
            let slot = view_slot.clone();
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(0.), px(0.)),
                            size: size(px(1024.), px(768.)),
                        })),
                        show: false,
                        focus: false,
                        titlebar: Some(TitlebarOptions {
                            title: Some("Tiny RDM".into()),
                            appears_transparent: true,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view = cx.new(|cx| {
                            let mut app = RdmApp::new(Store::new(data_dir), window, cx);
                            app.pref.general.theme = "light".into();
                            app.pref.editor.entry_text_align = 0;
                            app.apply_theme(window, cx);
                            app
                        });
                        *slot.borrow_mut() = Some(view.clone());
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("open offscreen QA window");
            let view = view_slot.borrow().clone().unwrap();
            #[cfg(target_os = "macos")]
            window.update(cx, |_, window, _| verify_macos_font_advances(window)).unwrap();
            cx.spawn(async move |cx| {
                let mut _copied_text=None;
                let qa_connections = configs.clone();
                let qa_decoders = window.update(cx, |_, _, cx| view.read(cx).pref.decoder.clone()).unwrap();
                let snapshots = [
                    "connection-tabs-light",
                    "connection-tabs-dark",
                    "connection-tabs-general18",
                    "connection-tabs-dark18",
                    "connection-tabs-two-left",
                    "connection-tabs-two-right",
                    "connection-tabs-narrow",
                    "connection-tabs-mark-light",
                    "connection-tabs-mark-dark",
                    "connection-tabs-retained-draft",
                    "tab-close-warning",
                    "tab-close-warning-dark",
                    "tab-close-behavior",
                    "connections",
                    "connections-selected",
                    "connections-groups",
                    "connections-group-open",
                    "connections-filtered",
                    "connections-filter-group",
                    "connections-connected",
                    "connections-empty",
                    "connections-empty-group",
                    "connections-copy",
                    "connections-groups-dark",
                    "connections-keyboard",
                    "connections-keyboard-filtered",
                    "connections-menu",
                    "connections-menu-hover",
                    "connections-menu-connected",
                    "connections-menu-group",
                    "connections-menu-dark",
                    "connections-more",
                    "settings-menu",
                    "community",
                    "message-success",
                    "message-error",
                    "message-error-modal",
                    "message-error-dark",
                    "message-stack",
                    "notification-error",
                    "notification-error-dark",
                    "welcome",
                    "sponsor",
                    "csv-import-default",
                    "new-key-string",
                    "new-key-hash",
                    "new-key-list",
                    "new-key-set",
                    "new-key-zset",
                    "new-key-stream",
                    "new-key-json",
                    "new-key-required",
                    "new-key-type-menu",
                    "new-key-db-filter",
                    "new-key-db-empty",
                    "new-key-hash-rows",
                    "new-key-zset-rows",
                    "new-key-stream-value",
                    "new-key-dark-string",
                    "new-key-dark-hash",
                    "new-key-list-scroll",
                    "new-key-value-resized",
                    "new-key-value-minimum",
                    "add-entries-hash",
                    "add-entries-list",
                    "add-entries-set",
                    "add-entries-zset",
                    "add-entries-stream",
                    "add-entries-hash-rows",
                    "add-entries-hash-ignore",
                    "add-entries-reopened",
                    "add-entries-list-prepend",
                    "add-entries-zset-rows",
                    "add-entries-stream-rows",
                    "add-entries-dark-hash",
                    "add-entries-dark-zset",
                    "add-entries-binary-key",
                    "add-entries-list-scroll",
                    "add-entries-saving",
                    "csv-import-ignore",
                    "csv-import-custom",
                    "csv-import-custom-minutes",
                    "csv-import-reopened",
                    "csv-import-custom-reopened",
                    "csv-unit-menu",
                    "csv-unit-hover",
                    "csv-unit-keyboard",
                    "csv-unit-selected",
                    "csv-unit-reopened",
                    "csv-unit-day",
                    "csv-dark-import",
                    "csv-dark-custom",
                    "csv-dark-unit",
                    "csv-dark-unit-hover",
                    "csv-dark-export-empty",
                    "csv-dark-export",
                    "csv-dark-export-ready",
                    "csv-list-start",
                    "csv-list-hover",
                    "csv-list-drag",
                    "csv-list-wheel",
                    "csv-list-middle",
                    "csv-list-selection",
                    "csv-list-end",
                    "csv-list-return",
                    "csv-list-dark-middle",
                    "csv-export-empty",
                    "csv-export-default",
                    "csv-export-expire",
                    "csv-export-reopened",
                    "csv-export-select-plain",
                    "csv-export-select-partial",
                    "csv-export-select-lines",
                    "csv-export-select-reverse",
                    "csv-export-select-word",
                    "csv-export-select-binary",
                    "csv-progress-initial",
                    "csv-export-progress-loading",
                    "csv-import-progress-loading",
                    "csv-export-progress-success",
                    "csv-export-progress-warning",
                    "csv-export-progress-error",
                    "csv-export-progress-cancel",
                    "csv-export-progress-failure",
                    "csv-import-progress-success",
                    "csv-import-progress-no-reload",
                    "csv-import-progress-cancel",
                    "csv-import-progress-failure",
                    "new-connection",
                    "connection-advanced",
                    "connection-alias",
                    "connection-ssl",
                    "connection-ssl-enabled",
                    "connection-ssh",
                    "connection-ssh-enabled",
                    "connection-ssh-pkfile",
                    "connection-ssh-agent",
                    "connection-sentinel",
                    "connection-sentinel-enabled",
                    "connection-cluster",
                    "connection-proxy",
                    "connection-proxy-custom",
                    "connection-proxy-auth",
                    "connection-alias-row",
                    "connection-advanced-tags",
                    "connection-test-success",
                    "connection-test-failure",
                    "connection-validation-general",
                    "connection-validation-name",
                    "connection-validation-advanced",
                    "connection-validation-fixed",
                    "connection-validation-stages",
                    "connection-validation-save",
                    "connection-numbers",
                    "connection-edit-incomplete",
                    "connection-edit-incomplete-ssh",
                    "preferences",
                    "font-size-preview",
                    "font-size-cleared",
                    "preferences-editor",
                    "preferences-cli",
                    "preferences-decoder",
                    "preferences-font-filter",
                    "preferences-font-keyboard",
                    "preferences-font-tags",
                    "preferences-theme-preview",
                    "preferences-language-preview",
                    "key-browser",
                    "key-browser-folder-selected",
                    "key-browser-folder-open",
                    "key-browser-key-selected",
                    "key-browser-filtered",
                    "key-browser-check-mode",
                    "key-browser-search",
                    "key-browser-menu-folder",
                    "key-browser-menu-key",
                    "key-browser-more",
                    "key-browser-delete",
                    "key-filter-base-14",
                    "key-filter-base-20",
                    "key-filter-dark-14",
                    "key-filter-dark-20",
                    "key-filter-query-14",
                    "key-filter-query-20",
                    "key-filter-query-dark-14",
                    "key-filter-query-dark-20",
                    "key-filter-match-14",
                    "key-filter-match-20",
                    "key-filter-clear-14",
                    "key-filter-clear-20",
                    "key-filter-match-close-14",
                    "key-filter-match-close-20",
                    "key-filter-match-edit-14",
                    "key-filter-match-edit-20",
                    "key-filter-exact-14",
                    "key-filter-exact-20",
                    "key-filter-exact-dark-14",
                    "key-filter-exact-dark-20",
                    "key-filter-menlo-14",
                    "key-filter-menlo-20",
                    "tag-style-light-14",
                    "tag-style-light-20",
                    "tag-style-dark-14",
                    "tag-style-dark-20",
                    "tag-style-draft-light",
                    "tag-style-draft-dark",
                    "tag-overflow-light-ascii-14",
                    "tag-overflow-light-unicode-14",
                    "tag-overflow-light-ascii-20",
                    "tag-overflow-dark-ascii-14",
                    "tag-overflow-dark-unicode-14",
                    "tag-overflow-dark-ascii-20",
                    "sidebar-resize-light-200",
                    "sidebar-resize-light-400",
                    "sidebar-resize-light-700",
                    "sidebar-resize-light-800",
                    "sidebar-resize-dark-200",
                    "sidebar-resize-dark-400",
                    "sidebar-resize-dark-700",
                    "sidebar-resize-dark-800",
                    "sidebar-contract-light-hover",
                    "sidebar-contract-light-pressed",
                    "sidebar-contract-light-dragging",
                    "sidebar-contract-light-overflow",
                    "sidebar-contract-dark-hover",
                    "sidebar-contract-dark-pressed",
                    "sidebar-contract-dark-dragging",
                    "sidebar-contract-dark-overflow",
                    "selector-font-light-0",
                    "selector-font-light-1",
                    "selector-font-light-2",
                    "selector-font-light-3",
                    "selector-font-light-4",
                    "selector-font-light-5",
                    "selector-font-light-6",
                    "selector-font-light-7",
                    "selector-font-dark-0",
                    "selector-font-dark-1",
                    "selector-font-dark-2",
                    "selector-font-dark-3",
                    "selector-font-dark-4",
                    "selector-font-dark-5",
                    "selector-font-dark-6",
                    "selector-font-dark-7",
                    "general-fallback-0",
                    "general-fallback-1",
                    "general-fallback-2",
                    "general-fallback-3",
                    "css-label-light-helvetica",
                    "css-label-dark-helvetica",
                    "css-label-light-courier",
                    "css-label-dark-courier",
                    "css-label-light-unicode",
                    "css-label-dark-unicode",
                    "css-label-light-times-unicode",
                    "css-label-dark-times-unicode",
                    "key-type-menu",
                    "key-type-hash",
                    "key-type-menu-dark",
                    "key-database-menu",
                    "key-database-filtered",
                    "key-database-empty",
                    "key-database-selected",
                    "key-database-keyboard",
                    "key-database-alias-empty",
                    "key-database-alias",
                    "status",
                    "status-refresh-settings",
                    "client-list",
                    "status-info",
                    "string",
                    "ttl",
                    "hash",
                    "hash-save-check",
                    "hash-edit",
                    "edit-entry-hash",
                    "edit-entry-list",
                    "edit-entry-set",
                    "edit-entry-zset",
                    "edit-entry-hash-pinned",
                    "edit-entry-hash-draft",
                    "edit-entry-hash-full",
                    "edit-entry-hash-full-dark",
                    "edit-entry-hash-dark",
                    "edit-entry-hash-field-focus",
                    "edit-entry-list-field-focus",
                    "edit-entry-hash-dark-default",
                    "edit-entry-hash-dark-field-focus",
                    "entry-picker-format",
                    "entry-picker-decode",
                    "entry-picker-format-dark",
                    "entry-picker-decode-dark",
                    "entry-picker-format-hover",
                    "entry-picker-format-full",
                    "entry-picker-custom",
                    "entry-picker-long",
                    "entry-picker-long-menu",
                    "entry-picker-format-tooltip",
                    "entry-picker-decode-tooltip",
                    "entry-picker-format-tooltip-dark",
                    "entry-preferences",
                    "entry-preferences-return",
                    "entry-preferences-full",
                    "entry-preferences-dark",
                    "entry-preferences-saved",
                    "entry-conversion-pending",
                    "entry-conversion-followup",
                    "entry-conversion-initial",
                    "entry-conversion-empty-pending",
                    "entry-conversion-empty-followup",
                    "entry-conversion-empty-ready",
                    "entry-diagnostic-syntax-light",
                    "entry-diagnostic-syntax-dark",
                    "entry-diagnostic-hover",
                    "entry-diagnostic-hover-dark",
                    "entry-diagnostic-hover-overlap",
                    "entry-diagnostic-hover-overlap-dark",
                    "entry-diagnostic-hover-full",
                    "entry-diagnostic-hover-short",
                    "entry-diagnostic-hover-duplicate",
                    "entry-diagnostic-hover-overlap-full",
                    "entry-diagnostic-navigation",
                    "entry-diagnostic-navigation-dark",
                    "entry-diagnostic-select-partial",
                    "entry-diagnostic-select-whole",
                    "entry-diagnostic-select-code",
                    "entry-diagnostic-select-wrapped",
                    "entry-diagnostic-select-overlap",
                    "entry-diagnostic-select-reverse",
                    "entry-diagnostic-select-word",
                    "entry-diagnostic-select-navigation",
                    "entry-diagnostic-select-unicode",
                    "entry-diagnostic-hover-scroll-top",
                    "entry-diagnostic-hover-scroll-middle",
                    "entry-diagnostic-hover-scroll-bottom",
                    "entry-diagnostic-hover-scroll-reopen",
                    "entry-diagnostic-save-hover",
                    "entry-diagnostic-save-navigation",
                    "entry-diagnostic-keyboard-hover",
                    "entry-diagnostic-keyboard-navigation",
                    "entry-diagnostic-keyboard-lines",
                    "entry-diagnostic-keyboard-show-hover",
                    "entry-diagnostic-keyboard-footer-character",
                    "entry-diagnostic-keyboard-footer-word",
                    "entry-diagnostic-keyboard-footer-boundaries",
                    "entry-diagnostic-keyboard-crossmessage",
                    "entry-diagnostic-save-footer",
                    "entry-diagnostic-editor",
                    "entry-diagnostic-editor-dark",
                    "entry-diagnostic-warning",
                    "entry-diagnostic-unicode",
                    "entry-diagnostic-multiline",
                    "entry-diagnostic-wrapped",
                    "list",
                    "set",
                    "zset",
                    "stream",
                    "entries-hash",
                    "entries-list",
                    "entries-set",
                    "entries-zset",
                    "entries-stream",
                    "entries-hash-dark",
                    "entries-hash-left",
                    "entries-hash-field-filter",
                    "entries-hash-value-filter",
                    "entries-list-index-filter",
                    "entries-list-value-filter",
                    "entries-hash-sort",
                    "entries-zset-sort",
                    "entries-hash-resize",
                    "entries-hash-font20",
                    "entries-list-font20",
                    "entries-set-font20",
                    "entries-zset-font20",
                    "entries-stream-font20",
                    "entries-hash-dark-font20",
                    "entries-hash-narrow-font14",
                    "entries-hash-narrow-font20",
                    "entries-hash-menlo-font14",
                    "entries-hash-menlo-font20",
                    "entries-hash-delete",
                    "entries-hash-delete-dark",
                    "footer-hash",
                    "footer-format-menu",
                    "footer-decode-menu",
                    "footer-hash-dark",
                    "footer-format-menu-dark",
                    "footer-decode-menu-dark",
                    "footer-memory-zero",
                    "footer-stream",
                    "footer-format-tooltip",
                    "footer-decode-tooltip",
                    "footer-format-tooltip-dark",
                    "footer-decode-custom",
                    "footer-decode-long",
                    "footer-decode-long-menu",
                    "footer-hash20",
                    "footer-decode-long20",
                    "footer-decode-long-menu20",
                    "content-nav-status",
                    "content-nav-key_detail",
                    "content-nav-cli",
                    "content-nav-slow_log",
                    "content-nav-cmd_monitor",
                    "content-nav-pub_message",
                    "content-nav-dark",
                    "content-nav-large",
                    "key-toolbar-binary",
                    "key-toolbar-ttl",
                    "key-toolbar-ttl-days",
                    "key-toolbar-ttl-zero",
                    "content-nav-narrow",
                    "content-nav-narrow-pubsub",
                    "key-input-focused",
                    "key-input-selection",
                    "key-input-dark",
                    "key-input-selection-dark",
                    "refresh-detail",
                    "refresh-detail-on",
                    "refresh-detail-dark",
                    "refresh-detail-on-dark",
                    "json-syntax-light",
                    "json-syntax-dark",
                    "json-object",
                    "json-dark",
                    "json-memory-zero",
                    "json-scalar",
                    "json-draft",
                    "json-saved",
                    "json-save-error",
                    "json-saving",
                    "scalar-string-plain",
                    "scalar-string-dark",
                    "scalar-string-memory-zero",
                    "scalar-string-json",
                    "scalar-string-draft",
                    "scalar-string-saved",
                    "scalar-string-save-error",
                    "scalar-string-saving",
                    "slow",
                    "cli",
                    "cli-keyboard",
                    "cli-results",
                    "cli-ansi",
                    "cli-long",
                    "cli-selection",
                    "cli-word-selection",
                    "cli-font-size",
                    "monitor",
                    "pubsub",
                    "dark",
                    "match-tooltip-key-light-ascii",
                    "match-tooltip-key-light-unicode",
                    "match-tooltip-key-dark-ascii",
                    "match-tooltip-key-dark-unicode",
                    "match-tooltip-entry-light-ascii",
                    "match-tooltip-entry-light-unicode",
                    "match-tooltip-entry-dark-ascii",
                    "match-tooltip-entry-dark-unicode",
                    "entry-match-width-system-1024-x",
                    "entry-match-width-system-1024-name",
                    "entry-match-width-system-1024-long",
                    "entry-match-width-system-1340-x",
                    "entry-match-width-system-1340-name",
                    "entry-match-width-system-1340-long",
                    "entry-match-width-courier-1024-x",
                    "entry-match-width-courier-1024-name",
                    "entry-match-width-courier-1024-long",
                    "entry-match-width-courier-1340-x",
                    "entry-match-width-courier-1340-name",
                    "entry-match-width-courier-1340-long",
                    "status-values",
                    "entry-control-system-matched",
                    "entry-control-system-draft",
                    "entry-control-courier-matched",
                    "entry-control-courier-draft",
                    "status-layout-light-system-1024",
                    "status-layout-dark-system-1024",
                    "status-layout-light-courier-1024",
                    "status-layout-dark-arial-1340",
                    "status-layout-zero",
                    "status-layout-valkey",
                    "status-layout-wavy",
                    "status-layout-gaps",
                    "status-layout-narrow-800",
                    "status-layout-empty",
                    "status-layout-large-counts",
                    "status-layout-scientific-counts",
                    "status-info-light-default",
                    "status-info-dark-all",
                    "status-info-courier-filter",
                    "status-info-case-miss",
                    "status-info-empty-cpu",
                    "status-info-narrow-memory",
                    "status-clients-age-0",
                    "status-clients-age-1",
                    "status-clients-age-2",
                    "status-clients-age-3",
                    "status-clients-addr-1",
                    "status-clients-addr-2",
                    "status-clients-addr-3",
                    "status-clients-dark-courier-idle",
                    "status-clients-empty-wide",
                    "status-clients-narrow-refresh",
                    "status-clients-reopen",
                    "status-clients-long-light",
                    "status-clients-long-dark",
                    "status-clients-long-narrow",
                    "status-clients-overflow",
                    "status-clients-loading-light",
                    "status-clients-loading-dark",
                ];
                let only_scene = std::env::var("RUST_RDM_QA_SCENE").ok();
                let scene_prefix = std::env::var("RUST_RDM_QA_PREFIX").ok();
                let selected_scenes = std::env::var("RUST_RDM_QA_SCENES").ok().map(|scenes| scenes.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect::<Vec<_>>());
                if let Some(scenes) = &selected_scenes { assert!(!scenes.is_empty() && scenes.iter().all(|scene| snapshots.contains(&scene.as_str())), "unknown QA scene list"); }
                if let Some(scene) = &only_scene {
                    assert!(snapshots.contains(&scene.as_str()), "unknown QA scene");
                }
                if let Some(prefix)=&scene_prefix {assert!(snapshots.iter().any(|scene|scene.starts_with(prefix)),"unknown QA scene prefix");}
                let snapshots = snapshots.into_iter().filter(|name| only_scene.as_ref().is_none_or(|scene| scene == name)&&scene_prefix.as_ref().is_none_or(|prefix|name.starts_with(prefix))&&selected_scenes.as_ref().is_none_or(|scenes| scenes.iter().any(|scene| scene.as_str() == *name))).collect::<Vec<_>>();
                if snapshots.iter().any(|name| name.starts_with("json-")) { qa_json::preflight(); }
                for snapshot in snapshots {
                    let mut restore_string = None;
                    let mut restore_footer_font = None;
                    let mut restore_entry_style = None;
                    let mut entry_conversion_qa = None;
                    let _ = cx.update_window(window.into(), |_, window, cx| {
                        window.dispatch_event(
                            PlatformInput::KeyDown(KeyDownEvent {
                                keystroke: Keystroke::parse("escape").unwrap(),
                                is_held: false,
                                prefer_character_input: false,
                            }),
                            cx,
                        );
                    });
                    let _ = window.update(cx, |_, window, cx| {
                        window.resize(size(px(if snapshot.starts_with("content-nav-narrow") || snapshot.starts_with("entries-hash-narrow") {800.} else {1024.}), px(768.)));
                        view.update(cx, |app, cx| {
                            app.close_dialog(window, cx);
                            app.tab_close_warnings.clear();
                            app.cached_entry_panels.clear();
                            if let Some(i) = app.index() && app.tabs[i].conn.sock != qa_connections[0].sock {
                                app.service.connect(&qa_connections[0], 0).expect("restore the ordinary QA Redis connection");
                                app.tabs[i].conn = qa_connections[0].clone();
                                app.tabs[i].value_saving = false;
                            }
                            app.messages.clear();app.notifications.clear();
                            if snapshot != "dark" {app.pref.general.theme="light".into();app.apply_theme(window,cx);}
                            app.pref.behavior.welcomed = true;
                            app.sponsors.clear();
                            app.connections=qa_connections.clone();
                            app.groups_open.clear();
                            app.connection_selected=None;
                            app.connection_pending=None;
                            app.connection_scroll.set_offset(point(px(0.),px(0.)));
                            set_input(&app.conn_filter,"",window,cx);
                            if snapshot.starts_with("connections") || matches!(snapshot,"settings-menu"|"community") {
                                let ids=app.tabs.iter().map(|tab|tab.id).collect::<Vec<_>>();
                                for id in ids {app.close_tab(id,window,cx);}
                                app.nav=Nav::Server;
                                app.pref.general.theme="light".into();app.apply_theme(window,cx);
                            }
                            match snapshot {
                                name if name.starts_with("match-tooltip-") || name.starts_with("entry-match-width-") || name.starts_with("entry-control-") => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(),window,cx); }
                                    app.qa_prepare_match_tooltip(name,window,cx);
                                },
                                name if name.starts_with("status-layout-") || name.starts_with("status-info-") || name.starts_with("status-clients-") => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(),window,cx); }
                                    app.nav=Nav::Browser;
                                },
                                "status-values" => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(),window,cx); }
                                    app.nav=Nav::Browser;
                                },
                                name if name.starts_with("entry-diagnostic-") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    diagnostic_ui::reset_model_counter();
                                    app.qa_prepare_entry_panel("edit-entry-hash",window,cx);
                                },
                                name if name.starts_with("entry-conversion-") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    app.qa_prepare_entry_panel("edit-entry-hash",window,cx);
                                },
                                name if name.starts_with("entry-preferences") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    app.qa_prepare_entry_preferences(window,cx);
                                },
                                name if name.starts_with("entry-picker-") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    app.qa_prepare_entry_picker(name,window,cx);
                                },
                                name if name.starts_with("edit-entry-") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    app.qa_prepare_entry_panel(name,window,cx);
                                },
                                name if name.starts_with("json-") || name.starts_with("scalar-string-") => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(), window, cx); }
                                    app.nav = Nav::Browser;
                                },
                                name if name.starts_with("content-nav-") || name.starts_with("key-toolbar-") || name.starts_with("key-input-") || name.starts_with("refresh-") => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(), window, cx); }
                                    app.nav = Nav::Browser;
                                    app.pref.general.font_family.clear(); app.pref.general.font.clear();
                                    app.pref.general.font_size = if name=="content-nav-large" {18.} else {14.};
                                    app.pref.general.theme = if name=="content-nav-dark" {"dark"} else {"light"}.into();
                                    app.apply_theme(window,cx);
                                    if let Some(i)=app.index() {
                                        app.tabs[i].content_nav_offset.set(0.);app.tabs[i].content_nav_layout.set(None);
                                        app.tabs[i].row_filter_value.clear();app.tabs[i].row_match.clear();app.tabs[i].field_sort=None;
                                        app.tabs[i].column_widths.retain(|key,_|!key.starts_with("entry-"));
                                        app.pref.editor.entry_text_align=0;
                                        let id=app.tabs[i].id;
                                        let key=if name=="key-toolbar-binary" {vec![255,0,65]} else {b"aahash".to_vec()};
                                        // Each reference starts from Redis. A previous TTL
                                        // sample must not survive the quick same-key guard.
                                        if app.tabs[i].current_key.as_ref()==Some(&key){app.reload_detail(id,true,window,cx);}
                                        else{app.select_key(id,key,window,cx);}
                                    }
                                },
                                name if name.starts_with("key-filter-") || name.starts_with("tag-style-") || name.starts_with("tag-overflow-") || name.starts_with("sidebar-resize-") || name.starts_with("sidebar-contract-") || name.starts_with("selector-font-") || name.starts_with("general-fallback-") || name.starts_with("css-label-") => {
                                    if app.active.is_none() {app.open_connection(qa_connections[0].clone(),window,cx);}
                                    app.qa_prepare_key_filter(name,window,cx);
                                },
                                name if name.starts_with("key-browser") || name.starts_with("key-type-") || name.starts_with("key-database-") => {
                                    if app.active.is_none() { app.open_connection(app.connections[0].clone(),window,cx); }
                                    if let Some(i)=app.index() {
                                        let tab=&mut app.tabs[i];
                                        tab.pane=Pane::Status;tab.current_key=None;tab.detail=None;tab.info_details=false;
                                        tab.key_selection=None;tab.check_mode=false;tab.selected.clear();tab.expanded.clear();
                                        tab.key_root_expanded=true;tab.key_match.clear();tab.key_filter.clear();tab.key_filter_changed=None;
                                        set_input(&tab.filter,"",window,cx);
                                        app.nav=Nav::Browser;
                                        let id=app.tabs[i].id;
                                        app.close_database_picker(id,window,cx);app.close_type_picker(id,cx);
                                        app.tabs[i].key_type.clear();
                                        if app.tabs[i].db!=0 {app.change_database(id,0,window,cx);}else{app.scan_keys(id,true,false,window,cx);}
                                        if name=="key-type-menu-dark"{app.pref.general.theme="dark".into();app.apply_theme(window,cx);}
                                        if name.starts_with("key-database-alias") {if let Some(database)=app.tabs[i].databases.iter_mut().find(|db|db.index==1){database.alias="缓存".into();}}

                                    }
                                },
                                name if name.starts_with("entries-") || name.starts_with("footer-") => {
                                    if app.active.is_none() {app.open_connection(qa_connections[0].clone(),window,cx);}
                                    if name.starts_with("entries-") && !name.starts_with("entries-hash-delete") {
                                        let source:Json=serde_json::from_str(include_str!("../../tests/fixtures/entries-macos-source.json")).unwrap();
                                        let record=source["records"].as_array().unwrap().iter().find(|r|r["name"]==name).unwrap();
                                        restore_entry_style=Some((app.pref.general.font_size,app.general_css_size,app.pref.general.font_family.clone()));
                                        app.pref.general.font_size=record["generalFontSize"].as_f64().unwrap()as f32;app.general_css_size=app.pref.general.font_size;
                                        app.pref.general.font_family=record["family"].as_str().filter(|s|!s.is_empty()).map(|s|vec![s.to_owned()]).unwrap_or_default();
                                        app.apply_theme(window,cx);
                                    }
                                    if let Some(i)=app.index() {
                                        if name.starts_with("footer-") {
                                            restore_footer_font=Some((app.pref.general.font_size,app.general_css_size));
                                            app.pref.general.font_size=if name.ends_with("20"){20.}else{14.};
                                            app.general_css_size=app.pref.general.font_size;
                                            app.pref.decoder=qa_decoders.clone();
                                            app.tabs[i].decode="None".into();app.tabs[i].format="Raw".into();
                                            app.tabs[i].footer_hover=None;app.tabs[i].footer_tooltip=None;app.tabs[i].footer_hover_epoch+=1;
                                        }
                                        app.pref.editor.entry_text_align=0;
                                        if name.ends_with("-dark") || name.contains("-dark-") {app.pref.general.theme="dark".into();app.apply_theme(window,cx);}
                                        app.tabs[i].row_match.clear();app.tabs[i].row_filter_value.clear();app.tabs[i].row_filter_epoch+=1;app.tabs[i].row_filter_changed=None;app.tabs[i].field_sort=None;app.tabs[i].score_sort=None;
                                        app.tabs[i].column_widths.retain(|key,_|!key.starts_with("entry-"));set_input(&app.tabs[i].row_filter,"",window,cx);
                                        let id=app.tabs[i].id;
                                        let key=if name.starts_with("entries-list"){b"aalist".to_vec()}else if name.starts_with("entries-zset"){b"aazset".to_vec()}else if name.starts_with("entries-set"){b"aaset".to_vec()}else if name.starts_with("entries-stream") || name=="footer-stream"{b"stream:events".to_vec()}else{b"aahash".to_vec()};
                                        app.select_key(id,key,window,cx);
                                    }
                                },
                                name if name.starts_with("add-entries-") => {
                                    if app.active.is_none(){app.open_connection(qa_connections[0].clone(),window,cx);}
                                    if name=="add-entries-hash"{app.qa_check_add_models(window,cx);}
                                    app.qa_prepare_add_entries(name,window,cx);
                                },
                                name if name.starts_with("new-key-") => {
                                    if app.active.is_none() { app.open_connection(qa_connections[0].clone(),window,cx); }
                                    if name == "new-key-string" { app.qa_check_new_key_models(window,cx); }
                                    app.qa_prepare_new_key(name,window,cx);
                                },
                                name if name.starts_with("csv-") => {
                                    let ids=app.tabs.iter().map(|tab|tab.id).collect::<Vec<_>>();
                                    for id in ids {app.close_tab(id,window,cx);}
                                    app.nav=Nav::Server;
                                    if name.contains("progress") {app.qa_prepare_csv_progress(name,cx);}
                                    else if name.starts_with("csv-list-") {app.qa_prepare_csv_list(name,window,cx);}
                                    else if name.starts_with("csv-unit-") {app.qa_prepare_csv_unit(name,window,cx);}
                                    else if name.contains("-select-") {app.qa_prepare_csv_selection(name,window,cx);}
                                    else {app.qa_prepare_csv(name,window,cx);}
                                },
                                "connections-selected"=>{},
                                "community"=>app.open_dialog(dialogs::DialogKind::Community,window,cx),
                                "connections-copy"=>{
                                    let source=qa_connections[0].clone();let mut taken=source.clone();taken.name="localhost2".into();app.connections.push(taken);
                                    app.duplicate_connection(source,window,cx);app.qa_assert_connection_tree_dialog("copy","localhost3",cx);
                                },
                                "connections-empty"=>app.connections.clear(),
                                "connections-menu-dark"=>{app.pref.general.theme="dark".into();app.apply_theme(window,cx);},
                                "connections-empty-group"=>app.connections=vec![Connection{name:"空分组".into(),category:"group".into(),..Default::default()}],
                                "connections-connected"|"connections-menu-connected"=>app.open_connection(qa_connections[0].clone(),window,cx),
                                "connections-groups"|"connections-group-open"|"connections-filtered"|"connections-filter-group"|"connections-groups-dark"|"connections-keyboard"|"connections-keyboard-filtered"|"connections-menu-group"=>{
                                    let fixture=qa_connections[0].clone();
                                    let leaf=|name:&str,color:&str,cluster:bool|{let mut c=fixture.clone();c.name=name.into();c.mark_color=color.into();c.cluster.enable=cluster;c};
                                    app.connections=vec![fixture.clone(),Connection{name:"开发环境".into(),category:"group".into(),connections:vec![leaf("cache","#67C23A",false),leaf("orders","#F56C6C",true)],..Default::default()},Connection{name:"生产环境".into(),category:"group".into(),connections:vec![leaf("primary","#409EFF",true)],..Default::default()}];
                                    if snapshot=="connections-groups-dark"{app.pref.general.theme="dark".into();app.apply_theme(window,cx);}
                                }

                                "connection-edit-incomplete" | "connection-edit-incomplete-ssh" => {
                                    let ssh = snapshot.ends_with("-ssh");
                                    let yaml = if ssh { "- name: tunnel\n  ssh: {enable: true, username: user}\n" } else { "- name: minimal\n" };
                                    let profile: Vec<Connection> = serde_yaml::from_str(yaml).unwrap();
                                    app.open_dialog(dialogs::DialogKind::Connection(Some(profile[0].clone())),window,cx);
                                    if ssh { app.dialog.as_mut().unwrap().tab="ssh".into(); }
                                    app.qa_assert_incomplete_connection_form(ssh,cx);
                                    println!("QA PASS: {snapshot} keeps omitted numeric fields blank and parses them as zero");
                                }
                                "connection-validation-general" | "connection-validation-name" | "connection-validation-advanced" | "connection-validation-fixed" | "connection-validation-stages" | "connection-validation-save" => {
                                    app.open_dialog(dialogs::DialogKind::Connection(None),window,cx);
                                    match snapshot {
                                        "connection-validation-general" => app.set_dialog_field("addr","",window,cx),
                                        "connection-validation-name" => app.set_dialog_field("name","bad/name",window,cx),
                                        "connection-validation-advanced" => {
                                            app.set_dialog_field("name","qa valid",window,cx);
                                            app.set_dialog_field("default_filter","",window,cx);
                                            app.set_dialog_field("key_separator","",window,cx);
                                        }
                                        "connection-validation-fixed" => app.qa_type_connection_input("name","bad/name",window,cx),
                                        "connection-validation-stages" => {
                                            app.set_dialog_field("default_filter","",window,cx);
                                            app.set_dialog_field("key_separator","",window,cx);
                                        }
                                        _ => {}
                                    }
                                }
                                "welcome" => app.pref.behavior.welcomed = false,
                                "sponsor" => {
                                    app.sponsors = vec![crate::update::Sponsor {
                                        name: "QA Sponsor".into(), link: "https://example.com".into(), region: vec!["zh".into()],
                                    }];
                                }
                                "preferences-font-filter" | "preferences-font-tags" | "preferences-font-keyboard" => {
                                    app.open_dialog(dialogs::DialogKind::Preferences, window, cx);
                                    if snapshot == "preferences-font-tags" {
                                        app.set_dialog_field("general.font_family", "[\"Monaco\"]", window, cx);
                                    }
                                }
                                "preferences-theme-preview" => {
                                    app.open_dialog(dialogs::DialogKind::Preferences, window, cx);
                                    app.set_dialog_field("general.theme", "dark", window, cx);
                                    assert_eq!(app.pref.general.theme, "light");
                                    assert_eq!(cx.theme().mode, ThemeMode::Dark);
                                }
                                "preferences-language-preview" => {
                                    assert_eq!(cx.theme().mode, ThemeMode::Light);
                                    app.open_dialog(dialogs::DialogKind::Preferences, window, cx);
                                    app.set_dialog_field("general.language", "en", window, cx);
                                    assert_eq!(app.tr("common.save"), "Save");
                                }
                                "ttl" => {
                                    app.open_dialog(dialogs::DialogKind::Ttl, window, cx);
                                }
                                "status-refresh-settings" | "client-list" => {
                                    if let Some(i) = app.index() {
                                        app.tabs[i].pane = Pane::Status;
                                        app.tabs[i].info_details = false;
                                    }
                                }
                                name if name.starts_with("connection-") && !name.starts_with("connection-tabs-") => {
                                    app.open_dialog(dialogs::DialogKind::Connection(None), window, cx);
                                    let tab = name.trim_start_matches("connection-").split('-').next().unwrap();
                                    if let Some(dialog) = &mut app.dialog { dialog.tab = if tab == "test" { "general" } else { tab }.into(); }
                                    if name == "connection-ssl-enabled" { app.set_dialog_field("ssl.enable", "true", window, cx); }
                                    if name.starts_with("connection-ssh-") {
                                        app.set_dialog_field("ssh.enable", "true", window, cx);
                                        if name.ends_with("pkfile") { app.set_dialog_field("ssh.login_type", "pkfile", window, cx); }
                                        if name.ends_with("agent") { app.set_dialog_field("ssh.login_type", "agent", window, cx); }
                                    }
                                    if name == "connection-sentinel-enabled" { app.set_dialog_field("sentinel.enable", "true", window, cx); }
                                    if name.starts_with("connection-proxy-") { app.set_dialog_field("proxy.type", "2", window, cx); }
                                    if name == "connection-proxy-auth" { app.set_dialog_field("proxy.auth", "true", window, cx); }
                                    if name == "connection-alias-row" { app.add_alias_row(None, window, cx); }
                                    if name == "connection-advanced-tags" { app.set_dialog_field("db_filter_type", "show", window, cx); app.set_dialog_field("db_filter_input", "3", window, cx); }
                                    if name.starts_with("connection-test-") {
                                        let fixture = flatten(&app.connections)[0].clone();
                                        app.set_dialog_field("network", "unix", window, cx);
                                        app.set_dialog_field("sock", if name.ends_with("success") { fixture.sock } else { PathBuf::from(&fixture.sock).with_file_name("missing.sock").to_string_lossy().into_owned() }, window, cx);
                                        app.test_connection(false, window, cx);
                                    }
                                }
                                name if name.starts_with("preferences-") => {
                                    app.open_dialog(dialogs::DialogKind::Preferences, window, cx);
                                    if let Some(d) = &mut app.dialog {
                                        d.tab = name.trim_start_matches("preferences-").into();
                                    }
                                }
                                "font-size-preview"|"font-size-cleared" => app.open_dialog(dialogs::DialogKind::Preferences,window,cx),
                                name if name.starts_with("message-") || name.starts_with("notification-") => app.qa_show_notice(name,window,cx),
                                "new-connection" => app.open_dialog(
                                    dialogs::DialogKind::Connection(None),
                                    window,
                                    cx,
                                ),
                                "preferences" => {
                                    app.open_dialog(dialogs::DialogKind::Preferences, window, cx)
                                }
                                "status" => {
                                    assert_eq!(app.tr("common.save"), "保存");
                                    if let Some(connection) =
                                        flatten(&app.connections).first().cloned()
                                    {
                                        app.open_connection(connection, window, cx);
                                    }
                                }
                                "string" => {
                                    if let Some(i) = app.index() {
                                        app.tabs[i].info_details = false;
                                    }
                                    if let Some(id) = app.active {
                                        app.select_key(id, b"account:1".to_vec(), window, cx);
                                    }
                                }
                                "hash" => {
                                    if let Some(id) = app.active {
                                        app.select_key(id, b"aahash".to_vec(), window, cx);
                                    }
                                }
                                "status-info" => {
                                    if let Some(i) = app.index() {
                                        app.tabs[i].info_details = true;
                                    }
                                }
                                "hash-edit" => {
                                    if let Some(i) = app.index()
                                        && let Some(row) = app.tabs[i]
                                            .detail
                                            .as_ref()
                                            .and_then(|d| d.rows.first())
                                            .cloned()
                                    {
                                        app.open_dialog(
                                            dialogs::DialogKind::EditEntry(row),
                                            window,
                                            cx,
                                        );
                                    }
                                }
                                "list" | "set" | "zset" | "stream" => {
                                    if let Some(id) = app.active {
                                        let key = match snapshot {
                                            "list" => b"aalist".to_vec(),
                                            "set" => b"aaset".to_vec(),
                                            "zset" => b"aazset".to_vec(),
                                            _ => b"stream:events".to_vec(),
                                        };
                                        app.select_key(id, key, window, cx);
                                    }
                                }
                                "slow" => {
                                    if let Some(id) = app.active {
                                        app.switch_pane(id, Pane::Slow, window, cx);
                                    }
                                }
                                "cli" | "cli-keyboard" | "cli-results" | "cli-ansi" | "cli-long" | "cli-selection" | "cli-word-selection" | "cli-font-size" => {
                                    if app.active.is_none() {
                                        app.open_connection(app.connections[0].clone(), window, cx);
                                    }
                                    if let Some(id) = app.active {
                                        app.switch_pane(id, Pane::Cli, window, cx);
                                        let index = app.tab_index(id).unwrap();
                                        if snapshot == "cli-font-size" {
                                            app.pref.cli.font_size = 14.;
                                            *app.tabs[index].cli_view.borrow_mut() = terminal::CliView::default();
                                        }
                                        app.tabs[index].cli_output = vec![cli::Line::text(""), cli::Line::text(app.tr("interface.cli_welcome")), cli::Line::text(""), cli::Line::text("")];
                                        app.tabs[index].cli_history = cli::History::default();
                                        set_input(&app.tabs[index].cli_input, "", window, cx);
                                        if snapshot=="cli-word-selection" {
                                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/cli-word-source.json")).unwrap();
                                            app.tabs[index].cli_output=vec![cli::Line::text(oracle["mouse"]["input"].as_str().unwrap())];
                                            app.tabs[index].cli_db=0;
                                            *app.tabs[index].cli_view.borrow_mut()=terminal::CliView::default();
                                        }

                                        if snapshot == "cli-ansi" {
                                            app.tabs[index].cli_output.push(cli::Line::text("normal \x1b[31mred\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[3;4mitalic underline\x1b[0m"));
                                            app.tabs[index].cli_output.push(cli::Line::text("\x1b[38;2;10;160;230mRGB color\x1b[0m \x1b[48;5;42m256 background\x1b[0m"));
                                            app.tabs[index].cli_output.push(cli::Line::text("Unicode: 用户 😀 a\u{301}"));
                                        }
                                        if snapshot == "cli-long" || snapshot == "cli-selection" {
                                            let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(5) + " 用户 😀";
                                            app.tabs[index].cli_output.push(cli::Line::prompt("localhost:db0> ", &format!("ECHO {text}")));
                                            app.tabs[index].cli_output.push(cli::Line::text(text));
                                            if snapshot == "cli-long" { set_input(&app.tabs[index].cli_input, "GET ".to_string() + &"field_".repeat(20), window, cx); }
                                        }
                                    }
                                }
                                "monitor" => {
                                    if let Some(id) = app.active {
                                        app.switch_pane(id, Pane::Monitor, window, cx);
                                    }
                                }
                                "pubsub" => {
                                    if let Some(id) = app.active {
                                        app.switch_pane(id, Pane::Pubsub, window, cx);
                                    }
                                }
                                "dark" => {
                                    app.pref.general.theme = "dark".into();
                                    app.nav = Nav::Server;
                                    app.apply_theme(window, cx);
                                }
                                _ => {}
                            }
                            cx.notify();
                        });
                    });
                    let _ = cx.update_window(window.into(), |_, window, cx| {
                        window.dispatch_event(
                            PlatformInput::MouseMove(MouseMoveEvent {
                                position: point(px(1.), px(1.)),
                                ..Default::default()
                            }),
                            cx,
                        );
                        let _ = window.draw(cx);
                        if snapshot == "connection-advanced-tags" {
                            view.update(cx, |app, cx| {
                                app.qa_focus_database_filter(window, cx);
                            });
                            let _ = window.draw(cx);
                            window.dispatch_event(
                                PlatformInput::KeyDown(KeyDownEvent {
                                    keystroke: Keystroke::parse("enter").unwrap(),
                                    is_held: false,
                                    prefer_character_input: false,
                                }), cx,
                            );
                        }
                        if snapshot.starts_with("preferences-font-") {
                            view.update(cx, |app, cx| {
                                app.qa_type_font_query("general.font_family", if snapshot == "preferences-font-keyboard" { "Monac" } else { "QA Custom Font" }, window, cx);
                            });
                            let _ = window.draw(cx);
                            if snapshot == "preferences-font-tags" {
                                for key in ["enter"] {
                                    window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                        keystroke: Keystroke::parse(key).unwrap(),
                                        is_held: false,
                                        prefer_character_input: false,
                                    }), cx);
                                }
                            }
                        }
                    });
                    cx.background_executor()
                        .timer(Duration::from_millis(750))
                        .await;
                    if snapshot.starts_with("json-") {
                        if let Some(text) = qa_json::verify(&view, window, snapshot, cx).await { _copied_text = Some(text); }
                    }
                    if snapshot.starts_with("scalar-string-") {
                        let (copied, original) = qa_string::verify(&view, window, snapshot, cx).await;
                        if let Some(text) = copied { _copied_text = Some(text); }
                        restore_string = Some(original);
                    }
                    let _selection_paint_guard = if snapshot.starts_with("key-input-") {
                        Some(window.update(cx, |_, window, _| gpui_base::test_support::paint_active_selections(window)).unwrap())
                    } else { None };
                    if snapshot.starts_with("key-filter-") || snapshot.starts_with("tag-style-") || snapshot.starts_with("tag-overflow-") || snapshot.starts_with("sidebar-resize-") || snapshot.starts_with("sidebar-contract-") || snapshot.starts_with("selector-font-") || snapshot.starts_with("general-fallback-") || snapshot.starts_with("css-label-") {key_filter_qa::verify(&view,window,snapshot,cx).await;}
                    if snapshot.starts_with("match-tooltip-") || snapshot.starts_with("entry-match-width-") || snapshot.starts_with("entry-control-") { match_tooltip_qa::verify(&view,window,snapshot,cx).await; }
                    let status_values_restore = if snapshot == "status-values" { Some(status_values_qa::verify(&view,window,cx).await) } else { None };
                    let status_layout_restore = if snapshot.starts_with("status-layout-") { Some(status_layout_qa::verify(&view,window,snapshot,cx).await) } else { None };
                    let status_info_restore = if snapshot.starts_with("status-info-") { Some(status_info_qa::verify(&view,window,snapshot,cx).await) } else { None };
                    let status_clients_restore = if snapshot.starts_with("status-clients-") { Some(status_clients_qa::verify(&view,window,snapshot,cx).await) } else { None };
                    if snapshot.starts_with("key-input-") {
                        if let Some(text) = qa_key_input::verify(&view, window, snapshot, cx).await {
                            _copied_text = Some(text);
                        }
                    }
                    if snapshot.starts_with("csv-") && !snapshot.starts_with("csv-unit-") && !snapshot.starts_with("csv-list-") && !snapshot.contains("progress") && !snapshot.contains("-select-") {
                        if snapshot == "csv-dark-unit-hover" {
                            cx.update_window(window.into(), |_,window,cx| {
                                let position = gpui_base::test_support::find(window, &[], &ElementId::from(("csv-unit-option",1usize))).unwrap().bounds().center();
                                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),cx);
                                window.refresh();let _=window.draw(cx);
                            }).unwrap();
                        }
                        window.update(cx, |_,window,cx| view.read(cx).qa_assert_csv_layout(snapshot,window,cx)).unwrap();
                    }
                    if snapshot.contains("progress") && snapshot.starts_with("csv-") {
                        window.update(cx, |_,window,cx| view.read(cx).qa_assert_csv_progress(snapshot,window,cx)).unwrap();
                    }
                    if snapshot.starts_with("csv-export-select-") {
                        if let Some(text)=dialogs::csv_selection_qa::verify(&view,window,snapshot,cx).await {_copied_text=Some(text);}
                    }
                    if snapshot.starts_with("csv-unit-") {
                        dialogs::csv_units_qa::verify_layout(&view,window,snapshot,cx).await;
                    }
                    if snapshot.starts_with("csv-list-") {
                        if let Some(text)=dialogs::csv_list_qa::verify(&view,window,snapshot,cx).await {_copied_text=Some(text);}
                    }
                    if snapshot.starts_with("new-key-") {
                        dialogs::new_key_qa::verify(&view,window,snapshot,cx).await;
                        window.update(cx,|_,window,cx|view.read(cx).qa_assert_new_key_layout(snapshot,window,cx)).unwrap();
                    }
                    if snapshot.starts_with("add-entries-") {
                        dialogs::add_entries_qa::verify(&view,window,snapshot,cx).await;
                        window.update(cx,|_,window,cx|view.read(cx).qa_assert_add_entries_layout(snapshot,window,cx)).unwrap();
                    }
                    if snapshot == "hash-edit" {
                        dialogs::edit_entries_qa::verify(&view,window,cx).await;
                    }
                    if snapshot.starts_with("entry-conversion-") {
                        entry_conversion_qa = Some(dialogs::entry_conversion_qa::prepare(&view,window,snapshot,cx).await);
                    }
                    if snapshot.starts_with("entry-diagnostic-") {
                        dialogs::json_diagnostics_qa::verify(&view,window,snapshot,cx).await;
                    }
                    if snapshot.starts_with("entry-preferences") {
                        dialogs::entry_preferences_qa::prepare(&view,window,snapshot,cx).await;
                        window.update(cx,|_,w,cx|view.read(cx).qa_assert_entry_preferences(snapshot,w,cx)).unwrap();
                    }
                    if snapshot.starts_with("entry-picker-") {
                        dialogs::entry_picker_qa::prepare(&view,window,snapshot,cx).await;
                        window.update(cx,|_,w,cx|view.read(cx).qa_assert_entry_picker(snapshot,w,cx)).unwrap();
                    }
                    if snapshot.starts_with("edit-entry-") {
                        dialogs::entry_panel_qa::verify(&view,window,snapshot,cx).await;
                        window.update(cx,|_,w,cx|view.read(cx).qa_assert_entry_panel(snapshot,w,cx)).unwrap();
                    }
                    if snapshot.starts_with("refresh-") {
                        qa_refresh::verify(&view, window, snapshot, cx).await;
                    }
                    if snapshot.starts_with("tab-close-") {
                        dialogs::tab_close_qa::verify(&view, window, snapshot, cx).await;
                    }
                    if snapshot.starts_with("connection-tabs-") {
                        dialogs::connection_tabs_qa::verify(&view, window, snapshot, cx).await;
                    }
                    if snapshot.starts_with("connection-test-") {
                        for _ in 0..50 {
                            if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap_or(false) { break; }
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                        }
                        window.update(cx, |_, _, cx| {
                            view.read(cx).qa_assert_connection_result(snapshot.ends_with("success"));
                        }).unwrap();
                        println!("QA PASS: {snapshot}");
                    }
                    if snapshot == "connection-advanced-tags" {
                        window.update(cx, |_, _, cx| {
                            view.read(cx).qa_assert_database_tags(cx);
                        }).unwrap();
                        println!("QA PASS: database tag entry with Return");
                    }
                    if snapshot == "preferences-font-keyboard" {
                        let _ = cx.update_window(window.into(), |_, window, cx| {
                            for key in ["down", "enter"] {
                                window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                    keystroke: Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false,
                                }), cx);
                            }
                        });
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                    }
                    if matches!(snapshot, "preferences-font-tags" | "preferences-font-keyboard") {
                        let _ = cx.update_window(window.into(), |_, window, cx| {
                            let position = point(px(270.), px(143.));
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }), cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {
                                button: MouseButton::Left, position, click_count: 1, ..Default::default()
                            }), cx);
                        });
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                        window.update(cx, |_, _, cx| {
                            if snapshot == "preferences-font-tags" { view.read(cx).qa_assert_font_tags(cx); }
                            else { view.read(cx).qa_assert_font_keyboard(cx); }
                        }).unwrap();
                        println!("QA PASS: {snapshot} selection and outside dismissal");
                    }
                    if snapshot == "preferences-font-filter" {
                        window.update(cx, |_, _, cx| view.read(cx).qa_assert_font_filter(cx)).unwrap();
                    }
                    if snapshot.starts_with("entries-hash-delete") {
                        cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&"entry-delete-0".into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            view.read(cx).qa_assert_collection_delete(snapshot,window);
                        }).unwrap();
                    }
                    if snapshot.starts_with("entries-") && !snapshot.starts_with("entries-hash-delete") {
                        let click=|id:ElementId,cx:&mut AsyncApp| {cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&id).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();};
                        if snapshot=="entries-hash-left" {click("entry-align-1".into(),cx);}
                        let query=match snapshot {"entries-hash-field-filter"=>Some("name"),"entries-hash-value-filter"=>Some("Tiny"),"entries-list-index-filter"=>Some("1"),"entries-list-value-filter"=>Some("Rust"),_=>None};
                        if let Some(query)=query {window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();app.tabs[i].row_filter.update(cx,|input,cx|{input.focus(window,cx);let length=input.value().len();input.replace_text_in_range(Some(0..length),query,window,cx);});})).unwrap();cx.background_executor().timer(Duration::from_millis(550)).await;}
                        if snapshot=="entries-hash-sort" {click("heading-entry-field".into(),cx);}else if snapshot=="entries-zset-sort"{click("heading-entry-score".into(),cx);}
                        if snapshot=="entries-hash-resize" {cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&"resize-entry-field".into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);let end=position+point(px(50.),px(0.));window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:end,pressed_button:Some(MouseButton::Left),..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:end,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();}
                        cx.update_window(window.into(),|_,window,cx|{window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(1.),px(1.)),..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        window.update(cx,|_,window,cx|view.read(cx).qa_assert_collection_layout(snapshot,window,cx)).unwrap();
                    }
                    if snapshot.starts_with("content-nav-") || snapshot.starts_with("key-toolbar-") {
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                            let i=app.index().unwrap();
                            if snapshot.starts_with("key-toolbar-ttl") {app.tabs[i].detail.as_mut().unwrap().ttl=match snapshot {"key-toolbar-ttl-days"=>172900,"key-toolbar-ttl-zero"=>0,_=>100};app.tabs[i].detail_loaded=Instant::now();}
                            let pane=match snapshot {"content-nav-status"=>Pane::Status,"content-nav-cli"=>Pane::Cli,"content-nav-slow_log"=>Pane::Slow,"content-nav-cmd_monitor"=>Pane::Monitor,"content-nav-pub_message"|"content-nav-narrow-pubsub"=>Pane::Pubsub,_=>Pane::Detail};
                            app.switch_pane(app.tabs[i].id,pane,window,cx);
                        })).unwrap();
                        cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);}).unwrap();
                        cx.background_executor().timer(Duration::from_millis(150)).await;
                        cx.update_window(window.into(),|_,window,cx|{
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(1.),px(1.)),..Default::default()}),cx);
                            window.refresh();let _=window.draw(cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/content-header-source.json")).unwrap();
                            let record=oracle["records"].as_array().unwrap().iter().find(|record|record["name"]==snapshot).unwrap();
                            let metrics=record["metrics"].as_array().unwrap();
                            let has_class=|m:&&Json,class:&str|m["class"].as_str().unwrap().split_whitespace().any(|c|c==class);
                            let check=|id:ElementId,m:&Json|{let actual=gpui_base::test_support::find(window,&[],&id).unwrap_or_else(||panic!("{snapshot} missing {id:?}")).bounds();for(value,key)in[(actual.origin.x,"x"),(actual.origin.y,"y"),(actual.size.width,"w"),(actual.size.height,"h")]{assert!((value.as_f32()-m[key].as_f64().unwrap()as f32).abs()<0.6,"{snapshot} {id:?} {key} {value:?} vs {}",m[key]);}};
                            check("content-nav".into(),&metrics[0]);
                            let tabs=metrics.iter().filter(|m|has_class(m,"n-tabs-tab")).collect::<Vec<_>>();
                            let icons=metrics.iter().filter(|m|has_class(m,"n-icon")).collect::<Vec<_>>();
                            for(index,(_,_,key))in content_header::CONTENT_PANES.iter().enumerate(){check(format!("pane-{key}").into(),tabs[index]);check(format!("pane-icon-{key}").into(),icons[index]);}
                            check("content-nav-indicator".into(),metrics.iter().find(|m|has_class(m,"n-tabs-bar")).unwrap());
                            if let Some(toolbar)=metrics.iter().find(|m|has_class(m,"content-toolbar")) {
                                check("key-toolbar".into(),toolbar);
                                for(id,class)in[("key-input-group","n-input-group"),("key-type-tag","n-tag"),("key-type-label","n-tag__content"),("key-name","n-input"),("key-reload-suffix","n-input__suffix"),("key-ttl-group","n-button-group")]{check(id.into(),metrics.iter().find(|m|has_class(m,class)).unwrap());}
                                let ttl_text=metrics.iter().find(|m|has_class(m,"n-button__content")&&!m["text"].as_str().unwrap().is_empty()).unwrap();
                                check("ttl-text".into(),ttl_text);
                                let buttons=metrics.iter().filter(|m|has_class(m,"n-button")).collect::<Vec<_>>();
                                for(id,source)in["reload-key","copy-key","ttl","rename-key","delete-key"].into_iter().zip(buttons){check(id.into(),source);}
                                if snapshot=="key-toolbar-binary" {check("key-type-binary".into(),metrics.iter().find(|m|has_class(m,"n-tag__icon")).unwrap());}
                            }
                        }).unwrap();
                        println!("QA PASS: {snapshot} original navigation and key toolbar geometry");
                    }
                    if snapshot.starts_with("footer-") {
                        window.update(cx, |_, _, cx| view.update(cx, |app, _| {
                            let i = app.index().unwrap();
                            app.tabs[i].detail.as_mut().unwrap().memory = if snapshot == "footer-memory-zero" { 0 } else { 64 };
                            if snapshot=="footer-decode-custom" || snapshot.starts_with("footer-decode-long") {
                                app.pref.decoder=vec![Decoder{name:"Base64".into(),..Default::default()},Decoder{name:"自定义解码器ABCDEFGHIJKLMN".into(),..Default::default()}];
                            }
                            if snapshot.starts_with("footer-decode-long") {app.tabs[i].decode="自定义解码器ABCDEFGHIJKLMN".into();}
                        })).unwrap();
                        if snapshot.contains("tooltip") {
                            let name=if snapshot.contains("format"){ "format" }else{ "decode" };
                            cx.update_window(window.into(), |_,window,cx|{
                                window.refresh();let _=window.draw(cx);
                                let position=gpui_base::test_support::find(window,&[],&name.into()).unwrap().bounds().center();
                                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx);
                            }).unwrap();
                            cx.background_executor().timer(Duration::from_millis(180)).await;
                        }
                        if snapshot.contains("-menu") || snapshot=="footer-decode-custom" {
                            let name = if snapshot.contains("format") { "format" } else { "decode" };
                            cx.update_window(window.into(), |_, window, cx| {
                                window.refresh(); let _ = window.draw(cx);
                                let position = gpui_base::test_support::find(window, &[], &name.into()).unwrap().bounds().center();
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                                window.refresh(); let _ = window.draw(cx);
                            }).unwrap();
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                        }
                        cx.update_window(window.into(), |_, window, cx| {
                            window.refresh(); let _ = window.draw(cx);
                            let oracle: Json = serde_json::from_str(include_str!("../../tests/fixtures/value-footer-source.json")).unwrap();
                            let extra: Json = serde_json::from_str(include_str!("../../tests/fixtures/value-footer-extra-source.json")).unwrap();
                            let macos: Json = serde_json::from_str(include_str!("../../tests/fixtures/value-footer-macos-source.json")).unwrap();
                            assert_eq!(view.read(cx).general_font_size(),if snapshot.ends_with("20"){20.}else{14.},"actual footer font preference is applied");
                            let metrics = oracle["records"].as_array().unwrap().iter().chain(extra["records"].as_array().unwrap()).chain(macos["records"].as_array().unwrap()).find(|record| record["name"] == snapshot).unwrap()["metrics"].as_array().unwrap();
                            if snapshot=="footer-decode-long-menu20" {
                                let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
                                println!("QA FOOTER20 MENU: pickerDecode={:?}, options={:?}",tab.footer_picker.as_ref().map(|p|p.kind==value_footer::PickerKind::Decode),app.footer_options(value_footer::PickerKind::Decode));
                                for id in ["footer-options","footer-menu-title","footer-menu-title-label"] {
                                    println!("QA FOOTER20 BOUNDS: {id} {:?}",gpui_base::test_support::find(window,&[],&id.into()).map(|e|e.bounds()));
                                }
                            }
                            if snapshot == "footer-hash" {
                                let samples: Json = serde_json::from_str(include_str!("../../tests/fixtures/footer-font-macos-source.json")).unwrap();
                                let chrome: Json = serde_json::from_str(include_str!("../../tests/fixtures/footer-font-source.json")).unwrap();
                                let mut historical_mismatches = 0;
                                for sample in samples["samples"].as_array().unwrap() {
                                    let actual = view.read(cx).footer_text_width_at(sample["text"].as_str().unwrap(), sample["size"].as_f64().unwrap() as f32, window, cx);
                                    let expected = sample["width"].as_f64().unwrap() as f32;
                                    assert!((actual - expected).abs() < 0.1, "macOS footer text shaping {sample}: {actual}");
                                    let old = chrome["samples"].as_array().unwrap().iter().find(|old|old["size"]==sample["size"]&&old["text"]==sample["text"]).unwrap()["width"].as_f64().unwrap() as f32;
                                    if (old - expected).abs() >= 0.1 {
                                        assert!((actual-old).abs()>=0.1,"historical Chrome difference unexpectedly disappeared: {sample}");
                                        historical_mismatches += 1;
                                        println!("QA RETAINED NEGATIVE: footer Chrome {}px {} expected {old}, actual {actual}, WK {expected}",sample["size"],sample["text"]);
                                    } else {assert!((actual-old).abs()<0.1,"retained Chrome control {sample}: {actual}");}
                                }
                                assert_eq!(historical_mismatches,5);
                                println!("QA PASS: footer CJK and western kerning in 40 original macOS WK font samples; 35 matching Chrome controls and five retained Chrome differences");
                            }
                            let measure = |id: ElementId, source: &Json| {
                                let actual = gpui_base::test_support::find(window, &[], &id).expect("actual footer control").bounds();
                                for (value, key) in [(actual.origin.x, "x"), (actual.origin.y, "y"), (actual.size.width, "w"), (actual.size.height, "h")] {
                                    assert!((value.as_f32() - source[key].as_f64().unwrap() as f32).abs() < 0.6, "{snapshot} {id:?} {key}: {value:?} vs {}", source[key]);
                                }
                            };
                            measure("value-footer".into(), metrics.iter().find(|m| m["class"].as_str().unwrap().contains("value-footer")).unwrap());
                            measure("footer-summary".into(), metrics.iter().find(|m| m["class"].as_str().unwrap().starts_with("n-text ") && m["text"].as_str().unwrap().starts_with("条目:")).unwrap());
                            if snapshot != "footer-memory-zero" {
                                measure("footer-memory".into(), metrics.iter().find(|m| m["class"].as_str().unwrap().starts_with("n-text ") && m["text"].as_str().unwrap().starts_with("内存占用:")).unwrap());
                                measure("footer-memory-divider".into(), metrics.iter().find(|m| m["class"].as_str().unwrap().starts_with("n-divider ")).unwrap());
                            } else { assert!(gpui_base::test_support::find(window, &[], &"footer-memory".into()).is_none()); }
                            if snapshot != "footer-stream" {
                                for (id, text) in [("format", "Raw"), ("decode", if snapshot.starts_with("footer-decode-long"){"自定义解码器ABCDEFGHIJKLMN"}else{"None"})] {
                                    measure(id.into(), metrics.iter().find(|m| m["class"].as_str().unwrap().split_whitespace().any(|class| class == "n-button") && m["text"] == text).unwrap());
                                }
                            } else { assert!(gpui_base::test_support::find(window, &[], &"footer-selectors".into()).is_none()); }
                            if snapshot.contains("tooltip") {
                                measure("footer-tooltip".into(), metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().any(|class|class=="n-tooltip")).unwrap());
                            }
                            if snapshot.contains("-menu") || snapshot=="footer-decode-custom" {
                                measure("footer-options".into(), metrics.iter().find(|m| m["class"].as_str().unwrap().starts_with("n-dropdown-menu ")).unwrap());
                                measure("footer-menu-title".into(), metrics.iter().find(|m| m["class"] == "type-selector-header").unwrap());
                                let title_samples: Json = serde_json::from_str(include_str!("../../tests/fixtures/footer-title-source.json")).unwrap();
                                let title = if snapshot.contains("format") { "查看方式" } else { "解码/解压方式" };
                                let expected = title_samples["samples"].as_array().unwrap().iter().find(|sample| sample["text"] == title).unwrap()["width"].as_f64().unwrap() as f32;
                                let actual = gpui_base::test_support::find(window, &[], &"footer-menu-title-label".into()).unwrap().bounds().size.width.as_f32();
                                assert!((actual - expected).abs() < 0.6, "{snapshot} bold title width {actual} versus {expected}");
                                let mut index = 0;
                                let options = view.read(cx).footer_options(if snapshot.contains("format") { value_footer::PickerKind::Format } else { value_footer::PickerKind::Decode });
                                for source in metrics.iter().filter(|m| m["class"].as_str().unwrap().split_whitespace().any(|class| class == "n-dropdown-option-body")) {
                                    while options[index].is_none() { index += 1; }
                                    measure(("footer-option", index).into(), source); index += 1;
                                }
                            }
                            if let Some(source)=macos["records"].as_array().unwrap().iter().find(|record|record["name"]==snapshot) {
                                let metrics=source["metrics"].as_array().unwrap();
                                for(id,class)in[("value-footer","value-footer"),("footer-summary","n-text"),("footer-memory-divider","n-divider")] {
                                    measure(id.into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().any(|c|c==class)).unwrap());
                                }
                                measure("footer-memory".into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().any(|c|c=="n-text")&&m["text"].as_str().unwrap().starts_with("内存占用:")).unwrap());
                                for(id,label)in[("format","Raw"),("decode",if snapshot.starts_with("footer-decode-long"){"自定义解码器ABCDEFGHIJKLMN"}else{"None"})] {
                                    measure(id.into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().any(|c|c=="n-button")&&m["text"]==label).unwrap());
                                }
                                if snapshot.contains("-menu") {
                                    measure("footer-options".into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().any(|c|c=="n-dropdown-menu")).unwrap());
                                    measure("footer-menu-title".into(),metrics.iter().find(|m|m["class"]=="type-selector-header").unwrap());
                                    let options=view.read(cx).footer_options(value_footer::PickerKind::Decode);let mut index=0;
                                    for row in metrics.iter().filter(|m|m["class"].as_str().unwrap().split_whitespace().any(|c|c=="n-dropdown-option-body")) {
                                        while options[index].is_none(){index+=1;}
                                        assert_eq!(options[index].as_deref(),row["text"].as_str());
                                        measure(("footer-option",index).into(),row);index+=1;
                                    }
                                    assert_eq!(options.iter().flatten().count(),12);
                                }
                                println!("QA PASS: {snapshot} original macOS WK component geometry at {}px",source["generalFontSize"]);
                            }
                        }).unwrap();
                        println!("QA PASS: {snapshot} original footer and menu geometry");
                    }
                    if snapshot.starts_with("key-type-") || snapshot.starts_with("key-database-") {
                        let click=|id:ElementId,cx:&mut AsyncApp| {cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&id).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);
                        }).unwrap();};
                        if snapshot.starts_with("key-type-") {
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&"type-filter".into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();
                            if snapshot=="key-type-hash" {click(("key-type-option",2usize).into(),cx);cx.background_executor().timer(Duration::from_millis(250)).await;window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.key_type,"HASH");assert_eq!(tab.keys.len(),1);assert_eq!(tab.keys[0].key,b"aahash".to_vec());}).unwrap();}
                        } else {
                            if snapshot.starts_with("key-database-alias") {window.update(cx,|_,_,cx|view.update(cx,|app,_|{let i=app.index().unwrap();app.tabs[i].databases.iter_mut().find(|db|db.index==1).expect("database list loaded").alias="缓存".into();})).unwrap();}
                            click("database-select".into(),cx);
                            let query=if snapshot.contains("alias-empty"){Some("缓存")}else if snapshot.ends_with("empty"){Some("01")}else if snapshot.ends_with("filtered")||snapshot.ends_with("selected")||snapshot=="key-database-alias"{Some("1")}else{None};
                            if let Some(query)=query{window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();app.tabs[i].database_picker.query.update(cx,|input,cx|{let length=input.value().len();input.replace_text_in_range(Some(0..length),query,window,cx);});})).unwrap();}
                            if matches!(snapshot,"key-database-selected"|"key-database-keyboard") {cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse(if snapshot=="key-database-selected"{"enter"}else{"down"}).unwrap(),is_held:false,prefer_character_input:false}),cx);}).unwrap();}
                        }
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/key-selectors-source.json")).unwrap();let record=oracle["records"].as_array().unwrap().iter().find(|record|record["name"]==snapshot).unwrap();
                            let class=if snapshot.starts_with("key-type-"){"n-dropdown-menu"}else{"n-base-select-menu"};let expected=record["metrics"].as_array().unwrap().iter().find(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some(class));
                            let actual=gpui_base::test_support::find(window,&[],&if snapshot.starts_with("key-type-"){"key-type-options"}else{"key-database-options"}.into());
                            if let Some(expected)=expected {let bounds=actual.expect("source menu is visible").bounds();println!("QA SELECTOR {snapshot}: {bounds:?}");for (dimension,value) in [("x",bounds.origin.x),("y",bounds.origin.y),("w",bounds.size.width),("h",bounds.size.height)]{assert!((value.as_f32()-expected[dimension].as_f64().unwrap() as f32).abs()<0.6,"{snapshot} {dimension}: {value:?} versus {}",expected[dimension]);}}
                            else {assert!(actual.is_none(),"source menu is closed");}
                            if snapshot=="key-type-menu"||snapshot=="key-type-menu-dark" {
                                let mut actual=gpui_base::test_support::snapshots(window).into_iter().filter(|item|item.role()==Some(gpui::Role::MenuItem)).collect::<Vec<_>>();actual.sort_by(|a,b|a.bounds().origin.y.partial_cmp(&b.bounds().origin.y).unwrap());
                                assert_eq!(actual.iter().map(|item|item.label().unwrap()).collect::<Vec<_>>(),vec!["ALL","STRING","HASH","LIST","SET","ZSET","STREAM","JSON"]);
                            }
                            if matches!(snapshot,"key-database-filtered"|"key-database-alias") {let option=gpui_base::test_support::find(window,&[],&("key-database-option",1usize).into()).unwrap();assert_eq!(option.label(),Some(if snapshot=="key-database-alias"{"缓存[1] (0)"}else{"db1 (0)"}));assert_eq!(option.bounds().size.height,px(32.));}
                            let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
                            match snapshot {"key-database-filtered"|"key-database-alias"=>assert_eq!(tab.database_picker.cursor,Some(1)),"key-database-empty"|"key-database-alias-empty"=>assert!(tab.database_picker.cursor.is_none()),"key-database-keyboard"=>assert_eq!(tab.database_picker.cursor,Some(1)),"key-database-selected"=>{assert_eq!(tab.db,1);assert!(!tab.database_picker.open);assert_eq!(text_input(&tab.database_picker.query,cx),"");},_=>{}}
                        }).unwrap();
                        println!("QA PASS: {snapshot} original key selector behavior");
                    }
                    if snapshot.starts_with("key-browser") {
                        cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);}).unwrap();
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        let click=|element:ElementId,count:usize,cx:&mut AsyncApp| {
                            cx.update_window(window.into(),|_,window,cx|{
                                window.refresh();let _=window.draw(cx);
                                let position=gpui_base::test_support::find(window,&[],&element).expect("visible browser control").bounds().center();
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:count,..Default::default()}),cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:count,..Default::default()}),cx);
                                window.refresh();let _=window.draw(cx);
                            }).unwrap();
                        };
                        if matches!(snapshot,"key-browser-folder-selected"|"key-browser-folder-open"|"key-browser-key-selected") {
                            click(("key-label",0usize).into(),1,cx);
                            window.update(cx,|_,_,cx|{
                                let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
                                assert_eq!(tab.key_selection,Some(key_browser::Selection::Folder("account:".into())));
                                assert!(!tab.expanded.contains("account:"),"single click selects a folder without expanding it");
                                assert!(tab.current_key.is_none(),"folder selection preserves current detail");
                            }).unwrap();
                        }
                        if matches!(snapshot,"key-browser-folder-open"|"key-browser-key-selected") {
                            click(("key-label",0usize).into(),2,cx);
                            window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.tabs[app.index().unwrap()].expanded.contains("account:"));}).unwrap();
                        }
                        if snapshot=="key-browser-key-selected" {
                            let index=window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];views::tree_rows(tab,"",false).iter().position(|row|row.key.as_deref()==Some(b"aahash")).unwrap()}).unwrap();
                            click(("key-label",index).into(),1,cx);
                            cx.background_executor().timer(Duration::from_millis(250)).await;
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.current_key.as_deref(),Some(b"aahash".as_slice()));assert_eq!(tab.detail.as_ref().unwrap().kind,KeyType::Hash);}).unwrap();
                        }
                        if snapshot=="key-browser-filtered" {
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();let id=app.tabs[i].id;set_input(&app.tabs[i].filter,"hash",window,cx);app.update_key_filter(id,window,cx);})).unwrap();
                            cx.background_executor().timer(Duration::from_millis(1100)).await;
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.keys.len(),19,"local filtering does not discard loaded keys");let rows=views::tree_rows(tab,&tab.key_filter,false);assert_eq!(rows.len(),1);assert_eq!(rows[0].key.as_deref(),Some(b"aahash".as_slice()));assert!(tab.key_match.is_empty());}).unwrap();
                        }
                        if snapshot=="key-browser-check-mode" {
                            click("check-mode".into(),1,cx);
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(tab.check_mode);let rows=views::tree_rows(tab,"",false);assert_eq!(rows[0].label,"db0");assert_eq!(rows[1].depth,1);}).unwrap();
                        }
                        if snapshot=="key-browser-delete" {
                            click(("key-label",4usize).into(),1,cx);
                            cx.background_executor().timer(Duration::from_millis(250)).await;
                            click("key-delete-4".into(),1,cx);
                            window.update(cx,|_,_,cx|{let app=view.read(cx);app.qa_assert_key_delete_warning(b"aahash");}).unwrap();
                        }
                        if matches!(snapshot,"key-browser-menu-folder"|"key-browser-menu-key"|"key-browser-more") {
                            if snapshot=="key-browser-more" {click("database-more".into(),1,cx);} else {
                                cx.update_window(window.into(),|_,window,cx|{
                                    let position=point(px(200.),px(if snapshot=="key-browser-menu-folder"{95.}else{215.}));let button=MouseButton::Right;
                                    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button,position,click_count:1,..Default::default()}),cx);
                                    window.refresh();let _=window.draw(cx);
                                }).unwrap();
                                window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(tab.current_key.is_none(),"right click selects without loading a key");assert_eq!(tab.key_selection,Some(if snapshot=="key-browser-menu-folder"{key_browser::Selection::Folder("account:".into())}else{key_browser::Selection::Key(b"aahash".to_vec())}));}).unwrap();
                            }
                            cx.update_window(window.into(),|_,window,cx|{window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(1000.),px(700.)),..Default::default()}),cx);}).unwrap();
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                        }
                        if snapshot=="key-browser-search" {
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();app.tabs[i].filter.update(cx,|input,cx|{input.focus(window,cx);input.replace_text_in_range(Some(0..0),"hash",window,cx);});})).unwrap();
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("enter").unwrap(),is_held:false,prefer_character_input:false}),cx);}).unwrap();
                            cx.background_executor().timer(Duration::from_millis(250)).await;
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.key_match,"hash");assert_eq!(text_input(&tab.filter,cx),"");assert_eq!(tab.keys.len(),1);assert_eq!(tab.keys[0].key,b"aahash".to_vec());}).unwrap();
                        }
                        if snapshot=="key-browser-check-mode" {
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/key-browser-source.json")).unwrap();
                            for (element,expected) in [("key-check",0usize),("key-check",1usize),("key-check",0usize),("key-check",0usize)].into_iter().zip(oracle["checks"].as_array().unwrap().iter()) {
                                click(element.into(),1,cx);
                                window.update(cx,|_,_,cx|{let app=view.read(cx);assert_eq!(app.tabs[app.index().unwrap()].selected.len(),expected["count"].as_u64().unwrap() as usize,"source checkbox cascade");}).unwrap();
                            }
                            click("quit-check-mode".into(),1,cx);
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(!tab.check_mode);assert!(tab.selected.is_empty());}).unwrap();
                            click("check-mode".into(),1,cx);
                        }
                        cx.update_window(window.into(),|_,window,cx| {
                            window.refresh();let _=window.draw(cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/key-browser-source.json")).unwrap();
                            let metrics=oracle["records"].as_array().unwrap().iter().find(|record|record["name"]==snapshot).unwrap()["metrics"].as_array().unwrap();
                            let check=|element:ElementId,expected:&Json| {
                                let actual=gpui_base::test_support::find(window,&[],&element).expect("source browser element").bounds();
                                for (dimension,value) in [("x",actual.origin.x),("y",actual.origin.y),("w",actual.size.width),("h",actual.size.height)] {
                                    assert!((value.as_f32()-expected[dimension].as_f64().unwrap() as f32).abs()<0.6,"{snapshot} {element:?} {dimension}: {value:?} versus {}",expected[dimension]);
                                }
                            };
                            let input_bounds=gpui_base::test_support::snapshots(window).into_iter().filter(|element|element.role()==Some(gpui::Role::TextInput)&&element.bounds().origin.x<px(300.)&&element.bounds().origin.y<px(80.)).collect::<Vec<_>>();
                            assert_eq!(input_bounds.len(),1);
                            assert_eq!(input_bounds[0].bounds().size.height,px(28.),"source small search input height");
                            check("exact-match".into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some("n-tag")&&m["class"].as_str().unwrap().contains("n-tag--checkable")).unwrap());
                            if snapshot=="key-browser-search" {check("key-search-match".into(),metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some("n-tag")&&m["text"]=="hash").unwrap());}
                            let headers=metrics.iter().filter(|m|m["class"].as_str().unwrap()=="flex-box-h nav-pane-func").collect::<Vec<_>>();
                            check("key-browser-header".into(),headers[0]);check("key-browser-footer".into(),headers[1]);
                            for (index,row) in metrics.iter().filter(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some("n-tree-node")).enumerate(){check(("key-row",index).into(),row);}
                            for (index,prefix) in metrics.iter().filter(|m|m["class"]=="n-tree-node-content__prefix").enumerate(){check(("key-prefix",index).into(),prefix);}
                            let footer_controls=metrics.iter().filter(|m|m["y"].as_f64().unwrap()>730. && (m["class"]=="n-select" || m["class"].as_str().unwrap().split_whitespace().any(|class|class=="n-button"))).collect::<Vec<_>>();
                            let footer_ids=if snapshot=="key-browser-check-mode" {vec!["export-checked","ttl-checked","delete-checked","quit-check-mode"]}else{vec!["database-select","load-more-keys","load-all-keys","check-mode","database-more"]};
                            assert_eq!(footer_controls.len(),footer_ids.len());for (name,expected) in footer_ids.into_iter().zip(footer_controls){check(name.into(),expected);}
                            if matches!(snapshot,"key-browser-menu-folder"|"key-browser-menu-key"|"key-browser-more") {
                                let menu=metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some("n-dropdown-menu")).unwrap();
                                check("popup-menu".into(),menu);
                                let expected=metrics.iter().filter(|m|m["class"]=="n-dropdown-option-body").collect::<Vec<_>>();
                                let mut actual=gpui_base::test_support::snapshots(window).into_iter().filter(|item|item.role()==Some(gpui::Role::MenuItem)&&item.label().is_some()).collect::<Vec<_>>();actual.sort_by(|a,b|a.bounds().origin.y.partial_cmp(&b.bounds().origin.y).unwrap());
                                assert_eq!(actual.len(),expected.len());for (actual,expected) in actual.iter().zip(expected){assert_eq!(actual.label().unwrap(),expected["text"].as_str().unwrap());assert_eq!(actual.bounds().size.height,px(28.));}
                            }
                            if snapshot=="key-browser-delete" {
                                let expected=metrics.iter().find(|m|m["class"].as_str().unwrap().split_whitespace().next()==Some("n-dialog")).unwrap();check("key-delete-warning".into(),expected);
                                for (name,class) in [("key-delete-warning-title","n-dialog__title"),("key-delete-warning-content","n-dialog__content"),("key-delete-warning-actions","n-dialog__action")] {check(name.into(),metrics.iter().find(|m|m["class"]==class).unwrap());}
                                for (name,label) in [("dialog-cancel","取消"),("dialog-save","确认")] {check(name.into(),metrics.iter().find(|m|m["text"]==label&&m["class"].as_str().unwrap().split_whitespace().any(|class|class=="n-button")).unwrap());}
                            }
                            for name in ["type-filter","filter-keys","refresh-keys","new-key","database-select","load-more-keys","load-all-keys","check-mode","database-more"] {
                                if let Some(element)=gpui_base::test_support::find(window,&[],&name.into()){println!("QA BROWSER {snapshot} control {name}: {:?}",element.bounds());}
                            }
                            for name in ["key-browser-header","key-browser-tree","key-browser-footer"] {
                                println!("QA BROWSER {snapshot} {name}: {:?}",gpui_base::test_support::find(window,&[],&name.into()).unwrap().bounds());
                            }
                            println!("QA BROWSER {snapshot} row0: {:?}",gpui_base::test_support::find(window,&[],&("key-row",0usize).into()).unwrap().bounds());
                        }).unwrap();
                        println!("QA PASS: {snapshot} source browser behavior");
                    }
                    if snapshot == "status" {
                        for _ in 0..4 {
                            let _ = window.update(cx, |_, window, cx| {
                                view.update(cx, |app, cx| {
                                    if let Some(id) = app.active {
                                        app.refresh_info(id, window, cx);
                                    }
                                })
                            });
                            cx.background_executor()
                                .timer(Duration::from_millis(150))
                                .await;
                        }
                    }
                    if snapshot == "status-refresh-settings" {
                        super::qa_refresh_status::verify(&view, window, cx).await;
                        let _ = cx.update_window(window.into(), |_, window, cx| {
                            window.dispatch_event(
                                PlatformInput::MouseMove(MouseMoveEvent {
                                    position: point(px(985.), px(112.)),
                                    ..Default::default()
                                }),
                                cx,
                            );
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(650))
                            .await;
                    }
                    if snapshot == "client-list" {
                        let _ = cx.update_window(window.into(), |_, window, cx| {
                            let position = point(px(564.), px(196.));
                            window.dispatch_event(
                                PlatformInput::MouseDown(MouseDownEvent {
                                    button: MouseButton::Left,
                                    position,
                                    click_count: 1,
                                    ..Default::default()
                                }),
                                cx,
                            );
                            window.dispatch_event(
                                PlatformInput::MouseUp(MouseUpEvent {
                                    button: MouseButton::Left,
                                    position,
                                    click_count: 1,
                                    ..Default::default()
                                }),
                                cx,
                            );
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(250))
                            .await;
                    }
                    if snapshot == "hash-save-check" {
                        let state = window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                            let i = app.index().expect("active QA connection");
                            assert_eq!(app.tabs[i].detail.as_ref().unwrap().kind, KeyType::Hash);
                            let busy = app.busy;
                            app.save_active(window, cx);
                            assert_eq!(app.busy, busy, "saving a collection without an entry editor must not issue SET");
                            let row = app.tabs[i].detail.as_ref().unwrap().rows.first().unwrap().clone();
                            let c = app.tabs[i].conn.clone();
                            let db = app.tabs[i].db;
                            let service = app.service.clone();
                            app.open_dialog(dialogs::DialogKind::EditEntry(row.clone()), window, cx);
                            app.set_dialog_field("pin", "true", window, cx);
                            app.set_dialog_field("value", "QA multiline\nentry", window, cx);
                            app.save_active(window, cx);
                            (service, c, db, row)
                        })).unwrap();
                        for _ in 0..50 {
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                            if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap_or(false) { break; }
                        }
                        let (service, c, db, row) = state;
                        let _ = window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                            assert!(app.editing_entry(), "a pinned editor remains open after saving");
                            let busy = app.busy;
                            app.save_active(window, cx);
                            assert!(app.busy > busy, "the original unchanged editor shortcut still submits");
                        }));
                        for _ in 0..50 {
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                            if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap_or(false) { break; }
                        }
                        let _ = window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                            app.full_editor = true;
                            app.open_decoder_settings(window, cx);
                            app.close_dialog(window, cx);
                            assert!(app.editing_entry());
                            assert!(app.full_editor, "returning from decoder settings preserves the editor size");
                            app.full_editor = false;
                        }));
                        cx.background_executor().spawn(async move {
                            assert_eq!(service.run(&c, db, command(&["TYPE", "aahash"]))?.text(), "hash");
                            assert_eq!(service.run(&c, db, vec![b"HGET".to_vec(), b"aahash".to_vec(), row.id.clone()])?.bytes(), b"QA multiline\nentry");
                            service.run(&c, db, vec![b"HSET".to_vec(), b"aahash".to_vec(), row.id, row.value])?;
                            Ok::<_, anyhow::Error>(())
                        }).await.expect("save shortcut integration check");
                        let _ = window.update(cx, |_, window, cx| view.update(cx, |app, cx| { if let Some(id) = app.active { app.reload_detail(id, true, window, cx); } }));
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        println!("QA PASS: collection save guard and multiline entry save");
                    }
                    if snapshot == "cli-keyboard" {
                        for (key, input, cursor, expected, expected_cursor) in [
                            ("ctrl-a", "PING", 4, "PING", 0),
                            ("ctrl-e", "PING", 0, "PING", 4),
                            ("ctrl-b", "SET 用户 值", 14, "SET 用户 值", 11),
                            ("ctrl-f", "SET 用户 值", 11, "SET 用户 值", 14),
                            ("ctrl-d", "SET 用户 值", 11, "SET 用户 ", 11),
                            ("ctrl-h", "SET 用户 值", 11, "SET 用户值", 10),
                            ("ctrl-u", "SET 用户 值", 11, "值", 0),
                            ("ctrl-k", "SET 用户 值", 11, "SET 用户 ", 11),
                            ("ctrl-w", "SET foo bar", 11, "SET foo", 7),
                        ] {
                            cx.update_window(window.into(), |_, window, cx| {
                                view.update(cx, |app, cx| {
                                    let i = app.index().unwrap();
                                    app.tabs[i].cli_input.update(cx, |input_state, cx| {
                                        input_state.set_value(input, window, cx);
                                        input_state.set_selected_range(cursor..cursor, cx);
                                    });
                                    app.tabs[i].cli_input.focus_handle(cx).focus(window, cx);
                                });
                                window.refresh(); let _ = window.draw(cx);
                                window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                    keystroke: Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false,
                                }), cx);
                                let app = view.read(cx); let i = app.index().unwrap();
                                assert_eq!(text_input(&app.tabs[i].cli_input, cx), expected, "CLI key {key}");
                                assert_eq!(app.tabs[i].cli_input.read(cx).cursor(), expected_cursor, "CLI cursor {key}");
                            }).unwrap();
                        }
                        cx.update_window(window.into(), |_, window, cx| {
                            view.update(cx, |app, cx| {
                                let i = app.index().unwrap();
                                app.tabs[i].cli_history = cli::History::default();
                                app.tabs[i].cli_history.finish("PING");
                                app.tabs[i].cli_history.finish("GET aahash");
                                set_input(&app.tabs[i].cli_input, "unfinished draft", window, cx);
                            });
                        }).unwrap();
                        for (key, expected) in [("ctrl-p", "GET aahash"), ("ctrl-n", "unfinished draft"), ("pageup", "GET aahash"), ("pagedown", "unfinished draft"), ("ctrl-l", "")] {
                            cx.update_window(window.into(), |_, window, cx| {
                                window.refresh(); let _ = window.draw(cx);
                                window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                    keystroke: Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false,
                                }), cx);
                                let app = view.read(cx); let i = app.index().unwrap();
                                assert_eq!(text_input(&app.tabs[i].cli_input, cx), expected, "CLI history {key}");
                            }).unwrap();
                        }
                        println!("QA PASS: CLI Ctrl editing, Unicode cursor, history draft and clear-screen keys");
                    }
                    if snapshot == "cli-results" || snapshot == "cli-keyboard" {
                        let lines: &[&str] = if snapshot == "cli-results" {
                            &["PING", "HGETALL aahash", "GET qa_cli_missing", "QA_BAD_COMMAND"]
                        } else {
                            &["SELECT 1", "SET qa_cli_item fixture", "GET qa_cli_item", "SELECT 999999", "DEL qa_cli_item", "SELECT 0"]
                        };
                        for line in lines {
                            let before = cx.update_window(window.into(), |_, window, cx| {
                                let before = view.read(cx).tabs[view.read(cx).index().unwrap()].cli_output.len();
                                view.update(cx, |app, cx| {
                                    let i = app.index().unwrap();
                                    set_input(&app.tabs[i].cli_input, *line, window, cx);
                                    app.tabs[i].cli_input.focus_handle(cx).focus(window, cx);
                                });
                                window.refresh(); let _ = window.draw(cx);
                                window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                    keystroke: Keystroke::parse("enter").unwrap(), is_held: false, prefer_character_input: false,
                                }), cx);
                                before
                            }).unwrap();
                            cx.background_executor().timer(Duration::from_millis(250)).await;
                            window.update(cx, |_, _, cx| {
                                let app = view.read(cx); let i = app.index().unwrap(); let tab = &app.tabs[i];
                                assert!(tab.cli_output.len() >= before + 2, "CLI command completed: {line}");
                                if *line == "SELECT 1" || *line == "SELECT 999999" { assert_eq!(tab.cli_db, 1); }
                                if *line == "GET qa_cli_item" { assert_eq!(tab.cli_output.last().unwrap().text, "fixture"); }
                                if *line == "SELECT 999999" { assert!(matches!(tab.cli_output.last().unwrap().kind, cli::LineKind::Error)); }
                                assert_eq!(tab.db, 0, "CLI SELECT does not change the browser database");
                            }).unwrap();
                        }
                        println!("QA PASS: {snapshot} commands use Enter and preserve browser database isolation");
                    }
                    if snapshot == "cli-ansi" || snapshot == "cli-long" || snapshot == "cli-selection" {
                        cx.update_window(window.into(), |_, window, cx| {
                            window.refresh(); let _ = window.draw(cx);
                            let app=view.read(cx); let i=app.index().unwrap(); let state=app.tabs[i].cli_view.borrow();
                            let frame=state.frame.as_ref().unwrap();
                            println!("QA terminal bounds: {:?}, rows {}, cols {}, cell {}, line {}",state.bounds,state.visible_rows,frame.cols,state.cell_width,state.line_height);
                            assert_eq!((state.visible_rows,frame.cols),(42,76),"same terminal dimensions as source fixture");
                        }).unwrap();
                    }
                    if snapshot=="cli-font-size" {
                        let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/cli-font-size-source.json")).unwrap();
                        for case in oracle["records"].as_array().unwrap() {
                            window.update(cx,|_,_,cx|view.update(cx,|app,cx|{app.pref.cli.font_size=case["size"].as_f64().unwrap() as f32;cx.notify();})).unwrap();
                            cx.update_window(window.into(),|_,window,cx|{
                                window.refresh();let _=window.draw(cx);
                                let app=view.read(cx);let state=app.tabs[app.index().unwrap()].cli_view.borrow();
                                println!("QA CLI font {}: rows {}, cols {}, cell {}, line {}",case["size"],state.visible_rows,state.frame.as_ref().unwrap().cols,state.cell_width,state.line_height);
                                assert_eq!(app.cli_font_size(),case["effective"].as_f64().unwrap() as f32,"zero font size falls back to source default");
                                assert_eq!(state.visible_rows,case["rows"].as_u64().unwrap() as usize,"source rows after font change");
                                assert_eq!(state.frame.as_ref().unwrap().cols,usize::try_from(case["cols"].as_u64().unwrap()).unwrap(),"source cols after font change");
                                assert!((state.cell_width-case["cell"]["width"].as_f64().unwrap() as f32).abs()<0.001,"source rounded canvas cell width");
                                assert_eq!(state.line_height,case["cell"]["height"].as_f64().unwrap() as f32,"source DOM character height");
                            }).unwrap();
                        }
                        println!("QA PASS: 8 original CLI font and grid states, including retained rows and zero fallback");
                    }
                    if snapshot == "cli-selection" {
                        cx.update_window(window.into(), |_, window, cx| {
                            let original=cx.read_from_clipboard();
                            let (start,end)={let app=view.read(cx);let i=app.index().unwrap();let state=app.tabs[i].cli_view.borrow();
                                (state.bounds.origin+point(px(10.+5.1*state.cell_width),px(4.5*state.line_height)),state.bounds.origin+point(px(10.+20.1*state.cell_width),px(5.5*state.line_height)))};
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {button:MouseButton::Left,position:start,click_count:1,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent {position:end,pressed_button:Some(MouseButton::Left),..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {button:MouseButton::Left,position:end,click_count:1,..Default::default()}),cx);
                            window.refresh(); let _ = window.draw(cx);
                            window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {keystroke:Keystroke::parse("cmd-c").unwrap(),is_held:false,prefer_character_input:false}),cx);
                            let actual=copy_result(original,cx);
                            let expected= "host:db0> ECHO ".to_string() + &"ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(5)[..76];
                            assert!(actual.as_deref()==Some(expected.as_str()),"wrapped terminal selection must copy the fixture text without extra newlines");
                            _copied_text=actual;
                        }).unwrap();
                        println!("QA PASS: terminal mouse selection and Cmd+C across a wrapped row");
                    }
                    if snapshot=="cli-word-selection" {
                        cx.update_window(window.into(),|_,window,cx|{
                            let original=cx.read_from_clipboard();
                            window.refresh();let _=window.draw(cx);
                            let position={let app=view.read(cx);let state=app.tabs[app.index().unwrap()].cli_view.borrow();state.bounds.origin+point(px(10.+4.25*state.cell_width),px(0.5*state.line_height))};
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                            window.refresh();let _=window.draw(cx);
                            window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("cmd-c").unwrap(),is_held:false,prefer_character_input:false}),cx);
                            let text=copy_result(original,cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/cli-word-source.json")).unwrap();
                            assert!(text.as_deref()==oracle["mouse"]["text"].as_str(),"actual double click and Cmd+C must match the original xterm fixture");_copied_text=text;
                            for case in oracle["drags"].as_array().unwrap() {
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                                let end={let app=view.read(cx);let state=app.tabs[app.index().unwrap()].cli_view.borrow();state.bounds.origin+point(px(10.+case["to"].as_f64().unwrap() as f32*state.cell_width),px(0.5*state.line_height))};
                                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:end,pressed_button:Some(MouseButton::Left),..Default::default()}),cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:end,click_count:2,..Default::default()}),cx);
                                let app=view.read(cx);let state=app.tabs[app.index().unwrap()].cli_view.borrow();
                                assert_eq!(state.selected_text().as_deref(),case["text"].as_str(),"source word drag direction");
                            }
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:3,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:3,..Default::default()}),cx);
                            {let app=view.read(cx);let state=app.tabs[app.index().unwrap()].cli_view.borrow();assert_eq!(state.selected_text().as_deref(),oracle["triple"]["text"].as_str(),"source triple click selects a complete logical line");}
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                        }).unwrap();
                        println!("QA PASS: terminal double click selects a word and Cmd+C copies the original text");
                    }
                    if matches!(snapshot,"connection-validation-general"|"connection-validation-name"|"connection-validation-advanced"|"connection-validation-stages") {
                        cx.update_window(window.into(),|_,window,cx| {
                            window.refresh();let _=window.draw(cx);
                            let position=point(px(758.),px(600.));
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                        }).unwrap();
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                        window.update(cx,|_,_,cx| {
                            use ConnectionFieldError::*;
                            let (errors,tab):(&[(&str,ConnectionFieldError)],&str)=match snapshot {
                                "connection-validation-general" => (&[("name",Required),("addr",Required)],"general"),
                                "connection-validation-name" => (&[("name",IllegalCharacters)],"general"),
                                "connection-validation-advanced" => (&[("default_filter",Required),("key_separator",Required)],"advanced"),
                                _ => (&[("name",Required)],"general"),
                            };
                            view.read(cx).qa_assert_connection_validation(errors,tab);
                        }).unwrap();
                        println!("QA PASS: {snapshot} uses the confirm button and source feedback");
                    }
                    if snapshot=="connection-validation-fixed" {
                        window.update(cx,|_,window,cx| view.update(cx,|app,cx| {
                            app.qa_assert_connection_validation(&[("name",ConnectionFieldError::IllegalCharacters)],"general");
                            app.qa_type_connection_input("name","fixed name",window,cx);
                        })).unwrap();
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                        window.update(cx,|_,_,cx|view.read(cx).qa_assert_connection_validation(&[],"general")).unwrap();
                        println!("QA PASS: actual input shows and clears connection feedback");
                    }
                    if snapshot=="connection-validation-save" {
                        let dir=tempfile::Builder::new().prefix("rust-rdm-form-qa-").tempdir_in("/private/tmp").unwrap();
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx| {
                            let original=std::mem::replace(&mut app.store,Store::new(dir.path()));
                            let connections=app.connections.clone();
                            app.set_dialog_field("name","qa incomplete",window,cx);
                            app.set_dialog_field("port","",window,cx);
                            app.set_dialog_field("ssh.enable","true",window,cx);
                            app.set_dialog_field("ssh.addr","",window,cx);
                            app.set_dialog_field("ssh.username","",window,cx);
                            app.submit_dialog(window,cx);
                            assert!(app.dialog.is_none(),"source accepts incomplete transport fields");
                            let stored=app.store.connections().unwrap();
                            let saved=stored.iter().find(|profile|profile.name=="qa incomplete").unwrap();
                            assert_eq!(saved.port,0);assert!(saved.ssh.enable);assert_eq!(saved.ssh.username,"");
                            app.store=original;app.connections=connections;
                        })).unwrap();
                        println!("QA PASS: connection confirmation persists nullable port and incomplete SSH in an isolated store");
                    }
                    if snapshot.starts_with("connections-") {
                        if snapshot=="connections-connected" {
                            for _ in 0..50 {
                                if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap_or(false){break;}
                                cx.background_executor().timer(Duration::from_millis(100)).await;
                            }
                            window.update(cx,|_,_,cx|view.update(cx,|app,cx|{assert_eq!(app.tabs.len(),1);app.nav=Nav::Server;cx.notify();})).unwrap();
                        }
                        let click=|position:Point<Pixels>,cx:&mut AsyncApp|{
                            cx.update_window(window.into(),|_,window,cx|{
                                window.refresh();let _=window.draw(cx);
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                window.refresh();let _=window.draw(cx);
                            }).unwrap();
                        };
                        match snapshot {
                            "connections-selected"|"connections-connected"=>{
                                click(point(px(170.),px(55.)),cx);
                                window.update(cx,|_,_,cx|assert_eq!(view.read(cx).connection_selected.as_deref(),Some("/localhost"))).unwrap();
                            }
                            "connections-group-open"|"connections-groups-dark"=>{
                                if snapshot=="connections-group-open" {
                                    click(point(px(170.),px(85.)),cx);
                                    window.update(cx,|_,_,cx|{assert!(view.read(cx).groups_open.is_empty());assert_eq!(view.read(cx).connection_selected.as_deref(),Some("开发环境/"));}).unwrap();
                                }
                                click(point(px(66.),px(85.)),cx);
                                window.update(cx,|_,_,cx|assert!(view.read(cx).groups_open.contains("开发环境"))).unwrap();
                            }
                            "connections-filtered"|"connections-filter-group"=>{
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                    app.conn_filter.update(cx,|input,cx|{input.focus(window,cx);input.replace_text_in_range(Some(0..0),if snapshot=="connections-filtered"{"orders"}else{"开发"},window,cx);});
                                })).unwrap();
                            }
                            _=>{}
                        }
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        println!("QA PASS: {snapshot} source tree interaction");
                    }
                    if matches!(snapshot,"connections-keyboard"|"connections-keyboard-filtered") {
                        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/connection-tree-keyboard-source.json")).unwrap();
                        // Hidden macOS windows do not emit activation-dependent blur events.
                        // The blur/refocus oracle is checked in the actual desktop window.
                        let sequence=if snapshot=="connections-keyboard-filtered" {
                            oracle["filtered"].as_array().unwrap().clone()
                        } else {
                            oracle["states"].as_array().unwrap().iter().take_while(|state|state["step"]!="blur").cloned().collect()
                        };
                        for expected in sequence {
                            let step=expected["step"].as_str().unwrap();
                            cx.update_window(window.into(),|_,window,cx|{
                                if matches!(step,"focus"|"refocus-down"|"filter-focus") {
                                    view.update(cx,|app,cx|{
                                        if step=="filter-focus" {app.conn_filter.update(cx,|input,cx|{input.focus(window,cx);input.replace_text_in_range(Some(0..0),"orders",window,cx);});}
                                        app.connection_focus.focus(window,cx);
                                    });
                                } else if step=="blur" {view.update(cx,|app,cx|app.conn_filter.update(cx,|input,cx|input.focus(window,cx)));}
                                window.refresh();let _=window.draw(cx);
                                let key=match step {"Enter"=>Some("enter"),"ArrowDown"|"refocus-down"=>Some("down"),"ArrowUp"=>Some("up"),"ArrowLeft"=>Some("left"),"ArrowRight"=>Some("right"),_=>None};
                                if let Some(key)=key {window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse(key).unwrap(),is_held:false,prefer_character_input:false}),cx);}
                            }).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,_,cx|{
                                let app=view.read(cx);
                                let actual=app.connection_tree_nodes().iter().map(|node|serde_json::json!({"label":node.label,"selected":app.connection_selected.as_deref()==Some(node.key.as_str()),"pending":app.connection_pending.as_deref()==Some(node.key.as_str()),"expanded":node.expanded})).collect::<Vec<_>>();
                                assert_eq!(serde_json::json!(actual),expected["rows"],"source keyboard state after {step}");
                                assert!(app.tabs.is_empty(),"Enter selects without opening a connection");
                            }).unwrap();
                        }
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        println!("QA PASS: {snapshot} actual key events match original tree oracle");
                    }
                    if matches!(snapshot,"connections-menu"|"connections-menu-hover"|"connections-menu-connected"|"connections-menu-group"|"connections-menu-dark"|"connections-more"|"settings-menu") {
                        if snapshot=="connections-menu-connected" {
                            for _ in 0..50 {if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap_or(false){break;}cx.background_executor().timer(Duration::from_millis(100)).await;}
                            window.update(cx,|_,_,cx|view.update(cx,|app,cx|{assert_eq!(app.tabs.len(),1);app.nav=Nav::Server;cx.notify();})).unwrap();
                        }
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            let (position,button)=match snapshot {"connections-menu-group"=>(point(px(170.),px(85.)),MouseButton::Right),"connections-more"=>(point(px(330.),px(748.)),MouseButton::Left),"settings-menu"=>(point(px(25.),px(657.)),MouseButton::Left),_=>(point(px(170.),px(55.)),MouseButton::Right)};
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button,position,click_count:1,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button,position,click_count:1,..Default::default()}),cx);
                            window.refresh();let _=window.draw(cx);
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:if snapshot=="connections-menu-hover"{point(px(200.),px(80.))}else{point(px(1000.),px(750.))},pressed_button:None,..Default::default()}),cx);
                            window.refresh();let _=window.draw(cx);
                        }).unwrap();
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/dropdown-menu-source.json")).unwrap();
                            let source=oracle["menus"].as_array().unwrap().iter().find(|menu|menu["name"]==snapshot).unwrap();
                            let expected=&source["metrics"][0];
                            let menu=gpui_base::test_support::find(window,&[],&"popup-menu".into()).expect("actual popup is visible");
                            let bounds=menu.bounds();
                            println!("QA MENU {snapshot}: {bounds:?}");
                            assert!((bounds.origin.x.as_f32()-expected["x"].as_f64().unwrap() as f32).abs()<0.6,"source menu x");
                            assert!((bounds.origin.y.as_f32()-expected["y"].as_f64().unwrap() as f32).abs()<0.6,"source menu y");
                            assert!((bounds.size.width.as_f32()-expected["w"].as_f64().unwrap() as f32).abs()<0.1,"source menu width");
                            assert!((bounds.size.height.as_f32()-expected["h"].as_f64().unwrap() as f32).abs()<0.1,"source menu height");
                            let mut items=gpui_base::test_support::snapshots(window).into_iter().filter(|item|item.role()==Some(gpui::Role::MenuItem)&&item.label().is_some()).collect::<Vec<_>>();items.sort_by(|a,b|a.bounds().origin.y.partial_cmp(&b.bounds().origin.y).unwrap());
                            let source_items=source["metrics"].as_array().unwrap().iter().filter(|item|item["class"].as_str().unwrap().split_whitespace().next()==Some("n-dropdown-option-body")).collect::<Vec<_>>();
                            assert_eq!(items.len(),source_items.len(),"source menu item count");
                            for (item,expected) in items.iter().zip(source_items) {
                                assert_eq!(item.label(),expected["text"].as_str(),"source menu label");
                                assert_eq!(item.bounds().size.height,px(28.),"source option row height");
                            }
                        }).unwrap();
                        println!("QA PASS: {snapshot} actual menu trigger");
                    }
                    if snapshot=="community" {
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/community-source.json")).unwrap();
                            let expected=oracle["metrics"].as_array().unwrap().iter().find(|element|element["class"]=="n-flex n-modal").unwrap();
                            let actual=gpui_base::test_support::find(window,&[],&"community-images".into()).expect("two community images are visible").bounds();
                            for (value,key) in [(actual.origin.x.as_f32(),"x"),(actual.origin.y.as_f32(),"y"),(actual.size.width.as_f32(),"w"),(actual.size.height.as_f32(),"h")] {assert!((value-expected[key].as_f64().unwrap() as f32).abs()<0.6,"source community {key}: {actual:?}");}
                            let position=point(px(400.),px(200.));
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                            assert!(view.read(cx).dialog.is_some(),"clicking an image keeps the original modal open");
                        }).unwrap();
                    }
                    if matches!(snapshot,"font-size-preview"|"font-size-cleared") {
                        let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/font-size-preview-source.json")).unwrap();
                        for case in oracle["records"].as_array().unwrap().iter().take(if snapshot=="font-size-preview"{2}else{5}) {
                            let step=case["step"].as_str().unwrap();
                            if let Some(raw)=step.strip_prefix("input:") {
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{app.qa_focus_number("general.font_size",window,cx);app.qa_type_connection_input("general.font_size",raw,window,cx);})).unwrap();
                                cx.background_executor().timer(Duration::from_millis(30)).await;
                            }
                            if step=="enter:0" {
                                cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("enter").unwrap(),is_held:false,prefer_character_input:false}),cx)).unwrap();
                                cx.background_executor().timer(Duration::from_millis(30)).await;
                            }
                            cx.update_window(window.into(),|_,window,cx|{
                                window.refresh();let _=window.draw(cx);
                                let expected=case["metrics"].as_array().unwrap().iter().find(|item|item["class"]=="n-tree-node-content__text").unwrap();
                                view.read(cx).qa_assert_font_size(expected["fontSize"].as_str().unwrap().trim_end_matches("px").parse().unwrap(),cx);
                                let actual=gpui_base::test_support::find(window,&[],&"font-label-/localhost".into()).unwrap().bounds();
                                for (value,key) in [(actual.origin.x,"x"),(actual.origin.y,"y"),(actual.size.width,"w"),(actual.size.height,"h")] {
                                    assert!((value.as_f32()-expected[key].as_f64().unwrap() as f32).abs()<0.7,"source font preview {key}: {value:?} versus {}",expected[key]);
                                }
                            }).unwrap();
                        }
                        if snapshot=="font-size-cleared" {
                            let original=window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.close_dialog(window,cx);
                                assert_eq!(app.pref.general.font_size,oracle["cancel"]["value"].as_f64().unwrap() as f32,"cancel restores source preference");
                                app.qa_assert_font_size(14.,cx);
                                let original=app.pref.clone();
                                app.pref.editor.font_size=0.;app.pref.cli.font_size=0.;
                                assert_eq!(app.editor_font_size(),14.);assert_eq!(app.cli_font_size(),14.);
                                app.pref=original.clone();app.open_dialog(dialogs::DialogKind::Preferences,window,cx);
                                app.set_dialog_field("editor.font_size","",window,cx);app.set_dialog_field("cli.font_size","",window,cx);
                                assert_eq!(app.editor_font_size(),14.);assert_eq!(app.cli_font_size(),14.);
                                let fonts:Json=serde_json::from_str(include_str!("../../tests/fixtures/font-family-source.json")).unwrap();
                                for case in fonts["records"].as_array().unwrap() {
                                    app.set_dialog_field("general.font_family",case["families"].to_string(),window,cx);
                                    assert_eq!(cx.theme().font_family.as_ref(),".SystemUIFont","source title bar and modal keep the default font");
                                    assert_eq!(app.general_font_stack(cx),serde_json::from_value::<Vec<String>>(case["families"].clone()).unwrap(),"font preview is independent of empty numeric fields");
                                    assert_eq!(app.preference_font("general",".SystemUIFont",cx).family.as_ref(),if case["families"].as_array().unwrap().is_empty(){".SystemUIFont"}else{"Monaco"},"source installed font fallback order");
                                }
                                app.close_dialog(window,cx);app.open_dialog(dialogs::DialogKind::Preferences,window,cx);
                                original
                            })).unwrap();
                            for raw in ["18",""] {
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{app.qa_focus_number("general.font_size",window,cx);app.qa_type_connection_input("general.font_size",raw,window,cx);})).unwrap();
                                cx.background_executor().timer(Duration::from_millis(30)).await;
                            }
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.submit_dialog(window,cx);
                                assert!(app.dialog.is_none(),"empty general font saves and closes");
                                assert_eq!(app.pref.general.font_size,0.,"source Go backend stores null numeric preference as zero");
                                app.qa_assert_font_size(18.,cx);
                                assert_eq!(app.store.preferences().unwrap().general.font_size,0.,"saved preference has source zero value");
                                app.pref=original;app.general_css_size=app.pref.general.font_size;app.save_preferences();app.apply_theme(window,cx);
                                app.open_dialog(dialogs::DialogKind::Preferences,window,cx);
                            })).unwrap();
                            for raw in ["18",""] {
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{app.qa_focus_number("general.font_size",window,cx);app.qa_type_connection_input("general.font_size",raw,window,cx);})).unwrap();
                                cx.background_executor().timer(Duration::from_millis(30)).await;
                            }
                        }
                        println!("QA PASS: source general font preview and cleared-input CSS retention; actual tree geometry and unchanged modal font");
                    }
                    if snapshot=="preferences" {
                        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/preference-number-source.json")).unwrap();
                        let go:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/preference-number-go-source.json")).unwrap();
                        for (case,rpc) in oracle["records"].as_array().unwrap().iter().zip(go.as_array().unwrap()) {
                            let field=case["field"].as_str().unwrap().replace("fontSize","font_size").replace("scanSize","scan_size");
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.close_dialog(window,cx);app.open_dialog(dialogs::DialogKind::Preferences,window,cx);
                                app.dialog.as_mut().unwrap().tab=field.split('.').next().unwrap().into();
                                app.qa_focus_number(&field,window,cx);
                                app.qa_type_connection_input(&field,case["raw"].as_str().unwrap(),window,cx);
                            })).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,_,cx|view.read(cx).qa_assert_number_text(&field,case["input"].as_str().unwrap(),cx)).unwrap();
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("enter").unwrap(),is_held:false,prefer_character_input:false}),cx);}).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.qa_assert_number_text(&field,case["enter"].as_str().unwrap(),cx);
                                app.qa_assert_number_focus(&field,case["enterFocus"]["editing"].as_bool().unwrap(),window,cx);
                                app.number_event(&field,&InputEvent::Blur,window,cx);
                                app.qa_assert_number_text(&field,case["blurred"].as_str().unwrap(),cx);
                                app.qa_assert_number_value(&field,&case["value"],cx);
                                app.qa_assert_preference_rpc(rpc["success"].as_bool().unwrap(),cx);
                                if rpc["success"]==false {
                                    app.submit_dialog(window,cx);
                                    assert!(app.dialog.is_some(),"rejected preference parameters keep the dialog open");
                                    assert_eq!(app.notifications.last().unwrap().text,rpc["error"].as_str().unwrap(),"original Go parsing error becomes a closable notification");
                                    app.notifications.clear();
                                }
                            })).unwrap();
                        }
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{app.close_dialog(window,cx);app.open_dialog(dialogs::DialogKind::Preferences,window,cx);})).unwrap();
                        println!("QA PASS: 19 original preference numbers, Enter focus, blur handler and Go RPC acceptance");
                    }
                    if snapshot=="connection-numbers" {
                        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/number-input-source.json")).unwrap();
                        for case in oracle["records"].as_array().unwrap() {
                            let field=match case["field"].as_str().unwrap(){"connTimeout"=>"conn_timeout","execTimeout"=>"exec_timeout","loadSize"=>"load_size",field=>field};
                            let raw=case["raw"].as_str().unwrap();
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.close_dialog(window,cx);app.open_dialog(dialogs::DialogKind::Connection(None),window,cx);
                                if field=="ssh.port" {app.set_dialog_field("ssh.enable","true",window,cx);}
                                if field=="proxy.port" {app.set_dialog_field("proxy.type","2",window,cx);app.set_dialog_field("proxy.port","8080",window,cx);}
                                app.dialog.as_mut().unwrap().tab=match field{"conn_timeout"|"exec_timeout"|"load_size"=>"advanced","ssh.port"=>"ssh","proxy.port"=>"proxy",_=>"general"}.into();
                                app.qa_focus_number(field,window,cx);
                                app.qa_type_connection_input(field,raw,window,cx);
                            })).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            if let Some(expected)=case["input"].as_str(){window.update(cx,|_,_,cx|view.read(cx).qa_assert_number_text(field,expected,cx)).unwrap();}
                            let key=case["key"].as_str().map(|key|if key=="ArrowUp"{"up"}else{"down"}).unwrap_or("enter");
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse(key).unwrap(),is_held:false,prefer_character_input:false}),cx);}).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                let expected=case[if case["key"].is_string(){"text"}else{"enter"}].as_str().unwrap();app.qa_assert_number_text(field,expected,cx);
                                app.qa_assert_number_focus(field,true,window,cx);
                                if let Some(blurred)=case["blurred"].as_str(){app.number_event(field,&InputEvent::Blur,window,cx);app.qa_assert_number_text(field,blurred,cx);app.qa_assert_number_value(field,&case["value"],cx);}
                            })).unwrap();
                        }
                        println!("QA PASS: 41 original number inputs, Enter and arrow events; blur handler and effective values");
                    }
                    if snapshot.starts_with("message-") || snapshot.starts_with("notification-") {
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/notice-source.json")).unwrap();
                            let record=oracle["records"].as_array().unwrap().iter().find(|record|record["name"]==snapshot).unwrap();
                            let app=view.read(cx);
                            let actual=if snapshot.starts_with("message-") {
                                app.messages.iter().map(|notice|gpui_base::test_support::find(window,&[],&("message",notice.id).into()).unwrap().bounds()).collect::<Vec<_>>()
                            } else {
                                app.notifications.iter().map(|notice|gpui_base::test_support::find(window,&[],&("error-notification",notice.id).into()).unwrap().bounds()).collect::<Vec<_>>()
                            };
                            let expected=record["metrics"].as_array().unwrap().iter().filter(|item|item["class"].as_str().unwrap().starts_with(if snapshot.starts_with("message-"){"n-message n-message--"}else{"n-notification n-notification--"})).collect::<Vec<_>>();
                            assert_eq!(actual.len(),expected.len());
                            for (actual,expected) in actual.iter().zip(expected) {
                                println!("QA NOTICE {snapshot}: {actual:?}");
                                for (value,key) in [(actual.origin.x,"x"),(actual.origin.y,"y"),(actual.size.width,"w"),(actual.size.height,"h")] {
                                    assert!((value.as_f32()-expected[key].as_f64().unwrap() as f32).abs()<0.7,"source {snapshot} {key}: {value:?} versus {}",expected[key]);
                                }
                            }
                        }).unwrap();
                    }
                    let path = output_dir.join(format!("{snapshot}.png"));
                    let result = cx.update_window(window.into(), |_, window, cx| {
                        window.refresh();
                        let _ = window.draw(cx);
                        window
                            .render_to_image()?
                            .save(&path)
                            .map_err(anyhow::Error::from)
                    });
                    match result {
                        Ok(Ok(())) => println!("{}", path.display()),
                        other => panic!("QA render failed {snapshot}: {other:?}"),
                    }
                    if let Some((stored,effective,families))=restore_entry_style {
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                            app.pref.general.font_size=stored;app.general_css_size=effective;app.pref.general.font_family=families;app.apply_theme(window,cx);
                        })).unwrap();
                    }
                    if let Some((stored,effective))=restore_footer_font {
                        window.update(cx,|_,_,cx|view.update(cx,|app,_|{
                            app.pref.general.font_size=stored;app.general_css_size=effective;
                        })).unwrap();
                    }
                    if snapshot.starts_with("entry-picker-") {
                        dialogs::entry_picker_qa::verify(&view,window,snapshot,cx).await;
                        window.update(cx,|_,_,cx|view.update(cx,|app,_|app.pref.decoder=qa_decoders.clone())).unwrap();
                    }
                    if let Some(control) = entry_conversion_qa {
                        dialogs::entry_conversion_qa::verify(control,&view,window,snapshot,cx).await;
                    }
                    if snapshot.starts_with("entry-preferences") {
                        dialogs::entry_preferences_qa::verify(&view,window,snapshot,cx).await;
                        window.update(cx,|_,_,cx|view.update(cx,|app,_|app.pref.decoder=qa_decoders.clone())).unwrap();
                    }
                    if snapshot == "csv-import-progress-success" {
                        dialogs::csv_transfer::qa_verify_transfers(&view, window, cx).await;
                    }
                    if snapshot == "csv-export-select-plain" {
                        dialogs::csv_selection_qa::verify_order(&view,window,cx).await;
                    }
                    if snapshot == "csv-unit-day" {
                        dialogs::csv_units_qa::verify_behaviour(&view,window,cx).await;
                    }
                    if snapshot == "csv-list-start" {
                        dialogs::csv_list_qa::verify_large_list(&view,window,cx).await;
                    }
                    if snapshot.starts_with("refresh-") {
                        window.update(cx, |_, window, cx| {
                            view.update(cx, |app, cx| {
                                let i = app.index().unwrap();
                                app.tabs[i].auto_refresh = false;
                                app.focus.focus(window, cx);
                            });
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent {
                                position: point(px(1.), px(1.)), ..Default::default()
                            }), cx);
                        }).unwrap();
                    }
                    if let Some(original) = restore_string {
                        window.update(cx, |_, _, cx| view.update(cx, |app, _| {
                            let i = app.index().unwrap();
                            app.service.set_string(&app.tabs[i].conn, 0, b"aastring", original, false).unwrap();
                            app.tabs[i].value_saving = false;
                        })).unwrap();
                    }
                    if snapshot=="content-nav-narrow" {
                        let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/content-header-behavior-source.json")).unwrap();
                        let checks=oracle["checks"].as_array().unwrap();let wheel=checks.iter().find(|c|c["name"]=="vertical-wheel").unwrap();
                        cx.update_window(window.into(),|_,window,cx|{
                            let position=gpui_base::test_support::find(window,&[],&"content-nav".into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent{position,delta:ScrollDelta::Pixels(point(px(0.),px(-wheel["delta"]["y"].as_f64().unwrap() as f32))),..Default::default()}),cx);
                            window.refresh();let _=window.draw(cx);
                        }).unwrap();
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert_eq!(app.tabs[app.index().unwrap()].content_nav_offset.get(),wheel["offset"].as_f64().unwrap() as f32);}).unwrap();
                        for(pane,name)in[(Pane::Pubsub,"reveal-last"),(Pane::Status,"reveal-first")] {
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let id=app.tabs[app.index().unwrap()].id;app.switch_pane(id,pane,window,cx);})).unwrap();
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);}).unwrap();
                            window.update(cx,|_,_,cx|{let app=view.read(cx);assert_eq!(app.tabs[app.index().unwrap()].content_nav_offset.get(),checks.iter().find(|c|c["name"]==name).unwrap()["offset"].as_f64().unwrap() as f32);}).unwrap();
                        }
                        cx.update_window(window.into(),|_,window,cx|{let position=gpui_base::test_support::find(window,&[],&"pane-key_detail".into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);}).unwrap();
                        window.update(cx,|_,_,cx|assert!(view.read(cx).tabs[view.read(cx).index().unwrap()].pane==Pane::Detail)).unwrap();
                        println!("QA PASS: original navigation wheel, selected tab reveal and actual tab click");
                    }
                    if snapshot=="content-nav-key_detail" {
                        let original=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
                        cx.update_window(window.into(),|_,window,cx|{
                            let position=gpui_base::test_support::find(window,&[],&"key-name-text".into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:2,..Default::default()}),cx);
                        }).unwrap();
                        let actual=window.update(cx,|_,_,cx|copy_result(original,cx)).unwrap();assert!(actual.as_deref()==Some("aahash"),"double click must copy the fixture key");_copied_text=actual;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.messages.iter().any(|message|message.text==app.tr("interface.copy_succ")));}).unwrap();
                        println!("QA PASS: original key-name double click copies the displayed key with success feedback");
                    }
                    if snapshot=="key-toolbar-binary" {
                        let click=|id:ElementId,cx:&mut AsyncApp|{cx.update_window(window.into(),|_,window,cx|{let position=gpui_base::test_support::find(window,&[],&id).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);}).unwrap();};
                        click("rename-key".into(),cx);window.update(cx,|_,_,cx|assert!(view.read(cx).dialog.is_none(),"binary keys cannot open rename")).unwrap();
                        let original=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
                        click("copy-key".into(),cx);let actual=window.update(cx,|_,_,cx|copy_result(original,cx)).unwrap();assert!(actual.as_deref()==Some(display_bytes(&[255,0,65]).as_str()),"copy must produce the displayed fixture key");_copied_text=actual;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.messages.iter().any(|message|message.text==app.tr("interface.copy_succ")));}).unwrap();
                        println!("QA PASS: binary key rename is disabled and copy writes the displayed key with success feedback");
                    }
                    if snapshot.contains("tooltip") && snapshot.starts_with("footer-") {
                        let position=window.update(cx,|_,window,_|gpui_base::test_support::find(window,&[],&"footer-tooltip".into()).unwrap().bounds().center()).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(180)).await;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.tabs[app.index().unwrap()].footer_tooltip.is_some(),"hovering the tooltip retains it");}).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(600.),px(500.)),..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(180)).await;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.tabs[app.index().unwrap()].footer_tooltip.is_none(),"leaving hides the tooltip");}).unwrap();
                        cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);
                            let name=if snapshot.contains("format"){ "format" }else{ "decode" };
                            let position=gpui_base::test_support::find(window,&[],&name.into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx);
                        }).unwrap();
                        cx.background_executor().timer(Duration::from_millis(180)).await;
                        cx.update_window(window.into(),|_,window,cx|{
                            let name=if snapshot.contains("format"){ "format" }else{ "decode" };
                            let position=gpui_base::test_support::find(window,&[],&name.into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                        }).unwrap();
                        window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(tab.footer_picker.is_some());assert!(tab.footer_tooltip.is_none(),"opening menu hides tooltip");}).unwrap();
                        println!("QA PASS: {snapshot} hover handoff, leave dismissal and menu suppression");
                    }
                    if snapshot=="footer-decode-custom" {
                        let extra:Json=serde_json::from_str(include_str!("../../tests/fixtures/value-footer-extra-source.json")).unwrap();
                        window.update(cx,|_,_,cx|{
                            let app=view.read(cx);let labels=app.footer_options(value_footer::PickerKind::Decode).into_iter().flatten().collect::<Vec<_>>();
                            let expected=extra["checks"].as_array().unwrap().iter().find(|check|check["name"]=="custom-options").unwrap()["labels"].as_array().unwrap().iter().map(|label|label.as_str().unwrap().to_string()).collect::<Vec<_>>();
                            assert_eq!(labels,expected,"custom decoders replace built-ins and preserve group order");
                        }).unwrap();
                    }
                    if snapshot=="footer-decode-custom" || snapshot.starts_with("footer-decode-long") {
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                            let i=app.index().unwrap();let id=app.tabs[i].id;app.close_footer_picker(id,window,cx);
                            app.pref.decoder=qa_decoders.clone();app.tabs[i].decode="None".into();
                        })).unwrap();
                    }
                    if snapshot == "footer-format-menu" {
                        let oracle: Json = serde_json::from_str(include_str!("../../tests/fixtures/value-footer-source.json")).unwrap();
                        for (index, key) in ["down", "down"].into_iter().enumerate() {
                            cx.update_window(window.into(), |_, window, cx| {
                                window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent { keystroke: Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false }), cx);
                            }).unwrap();
                            window.update(cx, |_, _, cx| {
                                let app = view.read(cx);
                                let picker = app.tabs[app.index().unwrap()].footer_picker.as_ref().unwrap();
                                let options = app.footer_options(picker.kind);
                                assert_eq!(options[picker.cursor.unwrap()].as_deref(), oracle["checks"][index]["pending"][0].as_str());
                            }).unwrap();
                        }
                        cx.update_window(window.into(), |_, window, cx| {
                            window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent { keystroke: Keystroke::parse("enter").unwrap(), is_held: false, prefer_character_input: false }), cx);
                        }).unwrap();
                        for _ in 0..50 {
                            if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap() { break; }
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                        }
                        window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                            let i = app.index().unwrap();
                            assert!(app.tabs[i].footer_picker.is_none());
                            assert_eq!(app.tabs[i].format, "JSON");
                            app.change_format(app.tabs[i].id, None, Some("Raw".into()), window, cx);
                        })).unwrap();
                        println!("QA PASS: original footer keyboard selects Raw, then JSON, and confirms");
                    }
                    if snapshot == "footer-decode-menu" {
                        cx.update_window(window.into(), |_, window, cx| {
                            window.refresh(); let _ = window.draw(cx);
                            let index = view.read(cx).footer_options(value_footer::PickerKind::Decode).len() - 1;
                            let position = gpui_base::test_support::find(window, &[], &("footer-option", index).into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                        }).unwrap();
                        window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                            assert!(app.tabs[app.index().unwrap()].footer_picker.is_none());
                            let dialog = app.dialog.as_ref().unwrap();
                            assert_eq!(dialog.tab, "decoder");
                            app.close_dialog(window, cx);
                        })).unwrap();
                        println!("QA PASS: footer custom decoder action opens decoder preferences");
                    }
                    if snapshot == "entries-hash" {
                        let oracle: Json = serde_json::from_str(include_str!("../../tests/fixtures/entry-delete-source.json")).unwrap();
                        let (service, connection, db, id) = window.update(cx, |_, _, cx| {
                            let app = view.read(cx);
                            let tab = &app.tabs[app.index().unwrap()];
                            (app.service.clone(), tab.conn.clone(), tab.db, tab.id)
                        }).unwrap();
                        for check in oracle["lastEntryChecks"].as_array().unwrap() {
                            let kind = KeyType::parse(check["kind"].as_str().unwrap());
                            let key = format!("__rdm_qa_last_{}__", check["kind"].as_str().unwrap()).into_bytes();
                            let fixture_key = key.clone();
                            let fixture_service = service.clone();
                            let fixture_connection = connection.clone();
                            let fixture_kind = kind.clone();
                            cx.background_executor().spawn(async move {
                                fixture_service.run(&fixture_connection, db, vec![b"DEL".to_vec(), fixture_key.clone()])?;
                                fixture_service.create(&fixture_connection, db, &fixture_key, fixture_kind.clone(), b"1720000000000-0", &[Entry {
                                    id: b"name".to_vec(),
                                    value: if matches!(fixture_kind, KeyType::List | KeyType::Set) { b"Rust".to_vec() } else { b"Tiny RDM".to_vec() },
                                    score: "1".into(),
                                    ..Default::default()
                                }], -1)
                            }).await.unwrap();
                            window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                                app.messages.clear();
                                app.select_key(id, key.clone(), window, cx);
                            })).unwrap();
                            for _ in 0..50 {
                                if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap() { break; }
                                cx.background_executor().timer(Duration::from_millis(100)).await;
                            }
                            window.update(cx, |_, _, cx| {
                                let app = view.read(cx);
                                assert_eq!(app.tabs[app.index().unwrap()].detail.as_ref().unwrap().rows.len(), 1);
                            }).unwrap();
                            for name in ["entry-delete-0", "entry-delete-confirm"] {
                                cx.update_window(window.into(), |_, window, cx| {
                                    window.refresh(); let _ = window.draw(cx);
                                    let position = gpui_base::test_support::find(window, &[], &name.into()).unwrap().bounds().center();
                                    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                                    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }), cx);
                                }).unwrap();
                            }
                            for _ in 0..50 {
                                if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap() { break; }
                                cx.background_executor().timer(Duration::from_millis(100)).await;
                            }
                            window.update(cx, |_, _, cx| {
                                let app = view.read(cx);
                                let tab = &app.tabs[app.index().unwrap()];
                                assert!(tab.entry_delete.is_none());
                                if check["state"]["tab"]["subTab"] == "status" {
                                    assert!(matches!(tab.pane, Pane::Status));
                                    assert!(tab.current_key.is_none());
                                    assert!(tab.detail.is_none());
                                } else {
                                    assert!(matches!(tab.pane, Pane::Detail));
                                    assert_eq!(tab.current_key.as_ref(), Some(&key));
                                    let detail = tab.detail.as_ref().unwrap();
                                    assert_eq!(detail.kind, KeyType::Stream);
                                    assert!(detail.rows.is_empty());
                                    assert_eq!(detail.length, 0);
                                }
                                let messages = app.messages.iter().map(|message| message.text.as_str()).collect::<Vec<_>>();
                                let expected = check["messages"].as_array().unwrap().iter().map(|message| message.as_str().unwrap()).collect::<Vec<_>>();
                                assert_eq!(messages, expected, "last {:?} deletion messages", kind);
                            }).unwrap();
                            let fixture_service = service.clone();
                            let fixture_connection = connection.clone();
                            cx.background_executor().spawn(async move {
                                let exists = fixture_service.run(&fixture_connection, db, vec![b"EXISTS".to_vec(), key.clone()])?.integer()?;
                                assert_eq!(exists, i64::from(kind == KeyType::Stream));
                                fixture_service.run(&fixture_connection, db, vec![b"DEL".to_vec(), key])?;
                                Ok::<_, anyhow::Error>(())
                            }).await.unwrap();
                        }
                        window.update(cx, |_, window, cx| view.update(cx, |app, cx| app.select_key(id, b"aahash".to_vec(), window, cx))).unwrap();
                        for _ in 0..50 {
                            if window.update(cx, |_, _, cx| view.read(cx).busy == 0).unwrap() { break; }
                            cx.background_executor().timer(Duration::from_millis(100)).await;
                        }
                        println!("QA PASS: five last-entry deletions match original pages, messages and Redis key lifetime");
                    }
                    if snapshot.starts_with("entries-hash-delete") {
                        let state=window.update(cx,|_,_,cx|{let app=view.read(cx);let i=app.index().unwrap();let tab=&app.tabs[i];assert!(tab.entry_delete.is_some());(app.service.clone(),tab.conn.clone(),tab.db,tab.id,tab.detail.as_ref().unwrap().rows[0].clone(),tab.detail.as_ref().unwrap().rows.clone())}).unwrap();
                        let click=|id:&str,cx:&mut AsyncApp|{cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&id.to_owned().into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();};
                        click("entry-delete-cancel",cx);
                        window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(tab.entry_delete.is_none());assert_eq!(tab.detail.as_ref().unwrap().rows.len(),2);}).unwrap();
                        let (service,connection,db,_,row,_)=state.clone();cx.background_executor().spawn(async move{assert_eq!(service.run(&connection,db,vec![b"HGET".to_vec(),b"aahash".to_vec(),row.id])?.bytes(),row.value);Ok::<_,anyhow::Error>(())}).await.unwrap();
                        click("entry-delete-0",cx);click("entry-delete-confirm",cx);
                        for _ in 0..50 {if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap(){break;}cx.background_executor().timer(Duration::from_millis(100)).await;}
                        window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(tab.entry_delete.is_none());assert_eq!(tab.detail.as_ref().unwrap().rows.len(),1);assert_eq!(tab.current_key.as_deref(),Some(b"aahash".as_slice()));}).unwrap();
                        let(service,connection,db,id,row,original_rows)=state;cx.background_executor().spawn(async move{assert_eq!(service.run(&connection,db,vec![b"HEXISTS".to_vec(),b"aahash".to_vec(),row.id.clone()])?.integer()?,0);assert_eq!(service.run(&connection,db,command(&["HLEN","aahash"]))?.integer()?,1);service.run(&connection,db,command(&["DEL","aahash"]))?;let mut restore=vec![b"HSET".to_vec(),b"aahash".to_vec()];for original in original_rows{restore.push(original.id);restore.push(original.value);}service.run(&connection,db,restore)?;Ok::<_,anyhow::Error>(())}).await.unwrap();
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|app.reload_detail(id,true,window,cx))).unwrap();for _ in 0..50 {if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap(){break;}cx.background_executor().timer(Duration::from_millis(100)).await;}
                        println!("QA PASS: {snapshot} cancel retains Redis data, confirm removes its field, fixture restored");
                    }
                    if matches!(snapshot,"entries-hash-sort"|"entries-zset-sort") {
                        let epoch=window.update(cx,|_,_,cx|{let app=view.read(cx);app.tabs[app.index().unwrap()].detail_epoch}).unwrap();
                        let column=if snapshot=="entries-hash-sort"{"heading-entry-field"}else{"heading-entry-score"};
                        for ascending in [Some(true),None] {
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&column.into()).unwrap().bounds().center();window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.refresh();let _=window.draw(cx);}).unwrap();
                            window.update(cx,|_,_,cx|{let app=view.read(cx);let i=app.index().unwrap();let tab=&app.tabs[i];assert_eq!(tab.detail_epoch,epoch,"sorting must retain loaded entries and avoid Redis reloads");let rows=app.entry_rows(i);assert_eq!(rows.len(),tab.detail.as_ref().unwrap().rows.len());if snapshot=="entries-hash-sort" {assert_eq!(tab.field_sort,ascending);assert_eq!(rows[0].id,if ascending.is_some(){b"language".as_slice()}else{b"name".as_slice()});}else{assert_eq!(tab.score_sort,ascending);assert_eq!(rows[0].value,b"Tiny RDM");}}).unwrap();
                        }
                        println!("QA PASS: {snapshot} original descending, ascending and cleared sort cycle retains loaded rows");
                    }
                    if snapshot=="message-success" {
                        let position=window.update(cx,|_,window,cx|{let app=view.read(cx);gpui_base::test_support::find(window,&[],&("message",app.messages[0].id).into()).unwrap().bounds().center()}).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(3400)).await;
                        window.update(cx,|_,_,cx|assert_eq!(view.read(cx).messages.len(),1,"original message hover retention")).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(0.),px(0.)),..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(3400)).await;
                        window.update(cx,|_,_,cx|assert!(view.read(cx).messages.is_empty(),"original message expiration after leave")).unwrap();
                        println!("QA PASS: actual hover retains message and leaving restarts the original 3-second timer");
                    }
                    if snapshot=="notification-error" {
                        window.update(cx,|_,_,cx|view.update(cx,|app,cx|{app.notifications.clear();for i in 0..6{app.error_notification(format!("通知 {i}"));}cx.notify();})).unwrap();
                        let position=cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);let app=view.read(cx);
                            let oracle:Json=serde_json::from_str(include_str!("../../tests/fixtures/notice-source.json")).unwrap();
                            assert_eq!(app.notifications.iter().map(|notice|notice.text.as_str()).collect::<Vec<_>>(),oracle["behavior"]["notificationLimit"].as_array().unwrap().iter().map(|value|value.as_str().unwrap()).collect::<Vec<_>>());
                            gpui_base::test_support::find(window,&[],&format!("close-notification-{}",app.notifications.last().unwrap().id).into()).unwrap().bounds().center()
                        }).unwrap();
                        cx.update_window(window.into(),|_,window,cx|{window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);}).unwrap();
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert_eq!(app.notifications.len(),4);assert_eq!(app.notifications.last().unwrap().text,"通知 4");}).unwrap();
                        println!("QA PASS: original five-notification limit, order and actual close click above the modal");
                    }
                    if snapshot=="connection-alias-row" {
                        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/alias-number-source.json")).unwrap();
                        for case in oracle["records"].as_array().unwrap() {
                            let field=window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                let key=app.qa_prepare_alias_number(window,cx);app.qa_focus_number(&key,window,cx);
                                app.qa_type_connection_input(&key,case["raw"].as_str().unwrap(),window,cx);key
                            })).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,_,cx|view.read(cx).qa_assert_number_text(&field,case["input"].as_str().unwrap(),cx)).unwrap();
                            cx.update_window(window.into(),|_,window,cx|{window.refresh();let _=window.draw(cx);window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("enter").unwrap(),is_held:false,prefer_character_input:false}),cx);}).unwrap();
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                app.qa_assert_number_text(&field,case["enter"].as_str().unwrap(),cx);
                                app.number_event(&field,&InputEvent::Blur,window,cx);
                                app.qa_assert_number_text(&field,case["blurred"].as_str().unwrap(),cx);
                                app.qa_assert_alias_number(&case["alias"],cx);
                            })).unwrap();
                        }
                        println!("QA PASS: 9 original alias numbers and integer map acceptance");
                    }
                    if snapshot=="community" {
                        for keyboard in [true,false] {
                            cx.update_window(window.into(),|_,window,cx|{
                                if !keyboard {view.update(cx,|app,cx|app.open_dialog(dialogs::DialogKind::Community,window,cx));}
                                window.refresh();let _=window.draw(cx);
                                if keyboard {window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse("escape").unwrap(),is_held:false,prefer_character_input:false}),cx);}else{
                                    let position=point(px(900.),px(400.));
                                    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                }
                            }).unwrap();
                            cx.background_executor().timer(Duration::from_millis(50)).await;
                            window.update(cx,|_,_,cx|assert!(view.read(cx).dialog.is_none(),"community Escape and mask close")).unwrap();
                        }
                        println!("QA PASS: community source image geometry, image click, Escape and mask close");
                    }
                    if matches!(snapshot,"connections-selected"|"connections-connected"|"connections-group-open") {
                        let click=|x:f32,y:f32,cx:&mut AsyncApp|{
                            cx.update_window(window.into(),|_,window,cx|{
                                window.refresh();let _=window.draw(cx);let position=point(px(x),px(y));
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                                window.refresh();let _=window.draw(cx);
                            }).unwrap();
                        };
                        if snapshot=="connections-selected" {
                            for (x,kind) in [(327.,"delete"),(306.,"edit")] {
                                click(x,55.,cx);
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                    app.qa_assert_connection_tree_dialog(kind,"localhost",cx);
                                    app.close_dialog(window,cx);
                                })).unwrap();
                            }
                            click(285.,55.,cx);
                            for _ in 0..50 {if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap_or(false){break;}cx.background_executor().timer(Duration::from_millis(100)).await;}
                            window.update(cx,|_,_,cx|assert_eq!(view.read(cx).tabs.len(),1,"inline open action connects to the fixture")).unwrap();
                        } else if snapshot=="connections-connected" {
                            click(306.,55.,cx);
                            window.update(cx,|_,_,cx|assert!(view.read(cx).tabs.is_empty(),"inline disconnect closes the connection")).unwrap();
                        } else {
                            for (x,kind) in [(306.,"group"),(327.,"delete-group")] {
                                click(x,85.,cx);
                                window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                                    app.qa_assert_connection_tree_dialog(kind,"开发环境",cx);app.close_dialog(window,cx);
                                })).unwrap();
                            }
                        }
                        println!("QA PASS: {snapshot} inline actions use source button coordinates");
                    }
                    if snapshot == "welcome" {
                        let original = window.update(cx, |_, _, cx| view.read(cx).pref.clone()).unwrap();
                        for (choice, x) in [(false, 866.), (true, 952.)] {
                            let _ = cx.update_window(window.into(), |_, window, cx| {
                                view.update(cx, |app, cx| { app.pref.behavior.welcomed = false; cx.notify(); });
                                window.refresh();
                                let _ = window.draw(cx);
                                let position = point(px(x), px(686.));
                                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent {
                                    button: MouseButton::Left, position, click_count: 1, ..Default::default()
                                }), cx);
                                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent {
                                    button: MouseButton::Left, position, click_count: 1, ..Default::default()
                                }), cx);
                            });
                            cx.background_executor().timer(Duration::from_millis(50)).await;
                            window.update(cx, |_, _, cx| {
                                let app = view.read(cx);
                                assert!(app.pref.behavior.welcomed, "click closes the welcome notification");
                                let stored = app.store.preferences().unwrap();
                                assert!(stored.behavior.welcomed);
                                assert_eq!(stored.general.allow_track, choice);
                            }).unwrap();
                        }
                        window.update(cx, |_, _, cx| view.update(cx, |app, _| {
                            app.pref = original;
                            app.pref.behavior.welcomed = true;
                            app.save_preferences();
                        })).unwrap();
                        println!("QA PASS: welcome buttons persist acceptance and rejection without redisplaying");
                    }
                    if snapshot == "sponsor" {
                        window.update(cx, |_, _, cx| view.update(cx, |app, _| {
                            let old = app.store.load::<Vec<crate::update::Sponsor>>("sponsors.yaml").unwrap();
                            let items = app.sponsors.clone();
                            app.cache_sponsors(&items);
                            app.cache_sponsors(&[]);
                            assert_eq!(app.sponsors, items);
                            assert_eq!(app.store.load::<Vec<crate::update::Sponsor>>("sponsors.yaml").unwrap(), items);
                            app.store.save("sponsors.yaml", &old).unwrap();
                        })).unwrap();
                        println!("QA PASS: sponsor cache survives empty update metadata");
                    }
                    if snapshot=="key-browser-delete" {
                        let click=|name:&'static str,cx:&mut AsyncApp| {cx.update_window(window.into(),|_,window,cx|{
                            window.refresh();let _=window.draw(cx);let position=gpui_base::test_support::find(window,&[],&name.into()).unwrap().bounds().center();
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position,click_count:1,..Default::default()}),cx);
                        }).unwrap();};
                        click("dialog-cancel",cx);
                        let (service,connection,db)=window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.dialog.is_none());let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.current_key.as_deref(),Some(b"aahash".as_slice()));(app.service.clone(),tab.conn.clone(),tab.db)}).unwrap();
                        let fixture=connection.clone();let service_copy=service.clone();
                        cx.background_executor().spawn(async move{service_copy.run(&fixture,db,command(&["SET","__rdm_qa_delete_confirm__","owned-test-key"]))}).await.unwrap();
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|app.open_dialog(dialogs::DialogKind::DeleteKey(b"__rdm_qa_delete_confirm__".to_vec()),window,cx))).unwrap();
                        click("dialog-save",cx);
                        for _ in 0..30 {if window.update(cx,|_,_,cx|view.read(cx).busy==0).unwrap(){break;}cx.background_executor().timer(Duration::from_millis(50)).await;}
                        cx.background_executor().spawn(async move{assert_eq!(service.run(&connection,db,command(&["EXISTS","__rdm_qa_delete_confirm__"]))?.integer()?,0);assert_eq!(service.run(&connection,db,command(&["TYPE","aahash"]))?.text(),"hash");Ok::<_,anyhow::Error>(())}).await.unwrap();
                        window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert!(app.dialog.is_none());assert!(tab.current_key.is_none());assert!(tab.detail.is_none());assert!(matches!(tab.pane,Pane::Status));}).unwrap();
                        println!("QA PASS: single-key cancel preserves data; confirmation deletes its captured target and preserves the prior detail key");
                    }
                    if snapshot=="key-type-menu"||snapshot=="key-type-menu-dark" {
                        let position=window.update(cx,|_,window,_|gpui_base::test_support::find(window,&[],&"key-type-options".into()).unwrap().bounds().center()).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(130)).await;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(app.tabs[app.index().unwrap()].type_picker.open,"moving into the menu keeps it open");}).unwrap();
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:point(px(1000.),px(700.)),..Default::default()}),cx)).unwrap();
                        cx.background_executor().timer(Duration::from_millis(150)).await;
                        window.update(cx,|_,_,cx|{let app=view.read(cx);assert!(!app.tabs[app.index().unwrap()].type_picker.open,"leaving the trigger and menu closes it");}).unwrap();
                        println!("QA PASS: type hover survives pointer transfer and closes after leaving");
                    }
                    if matches!(snapshot,"key-database-empty"|"key-database-keyboard") {
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent{keystroke:Keystroke::parse(if snapshot=="key-database-empty"{"enter"}else{"escape"}).unwrap(),is_held:false,prefer_character_input:false}),cx)).unwrap();
                        window.update(cx,|_,_,cx|{let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];assert_eq!(tab.db,0);assert_eq!(tab.database_picker.open,snapshot=="key-database-empty");if snapshot=="key-database-keyboard"{assert_eq!(text_input(&tab.database_picker.query,cx),"");}}).unwrap();
                        println!("QA PASS: database keyboard cancellation and unmatched confirmation keep the current database");
                    }
                    if snapshot.starts_with("key-type-") || snapshot.starts_with("key-database-") {
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{let i=app.index().unwrap();let id=app.tabs[i].id;app.close_database_picker(id,window,cx);app.close_type_picker(id,cx);app.tabs[i].key_type.clear();if app.tabs[i].db!=0{app.change_database(id,0,window,cx);}else{app.scan_keys(id,true,false,window,cx);}if let Some(db)=app.tabs[i].databases.iter_mut().find(|db|db.index==1){db.alias.clear();}})).unwrap();
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                    }
                    if let Some(saved) = status_values_restore { status_values_qa::restore(&view,window,saved,cx); }
                    if let Some(saved) = status_layout_restore { status_layout_qa::restore(&view,window,saved,cx); }
                    if let Some(saved) = status_info_restore { status_info_qa::restore(&view,window,saved,cx); }
                    if let Some(saved) = status_clients_restore { status_clients_qa::restore(&view,window,saved,cx).await; }
                    if snapshot.starts_with("match-tooltip-") || snapshot.starts_with("entry-match-width-") || snapshot.starts_with("entry-control-") { match_tooltip_qa::restore(&view,window,cx).await; }
                    if snapshot.starts_with("key-filter-") || snapshot.starts_with("tag-style-") || snapshot.starts_with("tag-overflow-") || snapshot.starts_with("sidebar-resize-") || snapshot.starts_with("sidebar-contract-") || snapshot.starts_with("selector-font-") || snapshot.starts_with("general-fallback-") || snapshot.starts_with("css-label-") {key_filter_qa::restore(&view,window,snapshot,cx).await;}
                    if snapshot.starts_with("key-browser") {
                        window.update(cx,|_,window,cx|view.update(cx,|app,cx|{
                            let i=app.index().unwrap();let tab=&mut app.tabs[i];let id=tab.id;
                            tab.check_mode=false;tab.selected.clear();tab.key_match.clear();tab.key_filter.clear();tab.expanded.clear();tab.key_selection=None;tab.key_filter_epoch+=1;
                            set_input(&tab.filter,"",window,cx);app.scan_keys(id,true,false,window,cx);
                        })).unwrap();
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                    }
                    if snapshot == "preferences-font-tags" {
                        let _ = cx.update_window(window.into(), |_, window, cx| {
                            view.update(cx, |app, cx| {
                                app.qa_type_font_query("general.font_family", "Monaco", window, cx);
                            });
                            let _ = window.draw(cx);
                            window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
                                keystroke: Keystroke::parse("escape").unwrap(),
                                is_held: false, prefer_character_input: false,
                            }), cx);
                        });
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                        window.update(cx, |_, _, cx| {
                            let app = view.read(cx);
                            assert!(app.dialog.is_none());
                            assert!(app.pref.general.font_family.is_empty());
                            assert_eq!(cx.theme().font_family.as_ref(), ".SystemUIFont");
                        }).unwrap();
                        println!("QA PASS: Escape closes preferences and restores font preview");
                    }
                }
                let _ = cx.update(|cx| {
                    cx.quit();
                });
            })
            .detach();
        });
}
