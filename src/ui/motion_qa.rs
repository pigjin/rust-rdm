use super::*;
use dialogs::DialogKind;
use dialogs::new_key_qa::click;

fn frame(window: WindowHandle<Root>, cx: &mut AsyncApp, path: Option<&std::path::Path>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        if let Some(path) = path {
            window.render_to_image().unwrap().save(path).unwrap();
        }
    })
    .unwrap();
}

async fn settle(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(210))
        .await;
    frame(window, cx, None);
}

async fn trace_card(window: WindowHandle<Root>, id: &str, cx: &mut AsyncApp) -> Vec<Json> {
    let started = Instant::now();
    let mut frames = Vec::new();
    for index in 0..16 {
        if index > 0 {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
        let bounds = cx
            .update_window(window.into(), |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
                gpui_base::test_support::find(
                    window,
                    &[],
                    &ElementId::from(SharedString::from(id.to_owned())),
                )
                .map(|node| node.bounds())
            })
            .unwrap();
        frames.push(serde_json::json!({
            "elapsedMs": started.elapsed().as_secs_f64() * 1000.,
            "bounds": bounds.map(|b| [b.origin.x.as_f32(), b.origin.y.as_f32(), b.size.width.as_f32(), b.size.height.as_f32()])
        }));
    }
    frames
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    output: &std::path::Path,
    cx: &mut AsyncApp,
) {
    let reduced = scene.ends_with("reduced");
    let images = output.join("motion-frames").join(scene);
    std::fs::create_dir_all(&images).unwrap();
    let original = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.close_dialog(window, cx);
                let original = app.pref.clone();
                app.messages.clear();
                app.notifications.clear();
                app.exiting_messages.clear();
                app.exiting_notifications.clear();
                app.pref.general.theme = if scene.ends_with("dark") {
                    "dark"
                } else {
                    "light"
                }
                .into();
                app.apply_theme(window, cx);
                cx.set_reduce_motion(reduced);
                app.open_dialog(DialogKind::Connection(None), window, cx);
                original
            })
        })
        .unwrap();
    let entrance = trace_card(window, "dialog-card", cx).await;
    std::fs::write(
        images.join("dialog-enter-frames.json"),
        serde_json::to_vec_pretty(&entrance).unwrap(),
    )
    .unwrap();
    let ys: Vec<_> = entrance
        .iter()
        .map(|frame| frame["bounds"][1].as_f64().unwrap())
        .collect();
    assert!(
        ys.windows(2).all(|pair| pair[0] >= pair[1]),
        "native entrance must move continuously upwards"
    );
    if reduced {
        assert_eq!(ys[0], *ys.last().unwrap());
    } else {
        assert!(
            ys.iter().any(|y| *y < ys[0] && *y > *ys.last().unwrap()),
            "native entrance must include intermediate frames"
        );
    }
    frame(window, cx, Some(&images.join("dialog-enter.png")));
    settle(window, cx).await;
    frame(window, cx, Some(&images.join("dialog-present.png")));
    let old_id = window
        .update(cx, |_, _, cx| view.read(cx).dialog.as_ref().unwrap().id)
        .unwrap();
    let submit_position = window
        .update(cx, |_, window, _| {
            gpui_base::test_support::find(window, &[], &"dialog-save".into())
                .unwrap()
                .bounds()
                .center()
        })
        .unwrap();
    click("dialog-cancel".into(), window, cx);
    window
        .update(cx, |_, window, cx| {
            let app = view.read(cx);
            assert!(app.dialog.is_none(), "logical close is immediate");
            assert!(
                app.focus.is_focused(window),
                "retired editors must release focus"
            );
            if !reduced {
                assert_eq!(app.exiting_dialog.as_ref().unwrap().value.id, old_id);
            }
        })
        .unwrap();
    if !reduced {
        // Click the previous bounds before image readback can outlast the exit.
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: submit_position,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    position: submit_position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    position: submit_position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
        window
            .update(cx, |_, _, cx| assert!(view.read(cx).dialog.is_none()))
            .unwrap();
    }
    let new_id = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.open_dialog(DialogKind::Preferences, window, cx);
                assert!(app.exiting_dialog.is_none());
                app.dialog.as_ref().unwrap().id
            })
        })
        .unwrap();
    settle(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).dialog.as_ref().unwrap().id, new_id)
        })
        .unwrap();
    frame(window, cx, Some(&images.join("dialog-reopened.png")));
    // Preview rollback and task cancellation happen before the old card finishes leaving.
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let old_size = app.pref.general.font_size;
                app.pref.general.font_size = old_size + 2.;
                let cancellation = Cancellation::default();
                app.transfer = Some(cancellation.clone());
                app.close_dialog(window, cx);
                assert_eq!(app.pref.general.font_size, old_size);
                assert!(cancellation.canceled());
                assert!(app.dialog.is_none());
                cx.set_reduce_motion(true);
            })
        })
        .unwrap();
    frame(window, cx, None);
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(app.exiting_dialog.is_none());
            cx.set_reduce_motion(reduced);
        })
        .unwrap();

    // Exercise every distinct modal renderer while its logical dialog is absent.
    for (name, kind) in [
        ("new-key", DialogKind::NewKey),
        ("add-entries", DialogKind::AddEntries),
        ("import", DialogKind::Import),
        ("export", DialogKind::Export),
        ("warning", DialogKind::CleanLog),
    ] {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| app.open_dialog(kind, window, cx))
            })
            .unwrap();
        frame(window, cx, None);
        settle(window, cx).await;
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.close_dialog(window, cx);
                    cx.notify();
                })
            })
            .unwrap();
        if name == "new-key" {
            let exit = trace_card(window, "dialog-card", cx).await;
            assert!(
                exit.last().unwrap()["bounds"].is_null(),
                "native exit releases its card"
            );
            let ys: Vec<_> = exit
                .iter()
                .filter_map(|frame| frame["bounds"][1].as_f64())
                .collect();
            assert!(
                ys.windows(2).all(|pair| pair[0] <= pair[1]),
                "native exit must reverse without a jump"
            );
            if reduced {
                assert!(ys.is_empty());
            } else {
                assert!(ys.len() > 1 && ys[0] < *ys.last().unwrap());
            }
            std::fs::write(
                images.join("new-key-exit-frames.json"),
                serde_json::to_vec_pretty(&exit).unwrap(),
            )
            .unwrap();
        }
        frame(window, cx, Some(&images.join(format!("{name}-exit.png"))));
        settle(window, cx).await;
        window
            .update(cx, |_, _, cx| {
                assert!(view.read(cx).exiting_dialog.is_none())
            })
            .unwrap();
    }

    let warning_id = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.qa_prepare_entry_panel(
                    if scene.ends_with("dark") {
                        "edit-entry-hash-dark"
                    } else {
                        "edit-entry-hash"
                    },
                    window,
                    cx,
                );
                app.request_tab_close(app.active.unwrap(), false, window, cx);
                app.tab_close_warnings.last().unwrap().id
            })
        })
        .unwrap();
    frame(window, cx, None);
    settle(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                assert!(app.cancel_tab_close(cx));
                assert!(app.editing_entry(), "cancel retains the inline editor");
            })
        })
        .unwrap();
    let warning_exit = trace_card(window, &format!("tab-close-card-{warning_id}"), cx).await;
    if !reduced {
        assert!(
            !warning_exit[0]["bounds"].is_null(),
            "inline editors must not suppress warning exits"
        );
    }
    assert!(warning_exit.last().unwrap()["bounds"].is_null());
    std::fs::write(
        images.join("warning-exit-frames.json"),
        serde_json::to_vec_pretty(&warning_exit).unwrap(),
    )
    .unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.close_dialog(window, cx))
        })
        .unwrap();

    let last_message = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.toast("第一条消息", false);
                let id = app.add_message("第二条消息".into(), notices::MessageKind::Info);
                app.error_notification("测试通知".into(), cx);
                cx.notify();
                id
            })
        })
        .unwrap();
    frame(window, cx, None);
    settle(window, cx).await;
    frame(window, cx, Some(&images.join("notices-present.png")));
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.destroy_message(last_message, cx);
                let id = app.notifications[0].id;
                app.dismiss_notification(id, cx);
                assert_eq!(app.messages.len(), 1);
                assert!(app.notifications.is_empty());
                cx.notify();
            })
        })
        .unwrap();
    frame(window, cx, Some(&images.join("notices-exit.png")));
    cx.background_executor()
        .timer(Duration::from_millis(45))
        .await;
    frame(window, cx, Some(&images.join("notices-exit-middle.png")));
    settle(window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(
                app.exiting_messages
                    .iter()
                    .all(|exit| exit.value.id != last_message)
            );
            assert!(app.exiting_notifications.is_empty());
        })
        .unwrap();
    frame(window, cx, Some(&images.join("notices-settled.png")));

    // Rapid navigation and geometry changes must settle on the current target.
    for pane in [Pane::Status, Pane::Detail, Pane::Cli, Pane::Status] {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    if let Some(id) = app.active {
                        app.switch_pane(id, pane, window, cx);
                    }
                })
            })
            .unwrap();
        frame(window, cx, None);
    }
    settle(window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.pref = original;
                app.messages.clear();
                app.notifications.clear();
                app.exiting_messages.clear();
                app.exiting_notifications.clear();
                cx.set_reduce_motion(false);
                app.apply_theme(window, cx);
                cx.notify();
            })
        })
        .unwrap();
    frame(window, cx, None);
    println!(
        "QA PASS: {scene}: immediate close/focus/cancel, inert exit, reopen isolation, five modal exits, notice cleanup and rapid navigation"
    );
}
