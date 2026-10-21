use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

impl Tab {
    pub(super) fn status_info_rows(&self, keyword: &str) -> Vec<(&str, &str)> {
        let selected = self
            .info
            .get(&self.info_group)
            .filter(|values| !values.is_empty());
        // An empty/missing selected group falls back to all sections, as in
        // the original computed info. The table filter is case sensitive.
        selected
            .into_iter()
            .flat_map(|values| values.iter())
            .chain(
                self.info
                    .values()
                    .filter(|_| selected.is_none())
                    .flat_map(|values| values.iter()),
            )
            .filter(|(key, _)| key.contains(keyword))
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }
}

impl RdmApp {
    pub(super) fn render_status_info_filter(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let mut families = self.pref.general.font_family.clone();
        if families.is_empty() {
            families.push(
                self.preference_font("general", ".SystemUIFont", cx)
                    .family
                    .to_string(),
            );
        }
        #[cfg(target_os = "macos")]
        let width = super::text_control_width::intrinsic(&families, 14., 20) + 66.;
        #[cfg(not(target_os = "macos"))]
        let width = 241.;
        let input = gpui::Styled::h(
            self.search_input(&tab.info_filter, window, cx)
                .id("status-info-filter-input")
                .prefix(icon("filter").size(px(18.)))
                .gap(px(4.))
                .pl(px(11.))
                .pr(px(31.))
                .rounded(px(4.))
                .w_full(),
            px(32.),
        );
        div()
            .id("status-info-filter")
            .test_support()
            .relative()
            .w(px(width))
            .h(px(32.))
            .font(self.preference_font("general", ".SystemUIFont", cx))
            .child(input)
            .child(
                div()
                    .absolute()
                    .right(px(12.))
                    .top(px(8.))
                    .size(px(16.))
                    .when(!text_input(&tab.info_filter, cx).is_empty(), |slot| {
                        slot.child(self.search_clear_button(
                            "clear-status-info-filter",
                            "status-info-filter-input",
                            tab.info_filter.focus_handle(cx).is_focused(window),
                            cx,
                            move |app, window, cx| {
                                if let Some(i) = app.tab_index(id) {
                                    let input = app.tabs[i].info_filter.clone();
                                    set_input(&input, "", window, cx);
                                    input.update(cx, |input, cx| input.focus(window, cx));
                                    cx.notify();
                                }
                            },
                        ))
                    }),
            )
            .children(self.search_input_ring(&tab.info_filter, window, cx))
            .into_any_element()
    }

