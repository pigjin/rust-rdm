use super::new_key_qa::{click, draw, idle, type_field};
use super::*;

pub(super) fn text(view: &Entity<RdmApp>, value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        let state = view.read(cx).dialog.as_ref().unwrap().editors["value"].clone();
        state.update(cx, |state, cx| {
            state.focus(w, cx);
            let length = state.value().len();
            state.replace_text_in_range(Some(0..length), value, w, cx);
        });
    })
    .unwrap();
}

pub(super) fn save_shortcut(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        view.read(cx).dialog.as_ref().unwrap().editors["value"]
            .focus_handle(cx)
            .focus(w, cx);
        w.refresh();
        let _ = w.draw(cx);
        w.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse("cmd-s").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    let source: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/edit-entries-behavior-source.json"
    ))
    .unwrap();
    assert_eq!(source["checks"].as_array().unwrap().len(), 16);
    let interactions: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/entry-panel-interactions-source.json"
    ))
    .unwrap();
    assert_eq!(interactions["checks"].as_array().unwrap().len(), 8);
    for sample in source["scoreSamples"].as_array().unwrap() {
        assert_eq!(
            super::edit_entries::score_value(sample["input"].as_str().unwrap()),
            sample["value"].as_f64().unwrap(),
            "{sample:?}"
        );
    }
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                if app.index().is_none() {
                    let connection = flatten(&app.connections).first().unwrap().clone();
                    app.open_connection(connection, w, cx);
                }
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                if !app.editing_entry() {
                    let id = app.active.unwrap();
                    app.select_key(id, b"aahash".to_vec(), w, cx);
                }
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                if !app.editing_entry() {
                    let row =
                        app.tabs[app.index().unwrap()].detail.as_ref().unwrap().rows[0].clone();
                    app.open_dialog(DialogKind::EditEntry(row), w, cx);
                }
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    let (service, connection, backup) = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                let tab = &app.tabs[i];
                (
                    app.service.clone(),
                    tab.conn.clone(),
                    (
                        tab.db,
                        tab.current_key.clone(),
                        tab.detail.clone(),
                        tab.row_values.clone(),
                        tab.removed_fields.clone(),
                        app.dialog.take(),
                        app.full_editor,
                    ),
                )
            })
        })
        .unwrap();
    let folder = std::path::Path::new(&connection.sock).parent().unwrap();
    assert_eq!(
        std::fs::read_to_string(folder.join("RDM-QA-FIXTURE")).unwrap(),
        "rust-rdm-render-fixture-v1\n"
    );
    let run = |args: &[&str]| {
        service
            .run(&connection, 15, crate::redis::command(args))
            .unwrap()
    };
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    let key = "edit-entries-uiqa:target";
    for empty in [false, true] {
        for kind in [KeyType::Hash, KeyType::List, KeyType::Set, KeyType::ZSet] {
            run(&["DEL", key]);
            match kind {
                KeyType::Hash => {
                    run(&["HSET", key, "old", "before", "other", "kept"]);
                }
                KeyType::List => {
                    run(&["RPUSH", key, "before", "kept"]);
                }
                KeyType::Set => {
                    run(&["SADD", key, "before", "kept"]);
                }
                KeyType::ZSet => {
                    run(&["ZADD", key, "1", "before", "3", "kept"]);
                }
                _ => unreachable!(),
            }
            let mut detail = service
                .detail(&connection, 15, key.as_bytes(), 0, "", 100, "", false)
                .unwrap();
            detail.rows.sort_by_key(|row| row.value.clone());
            detail.rows.truncate(1);
            detail.cursor = 77;
            detail.end = false;
            let row = detail.rows[0].clone();
            window
                .update(cx, |_, w, cx| {
                    view.update(cx, |app, cx| {
                        let i = app.index().unwrap();
                        let tab = &mut app.tabs[i];
                        tab.db = 15;
                        tab.current_key = Some(key.as_bytes().to_vec());
                        tab.detail = Some(detail);
                        tab.row_values.clear();
                        tab.removed_fields.clear();
                        tab.decode = "None".into();
                        tab.format = "Raw".into();
                        app.open_dialog(DialogKind::EditEntry(row), w, cx);
                    })
                })
                .unwrap();
            idle(view, window, cx).await;
            if matches!(kind,KeyType::List|KeyType::Set){
                window.update(cx,|_,_,cx|assert_eq!(view.read(cx).dialog.as_ref().unwrap().value("row_index",cx),"1","{kind:?} retains the original position after initial conversion")).unwrap();
            }
            assert!(
                !window
                    .update(cx, |_, _, cx| view.read(cx).entry_changed(cx))
                    .unwrap()
            );
            if !empty {
                save_shortcut(view, window, cx);
                assert!(
                    window
                        .update(cx, |_, _, cx| view.read(cx).dialog.is_none())
                        .unwrap(),
                    "Source unchanged editor shortcut closes an unpinned editor"
                );
                idle(view, window, cx).await;
                window
                    .update(cx, |_, w, cx| {
                        view.update(cx, |app, cx| {
                            let row = app.tabs[app.index().unwrap()].detail.as_ref().unwrap().rows
                                [0]
                            .clone();
                            app.open_dialog(DialogKind::EditEntry(row), w, cx);
                        })
                    })
                    .unwrap();
                idle(view, window, cx).await;
                click("pin-entry".into(), window, cx);
            }
            text(view, if empty { "" } else { "next" }, window, cx);
            if empty && kind == KeyType::Hash {
                type_field(view, "field", "", window, cx);
            }
            if kind == KeyType::ZSet {
                type_field(
                    view,
                    "score",
                    if empty { "2.5suffix" } else { "2.5" },
                    window,
                    cx,
                );
            }
            draw(window, cx).await;
            window.update(cx,|_,_,cx|{
                let app=view.read(cx);let dialog=app.dialog.as_ref().unwrap();
                let expected=if empty{""}else{"next"};
                assert_eq!(text_input(&dialog.editors["value"],cx),expected,"{kind:?} visible input before Save");
                assert_eq!(dialog.value("value",cx),expected,"{kind:?} internal input before Save, dialog {}",dialog.id);
            }).unwrap();
            let start = service.history.lock().len();
            click("save-entry".into(), window, cx);
            if empty {
                assert!(
                    window
                        .update(cx, |_, _, cx| view.read(cx).dialog.is_none())
                        .unwrap()
                );
            }
            idle(view, window, cx).await;
            assert!(
                service.history.lock().iter().skip(start).all(|entry| ![
                    "HSCAN", "SSCAN", "ZRANGE", "LRANGE", "XRANGE"
                ]
                .iter()
                .any(|name| entry.command.starts_with(name))),
                "saving must preserve the loaded page"
            );
            if !empty {
                let expected = source["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|sample| {
                        sample["name"] == format!("{}-pinned-success", kind.name().to_lowercase())
                    })
                    .unwrap()["enabled"]
                    .as_bool()
                    .unwrap();
                window
                    .update(cx, |_, _, cx| {
                        let app = view.read(cx);
                        assert!(app.editing_entry());
                        assert_eq!(app.entry_changed(cx), expected);
                        let detail = app.tabs[app.index().unwrap()].detail.as_ref().unwrap();
                        assert_eq!(detail.rows.len(), 1);
                        assert_eq!(detail.rows[0].value, b"next", "{kind:?} pinned detail after save; messages {:?}",app.messages.iter().map(|message|message.text.as_str()).collect::<Vec<_>>());
                        assert_eq!(detail.cursor, 77);
                        assert!(!detail.end);
                        assert_eq!(detail.length, 2);
                    })
                    .unwrap();
                if kind == KeyType::Hash {
                    save_shortcut(view, window, cx);
                    idle(view, window, cx).await;
                    window
                        .update(cx, |_, _, cx| {
                            let app = view.read(cx);
                            assert!(app.editing_entry());
                            assert!(!app.entry_changed(cx));
                        })
                        .unwrap();
                }
            }
            match kind {
                KeyType::Hash => assert_eq!(
                    run(&["HGET", key, "old"]).bytes(),
                    if empty {
                        b"".as_slice()
                    } else {
                        b"next".as_slice()
                    }
                ),
                KeyType::List => assert_eq!(
                    run(&["LINDEX", key, "0"]).bytes(),
                    if empty {
                        b"before".as_slice()
                    } else {
                        b"next".as_slice()
                    }
                ),
                KeyType::Set => assert_eq!(
                    run(&["SISMEMBER", key, if empty { "" } else { "next" }])
                        .integer()
                        .unwrap(),
                    1
                ),
                KeyType::ZSet => assert_eq!(
                    run(&["ZSCORE", key, if empty { "before" } else { "next" }]).text(),
                    "2.5"
                ),
                _ => unreachable!(),
            }
        }
    }
    // An unpinned invalid Set conversion closes immediately, after SREM is sent.
    run(&["DEL", key]);
    run(&["SADD", key, "before"]);
    let detail = service
        .detail(&connection, 15, key.as_bytes(), 0, "", 100, "", false)
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.tabs[i].detail = Some(detail.clone());
                app.open_dialog(DialogKind::EditEntry(detail.rows[0].clone()), w, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window.update(cx,|_,w,cx|view.update(cx,|app,cx|app.set_dialog_field("format","Hex",w,cx))).unwrap();
    text(view, "zz", window, cx);
    draw(window, cx).await;
    click("save-entry".into(), window, cx);
    idle(view, window, cx).await;
    assert_eq!(run(&["TYPE", key]).text(), "none");
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(app.dialog.is_none());
            assert!(
                app.messages
                    .iter()
                    .any(|m| m.text == "save to type \"Hex\" fail: invalid Hex data")
            );
        })
        .unwrap();
    // Changing format reads saved props again, dropping both unsaved controls.
    run(&["HSET", key, "old", "before"]);
    let detail = service
        .detail(&connection, 15, key.as_bytes(), 0, "", 100, "", false)
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.tabs[i].detail = Some(detail.clone());
                app.open_dialog(DialogKind::EditEntry(detail.rows[0].clone()), w, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    text(view, "discard this draft", window, cx);
    type_field(view, "field", "discard-field", window, cx);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.change_entry_format("format", "JSON".into(), w, cx)
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let dialog = app.dialog.as_ref().unwrap();
            assert_eq!(dialog.value("field", cx), "old");
            assert_eq!(dialog.value("value", cx), "before");
            assert!(!app.entry_changed(cx));
        })
        .unwrap();
    run(&["DEL", key]);
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                tab.db = backup.0;
                tab.current_key = backup.1;
                tab.detail = backup.2;
                tab.row_values = backup.3;
                tab.removed_fields = backup.4;
                tab.decode = "None".into();
                tab.format = "Raw".into();
                app.dialog = backup.5;
                app.full_editor = backup.6;
                app.messages.clear();
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx).await;
    println!(
        "EDIT ENTRIES INPUT QA PASS: four actual editor saves, partial page/summary only, Source pin states and empty fallbacks, destructive Set conversion failure, format draft reset and DB15 cleanup"
    );
    println!(
        "ENTRY EDITOR SHORTCUT QA PASS: four unchanged unpinned saves and unchanged pinned Hash save via actual Cmd+S"
    );
}
