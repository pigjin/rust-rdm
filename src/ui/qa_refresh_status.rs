//! Source-backed status refresh lifetime checks in the marked disposable profile.
use super::qa_json::{click, idle};
use super::*;

async fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
}
async fn show(window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &"status-refresh".into())
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
    })
    .unwrap();
    draw(window, cx).await;
    cx.update_window(window.into(), |_, window, _| {
        assert!(gpui_base::test_support::find(window, &[], &"refresh-form".into()).is_some());
    })
    .unwrap();
}
async fn reopen(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let connection = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let name = app.tabs[i].conn.name.clone();
                let connection = flatten(&app.connections)
                    .into_iter()
                    .find(|c| c.name == name)
                    .unwrap();
                app.close_tab(app.tabs[i].id, window, cx);
                connection
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.open_connection(connection, window, cx))
        })
        .unwrap();
    idle(view, window, cx).await;
}
async fn off(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    show(window, cx).await;
    let on = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.tabs[app.index().unwrap()].status_auto
        })
        .unwrap();
    if on {
        click("refresh-switch-frame", window, cx);
        draw(window, cx).await;
    }
}
async fn enable(view: &Entity<RdmApp>, window: WindowHandle<Root>, raw: &str, cx: &mut AsyncApp) {
    off(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            let app = view.read(cx);
            let field = app.tabs[app.index().unwrap()].status_interval.clone();
            field.update(cx, |field, cx| {
                field.focus(window, cx);
                let length = field.value().len();
                field.replace_text_in_range(Some(0..length), raw, window, cx);
            });
            field.update(cx, |_, cx| cx.emit(InputEvent::Blur));
        })
        .unwrap();
    draw(window, cx).await;
    click("refresh-switch-frame", window, cx);
    draw(window, cx).await;
}
async fn record(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    name: &str,
    oracle: &Json,
    cx: &mut AsyncApp,
) {
    show(window, cx).await;
    let actual=window.update(cx,|_,_,cx| {
        let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
        let disk=flatten(&app.store.connections().unwrap()).into_iter().find(|c|c.name==tab.conn.name).unwrap();
        assert!(app.messages.is_empty(),"status refresh must not show errors");
        serde_json::json!({
            "name":name,"input":text_input(&tab.status_interval,cx),"on":tab.status_auto,
            "disabled":!tab.status_interval.read(cx).is_editable(),
            "profile":app.status_refresh_profiles.get(&tab.conn.name).copied().unwrap_or(tab.conn.refresh_interval as f64),
            "persisted":disk.refresh_interval
        })
    }).unwrap();
    let expected = oracle["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap();
    for field in ["name", "input", "on", "disabled"] {
        assert_eq!(actual[field], expected[field], "status {name}: {field}");
    }
    for field in ["profile", "persisted"] {
        assert_eq!(
            actual[field].as_f64(),
            expected[field].as_f64(),
            "status {name}: {field}"
        );
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: point(px(1.), px(1.)),
                ..Default::default()
            }),
            cx,
        )
    })
    .unwrap();
    draw(window, cx).await;
}

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                if app.index().is_none() {
                    let connection = flatten(&app.connections).into_iter().next().unwrap();
                    app.open_connection(connection, window, cx);
                }
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    let oracle: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/refresh-status-source.json"
    ))
    .unwrap();
    let (original_connections, original_profiles, original_connection, original_key) = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let tab = &app.tabs[app.index().unwrap()];
                let original = (
                    app.connections.clone(),
                    app.status_refresh_profiles.clone(),
                    tab.conn.clone(),
                    tab.current_key.clone(),
                );
                let mut connection = tab.conn.clone();
                connection.refresh_interval = 5;
                crate::storage::save_connection(
                    &mut app.connections,
                    Some(&connection.name.clone()),
                    connection,
                )
                .unwrap();
                app.status_refresh_profiles.clear();
                app.save_connections();
                app.messages.clear();
                app.focus.focus(window, cx);
                original
            })
        })
        .unwrap();
    reopen(view, window, cx).await;
    record(view, window, "initial", &oracle, cx).await;
    enable(view, window, "1.5", cx).await;
    record(view, window, "fraction-enabled", &oracle, cx).await;
    reopen(view, window, cx).await;
    record(view, window, "fraction-reopened", &oracle, cx).await;
    off(view, window, cx).await;
    reopen(view, window, cx).await;
    record(view, window, "off-reopened", &oracle, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let tab = &app.tabs[app.index().unwrap()];
                let connection = flatten(&app.connections)
                    .into_iter()
                    .find(|c| c.name == tab.conn.name)
                    .unwrap();
                app.open_dialog(
                    dialogs::DialogKind::Connection(Some(connection)),
                    window,
                    cx,
                );
                app.set_dialog_field("mark_color", "#123456", window, cx);
                app.set_dialog_field("key_separator", "|", window, cx);
                app.submit_dialog(window, cx);
                assert!(app.dialog.is_none());
            })
        })
        .unwrap();
    for db in [1, 0] {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.change_database(app.active.unwrap(), db, window, cx);
                })
            })
            .unwrap();
        idle(view, window, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                let disk = flatten(&app.store.connections().unwrap())
                    .into_iter()
                    .find(|c| c.name == tab.conn.name)
                    .unwrap();
                assert_eq!(disk.last_db, db);
                assert_eq!(
                    disk.mark_color, "#123456",
                    "database switch must preserve later profile edits"
                );
                assert_eq!(disk.key_separator, "|");
            })
            .unwrap();
    }
    enable(view, window, "3", cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let tab = &app.tabs[app.index().unwrap()];
            let disk = flatten(&app.store.connections().unwrap())
                .into_iter()
                .find(|c| c.name == tab.conn.name)
                .unwrap();
            assert_eq!(disk.mark_color, "#123456");
            assert_eq!(
                disk.key_separator, "|",
                "refresh save must preserve later edits to the profile"
            );
        })
        .unwrap();
    record(view, window, "integer-enabled", &oracle, cx).await;
    reopen(view, window, cx).await;
    record(view, window, "integer-reopened", &oracle, cx).await;
    enable(view, window, "", cx).await;
    record(view, window, "empty-enabled", &oracle, cx).await;
    enable(view, window, "1.5", cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.open_dialog(dialogs::DialogKind::Group(None), window, cx);
                app.set_dialog_field("name", "__rdm_qa_status_profile_reload__", window, cx);
                app.submit_dialog(window, cx);
                assert!(app.dialog.is_none());
            })
        })
        .unwrap();
    reopen(view, window, cx).await;
    record(view, window, "fraction-after-profile-reload", &oracle, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let tab = &app.tabs[app.index().unwrap()];
                let connection = flatten(&app.connections)
                    .into_iter()
                    .find(|c| c.name == tab.conn.name)
                    .unwrap();
                app.open_dialog(
                    dialogs::DialogKind::Connection(Some(connection)),
                    window,
                    cx,
                );
                app.close_dialog(window, cx);
            })
        })
        .unwrap();
    reopen(view, window, cx).await;
    record(view, window, "profile-get-reopened", &oracle, cx).await;
    off(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                let tab = &app.tabs[app.index().unwrap()];
                let mut connection = flatten(&app.connections)
                    .into_iter()
                    .find(|c| c.name == tab.conn.name)
                    .unwrap();
                connection.refresh_interval = 0;
                crate::storage::save_connection(
                    &mut app.connections,
                    Some(&connection.name.clone()),
                    connection,
                )
                .unwrap();
                app.status_refresh_profiles.clear();
                app.save_connections();
            })
        })
        .unwrap();
    reopen(view, window, cx).await;
    record(view, window, "zero-reopened", &oracle, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, _| {
                app.connections = original_connections;
                app.status_refresh_profiles = original_profiles;
                app.save_connections();
            })
        })
        .unwrap();
    reopen(view, window, cx).await;
    if let Some(key) = original_key {
        window
            .update(cx, |_, window, cx| {
                view.update(cx, |app, cx| {
                    app.select_key(app.active.unwrap(), key, window, cx)
                })
            })
            .unwrap();
        idle(view, window, cx).await;
    }
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                assert_eq!(app.tabs[i].conn.name, original_connection.name);
                app.tabs[i].pane = Pane::Status;
                app.messages.clear();
                cx.notify();
            })
        })
        .unwrap();
    println!(
        "QA PASS: 10 original status refresh persistence states, connection reopen, profile reload and zero normalization"
    );
}
