//! Controlled diagnostic: compare actual rounded and original fractional text origins.
//! It renders only a label in its own hidden window; no Redis or clipboard access.
use gpui::{prelude::*, *};
use std::{path::PathBuf, time::Duration};

struct Probe {
    origin: Point<Pixels>,
    width: Pixels,
    text: String,
    color: Hsla,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let origin = self.origin;
        let width = self.width;
        let text = self.text.clone();
        let color = self.color;
        div().size_full().bg(rgb(0xfafafc)).child(
            canvas(|_, _, _| (), move |_, (), window, cx| {
                let line = window.text_system().shape_line(text.clone().into(), px(12.), &[TextRun {
                    len: text.len(), font: font("Times"), color,
                    background_color: None, underline: None, strikethrough: None,
                }], None);
                for family in ["Times", ".SystemUIFont", "Times New Roman"] {
                    let metrics=window.text_system().shape_line(text.clone().into(),px(12.),&[TextRun {len:text.len(),font:font(family),color,background_color:None,underline:None,strikethrough:None}],None);
                    println!("FONT BASELINE PROBE: {}",serde_json::json!({"family":family,"pointSize":12,"lineHeight":12,"originX":origin.x.as_f32(),"originY":origin.y.as_f32(),"ascent":metrics.ascent.as_f32(),"descent":metrics.descent.as_f32(),"baselineY":(origin.y+(px(12.)+metrics.ascent-metrics.descent)*0.5).as_f32()}));
                }
                window.with_content_mask(Some(ContentMask {
                    bounds: Bounds::new(origin, size(width, px(12.))),
                }), |window| line.paint(origin, px(12.), TextAlign::Left, None, window, cx).unwrap());
            }).size_full(),
        )
    }
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let log = std::fs::read_to_string(args.next().expect("native frame log")).unwrap();
    let output = PathBuf::from(args.next().expect("output directory"));
    // Black selects zero CoreGraphics dilation in the existing Apple backend.
    // This option is diagnostic only; production foreground colors are unchanged.
    let color: Hsla = rgb(if args.next().as_deref() == Some(std::ffi::OsStr::new("black")) { 0 } else { 0x333639 }).into();
    std::fs::create_dir_all(&output).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/general-fallback-macos-source.json"
    )).unwrap();
    let source = &fixture["records"][1];
    let rect = source["styles"].as_array().unwrap().iter().find(|v| v["id"] == "content").unwrap();
    let native = log.lines().filter_map(|line| line.strip_prefix("GENERAL FALLBACK FRAME: "))
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|v| v["scene"] == "general-fallback-1").unwrap();
    let native_rect = native["frames"].as_array().unwrap().iter().find(|v| v["id"] == "match-label").unwrap();
    let (sx, sy, nx, ny) = (rect["x"].as_f64().unwrap() as f32, rect["y"].as_f64().unwrap() as f32,
        native_rect["x"].as_f64().unwrap() as f32, native_rect["y"].as_f64().unwrap() as f32);
    let width = px(rect["w"].as_f64().unwrap() as f32);
    let text = source["match"].as_str().unwrap().to_owned();
    let positions = [("rounded", nx, ny), ("source-x", sx, ny), ("source-y", nx, sy), ("source-xy", sx, sy)];
    gpui_platform::headless().run(move |cx| {
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(1024.), px(768.))))),
            show: false, focus: false, ..Default::default()
        }, move |_, cx| cx.new(|_| Probe { origin: point(px(nx), px(ny)), width, text, color })).unwrap();
        cx.spawn(async move |cx| {
            for (name, x, y) in positions {
                window.update(cx, |view, window, cx| {
                    view.origin = point(px(x), px(y)); cx.notify(); window.refresh();
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(100)).await;
                cx.update_window(window.into(), |_, window, cx| {
                    let _ = window.draw(cx);
                    window.render_to_image().unwrap().save(output.join(format!("{name}.png"))).unwrap();
                }).unwrap();
                println!("FONT ORIGIN PROBE: {name} ({x}, {y})");
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
}
