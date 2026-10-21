use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

impl RdmApp {
    pub(super) fn render_connection_tabs(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let active = self.index();
        let mark: Option<Hsla> = active.and_then(|index| {
            gpui::Rgba::try_from(self.tabs[index].conn.mark_color.as_str()).ok().map(Into::into)
        });
        let height = if mark.is_some() { 38. } else { 36. };
        let dark = self.dark(cx);
        let close_color = if dark { rgba(0xffffff85) } else { rgb(0x666666) };
        let mut tabs = div()
            .id("connection-tabs")
            .test_support()
            .flex()
            .flex_auto()
            .min_w_0()
            .ml(px(3.))
            .mb(px(-1.))
            .px(px(3.))
            .h(px(height))
            .text_size(px(14.))
            .line_height(px(18.))
            .overflow_x_scroll()
            .track_scroll(&self.connection_tabs_scroll);
        for (index, tab) in self.tabs.iter().enumerate() {
            let id = tab.id;
            let selected = self.active == Some(id);
            let mut font = self.preference_font("general", ".SystemUIFont", cx);
            font.weight = if selected { FontWeight::EXTRA_BOLD } else { FontWeight::NORMAL };
            let label_width = window.text_system().shape_line(
                tab.conn.name.clone().into(), px(14.),
                &[TextRun { len: tab.conn.name.len(), font, ..Default::default() }], None,
            ).width().min(px(150.));
            let marked = if selected { mark } else { None };
            let background = if selected { cx.theme().background } else { rgba(0x00000000).into() };
            let divider = !selected && active.and_then(|i| i.checked_sub(1)) != Some(index);
            let content = div().flex().flex_1().min_h_0().items_center().gap(px(6.))
                .pl(px(16.)).pr(px(8.)).pt(px(8.)).pb(px(9.))
                .border_l_1().border_r_1()
                .border_color(if selected { self.split_color(cx) } else { rgba(0x00000000).into() })
                .child(
                    div().id(("connection-tab-label", id)).test_support()
                        .flex().flex_shrink_0().items_center().gap(px(5.)).h(px(18.))
                        .child(div().id(("connection-tab-icon", id)).test_support()
                            .size(px(18.)).flex_shrink_0().child(icon("tab-server").size(px(18.))))
                        .child(div().id(("connection-tab-text", id)).test_support()
                            .w(label_width).max_w(px(150.)).truncate().line_height(px(18.)).child(tab.conn.name.clone())),
                )
                .child(
                    self.btn(format!("close-{id}"), "", "", cx, move |app, window, cx| {
                        cx.stop_propagation();
                        app.request_tab_close(id, true, window, cx);
                    })
                    .ghost()
                    .with_size(gpui_component::Size::Size(px(18.)))
                    .child(div().id(("connection-tab-close-icon", id)).test_support()
                        .size(px(14.)).child(icon("notice-close").size(px(14.))))
                    .w(px(18.)).h(px(18.)).p_0().rounded(px(4.))
                    .font_weight(FontWeight::NORMAL).text_color(close_color),
                );
            let top = div().id(("connection-tab-mark", id)).test_support()
                .h(px(if marked.is_some() { 3. } else { 1. })).w_full().flex_shrink_0()
                .bg(if selected { marked.unwrap_or_else(|| self.split_color(cx)) }
                    else { rgba(0x00000000).into() });
            let mut item = div()
                .id(("connection-tab", id))
                .test_support()
                .group("connection-tab")
                .relative()
                .flex()
                .flex_shrink_0()
                .flex_col()
                .h_full()
                .rounded_t(px(4.))
                .bg(background)
                .when(!selected, |item| item.hover(|style| style.bg(self.split_color(cx))))
                .font_weight(if selected { FontWeight::EXTRA_BOLD } else { FontWeight::NORMAL })
                .text_color(if selected { cx.theme().primary } else { close_color.into() })
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.activate_connection_tab(Some(id), cx);
                    if app.dialog.is_none() || app.editing_entry() { app.focus.focus(window, cx); }
                    cx.notify();
                }))
                .child(div().flex().flex_col().h_full().rounded_t(px(4.)).overflow_hidden()
                    .child(top).child(content));
            if divider {
                item = item.child(div().absolute().top(px((height - 2.) * 0.25)).right(px(-2.))
                    .h(px((height - 2.) * 0.5)).w(px(1.)).bg(cx.theme().border)
                    .group_hover("connection-tab", |line| line.opacity(0.)));
            }
            tabs = tabs.child(item);
        }
        tabs.into_any_element()
    }
}
