use super::qa_json::{click, idle};
use super::*;

fn key(key: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Option<String> {
    idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.pref.general.theme = if scene.ends_with("-dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(window, cx);
                let i = app.index().unwrap();
                app.tabs[i].pane = Pane::Detail;
                app.tabs[i].row_match.clear();
                app.tabs[i].row_filter_value.clear();
                app.tabs[i]
                    .column_widths
                    .retain(|key, _| !key.starts_with("entry-"));
                app.messages.clear();
            })
        })
        .unwrap();
    click("key-name-text", window, cx);
    let mut copied = None;
    if scene.starts_with("key-input-selection") {
        key("cmd-a", window, cx);
        window
            .update(cx, |_, window, cx| {
                let app = view.read(cx);
                let input = app.tabs[app.index().unwrap()].key_name.clone();
                assert_eq!(input.read(cx).selected_range(), 0..6);
                input.update(cx, |input, cx| {
                    input.replace_text_in_range(None, "changed", window, cx);
                    assert_eq!(input.value().as_ref(), "aahash");
                    assert_eq!(input.selected_range(), 0..6);
                });
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
            let app = view.read(cx);
            let bounds = gpui_base::test_support::find(window, &[], &"key-name-text".into())
                .unwrap()
                .bounds();
            let font = app.preference_font("general", ".SystemUIFont", cx);
            let x = |text: &str| {
                bounds.origin.x + px(text::script_width(text, font.clone(), 14., window))
            };
            let start = point(x("aaha"), bounds.center().y);
            let end = point(x("aahash") + px(2.), bounds.center().y);
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: end,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        let original=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
        key("cmd-c", window, cx);
        let text = window
            .update(cx, |_, _, cx| {
                let text=super::qa::copy_result(original,cx);
                let app = view.read(cx);
                assert_eq!(
                    app.tabs[app.index().unwrap()]
                        .key_name
                        .read(cx)
                        .selected_range(),
                    4..6
                );
                text
            })
            .unwrap();
        assert!(text.as_deref()==Some("sh"),"key selection must copy the fixture substring");
        copied = text;
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let app = view.read(cx);
        let input = &app.tabs[app.index().unwrap()].key_name;
        assert_eq!(text_input(input, cx), "aahash");
        assert!(!input.read(cx).is_editable());
        assert!(input.focus_handle(cx).is_focused(window));
        let oracle: Json =
            serde_json::from_str(include_str!("../../tests/fixtures/key-input-source.json"))
                .unwrap();
        let record = oracle["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == scene)
            .unwrap();
        for (id, class) in [
            ("key-name", "n-input"),
            ("key-name-text", "n-input__input"),
            ("key-reload-suffix", "n-input__suffix"),
        ] {
            let source = record["metrics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| {
                    m["class"]
                        .as_str()
                        .unwrap()
                        .split_whitespace()
                        .any(|c| c == class)
                })
                .unwrap();
            let bounds = gpui_base::test_support::find(
                window,
                &[],
                &ElementId::from(SharedString::from(id)),
            )
            .unwrap()
            .bounds();
            for (actual, field) in [
                (bounds.origin.x, "x"),
                (bounds.origin.y, "y"),
                (bounds.size.width, "w"),
                (bounds.size.height, "h"),
            ] {
                assert!(
                    (actual.as_f32() - source[field].as_f64().unwrap() as f32).abs() < 0.6,
                    "{scene} {id} {field}: {actual:?} vs {}",
                    source[field]
                );
            }
        }
    })
    .unwrap();
    println!("QA PASS: {scene} readonly key focus, selection, copy and original geometry");
    copied
}
