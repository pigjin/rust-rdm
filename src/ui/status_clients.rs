use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use std::cmp::Ordering;
use unicode_segmentation::UnicodeSegmentation as _;

// CSS `white-space: normal; word-break: break-word`: retain Unicode line
// opportunities and split an overlong unbreakable sequence at graphemes.
// Measure the same script runs painted by CssLabel, rather than summing
// independent character advances (which loses kerning and fallback shaping).
fn wrapped_lines(text: &str, width: f32, font: &Font, window: &Window) -> Vec<String> {
    let text = text.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return vec![String::new()];
    }
    if super::text::script_width(&text, font.clone(), 14., window) <= width {
        return vec![text];
    }
    let mut opportunities: Vec<_> = unicode_linebreak::linebreaks(&text)
        .map(|(i, _)| i)
        .filter(|i| {
            let before = text[..*i].chars().next_back();
            let after = text[*i..].chars().next();
            // WebKit's normal ASCII shortcut keeps /latin paths together.
            !(before == Some('/') && after.is_some_and(|c| c.is_ascii_alphabetic()))
        })
        .collect();
    for (i, ch) in text.char_indices() {
        // The browser allows URL-style ABCD-1234 and 1234-5678 breaks,
        // while keeping a leading minus sign attached to its number.
        if ch.is_ascii_digit()
            && text[..i].ends_with('-')
            && text[..i - 1]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            opportunities.push(i);
        }
    }
    opportunities.sort_unstable();
    opportunities.dedup();
    let mut start = 0;
    let mut lines = Vec::new();
    while start < text.len() {
        let remainder = &text[start..];
        let mut fit = start;
        for (offset, grapheme) in remainder.grapheme_indices(true) {
            let end = start + offset + grapheme.len();
            if super::text::script_width(&text[start..end], font.clone(), 14., window) > width {
                if fit == start {
                    fit = end;
                }
                break;
            }
            fit = end;
        }
        let end = if fit == text.len() {
            fit
        } else {
            opportunities
                .iter()
                .copied()
                .filter(|i| *i > start && *i <= fit)
                .last()
                .unwrap_or(fit)
        };
        lines.push(text[start..end].trim_end_matches(' ').to_string());
        start = end;
        while text.as_bytes().get(start) == Some(&b' ') {
            start += 1;
        }
    }
    lines
}

