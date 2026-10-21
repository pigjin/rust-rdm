use super::text::{ScriptLabel, script_width};
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

pub(super) const CONTENT_PANES: [(Pane, &str, &str); 6] = [
    (Pane::Status, "status", "status"),
    (Pane::Detail, "detail", "key_detail"),
    (Pane::Cli, "terminal", "cli"),
    (Pane::Slow, "log", "slow_log"),
    (Pane::Monitor, "monitor", "cmd_monitor"),
    (Pane::Pubsub, "subscribe", "pub_message"),
];

impl RdmApp {
    fn header_text_width(&self, text: &str, weight: FontWeight, window: &Window, cx: &App) -> f32 {
        let mut font = self.preference_font("general", ".SystemUIFont", cx);
        font.weight = weight;
        script_width(text, font, 14., window)
    }

    pub(super) fn render_content_nav(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let id = tab.id;
        let available =
            (window.viewport_size().width.as_f32() - self.pref.behavior.aside_width - 52.).max(0.);
        let labels = CONTENT_PANES.map(|(_, _, key)| self.tr(&format!("interface.sub_tab.{key}")));
        let widths = labels
            .each_ref()
            .map(|label| self.header_text_width(label, FontWeight::NORMAL, window, cx) + 41.);
        let total = widths.iter().sum::<f32>() + 50.;
        let padding = if total + 10. <= available { 5. } else { 0. };
        let max_offset = (total.round() - available).max(0.);
        let active = CONTENT_PANES
            .iter()
            .position(|(pane, _, _)| *pane == tab.pane)
            .unwrap();
        let left = padding + widths[..active].iter().sum::<f32>() + active as f32 * 10.;
        if tab.content_nav_layout.get() != Some((tab.pane, available, widths)) {
            let current = tab.content_nav_offset.get();
            let next = if current > left.round() {
                left.round()
            } else if left.round() + widths[active].round() > current + available {
                left.round() + widths[active].round() - available
            } else {
                current
            };
            tab.content_nav_offset.set(next.clamp(0., max_offset));
            tab.content_nav_layout
                .set(Some((tab.pane, available, widths)));
        }
        let offset = tab.content_nav_offset.get().clamp(0., max_offset);
        let normal: Hsla = if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x1f2225).into()
        };
        let primary: Hsla = rgb(0xd33a31).into();
        let mut row = div()
            .id("content-nav-row")
            .test_support()
            .absolute()
            .left(px(-offset))
            .top_0()
            .w(px(available.max(total + padding * 2.)))
            .h_full()
            .flex()
            .items_start()
            .gap(px(10.))
            .px(px(padding));
        for (index, (pane, image, key)) in CONTENT_PANES.into_iter().enumerate() {
            let selected = pane == tab.pane;
            row =
                row.child(
                    div()
                        .id(format!("pane-{key}"))
                        .test_support()
                        .role(Role::Tab)
                        .aria_label(labels[index].clone())
                        .aria_selected(selected)
                        .w(px(widths[index]))
                        .h(px(CONTENT_TABS_HEIGHT - 1.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(5.))
                        .px(px(10.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::NORMAL)
                        .line_height(px(18.2))
                        .text_color(if selected { primary } else { normal })
                        .cursor_pointer()
                        .hover(move |style| style.text_color(primary))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.switch_pane(id, pane, window, cx)
                        }))
                        .child(
                            div()
                                .id(format!("pane-icon-{key}"))
                                .test_support()
                                .size(px(16.))
                                .flex_shrink_0()
                                .child(
                                    icon(&format!(
                                        "nav-{image}{}",
                                        if selected { "-active" } else { "" }
                                    ))
                                    .size(px(16.)),
                                ),
                        )
                        .child(div().id(format!("pane-label-{key}")).test_support().child(
                            ScriptLabel {
                                text: labels[index].clone(),
                            },
                        )),
                );
        }
        row = row.child(
            div()
                .id("content-nav-indicator")
                .test_support()
                .absolute()
                .left(px(left.round()))
                .bottom_0()
                .w(px(widths[active].round()))
                .h(px(2.))
                .bg(primary),
        );
        div()
            .id("content-nav")
            .test_support()
            .role(Role::TabList)
            .relative()
            .w_full()
            .h(px(CONTENT_TABS_HEIGHT))
            .flex_shrink_0()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(move |app, event: &ScrollWheelEvent, _, cx| {
                if max_offset > 0. {
                    if let Some(i) = app.tab_index(id) {
                        let delta = event.delta.pixel_delta(px(20.));
                        let next = (app.tabs[i].content_nav_offset.get()
                            - delta.x.as_f32()
                            - delta.y.as_f32())
                        .clamp(0., max_offset);
                        app.tabs[i].content_nav_offset.set(next);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(cx.theme().border),
            )
            .child(row)
            .when(offset > 0., |nav| {
                nav.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(20.))
                        .shadow(vec![
                            BoxShadow::new(px(10.), px(0.), rgba(0x0000001f).into())
                                .blur_radius(px(8.))
                                .spread_radius(px(-8.))
                                .inset(),
                        ]),
                )
            })
            .when(offset < max_offset, |nav| {
                nav.child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(20.))
                        .shadow(vec![
                            BoxShadow::new(px(-10.), px(0.), rgba(0x0000001f).into())
                                .blur_radius(px(8.))
                                .spread_radius(px(-8.))
                                .inset(),
                        ]),
                )
            })
            .into_any_element()
    }
}

