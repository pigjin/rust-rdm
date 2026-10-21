//! Independent diagnostic of CSS font metrics; never changes application state.
//! Compare raw GPUI painting with the rule measured in 49 WK/CoreText samples.
use gpui::{prelude::*, *};
use std::{path::PathBuf, time::Duration};

#[derive(Clone)]
struct Case {
    name: String,
    family: String,
    text: String,
    origin: Point<Pixels>,
    width: Pixels,
    css_metrics: bool,
}
struct Probe(Case);
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let case = self.0.clone();
        div().size_full().bg(rgb(0xfafafc)).child(
            canvas(|_, _, _| (), move |_, (), window, cx| {
                let line = window.text_system().shape_line(case.text.clone().into(), px(12.), &[TextRun {
                    len: case.text.len(), font: font(case.family.clone()), color: rgb(0x333639).into(),
                    background_color: None, underline: None, strikethrough: None,
                }], None);
                let raw_ascent = line.ascent.as_f32();
                let descent = line.descent.as_f32();
                let adjusted_ascent = if case.family == "Times" {
                    raw_ascent + ((raw_ascent + descent) * 0.15).round()
                } else { raw_ascent };
                let raw_baseline = (12. + raw_ascent - descent) / 2.;
                let css_baseline = ((12. + adjusted_ascent.round() - descent.round()) / 2.).floor();
                let shift = if case.css_metrics { css_baseline - raw_baseline } else { 0. };
                let paint_origin = case.origin + point(px(0.), px(shift));
                println!("CSS FONT BASELINE PROBE: {}", serde_json::json!({
                    "name": case.name, "family": case.family, "rawAscent": raw_ascent,
                    "rawDescent": descent, "adjustedAscent": adjusted_ascent,
                    "rawBaselineOffset": raw_baseline, "cssBaselineOffset": css_baseline,
                    "cssMetricRuleEnabled": case.css_metrics,
                    "originX": case.origin.x.as_f32(), "originY": case.origin.y.as_f32(),
                    "width": case.width.as_f32(), "height": 12,
                    "paintBaselineY": case.origin.y.as_f32() + raw_baseline + shift
                }));
                window.with_content_mask(Some(ContentMask {
                    bounds: Bounds::new(case.origin, size(case.width, px(12.))),
                }), |window| line.paint(paint_origin, px(12.), TextAlign::Left, None, window, cx).unwrap());
            }).size_full(),
        )
    }
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let native_log = std::fs::read_to_string(args.next().expect("native frame log")).unwrap();
    let output = PathBuf::from(args.next().expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/general-fallback-macos-source.json"
    )).unwrap();
    let frames = native_log.lines().filter_map(|line| line.strip_prefix("GENERAL FALLBACK FRAME: "))
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()).collect::<Vec<_>>();
    let mut cases = Vec::new();
    for (index, row) in fixture["records"].as_array().unwrap().iter().enumerate() {
        let source = row["styles"].as_array().unwrap().iter().find(|v| v["id"] == "content").unwrap();
        let native = frames.iter().find(|v| v["scene"] == row["name"]).unwrap()["frames"]
            .as_array().unwrap().iter().find(|v| v["id"] == "match-label").unwrap().clone();
        let configured = row["configuration"]["family"].as_str().unwrap();
        let family = if index < 2 { "Times" } else if configured.is_empty() { ".SystemUIFont" } else { configured };
        for (variant, rect, css_metrics) in [("raw", &native, false), ("css-rounded", &native, true), ("css-source", source, true)] {
            cases.push(Case {
                name: format!("general-{index}-{variant}"), family: family.to_owned(),
                text: row["match"].as_str().unwrap().to_owned(), css_metrics,
                origin: point(px(rect["x"].as_f64().unwrap() as f32), px(rect["y"].as_f64().unwrap() as f32)),
                width: px(rect["w"].as_f64().unwrap() as f32),
            });
        }
    }
    let first = cases[0].clone();
    gpui_platform::headless().run(move |cx| {
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(1024.), px(768.))))),
            show: false, focus: false, ..Default::default()
        }, move |_, cx| cx.new(|_| Probe(first))).unwrap();
        cx.spawn(async move |cx| {
            for case in cases {
                let name = case.name.clone();
                window.update(cx, |view, window, cx| { view.0 = case; cx.notify(); window.refresh(); }).unwrap();
                cx.background_executor().timer(Duration::from_millis(100)).await;
                cx.update_window(window.into(), |_, window, cx| {
                    let _ = window.draw(cx);
                    window.render_to_image().unwrap().save(output.join(format!("{name}.png"))).unwrap();
                }).unwrap();
                println!("CSS FONT BASELINE CAPTURE: {name}");
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
}
