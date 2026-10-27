//! JSON scenes use a second, explicitly marked disposable Redis with RedisJSON.
use super::*;

const INITIAL: &[u8] = r#"{"name":"中文","items":[1,true]}"#.as_bytes();
const DRAFT: &str = " { \"name\" : \"draft\", \"items\" : [2] } ";

pub(super) async fn idle(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..100 {
        if window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.busy == 0
                    && app
                        .index()
                        .is_none_or(|i| !app.tabs[i].values_loading && !app.tabs[i].value_saving && !app.tabs[i].clients.as_ref().is_some_and(|clients| clients.loading) && !app.tabs[i].key_types.in_flight)
            })
            .unwrap()
        {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("JSON QA did not settle");
}

pub(super) fn click(id: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let p = gpui_base::test_support::find(
            window,
            &[],
            &ElementId::from(SharedString::from(id.to_string())),
        )
        .unwrap()
        .bounds()
        .center();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: p,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: p,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
}

fn configurations() -> Vec<Connection> {
    let root = PathBuf::from(
        std::env::var_os("REDISDOCK_JSON_QA_DIR").expect("JSON UI QA requires REDISDOCK_JSON_QA_DIR"),
    );
    assert_eq!(
        std::fs::read_to_string(root.join("RDM-QA-FIXTURE")).unwrap(),
        "rust-rdm-render-fixture-v1\n"
    );
    let configs = Store::new(&root).connections().unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].network, "unix");
    assert_eq!(PathBuf::from(&configs[0].sock), root.join("redis.sock"));
    configs
}

