use super::new_key_qa::{click, draw};
use super::*;

fn source(scene: &str) -> Json {
    let all: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/edit-entry-layout-source.json"
    ))
    .unwrap();
    all["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene)
        .unwrap()
        .clone()
}

impl RdmApp {
    pub(in crate::ui) fn qa_prepare_entry_panel(
        &mut self,
        scene: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let record = source(scene);
        let kind = if scene.starts_with("edit-entry-list") {
            KeyType::List
        } else if scene.ends_with("-set") {
            KeyType::Set
        } else if scene.ends_with("-zset") {
            KeyType::ZSet
        } else {
            KeyType::Hash
        };
        let key = match kind {
            KeyType::List => "aalist",
            KeyType::Set => "aaset",
            KeyType::ZSet => "aazset",
            _ => "aahash",
        };
        self.pref.general.font_family.clear();
        self.pref.general.font.clear();
        self.pref.general.font_size = 14.;
        self.general_css_size = 14.;
        self.pref.editor.font_family.clear();
        self.pref.editor.font.clear();
        self.pref.editor.font_size = 14.;
        self.pref.general.theme = if scene.contains("dark") {
            "dark"
        } else {
            "light"
        }
        .into();
        self.pref.editor.entry_text_align = 0;
        self.apply_theme(w, cx);
        self.nav = Nav::Browser;
        let i = self.index().unwrap();
        let rows = if kind == KeyType::Hash {
            vec![
                Entry {
                    id: b"old".to_vec(),
                    value: b"before".to_vec(),
                    ..Default::default()
                },
                Entry {
                    id: b"other".to_vec(),
                    value: b"kept".to_vec(),
                    ..Default::default()
                },
            ]
        } else {
            ["before", "kept"]
                .into_iter()
                .enumerate()
                .map(|(index, value)| Entry {
                    id: if kind == KeyType::List {
                        index.to_string().into_bytes()
                    } else {
                        value.as_bytes().to_vec()
                    },
                    value: value.as_bytes().to_vec(),
                    score: if kind == KeyType::ZSet {
                        if index == 0 { "1" } else { "3" }.into()
                    } else {
                        String::new()
                    },
                    ..Default::default()
                })
                .collect()
        };
        let row = rows[0].clone();
        let tab = &mut self.tabs[i];
        tab.db = 0;
        tab.pane = Pane::Detail;
        tab.current_key = Some(key.as_bytes().to_vec());
        tab.detail_epoch += 1;
        tab.detail = Some(KeyDetail {
            key: key.as_bytes().to_vec(),
            kind,
            ttl: -1,
            memory: 64,
            length: 2,
            end: true,
            rows,
            ..Default::default()
        });
        tab.decode = "None".into();
        tab.format = "Raw".into();
        tab.auto_refresh = false;
        tab.row_match.clear();
        tab.row_filter_value.clear();
        tab.removed_fields.clear();
        tab.row_values.clear();
        tab.field_sort = None;
        tab.score_sort = None;
        tab.column_widths
            .retain(|key, _| !key.starts_with("entry-"));
        set_input(&tab.row_filter, "", w, cx);
        set_input(&tab.key_name, key, w, cx);
        self.open_dialog(DialogKind::EditEntry(row), w, cx);
        let pin = record["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                m["class"]
                    .as_str()
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "pinable-btn"))
            })
            .unwrap()["class"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .any(|c| c == "pin-btn");
        self.set_dialog_field("pin", pin.to_string(), w, cx);
        self.full_editor = scene.contains("full");
        cx.notify();
    }

    pub(in crate::ui) fn qa_finish_entry_panel(&mut self, scene: &str, w: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            scene,
            "edit-entry-hash-draft"
                | "edit-entry-hash-full"
                | "edit-entry-hash-full-dark"
                | "edit-entry-hash-dark"
        ) {
            self.set_dialog_field("value", "line one\nline two", w, cx);
            self.dialog.as_ref().unwrap().editors["value"]
                .focus_handle(cx)
                .focus(w, cx);
            self.dialog.as_ref().unwrap().editors["value"].update(cx, |editor, cx| {
                let length = editor.value().len();
                editor.set_selected_range(length..length, cx);
            });
        }
        if scene.ends_with("field-focus") {
            let key = if scene.starts_with("edit-entry-list") {
                "row_index"
            } else {
                "field"
            };
            self.dialog.as_ref().unwrap().fields[key].update(cx, |field, cx| {
                let length = field.value().len();
                field.set_selected_range(length..length, cx);
                field.focus(w, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::ui) fn qa_assert_entry_panel(&self, scene: &str, w: &Window, cx: &App) {
        let record = source(scene);
        let check = |name: &'static str, expected: &Json| {
            let actual = gpui_base::test_support::find(w, &[], &name.into())
                .unwrap_or_else(|| panic!("{scene}: missing {name}"))
                .bounds();
            for (value, key) in [
                (actual.origin.x, "x"),
                (actual.origin.y, "y"),
                (actual.size.width, "width"),
                (actual.size.height, "height"),
            ] {
                assert!(
                    (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
                    "{scene} {name} {key}: {value:?} vs {}",
                    expected[key]
                );
            }
        };
        check("entry-panel", &record["bounds"]);
        for (name, key) in [
            ("entry-panel-card", "card"),
            ("entry-panel-header", "header"),
            ("entry-panel-title", "title"),
            ("entry-panel-tools", "tools"),
            ("entry-panel-content", "content"),
            ("entry-field-label", "fieldLabel"),
            ("entry-field-input", "input"),
            ("entry-value-label", "valueLabel"),
            ("entry-editor-border", "editorBorder"),
            ("entry-value-editor", "editor"),
            ("entry-format-divider", "divider"),
            ("entry-panel-actions", "actions"),
            ("save-entry", "save"),
            ("pin-entry", "pin"),
            ("full-entry", "full"),
            ("close-entry", "close"),
            ("entry-select-format", "format"),
            ("entry-select-decode", "decode"),
        ] {
            check(name, &record["controls"][key]);
        }
        let dialog = self.dialog.as_ref().unwrap();
        let kind = &dialog.target.as_ref().unwrap().kind;
        let key = if *kind == KeyType::Hash {
            "field"
        } else if *kind == KeyType::ZSet {
            "score"
        } else {
            "row_index"
        };
        assert_eq!(
            dialog.value(key, cx),
            record["field"]["value"].as_str().unwrap()
        );
        if scene.contains("full") {
            let panel = gpui_base::test_support::find(w, &[], &"entry-panel".into())
                .unwrap()
                .bounds();
            for name in ["key-toolbar", "value-footer"] {
                let bounds = gpui_base::test_support::find(w, &[], &name.into())
                    .unwrap()
                    .bounds();
                assert!(panel.contains(&bounds.origin));
                assert!(panel.contains(
                    &(bounds.origin
                        + point(bounds.size.width - px(0.1), bounds.size.height - px(0.1)))
                ));
            }
        }
        println!(
            "QA PASS: {scene} original panel/card/header/input/editor/selectors/actions geometry"
        );
    }
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    super::new_key_qa::idle(view, window, cx).await;
    window.update(cx, |_, w, cx| view.update(cx, |app, cx| app.qa_finish_entry_panel(scene, w, cx))).unwrap();
    if scene == "edit-entry-hash-field-focus" {
        super::new_key_qa::type_field(view, "field", "next-field", window, cx);
        let before = window.update(cx, |_, _, cx| view.read(cx).busy).unwrap();
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("cmd-s").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            )
        })
        .unwrap();
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(app.busy, before);
                assert!(app.editing_entry());
                assert_eq!(
                    app.dialog.as_ref().unwrap().value("field", cx),
                    "next-field"
                );
            })
            .unwrap();
        super::new_key_qa::type_field(view, "field", "old", window, cx);
        println!("ENTRY FIELD SHORTCUT QA PASS: field focus does not submit or close the editor");
    }
    if scene == "edit-entry-hash" {
        let before = window.update(cx, |_, _, cx| view.read(cx).busy).unwrap();
        click("save-entry".into(), window, cx);
        draw(window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert!(!app.entry_changed(cx));
                assert_eq!(app.busy, before);
            })
            .unwrap();
        click("pin-entry".into(), window, cx);
        draw(window, cx).await;
        assert!(
            window
                .update(cx, |_, _, cx| view
                    .read(cx)
                    .dialog
                    .as_ref()
                    .unwrap()
                    .boolean("pin"))
                .unwrap()
        );
        click("full-entry".into(), window, cx);
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                assert!(view.read(cx).full_editor);
                let panel = gpui_base::test_support::find(w, &[], &"entry-panel".into())
                    .unwrap()
                    .bounds();
                let toolbar = gpui_base::test_support::find(w, &[], &"key-toolbar".into())
                    .unwrap()
                    .bounds();
                assert!(panel.contains(&toolbar.origin));
            })
            .unwrap();
        click("full-entry".into(), window, cx);
        draw(window, cx).await;
        assert!(
            !window
                .update(cx, |_, _, cx| view.read(cx).full_editor)
                .unwrap()
        );
        click("close-entry".into(), window, cx);
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert!(app.dialog.is_none());
                    assert!(!app.full_editor);
                    app.qa_prepare_entry_panel(scene, w, cx);
                })
            })
            .unwrap();
        println!("ENTRY PANEL INPUT QA PASS: disabled save, actual pin/full/restore/close buttons");
        super::new_key_qa::idle(view, window, cx).await;
        window.update(cx, |_, w, cx| view.update(cx, |app, cx| app.qa_finish_entry_panel(scene, w, cx))).unwrap();
    }
    if scene == "edit-entry-list-field-focus" {
        super::new_key_qa::type_field(view, "row_index", "changed", window, cx);
        window
            .update(cx, |_, _, cx| {
                assert_eq!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .value("row_index", cx),
                    "1"
                )
            })
            .unwrap();
        println!("ENTRY PANEL READONLY QA PASS: focused index rejects text changes");
    }
    draw(window, cx).await;
}
