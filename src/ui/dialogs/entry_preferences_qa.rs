use super::new_key_qa::{click, draw, type_field};
use super::*;
use crate::ui::value_footer::PickerKind;
use serde_json::json;

fn source(scene: &str) -> Json {
    let source: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/entry-preferences-source.json"
    ))
    .unwrap();
    source["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene)
        .unwrap()
        .clone()
}

impl RdmApp {
    pub(in crate::ui) fn qa_prepare_entry_preferences(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.qa_prepare_entry_panel("edit-entry-hash", w, cx);
        self.pref.decoder.clear();
    }

    pub(in crate::ui) fn qa_assert_entry_preferences(&self, scene: &str, w: &Window, cx: &App) {
        let source = source(scene);
        let check = |id: &str, expected: &Json| {
            let actual = gpui_base::test_support::find(w, &[], &id.to_owned().into())
                .unwrap_or_else(|| panic!("{scene}: missing {id}"))
                .bounds();
            for (value, key) in [
                (actual.origin.x, "x"),
                (actual.origin.y, "y"),
                (actual.size.width, "width"),
                (actual.size.height, "height"),
            ] {
                assert!(
                    (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
                    "{scene} {id} {key}: {value:?} vs {}",
                    expected[key]
                );
            }
        };
        assert!(self.entry_panel_visible());
        let entry = self.displayed_entry_dialog().unwrap();
        assert_eq!(
            entry.value("field", cx),
            source["field"]["value"].as_str().unwrap()
        );
        assert_eq!(
            entry.value("value", cx),
            source["valueLines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line.as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(entry.entry_changed(cx));
        assert!(self.editing_row(b"old"));
        assert_eq!(
            self.entry_panel_fullscreen(),
            (scene.ends_with("-full") || scene.ends_with("-saved"))
        );
        assert!(
            self.focus.is_focused(w),
            "original dropdown/preferences leave focus on the page"
        );
        check("entry-panel", &source["editor"]);
        check("entry-panel-card", &source["card"]);
        let mut field = source["field"].clone();
        // Source records the HTML input inside its 12px horizontal wrapper.
        field["x"] = json!(field["x"].as_f64().unwrap() - 12.);
        field["width"] = json!(field["width"].as_f64().unwrap() + 24.);
        check("entry-field-input", &field);
        check("entry-value-editor", &source["value"]);
        check("entry-select-format", &source["format"]);
        check("entry-select-decode", &source["decode"]);
        assert!(gpui_base::test_support::find(w, &[], &"footer-selectors".into()).is_none());
        if source["modal"].is_object() {
            assert!(matches!(
                self.dialog.as_ref().unwrap().kind,
                DialogKind::Preferences
            ));
            assert_eq!(self.dialog.as_ref().unwrap().tab, "decoder");
            assert_eq!(self.dialog_stack.len(), 1);
            check("dialog-card", &source["modal"]);
            check("dialog-mask", &source["mask"]);
        } else {
            assert!(self.editing_entry());
            assert!(self.dialog_stack.is_empty());
            assert!(gpui_base::test_support::find(w, &[], &"dialog-mask".into()).is_none());
        }
        println!(
            "QA PASS: {scene} original nested preferences, visible editor, drafts and page focus"
        );
    }
}

fn key(name: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(name).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
}

async fn open_settings(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    click("entry-select-decode".into(), window, cx);
    draw(window, cx).await;
    let last = window
        .update(cx, |_, _, cx| {
            view.read(cx).footer_options(PickerKind::Decode).len() - 1
        })
        .unwrap();
    click(("entry-picker-option", last).into(), window, cx);
    draw(window, cx).await;
}

pub(in crate::ui) async fn prepare(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    super::new_key_qa::idle(view, window, cx).await;
    type_field(view, "field", "draft-field", window, cx);
    super::edit_entries_qa::text(view, "line one\nline two", window, cx);
    draw(window, cx).await;
    if scene.ends_with("-full") || scene.ends_with("-saved") {
        click("full-entry".into(), window, cx);
        draw(window, cx).await;
    }
    open_settings(view, window, cx).await;
    if scene.ends_with("-dark") {
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.pref.general.theme = "dark".into();
                    app.apply_theme(w, cx);
                    cx.notify();
                })
            })
            .unwrap();
    }
    if scene.ends_with("-return") {
        key("escape", window, cx);
    }
    if scene.ends_with("-saved") {
        let (path, original_bytes) = window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let root = &app.store.root;
                    assert_eq!(
                        std::fs::read_to_string(root.join("RDM-QA-FIXTURE")).unwrap(),
                        "rust-rdm-render-fixture-v1\n"
                    );
                    let path = root.join("preferences.yaml");
                    let backup = std::fs::read(&path).ok();
                    let old = serde_json::to_value(&app.pref).unwrap();
                    app.dialog_stack.last_mut().unwrap().data["_qa_original_preferences"] = old;
                    for (name, value) in [
                        ("general.theme", "dark"),
                        ("editor.show_line_num", "false"),
                        ("editor.show_folding", "false"),
                        ("editor.drop_text", "false"),
                        ("editor.links", "false"),
                    ] {
                        app.set_dialog_field(name, value, w, cx);
                    }
                    (path, backup)
                })
            })
            .unwrap();
        click("dialog-save".into(), window, cx);
        let saved = std::fs::read(&path).unwrap();
        if let Some(bytes) = &original_bytes {
            std::fs::write(&path, bytes).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        assert_eq!(std::fs::read(&path).ok(), original_bytes);
        let saved: Preferences = serde_yaml::from_slice(&saved).unwrap();
        assert_eq!(saved.general.theme, "dark");
        assert!(
            !saved.editor.show_line_num
                && !saved.editor.show_folding
                && !saved.editor.drop_text
                && !saved.editor.links
        );
        println!(
            "ENTRY PREFERENCES SAVE QA PASS: actual Save persists theme/editor options, keeps parent draft/fullscreen and immediately restores the original QA preferences bytes"
        );
    }
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1.), px(1.)),
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(window, cx).await;
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    let original = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.displayed_entry_dialog().unwrap().id
        })
        .unwrap();
    let busy = window.update(cx, |_, _, cx| view.read(cx).busy).unwrap();
    key("cmd-s", window, cx);
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.busy, busy);
            assert_eq!(app.displayed_entry_dialog().unwrap().id, original);
            assert_eq!(
                matches!(app.dialog.as_ref().unwrap().kind, DialogKind::Preferences),
                !(scene.ends_with("-return") || scene.ends_with("-saved"))
            );
        })
        .unwrap();
    if !(scene.ends_with("-return") || scene.ends_with("-saved")) {
        key("escape", window, cx);
        draw(window, cx).await;
    }
    window
        .update(cx, |_, w, cx| {
            let app = view.read(cx);
            let entry = app.dialog.as_ref().unwrap();
            assert!(app.editing_entry());
            assert_eq!(entry.id, original);
            assert_eq!(entry.value("field", cx), "draft-field");
            assert_eq!(entry.value("value", cx), "line one\nline two");
            assert!(entry.entry_changed(cx));
            assert_eq!(
                app.entry_panel_fullscreen(),
                (scene.ends_with("-full") || scene.ends_with("-saved"))
            );
            assert!(app.focus.is_focused(w));
            if scene.ends_with("-dark") {
                assert_eq!(app.pref.general.theme, "light");
            }
        })
        .unwrap();
    if scene == "entry-preferences" {
        click("pin-entry".into(), window, cx);
        open_settings(view, window, cx).await;
        key("cmd-s", window, cx);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert!(matches!(
                    app.dialog.as_ref().unwrap().kind,
                    DialogKind::Preferences
                ));
                let entry = app.displayed_entry_dialog().unwrap();
                assert!(entry.boolean("pin"));
                assert!(entry.entry_changed(cx));
                assert_eq!(app.busy, busy);
            })
            .unwrap();
        key("escape", window, cx);
        draw(window, cx).await;
        key("cmd-,", window, cx);
        draw(window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert!(matches!(
                    app.dialog.as_ref().unwrap().kind,
                    DialogKind::Preferences
                ));
                assert_eq!(app.displayed_entry_dialog().unwrap().id, original);
            })
            .unwrap();
        key("escape", window, cx);
    }
    if scene.ends_with("-saved") {
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert_eq!(app.pref.general.theme, "dark");
                    assert!(
                        !app.pref.editor.show_line_num
                            && !app.pref.editor.show_folding
                            && !app.pref.editor.drop_text
                            && !app.pref.editor.links
                    );
                    let old = app
                        .dialog
                        .as_mut()
                        .unwrap()
                        .data
                        .as_object_mut()
                        .unwrap()
                        .remove("_qa_original_preferences")
                        .unwrap();
                    app.pref = serde_json::from_value(old).unwrap();
                    for editor in app.tabs.iter().map(|tab| &tab.editor).chain(
                        app.dialog
                            .as_ref()
                            .into_iter()
                            .flat_map(|dialog| dialog.editors.values()),
                    ) {
                        editor.update(cx, |editor, cx| {
                            editor.set_line_number(app.pref.editor.show_line_num, w, cx);
                            editor.set_folding(app.pref.editor.show_folding, w, cx);
                            editor.set_drag_text(app.pref.editor.drop_text, cx);
                            editor.set_links_enabled(app.pref.editor.links, cx);
                        });
                    }
                    app.apply_theme(w, cx);
                    app.load_language();
                })
            })
            .unwrap();
    }
    println!(
        "ENTRY PREFERENCES INPUT QA PASS: {scene} cancel restores draft/fullscreen/theme; Cmd+S does not submit settings or the unfocused editor"
    );
}
