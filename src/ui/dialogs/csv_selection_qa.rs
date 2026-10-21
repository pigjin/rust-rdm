use super::*;

fn fixture() -> Json {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/csv-selection-source.json"
    ))
    .unwrap()
}
fn bytes(value: &Json) -> Vec<u8> {
    value
        .as_str()
        .map(|text| text.as_bytes().to_vec())
        .unwrap_or_else(|| {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect()
        })
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
    pub(in crate::ui) fn qa_prepare_csv_selection(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sample = record(scene);
        self.csv_export_expire = false;
        self.open_dialog(DialogKind::Export, window, cx);
        self.set_dialog_field("server", "localhost", window, cx);
        self.set_dialog_field("db", "0", window, cx);
        self.dialog.as_mut().unwrap().target = Some(Target {
            id: 0,
            conn: Connection {
                name: "localhost".into(),
                ..Default::default()
            },
            db: 0,
            key: vec![],
            kind: KeyType::String,
            keys: sample["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(bytes)
                .collect(),
        });
        cx.notify();
    }
}
async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
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
fn mouse(
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
    from: Point<Pixels>,
    to: Point<Pixels>,
    clicks: usize,
) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: from,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                click_count: clicks,
                ..Default::default()
            }),
            cx,
        );
        if clicks == 1 {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: to,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
        }
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                click_count: clicks,
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
    window
        .update(cx, |_, window, _| {
            for (index, item) in sample["metrics"]
                .as_array()
                .unwrap()
                .iter()
                .skip(1)
                .enumerate()
            {
                let bounds =
                    gpui_base::test_support::find(window, &[], &("csv-affected-row", index).into())
                        .unwrap()
                        .bounds();
                for (name, value) in [
                    ("x", bounds.origin.x),
                    ("y", bounds.origin.y),
                    ("width", bounds.size.width),
                    ("height", bounds.size.height),
                ] {
                    let source = item["bounds"][name].as_f64().unwrap() as f32;
                    assert!(
                        (value.as_f32() - source).abs() < 0.8,
                        "{scene} row {index} {name}: {value:?} vs {source}"
                    );
                }
            }
        })
        .unwrap();
    if scene == "csv-export-select-plain" {
        return None;
    }
    if scene == "csv-export-select-word" {
        let position = record("csv-export-select-partial")["selection"]["fromPoint"].clone();
        let at = point(
            px(position["x"].as_f64().unwrap() as f32 + 8.),
            px(position["y"].as_f64().unwrap() as f32),
        );
        mouse(window, cx, at, at, 2);
    } else {
        let from = &sample["selection"]["fromPoint"];
        let to = &sample["selection"]["toPoint"];
        mouse(
            window,
            cx,
            point(
                px(from["x"].as_f64().unwrap() as f32),
                px(from["y"].as_f64().unwrap() as f32),
            ),
            point(
                px(to["x"].as_f64().unwrap() as f32),
                px(to["y"].as_f64().unwrap() as f32),
            ),
            1,
        );
    }
    draw(window, cx).await;
    let expected = sample["text"].as_str().unwrap();
    let previous=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(
            gpui_base::TextSelection::selected_text(window, cx),
            expected,
            "{scene} selected text"
        );
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse("cmd-c").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
    let copied = window
        .update(cx,|_,_,cx|crate::ui::qa::copy_result(previous,cx))
        .unwrap();
    assert!(
        copied.as_deref() == Some(expected),
        "{scene} Command+C did not copy the selected CSV text"
    );
    window
        .update(cx, |_, _, cx| {
            assert!(
                view.read(cx).dialog.is_some(),
                "selecting and copying preserves the CSV form"
            )
        })
        .unwrap();
    println!("QA PASS: {scene} source mouse selection and Command+C text");
    Some(expected.to_string())
}

pub(in crate::ui) async fn verify_order(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(window, cx);
                app.open_connection(app.connections[0].clone(), window, cx);
            })
        })
        .unwrap();
    crate::ui::qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let index = app.index().unwrap();
                let tab = &mut app.tabs[index];
                tab.check_mode = true;
                tab.key_root_expanded = true;
                tab.selected.clear();
                tab.key_filter.clear();
                tab.flat = false;
                cx.notify();
            })
        })
        .unwrap();
    for step in fixture()["order"].as_array().unwrap() {
        draw(window, cx).await;
        let name = step["step"].as_str().unwrap();
        let index = if matches!(name, "all" | "none") {
            0
        } else {
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    let tab = &app.tabs[app.index().unwrap()];
                    views::tree_rows(tab, "", false)
                        .iter()
                        .position(|row| row.key.as_deref() == Some(name.as_bytes()))
                        .unwrap()
                })
                .unwrap()
        };
        let at = window
            .update(cx, |_, window, _| {
                gpui_base::test_support::find(window, &[], &("key-check", index).into())
                    .unwrap()
                    .bounds()
                    .center()
            })
            .unwrap();
        mouse(window, cx, at, at, 1);
        draw(window, cx).await;
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let expected = step["keys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(bytes)
                        .collect::<Vec<_>>();
                    let actual = app.tabs[app.index().unwrap()]
                        .selected
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    assert_eq!(actual, expected, "original checked-key order at {name}");
                    app.open_dialog(DialogKind::Export, window, cx);
                    assert_eq!(
                        app.dialog.as_ref().unwrap().target.as_ref().unwrap().keys,
                        expected,
                        "dialog captures selection order at {name}"
                    );
                    app.close_dialog(window, cx);
                })
            })
            .unwrap();
    }
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let index = app.index().unwrap();
                app.tabs[index].check_mode = false;
                app.tabs[index].selected.clear();
                cx.notify();
            })
        })
        .unwrap();
    println!(
        "QA PASS: 7 original checkbox orders and export dialog snapshots preserve insertion and cascaded tree order"
    );
}
