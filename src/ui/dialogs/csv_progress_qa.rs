use super::*;

fn fixture() -> Json {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/csv-progress-source.json"
    ))
    .unwrap()
}
fn record(scene: &str) -> Json {
    let scene = match scene {
        "csv-progress-initial" | "csv-export-progress-loading" => "csv-export-progress-success",
        "csv-import-progress-loading" => "csv-import-progress-success",
        other => other,
    };
    fixture()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == scene)
        .unwrap()
        .clone()
}
impl RdmApp {
    pub(in crate::ui) fn qa_prepare_csv_progress(&mut self, scene: &str, cx: &mut Context<Self>) {
        self.csv_transfers.clear();
        while self.csv_progress.1.try_recv().is_ok() {}
        self.messages.clear();
        let record = record(scene);
        let kind = if record["kind"] == "import" {
            CsvKind::Import
        } else {
            CsvKind::Export
        };
        let message = self.csv_loading_message();
        if scene == "csv-progress-initial" || scene.ends_with("-loading") {
            let path = "/tmp/source.csv".to_string();
            self.csv_transfers.insert(
                message,
                CsvTransfer {
                    kind,
                    path: path.clone(),
                    cancel: Cancellation::default(),
                },
            );
            if scene != "csv-progress-initial" {
                let report = if kind == CsvKind::Export {
                    TransferProgress {
                        current: 1,
                        total: 3,
                        ..Default::default()
                    }
                } else {
                    TransferProgress {
                        done: 1,
                        skipped: 2,
                        ..Default::default()
                    }
                };
                self.csv_progress
                    .0
                    .send(CsvProgressEvent { kind, path, report })
                    .unwrap();
                assert!(self.poll_csv_transfers());
            }
        } else {
            self.destroy_message(message);
            let response = &record["response"];
            let result = if response["success"] == false {
                Outcome::Failed(response["msg"].as_str().unwrap().into())
            } else {
                Outcome::Completed(TransferProgress {
                    done: response["data"][if kind == CsvKind::Import {
                        "imported"
                    } else {
                        "exported"
                    }]
                    .as_u64()
                    .unwrap() as usize,
                    failed: response["data"][if kind == CsvKind::Import {
                        "ignored"
                    } else {
                        "failed"
                    }]
                    .as_u64()
                    .unwrap() as usize,
                    canceled: response["data"]["canceled"].as_bool().unwrap(),
                    ..Default::default()
                })
            };
            for (level, text) in
                completion(kind, record["reload"] == true, result, |key| self.tr(key)).messages
            {
                self.add_message(text, level);
            }
        }
        cx.notify();
    }
    pub(in crate::ui) fn qa_assert_csv_progress(&self, scene: &str, window: &mut Window, _: &App) {
        let record = record(scene);
        let expected = &record[if scene == "csv-progress-initial" {
            "initial"
        } else if scene.ends_with("-loading") {
            "progress"
        } else {
            "completed"
        }];
        let expected = expected.as_array().unwrap();
        assert_eq!(self.messages.len(), expected.len(), "{scene}");
        for (notice, expected) in self.messages.iter().zip(expected) {
            assert_eq!(notice.text, expected["text"].as_str().unwrap());
            assert_eq!(notice.closable, expected["close"].is_object());
            let bounds = gpui_base::test_support::find(window, &[], &("message", notice.id).into())
                .unwrap()
                .bounds();
            for (key, actual) in [
                ("x", bounds.origin.x),
                ("y", bounds.origin.y),
                ("width", bounds.size.width),
                ("height", bounds.size.height),
            ] {
                let wanted = expected["bounds"][key].as_f64().unwrap() as f32;
                assert!(
                    (actual.as_f32() - wanted).abs() < 0.7,
                    "{scene} {key}: {actual:?} vs {wanted}"
                );
            }
            if notice.closable {
                let bounds = gpui_base::test_support::find(
                    window,
                    &[],
                    &("message-close", notice.id).into(),
                )
                .unwrap()
                .bounds();
                for (key, actual) in [
                    ("x", bounds.origin.x),
                    ("y", bounds.origin.y),
                    ("width", bounds.size.width),
                    ("height", bounds.size.height),
                ] {
                    let wanted = expected["close"]["bounds"][key].as_f64().unwrap() as f32;
                    assert!(
                        (actual.as_f32() - wanted).abs() < 0.7,
                        "{scene} close {key}: {actual:?} vs {wanted}"
                    );
                }
            }
        }
    }
}

