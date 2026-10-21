//! The application's real JSON diagnostic callback and editor models, with
//! original Monaco samples and controlled in-flight worker completion.
use super::*;
use gpui_base::input::{DiagnosticSeverity, Position, RopeExt as _};
use std::sync::OnceLock;

const OLD: &str = "{\"rdm-diagnostic-qa-old\":}";
const NEW: &str = "{\"rdm-diagnostic-qa-new\":1}";
static CONTROL: OnceLock<Mutex<Option<mpsc::Sender<Pending>>>> = OnceLock::new();

struct Pending {
    text: String,
    release: mpsc::Sender<()>,
}

pub(in crate::ui) fn hold(text: &str) {
    if !matches!(text, OLD | NEW) {
        return;
    }
    let Some(sender) = CONTROL.get_or_init(|| Mutex::new(None)).lock().clone() else {
        return;
    };
    let (release, receiver) = mpsc::channel();
    if sender
        .send(Pending {
            text: text.into(),
            release,
        })
        .is_ok()
    {
        receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("release diagnostic QA worker");
    }
}

struct Control(mpsc::Receiver<Pending>);
impl Control {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        assert!(
            CONTROL
                .get_or_init(|| Mutex::new(None))
                .lock()
                .replace(sender)
                .is_none()
        );
        Self(receiver)
    }
    async fn next(&self, expected: &str, cx: &mut AsyncApp) -> Pending {
        for _ in 0..200 {
            if let Ok(pending) = self.0.try_recv() {
                assert_eq!(pending.text, expected);
                return pending;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        panic!("diagnostic worker did not start for {expected}");
    }
    fn empty(&self) {
        assert!(matches!(self.0.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        *CONTROL.get().unwrap().lock() = None;
    }
}

fn editor(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) -> Entity<EditorState> {
    window
        .update(cx, |_, _, cx| {
            view.read(cx).dialog.as_ref().unwrap().editors["value"].clone()
        })
        .unwrap()
}

fn set(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    text: &str,
    format: &str,
    fresh: bool,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let dialog = app.dialog.as_mut().unwrap();
                let state = dialog.editors["value"].clone();
                set_input(&dialog.fields["format"], format.to_owned(), w, cx);
                state.update(cx, |state, cx| {
                    state.set_value(text.to_owned(), w, cx);
                    state.set_highlighter(editor_language(format), cx);
                });
                let content = dialog.entry_content.as_mut().unwrap();
                content.editing = text.into();
                if fresh {
                    content.diagnostics = Default::default();
                }
                dialog.refresh_entry_json(w, cx);
            })
        })
        .unwrap();
}

pub(super) async fn idle(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..250 {
        if window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.busy == 0
                    && app
                        .dialog
                        .as_ref()
                        .and_then(|d| d.entry_content.as_ref())
                        .is_none_or(|c| !c.diagnostics.is_pending())
            })
            .unwrap()
        {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    panic!("JSON diagnostics did not settle");
}

fn markers(state: &EditorState) -> Vec<Json> {
    let position = |p: Position| {
        let line = state.text().slice_line(p.line as usize);
        let col: usize = line
            .chars()
            .take(p.character as usize)
            .map(char::len_utf16)
            .sum();
        (p.line + 1, col + 1)
    };
    let mut out:Vec<_>=state.diagnostics().unwrap().iter().map(|d|{
        let start=position(d.diagnostic.range.start);let end=position(d.diagnostic.range.end);
        serde_json::json!({"code":d.code,"message":d.message,"source":d.source,"severity":if d.severity==DiagnosticSeverity::Warning{4}else{8},"startLineNumber":start.0,"startColumn":start.1,"endLineNumber":end.0,"endColumn":end.1})
    }).collect();
    out.sort_by_key(Json::to_string);
    out
}

