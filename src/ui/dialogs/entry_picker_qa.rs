use super::new_key_qa::{click, draw};
use super::*;
use crate::ui::value_footer::PickerKind;

fn source(scene: &str) -> Json {
    let source: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/entry-picker-source.json"
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
    pub(in crate::ui) fn qa_prepare_entry_picker(
        &mut self,
        scene: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.qa_prepare_entry_panel("edit-entry-hash", w, cx);
        self.pref.decoder.clear();
        self.full_editor = scene.ends_with("-full");
        if scene.contains("dark") {
            self.pref.general.theme = "dark".into();
            self.apply_theme(w, cx);
        }
        if scene == "entry-picker-custom" || scene.starts_with("entry-picker-long") {
            self.pref.decoder = ["Base64", "自定义解码器ABCDEFGHIJKLMN"]
                .into_iter()
                .map(|name| Decoder {
                    name: name.into(),
                    enable: true,
                    decode_path: "/bin/echo".into(),
                    decode_args: vec!["{VALUE}".into()],
                    ..Default::default()
                })
                .collect();
        }
        cx.notify();
    }

    pub(in crate::ui) fn qa_assert_entry_picker(&self, scene: &str, w: &Window, cx: &App) {
        let record = source(scene);
        let kind = if record["kind"] == "format" {
            PickerKind::Format
        } else {
            PickerKind::Decode
        };
        let check = |id: ElementId, source: &Json| {
            let actual = gpui_base::test_support::find(w, &[], &id)
                .unwrap_or_else(|| panic!("missing {id:?} for {scene}"))
                .bounds();
            for (value, key) in [
                (actual.origin.x, "x"),
                (actual.origin.y, "y"),
                (actual.size.width, "width"),
                (actual.size.height, "height"),
            ] {
                assert!(
                    (value.as_f32() - source[key].as_f64().unwrap() as f32).abs() < 0.65,
                    "{scene} {id:?} {key}: {value:?} vs {}",
                    source[key]
                );
            }
        };
        check(
            format!("entry-select-{}", record["kind"].as_str().unwrap()).into(),
            &record["controls"]["trigger"],
        );
        assert_eq!(
            self.dialog
                .as_ref()
                .unwrap()
                .value(record["kind"].as_str().unwrap(), cx),
            record["selected"].as_str().unwrap()
        );
        if record["controls"]["menu"].is_object() {
            check("entry-picker-options".into(), &record["controls"]["menu"]);
            check(
                "entry-picker-menu-title".into(),
                &record["controls"]["header"],
            );
            check(
                "entry-picker-menu-title-divider".into(),
                &record["controls"]["titleDivider"],
            );
            let options = self.footer_options(kind);
            let rows = options
                .iter()
                .enumerate()
                .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
                .collect::<Vec<_>>();
            let expected = record["options"].as_array().unwrap();
            assert_eq!(rows.len(), expected.len());
            for ((i, label), expected) in rows.into_iter().zip(expected) {
                assert_eq!(label, expected["label"].as_str().unwrap());
                check(("entry-picker-option", i).into(), &expected["bounds"]);
            }
            assert!(self.dialog.as_ref().unwrap().entry_picker.tooltip.is_none());
            if scene.ends_with("-hover") {
                let expected = record["options"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|option| option["pending"] == true)
                    .unwrap()["label"]
                    .as_str()
                    .unwrap();
                let picker = self
                    .dialog
                    .as_ref()
                    .unwrap()
                    .entry_picker
                    .open
                    .as_ref()
                    .unwrap();
                let options = self.footer_options(picker.kind);
                assert_eq!(options[picker.cursor.unwrap()].as_deref(), Some(expected));
            }
        }
        if record["controls"]["tooltip"].is_object() {
            check(
                "entry-picker-tooltip".into(),
                &record["controls"]["tooltip"],
            );
        }
        println!("QA PASS: {scene} original entry picker bounds, option order and selected label");
    }
}

