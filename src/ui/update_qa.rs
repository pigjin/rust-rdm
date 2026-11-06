use super::*;
use http_client::{AsyncBody, FakeHttpClient, Response};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(in crate::ui) struct Saved {
    pref: Preferences,
    updates: State,
    record: Option<Vec<u8>>,
}

fn fixture() -> Vec<u8> {
    let version = "999.0.0";
    let os = std::env::consts::OS;
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        std::env::consts::ARCH
    };
    let extensions = match os {
        "macos" => vec![".dmg"],
        "windows" => vec!["-setup.exe"],
        _ => vec![".AppImage", ".deb"],
    };
    let assets = extensions.into_iter().map(|extension| {
        let name = format!("RedisDock-{version}-{os}-{arch}{extension}");
        serde_json::json!({"name":name,"browser_download_url":format!("https://github.com/pigjin/rust-rdm/releases/download/v{version}/{name}")})
    }).collect::<Vec<_>>();
    let notes = (0..80).map(|index| format!("Change {index}: RedisDock — 中文 notes. <b>Plain text</b> [link](https://invalid.test)\n")).collect::<String>();
    serde_json::to_vec(&serde_json::json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,
        "html_url":format!("https://github.com/pigjin/rust-rdm/releases/tag/v{version}"),"body":notes,"assets":assets})).unwrap()
}

fn client(status: u16, body: Vec<u8>, calls: Arc<AtomicUsize>) -> Arc<dyn http_client::HttpClient> {
    FakeHttpClient::create(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        let body = body.clone();
        async move {
            Ok(Response::builder()
                .status(status)
                .body(AsyncBody::from(body))?)
        }
    })
}

async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(220))
        .await;
    cx.update_window(window.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    })
    .unwrap();
}

