//! Bounded rendering and ownership checks against disposable Redis only.
use super::qa_json::click;
use super::status_clients::{ClientRows, Column, SORT_COUNT, sorted_indices};
use super::status_clients_qa::{LoadGate, settled};
use super::*;
use std::sync::{Weak, atomic::Ordering};

fn rows(count: usize) -> ClientRows {
    (0..count)
        .map(|i| {
            [
                ("addr".into(), format!("client-{i:05}.local:6000")),
                ("db".into(), (i % 16).to_string()),
                ("age".into(), (count - i).to_string()),
                ("idle".into(), i.to_string()),
            ]
            .into()
        })
        .collect()
}

fn mounted(window: &Window) -> usize {
    gpui_base::test_support::snapshots(window).iter().filter(|node| {
        matches!(node.path().last(), Some(ElementId::Name(id)) if id.starts_with("status-clients-row-"))
    }).count()
}

fn rss_kib() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("sample own QA process memory");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

async fn install(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    count: usize,
    cx: &mut AsyncApp,
) -> Weak<ClientRows> {
    let sort = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.tabs[app.index().unwrap()]
                .clients
                .as_ref()
                .unwrap()
                .sort
        })
        .unwrap();
    let (rows, order) = cx
        .background_executor()
        .spawn(async move {
            let rows = Arc::new(rows(count));
            let order = sorted_indices(&rows, sort);
            (rows, order)
        })
        .await;
    let weak = Arc::downgrade(&rows);
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let clients = app.tabs[i].clients.as_mut().unwrap();
                clients.list = ListState::new(0, ListAlignment::Top, px(100.));
                clients.replace_rows(rows, order);
                cx.notify();
            })
        })
        .unwrap();
    settled(view, window, cx).await;
    weak
}

fn draw(window: WindowHandle<Root>, cx: &mut AsyncApp) -> f64 {
    cx.update_window(window.into(), |_, window, cx| {
        let start = Instant::now();
        for _ in 0..3 {
            window.refresh();
            window.draw(cx).clear(cx);
        }
        assert!(
            mounted(window) <= 64,
            "Client rows must stay bounded by the viewport"
        );
        start.elapsed().as_secs_f64() * 1000. / 3.
    })
    .unwrap()
}

fn wheel(window: WindowHandle<Root>, delta: f32, cx: &mut AsyncApp) {
    cx.update_window(window.into(), |_, window, cx| {
        let position = gpui_base::test_support::find(window, &[], &"clients-scroll".into())
            .unwrap()
            .bounds()
            .center();
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            }),
            cx,
        );
        for _ in 0..3 {
            window.refresh();
            window.draw(cx).clear(cx);
        }
        assert!(mounted(window) <= 64);
    })
    .unwrap();
}

async fn released(weak: &Weak<ClientRows>, cx: &mut AsyncApp) {
    for _ in 0..250 {
        if weak.upgrade().is_none() {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    panic!("Closed client snapshot is still owned after its worker finished");
}

async fn hold(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    sort: bool,
    cx: &mut AsyncApp,
) -> Arc<LoadGate> {
    let gate = LoadGate::new();
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.index().unwrap();
            app.tabs[i].clients.as_mut().unwrap().gate = Some(gate.clone());
            app.update_clients(app.tabs[i].id, sort.then_some(Column::Address), window, cx);
        })
    })
    .unwrap();
    for _ in 0..250 {
        if gate.observed().is_some() {
            return gate;
        }
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    gate.release();
    panic!("Background client work did not reach the completion gate");
}

async fn release(gate: Arc<LoadGate>, cx: &mut AsyncApp) {
    let weak = Arc::downgrade(&gate.take_observed().unwrap());
    gate.release();
    released(&weak, cx).await;
}

