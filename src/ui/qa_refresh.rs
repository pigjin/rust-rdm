use super::qa_json::{click, idle};
use super::*;

async fn settle(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(35))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
}

async fn assert_form_visible(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    // Exceed the Source close delay: typing and focus changes must not
    // accidentally dismiss this interactive hover surface.
    cx.background_executor()
        .timer(Duration::from_millis(140))
        .await;
    settle(window, cx).await;
    window
        .update(cx, |_, window, _| {
            assert!(
                gpui_base::test_support::find(window, &[], &"refresh-form".into()).is_some(),
                "refresh form dismissed while the pointer was still over it"
            );
        })
        .unwrap();
}

fn type_interval(view: &Entity<RdmApp>, window: WindowHandle<Root>, text: &str, cx: &mut AsyncApp) {
    window
        .update(cx, |_, window, cx| {
            if let Some(field) =
                gpui_base::test_support::find(window, &[], &"refresh-interval-input".into())
            {
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position: field.bounds().center(),
                        ..Default::default()
                    }),
                    cx,
                );
            }
            let app = view.read(cx);
            let input = app.tabs[app.index().unwrap()].refresh_interval.clone();
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                let length = input.value().len();
                input.replace_text_in_range(Some(0..length), text, window, cx);
            });
        })
        .unwrap();
}

fn interval_text(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) -> String {
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            text_input(&app.tabs[app.index().unwrap()].refresh_interval, cx)
        })
        .unwrap()
}

fn blur_interval(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, window, cx| {
            let app = view.read(cx);
            let focus = app.focus.clone();
            let input = app.tabs[app.index().unwrap()].refresh_interval.clone();
            focus.focus(window, cx);
            // Inactive offscreen windows do not dispatch OS focus callbacks.
            input.update(cx, |_, cx| cx.emit(InputEvent::Blur));
        })
        .unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
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
                app.tabs[i].auto_refresh = false;
                app.tabs[i].pane = Pane::Detail;
                set_input(&app.tabs[i].refresh_interval, "2", window, cx);
                app.tabs[i].refresh_numbers[1] = Some(2.);
                app.messages.clear();
                app.focus.focus(window, cx);
            })
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &"reload-key".into())
            .unwrap()
            .bounds()
            .center();
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(650))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    let oracle: Json =
        serde_json::from_str(include_str!("../../tests/fixtures/refresh-source.json")).unwrap();
    if scene == "refresh-detail" {
        for sample in oracle["numbers"].as_array().unwrap() {
            assert_form_visible(window, cx).await;
            type_interval(view, window, "2", cx);
            settle(window, cx).await;
            type_interval(view, window, sample["raw"].as_str().unwrap(), cx);
            settle(window, cx).await;
            assert_eq!(
                interval_text(view, window, cx),
                sample["input"].as_str().unwrap(),
                "refresh input {sample}"
            );
            assert_form_visible(window, cx).await;
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(
                    PlatformInput::KeyDown(KeyDownEvent {
                        keystroke: Keystroke::parse("enter").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    }),
                    cx,
                );
            })
            .unwrap();
            settle(window, cx).await;
            assert_eq!(
                interval_text(view, window, cx),
                sample["enter"].as_str().unwrap(),
                "refresh Enter {sample}"
            );
            blur_interval(view, window, cx);
            settle(window, cx).await;
            assert_eq!(
                interval_text(view, window, cx),
                sample["blurred"].as_str().unwrap(),
                "refresh blur {sample}"
            );
        }
        type_interval(view, window, "1.5", cx);
        settle(window, cx).await;
        assert_form_visible(window, cx).await;
        click("refresh-switch-frame", window, cx);
        settle(window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert!(tab.auto_refresh);
                assert_eq!(tab.refresh_seconds, 1.5);
                assert!(
                    app.messages.is_empty(),
                    "Source accepts fractional refresh intervals without errors"
                );
            })
            .unwrap();
        click("refresh-switch-frame", window, cx);
        settle(window, cx).await;
    }
    assert_form_visible(window, cx).await;
    type_interval(
        view,
        window,
        if scene.starts_with("refresh-detail-on") {
            ""
        } else {
            "2"
        },
        cx,
    );
    settle(window, cx).await;
    blur_interval(view, window, cx);
    settle(window, cx).await;
    if scene.starts_with("refresh-detail-on") {
        assert_form_visible(window, cx).await;
        click("refresh-switch-frame", window, cx);
        settle(window, cx).await;
        assert_eq!(
            interval_text(view, window, cx),
            "2",
            "empty interval restores the Source default when enabled"
        );
    }
    if !scene.starts_with("refresh-detail-on") {
        cx.update_window(window.into(), |_, window, cx| {
            let position = gpui_base::test_support::find(window, &[], &"reload-key".into())
                .unwrap()
                .bounds()
                .center();
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
    }
    assert_form_visible(window, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let record = oracle["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == scene)
            .unwrap();
        let metrics = record["metrics"].as_array().unwrap();
        let class = |m: &&Json, class: &str| {
            m["class"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .any(|c| c == class)
        };
        let rows = metrics
            .iter()
            .filter(|m| class(m, "n-form-item"))
            .collect::<Vec<_>>();
        let labels = metrics
            .iter()
            .filter(|m| class(m, "n-form-item-label"))
            .collect::<Vec<_>>();
        for (id, source) in [
            (
                "refresh-form",
                metrics.iter().find(|m| class(m, "n-form")).unwrap(),
            ),
            ("refresh-auto-row", rows[0]),
            ("refresh-auto-label", labels[0]),
            ("refresh-interval-row", rows[1]),
            ("refresh-interval-label", labels[1]),
            (
                "refresh-unit",
                metrics
                    .iter()
                    .find(|m| class(m, "n-input__suffix"))
                    .unwrap(),
            ),
            (
                "refresh-switch-frame",
                metrics.iter().find(|m| class(m, "n-switch")).unwrap(),
            ),
            (
                "refresh-interval-input",
                metrics.iter().find(|m| class(m, "n-input-number")).unwrap(),
            ),
        ] {
            let actual = gpui_base::test_support::find(
                window,
                &[],
                &ElementId::from(SharedString::from(id)),
            )
            .unwrap_or_else(|| panic!("missing {id}"))
            .bounds();
            for (actual, key) in [
                (actual.origin.x, "x"),
                (actual.origin.y, "y"),
                (actual.size.width, "w"),
                (actual.size.height, "h"),
            ] {
                assert!(
                    (actual.as_f32() - source[key].as_f64().unwrap() as f32).abs() < 0.7,
                    "{scene} {id} {key}: {actual:?} vs {}",
                    source[key]
                );
            }
        }
        let app = view.read(cx);
        assert_eq!(
            app.tabs[app.index().unwrap()]
                .refresh_interval
                .read(cx)
                .is_editable(),
            !scene.starts_with("refresh-detail-on"),
            "refresh interval editability must follow the switch"
        );
    })
    .unwrap();
    println!("QA PASS: {scene} original refresh form geometry, numeric inputs and toggle behavior");
}