pub(super) fn preflight() {
    let configs = configurations();
    let mut client = Client::connect(&configs[0], 0).expect("connect the isolated RedisJSON fixture before rendering");
    let command = client.query(&command(&["COMMAND", "INFO", "JSON.GET"]))
        .expect("read RedisJSON capability before rendering");
    assert!(command.array().unwrap().first().is_some_and(|value| !matches!(value, crate::redis::Value::Null)),
        "JSON QA fixture must expose JSON.GET before rendering any scenes");
    println!("JSON FIXTURE PREFLIGHT PASS: marked isolated Unix socket and RedisJSON command");
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Option<String> {
    idle(view, window, cx).await;
    let configs = configurations();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.service.connect(&configs[0], 0).unwrap();
                app.service
                    .create(
                        &configs[0],
                        0,
                        b"aahash",
                        KeyType::Json,
                        if scene == "json-scalar" {
                            b"123"
                        } else {
                            INITIAL
                        },
                        &[],
                        -1,
                    )
                    .unwrap();
                app.tabs[i].conn = configs[0].clone();
                app.tabs[i].current_key = None;
                app.pref.general.font_size = 14.;
                app.pref.general.font_family.clear();
                app.pref.general.font.clear();
                app.pref.editor.font_size = 14.;
                app.pref.editor.show_line_num = true;
                app.pref.general.theme = if scene == "json-dark" || scene == "json-syntax-dark" {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(window, cx);
                let id = app.tabs[i].id;
                app.close_footer_picker(id, window, cx);
                app.tabs[i].content_nav_offset.set(0.);
                app.tabs[i].content_nav_layout.set(None);
                app.select_key(id, b"aahash".to_vec(), window, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                assert_eq!(tab.detail.as_ref().unwrap().kind, KeyType::Json);
                assert_eq!(tab.detail.as_ref().unwrap().length, 0);
                assert_eq!(tab.decode, "None");
                assert_eq!(tab.format, "JSON");
                tab.detail.as_mut().unwrap().memory =
                    if scene == "json-memory-zero" { 0 } else { 64 };
            })
        })
        .unwrap();
    let mut copied = None;
    if scene.starts_with("json-syntax-") {
        let state = window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
            let i = app.index().unwrap();
            let state = app.tabs[i].editor.clone();
            state.update(cx, |state, cx| state.set_value(dialogs::json_diagnostics_qa::SYNTAX_TEXT, w, cx));
            let id = app.tabs[i].id;
            app.update_json_brackets(id, cx);
            app.update_scalar_diagnostics(id, w, cx);
            state
        })).unwrap();
        for attempt in 0..250 {
            let pending = window.update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.tabs[app.index().unwrap()].json_diagnostics.is_pending()
            }).unwrap();
            if !pending { break; }
            assert!(attempt != 249, "syntax scalar diagnostics did not settle");
            cx.background_executor().timer(Duration::from_millis(10)).await;
        }
        dialogs::json_diagnostics_qa::syntax_frame(view, window, &state, scene, cx).await;
        window.update(cx, |_, _, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            assert_eq!(app.service.detail(&tab.conn, 0, b"aahash", 0, "", 100, "", false).unwrap().value, INITIAL);
        }).unwrap();
        return None;
    }
    if scene == "json-object" {
        cx.update_window(window.into(), |_, window, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            tab.editor.focus_handle(cx).focus(window, cx);
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("cmd-s").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            let app = view.read(cx);
            assert!(
                app.tabs[app.index().unwrap()].value_saving,
                "unchanged JSON still saves through the editor shortcut"
            );
        })
        .unwrap();
        idle(view, window, cx).await;
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.messages.clear();
                    app.focus.focus(window, cx);
                })
            })
            .unwrap();
    }
    if matches!(
        scene,
        "json-draft" | "json-saved" | "json-save-error" | "json-saving"
    ) {
        let original = window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let i = app.index().unwrap();
                    let original = app.tabs[i].editor_original.clone();
                    app.tabs[i]
                        .editor
                        .update(cx, |editor, cx| editor.set_value(DRAFT, window, cx));
                    app.update_json_brackets(app.tabs[i].id, cx);
                    original
                })
            })
            .unwrap();
        if scene != "json-draft" {
            let previous=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
            click("copy-value", window, cx);
            let clipboard = window
                .update(cx, |_, _, cx| {
                    super::qa::copy_result(previous,cx)
                })
                .unwrap();
            assert!(clipboard.as_deref()==Some(original.as_str()),"copy must produce the saved JSON fixture");
            copied = Some(original);
            click("save-value", window, cx);
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    assert!(app.tabs[app.index().unwrap()].value_saving);
                })
                .unwrap();
            idle(view, window, cx).await;
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    let tab = &app.tabs[app.index().unwrap()];
                    assert_eq!(tab.editor_original, DRAFT);
                    assert_eq!(text_input(&tab.editor, cx), DRAFT);
                    assert_eq!(
                        app.service
                            .detail(&tab.conn, 0, b"aahash", 0, "", 100, "", false)
                            .unwrap()
                            .value,
                        br#"{"name":"draft","items":[2]}"#
                    );
                })
                .unwrap();
            if scene != "json-saved" {
                window
                    .update(cx, |_, window, cx| {
                        view.update(cx, |app, cx| {
                            let i = app.index().unwrap();
                            app.tabs[i]
                                .editor
                                .update(cx, |editor, cx| editor.set_value("{broken}", window, cx));
                            app.update_json_brackets(app.tabs[i].id, cx);
                        })
                    })
                    .unwrap();
                click("save-value", window, cx);
                idle(view, window, cx).await;
                window
                    .update(cx, |_, _, cx| {
                        let app = view.read(cx);
                        let tab = &app.tabs[app.index().unwrap()];
                        assert_eq!(text_input(&tab.editor, cx), "{broken}");
                        assert_eq!(tab.editor_original, DRAFT);
                        assert!(
                            app.messages
                                .last()
                                .unwrap()
                                .text
                                .contains("key must be a string")
                        );
                        assert_eq!(
                            app.service
                                .detail(&tab.conn, 0, b"aahash", 0, "", 100, "", false)
                                .unwrap()
                                .value,
                            br#"{"name":"draft","items":[2]}"#
                        );
                    })
                    .unwrap();
                if scene == "json-saving" {
                    window
                        .update(cx, |_, _, cx| {
                            view.update(cx, |app, _| {
                                let i = app.index().unwrap();
                                app.tabs[i].value_saving = true;
                            })
                        })
                        .unwrap();
                }
            }
        }
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let oracle: Json =
            serde_json::from_str(include_str!("../../tests/fixtures/json-source.json")).unwrap();
        let metrics = oracle["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["name"] == scene)
            .unwrap()["metrics"]
            .as_array()
            .unwrap();
        let has_class = |m: &Json, class: &str| {
            m["class"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .any(|part| part == class)
        };
        let check = |id: &str, source: &Json| {
            let bounds = gpui_base::test_support::find(
                window,
                &[],
                &ElementId::from(SharedString::from(id.to_string())),
            )
            .unwrap()
            .bounds();
            for (actual, key) in [
                (bounds.origin.x, "x"),
                (bounds.origin.y, "y"),
                (bounds.size.width, "w"),
                (bounds.size.height, "h"),
            ] {
                assert!(
                    (actual.as_f32() - source[key].as_f64().unwrap() as f32).abs() < 0.6,
                    "{scene} {id} {key}: {actual:?} vs {}",
                    source[key]
                );
            }
        };
        for (id, class) in [
            ("json-toolbar", "tb2"),
            ("json-value-wrapper", "value-wrapper"),
            ("json-editor", "editor-inst"),
            ("value-footer", "value-footer"),
        ] {
            check(id, metrics.iter().find(|m| has_class(m, class)).unwrap());
        }
        for (id, source) in ["copy-value", "save-value"]
            .into_iter()
            .zip(metrics.iter().filter(|m| has_class(m, "n-button")))
        {
            check(id, source);
        }
        if matches!(scene, "json-object" | "json-dark" | "json-scalar") {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            let caret = tab
                .editor
                .read(cx)
                .prepaint_caret_geometry()
                .expect("JSON editor geometry");
            let source = metrics.iter().find(|m| has_class(m, "view-line")).unwrap();
            assert!(
                (caret.line_height().as_f32() - source["h"].as_f64().unwrap() as f32).abs() < 0.6
            );
            assert!(
                (caret.cursor_bounds().origin.x.as_f32() - source["x"].as_f64().unwrap() as f32)
                    .abs()
                    < 0.6,
                "{scene} first code glyph: {:?} vs {}",
                caret.cursor_bounds(),
                source["x"]
            );
        }
        assert!(gpui_base::test_support::find(window, &[], &"footer-summary".into()).is_none());
        assert!(gpui_base::test_support::find(window, &[], &"footer-selectors".into()).is_none());
        assert_eq!(
            gpui_base::test_support::find(window, &[], &"footer-memory".into()).is_some(),
            scene != "json-memory-zero"
        );
    })
    .unwrap();
    println!("QA PASS: {scene} original JSON geometry and real-module save behavior");
    copied
}
