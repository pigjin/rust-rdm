//! Native GPUI rendering of the original status page's Chart.js 4.5.1 options.
//! Linear ticks, autoskip, legend rows and spline math are adapted from Chart.js
//! (MIT; assets/licenses/Chart-JS-LICENSE.txt). No JavaScript runtime is involved.
use super::*;
use gpui::prelude::*;

#[derive(Clone)]
pub(super) struct Series {
    pub label: String,
    pub color: u32,
    pub values: Vec<f64>,
}
#[derive(IntoElement)]
pub(super) struct StatusChart {
    pub id: String,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    pub bytes_axis: bool,
    pub dark: bool,
}
#[derive(Clone, Copy, serde::Serialize)]
struct XY {
    x: f64,
    y: f64,
}
#[derive(serde::Serialize)]
struct ChartPoint {
    x: f64,
    y: f64,
    previous: XY,
    next: XY,
    skip: bool,
}
#[derive(serde::Serialize)]
struct LegendBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
#[derive(serde::Serialize)]
struct Layout {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    min: f64,
    max: f64,
    rotation: f64,
    ticks: Vec<f64>,
    tick_labels: Vec<String>,
    x_ticks: Vec<usize>,
    legends: Vec<LegendBox>,
    points: Vec<Vec<ChartPoint>>,
}

