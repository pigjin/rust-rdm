//! Controlled original status payloads through the actual GPUI renderer.
use super::qa_json::idle;
use super::*;

pub(super) struct Saved {
    tab_id: u64,
    info: Info,
    samples: VecDeque<Sample>,
    pane: Pane,
    details: bool,
    auto: bool,
    preferences: Preferences,
}

fn source(scene: &str) -> Json {
    for text in [
        include_str!("../../tests/fixtures/status-layout-macos-source.json"),
        include_str!("../../tests/fixtures/status-large-numeric-macos-source.json"),
    ] {
        let fixture: Json = serde_json::from_str(text).unwrap();
        if let Some(row) = fixture["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == scene)
        {
            return row.clone();
        }
    }
    panic!("Missing original status fixture {scene}")
}

fn original_info(row: &Json) -> Info {
    row["serverInfo"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(section, values)| {
            (
                section.clone(),
                values
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
                    .collect(),
            )
        })
        .collect()
}

pub(super) async fn verify(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    scene: &str,
    cx: &mut AsyncApp,
) -> Saved {
    idle(view, window, cx).await;
    let row = source(scene);
    println!("STATUS LAYOUT START: {scene}");
    let saved = cx
        .update_window(window.into(), |_, window, cx| {
            view.update(cx, |app, cx| {
                let i = app.index().unwrap();
                let preferences = app.pref.clone();
                let tab = &mut app.tabs[i];
                let saved = Saved {
                    tab_id: tab.id,
                    info: std::mem::take(&mut tab.info),
                    samples: std::mem::take(&mut tab.samples),
                    pane: tab.pane,
                    details: tab.info_details,
                    auto: tab.status_auto,
                    preferences,
                };
                tab.pane = Pane::Status;
                tab.status_auto = false;
                tab.info_details = false;
                let data = original_info(&row);
                let charts = row["charts"].as_array().unwrap();
                let labels = charts[0]["data"]["labels"].as_array().unwrap();
                for (index, label) in labels.iter().enumerate() {
                    let mut sample = data.clone();
                    for (chart, dataset, section, key) in [
                        (0, 0, "Stats", "instantaneous_ops_per_sec"),
                        (1, 0, "Clients", "connected_clients"),
                        (2, 0, "Memory", "used_memory"),
                        (3, 0, "Stats", "instantaneous_input_kbps"),
                        (3, 1, "Stats", "instantaneous_output_kbps"),
                    ] {
                        let value = &charts[chart]["data"]["datasets"][dataset]["data"][index];
                        sample.get_mut(section).unwrap().insert(
                            key.into(),
                            value
                                .as_f64()
                                .map_or_else(|| "NaN".into(), |n| n.to_string()),
                        );
                    }
                    tab.record_info(sample);
                    tab.samples.back_mut().unwrap().time = label.as_str().unwrap().into();
                }
                assert_eq!(tab.samples.len(), labels.len());
                tab.info = data;
                let configuration = &row["configuration"];
                app.pref.general.theme = configuration["theme"].as_str().unwrap().into();
                let family = configuration["family"].as_str().unwrap();
                app.pref.general.font_family = if family.is_empty() {
                    vec![]
                } else {
                    vec![family.to_string()]
                };
                app.pref.general.font_size = configuration["size"].as_f64().unwrap() as f32;
                app.general_css_size = app.pref.general.font_size;
                app.pref.behavior.aside_width = 300.;
                app.nav = Nav::Browser;
                app.apply_theme(window, cx);
                window.resize(size(
                    px(configuration["windowWidth"].as_f64().unwrap() as f32),
                    px(768.),
                ));
                window.blur(cx);
                cx.notify();
                saved
            })
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update_window(window.into(), |_, window, cx| {
        for _ in 0..2 {
            window.refresh();
            let _ = window.draw(cx);
        }
        let font=view.read(cx).preference_font("general",".SystemUIFont",cx);
        let fid=window.text_system().resolve_font(&font);
        let text_run=|text:&str,font:Font|TextRun{len:text.len(),font,color:Hsla::white(),background_color:None,underline:None,strikethrough:None};
        let numeric=window.text_system().shape_line("21".into(),px(24.),&[text_run("21",font.clone())],None);
        let weights:Vec<_>=[400.,450.,500.,550.,600.,700.].into_iter().map(|weight|{
            let mut f=font.clone();f.weight=FontWeight(weight);
            let line=window.text_system().shape_line("活动状态".into(),px(16.),&[text_run("活动状态",f)],None);
            serde_json::json!({"weight":weight,"width":line.width.as_f32(),"ascent":line.ascent.as_f32(),"descent":line.descent.as_f32()})
        }).collect();
        println!("STATUS NATIVE FONT METRICS: {}",serde_json::json!({"scene":scene,"family":font.family.as_ref(),"xHeight14":window.text_system().x_height(fid,px(14.)).as_f32(),"xHeight24":window.text_system().x_height(fid,px(24.)).as_f32(),"ascent24":numeric.ascent.as_f32(),"descent24":numeric.descent.as_f32(),"tabWeights":weights}));
        let mut frames = Vec::new();
        for id in [
            "status-root",
            "status-summary-card",
            "status-header-content",
            "status-summary-grid",
            "status-activity-card",
            "status-chart-slot-ff6384",
            "status-chart-slot-ff9f40",
            "status-chart-slot-9966ff",
            "status-chart-slot-4bc0c0",
        ]
        .into_iter()
        .map(str::to_string)
        .chain((0..4).flat_map(|i| {
            [
                format!("status-stat-{i}"),
                format!("status-stat-label-{i}"),
                format!("status-stat-row-{i}"),
                format!("status-value-{i}"),
            ]
        })) {
            let node = gpui_base::test_support::find(window, &[], &id.clone().into())
                .unwrap_or_else(|| panic!("Missing status observer {id}"));
            let b = node.bounds();
            frames.push(serde_json::json!({"id":id,"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32(),"label":node.label()}));
        }
        for id in ["status-server-name","status-tag-version","status-tag-mode","status-tag-role","status-tab-text-activity","status-tab-text-status-info","status-refresh","show-clients","activity","status-info"] {
            if let Some(node)=gpui_base::test_support::find(window,&[],&id.into()) {
                let b=node.bounds();frames.push(serde_json::json!({"id":id,"x":b.origin.x.as_f32(),"y":b.origin.y.as_f32(),"w":b.size.width.as_f32(),"h":b.size.height.as_f32(),"label":node.label()}));
            }
        }
        println!("STATUS LAYOUT FRAME: {}",serde_json::json!({"scene":scene,"frames":frames,"scope":"Actual controlled Tab.record_info and GPUI bounds;original geometry/paint comparison is independent"}));
    }).unwrap();
    saved
}

pub(super) fn restore(
    view: &Entity<RdmApp>,
    window: WindowHandle<Root>,
    saved: Saved,
    cx: &mut AsyncApp,
) {
    cx.update_window(window.into(), |_, window, cx| {
        view.update(cx, |app, cx| {
            let i = app.tab_index(saved.tab_id).unwrap();
            let tab = &mut app.tabs[i];
            tab.info = saved.info;
            tab.samples = saved.samples;
            tab.pane = saved.pane;
            tab.info_details = saved.details;
            tab.status_auto = saved.auto;
            app.pref = saved.preferences;
            app.general_css_size = app.pref.general.font_size;
            app.apply_theme(window, cx);
            cx.notify();
        });
    })
    .unwrap();
}
