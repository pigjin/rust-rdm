//! Controlled replies exercise the real background-job and UI callback path.
//! Synthetic texts match the original frontend's delayed-RPC fixture.
use super::new_key_qa::{click, draw, idle, type_field};
use super::*;
use std::sync::{OnceLock, mpsc};

struct Pending {
    raw: Vec<u8>,
    decode: String,
    format: String,
    reply: Option<mpsc::Sender<Result<codec::Converted>>>,
}

static CONTROL: OnceLock<Mutex<Option<mpsc::Sender<Pending>>>> = OnceLock::new();

pub(super) fn controlled(
    raw: &[u8],
    decode: &str,
    format: &str,
) -> Option<Result<codec::Converted>> {
    let sender = CONTROL.get_or_init(|| Mutex::new(None)).lock().clone()?;
    let (reply, receiver) = mpsc::channel();
    sender
        .send(Pending {
            raw: raw.to_vec(),
            decode: decode.into(),
            format: format.into(),
            reply: Some(reply),
        })
        .ok()?;
    Some(
        receiver
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "controlled entry conversion was not released"
                ))
            }),
    )
}

pub(in crate::ui) struct Controller {
    receiver: mpsc::Receiver<Pending>,
    requests: Vec<Pending>,
    active: bool,
}

impl Controller {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut slot = CONTROL.get_or_init(|| Mutex::new(None)).lock();
        assert!(slot.is_none());
        *slot = Some(sender);
        Self {
            receiver,
            requests: vec![],
            active: true,
        }
    }

    fn stop(&mut self) {
        if self.active {
            *CONTROL.get().unwrap().lock() = None;
            self.active = false;
        }
    }

    async fn next(&mut self, cx: &mut AsyncApp) {
        for _ in 0..150 {
            match self.receiver.try_recv() {
                Ok(request) => {
                    self.requests.push(request);
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("entry conversion controller disconnected")
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        panic!("entry conversion did not start");
    }

    fn respond(&mut self, index: usize, text: &str) {
        let request = &mut self.requests[index];
        request
            .reply
            .take()
            .unwrap()
            .send(Ok(codec::Converted {
                text: text.into(),
                decode: request.decode.clone(),
                format: if request.format.is_empty() {
                    "Raw".into()
                } else {
                    request.format.clone()
                },
            }))
            .unwrap();
    }

    fn assert_requests(&self, formats: &[&str]) {
        self.assert_requests_raw(formats, b"before");
    }

    fn assert_requests_raw(&self, formats: &[&str], raw: &[u8]) {
        assert_eq!(self.requests.len(), formats.len());
        for (request, format) in self.requests.iter().zip(formats) {
            assert_eq!(request.raw, raw);
            assert_eq!(request.decode, "None");
            assert_eq!(request.format, *format);
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop();
    }
}

fn fixture() -> Json {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/entry-conversion-source.json"
    ))
    .unwrap()
}

fn edge_fixture() -> Json {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/entry-conversion-edge-source.json"
    ))
    .unwrap()
}

fn assert_state(name: &str, view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let source = [fixture(), edge_fixture()]
        .into_iter()
        .find_map(|fixture| {
            fixture["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|event| event["name"] == name)
                .cloned()
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let dialog = app.displayed_entry_dialog().unwrap();
            assert_eq!(
                dialog.value("field", cx),
                source["field"].as_str().unwrap(),
                "{name} field"
            );
            assert_eq!(
                text_input(&dialog.editors["value"], cx),
                source["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("\n"),
                "{name} visible value"
            );
            assert_eq!(
                dialog.value("format", cx),
                source["format"].as_str().unwrap(),
                "{name} format"
            );
            assert_eq!(
                dialog.value("decode", cx),
                source["decode"].as_str().unwrap(),
                "{name} decode"
            );
            assert_eq!(
                dialog.entry_changed(cx),
                source["enabled"].as_bool().unwrap(),
                "{name} update enabled"
            );
        })
        .unwrap();
    println!("ENTRY CONVERSION STATE QA PASS: {name}");
}

async fn select(
    format: &str,
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    click("entry-select-format".into(), window, cx);
    draw(window, cx).await;
    let index = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .footer_options(value_footer::PickerKind::Format)
                .iter()
                .position(|value| value.as_deref() == Some(format))
                .unwrap()
        })
        .unwrap();
    click(("entry-picker-option", index).into(), window, cx);
    draw(window, cx).await;
}