async fn idle(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    for _ in 0..250 {
        if window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.csv_transfers.is_empty() && app.busy == 0
            })
            .unwrap()
        {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("CSV transfer did not settle");
}
fn click(element: ElementId, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        let position = gpui_base::test_support::find(window, &[], &element)
            .unwrap()
            .bounds()
            .center();
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
}
fn confirm(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    click("dialog-save".into(), window, cx);
    window
        .update(cx, |_, _, cx| {
            assert!(
                view.read(cx).dialog.is_none(),
                "CSV form must close immediately on confirmation"
            );
        })
        .unwrap();
}

pub(in crate::ui) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("transfer.csv");
    let key = b"csv-ui:source".to_vec();
    let (service, config) = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.csv_transfers.clear();
                app.messages.clear();
                let config = app.connections[0].clone();
                assert_eq!(config.network, "unix");
                let root = PathBuf::from(&config.sock).parent().unwrap().to_path_buf();
                assert_eq!(
                    std::fs::read_to_string(root.join("RDM-QA-FIXTURE")).unwrap(),
                    "rust-rdm-render-fixture-v1\n"
                );
                assert_eq!(
                    app.service
                        .run(&config, 15, command(&["DBSIZE"]))
                        .unwrap()
                        .integer()
                        .unwrap(),
                    0,
                    "CSV UI QA requires an empty owned DB15"
                );
                app.service
                    .run(
                        &config,
                        15,
                        vec![b"SET".to_vec(), key.clone(), b"CSV UI fixture".to_vec()],
                    )
                    .unwrap();
                app.open_connection(config.clone(), window, cx);
                (app.service.clone(), config)
            })
        })
        .unwrap();
    idle(view, window, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let index = app.index().unwrap();
                app.tabs[index].db = 15;
                app.tabs[index].current_key = Some(key.clone());
                app.tabs[index].selected.clear();
                app.open_dialog(DialogKind::Export, window, cx);
                app.set_dialog_field("path", path.to_str().unwrap(), window, cx);
            })
        })
        .unwrap();
    confirm(view, window, cx);
    idle(view, window, cx).await;
    let rows = ::csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(&path)
        .unwrap()
        .records()
        .map(|row| row.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert_eq!(&rows[0][0], hex::encode(&key));
    assert!(!rows[0][1].is_empty());
    service
        .run(
            &config,
            15,
            vec![b"SET".to_vec(), key.clone(), b"before".to_vec()],
        )
        .unwrap();
    let epoch = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                let epoch = app.tabs[app.index().unwrap()].scan_epoch;
                app.open_dialog(DialogKind::Import, window, cx);
                for (field, value) in [
                    ("path", path.to_str().unwrap()),
                    ("ttl_type", "2"),
                    ("ttl", "2.5"),
                    ("unit", "60"),
                    ("reload", "false"),
                ] {
                    app.set_dialog_field(field, value, window, cx);
                }
                epoch
            })
        })
        .unwrap();
    confirm(view, window, cx);
    idle(view, window, cx).await;
    assert_eq!(
        service
            .run(&config, 15, vec![b"GET".to_vec(), key.clone()])
            .unwrap()
            .bytes(),
        b"CSV UI fixture"
    );
    let ttl = service
        .run(&config, 15, vec![b"PTTL".to_vec(), key.clone()])
        .unwrap()
        .integer()
        .unwrap();
    assert!((145_000..=150_000).contains(&ttl), "2.5 minute TTL: {ttl}");
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.tabs[app.index().unwrap()].scan_epoch,
                epoch,
                "reload=false must preserve browser scan"
            );
        })
        .unwrap();
    service
        .run(
            &config,
            15,
            vec![b"SET".to_vec(), key.clone(), b"keep".to_vec()],
        )
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.open_dialog(DialogKind::Import, window, cx);
                app.set_dialog_field("path", path.to_str().unwrap(), window, cx);
                app.set_dialog_field("conflict", "1", window, cx);
            })
        })
        .unwrap();
    confirm(view, window, cx);
    idle(view, window, cx).await;
    assert_eq!(
        service
            .run(&config, 15, vec![b"GET".to_vec(), key.clone()])
            .unwrap()
            .bytes(),
        b"keep"
    );
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(
                app.tabs[app.index().unwrap()].scan_epoch > epoch,
                "reload=true must rescan"
            );
            assert_eq!(
                app.messages.last().unwrap().text,
                "已完成导入操作，成功0个，忽略1个"
            );
        })
        .unwrap();
    // A fractional number of seconds rejects the int64 RPC; the Source caller
    // swallows that rejection after closing the form, without reloading.
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.open_dialog(DialogKind::Import, window, cx);
                for (field, value) in [
                    ("path", path.to_str().unwrap()),
                    ("ttl_type", "2"),
                    ("ttl", "2.5"),
                    ("unit", "1"),
                ] {
                    app.set_dialog_field(field, value, window, cx);
                }
            })
        })
        .unwrap();
    confirm(view, window, cx);
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            assert!(
                view.read(cx).messages.is_empty(),
                "rejected RPC must skip completion messages"
            )
        })
        .unwrap();
    assert_eq!(
        service
            .run(&config, 15, vec![b"GET".to_vec(), key.clone()])
            .unwrap()
            .bytes(),
        b"keep"
    );
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                app.open_dialog(DialogKind::Import, window, cx);
                app.set_dialog_field(
                    "path",
                    temporary.path().join("missing.csv").to_str().unwrap(),
                    window,
                    cx,
                );
            })
        })
        .unwrap();
    confirm(view, window, cx);
    idle(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(app.messages.len(), 2);
            assert_eq!(app.messages[0].kind, MessageKind::Error);
            assert_eq!(app.messages[1].kind, MessageKind::Success);
            assert_eq!(app.messages[1].text, "已完成导入操作，成功0个，忽略0个");
        })
        .unwrap();
    // Actual close-button events exercise the same event scope as Source Wails.
    let (ids, tokens) = window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.messages.clear();
                let mut ids = vec![];
                let mut tokens = vec![];
                for (kind, path) in [
                    (CsvKind::Export, "/tmp/source.csv"),
                    (CsvKind::Export, "/tmp/source.csv"),
                    (CsvKind::Export, "/tmp/./source.csv"),
                    (CsvKind::Import, "/tmp/source.csv"),
                ] {
                    let id = app.csv_loading_message();
                    let cancel = Cancellation::default();
                    app.csv_transfers.insert(
                        id,
                        CsvTransfer {
                            kind,
                            path: path.into(),
                            cancel: cancel.clone(),
                        },
                    );
                    ids.push(id);
                    tokens.push(cancel);
                }
                app.csv_progress
                    .0
                    .send(CsvProgressEvent {
                        kind: CsvKind::Export,
                        path: "/tmp/source.csv".into(),
                        report: TransferProgress {
                            current: 1,
                            total: 3,
                            ..Default::default()
                        },
                    })
                    .unwrap();
                app.poll_csv_transfers();
                assert_eq!(app.messages[0].text, app.messages[1].text);
                assert_eq!(app.messages[2].text, "");
                assert_eq!(app.messages[3].text, "");
                cx.notify();
                (ids, tokens)
            })
        })
        .unwrap();
    click(("message-close", ids[0]).into(), window, cx);
    assert!(tokens[0].canceled() && tokens[1].canceled());
    assert!(!tokens[2].canceled() && !tokens[3].canceled());
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                assert_eq!(app.messages.len(), 3);
                assert_eq!(
                    app.csv_transfers.len(),
                    4,
                    "closing the message does not finish the RPC"
                );
                app.csv_transfers.clear();
                app.messages.clear();
                let index = app.index().unwrap();
                app.tabs[index].db = 0;
                app.tabs[index].current_key = None;
                let id = app.tabs[index].id;
                app.scan_keys(id, true, false, window, cx);
            })
        })
        .unwrap();
    service
        .run(&config, 15, vec![b"DEL".to_vec(), key])
        .unwrap();
    assert_eq!(
        service
            .run(&config, 15, command(&["DBSIZE"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
    idle(view, window, cx).await;
    println!(
        "QA PASS: real CSV confirmation closes forms immediately; export, 2.5 minute TTL, ignore conflict, reload switch, rejected RPC, failed import summary, grouped close cancellation and raw event path identity"
    );
}
