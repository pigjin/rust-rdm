use super::*;
use gpui_base::ScrollbarHandle as _;

fn fixture() -> Json {
    serde_json::from_str(include_str!("../../../tests/fixtures/csv-list-source.json")).unwrap()
}
fn record(scene: &str) -> Json {
    fixture()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == scene)
        .unwrap()
        .clone()
}
impl RdmApp {
    pub(in crate::ui) fn qa_prepare_csv_list(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if scene == "csv-list-dark-middle" {
            self.pref.general.theme = "dark".into();
            self.apply_theme(window, cx);
        }
        self.open_dialog(DialogKind::Export, window, cx);
        self.set_dialog_field("server", "localhost", window, cx);
        self.set_dialog_field("db", "0", window, cx);
        // Layout-only target; these previews never submit an export.
        self.dialog.as_mut().unwrap().target = Some(Target {
            id: 0,
            conn: Connection {
                name: "localhost".into(),
                ..Default::default()
            },
            db: 0,
            key: vec![],
            kind: KeyType::String,
            keys: (0..fixture()["count"].as_u64().unwrap())
                .map(|index| format!("key:{index:04}").into_bytes())
                .collect(),
        });
        self.dialog.as_ref().unwrap().fields["server"]
            .update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }
}
async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..2 {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        })
        .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(35))
            .await;
    }
}
async fn scroll(view: &Entity<RdmApp>, window: WindowHandle<Root>, top: f32, cx: &mut AsyncApp) {
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.dialog
                    .as_ref()
                    .unwrap()
                    .csv_key_scroll
                    .set_offset(point(px(0.), px(-top)));
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx).await;
}
fn gesture(window: WindowHandle<Root>, cx: &mut AsyncApp, data: &Json) {
    cx.update_window(window.into(), |_, window, cx| {
        let from = &data["fromPoint"];
        let to = &data["toPoint"];
        let from = point(
            px(from["x"].as_f64().unwrap() as f32),
            px(from["y"].as_f64().unwrap() as f32),
        );
        let to = point(
            px(to["x"].as_f64().unwrap() as f32),
            px(to["y"].as_f64().unwrap() as f32),
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: from,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position: from,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position: to,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
}
pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Option<String> {
    draw(window, cx).await;
    let sample = record(scene);
    let selected = matches!(
        scene,
        "csv-list-selection" | "csv-list-end" | "csv-list-return" | "csv-list-dark-middle"
    );
    if scene == "csv-list-wheel" {
        cx.update_window(window.into(), |_, window, cx| {
            let position = point(px(400.), px(480.));
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            // A logical pixel delta. The Source Chromium harness scales its
            // physical wheel delta by DPR; this checks the resulting position.
            window.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(-250.))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            );
        })
        .unwrap();
        draw(window, cx).await;
    } else if matches!(scene, "csv-list-hover" | "csv-list-drag") {
        cx.update_window(window.into(), |_, window, cx| {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: point(px(682.), px(419.)),
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        draw(window, cx).await;
        if scene == "csv-list-drag" {
            gesture(window, cx, &sample["gesture"]);
            draw(window, cx).await;
        }
    } else if selected {
        scroll(view, window, 12417., cx).await;
        gesture(window, cx, &record("csv-list-selection")["gesture"]);
        draw(window, cx).await;
        if scene != "csv-list-selection" {
            scroll(view, window, 24820., cx).await;
            if matches!(scene, "csv-list-return" | "csv-list-dark-middle") {
                scroll(view, window, 0., cx).await;
            }
            if scene == "csv-list-dark-middle" {
                scroll(view, window, 12417., cx).await;
            }
        }
    } else {
        scroll(
            view,
            window,
            sample["scrollTop"].as_f64().unwrap() as f32,
            cx,
        )
        .await;
    }
    if scene != "csv-list-hover" {
        cx.update_window(window.into(), |_, window, cx| {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: point(px(1000.), px(700.)),
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        draw(window, cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            let assert_rect = |id: ElementId, b: &Json| {
                let actual = gpui_base::test_support::find(window, &[], &id)
                    .unwrap()
                    .bounds();
                for (name, value) in [
                    ("x", actual.origin.x),
                    ("y", actual.origin.y),
                    ("width", actual.size.width),
                    ("height", actual.size.height),
                ] {
                    let expected = b[name].as_f64().unwrap() as f32;
                    assert!(
                        (value.as_f32() - expected).abs() < 1.2,
                        "{scene} {id:?} {name}: {value:?} vs {expected}"
                    );
                }
            };
            assert_rect("dialog-card".into(), &sample["dialog"]);
            assert_rect("csv-affected-keys".into(), &sample["card"]);
            assert_rect("csv-affected-list".into(), &sample["bounds"]);
            if scene == "csv-list-hover" {
                assert_rect(
                    "csv-key-scroll-thumb".into(),
                    &sample["scrollbars"][1]["bounds"],
                );
            }
            for (slot, row) in sample["rows"].as_array().unwrap().iter().enumerate() {
                let id: ElementId = ("csv-affected-row", slot).into();
                let actual = gpui_base::test_support::find(window, &[], &id).unwrap();
                assert_eq!(actual.label(), row["text"].as_str(), "{scene} row {slot}");
                assert_rect(id, &row["bounds"]);
            }
            assert!(
                gpui_base::test_support::find(window, &[], &("csv-affected-row", 10usize).into())
                    .is_none(),
                "only Source's visible window may be mounted"
            );
            let dialog = view.read(cx).dialog.as_ref().unwrap();
            assert!((dialog.csv_key_scroll.content_size().height.as_f32() - 25000.).abs() < 0.6);
            assert!(
                (-dialog.csv_key_scroll.offset().y.as_f32()
                    - sample["scrollTop"].as_f64().unwrap() as f32)
                    .abs()
                    < 0.6
            );
        })
        .unwrap();
    let expected = sample["selection"].as_str().unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(
            gpui_base::TextSelection::selected_text(window, cx),
            expected,
            "{scene} original selection after scrolling"
        );
    })
    .unwrap();
    if selected {
        let matches = cx
            .update_window(window.into(), |_, window, cx| {
                let previous = cx.read_from_clipboard();
                cx.write_to_clipboard(ClipboardItem::new_string("rust-rdm-copy-check".into()));
                window.dispatch_event(
                    PlatformInput::KeyDown(KeyDownEvent {
                        keystroke: Keystroke::parse("cmd-c").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    }),
                    cx,
                );
                let matches=crate::ui::qa::copy_result(previous,cx).as_deref()==Some(expected);
                matches
            })
            .unwrap();
        assert!(matches, "{scene} copy did not preserve Source text");
    }
    eprintln!("QA PASS: {scene} original window, scroll extent, rows and copied selection");
    selected.then(|| expected.to_string())
}

pub(in crate::ui) async fn verify_large_list(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    let original = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let target = app.dialog.as_mut().unwrap().target.as_mut().unwrap();
                let original = std::mem::replace(
                    &mut target.keys,
                    (0..100_000)
                        .map(|i| format!("key:{i:04}").into_bytes())
                        .collect(),
                );
                cx.notify();
                original
            })
        })
        .unwrap();
    draw(window, cx).await;
    for top in [0., 1_250_000., 2_499_820.] {
        scroll(view, window, top, cx).await;
        window
            .update(cx, |_, window, cx| {
                let start = ((top / 25.).floor() as usize).saturating_sub(1);
                let row = gpui_base::test_support::find(
                    window,
                    &[],
                    &("csv-affected-row", 0usize).into(),
                )
                .unwrap();
                assert_eq!(row.label(), Some(format!("key:{start:04}").as_str()));
                assert!(
                    gpui_base::test_support::find(
                        window,
                        &[],
                        &("csv-affected-row", 10usize).into()
                    )
                    .is_none()
                );
                assert!(
                    (view
                        .read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .csv_key_scroll
                        .content_size()
                        .height
                        .as_f32()
                        - 2_500_000.)
                        .abs()
                        < 0.6
                );
            })
            .unwrap();
    }
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.dialog.as_mut().unwrap().target.as_mut().unwrap().keys = original;
                app.dialog
                    .as_ref()
                    .unwrap()
                    .csv_key_scroll
                    .set_offset(point(px(0.), px(0.)));
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx).await;
    eprintln!(
        "QA PASS: 100000 CSV keys mount at most 10 rows at top, middle and end; original keys restored"
    );
}
