//! Client popup geometry and real GPUI mouse events with source-controlled rows.
use super::qa_json::{click, idle};
use super::*;
use std::sync::{Arc, Condvar, Mutex};

// Holds completion after a real disposable Redis response. No production delay,
// fake network rows, or clipboard access. Drop releases the worker on QA failure.
pub(super) struct LoadGate {
    state: Mutex<(Option<Vec<BTreeMap<String, String>>>, bool, usize)>,
    ready: Condvar,
}
impl LoadGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new((None, false, 0)),
            ready: Condvar::new(),
        })
    }
    pub(super) fn hold(&self, rows: Vec<BTreeMap<String, String>>) {
        let mut state = self.state.lock().unwrap();
        state.0 = Some(rows);
        state.2 += 1;
        let _ = self
            .ready
            .wait_timeout_while(state, Duration::from_secs(30), |v| !v.1)
            .unwrap();
    }
    fn observed(&self) -> Option<Vec<BTreeMap<String, String>>> {
        self.state.lock().unwrap().0.clone()
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.ready.notify_all();
    }
}
struct Release(Arc<LoadGate>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

pub(super) struct Saved {
    scene: String,
    id: u64,
    info: Info,
    samples: VecDeque<Sample>,
    clients: Vec<BTreeMap<String, String>>,
    sort: super::status_clients::ClientSort,
    pane: Pane,
    details: bool,
    auto: bool,
    preferences: Preferences,
    viewport: gpui::Size<Pixels>,
    pending: Option<Release>,
}

fn source_rows(row: &Json) -> Vec<BTreeMap<String, String>> {
    row["currentClients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            value
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect()
        })
        .collect()
}

fn install_rows(view: &Entity<RdmApp>, window: WindowHandle<Root>, row: &Json, cx: &mut AsyncApp) {
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                app.tabs[i].clients = source_rows(row);
                cx.notify();
            })
        })
        .unwrap();
}