fn expected(record: &Json) -> Vec<Json> {
    let mut out: Vec<_> = record["markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|marker| {
            let mut object = serde_json::Map::new();
            for k in [
                "code",
                "message",
                "source",
                "severity",
                "startLineNumber",
                "startColumn",
                "endLineNumber",
                "endColumn",
            ] {
                object.insert(k.into(), marker[k].clone());
            }
            Json::Object(object)
        })
        .collect();
    out.sort_by_key(Json::to_string);
    out
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    super::new_key_qa::idle(view, window, cx).await;
    if scene.starts_with("entry-diagnostic-syntax-") {
        set(view, window, SYNTAX_TEXT, "JSON", true, cx);
        window.update(cx, |_, w, cx| view.update(cx, |app, cx| {
            app.pref.general.theme = if scene.ends_with("-dark") { "dark" } else { "light" }.into();
            app.apply_theme(w, cx);
        })).unwrap();
        idle(view, window, cx).await;
        syntax_frame(view, window, &editor(view, window, cx), scene, cx).await;
        return;
    }
    if scene == "entry-diagnostic-keyboard-show-hover" {
        keyboard_show_hover(view, window, cx).await;
        return;
    }
    if scene.starts_with("entry-diagnostic-keyboard-") {
        keyboard_selection(view, window, scene, cx).await;
        return;
    }
    if scene.starts_with("entry-diagnostic-save-") {
        save_from_message(view, window, scene, cx).await;
        return;
    }
    let state = editor(view, window, cx);
    if scene.starts_with("entry-diagnostic-select-") {
        selection(view, window, scene, cx).await;
        return;
    }
    if scene.starts_with("entry-diagnostic-hover-scroll-") {
        hover_scroll(view, window, scene, cx).await;
        return;
    }
    if scene.contains("-hover") || scene.contains("-navigation") {
        interaction(view, window, scene, cx).await;
        return;
    }
    if scene == "entry-diagnostic-editor" {
        let mut count = 0;
        for fixture in [
            include_str!("../../../tests/fixtures/entry-diagnostics-source.json"),
            include_str!("../../../tests/fixtures/json-diagnostics-extended-source.json"),
        ] {
            let fixture: Json = serde_json::from_str(fixture).unwrap();
            for record in fixture["records"].as_array().unwrap() {
                set(
                    view,
                    window,
                    record["value"].as_str().unwrap(),
                    "JSON",
                    true,
                    cx,
                );
                idle(view, window, cx).await;
                window
                    .update(cx, |_, _, cx| {
                        assert_eq!(
                            markers(state.read(cx)),
                            expected(record),
                            "actual editor diagnostics for {}",
                            record["input"]
                        )
                    })
                    .unwrap();
                count += 1;
            }
        }
        assert_eq!(count, 77);
        println!(
            "JSON DIAGNOSTICS QA PASS: 77 original samples through actual background validation and editor ranges, including duplicate warnings, CRLF and Unicode"
        );
        timing(view, window, cx).await;
        // Main String and RedisJSON editors share the controller, but their
        // language selection enters through the separate scalar callback.
        let (tab_id, saved_format, saved_kind, saved_text) = window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                (
                    tab.id,
                    tab.format.clone(),
                    tab.detail.as_ref().unwrap().kind.clone(),
                    text_input(&tab.editor, cx),
                )
            })
            .unwrap();
        for kind in [KeyType::String, KeyType::Json] {
            window
                .update(cx, |_, w, cx| {
                    view.update(cx, |app, cx| {
                        let i = app.tab_index(tab_id).unwrap();
                        let tab = &mut app.tabs[i];
                        tab.detail.as_mut().unwrap().kind = kind.clone();
                        tab.format = if kind == KeyType::Json { "Raw" } else { "JSON" }.into();
                        tab.json_diagnostics = Default::default();
                        tab.editor.update(cx, |editor, cx| {
                            editor.set_highlighter("json", cx);
                            editor.set_value("{\"🙂\":}", w, cx)
                        });
                        app.update_scalar_diagnostics(tab_id, w, cx);
                    })
                })
                .unwrap();
            for _ in 0..150 {
                if window
                    .update(cx, |_, _, cx| {
                        !view
                            .read(cx)
                            .tabs
                            .iter()
                            .find(|t| t.id == tab_id)
                            .unwrap()
                            .json_diagnostics
                            .is_pending()
                    })
                    .unwrap()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    let tab = &app.tabs[app.tab_index(tab_id).unwrap()];
                    assert!(!tab.json_diagnostics.is_pending());
                    let actual = markers(tab.editor.read(cx));
                    assert_eq!(actual.len(), 1);
                    assert_eq!(actual[0]["startColumn"], 7);
                    assert_eq!(actual[0]["code"], "516");
                })
                .unwrap();
        }
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let i = app.tab_index(tab_id).unwrap();
                    let tab = &mut app.tabs[i];
                    tab.format = saved_format;
                    tab.detail.as_mut().unwrap().kind = saved_kind;
                    tab.editor.update(cx, |editor, cx| {
                        editor.set_highlighter(editor_language(&tab.format), cx);
                        editor.set_value(saved_text, w, cx);
                    });
                    app.update_scalar_diagnostics(tab_id, w, cx);
                })
            })
            .unwrap();
        println!(
            "JSON DIAGNOSTICS SCALAR QA PASS: actual String and RedisJSON language-selection callbacks preserve Emoji columns"
        );
    }
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.pref.general.theme = if scene.ends_with("-dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(w, cx);
            })
        })
        .unwrap();
    let cases: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/json-diagnostic-render-source.json"
    ))
    .unwrap();
    let render_case = cases["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene);
    if let Some(record) = render_case {
        set(
            view,
            window,
            record["value"].as_str().unwrap(),
            "JSON",
            true,
            cx,
        );
        idle(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                assert_eq!(markers(state.read(cx)), expected(record), "{scene} markers")
            })
            .unwrap();
    } else {
        set(view, window, "", "JSON", true, cx);
        idle(view, window, cx).await;
        // Exercise the actual editor input subscription for the pictured draft.
        window
            .update(cx, |_, w, cx| {
                state.update(cx, |editor, cx| {
                    editor.focus(w, cx);
                    editor.replace_text_in_range(Some(0..0), "visible draft", w, cx);
                })
            })
            .unwrap();
        idle(view, window, cx).await;
    }
    window
        .update(cx, |_, w, cx| {
            let app = view.read(cx);
            let dialog = app.dialog.as_ref().unwrap();
            assert_eq!(
                dialog.value("value", cx),
                render_case.map_or("visible draft", |r| r["value"].as_str().unwrap())
            );
            assert!(app.busy == 0);
            let focus = app.focus.clone();
            focus.focus(w, cx);
        })
        .unwrap();
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        let _ = w.draw(cx);
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1.), px(1.)),
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    super::new_key_qa::draw(window, cx).await;
    println!(
        "JSON DIAGNOSTICS INPUT QA PASS: {scene} actual editor change starts validation and retains draft; no global loading indicator"
    );
}

pub(in crate::ui) const SYNTAX_TEXT: &str = "/* a { */\n{\"n\":true}";

pub(in crate::ui) async fn syntax_frame(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    state: &Entity<EditorState>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    let mut positions = Vec::new();
    for column in [5, 6, 0] {
        window.update(cx, |_, w, cx| {
            state.update(cx, |state, cx| state.set_cursor_position(Position::new(0, column), w, cx));
            let focus = view.read(cx).focus.clone();
            focus.focus(w, cx);
        }).unwrap();
        super::new_key_qa::draw(window, cx).await;
        positions.push(window.update(cx, |_, _, cx| {
            let state = state.read(cx);
            assert_eq!(state.language_name(), "json");
            assert_eq!(state.value().as_ref(), SYNTAX_TEXT);
            state.prepaint_caret_geometry().expect("fresh syntax caret geometry")
        }).unwrap());
    }
    let bounds = positions[2].input_bounds();
    let x = positions[0].cursor_bounds().origin.x.as_f32();
    let right = positions[1].cursor_bounds().origin.x.as_f32();
    assert!(right > x);
    let metadata = serde_json::json!({
        "scene": scene,
        "input": [bounds.origin.x.as_f32(), bounds.origin.y.as_f32(), bounds.size.width.as_f32(), bounds.size.height.as_f32()],
        "firstCodeX": positions[2].cursor_bounds().origin.x.as_f32(),
        "firstLineY": positions[2].cursor_bounds().origin.y.as_f32(),
        "lineHeight": positions[2].line_height().as_f32(),
        "commentBraceX": x,
        "commentBraceWidth": right - x,
        "dark": scene.ends_with("-dark"),
    });
    println!("QA SYNTAX FRAME: {metadata}");
    println!("QA PASS: {scene} real JSON editor model, decorations and unmodified comment/keyword draft");
}

