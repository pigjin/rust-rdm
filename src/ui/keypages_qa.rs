//! Controlled original macOS page/dialog states; measurements come from paint.
use super::*;

pub(super) struct Restore {
    history: VecDeque<crate::service::CommandLog>,
    monitor: Option<StreamState>,
    pubsub: Option<StreamState>,
    _sender: mpsc::SyncSender<StreamEvent>,
}

pub(super) async fn prepare(view: &Entity<RdmApp>, window: WindowHandle<Root>, scene: &str, cx: &mut AsyncApp) -> Restore {
    window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
        app.nav = Nav::Browser;
        if app.active.is_none() { app.open_connection(app.connections[0].clone(), w, cx); }
    })).unwrap();
    qa_json::idle(view, window, cx).await;
    window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
        let id = app.active.unwrap();
        app.select_key(id, b"aastring".to_vec(), w, cx);
    })).unwrap();
    qa_json::idle(view, window, cx).await;
    let kind = scene.strip_prefix("keypages-").unwrap().rsplit_once('-').unwrap().0;
    let restore = window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
        let i = app.index().unwrap();
        let id = app.tabs[i].id;
        let (sender, receiver) = mpsc::sync_channel(16);
        let restore = Restore { history: app.service.history.lock().clone(),
            monitor: app.tabs[i].monitor.take(), pubsub: app.tabs[i].pubsub.take(), _sender: sender };
        app.pref.general.font_size = 14.;
        app.pref.general.font_family.clear();
        app.pref.general.font.clear();
        app.pref.cli.font_family = vec!["Menlo".into()];
        app.pref.general.theme = if scene.ends_with("-dark") { "dark" } else { "light" }.into();
        app.pref.behavior.aside_width = 300.;
        app.tabs[i].content_nav_offset.set(0.);
        app.tabs[i].check_mode = false;
        app.tabs[i].selected.clear();
        app.apply_theme(w, cx);
        if kind.starts_with("slow-") {
            app.tabs[i].pane = Pane::Slow;
            app.tabs[i].slow_ascending = Some(false);
            app.tabs[i].slow.clear();
            set_input(&app.tabs[i].slow_limit, "20", w, cx);
            set_input(&app.tabs[i].slow_keyword, "", w, cx);
            set_input(&app.tabs[i].slow_client, "", w, cx);
            if kind.ends_with("rows") {
                app.tabs[i].slow = vec![
                    SlowLog { id: 1, time: 1700000000, duration: 12000, args: "GET astring".into(), name: "worker".into(), address: "127.0.0.1:1234".into() },
                    SlowLog { id: 2, time: 1700000001, duration: 1500000, args: "SET astring value".into(), name: "".into(), address: "127.0.0.1:5678".into() },
                ];
            }
        } else if kind.starts_with("monitor-") || kind.starts_with("pubsub-") {
            let pubsub = kind.starts_with("pubsub-");
            app.tabs[i].pane = if pubsub { Pane::Pubsub } else { Pane::Monitor };
            let mut lines = VecDeque::new();
            if kind.ends_with("rows") {
                for (n, text) in if pubsub { vec!["Hello Redis", "中文消息 😀"] } else { vec!["1700000000.000000 [0 127.0.0.1:1234] \"GET\" \"astring\"", "1700000000.100000 [0 127.0.0.1:1234] \"SET\" \"astring\" \"value\""] }.into_iter().enumerate() {
                    lines.push_back(StreamLine { time: if n == 0 { "2023-11-15 06:13:20.123" } else { "2023-11-15 06:13:21.456" }.into(), channel: pubsub.then(|| "news:updates".into()), text: text.into() });
                }
            }
            let state = Some(StreamState { cancel: Cancellation::default(), receiver, lines,
                running: kind.ends_with("rows"), auto_last: true,
                list: ListState::new(0, ListAlignment::Top, px(500.)) });
            if pubsub { app.tabs[i].pubsub = state; } else { app.tabs[i].monitor = state; }
            set_input(&app.tabs[i].stream_filter, "", w, cx);
            set_input(&app.tabs[i].pub_channel, "", w, cx);
            set_input(&app.tabs[i].pub_message, "", w, cx);
            app.tabs[i].pub_received = None;
        } else if kind.starts_with("log-") {
            app.nav = Nav::Log;
            app.log_ascending = Some(true);
            app.log_command_width = None;
            app.log_server.clear();
            set_input(&app.log_filter, "", w, cx);
            app.service.history.lock().clear();
            if kind.ends_with("rows") {
                for (time, server, command, elapsed_ms) in [
                    ("2023-11-15 06:13:20", "localhost", "GET astring", 12.),
                    ("2023-11-15 06:13:21", "other", "SET astring value", 1500.),
                ] { app.service.history.lock().push_back(crate::service::CommandLog { time: time.into(), server: server.into(), db: 0, command: command.into(), elapsed_ms, error: String::new() }); }
            }
        } else {
            let dialog = match kind {
                "ttl" | "ttl-days" | "ttl-batch" => {
                    if kind == "ttl-batch" { app.tabs[i].check_mode = true;app.tabs[i].selected = [b"aastring".to_vec(), b"aahash".to_vec()].into_iter().collect(); }
                    dialogs::DialogKind::Ttl
                }
                "rename" => dialogs::DialogKind::Rename,
                "group" => dialogs::DialogKind::Group(None),
                "group-edit" => dialogs::DialogKind::Group(Some("Work".into())),
                "flush" => dialogs::DialogKind::Flush,
                "warning" => dialogs::DialogKind::CleanLog,
                _ if kind.starts_with("delete-") => dialogs::DialogKind::DeleteKeys,
                _ => panic!("unknown key page {kind}"),
            };
            app.open_dialog(dialog, w, cx);
            if kind == "ttl-days" { app.set_dialog_field("ttl", "2", w, cx);app.set_dialog_field("unit", "86400", w, cx); }
            if kind == "delete-selected" { app.set_dialog_keys(vec![b"aastring".to_vec(), b"aahash".to_vec()]); }
            if matches!(kind, "delete-pattern" | "delete-direct" | "delete-affected") { app.set_dialog_field("pattern", "test:*", w, cx); }
            if kind == "delete-direct" { app.set_dialog_field("direct", "true", w, cx); }
            if kind == "delete-affected" {
                app.set_dialog_keys(vec![b"test:0".to_vec(), b"test:1".to_vec()]);
                app.set_dialog_field("checked", "false", w, cx);
            }
        }
        let _ = id;
        cx.notify();
        restore
    })).unwrap();
    cx.background_executor().timer(Duration::from_millis(120)).await;
    cx.update_window(window.into(), |_, w, cx| { w.refresh(); let _ = w.draw(cx); }).unwrap();
    restore
}

