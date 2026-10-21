use super::*;

fn fixture() -> Json {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/add-entries-source.json"
    ))
    .unwrap()
}
fn source(scene: &str) -> Json {
    fixture()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == scene)
        .unwrap()
        .clone()
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene != "add-entries-saving" {
        return;
    }
    use super::new_key_qa::{click, draw, idle, type_field};
    let (service, connection) = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            (
                app.service.clone(),
                app.tabs[app.index().unwrap()].conn.clone(),
            )
        })
        .unwrap();
    let folder = std::path::Path::new(&connection.sock).parent().unwrap();
    assert_eq!(
        std::fs::read_to_string(folder.join("RDM-QA-FIXTURE")).unwrap(),
        "rust-rdm-render-fixture-v1\n"
    );
    assert_eq!(
        service
            .run(&connection, 15, crate::redis::command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    let scratch = "add-entries-uiqa:target";
    service
        .run(
            &connection,
            15,
            crate::redis::command(&["RPUSH", scratch, "old"]),
        )
        .unwrap();
    let detail = service
        .detail(&connection, 15, scratch.as_bytes(), 0, "", 100, "", false)
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.tabs[i].db = 15;
                app.tabs[i].current_key = Some(scratch.as_bytes().to_vec());
                app.tabs[i].detail = Some(detail);
                app.tabs[i].row_values.clear();
                app.add_entries_reload = false;
                app.open_dialog(DialogKind::AddEntries, window, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    let row_key = |index: usize, window: WindowHandle<Root>, cx: &mut AsyncApp| {
        window
            .update(cx, |_, _, cx| {
                new_key::value_key(&view.read(cx).dialog.as_ref().unwrap().rows[index])
            })
            .unwrap()
    };
    type_field(view, &row_key(0, window, cx), "alpha", window, cx);
    click("new-key-add-0".into(), window, cx);
    draw(window, cx).await;
    type_field(view, &row_key(1, window, cx), "beta", window, cx);
    draw(window, cx).await;
    let start = service.history.lock().len();
    click("dialog-save".into(), window, cx);
    idle(view, window, cx).await;
    let no_full_read = service
        .history
        .lock()
        .iter()
        .skip(start)
        .filter(|log| log.db == 15)
        .all(|log| !log.command.starts_with("LRANGE"));
    let first = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            let detail = tab.detail.as_ref().unwrap();
            (
                app.dialog.is_none(),
                detail.length,
                detail
                    .rows
                    .iter()
                    .map(|row| row.value.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap();
    // Persist the checkbox across cancel/reopen, then require a real reload.
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_dialog(DialogKind::AddEntries, w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    click("check-reload".into(), window, cx);
    click("dialog-cancel".into(), window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_dialog(DialogKind::AddEntries, w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    let remembered = window
        .update(cx, |_, _, cx| {
            view.read(cx).dialog.as_ref().unwrap().boolean("reload")
        })
        .unwrap();
    type_field(view, &row_key(0, window, cx), "gamma", window, cx);
    draw(window, cx).await;
    let start = service.history.lock().len();
    click("dialog-save".into(), window, cx);
    idle(view, window, cx).await;
    let full_read = service
        .history
        .lock()
        .iter()
        .skip(start)
        .any(|log| log.db == 15 && log.command.starts_with("LRANGE"));
    // A held client lock gives a deterministic pending request without pausing
    // the Redis server. Cancel closes the UI while the authorized write finishes.
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_dialog(DialogKind::AddEntries, w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    type_field(view, &row_key(0, window, cx), "delta", window, cx);
    draw(window, cx).await;
    let guard = service.client(&connection, 15).unwrap();
    click("dialog-save".into(), window, cx);
    draw(window, cx).await;
    let pending = window
        .update(cx, |_, _, cx| view.read(cx).dialog.is_some())
        .unwrap();
    click("dialog-cancel".into(), window, cx);
    draw(window, cx).await;
    let canceled = window
        .update(cx, |_, _, cx| view.read(cx).dialog.is_none())
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_dialog(DialogKind::AddEntries, w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    type_field(view, &row_key(0, window, cx), "fresh_form", window, cx);
    draw(window, cx).await;
    let reopened = window
        .update(cx, |_, _, cx| view.read(cx).dialog.is_some())
        .unwrap();
    drop(guard);
    idle(view, window, cx).await;
    let reopened_closed = window
        .update(cx, |_, _, cx| view.read(cx).dialog.is_none())
        .unwrap();
    let saved = service
        .run(
            &connection,
            15,
            crate::redis::command(&["LRANGE", scratch, "0", "-1"]),
        )
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .map(crate::redis::Value::text)
        .collect::<Vec<_>>();
    // A list request against a changed key type returns an error and still closes.
    service
        .run(
            &connection,
            15,
            crate::redis::command(&["SET", scratch, "wrong-type"]),
        )
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_dialog(DialogKind::AddEntries, w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    type_field(view, &row_key(0, window, cx), "bad", window, cx);
    draw(window, cx).await;
    click("dialog-save".into(), window, cx);
    idle(view, window, cx).await;
    let error_closed = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.dialog.is_none()
                && app
                    .messages
                    .iter()
                    .any(|message| message.text.contains("WRONGTYPE"))
        })
        .unwrap();
    service
        .run(&connection, 15, crate::redis::command(&["DEL", scratch]))
        .unwrap();
    assert!(first.0 && no_full_read);
    assert_eq!(first.1, 3);
    assert_eq!(
        first.2,
        vec![b"old".to_vec(), b"alpha".to_vec(), b"beta".to_vec()]
    );
    assert!(
        remembered
            && full_read
            && pending
            && canceled
            && reopened
            && reopened_closed
            && error_closed
    );
    assert_eq!(saved, vec!["old", "alpha", "beta", "gamma", "delta"]);
    assert_eq!(
        service
            .run(&connection, 15, crate::redis::command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.qa_prepare_add_entries(scene, w, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    println!(
        "ADD ENTRIES INPUT QA PASS: actual row inputs/buttons, no-reload metadata, remembered reload/full read, pending cancel, error close and DB15 cleanup"
    );
}

impl RdmApp {
    fn qa_add_model_kind(&mut self, kind: KeyType, w: &mut Window, cx: &mut Context<Self>) {
        let i = self.index().unwrap();
        self.tabs[i].db = 0;
        self.tabs[i].current_key = Some(b"model:key".to_vec());
        self.tabs[i].detail = Some(KeyDetail {
            key: b"model:key".to_vec(),
            kind,
            ..Default::default()
        });
        self.open_dialog(DialogKind::AddEntries, w, cx);
    }
    fn qa_add_model_input(
        &mut self,
        index: usize,
        name: &str,
        value: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = self.dialog.as_ref().unwrap().rows[index].clone();
        let key = match name {
            "field" => new_key::field_key(&row),
            "score" => new_key::score_key(&row),
            _ => new_key::value_key(&row),
        };
        self.set_dialog_field(&key, value, w, cx);
        self.new_key_input_changed(&key, cx);
    }
    pub(in crate::ui) fn qa_check_add_models(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        fn numbers(value: Json) -> Json {
            match value {
                Json::Number(n) => {
                    Json::Number(serde_json::Number::from_f64(n.as_f64().unwrap()).unwrap())
                }
                Json::Object(v) => {
                    Json::Object(v.into_iter().map(|(k, v)| (k, numbers(v))).collect())
                }
                Json::Array(v) => Json::Array(v.into_iter().map(numbers).collect()),
                v => v,
            }
        }
        let source = fixture();
        let expected = |name: &str, field: &str| {
            source["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == name)
                .unwrap()["request"]["body"][field]
                .clone()
        };
        for (kind, field) in [
            (KeyType::Hash, "fieldItems"),
            (KeyType::List, "items"),
            (KeyType::Set, "members"),
            (KeyType::ZSet, "valueScore"),
        ] {
            self.qa_add_model_kind(kind.clone(), w, cx);
            assert_eq!(
                self.new_key_value(cx),
                expected(&format!("untouched-{}", kind.name()), field)
            );
        }
        self.qa_add_model_kind(KeyType::Hash, w, cx);
        for (index, (field, value)) in [("name", "new"), ("name", "last"), ("", "ignored")]
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                self.append_new_key_row(index - 1, w, cx);
            }
            self.qa_add_model_input(index, "field", field, w, cx);
            self.qa_add_model_input(index, "value", value, w, cx);
        }
        assert_eq!(
            self.new_key_value(cx),
            expected("hash-filter-empty-duplicates", "fieldItems")
        );
        for (kind, name) in [
            (KeyType::List, "list-prepend-duplicates"),
            (KeyType::Set, "set-deduplicate"),
        ] {
            self.qa_add_model_kind(kind, w, cx);
            for index in 0..3 {
                if index > 0 {
                    self.append_new_key_row(index - 1, w, cx);
                }
                self.qa_add_model_input(index, "value", if index < 2 { "a" } else { "" }, w, cx);
            }
            assert_eq!(
                self.new_key_value(cx),
                expected(
                    name,
                    if name.starts_with("list") {
                        "items"
                    } else {
                        "members"
                    }
                )
            );
        }
        self.qa_add_model_kind(KeyType::ZSet, w, cx);
        self.qa_add_model_input(0, "value", "member", w, cx);
        self.qa_add_model_input(0, "score", "1.5", w, cx);
        self.append_new_key_row(0, w, cx);
        self.qa_add_model_input(1, "value", "member", w, cx);
        self.qa_add_model_input(1, "score", "-3", w, cx);
        assert_eq!(
            numbers(self.new_key_value(cx)),
            numbers(expected("zset-last-duplicate", "valueScore"))
        );
        self.qa_add_model_kind(KeyType::ZSet, w, cx);
        self.qa_add_model_input(0, "value", "member", w, cx);
        self.qa_add_model_input(0, "score", "", w, cx);
        assert_eq!(self.new_key_value(cx), expected("zset-null", "valueScore"));
        self.qa_add_model_kind(KeyType::ZSet, w, cx);
        self.qa_add_model_input(0, "value", "__proto__", w, cx);
        assert_eq!(self.new_key_value(cx), expected("zset-proto", "valueScore"));
        self.qa_add_model_kind(KeyType::Stream, w, cx);
        assert_eq!(self.new_key_value(cx), serde_json::json!(["*"]));
        self.qa_add_model_input(0, "field", "event", w, cx);
        self.qa_add_model_input(0, "value", "new", w, cx);
        self.set_dialog_field("stream_id", "1720000000001-0", w, cx);
        assert_eq!(
            self.new_key_value(cx),
            serde_json::json!(["*", "event", "new"])
        );
        // Preserve existing decoded rows and partial-page length while inserting.
        self.qa_add_model_kind(KeyType::List, w, cx);
        let target = self.dialog.as_ref().unwrap().target.clone().unwrap();
        let i = self.index().unwrap();
        self.tabs[i].detail.as_mut().unwrap().rows = vec![Entry {
            id: b"0".to_vec(),
            value: b"raw-old".to_vec(),
            ..Default::default()
        }];
        self.tabs[i].detail.as_mut().unwrap().length = 100;
        self.tabs[i].row_values.insert(
            b"0".to_vec(),
            codec::Converted {
                text: "decoded-old".into(),
                decode: "Base64".into(),
                format: "Raw".into(),
            },
        );
        self.apply_added_entries(
            &target,
            &serde_json::json!({"left":[{"index":0,"v":"raw-new"}]}),
            0,
            cx,
        );
        let detail = self.tabs[i].detail.as_ref().unwrap();
        assert_eq!(detail.length, 101);
        assert_eq!(detail.rows[0].value, b"raw-new");
        assert_eq!(detail.rows[1].value, b"raw-old");
        assert_eq!(detail.rows[1].id, b"1");
        assert_eq!(detail.rows[1].display_value.as_deref(), Some("decoded-old"));
        println!(
            "ADD ENTRIES MODEL QA PASS: defaults, filtering, duplicate/null/prototype rules, stream ID and preserved decoded partial page"
        );
    }

    pub(in crate::ui) fn qa_prepare_add_entries(
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
        let kind = if scene.contains("list") {
            KeyType::List
        } else if scene.contains("zset") {
            KeyType::ZSet
        } else if scene.ends_with("-set") {
            KeyType::Set
        } else if scene.contains("stream") {
            KeyType::Stream
        } else {
            KeyType::Hash
        };
        let key = if scene == "add-entries-binary-key" {
            vec![255, 0, 65]
        } else {
            record["inputs"][0]["value"]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec()
        };
        let i = self.index().unwrap();
        self.tabs[i].db = 0;
        self.tabs[i].current_key = Some(key.clone());
        self.tabs[i].detail = Some(KeyDetail {
            key,
            kind: kind.clone(),
            ..Default::default()
        });
        self.add_entries_reload = record["checkboxes"][0]["checked"].as_bool().unwrap();
        self.open_dialog(DialogKind::AddEntries, window, cx);
        let op = record["radios"]
            .as_array()
            .unwrap()
            .iter()
            .position(|row| row["checked"] == true)
            .unwrap_or(0);
        self.set_dialog_field("op_type", op.to_string(), window, cx);
        let count = record["dynamic"].as_array().unwrap().len();
        while self.dialog.as_ref().unwrap().rows.len() < count {
            let i = self.dialog.as_ref().unwrap().rows.len() - 1;
            self.append_new_key_row(i, window, cx);
        }
        let inputs = record["inputs"].as_array().unwrap();
        let offset = if kind == KeyType::Stream { 2 } else { 1 };
        let pair = matches!(kind, KeyType::Hash | KeyType::ZSet | KeyType::Stream);
        if kind == KeyType::Stream {
            self.set_dialog_field(
                "stream_id",
                inputs[1]["value"].as_str().unwrap(),
                window,
                cx,
            );
        }
        for index in 0..count {
            let row = self.dialog.as_ref().unwrap().rows[index].clone();
            let first = offset + index * if pair { 2 } else { 1 };
            let set =
                |app: &mut Self, key: &str, value: &str, w: &mut Window, cx: &mut Context<Self>| {
                    app.set_dialog_field(key, value, w, cx);
                    app.new_key_input_changed(key, cx);
                };
            if matches!(kind, KeyType::Hash | KeyType::Stream) {
                set(
                    self,
                    &new_key::field_key(&row),
                    inputs[first]["value"].as_str().unwrap(),
                    window,
                    cx,
                );
                set(
                    self,
                    &new_key::value_key(&row),
                    inputs[first + 1]["value"].as_str().unwrap(),
                    window,
                    cx,
                );
            } else {
                set(
                    self,
                    &new_key::value_key(&row),
                    inputs[first]["value"].as_str().unwrap(),
                    window,
                    cx,
                );
                if kind == KeyType::ZSet {
                    set(
                        self,
                        &new_key::score_key(&row),
                        inputs[first + 1]["value"].as_str().unwrap(),
                        window,
                        cx,
                    );
                }
            }
        }
        if let Some(index) = inputs.iter().position(|field| field["focused"] == true) {
            let dialog = self.dialog.as_ref().unwrap();
            let state = if index == 0 {
                dialog.fields["key"].clone()
            } else if kind == KeyType::Stream && index == 1 {
                dialog.fields["stream_id"].clone()
            } else {
                let row = &dialog.rows[(index - offset) / if pair { 2 } else { 1 }];
                if matches!(kind, KeyType::Hash | KeyType::Stream) && (index - offset) % 2 == 0 {
                    row.id.clone()
                } else if kind == KeyType::ZSet && (index - offset) % 2 == 1 {
                    row.score.clone()
                } else {
                    let RowValue::Single(state) = &row.value else {
                        unreachable!()
                    };
                    state.clone()
                }
            };
            state.update(cx, |state, cx| state.focus(window, cx));
        } else if scene.contains("ignore") || scene == "add-entries-dark-zset" {
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
        self.dialog
            .as_ref()
            .unwrap()
            .new_key
            .as_ref()
            .unwrap()
            .scroll
            .set_offset(point(
                px(0.),
                px(-(record["scrollTop"].as_f64().unwrap() as f32)),
            ));
        cx.notify();
    }

    pub(in crate::ui) fn qa_assert_add_entries_layout(
        &self,
        scene: &str,
        window: &mut Window,
        cx: &App,
    ) {
        let source = source(scene);
        let dialog = self.dialog.as_ref().unwrap();
        let rect = |id: ElementId, expected: &Json| {
            let actual = gpui_base::test_support::find(window, &[], &id)
                .unwrap()
                .bounds();
            for (name, value) in [
                ("x", actual.origin.x.as_f32()),
                ("y", actual.origin.y.as_f32()),
                ("width", actual.size.width.as_f32()),
                ("height", actual.size.height.as_f32()),
            ] {
                let expected = expected[name].as_f64().unwrap() as f32;
                assert!(
                    (value - expected).abs() < 1.6,
                    "{scene} {id:?} {name}: {value} vs {expected}"
                );
            }
        };
        rect("dialog-card".into(), &source["bounds"]);
        rect("add-entries-scroll".into(), &source["scroll"]);
        rect("add-entries-key".into(), &source["inputs"][0]["control"]);
        let kind = dialog.value("type", cx);
        let inputs = source["inputs"].as_array().unwrap();
        let offset = if kind == "STREAM" { 2 } else { 1 };
        let pair = matches!(kind.as_str(), "HASH" | "ZSET" | "STREAM");
        if kind == "STREAM" {
            rect("add-entries-stream-id".into(), &inputs[1]["control"]);
        }
        for (index, row) in source["dynamic"].as_array().unwrap().iter().enumerate() {
            rect(("new-key-row", index).into(), &row["bounds"]);
            let first = offset + index * if pair { 2 } else { 1 };
            if matches!(kind.as_str(), "HASH" | "STREAM") {
                rect(
                    ("new-key-row-field", index).into(),
                    &inputs[first]["control"],
                );
                rect(
                    ("new-key-row-value", index).into(),
                    &inputs[first + 1]["control"],
                );
            } else {
                rect(
                    ("new-key-row-value", index).into(),
                    &inputs[first]["control"],
                );
                if kind == "ZSET" {
                    rect(
                        ("new-key-row-score", index).into(),
                        &inputs[first + 1]["control"],
                    );
                }
            }
        }
        for (index, radio) in source["radios"].as_array().unwrap().iter().enumerate() {
            rect(
                SharedString::from(format!("add-entries-op-{index}")).into(),
                &radio["bounds"],
            );
        }
        rect("add-entries-actions".into(), &source["actions"]);
        assert_eq!(dialog.title, source["title"].as_str().unwrap());
        assert!(dialog.editors.is_empty());
        assert!(!dialog.fields.contains_key("format"));
        assert!(!dialog.fields.contains_key("decode"));
        assert_eq!(
            dialog.boolean("reload"),
            source["checkboxes"][0]["checked"].as_bool().unwrap()
        );
        println!("QA PASS: {scene} original controls, operations, reload and geometry");
    }
}