fn pointer(position: Point<Pixels>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, w, cx| {
            w.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
}

fn diagnostic_key(key: &str, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .unwrap();
}

fn element(
    id: &str,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) -> Option<gpui_base::test_support::ElementSnapshot> {
    window
        .update(cx, |_, w, _| {
            gpui_base::test_support::find(w, &[], &id.to_owned().into())
        })
        .unwrap()
}

async fn hover_delay(
    phase: &str,
    before_visible: bool,
    trigger: Point<Pixels>,
    reset: Point<Pixels>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::draw;
    let delay = Duration::from_millis(300);
    for attempt in 1..=3 {
        let started = std::time::Instant::now();
        pointer(trigger, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        // Sample in one synchronous frame. The general draw helper also waits
        // 40ms and draws again, so its return time is not a 100ms observation.
        let visible = cx
            .update_window(window.into(), |_, w, cx| {
                w.refresh();
                let _ = w.draw(cx);
                gpui_base::test_support::find(w, &[], &"diagnostic-hover-card".into()).is_some()
            })
            .unwrap();
        let elapsed = started.elapsed();
        if elapsed < delay {
            assert_eq!(
                visible, before_visible,
                "{phase}: changed before 300ms at {elapsed:?}"
            );
            cx.background_executor()
                .timer(Duration::from_millis(340).saturating_sub(elapsed))
                .await;
            draw(window, cx).await;
            assert_eq!(
                element("diagnostic-hover-card", window, cx).is_some(),
                !before_visible,
                "{phase}: did not change after 300ms"
            );
            println!(
                "DIAGNOSTIC DELAY QA PASS: {phase} pre-deadline sample={elapsed:?}, attempt={attempt}; 300ms contract unchanged"
            );
            return;
        }
        // An overslept sample cannot establish an early transition. Retry only
        // this inconclusive observation; an actual pre-deadline failure above
        // never retries or becomes a pass.
        println!(
            "DIAGNOSTIC DELAY QA INCONCLUSIVE: {phase} sample={elapsed:?} exceeds 300ms, attempt={attempt}"
        );
        pointer(reset, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        assert_eq!(
            element("diagnostic-hover-card", window, cx).is_some(),
            before_visible,
            "{phase}: reset before retry"
        );
    }
    panic!(
        "{phase}: scheduler did not provide a sample before the 300ms deadline in three attempts"
    );
}

async fn interaction(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::{click, draw};
    let navigation = scene.contains("-navigation");
    let source_name = match scene {
        "entry-diagnostic-hover" => "json-diagnostic-hover",
        "entry-diagnostic-hover-dark" => "json-diagnostic-hover-dark",
        "entry-diagnostic-hover-overlap" => "json-diagnostic-hover-overlap",
        "entry-diagnostic-hover-overlap-dark" => "json-diagnostic-hover-overlap-dark",
        "entry-diagnostic-hover-full" => "json-hover-full",
        "entry-diagnostic-hover-short" => "json-hover-short",
        "entry-diagnostic-hover-duplicate" => "json-hover-warning",
        "entry-diagnostic-hover-overlap-full" => "json-hover-overlap-full",
        "entry-diagnostic-navigation" => "json-diagnostic-navigation",
        "entry-diagnostic-navigation-dark" => "json-diagnostic-navigation-dark",
        _ => unreachable!(),
    };
    let fixtures = [
        include_str!("../../../tests/fixtures/json-diagnostic-hover-source.json"),
        include_str!("../../../tests/fixtures/json-diagnostic-hover-interaction-source.json"),
        include_str!("../../../tests/fixtures/json-hover-layout-source.json"),
    ];
    let source = fixtures
        .into_iter()
        .flat_map(|fixture| {
            serde_json::from_str::<Json>(fixture).unwrap()["records"]
                .as_array()
                .unwrap()
                .clone()
        })
        .find(|record| record["name"] == source_name)
        .unwrap();
    let value = source["value"]
        .as_str()
        .unwrap_or(if scene.contains("overlap") || navigation {
            "/*"
        } else {
            "visible draft"
        });
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.pref.general.theme = if scene.ends_with("-dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(w, cx);
                app.full_editor = source["full"].as_bool().unwrap_or(false);
            })
        })
        .unwrap();
    set(view, window, value, "JSON", true, cx);
    idle(view, window, cx).await;
    draw(window, cx).await;
    let editor = editor(view, window, cx);
    let anchor = window
        .update(cx, |_, _, cx| {
            let state = editor.read(cx);
            let marker = state.diagnostics().unwrap().iter().next().unwrap();
            let bounds = state
                .range_to_bounds(&(marker.range.start..marker.range.start))
                .unwrap();
            bounds.origin + point(px(3.), bounds.size.height / 2.)
        })
        .unwrap();
    hover_delay("show", false, anchor, point(px(2.), px(2.)), window, cx).await;
    let hover =
        element("diagnostic-hover-card", window, cx).expect("real diagnostic pointer hover");
    assert_eq!(
        hover.label().unwrap().lines().count(),
        if scene.contains("overlap") || navigation {
            2
        } else {
            1
        }
    );
    if navigation {
        // The footer opens a real view zone; navigation is checked through the
        // rendered accessibility label, cursor, buffer and actual controls.
        click("diagnostic-view-problem".into(), window, cx);
        draw(window, cx).await;
        assert!(element("diagnostic-hover-card", window, cx).is_none());
        let checks: Json = serde_json::from_str(include_str!(
            "../../../tests/fixtures/json-navigation-behavior-source.json"
        ))
        .unwrap();
        let records = checks["events"].as_array().unwrap();
        for name in [
            "keyboard-open",
            "next",
            "next-wrap",
            "previous-wrap",
            "escape-close",
            "keyboard-reopen",
            "edited-before-validation",
            "edited-valid-stale",
            "next-without-markers",
            "final-close",
        ] {
            match name {
                "next" | "next-wrap" => click("diagnostic-next".into(), window, cx),
                "previous-wrap" => diagnostic_key("alt-shift-f8", window, cx),
                "escape-close" | "final-close" => diagnostic_key("escape", window, cx),
                "keyboard-reopen" | "next-without-markers" => diagnostic_key("alt-f8", window, cx),
                "edited-before-validation" => {
                    window
                        .update(cx, |_, w, cx| {
                            editor.update(cx, |state, cx| {
                                state.select_all(w, cx);
                                state.replace_text_in_range(None, "null", w, cx);
                            })
                        })
                        .unwrap();
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
                "edited-valid-stale" => idle(view, window, cx).await,
                _ => {}
            }
            draw(window, cx).await;
            let expected = records
                .iter()
                .find(|record| record["name"] == name)
                .unwrap();
            let actual = element("diagnostic-navigation-zone", window, cx);
            assert_eq!(
                actual.is_some(),
                expected["visible"].as_bool().unwrap(),
                "{name} visibility"
            );
            if let Some(actual) = actual {
                assert_eq!(
                    actual.label().unwrap(),
                    expected["text"].as_str().unwrap(),
                    "{name} text"
                );
            }
            window
                .update(cx, |_, _, cx| {
                    let state = editor.read(cx);
                    let position = state.cursor_position();
                    assert_eq!(
                        position.line + 1,
                        expected["position"]["lineNumber"].as_u64().unwrap() as u32,
                        "{name} row"
                    );
                    assert_eq!(
                        position.character + 1,
                        expected["position"]["column"].as_u64().unwrap() as u32,
                        "{name} column"
                    );
                    assert!(
                        state.selected_range().is_empty(),
                        "navigation collapses the selection"
                    );
                })
                .unwrap();
        }
        // Recreate the original first marker for the paired frame.
        set(view, window, value, "JSON", true, cx);
        idle(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                editor.update(cx, |state, cx| {
                    state.set_selected_range(0..0, cx);
                    state.focus(w, cx);
                })
            })
            .unwrap();
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
    } else {
        // Sticky popup handoff, then the original 300ms delayed dismissal.
        pointer(hover.bounds().center(), window, cx);
        draw(window, cx).await;
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        assert!(
            element("diagnostic-hover-card", window, cx).is_some(),
            "hover survives editor-to-popup handoff"
        );
        hover_delay("hide", true, point(px(2.), px(2.)), anchor, window, cx).await;
        pointer(anchor, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(340))
            .await;
        draw(window, cx).await;
    }
    let id = if navigation {
        "diagnostic-navigation-zone"
    } else {
        "diagnostic-hover-card"
    };
    let actual = element(id, window, cx).unwrap().bounds();
    window.update(cx,|_,w,cx|println!("DIAGNOSTIC SURFACE GEOMETRY: {scene} card={actual:?} input={:?} editor={:?} border={:?}",editor.read(cx).input_bounds(),gpui_base::test_support::find(w,&[],&"entry-value-editor".into()).map(|el|el.bounds()),gpui_base::test_support::find(w,&[],&"entry-editor-border".into()).map(|el|el.bounds()))).unwrap();
    let expected = source
        .get("hover")
        .or_else(|| source.get("bounds"))
        .unwrap();
    for (value, key) in [
        (actual.left(), "x"),
        (actual.top(), "y"),
        (actual.size.width, "width"),
        (actual.size.height, "height"),
    ] {
        assert!(
            (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
            "{scene} {key}: {value:?} vs {}",
            expected[key]
        );
    }
    println!(
        "JSON DIAGNOSTIC SURFACE QA PASS: {scene} original layout, delayed/sticky pointer hover or actual footer, Alt+F8, next/previous wrap, stale retention and Escape"
    );
}

async fn timing(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let state = editor(view, window, cx);
    let control = Control::new();
    set(view, window, OLD, "JSON", true, cx);
    let old = control.next(OLD, cx).await;
    set(view, window, NEW, "JSON", false, cx);
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    control.empty();
    let new = control.next(NEW, cx).await;
    old.release.send(()).unwrap();
    for _ in 0..100 {
        if window
            .update(cx, |_, _, cx| {
                state.read(cx).diagnostics().unwrap().len() == 1
            })
            .unwrap()
        {
            break;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    window
        .update(cx, |_, _, cx| {
            assert_eq!(state.read(cx).value().as_str(), NEW);
            assert_eq!(
                markers(state.read(cx))[0]["code"],
                "516",
                "an older in-flight reply must still apply"
            );
        })
        .unwrap();
    new.release.send(()).unwrap();
    idle(view, window, cx).await;
    assert!(
        window
            .update(cx, |_, _, cx| markers(state.read(cx)).is_empty())
            .unwrap()
    );
    set(view, window, NEW, "Unicode JSON", false, cx);
    control.empty();
    assert!(
        window
            .update(cx, |_, _, cx| !view
                .read(cx)
                .dialog
                .as_ref()
                .unwrap()
                .entry_content
                .as_ref()
                .unwrap()
                .diagnostics
                .is_pending())
            .unwrap()
    );
    set(view, window, OLD, "JSON", false, cx);
    let running = control.next(OLD, cx).await;
    set(view, window, OLD, "Raw", false, cx);
    running.release.send(()).unwrap();
    idle(view, window, cx).await;
    assert!(
        window
            .update(cx, |_, _, cx| markers(state.read(cx)).is_empty())
            .unwrap()
    );
    // A language switch preserves a pending old listener timeout. When it
    // fires, it validates the then-current model and may complete in JSON.
    set(view, window, "{}", "JSON", true, cx);
    idle(view, window, cx).await;
    set(view, window, OLD, "JSON", false, cx);
    set(view, window, NEW, "Raw", false, cx);
    set(view, window, NEW, "JSON", false, cx);
    let immediate = control.next(NEW, cx).await;
    let delayed = control.next(NEW, cx).await;
    immediate.release.send(()).unwrap();
    delayed.release.send(()).unwrap();
    idle(view, window, cx).await;
    // Ordinary consecutive edits cancel only the debounce that has not begun.
    set(view, window, OLD, "JSON", false, cx);
    set(view, window, NEW, "JSON", false, cx);
    let final_edit = control.next(NEW, cx).await;
    control.empty();
    final_edit.release.send(()).unwrap();
    idle(view, window, cx).await;
    println!(
        "JSON DIAGNOSTICS TIMING QA PASS: 500ms debounce, running stale reply, same JSON language, Raw suppression, surviving language-switch timeout reads current model, latest unscheduled edit"
    );
}

async fn hover_scroll(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::draw;
    let fixture: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/json-diagnostic-scroll-source.json"
    ))
    .unwrap();
    let record = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == scene)
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.pref.general.theme = "light".into();
                app.full_editor = false;
                app.apply_theme(w, cx);
            })
        })
        .unwrap();
    set(view, window, "/*", "JSON", true, cx);
    idle(view, window, cx).await;
    let editor = editor(view, window, cx);
    window
        .update(cx, |_, w, cx| {
            editor.update(cx, |state, cx| {
                let diagnostics = state.diagnostics_mut().unwrap();
                diagnostics.clear();
                diagnostics.push(
                    gpui_base::input::Diagnostic::new(
                        Position::new(0, 0)..Position::new(0, 2),
                        fixture["message"].as_str().unwrap().to_owned(),
                    )
                    .with_severity(DiagnosticSeverity::Error)
                    .with_source("json")
                    .with_code("777"),
                );
                state.diagnostics_did_change(cx);
                state.focus(w, cx);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx).await;
    let anchor = window
        .update(cx, |_, _, cx| {
            editor.read(cx).range_to_bounds(&(0..0)).unwrap().origin + point(px(3.), px(10.5))
        })
        .unwrap();
    pointer(anchor, window, cx);
    cx.background_executor()
        .timer(Duration::from_millis(350))
        .await;
    draw(window, cx).await;
    let card = element("diagnostic-hover-card", window, cx)
        .expect("controlled long marker opens on real hover");
    let position = card.bounds().center();
    pointer(position, window, cx);
    draw(window, cx).await;
    let wheel = |delta, cx: &mut AsyncApp| {
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            )
        })
        .unwrap();
    };
    if !scene.ends_with("-top") {
        wheel(-25., cx);
        draw(window, cx).await;
    } else {
        wheel(25., cx);
        draw(window, cx).await;
    }
    if scene.ends_with("-bottom") || scene.ends_with("-reopen") {
        for _ in 0..48 {
            wheel(-25., cx);
            draw(window, cx).await;
        }
        // A wheel event beyond the end is consumed by the hover, leaving the
        // underlying editor's scroll position and draft untouched.
        wheel(-25., cx);
        draw(window, cx).await;
    }
    if scene.ends_with("-reopen") {
        pointer(point(px(1.), px(1.)), window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        assert!(element("diagnostic-hover-card", window, cx).is_none());
        pointer(anchor, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        assert!(element("diagnostic-hover-card", window, cx).is_some());
        pointer(position, window, cx);
        draw(window, cx).await;
        let track = element("diagnostic-hover-scrollbar", window, cx)
            .unwrap()
            .bounds();
        let start = track.center();
        pointer(start, window, cx);
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            )
        })
        .unwrap();
        draw(window, cx).await;
        let viewport = element("diagnostic-hover-scroll", window, cx)
            .unwrap()
            .bounds();
        let content = element("diagnostic-hover-content", window, cx)
            .unwrap()
            .bounds();
        assert_eq!(
            viewport.top() - content.top(),
            px(fixture["events"][0]["scrollTop"].as_f64().unwrap() as f32),
            "source track-centering click"
        );
        let outside = start + point(px(-50.), px(500.));
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: outside,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            )
        })
        .unwrap();
        draw(window, cx).await;
        assert!(
            element("diagnostic-hover-card", window, cx).is_some(),
            "dragging outside retains the hover"
        );
        let content = element("diagnostic-hover-content", window, cx)
            .unwrap()
            .bounds();
        assert_eq!(
            viewport.top() - content.top(),
            px(fixture["events"][1]["scrollTop"].as_f64().unwrap() as f32)
        );
        cx.update_window(window.into(), |_, w, cx| {
            w.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: outside,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            )
        })
        .unwrap();
        draw(window, cx).await;
        assert!(
            element("diagnostic-hover-card", window, cx).is_none(),
            "source closes after releasing outside"
        );
        pointer(anchor, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        assert!(element("diagnostic-hover-card", window, cx).is_some());
        pointer(position, window, cx);
        draw(window, cx).await;
        println!(
            "JSON DIAGNOSTIC HOVER DRAG QA PASS: track centers at 521px, dragging outside clamps at 1041px, outside release closes and reopening preserves position"
        );
    }
    let metrics = record["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["class"] == "monaco-hover-content")
        .unwrap();
    let viewport = element("diagnostic-hover-scroll", window, cx)
        .unwrap()
        .bounds();
    let content = element("diagnostic-hover-content", window, cx)
        .unwrap()
        .bounds();
    let actual = viewport.top() - content.top();
    let expected = px(metrics["scrollTop"].as_f64().unwrap() as f32);
    println!(
        "DIAGNOSTIC HOVER SCROLL: {scene} offset={actual} expected={expected}, content={} viewport={}",
        content.size.height, viewport.size.height
    );
    assert!(
        (actual - expected).abs() <= px(0.5),
        "{scene} source scroll offset"
    );
    assert!(
        (viewport.size.height - px(metrics["clientHeight"].as_f64().unwrap() as f32)).abs()
            <= px(0.5)
    );
    assert!(
        (content.size.height - px(metrics["scrollHeight"].as_f64().unwrap() as f32)).abs()
            <= px(0.5)
    );
    window
        .update(cx, |_, _, cx| {
            assert_eq!(editor.read(cx).text().to_string(), "/*");
            assert_eq!(editor.read(cx).scroll_offset().y, px(0.));
        })
        .unwrap();
    println!(
        "JSON DIAGNOSTIC HOVER SCROLL QA PASS: {scene} actual wheel events, bounded viewport/content, source position and unchanged editor"
    );
}

