use super::*;
use gpui_base::TestSupportExt as _;
use gpui_base::motion::{Easing, Interpolate, Transition, transition};

#[derive(Clone, Copy, PartialEq)]
struct DividerColor([f32; 4]);

impl From<Rgba> for DividerColor {
    fn from(c: Rgba) -> Self {
        Self([c.r * c.a, c.g * c.a, c.b * c.a, c.a])
    }
}

impl DividerColor {
    fn color(self) -> Rgba {
        let [r, g, b, a] = self.0;
        if a == 0. {
            Rgba::default()
        } else {
            Rgba {
                r: r / a,
                g: g / a,
                b: b / a,
                a,
            }
        }
    }
}

impl Interpolate for DividerColor {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        Self(std::array::from_fn(|i| {
            self.0[i] + (target.0[i] - self.0[i]) * progress
        }))
    }
}

impl RdmApp {
    pub(super) fn render_sidebar(
        &self,
        sidebar: AnyElement,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = if self.sidebar_resizing {
            cx.theme().primary.to_rgb()
        } else if self.sidebar_hover {
            cx.theme().border.to_rgb()
        } else {
            Rgba::default()
        };
        let color = transition(
            "sidebar-divider-color",
            DividerColor::from(target),
            Transition::new(Duration::from_millis(300)).easing(Easing::EaseIn),
            window,
            cx,
        )
        .color();
        let mut divider = div()
            .id("sidebar-divider")
            .test_support()
            .absolute()
            .top_0()
            .left(px(self.pref.behavior.aside_width - 2.))
            .w(px(4.))
            .h_full()
            .bg(color)
            .cursor(CursorStyle::ResizeColumn)
            .on_hover(cx.listener(|app, hovered: &bool, _, cx| {
                if app.sidebar_hover != *hovered {
                    app.sidebar_hover = *hovered;
                    cx.notify();
                }
            }));
        // ResizeableWrapper starts on any mousedown and ends on any mouseup.
        for button in MouseButton::all() {
            divider = divider.on_mouse_down(
                button,
                cx.listener(|app, _, _, cx| {
                    app.sidebar_resizing = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        }
        div()
            .id("rdm-panels")
            .relative()
            .flex()
            .size_full()
            .child(
                div()
                    .id("app-sidebar")
                    .test_support()
                    .relative()
                    .w(px(self.pref.behavior.aside_width))
                    .h_full()
                    .flex_shrink_0()
                    .child(sidebar),
            )
            .child(div().flex_1().min_w_0().h_full().child(main))
            // Source z-index:1 places the whole band over the adjacent pane.
            .child(divider)
            .into_any_element()
    }

    pub(super) fn sidebar_events(&self, cx: &Context<Self>) -> AnyElement {
        let moving = cx.weak_entity();
        let releasing = moving.clone();
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                // Document-level listeners continue outside the divider/window box.
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase.bubble() {
                        let _ = moving.update(cx, |app, cx| {
                            if app.sidebar_resizing {
                                app.move_sidebar(event.position.x.as_f32(), window, cx);
                            }
                        });
                    }
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase.bubble() {
                        let _ = releasing.update(cx, |app, cx| {
                            if app.sidebar_resizing {
                                app.sidebar_resizing = false;
                                cx.notify();
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    fn move_sidebar(&mut self, client_x: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.pref.behavior.aside_width = (client_x - 50.).max(300.);
        self.sidebar_save_epoch = self.sidebar_save_epoch.wrapping_add(1);
        let epoch = self.sidebar_save_epoch;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update_in(cx, |app, _, _| {
                if app.sidebar_save_epoch == epoch {
                    app.save_preferences();
                }
            });
        })
        .detach();
        cx.notify();
    }
}