fn move_to(position: Point<Pixels>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
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
fn center(id: ElementId, window: WindowHandle<Root>, cx: &mut AsyncApp) -> Point<Pixels> {
    window
        .update(cx, |_, w, _| {
            gpui_base::test_support::find(w, &[], &id)
                .unwrap()
                .bounds()
                .center()
        })
        .unwrap()
}

pub(in crate::ui) async fn prepare(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    super::new_key_qa::idle(view, window, cx).await;
    if scene.starts_with("entry-picker-long") {
        window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
            app.set_dialog_field("decode", "自定义解码器ABCDEFGHIJKLMN", w, cx);
            app.dialog.as_mut().unwrap().entry_content.as_mut().unwrap().bound_decode="自定义解码器ABCDEFGHIJKLMN".into();
        })).unwrap();
    }
    let record = source(scene);
    let name = format!("entry-select-{}", record["kind"].as_str().unwrap());
    if scene.contains("tooltip") {
        move_to(center(name.into(), window, cx), window, cx);
        window
            .update(cx, |_, _, cx| {
                assert!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .entry_picker
                        .tooltip
                        .is_none()
                )
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
    } else if scene != "entry-picker-long" {
        click(name.into(), window, cx);
    }
    draw(window, cx).await;
    if scene == "entry-picker-format-hover" {
        move_to(
            center(("entry-picker-option", 1_usize).into(), window, cx),
            window,
            cx,
        );
    } else if !scene.contains("tooltip") {
        move_to(point(px(1.), px(1.)), window, cx);
    }
    draw(window, cx).await;
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene == "entry-picker-format" {
        let fixture: Json = serde_json::from_str(include_str!(
            "../../../tests/fixtures/entry-picker-source.json"
        ))
        .unwrap();
        let events = fixture["events"].as_array().unwrap();
        let oracle = events
            .iter()
            .find(|event| event["name"] == "format-keyboard-enter")
            .unwrap();
        for (i, name) in ["down", "down"].into_iter().enumerate() {
            key(name, window, cx);
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    let picker = app
                        .dialog
                        .as_ref()
                        .unwrap()
                        .entry_picker
                        .open
                        .as_ref()
                        .unwrap();
                    let options = app.footer_options(picker.kind);
                    assert_eq!(
                        options[picker.cursor.unwrap()].as_deref(),
                        oracle["pending"][i][0].as_str()
                    );
                })
                .unwrap();
        }
        key("enter", window, cx);
        for _ in 0..100 {
            if window
                .update(cx, |_, _, cx| view.read(cx).busy == 0)
                .unwrap()
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(20))
                .await;
        }
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let dialog = app.dialog.as_ref().unwrap();
                assert!(dialog.entry_picker.open.is_none());
                assert_eq!(dialog.value("format", cx), "JSON");
                assert_eq!(dialog.value("value", cx), "before");
            })
            .unwrap();
        click("entry-select-format".into(), window, cx);
        key("up", window, cx);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let picker = app
                    .dialog
                    .as_ref()
                    .unwrap()
                    .entry_picker
                    .open
                    .as_ref()
                    .unwrap();
                let options = app.footer_options(picker.kind);
                let expected = events
                    .iter()
                    .find(|event| event["name"] == "up-wrap-and-escape")
                    .unwrap();
                assert_eq!(
                    options[picker.cursor.unwrap()].as_deref(),
                    expected["pending"][0].as_str()
                );
            })
            .unwrap();
        key("escape", window, cx);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let dialog = app.dialog.as_ref().unwrap();
                assert!(dialog.entry_picker.open.is_none());
                assert_eq!(dialog.value("format", cx), "JSON");
            })
            .unwrap();
        click("entry-select-format".into(), window, cx);
        click("entry-field-input".into(), window, cx);
        window
            .update(cx, |_, _, cx| {
                assert!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .entry_picker
                        .open
                        .is_none()
                )
            })
            .unwrap();
        click("entry-select-decode".into(), window, cx);
        let last = window
            .update(cx, |_, _, cx| {
                view.read(cx).footer_options(PickerKind::Decode).len() - 1
            })
            .unwrap();
        click(("entry-picker-option", last).into(), window, cx);
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert!(matches!(
                        app.dialog.as_ref().unwrap().kind,
                        DialogKind::Preferences
                    ));
                    assert_eq!(app.dialog.as_ref().unwrap().tab, "decoder");
                    assert!(matches!(
                        app.dialog_stack.last().unwrap().kind,
                        DialogKind::EditEntry(_)
                    ));
                    app.close_dialog(w, cx);
                    assert!(app.editing_entry());
                })
            })
            .unwrap();
        println!(
            "ENTRY PICKER INPUT QA PASS: original option order, keyboard confirmation and retained cursor, Escape/outside dismissal, and nested decoder settings preserve the editor"
        );
    }
    if scene.contains("tooltip") {
        move_to(
            center("entry-picker-tooltip".into(), window, cx),
            window,
            cx,
        );
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        window
            .update(cx, |_, _, cx| {
                assert!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .entry_picker
                        .tooltip
                        .is_some()
                )
            })
            .unwrap();
        move_to(point(px(1.), px(1.)), window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        window
            .update(cx, |_, _, cx| {
                assert!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .entry_picker
                        .tooltip
                        .is_none()
                )
            })
            .unwrap();
        let name = if scene.contains("format") {
            "entry-select-format"
        } else {
            "entry-select-decode"
        };
        click(name.into(), window, cx);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let state = &app.dialog.as_ref().unwrap().entry_picker;
                assert!(state.open.is_some());
                assert!(state.tooltip.is_none());
            })
            .unwrap();
        key("escape", window, cx);
        println!(
            "ENTRY PICKER TOOLTIP QA PASS: delayed hover, tooltip handoff, leave dismissal and menu suppression"
        );
    }
}