async fn fresh(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                app.qa_prepare_entry_panel("edit-entry-hash", w, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    type_field(view, "field", "draft-field", window, cx);
    super::edit_entries_qa::text(view, "draft one\ndraft two", window, cx);
    draw(window, cx).await;
}

pub(in crate::ui) async fn prepare(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Controller {
    if scene == "entry-conversion-initial" || scene.starts_with("entry-conversion-empty-") {
        return prepare_edge(view, window, scene, cx).await;
    }
    fresh(view, window, cx).await;
    let mut control = Controller::new();
    select("JSON", view, window, cx).await;
    control.next(cx).await;
    if scene == "entry-conversion-followup" {
        control.respond(0, "first converted\nvalue");
        control.next(cx).await;
    }
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1.), px(1.)),
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(window, cx).await;
    assert_quiet(view, window, cx);
    assert_state(scene, view, window, cx);
    let record = fixture()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene)
        .unwrap()
        .clone();
    window
        .update(cx, |_, w, _| {
            for (id, name) in [
                ("entry-panel", "panel"),
                ("entry-panel-card", "card"),
                ("entry-value-editor", "editor"),
                ("entry-field-input", "field"),
            ] {
                let actual = gpui_base::test_support::find(w, &[], &id.to_owned().into())
                    .unwrap()
                    .bounds();
                let mut expected = record["geometry"][name].clone();
                if name == "field" {
                    expected["x"] = serde_json::json!(expected["x"].as_f64().unwrap() - 12.);
                    expected["width"] =
                        serde_json::json!(expected["width"].as_f64().unwrap() + 24.);
                }
                for (value, key) in [
                    (actual.origin.x, "x"),
                    (actual.origin.y, "y"),
                    (actual.size.width, "width"),
                    (actual.size.height, "height"),
                ] {
                    assert!(
                        (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
                        "{scene} {id} {key}: {value:?} vs {}",
                        expected[key]
                    );
                }
            }
        })
        .unwrap();
    control
}