fn nice_num(mut range: f64) -> f64 {
    if (range - range.round()).abs() < range / 1000. {
        range = range.round();
    }
    let power = 10f64.powf(range.log10().floor());
    let fraction = range / power;
    power
        * if fraction <= 1. {
            1.
        } else if fraction <= 2. {
            2.
        } else if fraction <= 5. {
            5.
        } else {
            10.
        }
}
fn linear_ticks(min: f64, max: f64, height: f64) -> Vec<f64> {
    let max_ticks = (height / 14.4).ceil().clamp(2., 11.);
    let spaces = max_ticks - 1.;
    let mut spacing = nice_num((max - min) / spaces);
    let count = (max / spacing).ceil() - (min / spacing).floor();
    if count > spaces {
        spacing = nice_num(count * spacing / spaces);
    }
    // Original ticks.precision=0, not a byte-axis fractional interval.
    spacing = spacing.ceil().max(1.);
    let first = (min / spacing).floor() * spacing;
    let last = (max / spacing).ceil() * spacing;
    let count = ((last - first) / spacing).round() as usize;
    (0..=count)
        .map(|i| (first + i as f64 * spacing).round())
        .collect()
}
fn skip_indices(count: usize, length: f64, max_length: f64, tick_size: f64) -> Vec<usize> {
    if count == 0 {
        return vec![];
    }
    let limit = (length / tick_size + 1.)
        .min(max_length / tick_size)
        .floor()
        .max(1.);
    let spacing = (count as f64 / limit).max(1.).ceil() as usize;
    (0..count).step_by(spacing).collect()
}
fn axis_label(value: f64, bytes: bool, domain: &[f64]) -> String {
    if bytes {
        super::status_values::format_bytes(value, 1, "")
    } else {
        let max = domain
            .first()
            .copied()
            .unwrap_or(value)
            .abs()
            .max(domain.last().copied().unwrap_or(value).abs());
        super::status_number::format(value, domain.len() > 1 && max > 1e15)
    }
}
impl StatusChart {
    fn layout(&self, w: f64, h: f64, measure: impl Fn(&str) -> f64) -> Layout {
        // Chart.js legend labels: boxWidth=40, fontSize=12, padding=10.
        let mut rows: Vec<Vec<(usize, f64)>> = vec![vec![]];
        let mut row_width = 0.;
        for (i, s) in self.series.iter().enumerate() {
            let width = 46. + measure(&s.label);
            if row_width > 0. && row_width + width + 20. > w {
                rows.push(vec![]);
                row_width = 0.;
            }
            rows.last_mut().unwrap().push((i, width));
            row_width += width + 10.;
        }
        let legend_height = rows.len() as f64 * 22. + 10.;
        let mut legends: Vec<LegendBox> = (0..self.series.len())
            .map(|_| LegendBox {
                x: 0.,
                y: 0.,
                w: 0.,
                h: 12.,
            })
            .collect();
        for (row, items) in rows.iter().enumerate() {
            let width = items.iter().map(|(_, w)| w + 10.).sum::<f64>();
            let mut x = (w - width) / 2. + 5.;
            for &(i, width) in items {
                legends[i] = LegendBox {
                    x,
                    y: 10. + row as f64 * 22.,
                    w: width,
                    h: 12.,
                };
                x += width + 10.;
            }
        }
        let mut low = 0f64;
        let mut high = 0f64;
        let mut finite = false;
        for n in self
            .series
            .iter()
            .flat_map(|s| s.values.iter())
            .filter(|v| v.is_finite())
        {
            low = low.min(*n);
            high = high.max(*n);
            finite = true;
        }
        if !finite || low == high {
            high = if high == 0. {
                1.
            } else {
                high + high.abs() * 0.05
            };
        }
        let overflow = if self.labels.is_empty() { 3. } else { 5. };
        let label_widths: Vec<_> = self.labels.iter().map(|s| measure(s)).collect();
        let widest = label_widths.iter().copied().fold(0., f64::max);
        let mut bottom = h - if self.labels.is_empty() { 10.2 } else { 28.4 };
        let top = legend_height.max(10.2);
        let mut rotation = 0f64;
        let mut candidates = vec![];
        // Scale layout can refit after the horizontal labels rotate. Autoskip
        // happens after rotation; its narrower surviving labels do not rerotate.
        for _ in 0..3 {
            candidates = linear_ticks(low, high, (bottom - top).max(1.));
            let y_width = (candidates
                .iter()
                .map(|v| measure(&axis_label(*v, self.bytes_axis, &candidates)))
                .fold(0., f64::max)
                + 14.)
                .min(w / 2.);
            rotation = 0.;
            if self.labels.len() > 1 {
                let max_width = (w - widest).clamp(0., (w - y_width - overflow).max(0.));
                let tick_width = max_width / (self.labels.len() - 1) as f64;
                if widest + 6. > tick_width {
                    let diagonal = widest.hypot(14.4);
                    rotation = ((20.4 / tick_width).clamp(-1., 1.).asin())
                        .min(
                            ((h / 2. - 11.) / diagonal).clamp(-1., 1.).asin()
                                - (14.4 / diagonal).asin(),
                        )
                        .clamp(0., 50f64.to_radians());
                }
            }
            let axis_height = if self.labels.is_empty() {
                8.
            } else {
                (14. + rotation.sin() * widest + rotation.cos() * 14.4).min(h / 2.)
            };
            let next = h - axis_height.max(10.2);
            if (next - bottom).abs() < 1e-8 {
                break;
            }
            bottom = next;
        }
        let min = *candidates.first().unwrap();
        let max = *candidates.last().unwrap();
        let indices = skip_indices(candidates.len(), bottom - top, h, 17.4);
        let ticks: Vec<_> = indices.iter().map(|i| candidates[*i]).collect();
        let tick_labels: Vec<_> = ticks
            .iter()
            .map(|v| axis_label(*v, self.bytes_axis, &candidates))
            .collect();
        let y_width = (tick_labels.iter().map(|s| measure(s)).fold(0., f64::max) + 14.).min(w / 2.);
        let first = label_widths.first().copied().unwrap_or(0.);
        let last = label_widths.last().copied().unwrap_or(0.);
        let pad_left = if rotation > 0. {
            rotation.cos() * first + 3.
        } else {
            first / 2. + 3.
        };
        let pad_right = if rotation > 0. {
            rotation.sin() * 14.4 + 3.
        } else {
            last / 2. + 3.
        };
        let left = y_width.max(pad_left).max(overflow);
        let right = (w - pad_right.max(overflow)).max(left);
        let tick_size = if rotation > 0. && (widest + 3.) * rotation.sin() >= 17.4 * rotation.cos()
        {
            17.4 / rotation.sin()
        } else {
            (widest + 3.) / rotation.cos()
        };
        let x_ticks = skip_indices(self.labels.len(), right - left, w, tick_size.max(1.));
        let mut points = Vec::new();
        for s in &self.series {
            let base: Vec<_> = s
                .values
                .iter()
                .enumerate()
                .map(|(i, v)| XY {
                    x: left
                        + (right - left) * i as f64
                            / (self.labels.len().saturating_sub(1).max(1)) as f64,
                    y: if v.is_finite() {
                        bottom - (bottom - top) * (v - min) / (max - min)
                    } else {
                        bottom
                    },
                })
                .collect();
            let mut data = Vec::new();
            for (i, p) in base.iter().enumerate() {
                let prev = if i > 0 && s.values[i - 1].is_finite() {
                    base[i - 1]
                } else {
                    *p
                };
                let next = if i + 1 < base.len() && s.values[i + 1].is_finite() {
                    base[i + 1]
                } else {
                    *p
                };
                let d01 = (p.x - prev.x).hypot(p.y - prev.y);
                let d12 = (next.x - p.x).hypot(next.y - p.y);
                let sum = d01 + d12;
                let a = if sum == 0. { 0. } else { 0.4 * d01 / sum };
                let b = if sum == 0. { 0. } else { 0.4 * d12 / sum };
                let clip = |q: XY| XY {
                    x: q.x.clamp(left, right),
                    y: q.y.clamp(top, bottom),
                };
                data.push(ChartPoint {
                    x: p.x,
                    y: p.y,
                    skip: !s.values[i].is_finite(),
                    previous: clip(XY {
                        x: p.x - a * (next.x - prev.x),
                        y: p.y - a * (next.y - prev.y),
                    }),
                    next: clip(XY {
                        x: p.x + b * (next.x - prev.x),
                        y: p.y + b * (next.y - prev.y),
                    }),
                });
            }
            points.push(data);
        }
        Layout {
            left,
            right,
            top,
            bottom,
            min,
            max,
            rotation,
            ticks,
            tick_labels,
            x_ticks,
            legends,
            points,
        }
    }
}
fn run(text: &str, color: Hsla) -> TextRun {
    TextRun {
        len: text.len(),
        font: gpui::font("Helvetica Neue"),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}
fn text_width(text: &str, window: &Window) -> f64 {
    super::text::script_width(text, gpui::font("Helvetica Neue"), 12., window) as f64
}
fn paint_text(
    text: &str,
    x: f64,
    y: f64,
    align: TextAlign,
    color: Hsla,
    origin: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let width = text_width(text, window);
    let mut x = x - match align {
        TextAlign::Right => width,
        TextAlign::Center => width / 2.,
        _ => 0.,
    };
    for part in super::text::script_runs(text) {
        let line = window.text_system().shape_line(
            part.to_owned().into(),
            px(12.),
            &[run(part, color)],
            None,
        );
        let _ = line.paint(
            origin + point(px(x as f32), px(y as f32 - 7.2)),
            px(14.4),
            TextAlign::Left,
            None,
            window,
            cx,
        );
        x += line.width.as_f32() as f64;
    }
}
fn stroke(points: &[XY], origin: Point<Pixels>, width: f32, color: Hsla, window: &mut Window) {
    if let Some(first) = points.first() {
        let mut path = PathBuilder::stroke(px(width));
        path.move_to(origin + point(px(first.x as f32), px(first.y as f32)));
        let closed = points.len() > 2
            && points
                .last()
                .is_some_and(|p| p.x == first.x && p.y == first.y);
        for p in &points[1..points.len() - usize::from(closed)] {
            path.line_to(origin + point(px(p.x as f32), px(p.y as f32)));
        }
        if closed {
            path.close();
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}
fn circle(
    x: f64,
    y: f64,
    r: f64,
    origin: Point<Pixels>,
    color: Hsla,
    outline: bool,
    window: &mut Window,
) {
    let mut path = if outline {
        PathBuilder::stroke(px(1.))
    } else {
        PathBuilder::fill()
    };
    let p = |x: f64, y: f64| origin + point(px(x as f32), px(y as f32));
    let k = r * 0.5522847498307936;
    path.move_to(p(x + r, y));
    path.cubic_bezier_to(p(x, y + r), p(x + r, y + k), p(x + k, y + r));
    path.cubic_bezier_to(p(x - r, y), p(x - k, y + r), p(x - r, y + k));
    path.cubic_bezier_to(p(x, y - r), p(x - r, y - k), p(x - k, y - r));
    path.cubic_bezier_to(p(x + r, y), p(x + k, y - r), p(x + r, y - k));
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
impl RenderOnce for StatusChart {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let data = std::rc::Rc::new(self);
        let prepare = data.clone();
        canvas(move|bounds,window,_|prepare.layout(bounds.size.width.as_f32() as f64,bounds.size.height.as_f32() as f64,|s|text_width(s,window)),
            move|bounds,layout,window,cx| {
                #[cfg(feature="ui-qa")]
                if std::env::var_os("RUST_RDM_QA_STATUS_CHART_TRACE").is_some() {
                    for (value,label) in layout.ticks.iter().zip(&layout.tick_labels).rev() {println!("STATUS AXIS TICK: {}",serde_json::json!({"chart":data.id,"value":value,"label":label}));}
                    println!("STATUS CHART FRAME: {}",serde_json::json!({"chart":data.id,"canvas":{"x":bounds.origin.x.as_f32(),"y":bounds.origin.y.as_f32(),"w":bounds.size.width.as_f32(),"h":bounds.size.height.as_f32()},"layout":layout}));
                }
                let origin=bounds.origin;
                let fg: Hsla=if data.dark{rgba(0xffffffd1).into()}else{rgb(0x333639).into()};
                let muted:Hsla=if data.dark{rgba(0xffffff85).into()}else{rgb(0x767c82).into()};
                let grid:Hsla=if data.dark{rgba(0xffffff17).into()}else{rgb(0xe0e0e6).into()};
                let border:Hsla=rgba(0x0000001a).into();
                let dpr=window.scale_factor() as f64;
                let align=|p:f64|((p-0.5)*dpr).round()/dpr+0.5;
                for (i,s) in data.series.iter().enumerate() {
                    let b=&layout.legends[i];let color:Hsla=rgb(s.color).into();
                    window.paint_quad(fill(Bounds::new(origin+point(px(b.x as f32),px(b.y as f32)),size(px(40.),px(12.))),color.opacity(0.2)));
                    stroke(&[XY{x:b.x,y:b.y},XY{x:b.x+40.,y:b.y},XY{x:b.x+40.,y:b.y+12.},XY{x:b.x,y:b.y+12.},XY{x:b.x,y:b.y}],origin,3.,color,window);
                    paint_text(&s.label,b.x+46.,b.y+6.,TextAlign::Left,fg,origin,window,cx);
                }
                for (tick,label) in layout.ticks.iter().zip(&layout.tick_labels) {
                    let y=layout.bottom-(layout.bottom-layout.top)*(tick-layout.min)/(layout.max-layout.min);
                    stroke(&[XY{x:align(layout.left)+0.5,y:align(y)},XY{x:layout.right,y:align(y)}],origin,1.,grid,window);
                    stroke(&[XY{x:layout.left-8.,y:align(y)},XY{x:align(layout.left)-0.5,y:align(y)}],origin,1.,grid,window);
                    paint_text(label,layout.left-11.,y,TextAlign::Right,muted,origin,window,cx);
                }
                for &i in &layout.x_ticks {
                    let x=layout.left+(layout.right-layout.left)*i as f64/data.labels.len().saturating_sub(1).max(1) as f64;
                    stroke(&[XY{x:align(x),y:layout.top},XY{x:align(x),y:align(layout.bottom)-0.5}],origin,1.,grid,window);
                    stroke(&[XY{x:align(x),y:align(layout.bottom)+0.5},XY{x:align(x),y:layout.bottom+8.}],origin,1.,grid,window);
                    if layout.rotation==0. {paint_text(&data.labels[i],x,layout.bottom+18.2,TextAlign::Center,muted,origin,window,cx);}
                    else {super::status_chart_text::paint_rotated(&data.labels[i],origin+point(px((x-7.2*layout.rotation.sin()) as f32),px((layout.bottom+11.) as f32)), -layout.rotation, muted, window, cx);}
                }
                stroke(&[XY{x:align(layout.left),y:layout.top},XY{x:align(layout.left),y:align(layout.bottom)+0.5}],origin,1.,border,window);
                stroke(&[XY{x:align(layout.left)-0.5,y:align(layout.bottom)},XY{x:align(layout.right)+0.5,y:align(layout.bottom)}],origin,1.,border,window);
                // Original default drawing order is last dataset first.
                for (series,points) in data.series.iter().zip(&layout.points).rev() {
                    let color:Hsla=rgb(series.color).into();let mut start=0;
                    while start<points.len() {
                        while start<points.len()&&points[start].skip{start+=1;}
                        if start==points.len(){break;}
                        let end=points[start..].iter().position(|p|p.skip).map_or(points.len(),|n|start+n);
                        let p=|x:f64,y:f64|origin+point(px(x as f32),px(y as f32));
                        for fill in [true,false] {
                            let mut path=if fill{PathBuilder::fill()}else{PathBuilder::stroke(px(3.))};
                            if fill {path.move_to(p(points[start].x,layout.bottom));path.line_to(p(points[start].x,points[start].y));}
                            else{path.move_to(p(points[start].x,points[start].y));}
                            for i in start+1..end {let a=&points[i-1];let b=&points[i];path.cubic_bezier_to(p(b.x,b.y),p(a.next.x,a.next.y),p(b.previous.x,b.previous.y));}
                            if fill {path.line_to(p(points[end-1].x,layout.bottom));path.close();}
                            if let Ok(path)=path.build(){window.paint_path(path,if fill{color.opacity(0.2)}else{color});}
                        }
                        start=end+1;
                    }
                    for p in points.iter().filter(|p|!p.skip) {circle(p.x,p.y,3.,origin,color.opacity(0.2),false,window);circle(p.x,p.y,3.,origin,color,true,window);}
                }
            }).size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::{axis_label, linear_ticks, skip_indices};
    #[test]
    fn original_status_chart_integer_domains_and_autoskip() {
        let rounding: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/status-numeric-rounding-source.json"
        ))
        .unwrap();
        for row in rounding["records"].as_array().unwrap() {
            let ticks: Vec<_> = row["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            let actual: Vec<_> = ticks
                .iter()
                .map(|v| axis_label(*v, false, &ticks))
                .collect();
            let expected: Vec<_> = row["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(actual, expected, "scientific rounding at {ticks:?}");
        }
        for text in [
            include_str!("../../tests/fixtures/status-layout-macos-source.json"),
            include_str!("../../tests/fixtures/status-large-numeric-macos-source.json"),
        ] {
            let source: serde_json::Value = serde_json::from_str(text).unwrap();
            for row in source["records"].as_array().unwrap() {
                for (chart_index, chart) in row["charts"].as_array().unwrap().iter().enumerate() {
                    let max = chart["data"]["datasets"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|d| d["data"].as_array().unwrap())
                        .filter_map(|v| v.as_f64())
                        .fold(0f64, f64::max)
                        .max(1.);
                    let height = chart["chartArea"]["height"].as_f64().unwrap();
                    let candidates = linear_ticks(0., max, height);
                    assert_eq!(
                        *candidates.first().unwrap(),
                        chart["scales"]["y"]["min"].as_f64().unwrap(),
                        "{}",
                        row["name"]
                    );
                    assert_eq!(
                        *candidates.last().unwrap(),
                        chart["scales"]["y"]["max"].as_f64().unwrap(),
                        "{}",
                        row["name"]
                    );
                    let indices = skip_indices(
                        candidates.len(),
                        height,
                        chart["height"].as_f64().unwrap(),
                        17.4,
                    );
                    let expected: Vec<_> = chart["scales"]["y"]["ticks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| t["value"].as_f64().unwrap())
                        .collect();
                    assert_eq!(
                        indices.iter().map(|i| candidates[*i]).collect::<Vec<_>>(),
                        expected,
                        "{}",
                        row["name"]
                    );
                    let bytes = chart_index >= 2;
                    let labels: Vec<_> = indices
                        .iter()
                        .map(|i| axis_label(candidates[*i], bytes, &candidates))
                        .collect();
                    let expected_labels: Vec<_> = chart["scales"]["y"]["ticks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| t["label"].as_str().unwrap())
                        .collect();
                    assert_eq!(labels, expected_labels, "{}", row["name"]);
                }
            }
        }
    }
}
