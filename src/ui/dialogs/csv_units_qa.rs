use super::*;

fn fixture() -> Json {
    serde_json::from_str(include_str!("../../../tests/fixtures/csv-unit-source.json")).unwrap()
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
const UNITS: [u32; 4] = [1, 60, 3600, 86400];
impl RdmApp {
    pub(in crate::ui) fn qa_prepare_csv_unit(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sample = record(scene);
        self.csv_import_unit = 1;
        self.open_dialog(DialogKind::Import, window, cx);
        self.set_dialog_field("server", "localhost", window, cx);
        self.set_dialog_field("db", "0", window, cx);
        self.set_dialog_field("ttl_type", "2", window, cx);
        let index = match sample["unit"].as_str().unwrap() {
            "分" => 1,
            "小时" => 2,
            "天" => 3,
            _ => 0,
        };
        self.set_dialog_field("unit", UNITS[index].to_string(), window, cx);
        // Layout previews do not submit this target. The functional check below
        // explicitly replaces it with the marked disposable Unix connection.
        self.dialog.as_mut().unwrap().target = Some(Target {
            id: 0,
            conn: Connection {
                name: "localhost".into(),
                ..Default::default()
            },
            db: 0,
            key: vec![],
            kind: KeyType::String,
            keys: vec![],
        });
        if sample["menu"] == true {
            self.open_csv_unit(window, cx);
        }
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
fn move_to(id: ElementId, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &id)
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
fn click(id: ElementId, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &id)
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
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
}
fn key(value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(value).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        )
    })
    .unwrap();
}
pub(in crate::ui) async fn verify_layout(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    draw(window, cx).await;
    if matches!(scene, "csv-unit-hover" | "csv-unit-keyboard") {
        move_to(("csv-unit-option", 1usize).into(), window, cx);
        draw(window, cx).await;
        if scene == "csv-unit-keyboard" {
            key("down", window, cx);
            draw(window, cx).await;
        }
    }
    window
        .update(cx, |_, window, cx| {
            let sample = record(scene);
            let elements = sample["elements"].as_array().unwrap();
            let compare = |id: ElementId, expected: &Json| {
                let bounds = gpui_base::test_support::find(window, &[], &id)
                    .unwrap()
                    .bounds();
                for (name, actual) in [
                    ("x", bounds.origin.x),
                    ("y", bounds.origin.y),
                    ("width", bounds.size.width),
                    ("height", bounds.size.height),
                ] {
                    let wanted = expected["bounds"][name].as_f64().unwrap() as f32;
                    assert!(
                        (actual.as_f32() - wanted).abs() < 0.7,
                        "{scene} {id:?} {name}: {actual:?} vs {wanted}"
                    );
                }
            };
            compare(
                "csv-unit".into(),
                elements
                    .iter()
                    .find(|e| {
                        e["classes"]
                            .as_str()
                            .unwrap()
                            .starts_with("n-base-selection ")
                    })
                    .unwrap(),
            );
            if sample["menu"] == true {
                compare(
                    "csv-unit-options".into(),
                    elements
                        .iter()
                        .find(|e| {
                            e["classes"]
                                .as_str()
                                .unwrap()
                                .starts_with("n-base-select-menu ")
                        })
                        .unwrap(),
                );
                let rows = elements
                    .iter()
                    .filter(|e| {
                        e["classes"]
                            .as_str()
                            .unwrap()
                            .starts_with("n-base-select-option ")
                    })
                    .collect::<Vec<_>>();
                for (index, row) in rows.iter().enumerate() {
                    compare(("csv-unit-option", index).into(), row);
                }
                let pending = rows
                    .iter()
                    .position(|e| e["classes"].as_str().unwrap().contains("--pending"))
                    .unwrap();
                assert_eq!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .csv_unit_picker
                        .pending,
                    pending
                );
            }
        })
        .unwrap();
    println!("QA PASS: {scene} original select control and option geometry");
}
fn check(step: &str, view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, _, cx| {
            let data = fixture();
            let expected = data["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["step"] == step)
                .unwrap();
            let app = view.read(cx);
            assert_eq!(
                app.dialog.is_some(),
                expected["modal"].as_bool().unwrap(),
                "{step} form lifetime"
            );
            if let Some(dialog) = &app.dialog {
                assert_eq!(
                    dialog.csv_unit_picker.open,
                    expected["menu"].as_bool().unwrap(),
                    "{step} menu lifetime"
                );
                let label = match dialog.value("unit", cx).as_str() {
                    "60" => "分",
                    "3600" => "小时",
                    "86400" => "天",
                    _ => "秒",
                };
                assert_eq!(
                    label,
                    expected["unit"].as_str().unwrap(),
                    "{step} selected unit"
                );
                if let Some(pending) = expected["pending"].as_str() {
                    assert_eq!(
                        ["秒", "分", "小时", "天"][dialog.csv_unit_picker.pending],
                        pending,
                        "{step} pending unit"
                    );
                }
            }
        })
        .unwrap();
}
pub(in crate::ui) async fn verify_behaviour(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(window, cx);
                app.qa_prepare_csv_unit("csv-unit-menu", window, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    check("csv-unit-menu", view, window, cx);
    move_to(("csv-unit-option", 1usize).into(), window, cx);
    draw(window, cx).await;
    check("csv-unit-hover", view, window, cx);
    key("down", window, cx);
    draw(window, cx).await;
    check("csv-unit-keyboard", view, window, cx);
    key("enter", window, cx);
    draw(window, cx).await;
    check("keyboard-enter", view, window, cx);
    click("csv-unit".into(), window, cx);
    draw(window, cx).await;
    check("csv-unit-selected", view, window, cx);
    key("escape", window, cx);
    draw(window, cx).await;
    check("menu-escape", view, window, cx);
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.open_dialog(DialogKind::Import, window, cx);
                app.set_dialog_field("ttl_type", "2", window, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    click("csv-unit".into(), window, cx);
    draw(window, cx).await;
    click(("csv-unit-option", 3usize).into(), window, cx);
    draw(window, cx).await;
    check("mouse-day", view, window, cx);
    click("dialog-cancel".into(), window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.open_dialog(DialogKind::Import, window, cx);
                app.set_dialog_field("ttl_type", "2", window, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    check("csv-unit-reopened", view, window, cx);
    click("csv-unit".into(), window, cx);
    draw(window, cx).await;
    check("csv-unit-day", view, window, cx);
    key("up", window, cx);
    key("enter", window, cx);
    draw(window, cx).await;
    check("day-up-enter", view, window, cx);
    key("up", window, cx);
    draw(window, cx).await;
    check("closed-up", view, window, cx);
    key("down", window, cx);
    draw(window, cx).await;
    check("closed-down", view, window, cx);
    key("space", window, cx);
    draw(window, cx).await;
    check("space-enter", view, window, cx);
    click("csv-unit".into(), window, cx);
    draw(window, cx).await;
    click(("csv-unit-option", 0usize).into(), window, cx);
    draw(window, cx).await;
    check("mouse-second", view, window, cx);
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1000.), px(700.)),
                ..Default::default()
            }),
            cx,
        )
    })
    .unwrap();
    key("down", window, cx);
    draw(window, cx).await;
    check("open-second", view, window, cx);
    key("up", window, cx);
    draw(window, cx).await;
    check("wrap-up", view, window, cx);
    key("down", window, cx);
    draw(window, cx).await;
    check("wrap-down", view, window, cx);
    key("down", window, cx);
    key("down", window, cx);
    key("enter", window, cx);
    draw(window, cx).await;
    check("restore-hour", view, window, cx);
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("unit.csv");
    let key_bytes = b"csv-unit:target".to_vec();
    let (service, config) = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let config = app.connections[0].clone();
                assert_eq!(config.network, "unix");
                let root = PathBuf::from(&config.sock).parent().unwrap().to_path_buf();
                assert_eq!(
                    std::fs::read_to_string(root.join("RDM-QA-FIXTURE")).unwrap(),
                    "rust-rdm-render-fixture-v1\n"
                );
                assert_eq!(
                    app.service
                        .run(&config, 15, command(&["DBSIZE"]))
                        .unwrap()
                        .integer()
                        .unwrap(),
                    0
                );
                let dump = app
                    .service
                    .run(&config, 0, command(&["DUMP", "aastring"]))
                    .unwrap()
                    .bytes();
                let mut writer = ::csv::Writer::from_path(&path).unwrap();
                writer
                    .write_record([hex::encode(&key_bytes), hex::encode(dump)])
                    .unwrap();
                writer.flush().unwrap();
                app.dialog.as_mut().unwrap().target = Some(Target {
                    id: 0,
                    conn: config.clone(),
                    db: 15,
                    key: vec![],
                    kind: KeyType::String,
                    keys: vec![],
                });
                app.set_dialog_field("path", path.to_str().unwrap(), window, cx);
                app.set_dialog_field("ttl", "2.5", window, cx);
                (app.service.clone(), config)
            })
        })
        .unwrap();
    click("dialog-save".into(), window, cx);
    window
        .update(cx, |_, _, cx| {
            assert!(
                view.read(cx).dialog.is_none(),
                "confirmed import closes immediately"
            )
        })
        .unwrap();
    for _ in 0..250 {
        if window
            .update(cx, |_, _, cx| view.read(cx).csv_transfers.is_empty())
            .unwrap()
        {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    let ttl = service
        .run(&config, 15, vec![b"TTL".to_vec(), key_bytes.clone()])
        .unwrap()
        .integer()
        .unwrap();
    let expected = fixture()["calls"][0][4].as_i64().unwrap();
    assert!(
        (expected - 5..=expected).contains(&ttl),
        "original unit conversion {expected}: {ttl}"
    );
    assert_eq!(
        service
            .run(&config, 15, vec![b"GET".to_vec(), key_bytes.clone()])
            .unwrap()
            .bytes(),
        b"Hello Rust + GPUI"
    );
    service
        .run(&config, 15, vec![b"DEL".to_vec(), key_bytes])
        .unwrap();
    assert_eq!(
        service
            .run(&config, 15, command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    println!(
        "QA PASS: 18 original unit-menu mouse/keyboard/Escape/reopen states; confirmed 2.5 hours imports with the original 9000 second TTL"
    );
}
