use super::*;
use dialogs::DialogKind;

pub(in crate::ui) struct Saved {
    connections: Vec<Connection>,
    path: PathBuf,
    bytes: Vec<u8>,
    id: u64,
}

async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(30))
        .await;
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
}

fn disabled(window: WindowHandle<Root>, ids: &[&str], cx: &mut AsyncApp) {
    window
        .update(cx, |_, w, _| {
            for id in ids {
                let node = gpui_base::test_support::find(
                    w,
                    &[],
                    &SharedString::from(id.to_string()).into(),
                )
                .unwrap();
                assert!(node.visible(), "{id} must remain visible");
                if let Some(disabled) = node.disabled() {
                    assert!(disabled, "{id} must be disabled");
                }
            }
        })
        .unwrap();
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    qa_json::idle(view, window, cx).await;
    let (connections, path, bytes, c) = window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                let connections = app.connections.clone();
                let path = app.store.root.join("connections.yaml");
                let bytes = std::fs::read(&path).unwrap();
                let mut c = flatten(&connections)[0].clone();
                c.name = "readonly-qa".into();
                c.read_only = true;
                app.connections.push(c.clone());
                app.open_connection(c.clone(), w, cx);
                app.nav = Nav::Browser;
                app.pref.general.theme = if scene.ends_with("dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(w, cx);
                (connections, path, bytes, c)
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    let id = window
        .update(cx, |_, _, cx| view.read(cx).active.unwrap())
        .unwrap();
    assert!(
        window
            .update(cx, |_, _, cx| view.read(cx).tabs
                [view.read(cx).tab_index(id).unwrap()]
            .conn
            .read_only)
            .unwrap()
    );
    if scene.ends_with("connections") {
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, cx| {
                    app.nav = Nav::Server;
                    cx.notify();
                })
            })
            .unwrap();
    } else if scene.contains("settings") {
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.open_dialog(DialogKind::Connection(Some(c.clone())), w, cx)
                })
            })
            .unwrap();
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                let app = view.read(cx);
                assert!(app.dialog.as_ref().unwrap().boolean("read_only"));
                let node =
                    gpui_base::test_support::find(w, &[], &"check-read_only".into()).unwrap();
                assert!(node.visible());
            })
            .unwrap();
    } else if scene.contains("string") || scene.ends_with("entry") {
        let entry = scene.ends_with("entry");
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.select_key(
                        id,
                        if entry {
                            b"aahash".to_vec()
                        } else {
                            b"account:1".to_vec()
                        },
                        w,
                        cx,
                    );
                })
            })
            .unwrap();
        qa_json::idle(view, window, cx).await;
        if entry {
            window
                .update(cx, |_, w, cx| {
                    view.update(cx, |app, cx| {
                        let row = app.tabs[app.tab_index(id).unwrap()]
                            .detail
                            .as_ref()
                            .unwrap()
                            .rows[0]
                            .clone();
                        app.open_dialog(DialogKind::EditEntry(row), w, cx);
                    })
                })
                .unwrap();
            qa_json::idle(view, window, cx).await;
        }
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    if entry {
                        let dialog = app.dialog.as_ref().unwrap();
                        assert!(!dialog.editors["value"].read(cx).is_editable());
                        assert!(!dialog.fields["field"].read(cx).is_editable());
                        app.set_dialog_field("value", "readonly attempt", w, cx);
                    } else {
                        let i = app.tab_index(id).unwrap();
                        assert!(!app.tabs[i].editor.read(cx).is_editable());
                        set_input(&app.tabs[i].editor, "readonly attempt", w, cx);
                        let before = app.service.history.lock().len();
                        app.save_value(w, cx);
                        assert_eq!(app.service.history.lock().len(), before);
                        app.open_dialog(DialogKind::NewKey, w, cx);
                        assert!(app.dialog.is_none());
                        app.open_dialog(DialogKind::Import, w, cx);
                        assert!(app.dialog.is_none());
                        app.open_dialog(DialogKind::Flush, w, cx);
                        assert!(app.dialog.is_none());
                    }
                })
            })
            .unwrap();
        draw(window, cx).await;
        disabled(
            window,
            if entry {
                &["save-entry", "add-entry"]
            } else {
                &["save-value", "new-key", "ttl", "rename-key", "delete-key"]
            },
            cx,
        );
        if entry {
            qa_json::click("save-entry", window, cx);
        } else {
            for button in ["save-value", "new-key", "ttl", "rename-key", "delete-key"] {
                qa_json::click(button, window, cx);
                window
                    .update(cx, |_, _, cx| {
                        assert!(
                            view.read(cx).dialog.is_none(),
                            "{button} must not open an editor"
                        )
                    })
                    .unwrap();
            }
        }
        cx.update_window(window.into(), |_, w, cx| {
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
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    if entry {
                        app.set_dialog_field("value", "Tiny RDM", w, cx);
                    } else {
                        let i = app.tab_index(id).unwrap();
                        set_input(
                            &app.tabs[i].editor,
                            app.tabs[i].editor_original.clone(),
                            w,
                            cx,
                        );
                    }
                    app.messages.clear();
                })
            })
            .unwrap();
        if !entry {
            draw(window, cx).await;
            let previous = window
                .update(cx, |_, _, cx| cx.read_from_clipboard())
                .unwrap();
            qa_json::click("copy-value", window, cx);
            let copied = window
                .update(cx, |_, _, cx| qa::copy_result(previous, cx))
                .unwrap()
                .unwrap();
            assert!(
                copied.contains("Tiny RDM"),
                "copy remains available on a read-only editor"
            );
        }
    } else if scene.ends_with("cli") {
        for input in [
            "SELECT 0",
            "GET aastring",
            "SET aastring changed",
            "CONFIG SET timeout 1",
            "EVAL_RO 'return 1' 0",
        ] {
            window
                .update(cx, |_, w, cx| {
                    view.update(cx, |app, cx| {
                        app.switch_pane(id, Pane::Cli, w, cx);
                        let i = app.tab_index(id).unwrap();
                        set_input(&app.tabs[i].cli_input, input, w, cx);
                        app.execute_cli(id, w, cx);
                    })
                })
                .unwrap();
            qa_json::idle(view, window, cx).await;
        }
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.tab_index(id).unwrap()];
                assert!(
                    tab.cli_output
                        .iter()
                        .any(|line| line.text.contains("Hello Rust"))
                );
                assert_eq!(
                    tab.cli_output
                        .iter()
                        .filter(|line| line.kind == cli::LineKind::Error)
                        .count(),
                    3
                );
                assert!(read_only::cli_prompt(&c, 0).contains(&app.tr("read_only.badge")));
            })
            .unwrap();
    } else if scene.ends_with("pubsub") {
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.switch_pane(id, Pane::Pubsub, w, cx);
                    let i = app.tab_index(id).unwrap();
                    set_input(&app.tabs[i].pub_channel, "readonly-channel", w, cx);
                    set_input(&app.tabs[i].pub_message, "blocked", w, cx);
                    let before = app.service.history.lock().len();
                    app.publish(id, w, cx);
                    assert_eq!(app.service.history.lock().len(), before);
                })
            })
            .unwrap();
        draw(window, cx).await;
        disabled(window, &["publish"], cx);
    } else if scene.ends_with("tasks") {
        let writable = Connection {
            read_only: false,
            ..c.clone()
        };
        let (release, gate) = mpsc::channel();
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.write_job(
                        w,
                        cx,
                        writable.clone(),
                        move |_| -> Result<()> {
                            gate.recv_timeout(Duration::from_secs(10))?;
                            anyhow::bail!("QA canceled write")
                        },
                        |_, _, _, _| panic!("failed task must not apply"),
                    );
                    assert!(app.service.ensure_no_writes(&c.name).is_err());
                    assert!(app.service.ensure_no_writes("another-connection").is_ok());
                    app.close_tab(id, w, cx);
                    assert!(app.service.ensure_no_writes(&c.name).is_err());
                    app.open_connection(c.clone(), w, cx);
                    assert!(app.tab_index(id).is_none());
                    assert!(!app.tabs.iter().any(|tab| tab.conn.name == c.name));
                    app.open_dialog(DialogKind::Connection(Some(writable)), w, cx);
                    app.set_dialog_field("read_only", "true", w, cx);
                    app.submit_dialog(w, cx);
                    assert!(
                        app.dialog.is_some(),
                        "save must be refused while a write is pending"
                    );
                    assert!(app.service.ensure_no_writes(&c.name).is_err());
                })
            })
            .unwrap();
        release.send(()).unwrap();
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert!(app.service.ensure_no_writes(&c.name).is_ok());
                    app.submit_dialog(w, cx);
                    assert!(app.dialog.is_none(), "saving is allowed after worker exit");
                    assert!(
                        flatten(&app.store.connections().unwrap())
                            .iter()
                            .find(|profile| profile.name == c.name)
                            .unwrap()
                            .read_only
                    );
                })
            })
            .unwrap();
    }
    draw(window, cx).await;
    println!("READ ONLY QA PASS: {scene}");
    Saved {
        connections,
        path,
        bytes,
        id,
    }
}

pub(in crate::ui) async fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: Saved,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                if app.tab_index(saved.id).is_some() {
                    app.close_tab(saved.id, w, cx);
                }
                app.connections = saved.connections;
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    crate::storage::atomic_write(&saved.path, &saved.bytes).unwrap();
}
