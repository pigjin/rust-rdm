use super::*;
use super::new_key_qa::{click, draw};

const DRAFT: &str = "retained across tabs";

fn fixture() -> Json {
    serde_json::from_str(include_str!("../../../tests/fixtures/connection-tabs-source.json")).unwrap()
}

async fn settled(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..120 {
        draw(window, cx).await;
        if window.update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.busy == 0 && app.entry_conversions == 0
        }).unwrap() { return; }
    }
    panic!("connection tab background work did not settle");
}

fn key(value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse(value).unwrap(), is_held: false, prefer_character_input: false,
        }), cx);
    }).unwrap();
}

async fn reset(
    view: &Entity<RdmApp>, window: WindowHandle<Root>, scene: &str, cx: &mut AsyncApp,
) -> Vec<u64> {
    settled(view, window, cx).await;
    let two = scene.contains("two-") || scene.contains("narrow") || scene.contains("mark-");
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let connection = flatten(&app.connections).into_iter().next().unwrap();
        for tab in &app.tabs { app.service.close(&tab.conn.name); }
        app.dialog = None; app.dialog_stack.clear(); app.cached_entry_panels.clear();
        app.tab_close_warnings.clear(); app.tabs.clear(); app.active = None;
        app.connection_tabs_scroll.set_offset(point(px(0.), px(0.)));
        app.pref.behavior.aside_width = 300.;
        app.pref.general.font.clear(); app.pref.general.font_family.clear();
        let size = if scene.ends_with("18") { 18. } else { 14. };
        app.pref.general.font_size = size; app.general_css_size = size;
        app.pref.general.theme = if scene.contains("dark") || scene.contains("two-") || scene.contains("narrow") { "dark" } else { "light" }.into();
        app.apply_theme(window, cx);
        window.resize(size_fn(if scene.contains("narrow") { 800. } else { 1024. }));
        let mut first = connection.clone(); first.mark_color.clear();
        if scene.contains("mark-") { first.mark_color = "#123456".into(); }
        app.open_connection(first, window, cx);
        if two { let mut second = connection; second.name = "replica".into(); second.mark_color.clear(); app.open_connection(second, window, cx); }
        let index = usize::from(scene.contains("two-right"));
        app.activate_connection_tab(Some(app.tabs[index].id), cx);
        for tab in &mut app.tabs { tab.auto_refresh = false; tab.status_auto = false; }
        app.messages.clear(); app.notifications.clear();
        app.focus.focus(window, cx);
    })).unwrap();
    settled(view, window, cx).await;
    window.update(cx, |_, _, cx| view.read(cx).tabs.iter().map(|tab| tab.id).collect()).unwrap()
}

fn size_fn(width: f32) -> gpui::Size<Pixels> { size(px(width), px(768.)) }

fn source_state(scene: &str) -> Json {
    let source = fixture();
    let name = match scene {
        "connection-tabs-light" => "entry-closed-light14",
        "connection-tabs-dark" => "entry-closed-dark14",
        "connection-tabs-general18" => "entry-closed-light18",
        "connection-tabs-dark18" => "entry-closed-dark18",
        "connection-tabs-two-left" => "two-tabs-left-active-dark14",
        "connection-tabs-two-right" => "two-tabs-right-active-dark14",
        "connection-tabs-narrow" => "two-tabs-left-active-narrow-dark14",
        "connection-tabs-mark-light" => "marked-tab-light",
        "connection-tabs-mark-dark" => "marked-tab-dark",
        _ => panic!("unknown tab layout {scene}"),
    };
    source[if scene.contains("mark-") { "programmaticBehaviorStates" } else { "layoutStates" }]
        .as_array().unwrap().iter().find(|row| row["name"] == name).unwrap().clone()
}

