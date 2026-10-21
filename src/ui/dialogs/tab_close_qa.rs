//! Real GPUI events with source observations and disposable Redis connections.
use super::*;
use super::new_key_qa::{click, draw};

const DRAFT: &str = "draft retained";

fn source() -> Json {
    serde_json::from_str(include_str!("../../../tests/fixtures/tab-close-source.json")).unwrap()
}

fn key(value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse(value).unwrap(),
            is_held: false,
            prefer_character_input: false,
        }), cx);
    }).unwrap();
}

async fn settled(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..100 {
        draw(window, cx).await;
        if window.update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.busy == 0 && app.entry_conversions == 0
        }).unwrap() { return; }
    }
    panic!("tab close QA background work did not finish");
}

fn warning(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) -> u64 {
    window.update(cx, |_, _, cx| {
        let app = view.read(cx);
        assert_eq!(app.tab_close_warnings.len(), 1);
        app.tab_close_warnings[0].id
    }).unwrap()
}

fn assert_draft(view: &Entity<RdmApp>, editor: EntityId, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window.update(cx, |_, _, cx| {
        let app = view.read(cx);
        assert_eq!(app.tabs.len(), 1);
        assert!(app.editing_entry(), "confirmation cancellation must retain the inline editor");
        let dialog = app.dialog.as_ref().unwrap();
        assert_eq!(dialog.editors["value"].entity_id(), editor);
        assert_eq!(dialog.value("value", cx), DRAFT);
    }).unwrap();
}

async fn prepare_draft(
    view: &Entity<RdmApp>, window: WindowHandle<Root>, dark: bool, cx: &mut AsyncApp,
) -> (Connection, EntityId, u64) {
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        app.tab_close_warnings.clear();
        app.close_dialog(window, cx);
        let connection = flatten(&app.connections).into_iter().next().unwrap();
        if app.active.is_none() { app.open_connection(connection, window, cx); }
    })).unwrap();
    settled(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        app.qa_prepare_entry_panel("edit-entry-hash", window, cx);
        app.pref.general.theme = if dark { "dark" } else { "light" }.into();
        app.apply_theme(window, cx);
    })).unwrap();
    settled(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        app.set_dialog_field("value", DRAFT, window, cx);
        app.dialog.as_ref().unwrap().editors["value"].focus_handle(cx).focus(window, cx);
        let index = app.index().unwrap();
        app.tabs[index].status_auto = false;
        app.tabs[index].auto_refresh = false;
        app.messages.clear();
        app.notifications.clear();
        let editor = app.dialog.as_ref().unwrap().editors["value"].entity_id();
        (app.tabs[index].conn.clone(), editor, app.tabs[index].id)
    })).unwrap()
}

async fn client_id(service: Arc<Service>, connection: Connection, cx: &mut AsyncApp) -> i64 {
    cx.background_executor().spawn(async move {
        service.run(&connection, 0, command(&["CLIENT", "ID"]))?.integer()
    }).await.unwrap()
}

fn assert_layout(view: &Entity<RdmApp>, dark: bool, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let oracle = source();
    let layout = oracle["layouts"].as_array().unwrap().iter()
        .find(|row| row["theme"] == if dark { "dark" } else { "light" }).unwrap();
    window.update(cx, |_, window, cx| {
        let app = view.read(cx);
        let id = app.tab_close_warnings.last().unwrap().id;
        for part in ["card", "title", "content", "actions", "cancel", "confirm"] {
            let name = format!("tab-close-{part}-{id}");
            let actual = gpui_base::test_support::find(window, &[], &name.clone().into())
                .unwrap_or_else(|| panic!("missing {name}")).bounds();
            let expected = &layout["parts"][part];
            for (dimension, value) in [("x", actual.origin.x), ("y", actual.origin.y),
                ("width", actual.size.width), ("height", actual.size.height)] {
                assert!((value.as_f32() - expected[dimension].as_f64().unwrap() as f32).abs() < 0.6,
                    "tab close {name} {dimension}: {value:?} != {}", expected[dimension]);
            }
        }
    }).unwrap();
}

fn confirm_and_assert_pending(view: &Entity<RdmApp>, id: u64, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let element = format!("tab-close-confirm-{id}");
        let position = gpui_base::test_support::find(window, &[], &element.into()).unwrap().bounds().center();
        for event in [PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() })] {
            window.dispatch_event(event, cx);
        }
        let app = view.read(cx);
        assert!(app.tab_close_warnings.is_empty(), "the warning closes before the asynchronous callback");
        assert_eq!(app.busy, 0, "tab close must not introduce a global loading overlay");
        assert!(!app.tabs.is_empty(), "tab removal waits for the asynchronous close result");
    }).unwrap();
}

