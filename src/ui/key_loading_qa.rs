use super::*;
use parking_lot::{ArcMutexGuard, RawMutex};

pub(super) struct Pending {
    client: ArcMutexGuard<RawMutex, Client>,
    id: u64,
    theme: String,
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Pending {
    qa_json::idle(view, window, cx).await;
    if scene == "key-loading-light" {
        verify_load_size(view, window, cx).await;
    }
    let (service, connection, id, theme) = window.update(cx, |_, _, cx| {
        let app = view.read(cx);
        let tab = &app.tabs[app.index().unwrap()];
        (app.service.clone(), tab.conn.clone(), tab.id, app.pref.general.theme.clone())
    }).unwrap();
    let setup_service = service.clone();
    let setup_connection = connection.clone();
    cx.background_executor().spawn(async move {
        setup_service.connect(&setup_connection, 1).unwrap();
        setup_service.connect(&setup_connection, 2).unwrap();
    }).await;
    // Hold the real per-DB clients so completion order is deterministic.
    let first = service.client(&connection, 1).unwrap();
    let second = service.client(&connection, 2).unwrap();
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        app.pref.general.theme = if scene.ends_with("dark") { "dark" } else { "light" }.into();
        app.apply_theme(window, cx);
        app.change_database(id, 1, window, cx);
        assert!(app.tabs[app.tab_index(id).unwrap()].keys_loading);
        app.change_database(id, 2, window, cx);
        assert_eq!(app.busy, 2);
        app.scan_keys(id, false, false, window, cx);
        assert_eq!(app.busy, 2, "loading more must not duplicate the pending scan");
    })).unwrap();
    drop(first);
    for _ in 0..100 {
        if window.update(cx, |_, _, cx| view.read(cx).busy == 1).unwrap() {
            break;
        }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    cx.update_window(window.into(), |_, window, cx| {
        let app = view.read(cx);
        let tab = &app.tabs[app.tab_index(id).unwrap()];
        assert_eq!(app.busy, 1, "old DB request must have finished");
        assert_eq!(tab.db, 2);
        assert!(tab.keys_loading && tab.keys.is_empty(), "old completion must preserve the current loading state");
        window.refresh();
        let _ = window.draw(cx);
        let loading = gpui_base::test_support::find(window, &[], &"key-browser-loading".into()).unwrap();
        let tree = gpui_base::test_support::find(window, &[], &"key-browser-tree".into()).unwrap();
        assert_eq!(loading.bounds(), tree.bounds());
        assert!(gpui_base::test_support::find(window, &[], &"app-loading-indicator".into()).is_none(),
            "key loading must show only the list spinner without the global loading text");
        assert!(gpui_base::test_support::find(window, &[], &"database-select".into()).is_some());
    }).unwrap();
    println!("QA PASS: {scene} pending real scan, visible list loading, stale completion ignored, duplicate load blocked");
    Pending { client: second, id, theme }
}

async fn verify_load_size(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let (id, scan_size, load_size, total) = window.update(cx, |_, _, cx| {
        let app = view.read(cx);
        let tab = &app.tabs[app.index().unwrap()];
        (tab.id, app.pref.general.scan_size, tab.conn.load_size, tab.total as usize)
    }).unwrap();
    assert_eq!(total, 19, "use the disposable key-browser fixture");
    for (count, limit) in [(3000, 1), (3, 7), (3, 0)] {
        let size = if limit == 0 { count } else { limit };
        for (reset, all) in [(true, false), (false, false), (false, true), (true, false)] {
            let (previous, history_start) = window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
                let i = app.tab_index(id).unwrap();
                let previous = if reset { 0 } else { app.tabs[i].keys.len() };
                let history_start = app.service.history.lock().len();
                app.tabs[i].conn.load_size = limit;
                app.pref.general.scan_size = count;
                app.scan_keys(id, reset, all, window, cx);
                (previous, history_start)
            })).unwrap();
            qa_json::idle(view, window, cx).await;
            window.update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.tab_index(id).unwrap()];
                let expected = if all { total } else { (previous + size).min(total) };
                assert!(tab.keys.len() >= expected && tab.keys.len() <= total,
                    "SCAN={count}, load={limit}, reset={reset}, all={all}");
                assert_eq!(tab.total as usize, total);
                let maximum = if all || limit == 0 { count } else { count.min(limit) };
                let history = app.service.history.lock();
                for entry in history.iter().skip(history_start).filter(|entry| entry.command.starts_with("SCAN ")) {
                    let requested = entry.command.split_whitespace().skip_while(|word| *word != "COUNT")
                        .nth(1).unwrap().parse::<usize>().unwrap();
                    assert!((1..=maximum).contains(&requested), "connection load size must limit SCAN COUNT");
                }
                if all {
                    assert!(tab.scan.end);
                    assert!(tab.keys.iter().any(|key| key.key == [0xff, 0, 65]));
                }
            }).unwrap();
        }
    }
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let i = app.tab_index(id).unwrap();
        app.tabs[i].conn.load_size = load_size;
        app.pref.general.scan_size = scan_size;
        app.scan_keys(id, true, false, window, cx);
    })).unwrap();
    qa_json::idle(view, window, cx).await;
    println!("QA PASS: connection load size controls SCAN COUNT; complete batches display immediately; more/all/reset and zero load size stay valid");
}

pub(super) async fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    pending: Pending,
    cx: &mut AsyncApp,
) {
    let Pending { client, id, theme } = pending;
    drop(client);
    qa_json::idle(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let tab = &app.tabs[app.tab_index(id).unwrap()];
        assert!(!tab.keys_loading && tab.keys.is_empty() && tab.scan.end);
        app.messages.clear();
        app.change_database(id, u32::MAX, window, cx);
        assert!(app.tabs[app.tab_index(id).unwrap()].keys_loading);
    })).unwrap();
    qa_json::idle(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        assert!(!app.tabs[app.tab_index(id).unwrap()].keys_loading, "failed scan must clear loading");
        assert!(app.messages.iter().any(|notice| notice.kind == notices::MessageKind::Error));
        app.messages.clear();
        app.change_database(id, 0, window, cx);
        app.pref.general.theme = theme;
        app.apply_theme(window, cx);
    })).unwrap();
    qa_json::idle(view, window, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        let app = view.read(cx);
        let tab = &app.tabs[app.tab_index(id).unwrap()];
        assert!(!tab.keys_loading && !tab.keys.is_empty());
        window.refresh();
        let _ = window.draw(cx);
        assert!(gpui_base::test_support::find(window, &[], &"key-browser-loading".into()).is_none());
    }).unwrap();
    println!("QA PASS: key loading clears on empty success and Redis error; DB0 retry restores keys and hides loading");
}
