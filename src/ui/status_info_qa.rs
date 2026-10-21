//! Actual status-info rendering with captured original INFO payloads.
use super::qa_json::idle;
use super::*;

pub(super) struct Saved {
    id: u64,
    info: Info,
    pane: Pane,
    details: bool,
    auto: bool,
    group: String,
    keyword: String,
    preferences: Preferences,
    viewport: gpui::Size<Pixels>,
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    idle(view, window, cx).await;
    let fixture: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/status-info-macos-source.json"
    ))
    .unwrap();
    let row = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == scene)
        .unwrap();
    println!("STATUS INFO START: {scene}");
    let saved = cx
        .update_window(window.into(), |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let preferences = app.pref.clone();
                let keyword = text_input(&app.tabs[i].info_filter, cx);
                let tab = &mut app.tabs[i];
                let saved = Saved {
                    id: tab.id,
                    info: std::mem::take(&mut tab.info),
                    pane: tab.pane,
                    details: tab.info_details,
                    auto: tab.status_auto,
                    group: tab.info_group.clone(),
                    keyword,
                    preferences,
                    viewport: window.viewport_size(),
                };
                tab.info = row["serverInfo"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(section, values)| {
                        (
                            section.clone(),
                            values
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                                .collect(),
                        )
                    })
                    .collect();
                tab.pane = Pane::Status;
                tab.status_auto = false;
                tab.info_details = true;
                tab.info_group = row["configuration"]["group"].as_str().unwrap().into();
                set_input(
                    &tab.info_filter,
                    row["configuration"]["keyword"].as_str().unwrap(),
                    window,
                    cx,
                );
                let config = &row["configuration"];
                app.pref.general.theme = config["theme"].as_str().unwrap().into();
                let family = config["family"].as_str().unwrap();
                app.pref.general.font_family = if family.is_empty() {
                    vec![]
                } else {
                    vec![family.into()]
                };
                app.pref.general.font_size = config["size"].as_f64().unwrap() as f32;
                app.general_css_size = app.pref.general.font_size;
                app.pref.behavior.aside_width = 300.;
                app.nav = Nav::Browser;
                app.apply_theme(window, cx);
                window.resize(size(
                    px(config["windowWidth"].as_f64().unwrap() as f32),
                    px(768.),
                ));
                window.blur(cx);
                cx.notify();
                saved
            })
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        draw(window, cx);
        if scene == "status-info-light-default" {
            click("info-group-CPU", window, cx);
            assert_state(view, "", "", 16, cx);
            click("info-group-Memory", window, cx);
            assert_state(view, "Memory", "", 4, cx);
            view.update(cx, |app, cx| {
                let input = app.tabs[app.index().unwrap()].info_filter.clone();
                set_input(&input, "Alpha", window, cx);
            });
            draw(window, cx);
            assert_state(view, "Memory", "Alpha", 1, cx);
            view.update(cx, |app, cx| {
                let input = app.tabs[app.index().unwrap()].info_filter.clone();
                set_input(&input, "ALPHA", window, cx);
            });
            draw(window, cx);
            assert_state(view, "Memory", "ALPHA", 0, cx);
            click("clear-status-info-filter", window, cx);
            assert_state(view, "Memory", "", 4, cx);
            let input = view.read(cx).tabs[view.read(cx).index().unwrap()].info_filter.clone();
            assert!(input.focus_handle(cx).is_focused(window));
            click("activity", window, cx);
            assert!(!view.read(cx).tabs[view.read(cx).index().unwrap()].info_details);
            click("status-info", window, cx);
            assert_state(view, "Memory", "", 4, cx);
            click("info-group-CPU", window, cx);
            assert_state(view, "CPU", "", 2, cx);
            println!("QA PASS: status-info mouse group toggle,case-sensitive InputState changes,clear/focus and tab preservation;6 mouse clicks,no clipboard/native keyboard claim");
        } else if scene == "status-info-empty-cpu" {
            click("info-group-CPU", window, cx);
            assert_state(view, "CPU", "", 14, cx);
            println!("QA PASS: status-info empty CPU is not clickable and falls back to all14 fields");
        }
        window.blur(cx);
        window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: point(px(20.), px(760.)), ..Default::default() }), cx);
        draw(window, cx);
        let mut ids: Vec<String> = ["status-root", "status-summary-card", "status-activity-card", "status-info", "activity", "status-info-filter", "status-info-groups", "status-info-table", "status-info-body", "status-info-header-key", "status-info-header-value", "status-info-empty"].into_iter().map(str::to_string).collect();
        ids.extend(row["serverInfo"].as_object().unwrap().keys().map(|k| format!("info-group-{k}")));
        for i in 0..100 { ids.push(format!("status-info-row-{i}")); for j in 0..2 { ids.push(format!("status-info-cell-{i}-{j}")); } }
        let mut frames = Vec::new();
        for id in ids {
            if let Some(node) = gpui_base::test_support::find(window, &[], &id.clone().into()) {
                let b = node.bounds(); frames.push(serde_json::json!({"id":id,"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32(),"label":node.label(),"visible":node.visible(),"disabled":node.disabled(),"supportsClick":node.supports_click()}));
            }
        }
        let app = view.read(cx); let tab = &app.tabs[app.index().unwrap()];
        println!("STATUS INFO FRAME: {}", serde_json::json!({"scene":scene,"frames":frames,"group":tab.info_group,"keyword":text_input(&tab.info_filter,cx),"scope":"Actual GPUI identified layout and accessible row data;source comparison separate,no whole-image/native acceptance."}));
    }).unwrap();
    saved
}

fn draw(window: &mut Window, cx: &mut App) {
    for _ in 0..2 {
        window.refresh();
        let _ = window.draw(cx);
    }
}

fn click(id: &str, window: &mut Window, cx: &mut App) {
    draw(window, cx);
    let point = gpui_base::test_support::find(window, &[], &id.to_string().into())
        .unwrap()
        .bounds()
        .center();
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position: point,
            ..Default::default()
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: point,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: point,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
}

fn assert_state(view: &Entity<RdmApp>, group: &str, keyword: &str, rows: usize, cx: &App) {
    let app = view.read(cx);
    let tab = &app.tabs[app.index().unwrap()];
    assert_eq!(tab.info_group, group);
    assert_eq!(text_input(&tab.info_filter, cx), keyword);
    assert_eq!(tab.status_info_rows(keyword).len(), rows);
    assert!(tab.info_details);
}

pub(super) fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: Saved,
    cx: &mut AsyncApp,
) {
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.tab_index(saved.id).unwrap();
            let tab = &mut app.tabs[i];
            tab.info = saved.info;
            tab.pane = saved.pane;
            tab.info_details = saved.details;
            tab.status_auto = saved.auto;
            tab.info_group = saved.group;
            set_input(&tab.info_filter, saved.keyword, window, cx);
            app.pref = saved.preferences;
            app.general_css_size = app.pref.general.font_size;
            app.apply_theme(window, cx);
            window.resize(saved.viewport);
            cx.notify();
        })
    })
    .unwrap();
}
