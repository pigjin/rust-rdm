use super::key_types::State;
use super::*;

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        })
        .unwrap();
    }
    qa_json::idle(view, window, cx).await;
    let (service, config, id, flat) = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            (app.service.clone(), tab.conn.clone(), tab.id, tab.flat)
        })
        .unwrap();
    let gate = service.client(&config, 0).unwrap();
    let rows = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let t = &mut app.tabs[i];
                t.key_types.reset();
                t.scan_epoch += 1;
                t.flat = true;
                t.keys = std::iter::once(b"aastring".to_vec())
                    .chain((1..300).map(|i| format!("lazy:{i}").into_bytes()))
                    .map(|key| KeyEntry {
                        key,
                        kind: KeyType::Unknown,
                    })
                    .collect();
                t.key_revision += 1;
                t.key_scroll.scroll_to_item(0, ScrollStrategy::Top);
                let rows = key_tree_cache::visible_rows(t);
                cx.notify();
                rows
            })
        })
        .unwrap();
    draw(window, cx);
    let pending = window
        .update(cx, |_, window, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.tab_index(id).unwrap()];
            assert!(!tab.keys_loading && tab.key_types.in_flight);
            assert!(
                gpui_base::test_support::find(window, &[], &("key-label", 0usize).into()).is_some(),
                "names paint while TYPE is blocked"
            );
            assert!((1..=64).contains(&tab.key_types.states.len()));
            tab.key_types.states.len()
        })
        .unwrap();
    // Dispatch a real click while TYPE holds the client; selection must respond immediately.
    cx.update_window(window.into(), |_, window, cx| {
        let position = gpui_base::test_support::find(window, &[], &("key-label", 0usize).into())
            .unwrap()
            .bounds()
            .center();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.tab_index(id).unwrap();
                let t = &mut app.tabs[i];
                assert_eq!(t.current_key.as_deref(), Some(b"aastring".as_slice()));
                t.key_scroll.scroll_to_item(150, ScrollStrategy::Top);
            })
        })
        .unwrap();
    for _ in 0..3 {
        draw(window, cx);
    }
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.tabs[app.tab_index(id).unwrap()].key_types.states.len(),
                pending
            );
        })
        .unwrap();
    drop(gate);
    qa_json::idle(view, window, cx).await;
    draw(window, cx);
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let t = &app.tabs[app.tab_index(id).unwrap()];
            assert_eq!(
                t.key_types.states.get(b"aastring".as_slice()),
                Some(&State::Ready(KeyType::String))
            );
            assert_eq!(
                t.key_types.states.get(b"lazy:150".as_slice()),
                Some(&State::Ready(KeyType::None))
            );
            assert!(
                !t.key_types.states.contains_key(b"lazy:100".as_slice()),
                "offscreen keys never queued"
            );
            assert!(
                Arc::ptr_eq(&rows, &key_tree_cache::visible_rows(t)),
                "type updates reuse the row cache"
            );
        })
        .unwrap();

    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                app.tabs[i].key_types.reset();
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let t = &app.tabs[app.tab_index(id).unwrap()];
            assert!(t.key_types.states.contains_key(b"lazy:150".as_slice()));
            assert!(
                !t.key_types.states.contains_key(b"aastring".as_slice()),
                "offscreen measurement must not trigger TYPE"
            );
        })
        .unwrap();

    // A collapsed folder does not prefetch any of its unknown children.
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let t = &mut app.tabs[i];
                t.flat = false;
                t.key_types.reset();
                t.expanded.clear();
                t.keys.retain(|k| k.key.starts_with(b"lazy:"));
                t.key_revision += 1;
                t.key_scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(
                app.tabs[app.tab_index(id).unwrap()]
                    .key_types
                    .states
                    .is_empty()
            );
        })
        .unwrap();

    let mut denied = config.clone();
    denied.name.push_str("-type-qa");
    denied.username = "redisdock-type-qa".into();
    denied.password = "fixture-type-password".into();
    service
        .run(
            &config,
            0,
            command(&[
                "ACL",
                "SETUSER",
                &denied.username,
                "on",
                ">fixture-type-password",
                "~*",
                "+ping",
                "-type",
            ]),
        )
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let t = &mut app.tabs[i];
                t.conn = denied.clone();
                t.flat = true;
                t.key_types.reset();
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    qa_json::idle(view, window, cx).await;
    let failures = service
        .history
        .lock()
        .iter()
        .filter(|log| log.server == denied.name)
        .count();
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let t = &app.tabs[app.tab_index(id).unwrap()];
            assert!(!t.keys.is_empty() && !t.key_types.states.is_empty());
            assert!(
                t.key_types
                    .states
                    .values()
                    .all(|state| *state == State::Failed)
            );
        })
        .unwrap();
    for _ in 0..3 {
        draw(window, cx);
    }
    assert_eq!(
        service
            .history
            .lock()
            .iter()
            .filter(|log| log.server == denied.name)
            .count(),
        failures
    );
    assert!(
        service
            .history
            .lock()
            .iter()
            .any(|log| log.server == denied.name && !log.error.is_empty())
    );
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                app.tabs[i].conn = config.clone();
                app.scan_keys(id, true, false, w, cx);
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    draw(window, cx);
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let t = &app.tabs[app.tab_index(id).unwrap()];
            assert_eq!(
                t.key_types.states.get(b"aastring".as_slice()),
                Some(&State::Ready(KeyType::String)),
                "refresh retries failed visible types"
            );
            assert!(
                !t.key_types
                    .states
                    .values()
                    .any(|state| *state == State::Failed)
            );
        })
        .unwrap();
    service.close(&denied.name);
    service
        .run(&config, 0, command(&["ACL", "DELUSER", &denied.username]))
        .unwrap();

    // DB changes invalidate a real in-flight request waiting for the DB0 client.
    let gate = service.client(&config, 0).unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let t = &mut app.tabs[i];
                t.key_types.reset();
                t.current_key = None;
                t.detail = None;
                t.pane = Pane::Status;
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.change_database(id, 1, w, cx))
        })
        .unwrap();
    drop(gate);
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let t = &app.tabs[app.tab_index(id).unwrap()];
                assert_eq!(t.db, 1);
                assert!(t.keys.is_empty() && t.key_types.states.is_empty());
                app.change_database(id, 0, w, cx);
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    let gate = service.client(&config, 0).unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                app.tabs[i].key_types.reset();
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    let token = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let t = &app.tabs[app.tab_index(id).unwrap()];
                assert!(t.key_types.in_flight);
                let token = t.key_types.cancel.clone();
                app.remove_closed_tab(&config.name, cx);
                token
            })
        })
        .unwrap();
    assert!(token.canceled());
    drop(gate);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.open_connection(config.clone(), w, cx))
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let t = &mut app.tabs[i];
                assert_ne!(t.id, id);
                t.flat = flat;
                key_tree_cache::verify(t);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx);
    qa_json::idle(view, window, cx).await;
    println!(
        "QA PASS: lazy key types: visible/clickable before TYPE, bounded requests, scrolling, collapsed folders, failure/retry, DB/close isolation, cached tree reuse"
    );
}