fn assert_layout(view: &Entity<RdmApp>, scene: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let expected = source_state(scene);
    window.update(cx, |_, window, cx| {
        let app = view.read(cx);
        let mut measurements = Vec::new();
        let mut differences = Vec::new();
        let mut check = |id: ElementId, source: &Json| {
            let actual = gpui_base::test_support::find(window, &[], &id).unwrap_or_else(|| panic!("missing {id:?}")).bounds();
            for (key, value) in [("x", actual.origin.x), ("y", actual.origin.y), ("width", actual.size.width), ("height", actual.size.height)] {
                if (value.as_f32() - source[key].as_f64().unwrap() as f32).abs() >= 0.6 {
                    differences.push(format!("{id:?} {key}: {value:?} != {}", source[key]));
                }
            }
            measurements.push(serde_json::json!({"id":format!("{id:?}"),"x":actual.origin.x.as_f32(),"y":actual.origin.y.as_f32(),"width":actual.size.width.as_f32(),"height":actual.size.height.as_f32()}));
        };
        for (tab, source) in app.tabs.iter().zip(expected["tabs"].as_array().unwrap()) {
            let id = tab.id;
            check(("connection-tab", id).into(), &source["root"]);
            let children = source["children"].as_array().unwrap();
            let space = children.iter().find(|row| row["class"] == "n-space").unwrap();
            check(("connection-tab-label", id).into(), space);
            check(("connection-tab-icon", id).into(), children.iter().find(|row| row["tag"] == "I" && row["class"].as_str().is_some_and(|name| name.starts_with("n-icon "))).unwrap());
            check(("connection-tab-text", id).into(), children.iter().find(|row| row["class"].as_str().is_some_and(|name| name.split_whitespace().any(|class| class == "n-ellipsis"))).unwrap());
            check(format!("close-{id}").into(), children.iter().find(|row| row["tag"] == "BUTTON").unwrap());
            check(("connection-tab-close-icon", id).into(), children.iter().find(|row| row["class"] == "n-base-icon").unwrap());
            if source["root"]["borderTop"].as_str().is_some_and(|border| border.starts_with("3px")) {
                let mut strip = source["root"].clone();
                strip["height"] = serde_json::json!(3.);
                check(("connection-tab-mark", id).into(), &strip);
            }
        }
        if let Some(parents) = expected["tabs"][0]["parents"].as_array() {
            check("connection-tabs".into(), &parents[2]);
        }
        if let Some(header) = expected["header"].as_array() {
            check("titlebar".into(), &header[0]);
            check("titlebar-title".into(), &header[1]);
            check("connection-tabs".into(), &header[2]);
        }
        println!("CONNECTION TABS FRAME: {}", serde_json::json!({"scene":scene,"measurements":measurements}));
        if !differences.is_empty() {
            let text = app.tabs[0].conn.name.clone();
            for weight in [400., 600., 700., 800., 900.] {
                let mut font = app.preference_font("general", ".SystemUIFont", cx);
                font.weight = FontWeight(weight);
                let shaped = window.text_system().shape_line(text.clone().into(), px(14.),
                    &[TextRun { len: text.len(), font, ..Default::default() }], None);
                println!("CONNECTION TABS FONT: {text} 14px weight {weight}: {}", shaped.width().as_f32());
            }
        }
        assert!(differences.is_empty(), "{scene} source geometry: {}", differences.join("; "));
    }).unwrap();
    println!("CONNECTION TABS LAYOUT QA PASS: {scene} original tab, label, server, close glyph and container rectangles");
}

fn assert_current(view: &Entity<RdmApp>, id: u64, editor: EntityId, value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window.update(cx, |_, _, cx| {
        let app = view.read(cx); assert_eq!(app.active, Some(id)); assert!(app.editing_entry());
        let dialog = app.dialog.as_ref().unwrap();
        assert_eq!(dialog.target.as_ref().unwrap().id, id);
        assert_eq!(dialog.editors["value"].entity_id(), editor);
        assert_eq!(dialog.value("value", cx), value);
    }).unwrap();
}

async fn prepare_editor(view: &Entity<RdmApp>, value: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) -> EntityId {
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let index = app.index().unwrap();
        let db = app.tabs[index].db;
        app.qa_prepare_entry_panel("edit-entry-hash", window, cx);
        // The shared visual fixture always selects DB0. This behavior probe
        // must retain the real owning tab database before opening async work.
        app.tabs[index].db = db;
        app.dialog.as_mut().unwrap().target.as_mut().unwrap().db = db;
    })).unwrap();
    settled(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let id = app.dialog.as_ref().unwrap().id;
        app.convert_entry(id, "None".into(), "JSON".into(), window, cx);
    })).unwrap();
    settled(view, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        app.set_dialog_field("value", value.to_string(), window, cx);
        app.dialog.as_ref().unwrap().editors["value"].entity_id()
    })).unwrap()
}

async fn close_confirm(view: &Entity<RdmApp>, tab: u64, count: usize, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    click(format!("close-{tab}").into(), window, cx); draw(window, cx).await;
    let id = window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.last().unwrap().id).unwrap();
    click(format!("tab-close-confirm-{id}").into(), window, cx);
    for _ in 0..100 {
        draw(window, cx).await;
        if window.update(cx, |_, _, cx| view.read(cx).tabs.len() == count).unwrap() { return; }
    }
    panic!("tab close did not finish");
}