async fn wait_count(view: &Entity<RdmApp>, count: usize, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..100 {
        draw(window, cx).await;
        if window.update(cx, |_, _, cx| view.read(cx).tabs.len() == count).unwrap() { return; }
    }
    panic!("tab close count did not reach {count}");
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>, window: WindowHandle<Root>, scene: &str, cx: &mut AsyncApp,
) {
    let dark = scene.ends_with("-dark");
    let (connection, editor, tab) = prepare_draft(view, window, dark, cx).await;
    let service = window.update(cx, |_, _, cx| view.read(cx).service.clone()).unwrap();
    let original_client = client_id(service.clone(), connection.clone(), cx).await;
    click(format!("close-{tab}").into(), window, cx);
    draw(window, cx).await;
    let id = warning(view, window, cx);
    assert_draft(view, editor, window, cx);
    assert_layout(view, dark, window, cx);
    if scene != "tab-close-behavior" {
        assert_eq!(client_id(service, connection, cx).await, original_client,
            "opening the warning must not close its Redis connection");
        println!("TAB CLOSE LAYOUT QA PASS: {scene} six source rectangles; editor/draft and Redis connection retained");
        return;
    }

    click(format!("tab-close-cancel-{id}").into(), window, cx);
    draw(window, cx).await;
    assert_draft(view, editor, window, cx);
    assert!(window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.is_empty()).unwrap());
    assert_eq!(client_id(service.clone(), connection.clone(), cx).await, original_client);

    click(format!("close-{tab}").into(), window, cx);
    draw(window, cx).await;
    key("escape", window, cx);
    draw(window, cx).await;
    assert_draft(view, editor, window, cx);
    assert!(window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.is_empty()).unwrap());

    window.update(cx, |_, window, cx| {
        let app = view.read(cx);
        app.dialog.as_ref().unwrap().editors["value"].focus_handle(cx).focus(window, cx);
    }).unwrap();
    key("cmd-w", window, cx);
    draw(window, cx).await;
    warning(view, window, cx);
    assert_draft(view, editor, window, cx);
    assert!(window.update(cx, |_, window, cx| view.read(cx).entry_editor_focused(window, cx)).unwrap(),
        "the global close shortcut leaves editor focus unchanged");
    key("escape", window, cx);
    draw(window, cx).await;
    assert_draft(view, editor, window, cx);
    assert!(window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.is_empty()).unwrap());

    click(format!("close-{tab}").into(), window, cx);
    draw(window, cx).await;
    let id = warning(view, window, cx);
    confirm_and_assert_pending(view, id, window, cx);
    wait_count(view, 0, window, cx).await;
    window.update(cx, |_, _, cx| {
        let app = view.read(cx);
        assert!(app.active.is_none());
        assert!(app.nav == Nav::Server);
        assert!(!app.entry_panel_visible());
    }).unwrap();
    assert_ne!(client_id(service.clone(), connection.clone(), cx).await, original_client,
        "the test's explicit reconnection must obtain a new CLIENT ID after confirmation");
    service.close(&connection.name);

    let oracle = source();
    for case in oracle["cases"].as_array().unwrap() {
        let label = case["name"].as_str().unwrap();
        let before = oracle["states"].as_array().unwrap().iter()
            .find(|row| row["theme"] == "light" && row["name"] == format!("{label}-before")).unwrap();
        let after = oracle["states"].as_array().unwrap().iter()
            .find(|row| row["theme"] == "light" && row["name"] == format!("{label}-after")).unwrap();
        let names = before["names"].as_array().unwrap().iter().map(|name| name.as_str().unwrap().to_owned()).collect::<Vec<_>>();
        let configs = names.iter().map(|name| { let mut c = connection.clone(); c.name = name.clone(); c }).collect::<Vec<_>>();
        window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
            for tab in &app.tabs { app.service.close(&tab.conn.name); }
            app.tabs.clear(); app.active = None;
            for c in &configs { app.open_connection(c.clone(), window, cx); }
            app.active = Some(app.tabs[case["active"].as_u64().unwrap() as usize].id);
            for tab in &mut app.tabs { tab.status_auto = false; tab.auto_refresh = false; }
        })).unwrap();
        settled(view, window, cx).await;
        let mut client_ids = Vec::new();
        for c in &configs { client_ids.push(client_id(service.clone(), c.clone(), cx).await); }
        let close_index = case["close"].as_u64().unwrap() as usize;
        let closing = window.update(cx, |_, _, cx| view.read(cx).tabs[close_index].id).unwrap();
        click(format!("close-{closing}").into(), window, cx);
        draw(window, cx).await;
        let id = warning(view, window, cx);
        window.update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.tab_close_warnings[0].name, names[close_index]);
            assert_eq!(app.index().unwrap(), case["active"].as_u64().unwrap() as usize,
                "close button must not activate an inactive tab");
        }).unwrap();
        confirm_and_assert_pending(view, id, window, cx);
        wait_count(view, 2, window, cx).await;
        window.update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.tabs.iter().map(|tab| tab.conn.name.clone()).collect::<Vec<_>>(),
                after["names"].as_array().unwrap().iter().map(|name| name.as_str().unwrap().to_owned()).collect::<Vec<_>>(), "{label}");
            assert_eq!(app.index().unwrap(), after["activeIndex"].as_u64().unwrap() as usize, "{label}");
            assert_eq!(app.tabs[app.index().unwrap()].conn.name, after["currentName"].as_str().unwrap(), "{label}");
        }).unwrap();
        for (index, c) in configs.iter().enumerate() {
            let observed = client_id(service.clone(), c.clone(), cx).await;
            if index == close_index { assert_ne!(observed, client_ids[index], "{label}: closed pool"); }
            else { assert_eq!(observed, client_ids[index], "{label}: unrelated pool"); }
            service.close(&c.name);
        }
    }
    window.update(cx, |_, _, cx| view.update(cx, |app, _| { app.tabs.clear(); app.active = None; })).unwrap();
    let (_, _, tab) = prepare_draft(view, window, false, cx).await;
    let check_connection = connection.clone();
    cx.background_executor().spawn(async move {
        assert_eq!(service.run(&check_connection, 0, command(&["HGET", "aahash", "name"]))?.bytes(), b"Tiny RDM");
        assert_eq!(service.run(&check_connection, 0, command(&["DBSIZE"]))?.integer()?, 19);
        Ok::<_, anyhow::Error>(())
    }).await.unwrap();
    click(format!("close-{tab}").into(), window, cx);
    draw(window, cx).await;
    assert_layout(view, false, window, cx);
    println!("TAB CLOSE BEHAVIOR QA PASS: actual close/cancel/Escape/Cmd+W events preserve the draft; warning dismisses before async removal; last tab returns to server; five original activation cases and named Redis pool isolation; no key writes");
}