pub(in crate::ui) async fn verify(
    mut control: Controller,
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene == "entry-conversion-initial" || scene.starts_with("entry-conversion-empty-") {
        verify_edge(control, view, window, scene, cx).await;
        return;
    }
    if scene == "entry-conversion-pending" {
        control.respond(0, "first converted\nvalue");
        control.next(cx).await;
        draw(window, cx).await;
        assert_state("entry-conversion-followup", view, window, cx);
    }
    control.respond(1, "second converted\nvalue");
    idle(view, window, cx).await;
    control.assert_requests(&["JSON", "JSON"]);
    assert!(matches!(
        control.receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_state("entry-conversion-settled", view, window, cx);
    control.stop();
    if scene != "entry-conversion-pending" {
        return;
    }
    verify_race(view, window, cx).await;
    verify_settings(view, window, cx).await;
    verify_pending_save(view, window, cx).await;
    verify_command_count(view, window, cx).await;
    println!(
        "ENTRY CONVERSION INPUT QA PASS: two requests, editable pending value, save before completion, reply order and nested preferences"
    );
}

fn assert_quiet(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, w, cx| {
            assert_eq!(view.read(cx).busy, view.read(cx).entry_conversions);
            assert!(
                gpui_base::test_support::find(w, &[], &"app-loading-indicator".into()).is_none()
            );
        })
        .unwrap();
}

async fn prepare_edge(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Controller {
    idle(view, window, cx).await;
    let mut control;
    let event;
    if scene == "entry-conversion-initial" {
        control = Controller::new();
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.qa_prepare_entry_panel("edit-entry-hash", w, cx)
                })
            })
            .unwrap();
        control.next(cx).await;
        event = "normalized-initial-pending";
    } else {
        fresh(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    let DialogKind::EditEntry(mut row) = app.dialog.as_ref().unwrap().kind.clone()
                    else {
                        unreachable!()
                    };
                    row.value.clear();
                    app.open_dialog(DialogKind::EditEntry(row), w, cx);
                })
            })
            .unwrap();
        idle(view, window, cx).await;
        super::edit_entries_qa::text(view, "visible draft", window, cx);
        draw(window, cx).await;
        control = Controller::new();
        select("JSON", view, window, cx).await;
        control.next(cx).await;
        if scene.ends_with("-pending") {
            event = "empty-baseline-pending";
        } else {
            control.respond(0, "");
            control.next(cx).await;
            if scene.ends_with("-followup") {
                event = "empty-baseline-followup";
            } else {
                control.respond(1, "");
                idle(view, window, cx).await;
                event = "empty-baseline-ready";
            }
        }
    }
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1.), px(1.)),
                ..Default::default()
            }),
            cx,
        )
    })
    .unwrap();
    draw(window, cx).await;
    assert_state(event, view, window, cx);
    assert_quiet(view, window, cx);
    let record = edge_fixture()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["name"] == scene)
        .unwrap()
        .clone();
    window
        .update(cx, |_, w, _| {
            for (id, name) in [
                ("entry-panel", "panel"),
                ("entry-panel-card", "card"),
                ("entry-value-editor", "editor"),
                ("entry-field-input", "field"),
            ] {
                let actual = gpui_base::test_support::find(w, &[], &id.to_owned().into())
                    .unwrap()
                    .bounds();
                let mut expected = record["geometry"][name].clone();
                if name == "field" {
                    expected["x"] = serde_json::json!(expected["x"].as_f64().unwrap() - 12.);
                    expected["width"] =
                        serde_json::json!(expected["width"].as_f64().unwrap() + 24.);
                }
                for (value, key) in [
                    (actual.origin.x, "x"),
                    (actual.origin.y, "y"),
                    (actual.size.width, "width"),
                    (actual.size.height, "height"),
                ] {
                    assert!(
                        (value.as_f32() - expected[key].as_f64().unwrap() as f32).abs() < 0.65,
                        "{scene} {id} {key}"
                    );
                }
            }
        })
        .unwrap();
    control
}

async fn verify_edge(
    mut control: Controller,
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) {
    if scene == "entry-conversion-initial" {
        let (release, receiver) = mpsc::channel();
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.cpu_job(
                        w,
                        cx,
                        move || {
                            receiver.recv_timeout(Duration::from_secs(5))?;
                            Ok(())
                        },
                        |_, _, _, _| {},
                    )
                })
            })
            .unwrap();
        draw(window, cx).await;
        window
            .update(cx, |_, w, cx| {
                assert!(view.read(cx).busy > view.read(cx).entry_conversions);
                assert!(
                    gpui_base::test_support::find(w, &[], &"app-loading-indicator".into())
                        .is_some()
                );
            })
            .unwrap();
        release.send(()).unwrap();
        for _ in 0..100 {
            if window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    app.busy == app.entry_conversions
                })
                .unwrap()
            {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        draw(window, cx).await;
        assert_quiet(view, window, cx);
        control.respond(0, "before");
        idle(view, window, cx).await;
        assert_state("normalized-initial-ready", view, window, cx);
        control.assert_requests(&["Raw"]);
        assert!(matches!(
            control.receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        control.stop();
        println!(
            "ENTRY INITIAL QA PASS: one actual conversion from None/Raw, blank initial field/placeholder, ordinary jobs retain their loading indicator"
        );
    } else {
        if scene.ends_with("-pending") {
            control.respond(0, "");
            control.next(cx).await;
            draw(window, cx).await;
            assert_state("empty-baseline-followup", view, window, cx);
        }
        if control.requests[1].reply.is_some() {
            control.respond(1, "");
            idle(view, window, cx).await;
        }
        assert_state("empty-baseline-ready", view, window, cx);
        control.assert_requests_raw(&["JSON", "JSON"], b"");
        control.stop();
        window
            .update(cx, |_, _, cx| {
                let dialog = view.read(cx).dialog.as_ref().unwrap();
                assert_eq!(dialog.value("value", cx), "");
                assert_eq!(text_input(&dialog.editors["value"], cx), "visible draft");
            })
            .unwrap();
        if scene.ends_with("-ready") {
            verify_empty_save(view, window, cx).await;
        }
        println!(
            "ENTRY EMPTY QA PASS: {scene} retains visible draft but clears internal save value after empty replies"
        );
    }
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).busy, 0);
            assert_eq!(view.read(cx).entry_conversions, 0);
        })
        .unwrap();
}

