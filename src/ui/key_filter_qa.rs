use super::qa_json::{click, idle};
use super::*;

fn source(scene: &str) -> Json {
    let fixture: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/key-filter-macos-source.json"
    ))
    .unwrap();
    if let Some(record) = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == scene)
    {
        return record.clone();
    }
    let overflow = scene.starts_with("tag-overflow-");
    let general_fallback = scene.starts_with("general-fallback-");
    let css_label = scene.starts_with("css-label-");
    let selector_font = scene.starts_with("selector-font-") || general_fallback || css_label;
    let contract = scene.starts_with("sidebar-contract-");
    let sidebar = scene.starts_with("sidebar-resize-") || contract;
    let styles: Json = serde_json::from_str(if css_label {
        include_str!("../../tests/fixtures/css-label-macos-source.json")
    } else if general_fallback {
        include_str!("../../tests/fixtures/general-fallback-macos-source.json")
    } else if selector_font {
        include_str!("../../tests/fixtures/selector-font-macos-source.json")
    } else if contract {
        include_str!("../../tests/fixtures/sidebar-contract-macos-source.json")
    } else if sidebar {
        include_str!("../../tests/fixtures/sidebar-resize-macos-source.json")
    } else if overflow {
        include_str!("../../tests/fixtures/tag-overflow-macos-source.json")
    } else {
        include_str!("../../tests/fixtures/tag-style-macos-source.json")
    })
    .unwrap();
    let style = styles["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == scene)
        .unwrap();
    let config = &style["configuration"];
    let metrics = style["styles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let mut m = m.clone();
            m["id"] = match m["id"].as_str().unwrap() {
                "tag" => "match",
                "content" => "match-label",
                "close" => "match-close",
                id => id,
            }
            .into();
            m
        })
        .collect::<Vec<_>>();
    serde_json::json!({"name":scene,"configuration":{"size":config["size"],"theme":config["theme"],"family":if selector_font {config["family"].clone()}else{Json::from("")},"keyType":config["keyType"],"asideWidth":config["asideWidth"],"label":config["label"],"requestedWidth":config["requestedWidth"],"mode":config["mode"],"action":if config["draft"]==true {"match-draft"} else {"match"}},
        "filter":style["filter"],"match":style["match"],"inputFocused":style["inputFocused"],"exact":false,"searchDisabled":style["searchDisabled"],"metrics":metrics,"tagStyle":true,"tagOverflow":overflow,"selectorFont":selector_font,"generalFallback":general_fallback,"cssLabel":css_label,"sourceBaselineY":style["baselineMeasurement"]["baselineY"],"sidebarResize":sidebar,"sidebarContract":contract,"expectedResizing":style["resizing"],"actualSidebarWidth":style["actualSidebarWidth"]})
}

impl RdmApp {
    pub(super) fn qa_prepare_key_filter(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let record = source(scene);
        let config = &record["configuration"];
        self.pref.general.font_size = config["size"].as_f64().unwrap() as f32;
        self.pref.general.font.clear();
        self.pref.general.font_family = config["family"].as_str().filter(|s| !s.is_empty())
            .map(|family| vec![family.to_owned()]).unwrap_or_default();
        self.pref.general.theme = config["theme"].as_str().unwrap().into();
        self.pref.behavior.aside_width = config["asideWidth"].as_f64().unwrap_or(300.) as f32;
        self.apply_theme(window, cx);
        self.nav = Nav::Browser;
        let i = self.index().unwrap();
        let tab = &mut self.tabs[i];
        tab.pane = Pane::Status;
        tab.current_key = None;
        tab.detail = None;
        tab.info_details = false;
        tab.key_selection = None;
        tab.check_mode = false;
        tab.selected.clear();
        tab.expanded.clear();
        tab.key_root_expanded = true;
        tab.key_match.clear();
        tab.key_filter.clear();
        tab.key_filter_changed = None;
        tab.key_filter_epoch += 1;
        tab.exact = false;
        set_input(&tab.filter, "", window, cx);
        let id = tab.id;
        self.close_database_picker(id, window, cx);
        self.close_type_picker(id, cx);
        self.tabs[i].key_type = config["keyType"].as_str().unwrap_or("").into();
        if self.tabs[i].db != 0 {
            self.change_database(id, 0, window, cx);
        } else {
            self.scan_keys(id, true, false, window, cx);
        }
        window.blur(cx);
    }
}

