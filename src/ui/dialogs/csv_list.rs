//! Scrollbar geometry and dragging used by the original Naive UI CSV list.
use super::*;

#[derive(Default)]
pub(super) struct ScrollbarState {
    pub(super) hovered: bool,
    drag: Option<(f32, f32)>, // initial pointer Y and scroll top
}

impl RdmApp {
    pub(super) fn csv_list_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.dialog
            && dialog.csv_scrollbar.hovered != hovered
        {
            dialog.csv_scrollbar.hovered = hovered;
            cx.notify();
        }
    }

    pub(super) fn render_csv_scrollbar(
        &self,
        height: f32,
        count: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dialog = self.dialog.as_ref().unwrap();
        let content = count as f32 * 25.;
        let rail = (height - 4.).max(0.);
        let thumb = if content > 0. {
            (rail * height / content + 12.).min(height)
        } else {
            height
        };
        let limit = (content - height).max(0.);
        let top = (-dialog.csv_key_scroll.offset().y.as_f32()).clamp(0., limit);
        let thumb_top = if limit > 0. {
            top / limit * (rail - thumb)
        } else {
            0.
        };
        let visible =
            limit > 0. && (dialog.csv_scrollbar.hovered || dialog.csv_scrollbar.drag.is_some());
        let normal = if self.dark(cx) {
            rgba(0xffffff33)
        } else {
            rgba(0x00000040)
        };
        let hover = if self.dark(cx) {
            rgba(0xffffff4d)
        } else {
            rgba(0x00000066)
        };
        let weak = cx.weak_entity();
        let move_view = weak.clone();
        let up_view = weak.clone();
        div()
            .absolute()
            .inset_0()
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let weak = move_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.bubble() || !event.dragging() {
                                return;
                            }
                            let Some(view) = weak.upgrade() else {
                                return;
                            };
                            if !view
                                .read(cx)
                                .dialog
                                .as_ref()
                                .is_some_and(|d| d.csv_scrollbar.drag.is_some())
                            {
                                return;
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                            view.update(cx, |app, cx| {
                                app.csv_scrollbar_drag(
                                    event.position.y.as_f32(),
                                    height,
                                    content,
                                    thumb,
                                    false,
                                    cx,
                                )
                            });
                        });
                        let weak = up_view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if !phase.bubble() || event.button != MouseButton::Left {
                                return;
                            }
                            let Some(view) = weak.upgrade() else {
                                return;
                            };
                            if !view
                                .read(cx)
                                .dialog
                                .as_ref()
                                .is_some_and(|d| d.csv_scrollbar.drag.is_some())
                            {
                                return;
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                            view.update(cx, |app, cx| {
                                app.csv_scrollbar_drag(
                                    event.position.y.as_f32(),
                                    height,
                                    content,
                                    thumb,
                                    true,
                                    cx,
                                )
                            });
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .when(visible, |frame| {
                frame.child(
                    div()
                        .id("csv-key-scroll-thumb")
                        .test_support()
                        .absolute()
                        .right(px(4.))
                        .top(px(2. + thumb_top))
                        .w(px(8.))
                        .h(px(thumb))
                        .rounded(px(4.))
                        .bg(if dialog.csv_scrollbar.drag.is_some() {
                            hover
                        } else {
                            normal
                        })
                        .hover(move |style| style.bg(hover))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, event: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                if let Some(dialog) = &mut app.dialog {
                                    dialog.csv_scrollbar.drag = Some((
                                        event.position.y.as_f32(),
                                        -dialog.csv_key_scroll.offset().y.as_f32(),
                                    ));
                                    cx.notify();
                                }
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    fn csv_scrollbar_drag(
        &mut self,
        y: f32,
        height: f32,
        content: f32,
        thumb: f32,
        release: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let Some((start_y, start_top)) = dialog.csv_scrollbar.drag else {
            return;
        };
        // Naive UI uses the container height here, rather than the shorter rail.
        let limit = (content - height).max(0.);
        let top = (start_top + (y - start_y) * limit / (height - thumb).max(1.)).clamp(0., limit);
        dialog.csv_key_scroll.set_offset(point(px(0.), px(-top)));
        if release {
            dialog.csv_scrollbar.drag = None;
        }
        cx.notify();
    }
}