async fn verify_empty_save(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
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
    let run = |args: &[&str]| {
        service
            .run(&connection, 15, crate::redis::command(args))
            .unwrap()
    };
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    let key = "entry-empty-uiqa:target";
    run(&["HSET", key, "old", ""]);
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                tab.db = 15;
                tab.current_key = Some(key.as_bytes().to_vec());
                tab.detail = Some(
                    service
                        .detail(&connection, 15, key.as_bytes(), 0, "", 100, "", false)
                        .unwrap(),
                );
                let target = app.dialog.as_mut().unwrap().target.as_mut().unwrap();
                target.db = 15;
                target.key = key.as_bytes().to_vec();
            })
        })
        .unwrap();
    super::edit_entries_qa::save_shortcut(view, window, cx);
    window
        .update(cx, |_, _, cx| assert!(view.read(cx).dialog.is_none()))
        .unwrap();
    idle(view, window, cx).await;
    let exists = run(&["HEXISTS", key, "old"]).integer().unwrap();
    let saved = run(&["HGET", key, "old"]).text();
    run(&["DEL", key]);
    assert_eq!(exists, 1);
    assert_eq!(saved, "");
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    fresh(view, window, cx).await;
    println!(
        "ENTRY EMPTY SAVE QA PASS: editor Cmd+S submits empty internal content while the visible draft is retained; disposable DB15 cleaned"
    );
}

async fn verify_race(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    fresh(view, window, cx).await;
    let mut control = Controller::new();
    select("JSON", view, window, cx).await;
    control.next(cx).await;
    select("YAML", view, window, cx).await;
    control.next(cx).await;
    assert_state("two-selections-pending", view, window, cx);
    control.respond(1, "newer yaml");
    control.next(cx).await;
    draw(window, cx).await;
    assert_state("newer-first-reply", view, window, cx);
    control.respond(0, "older json");
    control.next(cx).await;
    draw(window, cx).await;
    assert_state("older-late-reply", view, window, cx);
    for index in 2..6 {
        let text = format!("followup {}", control.requests[index].format);
        control.respond(index, &text);
        control.next(cx).await;
    }
    draw(window, cx).await;
    assert_state("race-followup-chain", view, window, cx);
    control.assert_requests(&[
        "JSON", "YAML", "YAML", "JSON", "YAML", "JSON", "YAML", "JSON",
    ]);
    // End the bounded race with the actual close action, then release replies.
    click("close-entry".into(), window, cx);
    control.stop();
    control.respond(6, "drained YAML");
    control.respond(7, "drained JSON");
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| assert!(view.read(cx).dialog.is_none()))
        .unwrap();
}

async fn verify_settings(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    fresh(view, window, cx).await;
    let mut control = Controller::new();
    select("JSON", view, window, cx).await;
    control.next(cx).await;
    click("entry-select-decode".into(), window, cx);
    draw(window, cx).await;
    let last = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .footer_options(value_footer::PickerKind::Decode)
                .len()
                - 1
        })
        .unwrap();
    click(("entry-picker-option", last).into(), window, cx);
    draw(window, cx).await;
    control.respond(0, "under settings first");
    control.next(cx).await;
    control.respond(1, "under settings final");
    idle(view, window, cx).await;
    assert_state("conversion-finishes-under-settings", view, window, cx);
    window
        .update(cx, |_, _, cx| {
            assert!(matches!(
                view.read(cx).dialog.as_ref().unwrap().kind,
                DialogKind::Preferences
            ))
        })
        .unwrap();
    control.assert_requests(&["JSON", "JSON"]);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| app.close_dialog(w, cx))
        })
        .unwrap();
}

