use super::*;
use terminal::CliState;

fn enter(view: &Entity<RdmApp>, line: &str, window: &mut Window, cx: &mut App) {
    view.update(cx, |app, cx| {
        let i = app.index().unwrap();
        set_input(&app.tabs[i].cli_input, line, window, cx);
        app.tabs[i].cli_input.focus_handle(cx).focus(window, cx);
    });
    window.refresh();
    window.draw(cx).clear(cx);
    window.dispatch_event(
        PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        }),
        cx,
    );
}

fn check_ready(view: &Entity<RdmApp>, window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
    let app = view.read(cx);
    let tab = &app.tabs[app.index().unwrap()];
    assert_eq!(tab.cli_state, CliState::Ready);
    assert!(tab.cli_input.read(cx).is_editable());
    let state = tab.cli_view.borrow();
    let frame = state.frame.as_ref().unwrap();
    let row = frame.rows[frame.cursor.row]
        .cells
        .iter()
        .map(|cell| cell.contents())
        .collect::<String>();
    assert_eq!(
        row.trim_end(),
        read_only::cli_prompt(&tab.conn, tab.cli_db).trim_end()
    );
}

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    qa_json::idle(view, window, cx).await;
    let (service, config) = cx
        .update_window(window.into(), |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                app.tabs[i].conn.exec_timeout = 1;
                app.tabs[i].conn.conn_timeout = 1;
                (app.service.clone(), app.tabs[i].conn.clone())
            })
        })
        .unwrap();
    // Refresh a client created by an earlier CLI scene with the short test timeout.
    let s = service.clone();
    let c = config.clone();
    cx.background_executor()
        .spawn(async move {
            s.run_cli(&c, 0, command(&["PING"])).unwrap();
            s.reconnect_cli(&c).unwrap();
        })
        .await;

    cx.update_window(window.into(), |_, window, cx| {
        enter(view, "BLPOP qa_cli_pending 0.2", window, cx);
    })
    .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        let count = view.read(cx).tabs[view.read(cx).index().unwrap()]
            .cli_output
            .len();
        view.update(cx, |app, cx| {
            let i = app.index().unwrap();
            let tab = &app.tabs[i];
            assert_eq!(tab.cli_state, CliState::Running);
            assert!(!tab.cli_input.read(cx).is_editable());
            tab.cli_input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "PING", window, cx)
            });
            let id = tab.id;
            let busy = app.busy;
            app.execute_cli(id, window, cx);
            assert_eq!(
                app.busy, busy,
                "duplicate submission must not enqueue a second job"
            );
        });
        for key in ["enter", "up", "ctrl-c", "ctrl-l"] {
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        }
        window.refresh();
        window.draw(cx).clear(cx);
        let app = view.read(cx);
        let tab = &app.tabs[app.index().unwrap()];
        assert!(
            text_input(&tab.cli_input, cx).is_empty(),
            "pending input, including IME edits, stays disabled"
        );
        assert_eq!(tab.cli_output.len(), count);
        let state = tab.cli_view.borrow();
        let frame = state.frame.as_ref().unwrap();
        let row = frame.rows[frame.cursor.row]
            .cells
            .iter()
            .map(|cell| cell.contents())
            .collect::<String>();
        assert_eq!(
            row.trim_end(),
            "",
            "waiting for a reply shows neither status text nor the next prompt"
        );
        assert_eq!(
            frame.cursor.col, 0,
            "the waiting cursor stays at the start of the blank line"
        );
        assert!(
            tab.cli_input.focus_handle(cx).is_focused(window),
            "the waiting cursor keeps focus so it can blink"
        );
    })
    .unwrap();
    qa_json::idle(view, window, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        check_ready(view, window, cx);
        let app = view.read(cx);
        assert_eq!(
            app.tabs[app.index().unwrap()]
                .cli_output
                .last()
                .unwrap()
                .text,
            "(nil)"
        );
        enter(view, "SELECT 2", window, cx);
    })
    .unwrap();
    qa_json::idle(view, window, cx).await;

    for unavailable in [false, true] {
        let s = service.clone();
        let c = config.clone();
        let old_id = cx
            .background_executor()
            .spawn(async move {
                let id = s.run_cli(&c, 2, command(&["CLIENT", "ID"])).unwrap().text();
                s.run(&c, 0, command(&["CLIENT", "KILL", "ID", &id]))
                    .unwrap();
                id
            })
            .await;
        cx.update_window(window.into(), |_, window, cx| {
            if unavailable {
                view.update(cx, |app, _| {
                    let i = app.index().unwrap();
                    app.tabs[i].conn.sock.push_str(".unavailable");
                });
            }
            enter(view, "PING", window, cx);
        })
        .unwrap();
        qa_json::idle(view, window, cx).await;
        cx.update_window(window.into(), |_, window, cx| {
            check_ready(view, window, cx);
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                let tab = &app.tabs[i];
                assert_eq!(tab.cli_db, 2);
                assert!(
                    tab.cli_output
                        .iter()
                        .any(|line| line.text.contains(&app.tr("cli.connection_lost")))
                );
                let last = tab.cli_output.last().unwrap();
                assert!(last.text.contains(&app.tr(if unavailable {
                    "cli.reconnect_failed"
                } else {
                    "cli.reconnected"
                })));
                assert_eq!(matches!(last.kind, cli::LineKind::Error), unavailable);
                app.tabs[i].conn.sock = config.sock.clone();
            });
            enter(view, "PING", window, cx);
        })
        .unwrap();
        qa_json::idle(view, window, cx).await;
        cx.update_window(window.into(), |_, window, cx| {
            check_ready(view, window, cx);
            let app = view.read(cx);
            assert_eq!(
                app.tabs[app.index().unwrap()]
                    .cli_output
                    .last()
                    .unwrap()
                    .text,
                "PONG"
            );
        })
        .unwrap();
        let s = service.clone();
        let c = config.clone();
        let new_id = cx
            .background_executor()
            .spawn(async move { s.run_cli(&c, 2, command(&["CLIENT", "ID"])).unwrap().text() })
            .await;
        assert_ne!(old_id, new_id);
    }
    cx.update_window(window.into(), |_, window, cx| {
        enter(view, "SELECT 0", window, cx)
    })
    .unwrap();
    qa_json::idle(view, window, cx).await;
    println!(
        "QA PASS: CLI pending prompt/input/duplicate guards; lost connection recovery and failure feedback; selected DB retained; retry restores input and PONG"
    );
}