impl RdmApp {
    pub(super) fn key_button_style(&self, button: Button, cx: &App) -> Button {
        let normal: Hsla = if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let mut hover = div()
            .text_color(cx.theme().primary_hover)
            .border_color(cx.theme().primary_hover)
            .bg(Hsla::transparent_black());
        let mut active = div()
            .text_color(cx.theme().primary_active)
            .border_color(cx.theme().primary_active)
            .bg(Hsla::transparent_black());
        button
            .text_color(normal)
            .bg(Hsla::transparent_black())
            .border_color(cx.theme().border)
            .hover_style(hover.style().clone())
            .active_style(active.style().clone())
    }

    fn key_toolbar_button(
        &self,
        name: &'static str,
        image: &str,
        tip: String,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        let mut content = div().text_size(px(14.)).line_height(px(14.)).gap(px(6.));
        self.btn(name, "", image, cx, handler)
            .content_style(content.style().clone(), gpui_component::Size::Size(px(18.)))
            .w(px(42.))
            .px_0()
            .h(px(32.))
            .text_size(px(14.))
            .line_height(px(14.))
            .rounded(px(4.))
            .tooltip(tip)
            .tooltip_show_delay(Duration::from_millis(800))
            .map(|button| self.key_button_style(button, cx))
    }

    pub(super) fn render_key_toolbar(
        &self,
        i: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tab = &self.tabs[i];
        let detail = tab.detail.as_ref().unwrap();
        let id = tab.id;
        let key = display_bytes(&detail.key);
        let copy_key = key.clone();
        let double_key = key.clone();
        let key_focused = tab.key_name.focus_handle(cx).is_focused(window);
        let binary = contains_binary(&detail.key);
        let ttl = if detail.ttl < 0 {
            self.tr("interface.forever")
        } else {
            let value = detail
                .ttl
                .saturating_sub(tab.detail_loaded.elapsed().as_secs() as i64)
                .max(0);
            let time = format!(
                "{:02}:{:02}:{:02}",
                value % 86400 / 3600,
                value % 3600 / 60,
                value % 60
            );
            if value >= 86400 {
                format!("{}{} {time}", value / 86400, self.tr("common.unit_day"))
            } else {
                time
            }
        };
        let (foreground, background) = detail.kind.colors();
        let kind = detail.kind.name();
        let type_width = self.header_text_width(&kind, FontWeight::BOLD, window, cx)
            + 24.
            + if binary { 22. } else { 0. };
        let numbers = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let mut ttl_font = self.preference_font("general", ".SystemUIFont", cx);
        ttl_font.features = numbers.clone();
        let ttl_width = script_width(&ttl, ttl_font, 14., window) + 48.;
        let border = cx.theme().border;
        let text_color: Hsla = if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let mut tag = div()
            .id("key-type-tag")
            .test_support()
            .w(px(type_width))
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded_l(px(4.))
            .text_size(px(14.))
            .line_height(px(14.))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(foreground))
            .bg(rgb(background))
            .border_1()
            .border_color(border);
        if binary {
            tag = tag.child(
                div()
                    .id("key-type-binary")
                    .test_support()
                    .size(px(18.))
                    .child(icon("binary").size(px(18.))),
            );
        }
        tag = tag.child(
            div()
                .id("key-type-label")
                .test_support()
                .child(ScriptLabel {
                    text: kind.to_owned(),
                }),
        );
        let group = div()
            .id("key-input-group")
            .test_support()
            .flex_1()
            .min_w_0()
            .h(px(32.))
            .flex()
            .child(tag)
            .child(
                div()
                    .id("key-name")
                    .test_support()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .ml(px(-1.))
                    .h(px(32.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .text_color(text_color)
                    .bg(if self.dark(cx) {
                        rgba(0xffffff1a)
                    } else {
                        rgba(0xffffffff)
                    })
                    .border_1()
                    .border_color(if key_focused {
                        cx.theme().primary_hover
                    } else if self.dark(cx) {
                        Hsla::transparent_black()
                    } else {
                        border
                    })
                    .hover(|style| style.border_color(cx.theme().primary_hover))
                    .when(key_focused, |style| {
                        style.shadow(vec![
                            BoxShadow::new(px(0.), px(0.), cx.theme().primary.opacity(0.2))
                                .blur_radius(px(0.))
                                .spread_radius(px(2.)),
                        ])
                    })
                    .child(
                        div()
                            .id("key-name-text")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .h(px(32.))
                            .capture_any_mouse_down(cx.listener(
                                move |app, event: &MouseDownEvent, _, cx| {
                                    if event.button == MouseButton::Left && event.click_count == 2 {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            double_key.clone(),
                                        ));
                                        app.toast(app.tr("interface.copy_succ"), false);
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(
                                Input::new(&tab.key_name)
                                    .id("key-name-input")
                                    .accessibility_id("key-name-input")
                                    .aria_label(key)
                                    .readonly(true)
                                    .editor_style(gpui_base::input::InputEditorStyle {
                                        foreground: text_color,
                                        selection: rgb(if self.dark(cx) {
                                            0x607ea5
                                        } else {
                                            0xb3d7ff
                                        })
                                        .into(),
                                        ..Default::default()
                                    })
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .px_0()
                                    .py_0()
                                    .h(px(32.))
                                    .min_h(px(32.))
                                    .max_h(px(32.))
                                    .text_size(px(14.))
                                    .line_height(px(32.))
                                    .text_color(text_color),
                            ),
                    )
                    .child(
                        div()
                            .id("key-reload-suffix")
                            .test_support()
                            .w(px(18.))
                            .h(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(self.refresh_control(id, RefreshTarget::Detail, cx)),
                    ),
            )
            .child(
                self.key_toolbar_button(
                    "copy-key",
                    "copy",
                    self.tr("interface.copy_key"),
                    cx,
                    move |app, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_key.clone()));
                        app.toast(app.tr("interface.copy_succ"), false);
                        cx.notify();
                    },
                )
                .ml(px(-1.))
                .rounded_l(px(0.)),
            );
        let mut ttl_content = div().text_size(px(14.)).line_height(px(14.)).gap(px(6.));
        let ttl_label = ttl.clone();
        let ttl_group = div()
            .id("key-ttl-group")
            .test_support()
            .flex_shrink_0()
            .flex()
            .h(px(32.))
            .child(
                self.btn("ttl", "", "timer", cx, |app, window, cx| {
                    app.open_dialog(dialogs::DialogKind::Ttl, window, cx)
                })
                .content_style(
                    ttl_content.style().clone(),
                    gpui_component::Size::Size(px(18.)),
                )
                .accessibility_label(ttl_label.clone())
                .child(
                    div()
                        .id("ttl-text")
                        .test_support()
                        .child(ScriptLabel { text: ttl_label }),
                )
                .w(px(ttl_width))
                .flex_shrink_0()
                .px(px(11.))
                .gap(px(6.))
                .text_size(px(14.))
                .line_height(px(14.))
                .font_features(numbers)
                .rounded_l(px(4.))
                .rounded_r(px(0.))
                .tooltip("TTL")
                .tooltip_show_delay(Duration::from_millis(100))
                .map(|button| self.key_button_style(button, cx)),
            )
            .child(
                self.key_toolbar_button(
                    "rename-key",
                    "edit",
                    self.tr(if binary {
                        "dialogue.rename_binary_key_fail"
                    } else {
                        "interface.rename_key"
                    }),
                    cx,
                    |app, window, cx| app.open_dialog(dialogs::DialogKind::Rename, window, cx),
                )
                .disabled(binary)
                .when(binary, |button| button.opacity(0.5))
                .border_l_0()
                .pl(px(1.))
                .rounded_l(px(0.)),
            );
        div()
            .pt(px(5.))
            .flex_shrink_0()
            .child(
                div()
                    .id("key-toolbar")
                    .test_support()
                    .h(px(32.))
                    .w_full()
                    .px(px(5.))
                    .flex()
                    .gap(px(5.))
                    .items_center()
                    .child(group)
                    .child(ttl_group)
                    .child(
                        self.key_toolbar_button(
                            "delete-key",
                            "delete",
                            self.tr("interface.delete_key"),
                            cx,
                            |app, window, cx| {
                                if let Some(key) =
                                    app.index().and_then(|i| app.tabs[i].current_key.clone())
                                {
                                    app.open_dialog(
                                        dialogs::DialogKind::DeleteKey(key),
                                        window,
                                        cx,
                                    );
                                }
                            },
                        )
                        .tooltip_show_delay(Duration::from_millis(100)),
                    ),
            )
            .into_any_element()
    }
}