async fn behavior(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let ids = reset(view, window, "connection-tabs-light", cx).await;
    let first = ids[0]; let editor = prepare_editor(view, DRAFT, window, cx).await;
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    assert_current(view, first, editor, DRAFT, window, cx);
    click("full-entry".into(), window, cx); draw(window, cx).await;
    assert!(window.update(cx, |_, _, cx| view.read(cx).full_editor).unwrap());
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let mut connection = app.tabs[0].conn.clone(); connection.name = "replica".into();
        app.open_connection(connection, window, cx);
        for tab in &mut app.tabs { tab.status_auto = false; tab.auto_refresh = false; }
    })).unwrap();
    settled(view, window, cx).await;
    let second = window.update(cx, |_, _, cx| {
        let app = view.read(cx); assert!(!app.entry_panel_visible());
        let cached = &app.cached_entry_panels[&first].0;
        assert_eq!(cached.editors["value"].entity_id(), editor); assert_eq!(cached.value("value", cx), DRAFT);
        app.active.unwrap()
    }).unwrap();
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    assert_current(view, first, editor, DRAFT, window, cx);
    assert!(window.update(cx, |_, _, cx| view.read(cx).full_editor).unwrap(), "the surviving pane retains fullscreen state");
    click("full-entry".into(), window, cx); draw(window, cx).await;
    #[cfg(target_os = "macos")]
    {
        key("ctrl-w", window, cx); draw(window, cx).await;
        assert!(window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.is_empty()).unwrap());
        key("cmd-w", window, cx); draw(window, cx).await;
        let warning = window.update(cx, |_, _, cx| view.read(cx).tab_close_warnings.last().unwrap().id).unwrap();
        click(format!("tab-close-cancel-{warning}").into(), window, cx); draw(window, cx).await;
        assert_current(view, first, editor, DRAFT, window, cx);
    }
    click(("connection-tab", second).into(), window, cx); draw(window, cx).await;
    let second_editor = prepare_editor(view, "replica isolation", window, cx).await;
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    assert_current(view, first, editor, DRAFT, window, cx);
    // Start a real conversion and hide its pane in the same UI turn. The reply
    // must update the cached owner, while the other tab's draft stays intact.
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let id = app.dialog.as_ref().unwrap().id;
        app.convert_entry(id, "None".into(), "JSON".into(), window, cx);
        app.activate_connection_tab(Some(second), cx);
        app.dialog.as_ref().unwrap().editors["value"].focus_handle(cx).focus(window, cx);
    })).unwrap();
    settled(view, window, cx).await;
    assert_current(view, second, second_editor, "replica isolation", window, cx);
    assert!(window.update(cx, |_, window, cx| view.read(cx).entry_editor_focused(window, cx)).unwrap(),
        "a hidden pane's conversion must not take focus from the visible editor");
    window.update(cx, |_, _, cx| {
        let cached = &view.read(cx).cached_entry_panels[&first].0;
        assert!(!cached.format_loading); assert_eq!(cached.value("format", cx), "JSON");
        assert_eq!(cached.value("value", cx), "before"); assert_eq!(cached.editors["value"].entity_id(), editor);
    }).unwrap();
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    assert_current(view, first, editor, "before", window, cx);
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| app.set_dialog_field("value", DRAFT, window, cx))).unwrap();
    // Closing an inactive third tab decrements the active index in the source.
    // Preserve the surviving editor even when that side effect changes panes.
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let mut connection = app.tabs[0].conn.clone(); connection.name = "qa-third".into();
        app.open_connection(connection, window, cx);
        for tab in &mut app.tabs { tab.status_auto = false; tab.auto_refresh = false; }
    })).unwrap();
    settled(view, window, cx).await;
    let third = window.update(cx, |_, _, cx| view.read(cx).active.unwrap()).unwrap();
    click(("connection-tab", second).into(), window, cx); draw(window, cx).await;
    assert_current(view, second, second_editor, "replica isolation", window, cx);
    close_confirm(view, third, 2, window, cx).await;
    assert_current(view, first, editor, DRAFT, window, cx);
    click(("connection-tab", second).into(), window, cx); draw(window, cx).await;
    assert_current(view, second, second_editor, "replica isolation", window, cx);
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    // Exercise a pinned save whose callback arrives after its pane is hidden.
    // DB15 is verified empty, then its temporary hash is removed before any
    // assertion about the saved result or cached response is made.
    let (service, connection) = window.update(cx, |_, _, cx| {
        let app = view.read(cx); (app.service.clone(), app.tabs[0].conn.clone())
    }).unwrap();
    let seed_service = service.clone(); let seed_connection = connection.clone();
    cx.background_executor().spawn(async move {
        assert_eq!(seed_service.run(&seed_connection, 15, command(&["DBSIZE"]))?.integer()?, 0);
        assert_eq!(seed_service.run(&seed_connection, 15, command(&["HSET", "aahash", "old", "before", "other", "kept"]))?.integer()?, 2);
        Ok::<_, anyhow::Error>(())
    }).await.unwrap();
    println!("CONNECTION TABS PINNED SCRATCH QA SEEDED: db15 aahash old/before other/kept; pre-existing key count zero");
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        let mut pinned = connection.clone(); pinned.name = "qa-pinned".into(); pinned.last_db = 15;
        app.open_connection(pinned, window, cx);
        for tab in &mut app.tabs { tab.status_auto = false; tab.auto_refresh = false; }
    })).unwrap();
    settled(view, window, cx).await;
    let pinned = window.update(cx, |_, _, cx| view.read(cx).active.unwrap()).unwrap();
    let saved_value = r#""cached saved""#;
    serde_json::from_str::<Json>(saved_value).unwrap();
    assert_eq!(window.update(cx, |_, _, cx| {
        let app = view.read(cx); app.tabs[app.index().unwrap()].db
    }).unwrap(), 15, "pinned scratch connection must select DB15");
    let pinned_editor = prepare_editor(view, saved_value, window, cx).await;
    window.update(cx, |_, window, cx| view.update(cx, |app, cx| {
        assert_eq!(app.dialog.as_ref().unwrap().target.as_ref().unwrap().db, 15);
        app.set_dialog_field("pin", "true", window, cx);
        app.submit_dialog(window, cx);
        app.activate_connection_tab(Some(first), cx);
        app.dialog.as_ref().unwrap().editors["value"].focus_handle(cx).focus(window, cx);
    })).unwrap();
    settled(view, window, cx).await;
    let actual_service = service.clone(); let actual_connection = connection.clone();
    let (saved, kept, deleted, remaining) = cx.background_executor().spawn(async move {
        let saved = actual_service.run(&actual_connection, 15, command(&["HGET", "aahash", "old"]))?.bytes();
        let kept = actual_service.run(&actual_connection, 15, command(&["HGET", "aahash", "other"]))?.bytes();
        let deleted = actual_service.run(&actual_connection, 15, command(&["DEL", "aahash"]))?.integer()?;
        let remaining = actual_service.run(&actual_connection, 15, command(&["DBSIZE"]))?.integer()?;
        Ok::<_, anyhow::Error>((saved, kept, deleted, remaining))
    }).await.unwrap();
    assert_eq!((deleted, remaining), (1, 0));
    assert_eq!(saved, saved_value.as_bytes()); assert_eq!(kept, b"kept");
    assert_current(view, first, editor, DRAFT, window, cx);
    window.update(cx, |_, window, cx| {
        let app = view.read(cx);
        assert!(app.entry_editor_focused(window, cx));
        let cached = &app.cached_entry_panels[&pinned].0;
        assert_eq!(cached.editors["value"].entity_id(), pinned_editor);
        let DialogKind::EditEntry(row) = &cached.kind else { panic!("pinned editor owner was lost"); };
        assert_eq!(row.value, saved_value.as_bytes());
        assert_eq!(cached.value("value", cx), saved_value);
        assert_eq!(app.tabs[app.tab_index(first).unwrap()].detail.as_ref().unwrap().rows[0].value, b"before");
    }).unwrap();
    click(("connection-tab", pinned).into(), window, cx); draw(window, cx).await;
    assert_current(view, pinned, pinned_editor, saved_value, window, cx);
    close_confirm(view, pinned, 2, window, cx).await;
    assert_current(view, second, second_editor, "replica isolation", window, cx);
    click(("connection-tab", first).into(), window, cx); draw(window, cx).await;
    close_confirm(view, second, 1, window, cx).await;
    assert_current(view, first, editor, DRAFT, window, cx);
    let (service, connection) = window.update(cx, |_, _, cx| {
        let app = view.read(cx); assert!(app.cached_entry_panels.is_empty());
        (app.service.clone(), app.tabs[0].conn.clone())
    }).unwrap();
    cx.background_executor().spawn(async move {
        assert_eq!(service.run(&connection, 0, command(&["HGET", "aahash", "name"]))?.bytes(), b"Tiny RDM");
        assert_eq!(service.run(&connection, 0, command(&["DBSIZE"]))?.integer()?, 19);
        Ok::<_, anyhow::Error>(())
    }).await.unwrap();
    println!("CONNECTION TABS RETENTION QA PASS: same-active/open/switch/return retain model, draft and fullscreen; macOS modifiers; hidden conversion and pinned save update only their owner without stealing focus; inactive close retains surviving drafts; DB15 temporary save cleaned before result assertions; DB0 unchanged");
}

pub(in crate::ui) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, scene: &str, cx: &mut AsyncApp) {
    if scene == "connection-tabs-retained-draft" { behavior(view, window, cx).await; return; }
    reset(view, window, scene, cx).await;
    draw(window, cx).await;
    assert_layout(view, scene, window, cx);
}
