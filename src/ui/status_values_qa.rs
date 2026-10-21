//! Replay original numeric fixtures through the actual tab and status renderer.
use super::qa_json::idle;
use super::*;

pub(super) struct SavedStatus {
    tab_id: u64,
    info: Info,
    samples: VecDeque<Sample>,
    pane: Pane,
    details: bool,
    auto: bool,
}

fn info(raw: &str) -> Info {
    [
        (
            "Server",
            vec![
                ("redis_version", "8.0.0"),
                ("redis_mode", "standalone"),
                ("uptime_in_seconds", "216000"),
            ],
        ),
        ("Replication", vec![("role", "master")]),
        (
            "Stats",
            vec![
                ("instantaneous_ops_per_sec", "12"),
                ("instantaneous_input_kbps", raw),
                ("instantaneous_output_kbps", raw),
            ],
        ),
        ("Clients", vec![("connected_clients", "3")]),
        ("Memory", vec![("used_memory", "1152")]),
        ("Keyspace", vec![("db0", "keys=19,expires=0,avg_ttl=0")]),
    ]
    .into_iter()
    .map(|(section, values)| {
        (
            section.into(),
            values
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    })
    .collect()
}

fn set_rendered_info(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    data: Info,
    cx: &mut AsyncApp,
) {
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.index().unwrap();
            app.tabs[i].info = data;
            cx.notify();
        });
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
}

fn check_label(
    window: WindowHandle<Root>,
    index: usize,
    expected: &str,
    case: &str,
    cx: &mut AsyncApp,
) {
    window.update(cx, |_, window, _| {
        let id = SharedString::from(format!("status-value-{index}"));
        let actual = gpui_base::test_support::find(window, &[], &id.into()).unwrap();
        assert_eq!(actual.label(), Some(expected), "status rendered {case}");
        assert!(actual.bounds().size.width > px(0.) && actual.bounds().size.height > px(0.));
        println!("STATUS VALUE FRAME: {}", serde_json::json!({"case":case,"value":actual.label(),"index":index,"bounds":format!("{:?}",actual.bounds())}));
    }).unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    cx: &mut AsyncApp,
) -> SavedStatus {
    idle(view, window, cx).await;
    let source: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/status-values-source.json"
    ))
    .unwrap();
    let saved = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let tab = &mut app.tabs[i];
                let saved = SavedStatus {
                    tab_id: tab.id,
                    info: tab.info.clone(),
                    samples: std::mem::take(&mut tab.samples),
                    pane: tab.pane,
                    details: tab.info_details,
                    auto: tab.status_auto,
                };
                tab.status_auto = false;
                tab.info_details = false;
                tab.pane = Pane::Status;
                window.blur(cx);
                for row in source["records"]["samples"].as_array().unwrap() {
                    let mut data = info(row["raw"].as_str().unwrap());
                    data.get_mut("Memory")
                        .unwrap()
                        .insert("used_memory".into(), "1048576".into());
                    tab.record_info(data.clone());
                    assert_eq!(tab.info, data);
                    let sample = tab.samples.back().unwrap();
                    assert_eq!(sample.ops, 12.);
                    assert_eq!(sample.clients, 3.);
                    assert_eq!(sample.memory, 1048576.);
                    assert_eq!(sample.input, row["input"].as_f64().unwrap());
                    assert_eq!(sample.output, row["output"].as_f64().unwrap());
                    assert_eq!(
                        tab.samples.len(),
                        row["historyLength"].as_u64().unwrap() as usize
                    );
                }
                for row in source["records"]["integerParsing"].as_array().unwrap() {
                    let raw = row["raw"].as_str().unwrap();
                    let mut data = info(raw);
                    for (section, key) in [
                        ("Stats", "instantaneous_ops_per_sec"),
                        ("Clients", "connected_clients"),
                        ("Memory", "used_memory"),
                    ] {
                        data.get_mut(section)
                            .unwrap()
                            .insert(key.into(), raw.into());
                    }
                    tab.record_info(data);
                    let s = tab.samples.back().unwrap();
                    for actual in [s.ops, s.clients, s.memory, s.input, s.output] {
                        if let Some(expected) = row["value"].as_f64() {
                            assert_eq!(actual, expected, "record_info {raw:?}");
                            assert_eq!(
                                actual == 0. && actual.is_sign_negative(),
                                row["negativeZero"].as_bool().unwrap()
                            );
                        } else {
                            assert!(actual.is_nan(), "record_info {raw:?}");
                        }
                    }
                }
                tab.record_info(Info::new());
                let s = tab.samples.back().unwrap();
                assert_eq!([s.ops, s.clients, s.memory, s.input, s.output], [0.; 5]);
                let empty: Info = [(
                    "Stats".into(),
                    [("instantaneous_ops_per_sec".into(), String::new())].into(),
                )]
                .into();
                tab.record_info(empty);
                assert!(tab.samples.back().unwrap().ops.is_nan());
                tab.samples.clear();
                for row in source["records"]["samples"].as_array().unwrap() {
                    tab.record_info(info(row["raw"].as_str().unwrap()));
                }
                cx.notify();
                saved
            })
        })
        .unwrap();
    for row in source["records"]["uptime"].as_array().unwrap() {
        let seconds = row["seconds"].as_u64().unwrap();
        let mut data = info("2.5");
        data.get_mut("Server")
            .unwrap()
            .insert("uptime_in_seconds".into(), seconds.to_string());
        set_rendered_info(view, window, data, cx);
        let expected = window
            .update(cx, |_, _, cx| {
                format!(
                    "{} {}",
                    row["value"].as_u64().unwrap(),
                    view.read(cx).tr(row["unitKey"].as_str().unwrap())
                )
            })
            .unwrap();
        check_label(window, 0, &expected, &format!("uptime {seconds}"), cx);
    }
    for row in source["records"]["memory"].as_array().unwrap() {
        let bytes = row["bytes"].as_u64().unwrap();
        let mut data = info("2.5");
        data.get_mut("Memory")
            .unwrap()
            .insert("used_memory".into(), bytes.to_string());
        set_rendered_info(view, window, data, cx);
        check_label(
            window,
            3,
            row["expected"].as_str().unwrap(),
            &format!("memory {bytes}"),
            cx,
        );
    }
    set_rendered_info(view, window, info("2.5"), cx);
    check_label(window, 3, "1.13 KB", "final exact-half memory", cx);
    println!(
        "STATUS VALUES QA PASS: actual Tab.record_info six network histories,21 integer prefixes,missing/empty defaults;32 rendered summary values from original numeric fixture;full chart geometry and native acceptance separate"
    );
    saved
}

pub(super) fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: SavedStatus,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.tab_index(saved.tab_id).unwrap();
                let tab = &mut app.tabs[i];
                tab.info = saved.info;
                tab.samples = saved.samples;
                tab.pane = saved.pane;
                tab.info_details = saved.details;
                tab.status_auto = saved.auto;
                cx.notify();
            })
        })
        .unwrap();
}
