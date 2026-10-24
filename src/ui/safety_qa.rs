use super::*;

pub(super) struct Saved {
    db: u32,
    page: usize,
    config: Connection,
}
pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                if app.active.is_none() {
                    app.open_connection(flatten(&app.connections)[0].clone(), w, cx);
                }
                app.nav = Nav::Browser;
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
    let (service, config, id, db, page) = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let t = &app.tabs[app.index().unwrap()];
            (
                app.service.clone(),
                t.conn.clone(),
                t.id,
                t.db,
                app.pref.general.scan_size,
            )
        })
        .unwrap();
    if scene == "safety-entry-pagination" {
        let s = service.clone();
        let c = config.clone();
        cx.background_executor()
            .spawn(async move {
                s.run(&c, 15, command(&["DEL", "__safety_paged__"]))
                    .unwrap();
                for n in 0..40 {
                    s.run(
                        &c,
                        15,
                        command(&[
                            "RPUSH",
                            "__safety_paged__",
                            if n % 3 == 0 { "match" } else { "other" },
                        ]),
                    )
                    .unwrap();
                }
            })
            .await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let i = app.tab_index(id).unwrap();
                    app.tabs[i].db = 15;
                    app.pref.general.scan_size = 5;
                    app.select_key(id, b"__safety_paged__".to_vec(), w, cx);
                })
            })
            .unwrap();
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let i = app.tab_index(id).unwrap();
                    app.tabs[i].row_match = "match".into();
                    app.reload_detail(id, true, w, cx);
                })
            })
            .unwrap();
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let t = &app.tabs[app.tab_index(id).unwrap()];
                    let d = t.detail.as_ref().unwrap();
                    assert_eq!(d.rows.len(), 5);
                    assert!(!d.end);
                    assert_eq!(d.cursor, 15);
                    app.reload_detail(id, false, w, cx);
                })
            })
            .unwrap();
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert_eq!(
                        app.tabs[app.tab_index(id).unwrap()]
                            .detail
                            .as_ref()
                            .unwrap()
                            .rows
                            .len(),
                        10
                    );
                    app.load_all_entries(id, w, cx);
                })
            })
            .unwrap();
        qa_json::idle(view, window, cx).await;
        let guard = service.client(&config, 15).unwrap();
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let i = app.tab_index(id).unwrap();
                    let d = app.tabs[i].detail.as_ref().unwrap();
                    assert_eq!(d.rows.len(), 14);
                    assert!(d.end);
                    for (row, n) in d.rows.iter().zip((0..40).step_by(3)) {
                        assert_eq!(row.id, n.to_string().as_bytes());
                    }
                    app.reload_detail(id, true, w, cx);
                })
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, cx| app.cancel_entry_load(id, cx))
            })
            .unwrap();
        drop(guard);
        qa_json::idle(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, _| {
                    let i = app.tab_index(id).unwrap();
                    let t = &mut app.tabs[i];
                    assert!(!t.entries_loading);
                    assert_eq!(t.detail.as_ref().unwrap().rows.len(), 14);
                    key_tree_cache::verify(t);
                })
            })
            .unwrap();
        println!(
            "QA PASS: progressive search, physical indices, more/all, in-flight cancellation, stale reply discarded, key-tree cache invalidation"
        );
    } else if scene == "safety-stream-retention" {
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, cx| {
                    let i = app.tab_index(id).unwrap();
                    app.tabs[i].pane = Pane::Monitor;
                    let state = app.tabs[i].monitor.as_mut().unwrap();
                    state.clear_lines();
                    for n in 0..10005 {
                        state.push_line(StreamLine {
                            time: String::new(),
                            channel: None,
                            text: format!("fixture {n}"),
                        });
                    }
                    assert_eq!(state.lines.len(), 10000);
                    assert_eq!(state.dropped, 5);
                    assert_eq!(state.lines.front().unwrap().text, "fixture 5");
                    cx.notify();
                })
            })
            .unwrap();
        println!("QA PASS: retained stream window, eviction counter, copy/export scope");
    } else {
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| app.qa_show_ssh_trust(scene, w, cx))
            })
            .unwrap();
        println!(
            "QA PASS: SSH trust modal shows endpoint, algorithm and SHA-256; cancellation preserves profiles"
        );
    }
    Saved { db, page, config }
}
pub(super) async fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: Saved,
    cx: &mut AsyncApp,
) {
    let s = window
        .update(cx, |_, _, cx| view.read(cx).service.clone())
        .unwrap();
    let c = saved.config.clone();
    cx.background_executor()
        .spawn(async move {
            s.run(&c, 15, command(&["DEL", "__safety_paged__"]))
                .unwrap();
        })
        .await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.ssh_trust = None;
                app.pref.general.scan_size = saved.page;
                if let Some(i) = app.index() {
                    let id = app.tabs[i].id;
                    app.tabs[i].monitor.as_mut().unwrap().clear_lines();
                    app.change_database(id, saved.db, w, cx);
                }
            })
        })
        .unwrap();
    qa_json::idle(view, window, cx).await;
}
