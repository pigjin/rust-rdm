use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
pub(super) enum MatchKind {
    Key,
    Entry,
}

#[derive(Clone, Copy)]
pub(super) struct MatchTarget {
    pub tab_id: u64,
    pub kind: MatchKind,
    // The CSS follower rounds unsnapped target geometry before positioning.
    pub left: f32,
    pub width: f32,
}

impl MatchTarget {
    fn index(self) -> usize {
        match self.kind {
            MatchKind::Key => 0,
            MatchKind::Entry => 1,
        }
    }
    pub(super) fn tooltip_id(self) -> &'static str {
        match self.kind {
            MatchKind::Key => "key-match-tooltip",
            MatchKind::Entry => "entry-match-tooltip",
        }
    }
}

#[derive(Default)]
struct HoverTooltip {
    hovered: bool,
    visible: bool,
    epoch: u64,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

#[derive(Default)]
pub(super) struct MatchTooltips([HoverTooltip; 2]);

impl MatchTooltips {
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_state(&self, kind: MatchKind) -> (bool, bool, u64, Bounds<Pixels>) {
        let index = match kind {
            MatchKind::Key => 0,
            MatchKind::Entry => 1,
        };
        let state = &self.0[index];
        (
            state.hovered,
            state.visible,
            state.epoch,
            state.bounds.get(),
        )
    }
    pub(super) fn cancel(&mut self, kind: MatchKind) {
        let index = match kind {
            MatchKind::Key => 0,
            MatchKind::Entry => 1,
        };
        let state = &mut self.0[index];
        state.hovered = false;
        state.visible = false;
        state.epoch += 1;
    }
}

fn tooltip_font() -> Font {
    #[cfg(target_os = "macos")]
    {
        gpui::font(".SystemUIFont")
    }
    #[cfg(target_os = "windows")]
    {
        gpui::font("Segoe UI")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        gpui::font("sans-serif")
    }
}

impl RdmApp {
    fn match_tooltip_text(&self, target: MatchTarget) -> Option<String> {
        let tab = &self.tabs[self.tab_index(target.tab_id)?];
        let pattern = match target.kind {
            MatchKind::Key => tab.key_match.clone(),
            MatchKind::Entry => tab.row_match.clone(),
        };
        if pattern.is_empty() {
            return None;
        }
        let pattern = match target.kind {
            MatchKind::Key => pattern,
            MatchKind::Entry => format!("*{pattern}*"),
        };
        Some(
            self.tr("interface.full_search_result")
                .replace("{pattern}", &pattern),
        )
    }

