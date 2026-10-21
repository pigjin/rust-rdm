use super::qa_json::idle;
use super::*;

fn source(scene: &str) -> Json {
    let data: Json = serde_json::from_str(if scene.starts_with("entry-control-") {
        include_str!("../../tests/fixtures/entry-controls-macos-source.json")
    } else if scene.starts_with("entry-match-width-") {
        include_str!("../../tests/fixtures/entry-match-macos-source.json")
    } else {
        include_str!("../../tests/fixtures/match-tooltip-macos-source.json")
    })
    .unwrap();
    data["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == scene)
        .unwrap()
        .clone()
}

impl RdmApp {
    pub(super) fn qa_prepare_match_tooltip(
        &mut self,
        scene: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = source(scene);
        let c = &row["configuration"];
        self.qa_prepare_key_filter(
            if c["family"] == "Courier" {
                "css-label-dark-courier"
            } else {
                "css-label-light-unicode"
            },
            window,
            cx,
        );
        let i = self.index().unwrap();
        self.tabs[i]
            .match_tooltips
            .cancel(super::match_tooltip::MatchKind::Key);
        self.tabs[i]
            .match_tooltips
            .cancel(super::match_tooltip::MatchKind::Entry);
        self.tabs[i].row_match.clear();
        self.tabs[i].row_filter_value.clear();
        set_input(&self.tabs[i].row_filter, "", window, cx);
        self.tabs[i]
            .column_widths
            .retain(|key, _| !key.starts_with("entry-"));
        self.pref.editor.entry_text_align = 0;
        if let Some(width) = c["windowWidth"].as_f64() {
            window.resize(size(px(width as f32), px(768.)));
        }
        if c["host"] == "entry" {
            let id = self.tabs[i].id;
            self.select_key(id, b"aahash".to_vec(), window, cx);
        }
    }
}

fn move_pointer(window: WindowHandle<Root>, position: Point<Pixels>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    let row = source(scene);
    let c = &row["configuration"];
    let entry = c["host"] == "entry";
    idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let input = if entry {
                    app.tabs[i].row_filter.clone()
                } else {
                    app.tabs[i].filter.clone()
                };
                input.update(cx, |state, cx| {
                    state.focus(window, cx);
                    state.replace_text_in_range(None, c["label"].as_str().unwrap(), window, cx);
                });
            })
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(1100))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        )
    })
    .unwrap();
    idle(view, window, cx).await;
    if scene.starts_with("entry-control-") {
        verify_hidden_controls(view, window, &row, scene, cx).await;
        return;
    }
    let tag_id = if entry {
        "entry-search-match"
    } else {
        "key-search-match"
    };
    let tooltip_id = if entry {
        "entry-match-tooltip"
    } else {
        "key-match-tooltip"
    };
    cx.update_window(window.into(), |_, window, cx| {
        window.blur(cx);
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    move_pointer(window, point(px(1000.), px(700.)), cx);
    cx.background_executor()
        .timer(Duration::from_millis(160))
        .await;
    let tag = window
        .update(cx, |_, window, _| {
            gpui_base::test_support::find(window, &[], &tag_id.into())
                .unwrap()
                .bounds()
        })
        .unwrap();
    move_pointer(window, tag.center(), cx);
    cx.background_executor()
        .timer(Duration::from_millis(50))
        .await;
    window
        .update(cx, |_, window, _| {
            assert!(
                gpui_base::test_support::find(window, &[], &tooltip_id.into()).is_none(),
                "source show delay is100ms"
            )
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh(); let _ = window.draw(cx);
        let app = view.read(cx); let tab = &app.tabs[app.index().unwrap()];
        assert_eq!(if entry { &tab.row_match } else { &tab.key_match }, c["label"].as_str().unwrap());
        let tip = gpui_base::test_support::find(window, &[], &tooltip_id.into()).unwrap();
        assert_eq!(tip.label(), row["tooltipText"].as_str());
        let mut frames = Vec::new();
        if scene.starts_with("entry-match-width-") {
            for (id, native_id) in [("group", "entry-filter-group"), ("outer", "entry-filter"), ("search", "entry-search")] {
                let bounds = gpui_base::test_support::find(window, &[], &native_id.into()).unwrap().bounds();
                let reference = row["styles"].as_array().unwrap().iter().find(|v| v["id"] == id).unwrap();
                let values = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32(), bounds.size.width.as_f32(), bounds.size.height.as_f32()];
                for (name, actual) in ["x", "y", "w", "h"].into_iter().zip(values) {
                    assert!((actual - reference[name].as_f64().unwrap() as f32).abs() < 0.6, "{scene} {id}.{name}: {actual} != {}", reference[name]);
                }
                frames.push(serde_json::json!({"id":id,"x":values[0],"y":values[1],"w":values[2],"h":values[3]}));
            }
        }
        for (id, native_id) in [("tag", tag_id.to_string()), ("tooltip", tooltip_id.to_string()), ("arrow", format!("{tooltip_id}-arrow"))] {
            let observed = gpui_base::test_support::find(window, &[], &native_id.into()).unwrap();
            let bounds = observed.bounds();
            let reference = row["styles"].as_array().unwrap().iter().find(|v| v["id"] == id).unwrap();
            let values = [bounds.origin.x.as_f32(), bounds.origin.y.as_f32(), bounds.size.width.as_f32(), bounds.size.height.as_f32()];
            for (name, actual) in ["x","y","w","h"].into_iter().zip(values) { assert!((actual - reference[name].as_f64().unwrap() as f32).abs() < 0.6, "{scene} {id}.{name}: {actual} != {}", reference[name]); }
            frames.push(serde_json::json!({"id":id,"x":values[0],"y":values[1],"w":values[2],"h":values[3]}));
        }
        println!("MATCH TOOLTIP FRAME: {}", serde_json::json!({"scene":scene,"frames":frames,"text":tip.label(),"inputFocused":if entry {tab.row_filter.focus_handle(cx).is_focused(window)} else {tab.filter.focus_handle(cx).is_focused(window)}}));
    }).unwrap();
    let center = window
        .update(cx, |_, window, _| {
            gpui_base::test_support::find(window, &[], &tooltip_id.into())
                .unwrap()
                .bounds()
                .center()
        })
        .unwrap();
    move_pointer(window, center, cx);
    cx.background_executor()
        .timer(Duration::from_millis(160))
        .await;
    window
        .update(cx, |_, window, _| {
            assert!(
                gpui_base::test_support::find(window, &[], &tooltip_id.into()).is_some(),
                "pointer transfer retains tooltip"
            )
        })
        .unwrap();
    move_pointer(window, point(px(1000.), px(700.)), cx);
    cx.background_executor()
        .timer(Duration::from_millis(180))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        for _ in 0..2 {
            window.refresh();
            let _ = window.draw(cx);
        }
        let app = view.read(cx);
        let i = app.index().unwrap();
        let kind = if entry {
            super::match_tooltip::MatchKind::Entry
        } else {
            super::match_tooltip::MatchKind::Key
        };
        println!(
            "MATCH TOOLTIP HIDE DIAGNOSTIC: {scene} state={:?} mouse={:?} observed={:?}",
            app.tabs[i].match_tooltips.qa_state(kind),
            window.mouse_position(),
            gpui_base::test_support::find(window, &[], &tooltip_id.into())
        );
    })
    .unwrap();
    window
        .update(cx, |_, window, _| {
            assert!(
                gpui_base::test_support::find(window, &[], &tooltip_id.into()).is_none(),
                "leaving hides tooltip"
            )
        })
        .unwrap();
    for edge in ["right", "bottom"] {
        move_pointer(window, tag.center(), cx);
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        let body = cx
            .update_window(window.into(), |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
                gpui_base::test_support::find(window, &[], &tooltip_id.into())
                    .unwrap()
                    .bounds()
            })
            .unwrap();
        move_pointer(window, body.center(), cx);
        cx.background_executor()
            .timer(Duration::from_millis(160))
            .await;
        let outside = if edge == "right" {
            point(body.right() + px(5.), body.center().y)
        } else {
            point(body.center().x, body.bottom() + px(5.))
        };
        move_pointer(window, outside, cx);
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
        cx.update_window(window.into(), |_, window, cx| {
            for _ in 0..2 {
                window.refresh();
                let _ = window.draw(cx);
            }
            let app = view.read(cx);
            let i = app.index().unwrap();
            let kind = if entry {
                super::match_tooltip::MatchKind::Entry
            } else {
                super::match_tooltip::MatchKind::Key
            };
            let state = app.tabs[i].match_tooltips.qa_state(kind);
            println!(
                "MATCH TOOLTIP EDGE: {scene} {edge} body={body:?} mouse={outside:?} state={state:?}"
            );
            assert!(
                !state.0 && !state.1,
                "{scene}: leaving {edge} edge hides model tooltip"
            );
            assert!(
                gpui_base::test_support::find(window, &[], &tooltip_id.into()).is_none(),
                "{scene}: leaving {edge} edge removes tooltip"
            );
        })
        .unwrap();
    }
    move_pointer(window, tag.center(), cx);
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        assert!(gpui_base::test_support::find(window, &[], &tooltip_id.into()).is_some());
    })
    .unwrap();
    println!(
        "MATCH TOOLTIP QA PASS: {scene} actualEnter,100ms show,pointer transfer,far/right/bottom hide,full pattern and original rectangles"
    );
}