async fn save_from_message(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::draw;
    let source: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/json-diagnostic-keyboard-source.json"
    ))
    .unwrap();
    let value = source["expectedSavedRequest"]["value"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(value, "/*");
    assert_eq!(
        source["wkWebView"]["records"][6]["saveCalls"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (service, connection) = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            (
                app.service.clone(),
                app.tabs[app.index().unwrap()].conn.clone(),
            )
        })
        .unwrap();
    let folder = std::path::Path::new(&connection.sock).parent().unwrap();
    assert_eq!(
        std::fs::read_to_string(folder.join("RDM-QA-FIXTURE")).unwrap(),
        "rust-rdm-render-fixture-v1\n"
    );
    let key = "diagnostic-command-uiqa:target";
    let test_service = service.clone();
    let test_connection = connection.clone();
    let detail = cx
        .background_executor()
        .spawn(async move {
            assert_eq!(
                test_service
                    .run(&test_connection, 15, command(&["DBSIZE"]))
                    .unwrap()
                    .integer()
                    .unwrap(),
                0
            );
            test_service
                .run(
                    &test_connection,
                    15,
                    command(&["HSET", key, "old", "before", "other", "kept"]),
                )
                .unwrap();
            test_service
                .detail(&test_connection, 15, key.as_bytes(), 0, "", 100, "", false)
                .unwrap()
        })
        .await;
    let row = detail
        .rows
        .iter()
        .find(|row| row.id == b"old")
        .unwrap()
        .clone();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                let i = app.index().unwrap();
                app.tabs[i].db = 15;
                app.tabs[i].current_key = Some(key.as_bytes().to_vec());
                app.tabs[i].detail = Some(detail);
                app.tabs[i].format = "Raw".into();
                app.tabs[i].decode = "None".into();
                app.open_dialog(DialogKind::EditEntry(row), w, cx);
            })
        })
        .unwrap();
    super::new_key_qa::idle(view, window, cx).await;
    set(view, window, &value, "JSON", true, cx);
    idle(view, window, cx).await;
    let state = editor(view, window, cx);
    window
        .update(cx, |_, w, cx| {
            state.update(cx, |state, cx| state.focus(w, cx))
        })
        .unwrap();
    draw(window, cx).await;
    if scene.ends_with("navigation") {
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
        assert!(element("diagnostic-navigation-zone", window, cx).is_some());
    } else {
        let anchor = window
            .update(cx, |_, _, cx| {
                state.read(cx).range_to_bounds(&(0..0)).unwrap().center() + point(px(3.), px(0.))
            })
            .unwrap();
        pointer(anchor, window, cx);
        for _ in 0..20 {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            draw(window, cx).await;
            if element("diagnostic-hover-card", window, cx).is_some() {
                break;
            }
        }
        assert!(element("diagnostic-hover-card", window, cx).is_some());
    }
    let footer = scene.ends_with("footer");
    if footer {
        let fixture: Json = serde_json::from_str(include_str!(
            "../../../tests/fixtures/json-diagnostic-footer-source.json"
        ))
        .unwrap();
        assert_eq!(fixture["save"]["expectedRequest"]["value"], value);
        let (bounds, width) = window
            .update(cx, |_, w, cx| {
                let order = state.read(cx).diagnostics().unwrap().iter().count() + 1;
                let bounds = gpui_base::test_support::find(
                    w,
                    &[],
                    &("diagnostic-message-layout", order).into(),
                )
                .unwrap()
                .bounds();
                let font = gpui::font(".SystemUIFont");
                let width = w
                    .text_system()
                    .shape_line(
                        "No".into(),
                        px(12.),
                        &[TextRun {
                            len: 2,
                            font,
                            ..Default::default()
                        }],
                        None,
                    )
                    .width;
                (bounds, width)
            })
            .unwrap();
        let start = bounds.origin + point(px(0.2), px(11.));
        let end = bounds.origin + point(width, px(11.));
        selection_gesture(&serde_json::json!({"kind":"drag", "start":{"x":f32::from(start.x),"y":f32::from(start.y)}, "end":{"x":f32::from(end.x),"y":f32::from(end.y)}}), window, cx).await;
        draw(window, cx).await;
        for (key, expected) in [
            (None, "No"),
            (Some("cmd-a"), "No"),
            (Some("shift-right"), "No "),
        ] {
            if let Some(key) = key {
                diagnostic_key(key, window, cx);
                draw(window, cx).await;
            }
            window
                .update(cx, |_, w, cx| {
                    assert_eq!(gpui_base::TextSelection::selected_text(w, cx), expected);
                    assert!(!state.focus_handle(cx).is_focused(w));
                })
                .unwrap();
        }
    } else {
        let message = window
            .update(cx, |_, w, _| {
                gpui_base::test_support::find(w, &[], &("diagnostic-message-layout", 1usize).into())
                    .expect("first diagnostic message")
                    .bounds()
            })
            .unwrap();
        let position = message.origin + point(px(25.), px(10.5));
        selection_gesture(&serde_json::json!({"kind":"double-click","position":{"x":f32::from(position.x),"y":f32::from(position.y)}}), window, cx).await;
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                assert_eq!(gpui_base::TextSelection::selected_text(w, cx), "Unexpected");
                assert!(!state.focus_handle(cx).is_focused(w));
            })
            .unwrap();
    }
    diagnostic_key("cmd-s", window, cx);
    super::new_key_qa::idle(view, window, cx).await;
    let closed = window
        .update(cx, |_, _, cx| view.read(cx).dialog.is_none())
        .unwrap();
    // Restore the disposable scratch database before checking the result, even
    // if the command regresses and leaves the original value in place.
    let saved = cx
        .background_executor()
        .spawn(async move {
            let saved = service
                .run(&connection, 15, command(&["HGET", key, "old"]))
                .unwrap()
                .bytes();
            let other = service
                .run(&connection, 15, command(&["HGET", key, "other"]))
                .unwrap()
                .bytes();
            service
                .run(&connection, 15, command(&["DEL", key]))
                .unwrap();
            assert_eq!(
                service
                    .run(&connection, 15, command(&["DBSIZE"]))
                    .unwrap()
                    .integer()
                    .unwrap(),
                0
            );
            assert_eq!(other, b"kept");
            saved
        })
        .await;
    assert_eq!(
        saved,
        value.as_bytes(),
        "{scene}: Command+S from a diagnostic owns the editor's save command"
    );
    assert!(
        closed,
        "{scene}: unpinned entry closes after its save command"
    );
    println!(
        "JSON DIAGNOSTIC SAVE QA PASS: {scene} actual diagnostic selection and Command+S saved the owning editor, closed the entry and restored DB15"
    );
}