async fn finished(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..100 {
        if !window
            .update(cx, |_, _, cx| view.read(cx).updates.checker.running)
            .unwrap()
        {
            draw(window, cx).await;
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("update QA request did not settle");
}

fn key(window: WindowHandle<Root>, key: &str, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, w, cx| {
        w.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::KeyUp(KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            }),
            cx,
        );
    })
    .unwrap();
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    qa_json::idle(view, window, cx).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let saved = window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let saved = Saved {
                    pref: app.pref.clone(),
                    updates: std::mem::take(&mut app.updates),
                    record: std::fs::read(app.store.root.join(CHECK_FILE)).ok(),
                };
                app.pref.general.language = if scene.ends_with("en") { "en" } else { "zh" }.into();
                app.pref.general.theme = if scene.ends_with("dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(w, cx);
                app.load_language(cx);
                app.updates.client = Some(client(200, fixture(), calls.clone()));
                app.open_dialog(dialogs::DialogKind::Group(None), w, cx);
                app.set_dialog_field("name", "unsaved update draft", w, cx);
                let id = app.dialog.as_ref().unwrap().id;
                let focused = w.focused(cx);
                app.check_update(false, w, cx);
                assert_eq!(app.dialog.as_ref().unwrap().id, id);
                assert_eq!(w.focused(cx), focused);
                assert_eq!(app.busy, 0);
                saved
            })
        })
        .unwrap();
    finished(view, window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                assert_eq!(
                    app.dialog.as_ref().unwrap().value("name", cx),
                    "unsaved update draft"
                );
                assert!(app.updates.notice);
                assert!(!app.update_dialog_available());
                assert!(app.messages.is_empty());
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                app.close_dialog(w, cx);
            })
        })
        .unwrap();
    draw(window, cx).await;
    draw(window, cx).await;
    qa_json::click("update-details", window, cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert!(matches!(
                view.read(cx).dialog.as_ref().unwrap().kind,
                dialogs::DialogKind::Update
            ));
        })
        .unwrap();
    key(window, "escape", cx);
    draw(window, cx).await;
    draw(window, cx).await;
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.show_update_result(
                    Ok(Some(app.updates.release.clone().unwrap())),
                    false,
                    w,
                    cx,
                );
            })
        })
        .unwrap();
    draw(window, cx).await;
    qa_json::click("update-dismiss", window, cx);
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                let release = app.updates.release.clone().unwrap();
                app.show_update_result(Ok(Some(release)), false, w, cx);
                assert!(!app.updates.notice, "dismissal lasts for this session");
                app.check_update(true, w, cx);
                assert!(matches!(
                    app.dialog.as_ref().unwrap().kind,
                    dialogs::DialogKind::Update
                ));
                assert_eq!(
                    calls.load(Ordering::SeqCst),
                    1,
                    "manual check reuses the cached result"
                );
            })
        })
        .unwrap();
    draw(window, cx).await;
    key(window, "tab", cx);
    draw(window, cx).await;
    key(window, "enter", cx);
    draw(window, cx).await;
    qa_json::click("update-download-0", window, cx);
    window
        .update(cx, |_, w, cx| {
            let app = view.read(cx);
            let release = app.updates.release.as_ref().unwrap();
            assert_eq!(
                app.updates.opened_urls,
                vec![release.page_url.clone(), release.downloads[0].url.clone()]
            );
            assert!(gpui_base::test_support::find(w, &[], &"update-notes".into()).is_some());
        })
        .unwrap();
    let before = window
        .update(cx, |_, w, _| {
            gpui_base::test_support::find(w, &[], &"update-notes".into())
                .unwrap()
                .bounds()
                .origin
                .y
        })
        .unwrap();
    cx.update_window(window.into(), |_, w, cx| {
        let position = gpui_base::test_support::find(w, &[], &"dialog-body".into())
            .unwrap()
            .bounds()
            .center();
        w.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(window, cx).await;
    window
        .update(cx, |_, w, _| {
            let after = gpui_base::test_support::find(w, &[], &"update-notes".into())
                .unwrap()
                .bounds()
                .origin
                .y;
            assert!(
                after < before,
                "release notes must scroll independently of download buttons"
            );
            assert!(
                gpui_base::test_support::find(w, &[], &"update-download-0".into())
                    .unwrap()
                    .visible()
            );
        })
        .unwrap();
    key(window, "escape", cx);
    draw(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert!(
                view.read(cx).dialog.is_none(),
                "Escape closes update details"
            )
        })
        .unwrap();

    if scene == "update-light" {
        for (status, expected) in [
            (404, "updates.no_release"),
            (403, "updates.rate_limited"),
            (500, "updates.network_error"),
            (200, "updates.invalid_response"),
        ] {
            window
                .update(cx, |_, w, cx| {
                    view.update(cx, |app, cx| {
                        app.messages.clear();
                        app.updates = State::default();
                        app.updates.client =
                            Some(client(status, b"invalid JSON".to_vec(), calls.clone()));
                        app.check_update(false, w, cx);
                        if status == 404 {
                            app.check_update(true, w, cx);
                        }
                        assert_eq!(app.busy, 0);
                    })
                })
                .unwrap();
            finished(view, window, cx).await;
            if status != 404 {
                window
                    .update(cx, |_, w, cx| {
                        view.update(cx, |app, cx| {
                            assert!(app.messages.is_empty(), "automatic errors stay silent");
                            app.check_update(true, w, cx);
                        })
                    })
                    .unwrap();
            }
            window
                .update(cx, |_, _, cx| {
                    let app = view.read(cx);
                    assert_eq!(app.messages.last().unwrap().text, app.tr(expected));
                    assert!(app.updates.loading.is_none());
                })
                .unwrap();
        }
        let mut value: Json = serde_json::from_slice(&fixture()).unwrap();
        value["tag_name"] = format!("v{}", env!("CARGO_PKG_VERSION")).into();
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.messages.clear();
                    app.updates = State::default();
                    app.updates.client = Some(client(
                        200,
                        serde_json::to_vec(&value).unwrap(),
                        calls.clone(),
                    ));
                    app.check_update(true, w, cx);
                })
            })
            .unwrap();
        finished(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(
                    app.messages.last().unwrap().text,
                    app.tr("updates.up_to_date")
                );
            })
            .unwrap();
        value = serde_json::from_slice(&fixture()).unwrap();
        value["assets"] = serde_json::json!([]);
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    app.updates = State::default();
                    app.updates.client = Some(client(
                        200,
                        serde_json::to_vec(&value).unwrap(),
                        calls.clone(),
                    ));
                    app.check_update(true, w, cx);
                })
            })
            .unwrap();
        finished(view, window, cx).await;
        window
            .update(cx, |_, w, cx| {
                view.update(cx, |app, cx| {
                    assert!(app.updates.release.as_ref().unwrap().downloads.is_empty());
                    assert!(
                        gpui_base::test_support::find(w, &[], &"update-download-0".into())
                            .is_none()
                    );
                    app.close_dialog(w, cx);
                })
            })
            .unwrap();
        draw(window, cx).await;
        draw(window, cx).await;
    }
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.updates = State::default();
                app.updates.client = Some(client(200, fixture(), calls));
                app.check_update(true, w, cx);
            })
        })
        .unwrap();
    finished(view, window, cx).await;
    println!(
        "QA PASS: {scene} update UI preserves unsaved dialogs, caches checks, closes notices, scrolls plain notes and captures browser links without downloads"
    );
    saved
}

pub(in crate::ui) fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: Saved,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, w, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(w, cx);
                app.exiting_dialog = None;
                app.pref = saved.pref;
                app.updates = saved.updates;
                let path = app.store.root.join(CHECK_FILE);
                if let Some(bytes) = saved.record {
                    std::fs::write(path, bytes).unwrap();
                } else if path.exists() {
                    std::fs::remove_file(path).unwrap();
                }
                app.apply_theme(w, cx);
                app.load_language(cx);
                app.messages.clear();
                cx.notify();
            })
        })
        .unwrap();
}