pub(super) async fn verify(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let original_info = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.tabs[app.index().unwrap()].info.clone()
        })
        .unwrap();
    for count in [1_000, 10_000] {
        let weak = install(view, window, count, cx).await;
        let sorts = SORT_COUNT.load(Ordering::Relaxed);
        let frame_ms = draw(window, cx);
        wheel(window, -(count as f32 * 17.), cx);
        window
            .update(cx, |_, window, _| {
                assert!(
                    gpui_base::test_support::find(window, &[], &"status-clients-row-0".into())
                        .is_none()
                );
            })
            .unwrap();
        for _ in 0..8 {
            wheel(window, -10_000_000., cx);
            if window
                .update(cx, |_, window, _| {
                    gpui_base::test_support::find(
                        window,
                        &[],
                        &format!("status-clients-row-{}", count - 1).into(),
                    )
                    .is_some_and(|last| {
                        let body =
                            gpui_base::test_support::find(window, &[], &"clients-scroll".into())
                                .unwrap()
                                .bounds();
                        (body.bottom() - last.bounds().bottom()).as_f32().abs() < 0.6
                    })
                })
                .unwrap()
            {
                break;
            }
        }
        window
            .update(cx, |_, window, cx| {
                let last = gpui_base::test_support::find(
                    window,
                    &[],
                    &format!("status-clients-row-{}", count - 1).into(),
                )
                .unwrap_or_else(|| {
                    let app = view.read(cx);
                    let clients = app.tabs[app.index().unwrap()].clients.as_ref().unwrap();
                    let nodes: Vec<_> = gpui_base::test_support::snapshots(window).into_iter().filter(|node| {
                        matches!(node.path().last(), Some(ElementId::Name(id)) if id.starts_with("status-clients-row-"))
                    }).collect();
                    panic!("Missing final client row; offset={:?}, nodes={nodes:?}", clients.list.logical_scroll_top());
                });
                assert!(
                    last.label()
                        .unwrap()
                        .contains(&format!("client-{:05}", count - 1))
                );
                let body = gpui_base::test_support::find(window, &[], &"clients-scroll".into())
                    .unwrap()
                    .bounds();
                assert!((body.bottom() - last.bounds().bottom()).as_f32().abs() < 0.6);
            })
            .unwrap();
        wheel(window, 10_000_000., cx);
        assert_eq!(
            sorts,
            SORT_COUNT.load(Ordering::Relaxed),
            "Painting and scrolling must not sort"
        );
        window.update(cx, |_, window, _| {
            assert!(gpui_base::test_support::find(window, &[], &"status-clients-row-0".into()).is_some());
            println!("STATUS CLIENTS LARGE: {}", serde_json::json!({"rows":count,"mounted":mounted(window),"averageForcedFrameMs":frame_ms,"noSortDuringScroll":true,"firstAndLastReachable":true}));
        }).unwrap();
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        released(&weak, cx).await;
        click("show-clients", window, cx);
        settled(view, window, cx).await;
    }

    let mut memory = vec![];
    for cycle in 0..20 {
        let weak = install(view, window, 10_000, cx).await;
        let lifetime = window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                Arc::downgrade(
                    &app.tabs[app.index().unwrap()]
                        .clients
                        .as_ref()
                        .unwrap()
                        .lifetime,
                )
            })
            .unwrap();
        wheel(window, -170_000., cx);
        wheel(window, -10_000_000., cx);
        let open_rss = rss_kib();
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        released(&weak, cx).await;
        assert!(lifetime.upgrade().is_none());
        window
            .update(cx, |_, window, cx| {
                let app = view.read(cx);
                assert!(app.tabs[app.index().unwrap()].clients.is_none());
                assert_eq!(mounted(window), 0);
            })
            .unwrap();
        memory.push(serde_json::json!({"cycle":cycle + 1,"openRssKiB":open_rss,"closedRssKiB":rss_kib(),"snapshotAndStateReleased":true}));
        click("show-clients", window, cx);
        settled(view, window, cx).await;
    }
    let average = |range: std::ops::Range<usize>| {
        range
            .map(|i| memory[i]["closedRssKiB"].as_u64().unwrap())
            .sum::<u64>() as f64
            / 5.
    };
    let growth = average(15..20) - average(5..10);
    println!(
        "STATUS CLIENTS MEMORY: {}",
        serde_json::json!({"cycles":memory,"closedGrowthKiBAfterWarmup":growth,"maximumAllowedGrowthKiB":32768,"observedPeakRssKiB":memory.iter().map(|v|v["openRssKiB"].as_u64().unwrap()).max(),"allSnapshotsAndStatesReleased":true})
    );
    assert!(growth < 32_768., "Closed memory must plateau after warmup");

    // Failed refresh retains the exact old snapshot and sorting indices.
    let weak = install(view, window, 1_000, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.index().unwrap();
            let original = app.tabs[i].conn.clone();
            app.tabs[i].conn.name.push_str("-clients-missing-socket");
            app.tabs[i].conn.sock.push_str(".missing");
            app.update_clients(app.tabs[i].id, None, window, cx);
            app.tabs[i].conn = original;
        })
    })
    .unwrap();
    settled(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            let clients = app.tabs[app.index().unwrap()].clients.as_ref().unwrap();
            assert!(Arc::ptr_eq(&clients.rows, &weak.upgrade().unwrap()));
            assert_eq!(clients.order.len(), 1_000);
            assert!(!clients.loading);
        })
        .unwrap();

    // A late fetch/sort must not populate a closed or newly reopened popup.
    for sort in [false, true] {
        let gate = hold(view, window, sort, cx).await;
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        let replacement = window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.tabs[app.index().unwrap()]
                    .clients
                    .as_ref()
                    .unwrap()
                    .rows
                    .clone()
            })
            .unwrap();
        release(gate, cx).await;
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert!(Arc::ptr_eq(
                    &app.tabs[app.index().unwrap()]
                        .clients
                        .as_ref()
                        .unwrap()
                        .rows,
                    &replacement
                ));
            })
            .unwrap();
    }

    // Leaving the status view also drops the popup session.
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.tabs[app.index().unwrap()].info,
                original_info,
                "Client operations must not change INFO counts"
            );
        })
        .unwrap();
    let weak = install(view, window, 1_000, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            app.switch_pane(app.active.unwrap(), Pane::Cli, window, cx);
        })
    })
    .unwrap();
    settled(view, window, cx).await;
    released(&weak, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            app.switch_pane(app.active.unwrap(), Pane::Status, window, cx);
        })
    })
    .unwrap();
    settled(view, window, cx).await;

    // Close an owned secondary connection while its client request is pending.
    let original_id = window
        .update(cx, |_, _, cx| view.read(cx).active.unwrap())
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let mut connection = app.tabs[app.index().unwrap()].conn.clone();
            connection.name.push_str("-clients-close-qa");
            app.open_connection(connection, window, cx);
            let i = app.index().unwrap();
            app.tabs[i].status_auto = false;
        })
    })
    .unwrap();
    settled(view, window, cx).await;
    click("show-clients", window, cx);
    settled(view, window, cx).await;
    let gate = hold(view, window, false, cx).await;
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            app.close_tab(app.active.unwrap(), window, cx);
        })
    })
    .unwrap();
    release(gate, cx).await;
    settled(view, window, cx).await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.activate_connection_tab(Some(original_id), cx);
                let i = app.index().unwrap();
                assert!(app.tabs[i].clients.is_none());
                app.tabs[i].info = original_info;
                app.messages.clear();
                app.notifications.clear();
            })
        })
        .unwrap();
    click("show-clients", window, cx);
    settled(view, window, cx).await;
    println!(
        "STATUS CLIENTS LIFECYCLE PASS: failed refresh, late fetch/sort after close and reopen, pane change, pending connection close, INFO unchanged"
    );
}
