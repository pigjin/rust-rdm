use super::new_key::Selector;
use super::*;

fn source(scene: &str) -> Json {
    let fixture: Json =
        serde_json::from_str(include_str!("../../../tests/fixtures/new-key-source.json")).unwrap();
    fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == scene)
        .unwrap()
        .clone()
}

pub(super) async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(40))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
}
pub(super) fn click(id: ElementId, window: WindowHandle<Root>, cx: &mut AsyncApp) {
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
pub(super) fn type_field(
    view: &Entity<RdmApp>,
    name: &str,
    text: &str,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, window, cx| {
            let state = view.read(cx).dialog.as_ref().unwrap().fields[name].clone();
            state.update(cx, |state, cx| {
                state.focus(window, cx);
                let length = state.value().len();
                state.replace_text_in_range(Some(0..length), text, window, cx);
            });
        })
        .unwrap();
}
pub(super) async fn idle(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..100 {
        draw(window, cx).await;
        if window
            .update(cx, |_, _, cx| view.read(cx).busy == 0)
            .unwrap()
        {
            return;
        }
    }
    panic!("new-key QA background job did not finish");
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene != "new-key-value-minimum" {
        return;
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.qa_prepare_new_key("new-key-string", window, cx)
            })
        })
        .unwrap();
    draw(window, cx).await;
    click("new-key-type".into(), window, cx);
    draw(window, cx).await;
    key("down", window, cx);
    key("enter", window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.dialog.as_ref().unwrap().value("type", cx), "HASH");
        })
        .unwrap();
    click("new-key-add-0".into(), window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).dialog.as_ref().unwrap().rows.len(), 2)
        })
        .unwrap();
    click("new-key-remove-0".into(), window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).dialog.as_ref().unwrap().rows.len(), 1)
        })
        .unwrap();
    click("new-key-db".into(), window, cx);
    type_field(view, "new_db_query", "1", window, cx);
    draw(window, cx).await;
    key("enter", window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.dialog.as_ref().unwrap().value("db", cx), "1");
            assert_eq!(app.tabs[app.index().unwrap()].db, 0);
        })
        .unwrap();
    click("new-key-import".into(), window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let dialog = app.dialog.as_ref().unwrap();
            assert!(matches!(dialog.kind, DialogKind::Import));
            assert_eq!(dialog.value("db", cx), "1");
            assert_eq!(dialog.target.as_ref().unwrap().db, 1);
        })
        .unwrap();
    key("escape", window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.qa_prepare_new_key("new-key-string", window, cx)
            })
        })
        .unwrap();
    draw(window, cx).await;
    // Actual pointer events exercise the ordinary textarea's vertical handle.
    for (delta, expected) in [(80., 217.), (-300., 32.)] {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.qa_prepare_new_key("new-key-string", window, cx)
                })
            })
            .unwrap();
        draw(window, cx).await;
        cx.update_window(window.into(), |_, window, cx| {
            let position =
                gpui_base::test_support::find(window, &[], &"new-key-value-resize".into())
                    .unwrap()
                    .bounds()
                    .center();
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            let end = position + point(px(0.), px(delta));
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
                    position: end,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        draw(window, cx).await;
        window
            .update(cx, |_, _, cx| {
                assert_eq!(
                    view.read(cx)
                        .dialog
                        .as_ref()
                        .unwrap()
                        .new_key
                        .as_ref()
                        .unwrap()
                        .value_height,
                    expected
                )
            })
            .unwrap();
    }
    // Use one guarded scratch key in the marked fixture's empty DB15.
    let (service, connection) = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            (
                app.service.clone(),
                app.dialog
                    .as_ref()
                    .unwrap()
                    .target
                    .as_ref()
                    .unwrap()
                    .conn
                    .clone(),
            )
        })
        .unwrap();
    let fixture = std::path::Path::new(&connection.sock).parent().unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.join("RDM-QA-FIXTURE")).unwrap(),
        "rust-rdm-render-fixture-v1\n"
    );
    let scratch = "new-key-uiqa:target";
    assert_eq!(
        service
            .run(&connection, 15, crate::redis::command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    service
        .run(
            &connection,
            15,
            crate::redis::command(&["SET", scratch, "old"]),
        )
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.open_dialog(DialogKind::NewKey, window, cx);
                app.set_dialog_field("db", "15", window, cx);
                app.change_new_key_type("HASH".into(), window, cx);
            })
        })
        .unwrap();
    type_field(view, "key", scratch, window, cx);
    let (field, value) = window
        .update(cx, |_, _, cx| {
            let row = &view.read(cx).dialog.as_ref().unwrap().rows[0];
            (
                super::new_key::field_key(row),
                super::new_key::value_key(row),
            )
        })
        .unwrap();
    type_field(view, &field, "field", window, cx);
    type_field(view, &value, "value", window, cx);
    draw(window, cx).await;
    click("dialog-save".into(), window, cx);
    idle(view, window, cx).await;
    let kept = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.dialog.as_ref().is_some_and(|d| {
                matches!(d.kind, DialogKind::NewKey) && d.value("key", cx) == scratch
            }) && app.new_key_value(cx) == serde_json::json!(["field", "value"])
        })
        .unwrap();
    // Remove the test key before assertions, including a failed form-retention check.
    service
        .run(&connection, 15, crate::redis::command(&["DEL", scratch]))
        .unwrap();
    assert!(kept, "backend error keeps the form and its populated rows");
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.change_new_key_type("STRING".into(), window, cx)
            })
        })
        .unwrap();
    type_field(view, "value", "中文 new key", window, cx);
    draw(window, cx).await;
    click("dialog-save".into(), window, cx);
    idle(view, window, cx).await;
    let saved = service
        .run(&connection, 15, crate::redis::command(&["GET", scratch]))
        .unwrap()
        .text();
    service
        .run(&connection, 15, crate::redis::command(&["DEL", scratch]))
        .unwrap();
    assert_eq!(saved, "中文 new key");
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(app.dialog.is_none());
            assert_eq!(app.tabs[app.index().unwrap()].db, 0);
            assert!(
                !app.tabs[app.index().unwrap()]
                    .keys
                    .iter()
                    .any(|key| key.key == scratch.as_bytes())
            );
        })
        .unwrap();
    assert_eq!(
        service
            .run(&connection, 15, crate::redis::command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.qa_prepare_new_key(scene, window, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    println!(
        "NEW KEY INPUT QA PASS: type keys, row buttons, DB filter, import DB, textarea drag, retained failure and actual DB15 creation / cleanup"
    );
}

impl RdmApp {
    fn qa_change_new_key_input(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_dialog_field(key, value, window, cx);
        self.new_key_input_changed(key, cx);
    }

    pub(in crate::ui) fn qa_check_new_key_models(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source: Json =
            serde_json::from_str(include_str!("../../../tests/fixtures/new-key-source.json"))
                .unwrap();
        let expected = |name: &str| {
            source["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["name"] == name)
                .unwrap()["request"]["value"]
                .clone()
        };
        self.open_dialog(DialogKind::NewKey, window, cx);
        for kind in ["STRING", "HASH", "LIST", "SET", "ZSET", "STREAM", "JSON"] {
            self.change_new_key_type(kind.into(), window, cx);
            assert_eq!(
                self.new_key_value(cx),
                expected(&format!("untouched-{kind}")),
                "untouched {kind}"
            );
        }
        self.change_new_key_type("HASH".into(), window, cx);
        for (index, (field, value)) in [("name", "Tiny RDM"), ("name", "Rust"), ("", "ignored")]
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                self.append_new_key_row(index - 1, window, cx);
            }
            let row = self.dialog.as_ref().unwrap().rows[index].clone();
            self.qa_change_new_key_input(&super::new_key::field_key(&row), field, window, cx);
            self.qa_change_new_key_input(&super::new_key::value_key(&row), value, window, cx);
        }
        assert_eq!(
            self.new_key_value(cx),
            expected("hash-filter-empty-duplicates")
        );
        for (kind, name) in [
            ("LIST", "list-duplicates-empty"),
            ("SET", "set-duplicates-empty"),
        ] {
            self.change_new_key_type(kind.into(), window, cx);
            for index in 0..3 {
                if index > 0 {
                    self.append_new_key_row(index - 1, window, cx);
                }
                let row = self.dialog.as_ref().unwrap().rows[index].clone();
                self.qa_change_new_key_input(
                    &super::new_key::value_key(&row),
                    if index < 2 { "alpha" } else { "" },
                    window,
                    cx,
                );
            }
            assert_eq!(self.new_key_value(cx), expected(name), "{kind}");
        }
        self.change_new_key_type("ZSET".into(), window, cx);
        for index in 0..2 {
            if index > 0 {
                self.append_new_key_row(index - 1, window, cx);
            }
            let row = self.dialog.as_ref().unwrap().rows[index].clone();
            self.qa_change_new_key_input(&super::new_key::value_key(&row), "member", window, cx);
            self.qa_change_new_key_input(
                &super::new_key::score_key(&row),
                if index == 0 { "2.5" } else { "-3" },
                window,
                cx,
            );
        }
        assert_eq!(self.new_key_value(cx), expected("zset-duplicates"));
        let row = self.dialog.as_ref().unwrap().rows[1].clone();
        self.qa_change_new_key_input(&super::new_key::score_key(&row), "", window, cx);
        assert_eq!(self.new_key_value(cx), expected("zset-null-score"));
        self.change_new_key_type("STREAM".into(), window, cx);
        let row = self.dialog.as_ref().unwrap().rows[0].clone();
        self.qa_change_new_key_input(&super::new_key::field_key(&row), "event", window, cx);
        self.qa_change_new_key_input(&super::new_key::value_key(&row), "login", window, cx);
        self.qa_change_new_key_input("stream_id", "1720000000000-0", window, cx);
        assert_eq!(self.new_key_value(cx), expected("stream-id-only-change"));
        self.qa_change_new_key_input(&super::new_key::value_key(&row), "logout", window, cx);
        assert_eq!(self.new_key_value(cx), expected("stream-pair-after-id"));
        self.qa_change_new_key_input("stream_id", "", window, cx);
        self.qa_change_new_key_input(&super::new_key::value_key(&row), "empty-id", window, cx);
        assert_eq!(self.new_key_value(cx), expected("stream-empty-id"));
        self.change_new_key_type("JSON".into(), window, cx);
        self.qa_change_new_key_input("value", "", window, cx);
        assert_eq!(self.new_key_value(cx), expected("json-explicit-empty"));
        self.change_new_key_type("STRING".into(), window, cx);
        self.change_new_key_type("JSON".into(), window, cx);
        assert_eq!(self.new_key_value(cx), expected("type-reset-json"));
        self.change_new_key_type("HASH".into(), window, cx);
        self.append_new_key_row(0, window, cx);
        self.append_new_key_row(0, window, cx);
        let rows = self.dialog.as_ref().unwrap().rows.clone();
        assert_ne!(rows[0].id.entity_id(), rows[1].id.entity_id());
        self.remove_new_key_row(1, cx);
        self.remove_new_key_row(0, cx);
        self.remove_new_key_row(0, cx);
        assert_eq!(
            self.dialog.as_ref().unwrap().rows.len(),
            1,
            "last row has no remove action"
        );
        println!(
            "NEW KEY MODEL QA PASS: original defaults, filters, duplicate rules, reset, null score, stream ID and row removal"
        );
    }

    pub(in crate::ui) fn qa_prepare_new_key(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let record = source(scene);
        if scene.contains("dark") {
            self.pref.general.theme = "dark".into();
            self.apply_theme(window, cx);
        }
        self.nav = Nav::Server;
        self.open_dialog(DialogKind::NewKey, window, cx);
        let kind = if scene.contains("hash") {
            "HASH"
        } else if scene.contains("zset") {
            "ZSET"
        } else if scene.contains("stream") {
            "STREAM"
        } else if scene.contains("list") {
            "LIST"
        } else if scene.contains("json") {
            "JSON"
        } else if scene.ends_with("-set") {
            "SET"
        } else {
            "STRING"
        };
        self.change_new_key_type(kind.into(), window, cx);
        if scene.starts_with("new-key-value-") {
            self.dialog
                .as_mut()
                .unwrap()
                .new_key
                .as_mut()
                .unwrap()
                .value_height =
                record["inputs"].as_array().unwrap().last().unwrap()["control"]["height"]
                    .as_f64()
                    .unwrap() as f32;
        }
        let fields = record["inputs"].as_array().unwrap();
        self.set_dialog_field("key", fields[0]["value"].as_str().unwrap(), window, cx);
        self.set_dialog_field("ttl", fields[2]["value"].as_str().unwrap(), window, cx);
        if matches!(kind, "HASH" | "LIST" | "SET" | "ZSET" | "STREAM") {
            let count = record["dynamic"].as_array().unwrap().len();
            while self.dialog.as_ref().unwrap().rows.len() < count {
                let index = self.dialog.as_ref().unwrap().rows.len() - 1;
                self.append_new_key_row(index, window, cx);
            }
            let offset = if kind == "STREAM" { 4 } else { 3 };
            for index in 0..count {
                let row = self.dialog.as_ref().unwrap().rows[index].clone();
                let pair = matches!(kind, "HASH" | "ZSET" | "STREAM");
                let first = offset + index * if pair { 2 } else { 1 };
                let text = |i: usize| fields[i]["value"].as_str().unwrap();
                if matches!(kind, "HASH" | "STREAM") {
                    set_input(&row.id, text(first).to_string(), window, cx);
                    set_input(&row.value, text(first + 1).to_string(), window, cx);
                } else {
                    set_input(&row.value, text(first).to_string(), window, cx);
                    if kind == "ZSET" {
                        set_input(&row.score, text(first + 1).to_string(), window, cx);
                    }
                }
            }
            if kind == "STREAM" {
                self.set_dialog_field(
                    "stream_id",
                    fields[3]["value"].as_str().unwrap(),
                    window,
                    cx,
                );
            }
        }
        if scene == "new-key-required" {
            self.submit_dialog(window, cx);
        }
        if scene == "new-key-type-menu" {
            self.open_new_key_selector(Selector::Type, window, cx);
        } else if scene.starts_with("new-key-db-") {
            self.open_new_key_selector(Selector::Database, window, cx);
            self.set_dialog_field(
                "new_db_query",
                if scene.ends_with("empty") { "01" } else { "1" },
                window,
                cx,
            );
        } else if let Some(index) = fields.iter().position(|field| field["focused"] == true) {
            let dialog = self.dialog.as_ref().unwrap();
            let state = if index == 0 {
                dialog.fields["key"].clone()
            } else if index == 2 {
                dialog.fields["ttl"].clone()
            } else if matches!(kind, "STRING" | "JSON") {
                dialog.fields["value"].clone()
            } else {
                let offset = if kind == "STREAM" { 4 } else { 3 };
                let pair = matches!(kind, "HASH" | "ZSET" | "STREAM");
                let row = &dialog.rows[(index - offset) / if pair { 2 } else { 1 }];
                if matches!(kind, "HASH" | "STREAM") && (index - offset) % 2 == 0 {
                    row.id.clone()
                } else if kind == "ZSET" && (index - offset) % 2 == 1 {
                    row.score.clone()
                } else {
                    let RowValue::Single(state) = &row.value else {
                        unreachable!()
                    };
                    state.clone()
                }
            };
            state.update(cx, |state, cx| state.focus(window, cx));
        } else if matches!(
            scene,
            "new-key-hash"
                | "new-key-list"
                | "new-key-set"
                | "new-key-zset"
                | "new-key-stream"
                | "new-key-json"
        ) {
            self.dialog
                .as_ref()
                .unwrap()
                .new_key
                .as_ref()
                .unwrap()
                .type_focus
                .focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        if scene == "new-key-list-scroll" {
            self.dialog
                .as_ref()
                .unwrap()
                .new_key
                .as_ref()
                .unwrap()
                .scroll
                .set_offset(point(px(0.), px(-318.)));
        }
        cx.notify();
    }

    pub(in crate::ui) fn qa_assert_new_key_layout(
        &self,
        scene: &str,
        window: &mut Window,
        cx: &App,
    ) {
        let source = source(scene);
        let rect = |id: ElementId| {
            gpui_base::test_support::find(window, &[], &id)
                .unwrap()
                .bounds()
        };
        let assert_rect = |id: ElementId, expected: &Json, tolerance: f32| {
            let actual = rect(id.clone());
            for (key, value) in [
                ("x", actual.origin.x.as_f32()),
                ("y", actual.origin.y.as_f32()),
                ("width", actual.size.width.as_f32()),
                ("height", actual.size.height.as_f32()),
            ] {
                let expected = expected[key].as_f64().unwrap() as f32;
                assert!(
                    (value - expected).abs() < tolerance,
                    "{scene} {id:?} {key}: {value} vs {expected}"
                );
            }
        };
        assert_rect("dialog-card".into(), &source["bounds"], 1.2);
        assert_rect("new-key-scroll".into(), &source["scroll"], 1.5);
        for (key, index) in [("key", 0usize), ("db", 1), ("type", 2), ("ttl", 3)] {
            assert_rect(
                SharedString::from(format!("new-key-field-{key}")).into(),
                &source["items"][index]["bounds"],
                1.5,
            );
        }
        if let Some(menu) = source["menus"].as_array().unwrap().first() {
            assert_rect("new-key-options".into(), &menu["bounds"], 1.5);
        }
        let dialog = self.dialog.as_ref().unwrap();
        let kind = dialog.value("type", cx);
        assert_rect("new-key-key".into(), &source["inputs"][0]["control"], 1.5);
        assert_rect("new-key-db".into(), &source["selects"][0]["bounds"], 1.5);
        assert_rect("new-key-type".into(), &source["selects"][1]["bounds"], 1.5);
        assert_rect("new-key-ttl".into(), &source["inputs"][2]["control"], 1.5);
        if matches!(kind.as_str(), "STRING" | "JSON") {
            assert_rect("new-key-value".into(), &source["inputs"][3]["control"], 1.5);
        } else {
            let offset = if kind == "STREAM" { 4 } else { 3 };
            if kind == "STREAM" {
                assert_rect(
                    "new-key-stream-id".into(),
                    &source["inputs"][3]["control"],
                    1.5,
                );
            }
            for (index, row) in source["dynamic"].as_array().unwrap().iter().enumerate() {
                assert_rect(("new-key-row", index).into(), &row["bounds"], 1.5);
                let pair = matches!(kind.as_str(), "HASH" | "STREAM" | "ZSET");
                let first = offset + index * if pair { 2 } else { 1 };
                if matches!(kind.as_str(), "HASH" | "STREAM") {
                    assert_rect(
                        ("new-key-row-field", index).into(),
                        &source["inputs"][first]["control"],
                        1.5,
                    );
                    assert_rect(
                        ("new-key-row-value", index).into(),
                        &source["inputs"][first + 1]["control"],
                        1.5,
                    );
                } else {
                    assert_rect(
                        ("new-key-row-value", index).into(),
                        &source["inputs"][first]["control"],
                        1.5,
                    );
                    if kind == "ZSET" {
                        assert_rect(
                            ("new-key-row-score", index).into(),
                            &source["inputs"][first + 1]["control"],
                            1.5,
                        );
                    }
                }
            }
        }
        for (id, label) in [
            ("new-key-import", "导入数据"),
            ("dialog-cancel", "取消"),
            ("dialog-save", "确认"),
        ] {
            let expected = source["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .find(|button| button["text"] == label)
                .unwrap();
            assert_rect(SharedString::from(id).into(), &expected["bounds"], 1.5);
        }
        assert_eq!(
            dialog.rows.len(),
            source["dynamic"].as_array().unwrap().len().max(1)
        );
        assert!(
            dialog.editors.is_empty(),
            "NewKey uses ordinary input fields"
        );
        assert!(!dialog.fields.contains_key("format"));
        assert!(!dialog.fields.contains_key("decode"));
        assert_eq!(dialog.value("ttl", cx), "-1");
        println!("QA PASS: {scene} original form geometry and plain controls");
    }
}