/// Native MONITOR frames can include long Unix socket paths and command values.
/// Verify painted row heights, rather than just the retained stream contents.
pub(super) async fn verify_monitor_wrap(view: &Entity<RdmApp>, window: WindowHandle<Root>, scene: &str, cx: &mut AsyncApp) -> Restore {
    let theme = if scene.ends_with("-dark") { "dark" } else { "light" };
    let restore = prepare(view, window, &format!("keypages-monitor-rows-{theme}"), cx).await;
    window.update(cx, |_, _, cx| view.update(cx, |app, cx| {
        let i = app.index().unwrap();
        let stream = app.tabs[i].monitor.as_mut().unwrap();
        stream.lines = [
            format!("1792631653.944018 [0 unix:/private/tmp/rdm-acceptance-native-9pg85fjq/data/redis.sock] \"ECHO\" \"{}\"", "long command value 中文 😀 ".repeat(7)),
            "1792631654.000000 [0 127.0.0.1:1234] \"PING\"".into(),
            "1792631655.000000 [0 127.0.0.1:1234] \"GET\" \"aastring\"".into(),
        ].into_iter().map(|text| StreamLine { time: String::new(), channel: None, text }).collect();
        stream.auto_last = false;
        stream.list.reset(0);
        cx.notify();
    })).unwrap();
    cx.update_window(window.into(), |_, w, cx| { w.refresh(); let _ = w.draw(cx); }).unwrap();
    window.update(cx, |_, w, _| {
        let bounds = (0..3).map(|index| gpui_base::test_support::find(w, &[], &format!("monitor-row-{index}").into()).unwrap().bounds()).collect::<Vec<_>>();
        println!("QA MONITOR WRAP {scene}: {bounds:?}");
        assert!(bounds[0].size.height >= px(42.), "long MONITOR content must expand its row");
        for pair in bounds.windows(2) {
            assert!(pair[1].origin.y >= pair[0].bottom() + px(4.9), "wrapped MONITOR rows must not overlap");
        }
        assert_eq!(bounds[1].size.height, px(21.), "short MONITOR rows retain source line height");
    }).unwrap();
    restore
}

pub(super) fn capture(window: &Window, scene: &str) {
    let frames = gpui_base::test_support::snapshots(window).into_iter().filter(|row| row.visible()).map(|row| {
        let bounds = row.bounds();
        serde_json::json!({"path":format!("{:?}", row.path()), "id":row.path().last().map(|id|format!("{id:?}")),
            "x":bounds.origin.x.as_f32(), "y":bounds.origin.y.as_f32(), "w":bounds.size.width.as_f32(), "h":bounds.size.height.as_f32(),
            "role":format!("{:?}",row.role()), "label":row.label(), "value":row.value(), "disabled":row.disabled(), "click":row.supports_click()})
    }).collect::<Vec<_>>();
    let nodes = ["slow-refresh", "dialog-card", "dialog-body", "dialog-title", "dialog-footer", "dialog-save", "dialog-cancel", "key-delete-warning", "key-delete-warning-title", "key-delete-warning-content", "ttl-unit", "ttl-quick--1-1", "ttl-quick-10-1", "ttl-quick-1-60", "ttl-quick-1-3600", "ttl-quick-1-86400", "slow-list", "slow-sort-time", "slow-limit-step-1", "slow-limit-step--1", "stream-start", "stream-last", "stream-copy", "stream-export", "stream-clear", "publish", "log-refresh", "clear-logs", "log-server-filter", "log-sort-time", "operation-log-list", "keypages-root", "keypages-form", "keypages-table", "keypages-table-header", "keypages-count", "keypages-filter", "keypages-publish-row", "keypages-total", "keypages-title", "affected-keys"].into_iter().filter_map(|id| {
        let row = gpui_base::test_support::find(window, &[], &id.into())?;
        let b = row.bounds();
        Some(serde_json::json!({"id":id,"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32(),"label":row.label()}))
    }).collect::<Vec<_>>();
    println!("KEYPAGES FRAME: {}", serde_json::json!({"scene":scene,"frames":frames,"nodes":nodes}));
}

pub(super) fn restore(state: Restore, app: &mut RdmApp, w: &mut Window, cx: &mut Context<RdmApp>) {
    app.close_dialog(w, cx);
    let i = app.index().unwrap();
    app.tabs[i].monitor = state.monitor;
    app.tabs[i].pubsub = state.pubsub;
    app.tabs[i].check_mode = false;
    app.tabs[i].selected.clear();
    *app.service.history.lock() = state.history;
    app.pref.cli.font_family.clear();
    cx.notify();
}