async fn settled(view: &Entity<RdmApp>, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    idle(view, window, cx).await;
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        for _ in 0..2 {
            window.refresh();
            let _ = window.draw(cx);
        }
    })
    .unwrap();
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    idle(view, window, cx).await;
    let source: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/status-clients-macos-source.json"
    ))
    .unwrap();
    let context: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/status-clients-context-macos-source.json"
    ))
    .unwrap();
    let extended: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/status-clients-extended-macos-source.json"
    ))
    .unwrap();
    let row = source["records"]
        .as_array()
        .unwrap()
        .iter()
        .chain(extended["records"].as_array().unwrap().iter())
        .find(|v| v["name"] == scene)
        .unwrap();
    println!("STATUS CLIENTS START: {scene}");
    let mut saved = cx
        .update_window(window.into(), |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let preferences = app.pref.clone();
                let tab = &mut app.tabs[i];
                let saved = Saved {
                    scene: scene.to_string(),
                    id: tab.id,
                    info: std::mem::take(&mut tab.info),
                    samples: std::mem::take(&mut tab.samples),
                    clients: std::mem::take(&mut tab.clients),
                    sort: tab.client_sort.clone(),
                    pane: tab.pane,
                    details: tab.info_details,
                    auto: tab.status_auto,
                    preferences,
                    viewport: window.viewport_size(),
                    pending: None,
                };
                let server_info = row
                    .get("extra")
                    .and_then(|extra| extra.get("serverInfo"))
                    .unwrap_or(&context["serverInfo"]);
                tab.info = server_info
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(group, values)| {
                        (
                            group.clone(),
                            values
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                                .collect(),
                        )
                    })
                    .collect();
                tab.info.get_mut("Clients").unwrap().insert(
                    "connected_clients".into(),
                    source_rows(row).len().to_string(),
                );
                tab.pane = Pane::Status;
                tab.info_details = false;
                tab.status_auto = false;
                tab.client_sort = Default::default();
                let c = &row["configuration"];
                app.pref.general.theme = c["theme"].as_str().unwrap().into();
                let family = c["family"].as_str().unwrap();
                app.pref.general.font_family = if family.is_empty() {
                    vec![]
                } else {
                    vec![family.into()]
                };
                app.pref.general.font_size = c["size"].as_f64().unwrap() as f32;
                app.general_css_size = app.pref.general.font_size;
                app.pref.behavior.aside_width = 300.;
                app.nav = Nav::Browser;
                app.apply_theme(window, cx);
                window.resize(size(
                    px(c["windowWidth"].as_f64().unwrap() as f32),
                    px(768.),
                ));
                window.blur(cx);
                cx.notify();
                saved
            })
        })
        .unwrap();
    settled(view, window, cx).await;
    click("show-clients", window, cx);
    settled(view, window, cx).await;
    let mut service_loads = vec![];
    let actual = window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            app.tabs[app.index().unwrap()].clients.clone()
        })
        .unwrap();
    assert!(
        !actual.is_empty(),
        "Own Redis CLIENT LIST request must complete before installing controlled visual rows"
    );
    service_loads.push(serde_json::json!({"action":"open","rows":actual}));
    install_rows(view, window, row, cx);
    settled(view, window, cx).await;
    for _ in 0..row["configuration"]["clicks"].as_u64().unwrap() {
        click(
            &format!(
                "status-clients-header-{}",
                row["configuration"]["sort"].as_str().unwrap()
            ),
            window,
            cx,
        );
        settled(view, window, cx).await;
    }
    if row["configuration"]["refresh"].as_bool() == Some(true) {
        click("refresh-clients", window, cx);
        settled(view, window, cx).await;
        let actual = window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.tabs[app.index().unwrap()].clients.clone()
            })
            .unwrap();
        assert!(
            !actual.is_empty() && actual != source_rows(row),
            "Refresh must replace the controlled rows with own Redis CLIENT LIST data"
        );
        service_loads.push(serde_json::json!({"action":"refresh","rows":actual}));
        install_rows(view, window, row, cx);
        settled(view, window, cx).await;
    }
    if row["configuration"]["reopen"].as_bool() == Some(true) {
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        cx.update_window(window.into(), |_, window, _| {
            assert!(
                gpui_base::test_support::find(window, &[], &"status-clients-table".into())
                    .is_none(),
                "Trigger closes the current popup"
            );
        })
        .unwrap();
        click("show-clients", window, cx);
        settled(view, window, cx).await;
        let actual = window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.tabs[app.index().unwrap()].clients.clone()
            })
            .unwrap();
        assert!(!actual.is_empty() && actual != source_rows(row));
        service_loads.push(serde_json::json!({"action":"reopen","rows":actual}));
        install_rows(view, window, row, cx);
        settled(view, window, cx).await;
    }
    let mut scroll_proof = Json::Null;
    if row["configuration"]["kind"].as_str() == Some("overflow") {
        cx.update_window(window.into(), |_, window, cx| {
            let body=gpui_base::test_support::find(window,&[],&"clients-scroll".into()).unwrap().bounds();
            let first=gpui_base::test_support::find(window,&[],&"status-clients-row-0".into()).unwrap().bounds();
            let position=body.center();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,..Default::default()}),cx);
            window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent{
                position,delta:ScrollDelta::Pixels(point(px(0.),px(-10000.))),modifiers:Default::default(),touch_phase:TouchPhase::Moved,
            }),cx);
            for _ in 0..2 {window.refresh();let _=window.draw(cx);}
            let shifted=gpui_base::test_support::find(window,&[],&"status-clients-row-0".into()).unwrap().bounds();
            let last=gpui_base::test_support::find(window,&[],&"status-clients-row-23".into()).unwrap().bounds();
            assert!((body.bottom()-last.bottom()).as_f32().abs()<0.6,"Wheel must reach the final client row");
            assert!(shifted.top()<first.top());
            window.dispatch_event(PlatformInput::ScrollWheel(ScrollWheelEvent{
                position,delta:ScrollDelta::Pixels(point(px(0.),px(10000.))),modifiers:Default::default(),touch_phase:TouchPhase::Moved,
            }),cx);
            for _ in 0..2 {window.refresh();let _=window.draw(cx);}
            let restored=gpui_base::test_support::find(window,&[],&"status-clients-row-0".into()).unwrap().bounds();
            assert!((restored.top()-first.top()).as_f32().abs()<0.6);
            scroll_proof=serde_json::json!({"actualWheelToEndAndBack":true,"maximumOffset":(first.top()-shifted.top()).as_f32(),"lastBottom":last.bottom().as_f32(),"bodyBottom":body.bottom().as_f32(),"initialFirstTop":first.top().as_f32(),"restoredFirstTop":restored.top().as_f32()});
        }).unwrap();
    }
    let mut pending_proof = Json::Null;
    if row["configuration"]["loading"].as_bool() == Some(true) {
        let gate = LoadGate::new();
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, _| {
                    let i = app.index().unwrap();
                    app.tabs[i].client_load_gate = Some(gate.clone());
                })
            })
            .unwrap();
        saved.pending = Some(Release(gate.clone()));
        click("refresh-clients", window, cx);
        for _ in 0..100 {
            if gate.observed().is_some() {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(20))
                .await;
        }
        let actual = gate
            .observed()
            .expect("Own Redis response must reach the completion gate");
        assert!(!actual.is_empty() && actual != source_rows(row));
        service_loads.push(serde_json::json!({"action":"pending-refresh","rows":actual}));
        click("status-clients-header-addr", window, cx);
        click("refresh-clients", window, cx);
        let old_rows = source_rows(row);
        window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                let tab = &app.tabs[app.index().unwrap()];
                assert!(tab.clients_loading && tab.clients == old_rows);
                assert!(tab.client_sort.state() == Some(("age", "descend")));
                assert_eq!(gate.state.lock().unwrap().2, 1);
            })
            .unwrap();
        pending_proof = serde_json::json!({"oldRowsRetained":true,"sortAndRefreshBlocked":true,"actualOwnResponseHeld":true,"spinnerSeconds":0.4});
    }
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position:point(px(20.),px(760.)), ..Default::default() }),cx);
        for _ in 0..2 { window.refresh(); let _ = window.draw(cx); }
        let mut ids:Vec<String> = ["status-summary-grid","status-stat-1","status-stat-row-1","status-value-1","status-clients-popup","status-clients-table","status-clients-title","status-clients-headers","clients-scroll","status-clients-empty","status-clients-empty-icon","status-clients-empty-description","status-clients-loading","refresh-clients","show-clients"].into_iter().map(str::to_string).collect();
        for key in ["no","addr","db","age","idle"] { ids.push(format!("status-clients-header-{key}")); }
        for i in 0..row["currentClients"].as_array().unwrap().len() { ids.push(format!("status-clients-row-{i}")); for j in 0..5 { ids.push(format!("status-clients-cell-{i}-{j}")); for k in 0..30 { ids.push(format!("status-clients-line-{i}-{j}-{k}")); } } }
        let frames:Vec<_> = ids.into_iter().filter_map(|id| gpui_base::test_support::find(window,&[],&id.clone().into()).map(|node| {
            let b=node.bounds();serde_json::json!({"id":id,"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32(),"label":node.label(),"supportsClick":node.supports_click()})
        })).collect();
        assert!(frames.iter().any(|f|f["id"]=="status-clients-table"), "Client popup must actually be rendered");
        let app=view.read(cx);let tab=&app.tabs[app.index().unwrap()];
        let general=app.preference_font("general",".SystemUIFont",cx);
        let count=tab.info.get("Clients").unwrap().get("connected_clients").unwrap();
        let inline_widths=serde_json::json!({"number":super::text::script_width(count,general.clone(),24.,window),"button":super::text::script_width("↘",general,14.,window)+16.});
        println!("STATUS CLIENTS FRAME: {}",serde_json::json!({"scene":scene,"frames":frames,"inlineWidths":inline_widths,"sort":tab.client_sort.state().map(|(field,order)|serde_json::json!({"field":field,"order":order})),"serviceLoads":service_loads,"loading":tab.clients_loading,"pendingProof":pending_proof,"scrollProof":scroll_proof,"scope":"Actual GPUI trigger/sort/refresh/reopen andwheel events. Own Redis responses observed then replaced with source-controlled rows for geometry/paint. Pending case holds completion after a real own Redis response,retains controlled old rows and blocks actual sort/refresh clicks;not native input acceptance."}));
    }).unwrap();
    saved
}