async fn verify_hidden_controls(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    row: &Json,
    scene: &str,
    cx: &mut AsyncApp,
) {
    let draft = row["configuration"]["draft"].as_str().unwrap();
    if !draft.is_empty() {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let tab = &app.tabs[app.index().unwrap()];
                    tab.row_filter.update(cx, |state, cx| {
                        state.focus(window, cx);
                        state.replace_text_in_range(None, draft, window, cx);
                    });
                })
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(1100))
            .await;
    }
    cx.update_window(window.into(), |_, window, cx| window.blur(cx))
        .unwrap();
    move_pointer(window, point(px(1000.), px(700.)), cx);
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
    cx.update_window(window.into(), |_,window,cx| {
        for _ in 0..2 { window.refresh();let _=window.draw(cx); }
        assert!(gpui_base::test_support::find(window,&[],&"entry-match-tooltip".into()).is_none(), "control paint must exclude the tooltip shadow");
        let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
        assert_eq!(tab.row_match,row["fullMatch"].as_str().unwrap());
        assert_eq!(text_input(&tab.row_filter,cx),row["inputValue"].as_str().unwrap());
        assert!(!tab.row_filter.focus_handle(cx).is_focused(window));
        assert_eq!(!tab.row_match.is_empty() && draft.trim().is_empty(),row["searchDisabled"].as_bool().unwrap());
        let mut frames=Vec::new();
        for (id,native) in [("group","entry-filter-group"),("outer","entry-filter"),("search","entry-search"),("tag","entry-search-match")] {
            let b=gpui_base::test_support::find(window,&[],&native.into()).unwrap().bounds();
            let expected=row["styles"].as_array().unwrap().iter().find(|v| v["id"]==id).unwrap();
            let values=[b.origin.x.as_f32(),b.origin.y.as_f32(),b.size.width.as_f32(),b.size.height.as_f32()];
            for (key,actual) in ["x","y","w","h"].into_iter().zip(values) { assert!((actual-expected[key].as_f64().unwrap()as f32).abs()<0.6,"{scene} {id}.{key}: {actual} vs{}",expected[key]); }
            frames.push(serde_json::json!({"id":id,"x":values[0],"y":values[1],"w":values[2],"h":values[3]}));
        }
        println!("ENTRY CONTROL FRAME: {}",serde_json::json!({"scene":scene,"frames":frames,"match":tab.row_match,"input":text_input(&tab.row_filter,cx),"disabled":row["searchDisabled"],"tooltipVisible":false}));
    }).unwrap();
    if row["searchDisabled"] == true {
        super::qa_json::click("entry-search", window, cx);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert_eq!(tab.row_match, row["fullMatch"].as_str().unwrap());
                assert_eq!(text_input(&tab.row_filter, cx), draft);
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.blur(cx))
            .unwrap();
        move_pointer(window, point(px(1000.), px(700.)), cx);
    }
    println!(
        "ENTRY CONTROL QA PASS: {scene} actualEnter,matched/draft state,original rectangles,tooltip absent and disabled click inert"
    );
}

pub(super) async fn restore(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    move_pointer(window, point(px(1000.), px(700.)), cx);
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.tabs[i]
                    .match_tooltips
                    .cancel(super::match_tooltip::MatchKind::Key);
                app.tabs[i]
                    .match_tooltips
                    .cancel(super::match_tooltip::MatchKind::Entry);
                app.tabs[i].key_match.clear();
                app.tabs[i].key_filter.clear();
                app.tabs[i].row_match.clear();
                app.tabs[i].row_filter_value.clear();
                set_input(&app.tabs[i].filter, "", window, cx);
                set_input(&app.tabs[i].row_filter, "", window, cx);
                app.pref.general.theme = "light".into();
                app.pref.general.font_family.clear();
                app.pref.general.font_size = 14.;
                app.apply_theme(window, cx);
                let id = app.tabs[i].id;
                app.scan_keys(id, true, false, window, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
}