// Original BaseLoading: a 3s CSS rotation plus two 1.6s SMIL rotations,
// and the 567 -> 142 -> 567 dash offset of its radius-90 circle.
fn paint_loading(bounds: Bounds<Pixels>, seconds: f32, window: &mut Window) {
    let cycle = (seconds / 1.6).fract();
    let circle_rotation = if cycle <= 0.5 {
        270. * cycle
    } else {
        135. + 630. * (cycle - 0.5)
    };
    let angle = (seconds / 3. * 360. + 270. * cycle + circle_rotation).to_radians();
    let progress = if cycle <= 0.5 {
        cycle * 2.
    } else {
        (1. - cycle) * 2.
    };
    let length = 425. * progress;
    let sweep = length / 90.;
    let scale = bounds.size.width.as_f32() / 200.;
    let radius = 90. * scale;
    let stroke = 20. * scale;
    let position =
        |angle: f32| bounds.center() + point(px(radius * angle.cos()), px(radius * angle.sin()));
    let start = position(angle);
    let end = position(angle + sweep);
    let mut path = PathBuilder::stroke(px(stroke));
    path.move_to(start);
    path.arc_to(
        point(px(radius), px(radius)),
        px(0.),
        sweep > std::f32::consts::PI,
        true,
        end,
    );
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(0xd33a31));
    }
    for center in [start, end] {
        window.paint_quad(quad(
            Bounds {
                origin: center - point(px(stroke / 2.), px(stroke / 2.)),
                size: size(px(stroke), px(stroke)),
            },
            px(stroke / 2.),
            rgb(0xd33a31),
            px(0.),
            rgb(0),
            BorderStyle::Solid,
        ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Column {
    Address,
    Age,
    Idle,
}
impl Column {
    fn key(self) -> &'static str {
        match self {
            Self::Address => "addr",
            Self::Age => "age",
            Self::Idle => "idle",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ClientSort {
    Unsorted,
    Ascending(Column),
    Descending(Column),
}
impl Default for ClientSort {
    fn default() -> Self {
        Self::Descending(Column::Age)
    }
}
impl ClientSort {
    fn next(self, column: Column) -> Self {
        match self {
            Self::Descending(old) if old == column => Self::Ascending(column),
            Self::Ascending(old) if old == column => Self::Unsorted,
            _ => Self::Descending(column),
        }
    }
    pub(super) fn state(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Unsorted => None,
            Self::Ascending(c) => Some((c.key(), "ascend")),
            Self::Descending(c) => Some((c.key(), "descend")),
        }
    }
}

pub(super) type ClientRows = Vec<BTreeMap<String, String>>;

pub(super) struct ClientList {
    pub rows: Arc<ClientRows>,
    pub order: Vec<usize>,
    pub sort: ClientSort,
    pub loading: bool,
    pub started: Instant,
    pub list: ListState,
    layout: RefCell<Option<(f32, String)>>,
    #[cfg(feature = "ui-qa")]
    pub gate: Option<Arc<status_clients_qa::LoadGate>>,
    #[cfg(feature = "ui-qa")]
    pub lifetime: Arc<()>,
}

impl ClientList {
    fn new() -> Self {
        Self {
            rows: Arc::new(vec![]),
            order: vec![],
            sort: ClientSort::default(),
            loading: false,
            started: Instant::now(),
            list: ListState::new(0, ListAlignment::Top, px(100.)),
            layout: RefCell::new(None),
            #[cfg(feature = "ui-qa")]
            gate: None,
            #[cfg(feature = "ui-qa")]
            lifetime: Arc::new(()),
        }
    }

    pub(super) fn replace_rows(&mut self, rows: Arc<ClientRows>, order: Vec<usize>) {
        self.list.reset_with_uniform_height(rows.len(), px(34.));
        self.rows = rows;
        self.order = order;
        *self.layout.borrow_mut() = None;
    }

    fn fade(&self, color: Hsla, dark: bool) -> Hsla {
        if !self.loading {
            return color;
        }
        let alpha = if dark { 0.38 } else { 0.5 };
        let mut value: Rgba = color.into();
        let parent: Rgba = rgb(if dark { 0x18181c } else { 0xffffff }).into();
        value.r = value.r * alpha + parent.r * (1. - alpha);
        value.g = value.g * alpha + parent.g * (1. - alpha);
        value.b = value.b * alpha + parent.b * (1. - alpha);
        value.into()
    }
}

impl Tab {
    pub(super) fn close_clients(&mut self) {
        self.client_epoch = self.client_epoch.wrapping_add(1);
        self.clients = None;
    }
}

#[cfg(feature = "ui-qa")]
pub(super) static SORT_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(super) fn sorted_indices(clients: &ClientRows, sort: ClientSort) -> Vec<usize> {
    #[cfg(feature = "ui-qa")]
    SORT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut indices: Vec<_> = (0..clients.len()).collect();
    let Some((field, order)) = sort.state() else {
        return indices;
    };
    // Vec::sort_by is stable, including equal numeric ages and canonical
    // equivalent address strings, as in the original DataTable.
    indices.sort_by(|a, b| {
        let (a, b) = (&clients[*a], &clients[*b]);
        let order_value = if field == "addr" {
            match (a.get(field), b.get(field)) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(a), Some(b)) => compare_addresses(a, b),
            }
        } else {
            let number = |value: Option<&String>| {
                value.and_then(|v| {
                    if v.trim().is_empty() {
                        Some(0.)
                    } else {
                        v.parse::<f64>().ok()
                    }
                })
            };
            number(a.get(field))
                .zip(number(b.get(field)))
                .and_then(|(a, b)| a.partial_cmp(&b))
                .unwrap_or(Ordering::Equal)
        };
        if order == "descend" {
            order_value.reverse()
        } else {
            order_value
        }
    });
    indices
}

pub(super) fn compare_addresses(a: &str, b: &str) -> Ordering {
    #[cfg(target_os = "macos")]
    if let Some(order) = macos::compare(a, b) {
        return order;
    }
    // Other platforms still require their native localeCompare adapter.
    a.cmp(b)
}

impl RdmApp {
    pub(super) fn render_client_trigger(
        &self,
        i: usize,
        button: Button,
        width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = self.tabs[i].id;
        let measured = self.tabs[i].client_trigger_bounds.clone();
        let bounds = measured.get();
        let grid = self.tabs[i].client_grid_bounds.get();
        let value = self.tabs[i]
            .info
            .get("Clients")
            .and_then(|v| v.get("connected_clients"))
            .map(String::as_str)
            .unwrap_or("0");
        let number_width = super::text::script_width(
            value,
            self.preference_font("general", ".SystemUIFont", cx),
            24.,
            window,
        );
        // Preserve the original four-column grid and inline advances before
        // native half-pixel layout rounding. The source follower rounds the
        // complete target edge, not each nested flex child's painted bounds.
        let left = if grid.size.width > px(0.) {
            grid.left().as_f32()
                + (grid.size.width.as_f32() - 3. * 5.) / 4.
                + 5.
                + number_width
                + 4.
        } else {
            bounds.left().as_f32()
        };
        let tooltip_width = window.viewport_size().width.as_f32() * 0.7 + 28.;
        let follower_width =
            (tooltip_width * window.scale_factor()).round() / window.scale_factor();
        let center = left + width / 2.;
        let (anchor, anchor_x, arrow_x) = if center - follower_width / 2. < 0. {
            (Anchor::TopLeft, left.round(), 10. + 8.46875 / 2.)
        } else if center + follower_width / 2. > window.viewport_size().width.as_f32() {
            (
                Anchor::TopRight,
                (left + width).round(),
                tooltip_width - 10. - 8.46875 / 2.,
            )
        } else {
            (Anchor::TopCenter, center.round(), tooltip_width / 2.)
        };
        let parent = cx.entity_id();
        let load = cx.weak_entity();
        let content = cx.weak_entity();
        let trigger = div()
            .relative()
            .w(px(width))
            .h(px(28.))
            .child(button)
            .child(
                canvas(
                    move |value, _, cx| {
                        if measured.replace(value) != value {
                            cx.notify(parent);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        gpui_base::Popover::new(format!("client-list-{id}"))
            .w(px(width))
            .h(px(28.))
            .flex_shrink_0()
            .anchor(anchor)
            .anchor_x(px(anchor_x))
            .offset(px(10.))
            .snap_to_device_pixels()
            .preserve_horizontal_anchor()
            .trigger_with(move |_, _, _| trigger.into_any_element())
            .on_open_change(move |open, window, cx| {
                let _ = load.update(cx, |app, cx| {
                    if let Some(i) = app.tab_index(id) {
                        app.tabs[i].close_clients();
                        if *open {
                            app.tabs[i].clients = Some(ClientList::new());
                            app.update_clients(id, None, window, cx);
                        }
                        cx.notify();
                    }
                });
            })
            .content(move |_, window, cx| {
                content
                    .update(cx, |app, cx| app.render_clients(id, arrow_x, window, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element()
    }

    pub(super) fn update_clients(
        &mut self,
        id: u64,
        column: Option<Column>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.tabs[i];
        let Some(clients) = tab.clients.as_mut().filter(|clients| !clients.loading) else {
            return;
        };
        if let Some(column) = column {
            clients.sort = clients.sort.next(column);
        }
        clients.loading = true;
        clients.started = Instant::now();
        let sort = clients.sort;
        let rows = column.map(|_| clients.rows.clone());
        #[cfg(feature = "ui-qa")]
        let gate = clients.gate.clone();
        tab.client_epoch = tab.client_epoch.wrapping_add(1);
        let epoch = tab.client_epoch;
        let connection = self.tabs[i].conn.clone();
        let service = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let result = match rows {
                Some(rows) => Ok(rows),
                None => service.client_list(&connection).map(Arc::new),
            }
            .map(|rows| {
                let order = sorted_indices(&rows, sort);
                (rows, order)
            });
            #[cfg(feature = "ui-qa")]
            if let (Some(gate), Ok((rows, _))) = (gate, &result) {
                gate.hold(rows.clone());
            }
            result
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |app, _, cx| {
                if let Some(i) = app
                    .tab_index(id)
                    .filter(|i| app.tabs[*i].client_epoch == epoch)
                    && let Some(clients) = app.tabs[i].clients.as_mut()
                {
                    clients.loading = false;
                    match result {
                        Ok((rows, order)) => clients.replace_rows(rows, order),
                        Err(error) => app.toast(error.to_string(), true),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn seconds_text(&self, seconds: u64) -> String {
        let time = format!(
            "{:02}:{:02}:{:02}",
            seconds % 86400 / 3600,
            seconds % 3600 / 60,
            seconds % 60
        );
        if seconds >= 86400 {
            format!("{}{} {}", seconds / 86400, self.tr("common.unit_day"), time)
        } else {
            time
        }
    }

    fn render_client_row(
        &self,
        id: u64,
        index: usize,
        shared_column_width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(clients) = self
            .tab_index(id)
            .and_then(|i| self.tabs[i].clients.as_ref())
        else {
            return div().into_any_element();
        };
        let Some(client) = clients.order.get(index).and_then(|i| clients.rows.get(*i)) else {
            return div().into_any_element();
        };
        let dark = self.dark(cx);
        let fade = |color| clients.fade(color, dark);
        let foreground = fade(if dark {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        });
        let border = fade(rgb(if dark { 0x58585e } else { 0xefeff5 }).into());
        let mut font = gpui::font(".SystemUIFont");
        font.features = FontFeatures(Arc::new(vec![("tnum".into(), 1)]));
        let last = index + 1 == clients.order.len();
        let values = [
            (index + 1).to_string(),
            client.get("addr").cloned().unwrap_or_default(),
            client.get("db").cloned().unwrap_or_default(),
            self.seconds_text(client.get("age").and_then(|v| v.parse().ok()).unwrap_or(0)),
            self.seconds_text(client.get("idle").and_then(|v| v.parse().ok()).unwrap_or(0)),
        ];
        let mut row = div()
            .id(format!("status-clients-row-{index}"))
            .test_support()
            .role(Role::Label)
            .aria_label(values.join("\t"))
            .w(px(60. + shared_column_width * 4.))
            .flex()
            .flex_shrink_0();
        for (j, text) in values.into_iter().enumerate() {
            let key = ["no", "addr", "db", "age", "idle"][j];
            let cell_width = if j == 0 { 60. } else { shared_column_width };
            let lines = wrapped_lines(
                &text,
                cell_width - 16. - if j < 4 { 1. } else { 0. },
                &font,
                window,
            );
            let mut label = div().flex().flex_col().items_center().flex_shrink_0();
            for (line, text) in lines.into_iter().enumerate() {
                label = label.child(
                    div()
                        .id(format!("status-clients-line-{index}-{j}-{line}"))
                        .test_support()
                        .role(Role::Label)
                        .aria_label(text.clone())
                        .h(px(21.))
                        .child(super::views::status_text(
                            text,
                            14.,
                            21.,
                            FontWeight::NORMAL,
                            &format!("status-clients-text-{index}-{j}-{line}"),
                            font.clone(),
                            window,
                            cx,
                        )),
                );
            }
            let selected = clients.sort.state().is_some_and(|(field, _)| field == key);
            let color = if index % 2 == 1 {
                if dark { 0x515157 } else { 0xfafafc }
            } else if selected {
                if dark { 0x535359 } else { 0xf7f7fa }
            } else if dark {
                0x48484e
            } else {
                0xffffff
            };
            let mut cell = div()
                .id(format!("status-clients-cell-{index}-{j}"))
                .test_support()
                .px(px(8.))
                .py(px(6.))
                .when(j < 4, |d| d.border_r_1())
                .when(!last, |d| d.border_b_1())
                .border_color(border)
                .bg(fade(rgb(color).into()))
                .flex()
                .items_center()
                .justify_center()
                .text_color(foreground)
                .child(label);
            cell = if j == 0 {
                cell.w(px(60.)).flex_shrink_0()
            } else {
                cell.w(px(shared_column_width)).flex_shrink_0().min_w_0()
            };
            row = row.child(cell);
        }
        row.into_any_element()
    }

    fn render_clients(
        &self,
        id: u64,
        arrow_x: f32,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(i) = self.tab_index(id) else {
            return div().into_any_element();
        };
        let Some(clients) = self.tabs[i].clients.as_ref() else {
            return div().into_any_element();
        };
        let dark = self.dark(cx);
        // Preblend the loading group while preserving glyph alpha.
        let fade = |color| clients.fade(color, dark);
        // The original tooltip is teleported outside the configured body font.
        let mut font = gpui::font(".SystemUIFont");
        font.features = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let foreground: Hsla = fade(if dark {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        });
        let header_foreground: Hsla = fade(if dark {
            rgba(0xffffffe6).into()
        } else {
            rgb(0x1f2225).into()
        });
        let border = fade(rgb(if dark { 0x58585e } else { 0xefeff5 }).into());
        let width = window.viewport_size().width.as_f32() * 0.7;
        let shared_column_width = (width - 2. - 60.) / 4.;
        let header_bg = fade(rgb(if dark { 0x535359 } else { 0xfafafc }).into());
        let mut header = div()
            .id("status-clients-headers")
            .test_support()
            .flex()
            .h(px(34.))
            .flex_shrink_0();
        for (j, key, sort) in [
            (0, "no", None),
            (1, "addr", Some(Column::Address)),
            (2, "db", None),
            (3, "age", Some(Column::Age)),
            (4, "idle", Some(Column::Idle)),
        ] {
            let selected = clients.sort.state().is_some_and(|(field, _)| field == key);
            let title = if key == "no" {
                "#".into()
            } else {
                self.tr(&format!("status.client.{key}"))
            };
            let label = div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .justify_center()
                .child(super::views::status_text(
                    title,
                    14.,
                    21.,
                    FontWeight::MEDIUM,
                    &format!("status-clients-header-text-{key}"),
                    font.clone(),
                    window,
                    cx,
                ));
            let mut cell = div()
                .id(format!("status-clients-header-{key}"))
                .test_support()
                .role(Role::ColumnHeader)
                .h(px(34.))
                .px(px(8.))
                .border_b_1()
                .when(j < 4, |d| d.border_r_1())
                .border_color(border)
                .bg(if selected {
                    fade(rgb(if dark { 0x5d5d63 } else { 0xf3f3f7 }).into())
                } else {
                    header_bg
                })
                .text_color(header_foreground)
                .flex()
                .items_center()
                .gap(px(4.))
                .child(label);
            cell = if j == 0 {
                cell.w(px(60.)).flex_shrink_0()
            } else {
                cell.w(px(shared_column_width)).flex_shrink_0().min_w_0()
            };
            if let Some(column) = sort {
                let ascending = clients.sort.state() == Some((key, "ascend"));
                cell = cell
                    .cursor_pointer()
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.update_clients(id, Some(column), window, cx);
                    }))
                    .child(
                        div()
                            .w(px(15.))
                            .h(px(15.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(fade(if selected {
                                cx.theme().primary
                            } else if dark {
                                rgba(0xffffff61).into()
                            } else {
                                rgb(0xc2c2c2).into()
                            }))
                            .child(
                                icon(if ascending {
                                    "entry-sort-up"
                                } else {
                                    "entry-sort-down"
                                })
                                .size(px(14.)),
                            ),
                    );
            }
            header = header.child(cell);
        }
        let layout = (width, self.pref.general.language.clone());
        let layout_changed = clients.layout.borrow().as_ref() != Some(&layout);
        if layout_changed {
            clients.list.remeasure();
            *clients.layout.borrow_mut() = Some(layout);
        }
        let height = px(window.viewport_size().height.as_f32() * 0.5);
        let list_state = clients.list.clone();
        let parent = cx.entity_id();
        let mut body = div()
            .id("clients-scroll")
            .test_support()
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .max_h(height)
            .overflow_hidden();
        if !clients.order.is_empty() {
            body = body.child(
                gpui::list(
                    clients.list.clone(),
                    cx.processor(move |app, index, window, cx| {
                        app.render_client_row(id, index, shared_column_width, window, cx)
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .w_full()
                .max_h(height)
                .flex_shrink_0(),
            );
        }
        if layout_changed && !clients.order.is_empty() {
            // GPUI discards height hints during the first paint and width changes.
            body = body.child(
                canvas(
                    move |_, _, cx| {
                        list_state.clone().with_uniform_item_height(px(34.));
                        cx.notify(parent);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        }
        if clients.order.is_empty() {
            let text = if self.pref.general.language == "zh" {
                "无数据"
            } else {
                "No Data"
            };
            let label_width = super::text::script_width(text, font.clone(), 14., window).max(40.);
            body = body.child(
                div()
                    .h(px(165.))
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("status-clients-empty")
                            .test_support()
                            .w(px(label_width))
                            .h(px(69.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(8.))
                            .text_color(fade(rgb(if dark { 0x606060 } else { 0xc2c2c2 }).into()))
                            .child(
                                div()
                                    .id("status-clients-empty-icon")
                                    .test_support()
                                    .size(px(40.))
                                    .child(icon("empty").size(px(40.))),
                            )
                            .child(
                                div()
                                    .id("status-clients-empty-description")
                                    .test_support()
                                    .h(px(21.))
                                    .w(px(label_width))
                                    .child(super::views::status_text(
                                        text.into(),
                                        14.,
                                        21.,
                                        FontWeight::NORMAL,
                                        "status-clients-empty-text",
                                        font.clone(),
                                        window,
                                        cx,
                                    )),
                            ),
                    ),
            );
        }
        let title = div()
            .id("status-clients-title")
            .test_support()
            .h(px(35.))
            .flex_shrink_0()
            .px(px(8.))
            .border_b_1()
            .border_color(border)
            .bg(header_bg)
            .text_color(header_foreground)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .child(super::views::status_text(
                self.tr("status.client.title"),
                15.,
                21.,
                FontWeight(550.),
                "status-clients-title-text",
                font.clone(),
                window,
                cx,
            ))
            .child(
                Button::new("refresh-clients")
                    .ghost()
                    .tab_stop(false)
                    .tab_index(-1)
                    .rounded_none()
                    .w(px(18.))
                    .h(px(22.))
                    .p_0()
                    .text_color(foreground)
                    .icon(icon("refresh").size(px(16.)))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.update_clients(id, None, window, cx);
                    })),
            );
        let table = div()
            .id("status-clients-table")
            .test_support()
            .w(px(width))
            .border_1()
            .border_color(border)
            .rounded(px(4.))
            .overflow_hidden()
            .bg(rgb(if dark { 0x18181c } else { 0xffffff }))
            .flex()
            .flex_col()
            .font(font)
            .text_size(px(14.))
            .line_height(px(21.))
            .font_weight(FontWeight::NORMAL)
            .child(title)
            .child(header)
            .child(body);
        let table = div()
            .relative()
            .child(table)
            .when(clients.loading, |container| {
                let started = clients.started;
                #[cfg(feature = "ui-qa")]
                let phase = clients.gate.as_ref().map(|_| 0.4_f32);
                #[cfg(not(feature = "ui-qa"))]
                let phase: Option<f32> = None;
                let reduce_motion = cx.reduce_motion();
                container.child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .id("status-clients-loading")
                                .test_support()
                                .role(Role::Label)
                                .aria_label("loading")
                                .size(px(28.))
                                .child(
                                    canvas(
                                        |_, _, _| {},
                                        move |bounds, _, window, _| {
                                            let seconds = phase.unwrap_or_else(|| {
                                                if reduce_motion {
                                                    0.4
                                                } else {
                                                    started.elapsed().as_secs_f32()
                                                }
                                            });
                                            paint_loading(bounds, seconds, window);
                                            if phase.is_none() && !reduce_motion {
                                                window.request_animation_frame();
                                            }
                                        },
                                    )
                                    .size_full(),
                                ),
                        ),
                )
            });
        let background = rgb(if dark { 0x2c2c32 } else { 0x262626 });
        let half = 8.46875_f32 / std::f32::consts::SQRT_2;
        let arrow = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let center = bounds.center();
                let radius = bounds.size.width * 0.5;
                let mut path = PathBuilder::fill();
                path.move_to(center - point(px(0.), radius));
                path.line_to(center + point(radius, px(0.)));
                path.line_to(center + point(px(0.), radius));
                path.line_to(center - point(radius, px(0.)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, background);
                }
            },
        )
        .absolute()
        .left(px(arrow_x - half))
        .top(px(-half))
        .size(px(half * 2.));
        div()
            .id("status-clients-popup")
            .test_support()
            .relative()
            .w(px(width + 28.))
            .px(px(14.))
            .py(px(8.))
            .rounded(px(4.))
            .bg(background)
            .shadow(super::notices::shadows(dark))
            .child(arrow)
            .child(
                div()
                    .id("status-clients-content")
                    .test_support()
                    .w(px(width))
                    .bg(rgb(if dark { 0x18181c } else { 0xffffff }))
                    .child(table),
            )
            .into_any_element()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{cmp::Ordering, ffi::c_void, ptr};
    #[repr(C)]
    struct Range {
        location: isize,
        length: isize,
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            len: isize,
            encoding: u32,
            external: u8,
        ) -> *const c_void;
        fn CFStringGetLength(value: *const c_void) -> isize;
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFStringCompareWithOptionsAndLocale(
            a: *const c_void,
            b: *const c_void,
            range: Range,
            flags: usize,
            locale: *const c_void,
        ) -> isize;
        fn CFRelease(value: *const c_void);
    }
    struct Owned(*const c_void);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.0);
            }
        }
    }
    fn owned(value: *const c_void) -> Option<Owned> {
        (!value.is_null()).then(|| Owned(value))
    }
    pub(super) fn compare(a: &str, b: &str) -> Option<Ordering> {
        unsafe {
            let a = owned(CFStringCreateWithBytes(
                ptr::null(),
                a.as_ptr(),
                a.len() as isize,
                0x08000100,
                0,
            ))?;
            let b = owned(CFStringCreateWithBytes(
                ptr::null(),
                b.as_ptr(),
                b.len() as isize,
                0x08000100,
                0,
            ))?;
            let locale = owned(CFLocaleCopyCurrent())?;
            Some(
                CFStringCompareWithOptionsAndLocale(
                    a.0,
                    b.0,
                    Range {
                        location: 0,
                        length: CFStringGetLength(a.0),
                    },
                    0,
                    locale.0,
                )
                .cmp(&0),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_sort_is_stable_and_has_three_states() {
        use super::{ClientList, ClientSort, Column, sorted_indices};
        let rows = [3, 11, 11]
            .into_iter()
            .map(|age| [("age".to_string(), age.to_string())].into())
            .collect();
        let sort = ClientSort::default();
        assert_eq!(sorted_indices(&rows, sort), [1, 2, 0]);
        let sort = sort.next(Column::Age);
        assert_eq!(sorted_indices(&rows, sort), [0, 1, 2]);
        let sort = sort.next(Column::Age);
        assert_eq!(sort.state(), None);
        assert_eq!(sorted_indices(&rows, sort), [0, 1, 2]);
        assert_eq!(
            sort.next(Column::Address).state(),
            Some(("addr", "descend"))
        );
        let mut clients = ClientList::new();
        clients.replace_rows(std::sync::Arc::new(rows), vec![0, 1, 2]);
        let weak = std::sync::Arc::downgrade(&clients.rows);
        drop(clients);
        assert!(weak.upgrade().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn rdm_address_collation_matches_original() {
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/status-clients-collation-macos-source.json"
        ))
        .unwrap();
        let pairs = source["pairs"].as_array().unwrap();
        assert_eq!(pairs.len(), 361);
        let mut failed = vec![];
        for pair in pairs {
            let a = pair["a"].as_str().unwrap();
            let b = pair["b"].as_str().unwrap();
            let actual = match super::compare_addresses(a, b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            if actual != pair["order"].as_i64().unwrap() {
                failed.push((a, b, actual, pair["order"].clone()));
            }
        }
        assert!(
            failed.is_empty(),
            "OriginalJS localeCompare pairs: {failed:?}"
        );
    }
}