    pub(super) fn render_status_info(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let dark = self.dark(cx);
        let font = self.preference_font("general", ".SystemUIFont", cx);
        let fg: Hsla = if dark {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let border: Hsla = rgb(if dark { 0x2d2d30 } else { 0xefeff5 }).into();
        let widths: Vec<_> = tab
            .info
            .keys()
            .map(|key| {
                (
                    key,
                    super::text::script_width(key, font.clone(), 14., window).max(80.) + 16.,
                )
            })
            .collect();
        let group_width = widths.iter().map(|(_, w)| *w).fold(0f32, f32::max) + 10.;
        let mut groups = div()
            .id("status-info-groups")
            .test_support()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(8.))
            .px(px(5.))
            .w(px(group_width))
            .h_full()
            .flex_shrink_0();
        for (group, width) in widths {
            let name = group.clone();
            let selected = tab.info_group == *group;
            let disabled = tab.info[group].is_empty();
            let color = if selected { cx.theme().primary } else { fg };
            let idle: Hsla = if selected && !disabled {
                cx.theme().primary.opacity(0.16)
            } else if dark {
                rgba(0xffffff14).into()
            } else {
                rgba(0x2e33380d).into()
            };
            let hover_color: Hsla = if selected {
                cx.theme().primary.opacity(0.22)
            } else if dark {
                rgba(0xffffff1f).into()
            } else {
                rgba(0x2e333817).into()
            };
            let pressed_color: Hsla = if selected {
                cx.theme().primary.opacity(0.28)
            } else if dark {
                rgba(0xffffff29).into()
            } else {
                rgba(0x2e333821).into()
            };
            let mut hover = div().bg(hover_color).text_color(color);
            let mut pressed = div().bg(pressed_color).text_color(color);
            groups = groups.child(
                Button::new(SharedString::from(format!("info-group-{group}")))
                    .ghost()
                    .tab_stop(false)
                    .tab_index(-1)
                    .disabled(disabled)
                    .accessibility_label(group.clone())
                    .w(px(width))
                    .h(px(28.))
                    .p_0()
                    .rounded(px(4.))
                    .bg(idle)
                    .text_color(color)
                    .hover_style(hover.style().clone())
                    .active_style(pressed.style().clone())
                    .when(disabled, |button| {
                        button.opacity(if dark { 0.38 } else { 0.5 })
                    })
                    .child(super::views::status_text(
                        group.clone(),
                        14.,
                        14.,
                        FontWeight::NORMAL,
                        &format!("status-info-group-text-{group}"),
                        font.clone(),
                        window,
                        cx,
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if let Some(i) = app.tab_index(id) {
                            let tab = &mut app.tabs[i];
                            tab.info_group = if tab.info_group == name {
                                String::new()
                            } else {
                                name.clone()
                            };
                            cx.notify();
                        }
                    })),
            );
        }
        let keyword = text_input(&tab.info_filter, cx);
        let rows = tab.status_info_rows(&keyword);
        let mut header = div().flex().flex_shrink_0().h(px(46.)).w_full();
        for (index, key) in ["key", "value"].into_iter().enumerate() {
            header = header.child(
                div()
                    .id(format!("status-info-header-{key}"))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h(px(46.))
                    .px(px(12.))
                    .border_b_1()
                    .border_color(border)
                    .when(index == 0, |cell| cell.border_r_1().min_w(px(80.)))
                    .bg(rgb(if dark { 0x26262a } else { 0xfafafc }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(if dark {
                        Hsla::from(rgba(0xffffffe6))
                    } else {
                        rgb(0x1f2225).into()
                    })
                    .child(super::views::status_text(
                        self.tr(&format!("common.{key}")),
                        14.,
                        21.,
                        FontWeight::MEDIUM,
                        &format!("status-info-header-text-{key}"),
                        font.clone(),
                        window,
                        cx,
                    )),
            );
        }
        let mut body = div()
            .id("status-info-body")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for (index, (key, value)) in rows.iter().enumerate() {
            let last = index + 1 == rows.len();
            let mut row = div()
                .id(format!("status-info-row-{index}"))
                .test_support()
                .role(Role::Label)
                .aria_label(format!("{key}\t{value}"))
                .flex()
                .flex_shrink_0()
                .w_full();
            for (column, text) in [*key, *value].into_iter().enumerate() {
                row = row.child(
                    div()
                        .id(format!("status-info-cell-{index}-{column}"))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .px(px(12.))
                        .py(px(12.))
                        .border_color(border)
                        .when(column == 0, |cell| cell.border_r_1().min_w(px(80.)))
                        .when(!last, |cell| cell.border_b_1())
                        .bg(rgb(if dark {
                            if index % 2 == 0 { 0x18181c } else { 0x242427 }
                        } else if index % 2 == 0 {
                            0xffffff
                        } else {
                            0xfafafc
                        }))
                        .child(text.to_string()),
                );
            }
            body = body.child(row);
        }
        if rows.is_empty() {
            body = body.child(
                div()
                    .id("status-info-empty")
                    .test_support()
                    .flex_1()
                    .min_h(px(165.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(icon("empty").size(px(40.)).text_color(rgb(if dark {
                        0x606060
                    } else {
                        0xc2c2c2
                    })))
                    .child(
                        div()
                            .text_color(if dark {
                                Hsla::from(rgba(0xffffff61))
                            } else {
                                rgb(0xc2c2c2).into()
                            })
                            .child(if self.pref.general.language == "zh" {
                                "无数据"
                            } else {
                                "No Data"
                            }),
                    ),
            );
        }
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .p(px(10.))
            .gap(px(12.))
            .font(font)
            .text_size(px(14.))
            .line_height(px(21.))
            .text_color(fg)
            .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
            .child(groups)
            .child(
                div()
                    .id("status-info-table")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(border)
                    .overflow_hidden()
                    .child(header)
                    .child(body),
            )
            .into_any_element()
    }
}
