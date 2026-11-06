use super::*;
use crate::service::CommandLog;

fn record(index: usize, command: String) -> CommandLog {
    CommandLog {
        time: format!(
            "2026-10-30 {:02}:{:02}:{:02}.{:03}",
            12 + index / 3600,
            index / 60 % 60,
            index % 60,
            index % 1000
        ),
        server: if index % 2 == 0 {
            "server-a"
        } else {
            "server-b"
        }
        .into(),
        db: 0,
        command,
        elapsed_ms: 1.,
        error: String::new(),
    }
}

fn draw(window: WindowHandle<Root>, label: &str, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        let start = Instant::now();
        w.refresh();
        w.draw(cx).clear(cx);
        let elapsed = start.elapsed();
        let count = gpui_base::test_support::snapshots(w).iter().filter(|node| {
            matches!(node.path().last(), Some(ElementId::Name(name)) if name.starts_with("log-row-"))
        }).count();
        assert!((1..64).contains(&count), "only visible log rows should be built: {count}");
        println!("LOG PERFORMANCE QA: {label}: {:.2} ms, {count} observed rows", elapsed.as_secs_f64() * 1000.);
    }).unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    output_dir: &std::path::Path,
    cx: &mut AsyncApp,
) {
    let restore = keypages_qa::prepare(view, window, "keypages-log-rows-light", cx).await;
    let service = window
        .update(cx, |_, _, cx| view.read(cx).service.clone())
        .unwrap();
    {
        let mut history = service.history.lock();
        history.clear();
        for index in 0..5000 {
            history.push_back(record(index, format!("GET synthetic:{index}")));
        }
    }
    draw(window, "5000 short / first", cx);
    let cached = window
        .update(cx, |_, _, cx| view.read(cx).log_view.borrow().rows.clone())
        .unwrap();
    for _ in 0..3 {
        draw(window, "5000 short / cached", cx);
    }
    assert!(
        window
            .update(cx, |_, _, cx| Arc::ptr_eq(
                &cached,
                &view.read(cx).log_view.borrow().rows
            ))
            .unwrap()
    );
    {
        let _guard = service.history.lock();
        draw(window, "history locked / cached rows retained", cx);
        assert!(
            window
                .update(cx, |_, _, cx| Arc::ptr_eq(
                    &cached,
                    &view.read(cx).log_view.borrow().rows
                ))
                .unwrap()
        );
    }
    qa_json::click("nav-record", window, cx);
    draw(window, "enter / last row", cx);
    window
        .update(cx, |_, w, _| {
            assert!(gpui_base::test_support::find(w, &[], &"log-row-4999".into()).is_some());
            assert!(gpui_base::test_support::find(w, &[], &"log-row-0".into()).is_none());
        })
        .unwrap();
    let bottom = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .log_view
                .borrow()
                .list
                .logical_scroll_top()
                .item_ix
        })
        .unwrap();
    cx.update_window(window.into(), |_, w, cx| {
        let position = gpui_base::test_support::find(w, &[], &"operation-log-list".into())
            .unwrap()
            .bounds()
            .center();
        w.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(600.))),
                touch_phase: TouchPhase::Moved,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(window, "native wheel", cx);
    assert!(
        window
            .update(cx, |_, _, cx| view
                .read(cx)
                .log_view
                .borrow()
                .list
                .logical_scroll_top()
                .item_ix
                < bottom)
            .unwrap()
    );

    cx.update_window(window.into(), |_, w, cx| {
        view.update(cx, |app, cx| {
            set_input(&app.log_filter, "synthetic:49", w, cx);
            app.log_server = "server-a".into();
            cx.notify();
        })
    })
    .unwrap();
    draw(window, "server and keyword filter", cx);
    assert_eq!(
        window
            .update(cx, |_, _, cx| view.read(cx).log_view.borrow().rows.len())
            .unwrap(),
        55
    );
    qa_json::click("log-refresh", window, cx);
    draw(window, "refresh filtered last row", cx);
    window
        .update(cx, |_, w, _| {
            assert!(gpui_base::test_support::find(w, &[], &"log-row-54".into()).is_some());
        })
        .unwrap();

    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                set_input(&app.log_filter, "", w, cx);
                app.log_server.clear();
                cx.notify();
            })
        })
        .unwrap();
    service
        .history
        .lock()
        .push_back(record(5000, "GET rollover".into()));
    draw(window, "5000-entry rollover", cx);
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let state = app.log_view.borrow();
            assert_eq!(state.rows.len(), 5000);
            assert_eq!(state.rows[0].command, "GET synthetic:1");
            assert_eq!(state.rows.last().unwrap().command, "GET rollover");
        })
        .unwrap();

    {
        let mut history = service.history.lock();
        history.clear();
        for index in 0..1000 {
            history.push_back(record(
                index,
                (0..64)
                    .map(|line| format!("TYPE synthetic:{index}:{line}:{}", "x".repeat(48)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
    }
    draw(window, "1000 multiline / first", cx);
    draw(window, "1000 multiline / cached", cx);
    let full_command = format!("SET large {} __hidden_tail__", "中😀".repeat(50_000));
    {
        let mut history = service.history.lock();
        history.clear();
        history.push_back(record(0, full_command.clone()));
    }
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                set_input(&app.log_filter, "__hidden_tail__", w, cx);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, "large Unicode command / full-text filter", cx);
    window
        .update(cx, |_, w, _| {
            let row = gpui_base::test_support::find(w, &[], &"log-row-0".into()).unwrap();
            assert!(
                row.label().unwrap().chars().count() < 600,
                "accessibility text must also use the bounded preview"
            );
            assert!(!row.label().unwrap().contains("__hidden_tail__"));
            assert!(
                row.bounds().size.height <= px(160.),
                "long previews must stay compact"
            );
            w.render_to_image()
                .unwrap()
                .save(output_dir.join("log-preview.png"))
                .unwrap();
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.log_command_width = Some(60.);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, "narrow command column / bounded height", cx);
    window
        .update(cx, |_, w, _| {
            let row = gpui_base::test_support::find(w, &[], &"log-row-0".into()).unwrap();
            let button = gpui_base::test_support::find(w, &[], &"log-copy-0".into()).unwrap();
            assert!(row.bounds().size.height <= px(160.));
            assert!(
                button.bounds().size.width <= px(36.),
                "copy action must fit the narrow column"
            );
        })
        .unwrap();
    let clipboard = cx.update(|cx| cx.read_from_clipboard());
    qa_json::click("log-copy-0", window, cx);
    let copied = cx.update(|cx| qa::copy_result(clipboard, cx));
    assert_eq!(
        copied,
        Some(full_command),
        "copy the complete command and restore the user's clipboard"
    );
    qa_json::click("clear-logs", window, cx);
    qa_json::click("dialog-save", window, cx);
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(
        window
            .update(cx, |_, _, cx| view
                .read(cx)
                .log_view
                .borrow()
                .rows
                .is_empty())
            .unwrap()
    );
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                set_input(&app.log_filter, "", w, cx);
                app.log_command_width = None;
                keypages_qa::restore(restore, app, w, cx);
            })
        })
        .unwrap();
    println!(
        "LOG PERFORMANCE QA PASS: 5000 rows; cache reuse; lock contention; native scroll; filtered refresh; rollover; multiline/Unicode preview; full-text search and copy; clear confirmation"
    );
}