fn key(name: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(name).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
}

fn double_click(id: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        // A native first press may start outside the input's focus surface.
        // Test the complete sequence from an unfocused input, rather than
        // inheriting focus from the programmatic query used to make the tag.
        window.blur(cx);
        window.refresh();
        let _ = window.draw(cx);
        let position =
            gpui_base::test_support::find(window, &[], &SharedString::from(id.to_owned()).into())
                .unwrap()
                .bounds()
                .center();
        for click_count in [1, 2] {
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    click_count,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    click_count,
                    ..Default::default()
                }),
                cx,
            );
            window.refresh();
            let _ = window.draw(cx);
        }
    })
    .unwrap();
}

async fn drag_sidebar(width: f32, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.blur(cx);
        window.refresh();
        let _ = window.draw(cx);
        let header = gpui_base::test_support::find(window, &[], &"key-browser-header".into())
            .unwrap()
            .bounds();
        let position = point(header.right() + px(1.), px(300.));
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
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
            PlatformInput::MouseMove(MouseMoveEvent {
                position: position + point(px(6.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(30))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        let position = point(px(50. + width), px(300.));
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.refresh();
        let _ = window.draw(cx);
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
    cx.background_executor()
        .timer(Duration::from_millis(450))
        .await;
}

async fn hold_sidebar(mode: &str, width: f32, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.blur(cx);
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &"sidebar-divider".into())
            .unwrap()
            .bounds()
            .center();
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
        if mode != "hover" {
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        }
        window.refresh();
        let _ = window.draw(cx);
        if mode == "dragging" {
            let position = point(px(50. + width), px(300.));
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
            window.refresh();
            let _ = window.draw(cx);
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
        }
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(450))
        .await;
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    let reference = source(scene);
    let action = reference["configuration"]["action"].as_str().unwrap();
    let query = reference["configuration"]["label"]
        .as_str()
        .unwrap_or("hash");
    idle(view, window, cx).await;
    if !matches!(action, "base" | "dark" | "menlo") {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let input = app.tabs[app.index().unwrap()].filter.clone();
                    input.update(cx, |input, cx| {
                        input.focus(window, cx);
                        input.replace_text_in_range(None, query, window, cx);
                    });
                })
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
    }
    if action.starts_with("match") {
        key("enter", window, cx);
        idle(view, window, cx).await;
    }
    if action == "match-draft" {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let input = app.tabs[app.index().unwrap()].filter.clone();
                    input.update(cx, |input, cx| {
                        input.replace_text_in_range(None, "list", window, cx)
                    });
                })
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
    }
    let sidebar = reference["sidebarResize"] == true;
    let mode = reference["configuration"]["mode"]
        .as_str()
        .unwrap_or("idle");
    let holding = reference["sidebarContract"] == true && mode != "overflow";
    if sidebar {
        let before = window
            .update(cx, |_, _, cx| {
                view.read(cx)
                    .store
                    .preferences()
                    .unwrap()
                    .behavior
                    .aside_width
            })
            .unwrap();
        let width = reference["configuration"]["requestedWidth"]
            .as_f64()
            .unwrap() as f32;
        if holding {
            hold_sidebar(mode, width, window, cx).await;
        } else {
            drag_sidebar(width, window, cx).await;
        }
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(
                    app.store.preferences().unwrap().behavior.aside_width,
                    before,
                    "sidebar writes are trailing1000ms,not immediate"
                );
            })
            .unwrap();
        println!("SIDEBAR SAVE EARLY QA PASS: {scene} preference unchanged at450ms");
    }
    match action {
        "clear" => click("clear-key-filter", window, cx),
        "match-close" => click("clear-key-match", window, cx),
        "match-edit" => {
            double_click("key-search-match", window, cx);
            cx.background_executor()
                .timer(Duration::from_millis(1100))
                .await;
        }
        name if name.starts_with("exact") => click("exact-match", window, cx),
        _ => {}
    }
    idle(view, window, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        // The original source probe explicitly blurs after applying the query.
        if reference["selectorFont"] == true { window.blur(cx); }
        if !holding {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: point(px(1000.), px(700.)),
                    ..Default::default()
                }),
                cx,
            );
        }
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(if sidebar && !holding {
            450
        } else {
            100
        }))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    window.update(cx, |_, window, cx| {
        let app = view.read(cx);
        let tab = &app.tabs[app.index().unwrap()];
        let value = text_input(&tab.filter, cx);
        assert_eq!(value, reference["filter"].as_str().unwrap(), "{scene} actual input after original action");
        assert_eq!(tab.key_match, reference["match"].as_str().unwrap(), "{scene} match after original action");
        assert_eq!(tab.exact, reference["exact"].as_bool().unwrap());
        assert_eq!(tab.filter.focus_handle(cx).is_focused(window), reference["inputFocused"].as_bool().unwrap(), "{scene} input focus is preserved by controls");
        assert_eq!(!tab.key_match.is_empty() && value.trim().is_empty(), reference["searchDisabled"].as_bool().unwrap());
        let rendered_search=gpui_base::test_support::find(window,&[],&"filter-keys".into());
        // GPUI does not expose aria-disabled. The rendered Click action must
        // be removed while disabled; enabled pointer input is checked below.
        if let Some(search)=rendered_search {assert_eq!(search.supports_click(),reference["searchDisabled"]==false,"actual rendered search Click action");}else{assert!(sidebar && mode=="overflow","visible search must have a snapshot");}
        let tag_style=reference["tagStyle"]==true;
        let tag_overflow=reference["tagOverflow"]==true;
        let sidebar=reference["sidebarResize"]==true;
        let metrics = reference["metrics"].as_array().unwrap();
        let observed = gpui_base::test_support::snapshots(window).into_iter()
            .filter(|s| s.bounds().origin.x < px(350.) && s.bounds().origin.y < px(80.))
            .map(|s| { let b=s.bounds(); serde_json::json!({"id":format!("{:?}",s.path().last()),"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32()}) }).collect::<Vec<_>>();
        println!("KEY FILTER LAYOUT CANDIDATES: {}",serde_json::json!({"scene":scene,"frames":observed}));
        let selector_font=reference["selectorFont"]==true;
        if selector_font { assert_eq!(tab.key_type,reference["configuration"]["keyType"].as_str().unwrap()); }
        let mappings=if selector_font {vec![("key-browser-header","header"),("key-filter-group","group"),("type-filter","selector"),("key-filter-input-wrap","outer"),("filter-keys","search")]} else if sidebar {vec![("browser-sidebar","sidebar"),("sidebar-divider","divider"),("key-browser-header","header"),("key-filter-input-wrap","outer"),("filter-keys","search")]} else if tag_style {vec![("filter-keys","search")]} else {vec![("key-browser-header", "header"), ("key-filter-group", "group"), ("key-filter-input-wrap", "outer"), ("exact-match", "exact"), ("filter-keys", "search"), ("key-filter-clear-slot", "clear")]};
        let mut frames = Vec::new();
        let mut differences = Vec::new();
        let mut unobserved_offscreen = Vec::new();
        let mut check = |id: &str, source_id: &str| {
            let expected = metrics.iter().find(|m| m["id"] == source_id).unwrap();
            let snapshot=gpui_base::test_support::find(window,&[],&SharedString::from(id.to_owned()).into());
            let Some(snapshot)=snapshot else {
                assert!(sidebar && expected["x"].as_f64().unwrap()>=window.viewport_size().width.as_f32()as f64,"missing visible {id}");
                unobserved_offscreen.push(source_id.to_string());return;
            };
            let bounds=snapshot.bounds();
            let actual = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32(), bounds.size.width.as_f32(), bounds.size.height.as_f32()];
            for (value, dimension) in actual.into_iter().zip(["x", "y", "w", "h"]) {
                let delta=(value-expected[dimension].as_f64().unwrap() as f32).abs();
                if delta>=0.6 {differences.push(serde_json::json!({"id":id,"dimension":dimension,"actual":value,"expected":expected[dimension],"logicalDelta":delta,"threshold":0.6}));}
            }
            frames.push(serde_json::json!({"id":source_id,"x":actual[0],"y":actual[1],"w":actual[2],"h":actual[3]}));
        };
        for (id, source_id) in mappings { check(id, source_id); }
        if !tab.key_match.is_empty() { check("key-search-match", "match"); }
        if tag_style {
            check("key-search-match-label","match-label");check("clear-key-match","match-close");
        } else if !value.is_empty() { check("clear-key-filter", "clear"); }
        if sidebar {
            assert_eq!(app.sidebar_resizing,reference["expectedResizing"]==true,"source pressed/dragging state");
            let expected=reference["actualSidebarWidth"].as_f64().unwrap()as f32;
            if (app.pref.behavior.aside_width-expected).abs()>=0.6 {differences.push(serde_json::json!({"id":"preference","dimension":"aside_width","actual":app.pref.behavior.aside_width,"expected":expected,"threshold":0.6}));}
        }
        let baseline = if reference["cssLabel"] == true {
            let text = gpui_base::test_support::find(window, &[], &"key-search-match-text".into()).unwrap();
            let font = app.preference_font("general", ".SystemUIFont", cx);
            let family = font.family.to_string();
            let configured = reference["configuration"]["family"].as_str().unwrap();
            let expected_family = if configured.is_empty() { ".SystemUIFont" } else { configured };
            assert!(family.eq_ignore_ascii_case(expected_family), "{scene} resolved family: {family} vs {expected_family}");
            let line = window.text_system().shape_line(tab.key_match.clone().into(), px(12.),
                &[TextRun { len: tab.key_match.len(), font, color: Hsla::white(), background_color: None, underline: None, strikethrough: None }], None);
            let actual = (text.bounds().origin.y + (px(12.) + line.ascent - line.descent) * 0.5).as_f32();
            let expected = reference["sourceBaselineY"].as_f64().unwrap() as f32;
            let delta = (actual - expected).abs();
            assert!(delta < 0.6, "{scene} measured text baseline: {actual} vs {expected}");
            serde_json::json!({"actual":actual,"source":expected,"logicalDelta":delta,"threshold":0.6,"family":family})
        } else { Json::Null };
        let prefix=if reference["cssLabel"]==true {"CSS LABEL"}else if reference["generalFallback"]==true {"GENERAL FALLBACK"}else if selector_font {"SELECTOR FONT"}else if reference["sidebarContract"]==true {"SIDEBAR CONTRACT"}else if sidebar {"SIDEBAR RESIZE"}else if tag_overflow {"MATCH TAG OVERFLOW"}else if tag_style {"MATCH TAG MACOS"}else{"KEY FILTER MACOS"};
        println!("{prefix} FRAME: {}", serde_json::json!({"scene":scene,"frames":frames,"filter":value,"match":tab.key_match,"exact":tab.exact,"inputFocused":tab.filter.focus_handle(cx).is_focused(window),"asideWidth":app.pref.behavior.aside_width,"resizing":app.sidebar_resizing,"hovering":app.sidebar_hover,"unobservedOffscreen":unobserved_offscreen,"baseline":baseline}));
        if !differences.is_empty() {
            println!("{prefix} QA FAIL: {}",serde_json::json!({"scene":scene,"differences":differences}));
            let retain=(tag_overflow && std::env::var("RUST_RDM_QA_RETAIN_TAG_OVERFLOW").as_deref()==Ok("1")) ||(sidebar&&std::env::var("RUST_RDM_QA_RETAIN_SIDEBAR_RESIZE").as_deref()==Ok("1")) ||(selector_font&&std::env::var("RUST_RDM_QA_RETAIN_SELECTOR_FONT").as_deref()==Ok("1"));
            assert!(retain,"{scene} source rectangles failed: {differences:?}");
            println!("{prefix} QA RETAINED NEGATIVE: {scene}; diagnostic image only, not acceptance");
        } else {
            println!("{prefix} QA PASS: {scene} original header rectangles and input/Enter/clear/tag/exact operations");
        }
    }).unwrap();
}

pub(super) async fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene.starts_with("sidebar-resize-") || scene.starts_with("sidebar-contract-") {
        cx.update_window(window.into(), |_, window, cx| {
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: point(px(1000.), px(700.)),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            )
        })
        .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(
                    app.store.preferences().unwrap().behavior.aside_width,
                    app.pref.behavior.aside_width,
                    "trailing save persists current width"
                );
            })
            .unwrap();
        println!("SIDEBAR SAVE TRAILING QA PASS: {scene}");
        let original_size = cx
            .update_window(window.into(), |_, window, cx| {
                let original = window.viewport_size();
                let width = view.read(cx).pref.behavior.aside_width;
                if width + 54. > original.width.as_f32() {
                    window.resize(size(px(width + 300.), original.height));
                    window.refresh();
                    let _ = window.draw(cx);
                }
                original
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        drag_sidebar(300., window, cx).await;
        cx.update_window(window.into(), |_, window, cx| {
            window.resize(original_size);
            window.refresh();
            let _ = window.draw(cx);
        })
        .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
        window
            .update(cx, |_, _, cx| {
                assert_eq!(
                    view.read(cx)
                        .store
                        .preferences()
                        .unwrap()
                        .behavior
                        .aside_width,
                    300.
                )
            })
            .unwrap();
    }
    if scene.starts_with("tag-overflow-") {
        let original = source(scene)["match"].as_str().unwrap().to_string();
        double_click("key-search-match", window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
        window
            .update(cx, |_, window, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert_eq!(
                    text_input(&tab.filter, cx),
                    original,
                    "double click restores the complete untruncated query"
                );
                assert!(tab.key_match.is_empty());
                assert!(tab.filter.focus_handle(cx).is_focused(window));
            })
            .unwrap();
        key("enter", window, cx);
        idle(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert_eq!(tab.key_match, original);
                assert_eq!(text_input(&tab.filter, cx), "");
            })
            .unwrap();
        click("clear-key-match", window, cx);
        idle(view, window, cx).await;
        window
            .update(cx, |_, window, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert!(tab.key_match.is_empty());
                assert_eq!(text_input(&tab.filter, cx), "");
                assert!(tab.filter.focus_handle(cx).is_focused(window));
            })
            .unwrap();
        println!(
            "MATCH TAG OVERFLOW ACTIONS PASS: {scene} double-click,full query,Enter,close and focus"
        );
    }
    let query = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            text_input(&app.tabs[app.index().unwrap()].filter, cx)
                .trim()
                .to_string()
        })
        .unwrap();
    if !query.is_empty() {
        click("filter-keys", window, cx);
        idle(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert_eq!(
                    tab.key_match, query,
                    "actual enabled search click applies the input"
                );
                assert_eq!(text_input(&tab.filter, cx), "");
            })
            .unwrap();
        println!("MATCH TAG SEARCH ACTIVATION PASS: actual pointer click applies {query:?}");
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                tab.key_match.clear();
                tab.key_filter.clear();
                tab.key_filter_epoch += 1;
                tab.exact = false;
                tab.key_type.clear();
                set_input(&tab.filter, "", window, cx);
                let id = tab.id;
                app.pref.general.font_size = 14.;
                app.pref.general.font_family.clear();
                app.pref.general.font.clear();
                app.pref.general.theme = "light".into();
                app.pref.behavior.aside_width = 300.;
                app.apply_theme(window, cx);
                app.scan_keys(id, true, false, window, cx);
                window.blur(cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
}