pub(super) async fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    mut saved: Saved,
    cx: &mut AsyncApp,
) {
    if let Some(release) = saved.pending.take() {
        let expected = release.0.observed().unwrap();
        drop(release);
        idle(view, window, cx).await;
        window.update(cx, |_, _, cx| view.update(cx, |app, _| {
            let i=app.index().unwrap();let tab=&mut app.tabs[i];
            assert!(!tab.clients_loading && tab.clients==expected);
            tab.client_load_gate=None;
            println!("STATUS CLIENTS PENDING COMPLETION: {}",serde_json::json!({"scene":saved.scene,"tab":tab.id,"loadingCleared":true,"actualOwnRowsApplied":true,"rows":tab.clients,"gateRemoved":true}));
        })).unwrap();
    }
    click("show-clients", window, cx);
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.tab_index(saved.id).unwrap();
            let tab = &mut app.tabs[i];
            tab.info = saved.info;
            tab.samples = saved.samples;
            tab.clients = saved.clients;
            tab.client_sort = saved.sort;
            tab.pane = saved.pane;
            tab.info_details = saved.details;
            tab.status_auto = saved.auto;
            app.pref = saved.preferences;
            app.general_css_size = app.pref.general.font_size;
            app.apply_theme(window, cx);
            window.resize(saved.viewport);
            cx.notify();
        })
    })
    .unwrap();
}
