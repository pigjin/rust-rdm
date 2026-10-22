use super::dialogs::numbers;
use super::text::{ScriptLabel, script_width};
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

impl RefreshTarget {
    fn index(self) -> usize {
        match self {
            Self::Status => 0,
            Self::Detail => 1,
            Self::Slow => 2,
        }
    }
    fn default_seconds(self) -> f64 {
        match self {
            Self::Detail => 2.,
            _ => 5.,
        }
    }
}

impl RdmApp {
    fn refresh_input(&self, i: usize, target: RefreshTarget) -> Entity<InputState> {
        match target {
            RefreshTarget::Status => self.tabs[i].status_interval.clone(),
            RefreshTarget::Detail => self.tabs[i].refresh_interval.clone(),
            RefreshTarget::Slow => self.tabs[i].slow_interval.clone(),
        }
    }

    pub(super) fn refresh_number_event(
        &mut self,
        id: u64,
        target: RefreshTarget,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Focus) {
            cx.notify();
            return;
        }
        if !matches!(
            event,
            InputEvent::Change | InputEvent::Blur | InputEvent::PressEnter { .. }
        ) {
            return;
        }
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let input = self.refresh_input(i, target);
        let raw = text_input(&input, cx);
        if matches!(event, InputEvent::Change) && numbers::unfinished(&raw) {
            return;
        }
        let value = match numbers::parse(&raw) {
            Ok(Some(value))
                if matches!(event, InputEvent::Change) && !(1. ..=9999.).contains(&value) =>
            {
                return;
            }
            Ok(Some(value)) => Some(value.clamp(1., 9999.)),
            Ok(None) => None,
            Err(()) if matches!(event, InputEvent::Blur) => {
                self.tabs[i].refresh_numbers[target.index()]
            }
            Err(()) => return,
        };
        self.tabs[i].refresh_numbers[target.index()] = value;
        let formatted = numbers::display(value);
        if raw != formatted {
            set_input(&input, formatted, window, cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_control(
        &self,
        id: u64,
        target: RefreshTarget,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(i) = self.tab_index(id) else {
            return div().into_any_element();
        };
        let t = &self.tabs[i];
        let (name, on) = match target {
            RefreshTarget::Status => ("status-refresh", t.status_auto),
            RefreshTarget::Detail => ("reload-key", t.auto_refresh),
            RefreshTarget::Slow => ("slow-refresh", t.slow_auto),
        };
        let handler = move |a: &mut Self, w: &mut Window, cx: &mut Context<Self>| match target {
            RefreshTarget::Status => a.refresh_info(id, w, cx),
            RefreshTarget::Detail => a.reload_detail(id, true, w, cx),
            RefreshTarget::Slow => a.load_slow(id, w, cx),
        };
        let trigger = if matches!(target, RefreshTarget::Detail) {
            let mut content = div().text_size(px(14.)).line_height(px(14.));
            let mut hover = div()
                .bg(Hsla::transparent_black())
                .text_color(cx.theme().primary_hover);
            Button::new(name)
                .ghost()
                .icon(icon("refresh").size(px(18.)))
                .w(px(18.))
                .h(px(18.))
                .p_0()
                .allow_icon_overflow()
                .content_style(content.style().clone(), gpui_component::Size::Size(px(18.)))
                .on_click(cx.listener(move |a, _, w, cx| handler(a, w, cx)))
                .on_mouse_down(MouseButton::Left, |event, _, cx| {
                    if event.click_count > 1 {
                        cx.stop_propagation();
                    }
                })
                .hover_style(hover.style().clone())
        } else if matches!(target, RefreshTarget::Status | RefreshTarget::Slow) {
            let mut content=div().text_size(px(14.)).line_height(px(14.));
            Button::new(name)
                .ghost()
                .icon(icon("refresh").size(px(18.)))
                .size(px(28.))
                .allow_icon_overflow()
                .content_style(content.style().clone(),gpui_component::Size::Size(px(18.)))
                .rounded(px(28.))
                .p_0()
                .bg(if self.dark(cx) { rgba(0xffffff14) } else { rgba(0x2e33380d) })
                .on_click(cx.listener(move |a, _, w, cx| handler(a, w, cx)))
        } else {
            self.icon_btn(name, "refresh", self.tr("interface.reload"), cx, handler)
        };
        let trigger = trigger.text_color(if on {
            cx.theme().primary
        } else if matches!(target, RefreshTarget::Detail) && self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            cx.theme().foreground
        });
        let weak = cx.weak_entity();
        let surface: Hsla = if self.dark(cx) {
            rgb(0x2c2c32).into()
        } else {
            rgb(0xffffff).into()
        };
        gpui_component::hover_card::HoverCard::new(name)
            .anchor(if matches!(target, RefreshTarget::Status) {
                Anchor::TopRight
            } else {
                Anchor::TopCenter
            })
            .appearance(false)
            .open_delay(Duration::from_millis(
                if matches!(target, RefreshTarget::Status) {
                    100
                } else {
                    500
                },
            ))
            .close_delay(Duration::from_millis(100))
            .top(px(10.))
            .px(px(14.))
            .py(px(8.))
            .rounded(px(3.))
            .bg(surface)
            .map(|mut card| {
                card.style().overflow.x = Some(Overflow::Visible);
                card.style().overflow.y = Some(Overflow::Visible);
                card
            })
            .shadow(vec![
                BoxShadow::new(
                    px(0.),
                    px(3.),
                    rgba(if self.dark(cx) {
                        0x0000003d
                    } else {
                        0x0000001f
                    })
                    .into(),
                )
                .blur_radius(px(6.))
                .spread_radius(px(-4.)),
                BoxShadow::new(
                    px(0.),
                    px(6.),
                    rgba(if self.dark(cx) {
                        0x00000029
                    } else {
                        0x00000014
                    })
                    .into(),
                )
                .blur_radius(px(if self.dark(cx) { 12. } else { 16. })),
                BoxShadow::new(
                    px(0.),
                    px(9.),
                    rgba(if self.dark(cx) {
                        0x0000001a
                    } else {
                        0x0000000d
                    })
                    .into(),
                )
                .blur_radius(px(if self.dark(cx) { 18. } else { 28. }))
                .spread_radius(px(8.)),
            ])
            .trigger(trigger)
            .content(move |_, window, cx| {
                weak.update(cx, |a, cx| a.render_refresh_form(id, target, window, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element()
    }

    fn render_refresh_form(
        &self,
        id: u64,
        target: RefreshTarget,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(i) = self.tab_index(id) else {
            return div().into_any_element();
        };
        let t = &self.tabs[i];
        let on = match target {
            RefreshTarget::Status => t.status_auto,
            RefreshTarget::Detail => t.auto_refresh,
            RefreshTarget::Slow => t.slow_auto,
        };
        let interval = self.refresh_input(i, target);
        let focused = !on && interval.focus_handle(cx).is_focused(window);
        let labels = [
            self.tr("interface.auto_refresh"),
            self.tr("interface.refresh_interval"),
        ];
        // The original teleported form uses Naive UI's default font stack.
        let mut font = gpui::font(".SystemUIFont");
        // CSS weight 450 selects the medium system face on macOS.
        font.weight = FontWeight::MEDIUM;
        let label_width = if cfg!(target_os = "macos") && labels == ["自动刷新", "刷新间隔"]
        {
            // Measured Naive UI label column. CoreText's UI fallback and
            // Chromium's optical face have slightly different advances.
            65.6875
        } else {
            labels
                .iter()
                .map(|text| script_width(text, font.clone(), 14., window))
                .fold(0., f32::max)
                + 12.
        };
        let text: Hsla = if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let surface: Hsla = if self.dark(cx) {
            rgb(0x2c2c32).into()
        } else {
            rgb(0xffffff).into()
        };
        let weak = cx.weak_entity();
        let switch = gpui_base::Switch::new("refresh-switch")
            .checked(on)
            .accessibility_label(labels[0].clone())
            .on_change(move |on, _, window, cx| {
                let _ = weak.update(cx, |a, cx| a.toggle_refresh(id, target, on, window, cx));
            })
            .w(px(32.))
            .h(px(18.))
            .rounded_full()
            .relative()
            .cursor_pointer()
            .focus(|style| {
                style.shadow(vec![if self.dark(cx) {
                    BoxShadow::new(px(0.), px(0.), rgba(0xd33a314d).into()).blur_radius(px(8.))
                } else {
                    BoxShadow::new(px(0.), px(0.), rgba(0xd33a3133).into()).spread_radius(px(2.))
                }])
            })
            .bg(if on {
                if self.dark(cx) {
                    rgb(0xff6b6b).into()
                } else {
                    cx.theme().primary
                }
            } else if self.dark(cx) {
                rgba(0xffffff33).into()
            } else {
                rgba(0x00000024).into()
            })
            .child(
                div()
                    .absolute()
                    .top(px(2.))
                    .left(px(if on { 16. } else { 2. }))
                    .size(px(14.))
                    .rounded_full()
                    .bg(rgb(0xffffff))
                    .shadow(if self.dark(cx) {
                        vec![
                            BoxShadow::new(px(0.), px(2.), rgba(0x00000066).into())
                                .blur_radius(px(4.)),
                        ]
                    } else {
                        vec![
                            BoxShadow::new(px(0.), px(1.), rgba(0x0000004d).into())
                                .blur_radius(px(4.)),
                            BoxShadow::new(px(0.), px(0.), rgba(0x0000000d).into())
                                .blur_radius(px(1.))
                                .inset(),
                        ]
                    }),
            );
        div()
            .id("refresh-form")
            .test_support()
            .relative()
            .w(px(label_width + 100.))
            .h(px(56.))
            .flex()
            .flex_col()
            .text_size(px(14.))
            .font(font.clone())
            .font_weight(FontWeight::NORMAL)
            .line_height(px(21.))
            .text_color(text)
            .child(
                icon("refresh-arrow")
                    .absolute()
                    .top(px(-15.))
                    .left(px(if matches!(target, RefreshTarget::Status) {
                        label_width + 100. - 7.
                    } else {
                        (label_width + 100.) / 2. - 7.
                    }))
                    .w(px(14.))
                    .h(px(7.))
                    .text_color(surface),
            )
            .child(
                div()
                    .id("refresh-auto-row")
                    .test_support()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id("refresh-auto-label")
                            .test_support()
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(17.5))
                            .text_color(rgb(0x717880))
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .w(px(label_width))
                            .min_w_0()
                            .flex_shrink_0()
                            .pr(px(12.))
                            .child(ScriptLabel {
                                text: labels[0].clone(),
                            }),
                    )
                    .child(
                        div()
                            .id("refresh-switch-frame")
                            .test_support()
                            .h(px(18.))
                            .child(switch),
                    ),
            )
            .child(
                div()
                    .id("refresh-interval-row")
                    .test_support()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id("refresh-interval-label")
                            .test_support()
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(17.5))
                            .text_color(rgb(0x717880))
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .w(px(label_width))
                            .min_w_0()
                            .flex_shrink_0()
                            .pr(px(12.))
                            .child(ScriptLabel {
                                text: labels[1].clone(),
                            }),
                    )
                    .child(
                        Styled::h(Input::new(&interval).small(), px(28.))
                            .id("refresh-interval-input")
                            .accessibility_id("refresh-interval-input")
                            .w(px(100.))
                            .px(px(9.))
                            .py_0()
                            .text_size(px(14.))
                            .line_height(px(28.))
                            .disabled(on)
                            .focus_bordered(false)
                            .bg(if self.dark(cx) {
                                rgba(if on { 0xffffff0f } else { 0xffffff1a })
                            } else {
                                rgb(if on { 0xfafafc } else { 0xffffff })
                            })
                            .border_color(if focused {
                                cx.theme().primary_hover
                            } else if self.dark(cx) {
                                Hsla::transparent_black()
                            } else {
                                rgb(0xe0e0e6).into()
                            })
                            // The shared input halves disabled text opacity.
                            .text_color(if on {
                                if self.dark(cx) {
                                    rgba(0xffffffc2).into()
                                } else {
                                    rgb(0x8a8a88).into()
                                }
                            } else {
                                text
                            })
                            .when(focused, |field| {
                                field.shadow(vec![
                                    BoxShadow::new(px(0.), px(0.), rgba(0xd33a3133).into())
                                        .spread_radius(px(2.)),
                                ])
                            })
                            .suffix(
                                div()
                                    .id("refresh-unit")
                                    .test_support()
                                    .relative()
                                    .left(px(1.))
                                    .child(ScriptLabel {
                                        text: self.tr("common.unit_second"),
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn toggle_refresh(
        &mut self,
        id: u64,
        target: RefreshTarget,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let mut seconds = self.tabs[i].refresh_numbers[target.index()]
            .unwrap_or(target.default_seconds())
            .max(1.);
        if on {
            self.refresh_number_event(id, target, &InputEvent::Blur, window, cx);
            seconds = self.tabs[i].refresh_numbers[target.index()]
                .unwrap_or(target.default_seconds())
                .max(1.);
            self.tabs[i].refresh_numbers[target.index()] = Some(seconds);
            set_input(
                &self.refresh_input(i, target),
                numbers::display(Some(seconds)),
                window,
                cx,
            );
        }
        match target {
            RefreshTarget::Status => {
                self.tabs[i].status_auto = on;
                self.tabs[i].status_seconds = seconds;
                self.last_poll = Instant::now();
                if on {
                    self.tabs[i].info_details = false;
                }
                self.save_status_interval(id, window, cx);
            }
            RefreshTarget::Detail => {
                self.tabs[i].auto_refresh = on;
                self.tabs[i].refresh_seconds = seconds;
                self.last_auto = Instant::now();
            }
            RefreshTarget::Slow => {
                self.tabs[i].slow_auto = on;
                self.tabs[i].slow_seconds = seconds;
                self.tabs[i].slow_polled = Instant::now();
            }
        }
        cx.notify();
    }
}
