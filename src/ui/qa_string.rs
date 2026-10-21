use super::qa_json::{click, idle};
use super::*;

const DRAFT: &str = " { \"name\" : \"draft\", \"items\" : [2] } ";

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> (Option<String>, Vec<u8>) {
    idle(view, window, cx).await;
    let original = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let connection = app.tabs[i].conn.clone();
                let old = app
                    .service
                    .detail(&connection, 0, b"aastring", 0, "", 100, "", false)
                    .unwrap();
                assert_eq!(old.kind, KeyType::String);
                let bytes = if scene == "scalar-string-json" {
                    r#"{"name":"中文","items":[1,true]}"#.as_bytes()
                } else {
                    b"Hello Rust + GPUI"
                };
                app.service
                    .set_string(&connection, 0, b"aastring", bytes.to_vec(), false)
                    .unwrap();
                app.pref.general.font_size = 14.;
                app.pref.general.font_family.clear();
                app.pref.general.font.clear();
                app.pref.editor.font_size = 14.;
                app.pref.editor.font_family.clear();
                app.pref.editor.show_line_num = true;
                app.pref.general.theme = if scene == "scalar-string-dark" {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(window, cx);
                let id = app.tabs[i].id;
                app.close_footer_picker(id, window, cx);
                app.tabs[i].current_key = None;
                app.tabs[i].content_nav_offset.set(0.);
                app.tabs[i].content_nav_layout.set(None);
                app.select_key(id, b"aastring".to_vec(), window, cx);
                old.value
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                app.tabs[i].detail.as_mut().unwrap().memory =
                    if scene == "scalar-string-memory-zero" {
                        0
                    } else {
                        64
                    };
            })
        })
        .unwrap();
    if scene == "scalar-string-plain" {
        cx.update_window(window.into(), |_, window, cx| {
            let app = view.read(cx);
            app.tabs[app.index().unwrap()]
                .editor
                .focus_handle(cx)
                .focus(window, cx);
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("cmd-s").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            let app = view.read(cx);
            assert!(app.tabs[app.index().unwrap()].value_saving);
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
    let mut copied = None;
    if matches!(
        scene,
        "scalar-string-draft"
            | "scalar-string-saved"
            | "scalar-string-save-error"
            | "scalar-string-saving"
    ) {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    let i = app.index().unwrap();
                    app.tabs[i]
                        .editor
                        .update(cx, |editor, cx| editor.set_value(DRAFT, window, cx));
                    app.update_json_brackets(app.tabs[i].id, cx);
                    assert!(app.scalar_format_disabled(i, cx));
                })
            })
            .unwrap();
        for control in ["format", "decode"] {
            click(control, window, cx);
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    assert!(
                        app.tabs[app.index().unwrap()].footer_picker.is_none(),
                        "dirty String cannot change format or decoder"
                    );
                })
                .unwrap();
        }
        if scene == "scalar-string-draft" {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh(); let _ = window.draw(cx);
                let position = gpui_base::test_support::find(window, &[], &"format".into()).unwrap().bounds().center();
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent {position,..Default::default()}),cx);
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(250)).await;
            window.update(cx, |_, _, cx| {
                let app = view.read(cx); assert!(app.tabs[app.index().unwrap()].footer_tooltip.is_none(), "Source disabled format button does not show a tooltip");
            }).unwrap();
        }
        if scene != "scalar-string-draft" {
            let original=window.update(cx,|_,_,cx|cx.read_from_clipboard()).unwrap();
            click("copy-value", window, cx);
            let text = window
                .update(cx, |_, _, cx| {
                    super::qa::copy_result(original,cx)
                })
                .unwrap();
            assert!(text.as_deref()==Some("Hello Rust + GPUI"),"copy must produce the saved scalar fixture");
            copied = text;
            click("save-value", window, cx);
            idle(view, window, cx).await;
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    let i = app.index().unwrap();
                    let tab = &app.tabs[i];
                    assert_eq!(tab.editor_original, DRAFT);
                    assert_eq!(text_input(&tab.editor, cx), DRAFT);
                    assert_eq!(
                        tab.detail.as_ref().unwrap().length,
                        17,
                        "Source retains summary length until refresh"
                    );
                    assert_eq!(tab.detail.as_ref().unwrap().memory, 64);
                    assert!(!app.scalar_format_disabled(i, cx));
                    assert_eq!(
                        app.service
                            .detail(&tab.conn, 0, b"aastring", 0, "", 100, "", false)
                            .unwrap()
                            .value,
                        DRAFT.as_bytes()
                    );
                })
                .unwrap();
            if scene != "scalar-string-saved" {
                let root = window
                    .update(cx, |_, window, cx| {
                        view.update(cx, |app, cx| {
                            let i = app.index().unwrap();
                            let root = app.tabs[i].conn.clone();
                            app.service
                                .run(
                                    &root,
                                    0,
                                    command(&[
                                        "ACL",
                                        "SETUSER",
                                        "rdm-scalar-read-only",
                                        "reset",
                                        "on",
                                        ">rdm-scalar-fixture",
                                        "~*",
                                        "+@read",
                                        "+@connection",
                                        "+info",
                                    ]),
                                )
                                .unwrap();
                            let mut limited = root.clone();
                            limited.username = "rdm-scalar-read-only".into();
                            limited.password = "rdm-scalar-fixture".into();
                            app.service.connect(&limited, 0).unwrap();
                            app.tabs[i].conn = limited;
                            app.tabs[i]
                                .editor
                                .update(cx, |editor, cx| editor.set_value("{broken", window, cx));
                            root
                        })
                    })
                    .unwrap();
                click("save-value", window, cx);
                idle(view, window, cx).await;
                window
                    .update(cx, |_, _, cx| {
                        view.update(cx, |app, cx| {
                            let i = app.index().unwrap();
                            let tab = &app.tabs[i];
                            assert_eq!(tab.editor_original, DRAFT);
                            assert_eq!(text_input(&tab.editor, cx), "{broken");
                            assert!(app.messages.last().unwrap().text.contains("NOPERM"));
                            app.service.connect(&root, 0).unwrap();
                            app.tabs[i].conn = root.clone();
                            app.service
                                .run(
                                    &root,
                                    0,
                                    command(&["ACL", "DELUSER", "rdm-scalar-read-only"]),
                                )
                                .unwrap();
                            assert_eq!(
                                app.service
                                    .detail(&root, 0, b"aastring", 0, "", 100, "", false)
                                    .unwrap()
                                    .value,
                                DRAFT.as_bytes()
                            );
                            if scene == "scalar-string-saving" {
                                app.tabs[i].value_saving = true;
                            }
                        })
                    })
                    .unwrap();
            }
        }
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let oracle: Json = serde_json::from_str(include_str!(
            "../../tests/fixtures/scalar-string-source.json"
        ))
        .unwrap();
        let metrics = oracle["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["name"] == scene)
            .unwrap()["metrics"]
            .as_array()
            .unwrap();
        let has = |m: &Json, class: &str| {
            m["class"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .any(|part| part == class)
        };
        let check = |id: &str, m: &Json| {
            let b = gpui_base::test_support::find(
                window,
                &[],
                &ElementId::from(SharedString::from(id.to_string())),
            )
            .unwrap()
            .bounds();
            for (value, key) in [
                (b.origin.x, "x"),
                (b.origin.y, "y"),
                (b.size.width, "w"),
                (b.size.height, "h"),
            ] {
                assert!(
                    (value.as_f32() - m[key].as_f64().unwrap() as f32).abs() < 0.6,
                    "{scene} {id} {key}: {value:?} versus {}",
                    m[key]
                );
            }
        };
        for (id, class) in [
            ("string-toolbar", "tb2"),
            ("string-value-wrapper", "value-wrapper"),
            ("string-editor", "editor-inst"),
            ("value-footer", "value-footer"),
        ] {
            check(id, metrics.iter().find(|m| has(m, class)).unwrap());
        }
        for (id, m) in ["copy-value", "save-value"].into_iter().zip(
            metrics
                .iter()
                .filter(|m| has(m, "n-button") && m["y"].as_f64().unwrap() < 200.),
        ) {
            check(id, m);
        }
        let selectors = metrics
            .iter()
            .filter(|m| has(m, "n-button") && m["y"].as_f64().unwrap() > 700.);
        for (id, m) in ["format", "decode"].into_iter().zip(selectors) {
            check(id, m);
        }
        assert_eq!(
            gpui_base::test_support::find(window, &[], &"footer-memory".into()).is_some(),
            scene != "scalar-string-memory-zero"
        );
    })
    .unwrap();
    println!(
        "QA PASS: {scene} Source geometry, original-value copy, summary retention and actual Redis writes/errors"
    );
    (copied, original)
}
