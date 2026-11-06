use super::*;
use std::{io::Write, net::TcpListener};

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let config = Connection {
        name: "key-scan-qa".into(),
        port: listener.local_addr().unwrap().port(),
        exec_timeout: 2,
        ..Default::default()
    };
    let (release, gate) = mpsc::channel();
    let commands = Arc::new(Mutex::new(Vec::<Vec<Vec<u8>>>::new()));
    let blocked = Arc::new(Mutex::new(String::new()));
    let first_reply = Arc::new(Mutex::new(None::<Instant>));
    let (log, waiting, replied) = (commands.clone(), blocked.clone(), first_reply.clone());
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream);
        let mut types = 0;
        let mut failed = false;
        while let Ok(value) = crate::redis::read_value(&mut reader) {
            let args = value
                .array()
                .unwrap()
                .iter()
                .map(|v| v.bytes())
                .collect::<Vec<_>>();
            log.lock().push(args.clone());
            let reply = match args[0].as_slice() {
                b"PING" => "+PONG\r\n".to_string(),
                b"DBSIZE" => ":2000000\r\n".into(),
                b"TYPE" => {
                    types += 1;
                    "+string\r\n".into()
                }
                b"TTL" => ":-1\r\n".into(),
                b"MEMORY" => ":5\r\n".into(),
                b"OBJECT" => "+raw\r\n".into(),
                b"GET" => "$5\r\nvalue\r\n".into(),
                b"SCAN" => {
                    assert_eq!(args[5], b"3000");
                    let pattern = String::from_utf8(args[3].clone()).unwrap();
                    let cursor = String::from_utf8(args[1].clone()).unwrap();
                    if pattern == "*replacement*" {
                        "*2\r\n$1\r\n0\r\n*1\r\n$11\r\nreplacement\r\n".into()
                    } else if cursor == "0" {
                        "*2\r\n$1\r\n7\r\n*3\r\n$9\r\nstream-10\r\n$8\r\nstream-2\r\n$8\r\nstream-2\r\n".into()
                    } else if pattern == "*progress*" && types == 0 {
                        let next = (cursor.parse::<u64>().unwrap() + 1).to_string();
                        format!("*2\r\n${}\r\n{next}\r\n*0\r\n", next.len())
                    } else if pattern == "*error*" && failed {
                        "*2\r\n$1\r\n0\r\n*2\r\n$8\r\nstream-0\r\n$8\r\nstream-2\r\n".into()
                    } else if pattern == "*error*" && cursor == "7" {
                        "*2\r\n$1\r\n8\r\n*2\r\n$8\r\nstream-1\r\n$8\r\nstream-2\r\n".into()
                    } else {
                        *waiting.lock() = pattern.clone();
                        gate.recv_timeout(Duration::from_secs(5)).unwrap();
                        waiting.lock().clear();
                        if pattern == "*error*" {
                            failed = true;
                            "-ERR synthetic scan failure\r\n".into()
                        } else {
                            "*2\r\n$1\r\n0\r\n*1\r\n$8\r\nlate-key\r\n".into()
                        }
                    }
                }
                _ => panic!("unexpected QA command: {args:?}"),
            };
            std::thread::sleep(Duration::from_millis(20));
            if args[0] == b"SCAN" && args[1] == b"0" {
                *replied.lock() = Some(Instant::now());
            }
            reader.get_mut().write_all(reply.as_bytes()).unwrap();
        }
    });
    let (service, original, id, scan_size, flat, status_auto) = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let original = app.tabs[i].conn.clone();
                let scan_size = app.pref.general.scan_size;
                let tab = &mut app.tabs[i];
                let (flat, status_auto) = (tab.flat, tab.status_auto);
                tab.status_auto = false;
                tab.flat = true;
                tab.conn = config.clone();
                tab.key_type.clear();
                tab.exact = false;
                app.pref.general.scan_size = 3000;
                let id = app.tabs[i].id;
                set_input(&app.tabs[i].filter, "progress", window, cx);
                app.search_keys(id, window, cx);
                (
                    app.service.clone(),
                    original,
                    id,
                    scan_size,
                    flat,
                    status_auto,
                )
            })
        })
        .unwrap();
    for _ in 0..100 {
        if window
            .update(cx, |_, _, cx| {
                !view.read(cx).tabs[view.read(cx).tab_index(id).unwrap()]
                    .keys
                    .is_empty()
            })
            .unwrap()
        {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
    let first_ms = cx
        .update_window(window.into(), |_, window, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.tab_index(id).unwrap()];
            assert!(tab.keys_loading && !tab.scan.end);
            assert_eq!(
                tab.keys
                    .iter()
                    .map(|k| k.key.as_slice())
                    .collect::<Vec<_>>(),
                vec![b"stream-2".as_slice(), b"stream-10".as_slice()]
            );
            window.refresh();
            window.draw(cx).clear(cx);
            assert!(
                gpui_base::test_support::find(window, &[], &"key-browser-loading".into()).is_none()
            );
            assert!(
                gpui_base::test_support::find(window, &[], &"key-browser-scanning".into())
                    .is_some()
            );
            assert!(
                !gpui_base::test_support::find(window, &[], &"load-more-keys".into())
                    .unwrap()
                    .supports_click()
            );
            let position =
                gpui_base::test_support::find(window, &[], &("key-label", 0usize).into())
                    .unwrap()
                    .bounds()
                    .center();
            let first_ms = first_reply.lock().unwrap().elapsed().as_secs_f64() * 1000.;
            assert!(
                first_ms < 200.,
                "first matches must paint before the rest of the search finishes: {first_ms}ms"
            );
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
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("down").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            first_ms
        })
        .unwrap();
    for _ in 0..100 {
        if *blocked.lock() == "*progress*" {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_eq!(*blocked.lock(), "*progress*");
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                assert_eq!(
                    app.tabs[i].current_key.as_deref(),
                    Some(b"stream-10".as_slice()),
                    "keyboard selection works during scanning"
                );
                assert!(app.tabs[i].keys_loading);
                assert!(
                    app.tabs[i]
                        .key_types
                        .states
                        .values()
                        .any(|state| matches!(state, key_types::State::Ready(KeyType::String))),
                    "visible TYPE batches complete during scanning"
                );
                let old = app.tabs[i].scan_cancel.clone();
                set_input(&app.tabs[i].filter, "replacement", window, cx);
                app.search_keys(id, window, cx);
                assert!(old.canceled());
            })
        })
        .unwrap();
    release.send(()).unwrap();
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                assert_eq!(
                    app.tabs[i]
                        .keys
                        .iter()
                        .map(|k| k.key.as_slice())
                        .collect::<Vec<_>>(),
                    vec![b"replacement".as_slice()]
                );
                assert!(
                    app.messages
                        .iter()
                        .all(|notice| notice.kind != notices::MessageKind::Error)
                );
                set_input(&app.tabs[i].filter, "error", window, cx);
                app.search_keys(id, window, cx);
            })
        })
        .unwrap();
    for _ in 0..100 {
        if *blocked.lock() == "*error*" {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    assert_eq!(*blocked.lock(), "*error*");
    release.send(()).unwrap();
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let tab = &app.tabs[i];
                assert!(!tab.keys_loading && !tab.scan.end && tab.keys.len() == 3);
                assert_eq!(
                    tab.scan.cursor, 8,
                    "failure flushes pending results and their corresponding cursor"
                );
                assert!(
                    app.messages
                        .iter()
                        .any(|notice| notice.kind == notices::MessageKind::Error)
                );
                app.messages.clear();
                app.scan_keys(id, false, false, window, cx);
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.tab_index(id).unwrap()];
            assert!(tab.scan.end && !tab.keys_loading);
            assert_eq!(
                tab.keys
                    .iter()
                    .map(|k| k.key.as_slice())
                    .collect::<Vec<_>>(),
                vec![
                    b"stream-0".as_slice(),
                    b"stream-1".as_slice(),
                    b"stream-2".as_slice(),
                    b"stream-10".as_slice()
                ]
            );
            assert_eq!(app.busy, 0);
            assert_eq!(app.key_scans, 0);
        })
        .unwrap();
    assert_eq!(
        commands
            .lock()
            .iter()
            .filter(|args| args[0] == b"DBSIZE")
            .count(),
        4,
        "one DBSIZE per search/load operation"
    );
    let closing_service = service.clone();
    let name = config.name.clone();
    cx.background_executor()
        .spawn(async move {
            closing_service.close(&name);
        })
        .await;
    server.join().unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let tab = &mut app.tabs[i];
                tab.detail_cancel.cancel();
                tab.current_key = None;
                tab.key_selection = None;
                tab.detail = None;
                tab.pane = Pane::Status;
                tab.conn = original;
                tab.flat = flat;
                tab.status_auto = status_auto;
                tab.key_match.clear();
                app.pref.general.scan_size = scan_size;
                app.scan_keys(id, true, false, window, cx);
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    println!(
        "QA PASS: progressive key scan; first matches visible in {first_ms:.2}ms; 20ms replies; pointer/keyboard/types work while scanning; replacement cancels old work; failed batches retain keys/cursor; retry merges without duplicates"
    );
}