    pub(super) fn match_tooltip_hover(
        &mut self,
        target: MatchTarget,
        entered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(target.tab_id) else {
            return;
        };
        let state = &mut self.tabs[i].match_tooltips.0[target.index()];
        state.hovered = entered;
        state.epoch += 1;
        let epoch = state.epoch;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                let Some(i) = app.tab_index(target.tab_id) else {
                    return;
                };
                let active = app.index() == Some(i);
                let has_match = app.match_tooltip_text(target).is_some();
                let state = &mut app.tabs[i].match_tooltips.0[target.index()];
                if state.epoch != epoch {
                    return;
                }
                if !active || !has_match {
                    state.visible = false;
                } else if state.hovered {
                    state.visible = true;
                } else if !state.bounds.get().contains(&window.mouse_position()) {
                    state.visible = false;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_match_tooltip(
        &self,
        target: MatchTarget,
        trigger: AnyElement,
        width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(i) = self.tab_index(target.tab_id) else {
            return trigger;
        };
        let state = &self.tabs[i].match_tooltips.0[target.index()];
        let mut popup = gpui_base::Popup::new(format!("{}-host", target.tooltip_id()), trigger)
            .relative()
            .flex_shrink_0()
            .h(px(22.))
            .aspect_ratio(width / 22.)
            .snap_to_device_pixels()
            .offset(px(10.));
        let Some(text) = self.match_tooltip_text(target).filter(|_| state.visible) else {
            return popup.into_any_element();
        };
        let font = tooltip_font();
        let text_width = super::text::script_width(&text, font.clone(), 14., window);
        let tooltip_width = text_width + 28.;
        // The macOS follower's percentage translation uses its device-pixel
        // width while the tooltip retains its fractional intrinsic width.
        let scale = window.scale_factor();
        let follower_width = (tooltip_width * scale).round() / scale;
        let center = target.left + target.width / 2.;
        let viewport_width = window.viewport_size().width.as_f32();
        let (anchor, anchor_x, arrow_x) = if center - follower_width / 2. < 0. {
            (Anchor::TopLeft, target.left.round(), 10. + 8.46875 / 2.)
        } else if center + follower_width / 2. > viewport_width {
            (
                Anchor::TopRight,
                (target.left + target.width).round(),
                tooltip_width - 10. - 8.46875 / 2.,
            )
        } else {
            (Anchor::TopCenter, center.round(), tooltip_width / 2.)
        };
        popup = popup.anchor(anchor).anchor_x(px(anchor_x));
        let color = rgb(if self.dark(cx) { 0x2c2c32 } else { 0x262626 });
        let foreground: Hsla = if self.dark(cx) {
            rgba(0xffffffd1).into()
        } else {
            Hsla::white()
        };
        let bounds = state.bounds.clone();
        let half_diagonal = 8.46875_f32 / std::f32::consts::SQRT_2;
        let arrow = div()
            .id(format!("{}-arrow", target.tooltip_id()))
            .test_support()
            .absolute()
            .left(px(arrow_x - half_diagonal))
            .top(px(-half_diagonal))
            .size(px(half_diagonal * 2.))
            .on_hover(cx.listener(move |app, entered, w, cx| {
                app.match_tooltip_hover(target, *entered, w, cx)
            }))
            .child(
                canvas(
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
                            window.paint_path(path, color);
                        }
                    },
                )
                .size_full(),
            );
        let body = div()
            .id(target.tooltip_id())
            .test_support()
            .role(Role::Tooltip)
            .aria_label(text.clone())
            .relative()
            .occlude()
            .w(px(tooltip_width))
            .h(px(37.))
            .px(px(14.))
            .py(px(8.))
            .rounded(px(4.))
            .bg(color)
            .text_color(foreground)
            .font_family(font.family)
            .font_weight(FontWeight::NORMAL)
            .text_size(px(14.))
            .line_height(px(21.))
            .whitespace_nowrap()
            .shadow(super::notices::shadows(self.dark(cx)))
            .on_hover(cx.listener(move |app, entered, w, cx| {
                app.match_tooltip_hover(target, *entered, w, cx)
            }))
            .child(
                // Pin to the padding edge: an unpositioned absolute child
                // starts at the content origin while retaining the full size.
                canvas(move |value, _, _| bounds.set(value), |_, _, _, _| {})
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full(),
            )
            .child(arrow)
            .child(super::text::ScriptLabel { text });
        popup
            .content(
                div()
                    .relative()
                    .w(px(follower_width))
                    .h(px(37.))
                    // Naive clips the arrow and its shadow to the 10px gap. Paint
                    // the filtered alpha mask behind the opaque tooltip body.
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(-10.))
                            .w(px(tooltip_width))
                            .h(px(10.))
                            .overflow_hidden()
                            .child(
                                svg()
                                    // WebKit's CSS shadow kernel narrows the
                                    // nominal Gaussian sigma by 0.88. Keep the
                                    // standard CSS sigma for other platforms.
                                    .path(if cfg!(target_os = "macos") {
                                        "icons/match-tooltip-arrow-shadow-macos.svg"
                                    } else {
                                        "icons/match-tooltip-arrow-shadow.svg"
                                    })
                                    .absolute()
                                    .left(px(arrow_x - 24.))
                                    .top(px(10. - 24.))
                                    .size(px(48.))
                                    .text_color(Hsla::black()),
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }
}
