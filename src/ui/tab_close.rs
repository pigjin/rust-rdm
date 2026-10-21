use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

pub(super) struct CloseWarning {
    pub(super) id: u64,
    pub(super) name: String,
}

impl RdmApp {
    pub(super) fn request_tab_close(
        &mut self,
        id: u64,
        blur: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        let name = tab.conn.name.clone();
        self.next_dialog_id += 1;
        self.tab_close_warnings.push(CloseWarning {
            id: self.next_dialog_id,
            name,
        });
        // The tab close button blurs the active element. The global Cmd/Ctrl+W
        // handler calls the same warning API without changing editor focus.
        if blur {
            // The application focus handle represents the document body, so
            // Escape remains available after the editable element blurs.
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_tab_close(&mut self, cx: &mut Context<Self>) -> bool {
        let removed = self.tab_close_warnings.pop().is_some();
        if removed {
            cx.notify();
        }
        removed
    }

    fn dismiss_tab_close(&mut self, id: u64, cx: &mut Context<Self>) {
        self.tab_close_warnings.retain(|warning| warning.id != id);
        cx.notify();
    }

    fn confirm_tab_close(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tab_close_warnings.iter().position(|warning| warning.id == id) else {
            return;
        };
        let name = self.tab_close_warnings.remove(index).name;
        let service = self.service.clone();
        let closing_name = name.clone();
        // The original warning closes immediately; its callback does not return
        // the RPC promise. The tab and draft remain until CloseConnection ends.
        // The pinned Go endpoint always reports success, as does Service::close.
        let task = cx.background_executor().spawn(async move {
            service.close(&closing_name);
        });
        cx.spawn_in(window, async move |this, cx| {
            task.await;
            let _ = this.update_in(cx, |app, _, cx| app.remove_closed_tab(&name, cx));
        })
        .detach();
        cx.notify();
    }

    pub(super) fn remove_closed_tab(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.conn.name == name) else {
            return;
        };
        let active_index = self.index().unwrap_or(0);
        let removed = self.tabs.remove(index);
        self.cached_entry_panels.remove(&removed.id);
        // tabStore.removeTab decrements the active index for every removal,
        // including removal of an inactive tab; retain that source behavior.
        let active = self.tabs.get(active_index.saturating_sub(1)).map(|tab| tab.id);
        self.activate_connection_tab(active, cx);
        if self.active.is_none() {
            self.nav = Nav::Server;
        }
        cx.notify();
    }

    pub(super) fn render_tab_close_warning(
        &self,
        warning: &CloseWarning,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = warning.id;
        let dark = self.dark(cx);
        let foreground = if dark { rgba(0xffffffd1) } else { rgb(0x333639) };
        let warning_color = if dark { rgb(0xf2c97d) } else { rgb(0xf0a020) };
        let element = |part: &str| format!("tab-close-{part}-{id}");
        div()
            .id(element("mask"))
            .test_support()
            .absolute()
            .inset(px(-1.))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, _, cx| app.dismiss_tab_close(id, cx)),
            )
            .child(
                div()
                    .id(element("card"))
                    .test_support()
                    .w(px(446.))
                    .px(px(28.))
                    .pt(px(16.))
                    .pb(px(20.))
                    .rounded(px(4.))
                    .bg(if dark { rgb(0x2c2c32) } else { rgb(0xffffff) })
                    .shadow_lg()
                    .font({
                        let mut font = gpui::font(".SystemUIFont");
                        font.fallbacks = Some(FontFallbacks::default());
                        font
                    })
                    .text_size(px(14.))
                    .text_color(foreground)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id(element("title"))
                            .test_support()
                            .flex()
                            .items_center()
                            .h(px(28.))
                            .gap(px(4.))
                            .text_size(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if dark { rgba(0xffffffe6) } else { rgb(0x1f2225) })
                            .child(icon("notice-warning").size(px(28.)).text_color(warning_color))
                            .child(self.tr("common.warning")),
                    )
                    .child(
                        div()
                            .id(element("content"))
                            .test_support()
                            .mt(px(8.))
                            .mb(px(16.))
                            .min_h(px(21.))
                            .line_height(px(21.))
                            .child(self.tr("dialogue.close_confirm").replace("{name}", &warning.name)),
                    )
                    .child(
                        div()
                            .id(element("actions"))
                            .test_support()
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .h(px(28.))
                            .child(
                                self.btn(element("cancel"), self.tr("common.cancel"), "", cx,
                                    move |app, _, cx| app.dismiss_tab_close(id, cx))
                                    .h(px(28.)).px(px(7.)).rounded(px(4.))
                                    .bg(rgba(0x00000000))
                                    .border_color(if dark { rgba(0xffffff3d) } else { rgb(0xe0e0e6) })
                                    .text_color(foreground),
                            )
                            .child(
                                self.btn(element("confirm"), self.tr("common.confirm"), "", cx,
                                    move |app, window, cx| app.confirm_tab_close(id, window, cx))
                                    .warning().h(px(28.)).px(px(7.)).rounded(px(4.))
                                    .bg(warning_color).border_color(warning_color)
                                    .text_color(if dark { rgb(0x000000) } else { rgb(0xffffff) }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