async fn verify_pending_save(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    fresh(view, window, cx).await;
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
    let run = |args: &[&str]| {
        service
            .run(&connection, 15, crate::redis::command(args))
            .unwrap()
    };
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    let key = "entry-conversion-uiqa:target";
    run(&["HSET", key, "old", "before"]);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                tab.db = 15;
                tab.current_key = Some(key.as_bytes().to_vec());
                tab.detail = Some(
                    service
                        .detail(&connection, 15, key.as_bytes(), 0, "", 100, "", false)
                        .unwrap(),
                );
                let row = tab.detail.as_ref().unwrap().rows[0].clone();
                app.open_dialog(DialogKind::EditEntry(row), w, cx);
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    type_field(view, "field", "draft-field", window, cx);
    let mut control = Controller::new();
    select("JSON", view, window, cx).await;
    control.next(cx).await;
    super::edit_entries_qa::text(view, "typed while waiting", window, cx);
    draw(window, cx).await;
    assert_state("entry-conversion-edit-while-pending", view, window, cx);
    click("save-entry".into(), window, cx);
    window
        .update(cx, |_, _, cx| assert!(view.read(cx).dialog.is_none()))
        .unwrap();
    control.stop();
    control.respond(0, "closed reply");
    idle(view, window, cx).await;
    let saved = run(&["HGET", key, "draft-field"]).text();
    let old = run(&["HEXISTS", key, "old"]).integer().unwrap();
    run(&["DEL", key]);
    assert_eq!(saved, "typed while waiting");
    assert_eq!(old, 0);
    assert_eq!(run(&["DBSIZE"]).integer().unwrap(), 0);
    fresh(view, window, cx).await;
    println!(
        "ENTRY CONVERSION SAVE QA PASS: actual pending-save button writes only disposable DB15, closes immediately and ignores the closed reply"
    );
}

async fn verify_command_count(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    fresh(view, window, cx).await;
    let (path,backup)=window.update(cx,|_,_,cx|view.update(cx,|app,_|{
        assert_eq!(std::fs::read_to_string(app.store.root.join("RDM-QA-FIXTURE")).unwrap(),"rust-rdm-render-fixture-v1\n");
        let path=app.store.root.join(format!("entry-conversion-count-{}.log",std::process::id()));
        assert!(!path.exists());
        let code=format!("from pathlib import Path; import sys; p=Path({:?}); p.open('a').write('call\\n'); print(sys.argv[1])",path.to_str().unwrap());
        let backup=app.pref.decoder.clone();
        app.pref.decoder.push(Decoder {name:"Entry conversion count QA".into(),enable:true,decode_path:"/usr/bin/python3".into(),decode_args:vec!["-c".into(),code],..Default::default()});
        (path,backup)
    })).unwrap();
    click("entry-select-decode".into(), window, cx);
    draw(window, cx).await;
    let index = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .footer_options(value_footer::PickerKind::Decode)
                .iter()
                .position(|value| value.as_deref() == Some("Entry conversion count QA"))
                .unwrap()
        })
        .unwrap();
    click(("entry-picker-option", index).into(), window, cx);
    idle(view, window, cx).await;
    let calls = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| app.pref.decoder = backup)
        })
        .unwrap();
    assert_eq!(calls, "call\ncall\n");
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let dialog = app.dialog.as_ref().unwrap();
            assert_eq!(dialog.value("value", cx), "before");
            assert_eq!(dialog.value("field", cx), "old");
            assert!(!dialog.entry_changed(cx));
        })
        .unwrap();
    fresh(view, window, cx).await;
    println!(
        "ENTRY CONVERSION COMMAND QA PASS: actual local decoder executes exactly twice; count file removed and in-memory decoder list restored"
    );
}