async fn keyboard_show_hover(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    use super::new_key_qa::draw;
    let fixture: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/json-diagnostic-footer-source.json"
    ))
    .unwrap();
    let records = fixture["showCommand"].as_array().unwrap();
    let value = fixture["showCommandModelValue"].as_str().unwrap();
    set(view, window, value, "JSON", true, cx);
    idle(view, window, cx).await;
    let editor = editor(view, window, cx);
    window
        .update(cx, |_, w, cx| {
            editor.update(cx, |state, cx| {
                state.set_selected_range(5..5, cx);
                state.focus(w, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    for record in records {
        let name = record["name"].as_str().unwrap();
        if let Some(key) = record["key"].as_str() {
            diagnostic_key(key, window, cx);
            draw(window, cx).await;
        }
        let hover = element("diagnostic-hover-card", window, cx);
        assert_eq!(
            hover.is_some(),
            record["hover"].as_bool().unwrap(),
            "show/focus hover: {name}"
        );
        window
            .update(cx, |_, w, cx| {
                assert_eq!(editor.read(cx).value().as_str(), value);
                assert_eq!(
                    editor.focus_handle(cx).is_focused(w),
                    record["editorHasTextFocus"].as_bool().unwrap(),
                    "show/focus hover: {name}"
                );
                assert!(view.read(cx).dialog.is_some());
            })
            .unwrap();
        if let Some(hover) = hover {
            let actual = hover.bounds();
            let expected = &record["hoverBox"];
            for (value, key) in [
                (actual.left(), "x"),
                (actual.top(), "y"),
                (actual.size.width, "width"),
                (actual.size.height, "height"),
            ] {
                assert!(
                    (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
                    "show/focus hover: {name} {key}: {value:?} vs {}",
                    expected[key]
                );
            }
        }
    }
    println!(
        "JSON DIAGNOSTIC SHOW HOVER QA PASS: 6 source states, actual command chord shows then focuses, Escape returns to editor; source dimensions and unchanged buffer"
    );
}

async fn keyboard_selection(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::draw;
    let footer_case = match scene {
        "entry-diagnostic-keyboard-footer-character" => Some("character"),
        "entry-diagnostic-keyboard-footer-word" => Some("word"),
        "entry-diagnostic-keyboard-footer-boundaries" => Some("boundaries"),
        "entry-diagnostic-keyboard-crossmessage" => Some("crossmessage"),
        _ => None,
    };
    let fixture: Json = serde_json::from_str(if footer_case.is_some() {
        include_str!("../../../tests/fixtures/json-diagnostic-footer-source.json")
    } else {
        include_str!("../../../tests/fixtures/json-diagnostic-keyboard-boundaries-source.json")
    })
    .unwrap();
    let records = if let Some(case) = footer_case {
        fixture["sequences"][case].as_array().unwrap()
    } else {
        fixture[match scene {
            "entry-diagnostic-keyboard-hover" => "hoverRetreat",
            "entry-diagnostic-keyboard-navigation" => "navigation",
            "entry-diagnostic-keyboard-lines" => "hoverLinesAndWords",
            _ => unreachable!(),
        }]
        .as_array()
        .unwrap()
    };
    set(
        view,
        window,
        fixture["modelValue"].as_str().unwrap(),
        "JSON",
        true,
        cx,
    );
    idle(view, window, cx).await;
    let editor = editor(view, window, cx);
    window
        .update(cx, |_, w, cx| {
            editor.update(cx, |state, cx| {
                let diagnostics = state.diagnostics_mut().unwrap();
                diagnostics.clear();
                diagnostics.push(
                    gpui_base::input::Diagnostic::new(
                        Position::new(0, 0)..Position::new(0, 2),
                        fixture["message"].as_str().unwrap().to_owned(),
                    )
                    .with_severity(DiagnosticSeverity::Error)
                    .with_source("json")
                    .with_code("777"),
                );
                if footer_case == Some("crossmessage") {
                    diagnostics.push(
                        gpui_base::input::Diagnostic::new(
                            Position::new(0, 0)..Position::new(0, 2),
                            fixture["crossMessage"].as_str().unwrap().to_owned(),
                        )
                        .with_severity(DiagnosticSeverity::Error)
                        .with_source("json")
                        .with_code("778"),
                    );
                }
                state.diagnostics_did_change(cx);
                state.focus(w, cx);
                cx.notify();
            })
        })
        .unwrap();
    draw(window, cx).await;
    let navigation = scene.ends_with("navigation");
    if navigation {
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
        assert!(element("diagnostic-navigation-zone", window, cx).is_some());
    } else if footer_case.is_some() {
        window
            .update(cx, |_, _, cx| {
                editor.update(cx, |state, cx| state.set_selected_range(0..0, cx))
            })
            .unwrap();
        diagnostic_key("cmd-k", window, cx);
        diagnostic_key("cmd-i", window, cx);
        draw(window, cx).await;
        assert!(
            element("diagnostic-hover-card", window, cx).is_some(),
            "source keyboard-opened hover"
        );
    } else {
        let anchor = window
            .update(cx, |_, _, cx| {
                editor.read(cx).range_to_bounds(&(0..0)).unwrap().center() + point(px(3.), px(0.))
            })
            .unwrap();
        pointer(anchor, window, cx);
        for _ in 0..20 {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            draw(window, cx).await;
            if element("diagnostic-hover-card", window, cx).is_some() {
                break;
            }
        }
        assert!(element("diagnostic-hover-card", window, cx).is_some());
    }
    let (start, end) = window
        .update(cx, |_, w, cx| {
            let bounds = gpui_base::test_support::find(
                w,
                &[],
                &("diagnostic-message-layout", 1usize).into(),
            )
            .expect("controlled message")
            .bounds();
            let (font, font_size) = editor.read(cx).editor_font().unwrap_or_else(|| {
                (
                    gpui::font(cx.theme().mono_font_family.clone()),
                    cx.theme().mono_font_size,
                )
            });
            let prefix = records[0]["selected"].as_str().unwrap();
            let width = w
                .text_system()
                .shape_line(
                    prefix.to_owned().into(),
                    font_size,
                    &[TextRun {
                        len: prefix.len(),
                        font,
                        color: rgb(0).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width;
            (
                bounds.origin + point(px(0.2), px(10.5)),
                bounds.origin + point(width, px(10.5)),
            )
        })
        .unwrap();
    selection_gesture(
        &serde_json::json!({
            "kind":"drag", "start":{"x":f32::from(start.x),"y":f32::from(start.y)},
            "end":{"x":f32::from(end.x),"y":f32::from(end.y)},
        }),
        window,
        cx,
    )
    .await;
    draw(window, cx).await;
    let initial_top = element("diagnostic-hover-card", window, cx).map(|card| {
        window
            .update(cx, |_, w, _| {
                gpui_base::test_support::find(w, &[], &("diagnostic-message-layout", 1usize).into())
                    .unwrap()
                    .bounds()
                    .top()
                    - card.bounds().top()
            })
            .unwrap()
    });
    let mut copies = 0;
    for record in records {
        let name = record["name"].as_str().unwrap();
        if let Some(key) = record["key"].as_str() {
            diagnostic_key(key, window, cx);
            draw(window, cx).await;
        }
        let expected = record["selected"].as_str().unwrap();
        window
            .update(cx, |_, w, cx| {
                assert_eq!(
                    gpui_base::TextSelection::selected_text(w, cx),
                    expected,
                    "{scene}: {name}"
                );
                assert_eq!(
                    editor.read(cx).value().as_str(),
                    fixture["modelValue"].as_str().unwrap()
                );
                assert!(
                    !editor.focus_handle(cx).is_focused(w),
                    "{scene}: diagnostic keeps focus"
                );
                assert!(view.read(cx).dialog.is_some());
            })
            .unwrap();
        if footer_case.is_some() {
            let card = element("diagnostic-hover-card", window, cx)
                .unwrap()
                .bounds();
            let (message, footer) = window
                .update(cx, |_, w, _| {
                    let find = |order| {
                        gpui_base::test_support::find(
                            w,
                            &[],
                            &("diagnostic-message-layout", order).into(),
                        )
                        .unwrap()
                        .bounds()
                    };
                    (
                        find(1usize),
                        find(if footer_case == Some("crossmessage") {
                            3
                        } else {
                            2
                        }),
                    )
                })
                .unwrap();
            let expected_scroll = px(record["hoverScrollTop"].as_f64().unwrap() as f32);
            assert!(
                (initial_top.unwrap() - (message.top() - card.top()) - expected_scroll).abs()
                    <= px(0.5),
                "{scene}: {name} hidden-overflow reveal: actual={}, expected={expected_scroll}, message={message:?}, card={card:?}",
                initial_top.unwrap() - (message.top() - card.top())
            );
            for (actual, expected) in [
                (card.size.width, &record["hoverBox"]["width"]),
                (card.size.height, &record["hoverBox"]["height"]),
                (footer.size.width, &record["footerBox"]["width"]),
                (footer.size.height, &record["footerBox"]["height"]),
            ] {
                assert!(
                    (actual - px(expected.as_f64().unwrap() as f32)).abs() <= px(0.5),
                    "{scene}: {name} original hover/footer dimensions: actual={actual}, expected={expected}; card={card:?}, footer={footer:?}"
                );
            }
        }
        if !expected.is_empty()
            && (record["copy"].as_bool() == Some(true)
                || matches!(
                    name,
                    "after-local-AppKit-drag"
                        | "cmd-shift-left"
                        | "back-3"
                        | "forward-8"
                        | "forward-15"
                        | "forward-28"
                        | "word-back-1"
                        | "word-back-2"
                ))
        {
            let copied = cx
                .update_window(window.into(), |_, w, cx| {
                    let previous = cx.read_from_clipboard();
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        w.dispatch_event(
                            PlatformInput::KeyDown(KeyDownEvent {
                                keystroke: Keystroke::parse("cmd-c").unwrap(),
                                is_held: false,
                                prefer_character_input: false,
                            }),
                            cx,
                        )
                    }));
                    let copied = crate::ui::qa::copy_result(previous, cx);
                    if let Err(panic) = outcome {
                        std::panic::resume_unwind(panic);
                    }
                    copied
                })
                .unwrap();
            assert_eq!(
                copied.as_deref(),
                Some(expected),
                "{scene}: {name} raw copy after restoration"
            );
            copies += 1;
        }
    }
    assert!(copies > 0);
    let label = if footer_case.is_some() {
        "JSON DIAGNOSTIC FOOTER KEYBOARD QA PASS"
    } else {
        "JSON DIAGNOSTIC KEYBOARD QA PASS"
    };
    println!(
        "{label}: {scene} {} source states, {copies} actual copies restored immediately, editor unchanged and diagnostic focus retained",
        records.len()
    );
}

async fn selection(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    use super::new_key_qa::draw;
    let name = match scene {
        "entry-diagnostic-select-partial" => "hover-partial",
        "entry-diagnostic-select-whole" => "hover-whole",
        "entry-diagnostic-select-code" => "hover-code-span",
        "entry-diagnostic-select-wrapped" => "hover-wrapped-whole",
        "entry-diagnostic-select-overlap" => "hover-overlap-whole",
        "entry-diagnostic-select-reverse" => "hover-overlap-reverse",
        "entry-diagnostic-select-word" => "hover-word-double-click",
        "entry-diagnostic-select-navigation" => "navigation-partial",
        "entry-diagnostic-select-unicode" => "controlled-unicode-hard-line",
        _ => unreachable!(),
    };
    let fixture: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/json-diagnostic-selection-source.json"
    ))
    .unwrap();
    let record = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap();
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.pref.general.theme = "light".into();
                app.full_editor = false;
                app.apply_theme(w, cx);
            })
        })
        .unwrap();
    set(
        view,
        window,
        record["editorValue"].as_str().unwrap(),
        "JSON",
        true,
        cx,
    );
    idle(view, window, cx).await;
    let editor = editor(view, window, cx);
    window
        .update(cx, |_, w, cx| {
            editor.update(cx, |state, cx| {
                if name == "controlled-unicode-hard-line" {
                    let marker = record["markers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|d| d["owner"] == "selection-qa")
                        .unwrap();
                    let set = state.diagnostics_mut().unwrap();
                    set.clear();
                    set.push(
                        gpui_base::input::Diagnostic::new(
                            Position::new(0, 0)..Position::new(0, 2),
                            marker["message"].as_str().unwrap().to_owned(),
                        )
                        .with_severity(DiagnosticSeverity::Error)
                        .with_source("json")
                        .with_code("777"),
                    );
                    state.diagnostics_did_change(cx);
                    cx.notify();
                }
                state.focus(w, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    let nav = name == "navigation-partial";
    if nav {
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
    } else {
        let anchor = window
            .update(cx, |_, _, cx| {
                let state = editor.read(cx);
                let marker = state.diagnostics().unwrap().iter().next().unwrap();
                let bounds = state
                    .range_to_bounds(&(marker.range.start..marker.range.start))
                    .unwrap();
                bounds.origin + point(px(3.), bounds.size.height / 2.)
            })
            .unwrap();
        pointer(anchor, window, cx);
        for _ in 0..20 {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            draw(window, cx).await;
            if element("diagnostic-hover-card", window, cx).is_some() {
                break;
            }
        }
        assert!(
            element("diagnostic-hover-card", window, cx).is_some(),
            "{scene} hover opens"
        );
    }
    selection_gesture(&record["gesture"], window, cx).await;
    draw(window, cx).await;
    let expected = record["selected"].as_str().unwrap();
    let selected = window
        .update(cx, |_, w, cx| {
            gpui_base::TextSelection::selected_text(w, cx)
        })
        .unwrap();
    println!("DIAGNOSTIC SELECTION: {scene} actual={selected:?}, expected={expected:?}");
    assert_eq!(selected, expected, "{scene} exact logical selection");
    window
        .update(cx, |_, w, cx| {
            assert!(
                !editor.focus_handle(cx).is_focused(w),
                "selecting a diagnostic moves text focus out of the editor"
            );
        })
        .unwrap();
    let copied = cx
        .update_window(window.into(), |_, w, cx| {
            let previous = cx.read_from_clipboard();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                w.dispatch_event(
                    PlatformInput::KeyDown(KeyDownEvent {
                        keystroke: Keystroke::parse("cmd-c").unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    }),
                    cx,
                )
            }));
            let copied = crate::ui::qa::copy_result(previous, cx);
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
            copied
        })
        .unwrap();
    assert_eq!(
        copied.as_deref(),
        Some(expected),
        "{scene} actual Command+C; clipboard restored before assertion"
    );
    if name == "hover-partial" {
        diagnostic_key("escape", window, cx);
        draw(window, cx).await;
        assert!(element("diagnostic-hover-card", window, cx).is_none());
        window
            .update(cx, |_, w, cx| {
                assert!(view.read(cx).dialog.is_some());
                assert!(editor.focus_handle(cx).is_focused(w));
            })
            .unwrap();
        // Restore the paired selection after checking Escape.
        let anchor = window
            .update(cx, |_, _, cx| {
                editor.read(cx).range_to_bounds(&(5..5)).unwrap().center()
            })
            .unwrap();
        pointer(anchor, window, cx);
        cx.background_executor()
            .timer(Duration::from_millis(350))
            .await;
        draw(window, cx).await;
        selection_gesture(&record["gesture"], window, cx).await;
        draw(window, cx).await;
    }
    if nav {
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
        assert!(
            element("diagnostic-navigation-zone", window, cx)
                .unwrap()
                .label()
                .unwrap()
                .contains("2 of 2 problems")
        );
        window
            .update(cx, |_, w, cx| {
                assert!(editor.focus_handle(cx).is_focused(w))
            })
            .unwrap();
        diagnostic_key("escape", window, cx);
        draw(window, cx).await;
        assert!(element("diagnostic-navigation-zone", window, cx).is_none());
        assert!(
            window
                .update(cx, |_, _, cx| view.read(cx).dialog.is_some())
                .unwrap()
        );
        window
            .update(cx, |_, w, cx| {
                editor.update(cx, |state, cx| {
                    state.set_selected_range(0..0, cx);
                    state.focus(w, cx);
                })
            })
            .unwrap();
        diagnostic_key("alt-f8", window, cx);
        draw(window, cx).await;
        selection_gesture(&record["gesture"], window, cx).await;
        draw(window, cx).await;
    }
    println!(
        "JSON DIAGNOSTIC SELECTION QA PASS: {scene} actual mouse selection, exact original text, focus and Command+C with immediate clipboard restoration"
    );
}

async fn selection_gesture(gesture: &Json, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    use super::new_key_qa::draw;
    let position = |value: &Json| {
        point(
            px(value["x"].as_f64().unwrap() as f32),
            px(value["y"].as_f64().unwrap() as f32),
        )
    };
    let double = gesture["kind"] == "double-click";
    let start = position(if double {
        &gesture["position"]
    } else {
        &gesture["start"]
    });
    let end = if double {
        start
    } else {
        position(&gesture["end"])
    };
    cx.update_window(window.into(), |_, w, cx| {
        gpui_base::TextSelection::clear(w, cx);
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: start,
                ..Default::default()
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: if double { 2 } else { 1 },
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(window, cx).await;
    if !double {
        for step in 1..=12 {
            let position = start + (end - start) * (step as f32 / 12.);
            cx.update_window(window.into(), |_, w, cx| {
                w.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    }),
                    cx,
                )
            })
            .unwrap();
            draw(window, cx).await;
        }
    }
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: if double { 2 } else { 1 },
                ..Default::default()
            }),
            cx,
        )
    })
    .unwrap();
}
