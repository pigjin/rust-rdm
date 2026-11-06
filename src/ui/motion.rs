use super::*;
use gpui_base::motion::{
    Interpolate, Presence, PresenceSample, Transition, TransitionId, transition,
};

pub(super) const EXIT_DURATION: Duration = Duration::from_millis(120);

pub(super) struct Exit<T> {
    pub value: T,
    until: Instant,
}

impl<T> Exit<T> {
    pub fn new(value: T, cx: &App) -> Self {
        Self {
            value,
            until: cx.background_executor().now() + EXIT_DURATION,
        }
    }

    pub fn keep_visible(&mut self, cx: &App) {
        self.until = cx.background_executor().now() + EXIT_DURATION;
    }

    pub fn expired(&self, cx: &App) -> bool {
        cx.reduce_motion() || cx.background_executor().now() >= self.until
    }
}

pub(super) fn presence(
    id: impl Into<TransitionId>,
    present: bool,
    window: &mut Window,
    cx: &mut App,
) -> PresenceSample {
    let tokens = &cx.theme().motion;
    // Keep the curve unchanged on reversal so the sampled position stays continuous.
    let policy = Transition::new(if present {
        tokens.duration_normal
    } else {
        EXIT_DURATION
    })
    .easing(tokens.easing_enter.clone());
    Presence::new(id, present)
        .transition(policy)
        .sample(window, cx)
}

struct PageFade {
    key: String,
    from: f32,
    started: Option<Instant>,
}

pub(super) fn page(key: String, window: &mut Window, cx: &mut App) -> f32 {
    let now = cx.background_executor().now();
    let tokens = cx.theme().motion.clone();
    let state = window.use_keyed_state("page-fade", cx, |_, _| PageFade {
        key: key.clone(),
        from: 1.,
        started: None,
    });
    let opacity = state.update(cx, |state, _| {
        let value = state.started.map_or(1., |started| {
            let t = (now.saturating_duration_since(started).as_secs_f32()
                / tokens.duration_fast.as_secs_f32())
            .min(1.);
            state.from + (1. - state.from) * tokens.easing_enter.sample(t)
        });
        if state.key != key {
            state.key = key;
            state.from = if value >= 1. { 0.9 } else { value };
            state.started = Some(now);
            state.from
        } else {
            value
        }
    });
    if cx.reduce_motion() || opacity >= 1. {
        state.update(cx, |state, _| {
            state.started = None;
            state.from = 1.;
        });
        1.
    } else {
        window.request_animation_frame();
        opacity
    }
}

pub(super) fn move_to<T: Interpolate + PartialEq + 'static>(
    id: impl Into<TransitionId>,
    target: T,
    immediate: bool,
    window: &mut Window,
    cx: &mut App,
) -> T {
    let tokens = &cx.theme().motion;
    let policy = Transition::new(if immediate {
        Duration::ZERO
    } else {
        tokens.duration_normal
    })
    .easing(tokens.easing_move.clone());
    transition(id, target, policy, window, cx)
}

#[derive(Clone, Copy, PartialEq)]
struct Tint([f32; 4]);

impl Interpolate for Tint {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        Self(std::array::from_fn(|i| {
            self.0[i] + (target.0[i] - self.0[i]) * progress
        }))
    }
}

pub(super) fn color(
    id: impl Into<TransitionId>,
    target: Hsla,
    immediate: bool,
    window: &mut Window,
    cx: &mut App,
) -> Hsla {
    let c = target.to_rgb();
    let tokens = &cx.theme().motion;
    let policy = Transition::new(if immediate {
        Duration::ZERO
    } else {
        tokens.duration_fast
    })
    .easing(tokens.easing_enter.clone());
    let Tint([r, g, b, a]) = transition(
        id,
        Tint([c.r * c.a, c.g * c.a, c.b * c.a, c.a]),
        policy,
        window,
        cx,
    );
    if a == 0. {
        Rgba::default().into()
    } else {
        Rgba {
            r: r / a,
            g: g / a,
            b: b / a,
            a,
        }
        .into()
    }
}

pub(super) fn hover(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> (bool, impl Fn(&bool, &mut Window, &mut App) + 'static) {
    let state = window.use_keyed_state(id.into(), cx, |_, _| false);
    let hovered = *state.read(cx);
    (hovered, move |hovered, window, cx| {
        if *state.read(cx) != *hovered {
            state.update(cx, |state, _| *state = *hovered);
            window.refresh();
        }
    })
}

pub(super) fn shield() -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .capture_any_mouse_down(|_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .capture_any_mouse_up(|_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .into_any_element()
}

pub(super) fn inert<E: InteractiveElement + ParentElement>(
    card: E,
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> E {
    let guard = window.use_keyed_state(id.into(), cx, |window, cx| {
        let scope = cx.focus_handle();
        let scope_in = scope.clone();
        let fallback = window.focused(cx);
        let subscription = window.on_focus_in(&scope, cx, move |window, cx| {
            // Retained controls must also be skipped by keyboard navigation.
            let mut visited = Vec::new();
            while scope_in.contains_focused(window, cx) {
                let focused = window.focused(cx);
                if visited.contains(&focused) {
                    if let Some(focus) = &fallback
                        && !visited.contains(&fallback)
                    {
                        focus.focus(window, cx);
                    } else {
                        window.blur(cx);
                    }
                    break;
                }
                visited.push(focused);
                if window.modifiers().shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
            }
        });
        (scope, subscription)
    });
    card.track_focus(&guard.read(cx).0).child(shield())
}

pub(super) fn overlay(content: AnyElement, progress: f32) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .opacity(progress)
        .child(content)
        .into_any_element()
}

impl RdmApp {
    pub(super) fn clean_exits(&mut self, cx: &App) -> bool {
        let mut changed = false;
        macro_rules! clean {
            ($field:ident) => {
                if self.$field.as_ref().is_some_and(|exit| exit.expired(cx)) {
                    self.$field = None;
                    changed = true;
                }
            };
        }
        clean!(exiting_dialog);
        clean!(exiting_ssh);
        clean!(exiting_welcome);
        macro_rules! clean_list {
            ($field:ident) => {
                let count = self.$field.len();
                self.$field.retain(|exit| !exit.expired(cx));
                changed |= count != self.$field.len();
            };
        }
        clean_list!(exiting_warnings);
        clean_list!(exiting_messages);
        clean_list!(exiting_notifications);
        changed
    }
}

#[cfg(all(test, feature = "ui-qa"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    struct Probe {
        key: u64,
        present: bool,
        page: f32,
        progress: f32,
        mounted: bool,
        target: f32,
        position: f32,
        immediate: bool,
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.page = page(self.key.to_string(), window, cx);
            let sample = presence("overlay", self.present, window, cx);
            self.progress = sample.progress;
            self.mounted = sample.should_render();
            self.position = move_to("marker", self.target, self.immediate, window, cx);
            div().size_full().opacity(self.page)
        }
    }

    fn probe(cx: &mut TestAppContext) -> WindowHandle<Probe> {
        cx.update(gpui_component::init);
        let window = cx.open_window(size(px(100.), px(100.)), |_, _| Probe {
            key: 1,
            present: true,
            page: 0.,
            progress: 0.,
            mounted: false,
            target: 0.,
            position: 0.,
            immediate: false,
        });
        cx.run_until_parked();
        window
    }

    fn sample(window: WindowHandle<Probe>, cx: &mut TestAppContext) -> (f32, f32, bool, f32) {
        window.update(cx, |_, window, _| window.refresh()).unwrap();
        cx.run_until_parked();
        window
            .update(cx, |p, _, _| (p.page, p.progress, p.mounted, p.position))
            .unwrap()
    }

    #[gpui::test]
    fn page_starts_visible_and_only_replays_on_identity_change(cx: &mut TestAppContext) {
        let window = probe(cx);
        assert_eq!(sample(window, cx).0, 1.);
        window.update(cx, |p, _, _| p.key = 2).unwrap();
        assert_eq!(sample(window, cx).0, 0.9);
        cx.executor().advance_clock(Duration::from_millis(40));
        let middle = sample(window, cx).0;
        assert!(middle > 0.9 && middle < 1.);
        assert_eq!(sample(window, cx).0, middle);
        window.update(cx, |p, _, _| p.key = 3).unwrap();
        assert_eq!(
            sample(window, cx).0,
            middle,
            "rapid switches must not flash darker"
        );
        cx.executor().advance_clock(EXIT_DURATION);
        assert_eq!(sample(window, cx).0, 1.);
    }

    #[gpui::test]
    fn entrance_and_interrupted_exit_are_continuous(cx: &mut TestAppContext) {
        let window = probe(cx);
        assert_eq!(sample(window, cx).1, 0.);
        cx.executor().advance_clock(Duration::from_millis(45));
        let middle = sample(window, cx).1;
        assert!(middle > 0. && middle < 1.);
        window.update(cx, |p, _, _| p.present = false).unwrap();
        assert_eq!(sample(window, cx).1, middle);
        cx.executor().advance_clock(Duration::from_millis(30));
        let leaving = sample(window, cx).1;
        assert!(leaving > 0. && leaving < middle);
        window.update(cx, |p, _, _| p.present = true).unwrap();
        assert_eq!(sample(window, cx).1, leaving);
        cx.executor().advance_clock(Duration::from_millis(180));
        assert_eq!(sample(window, cx).1, 1.);
        window.update(cx, |p, _, _| p.present = false).unwrap();
        assert!(sample(window, cx).2);
        cx.executor().advance_clock(EXIT_DURATION);
        let final_frame = sample(window, cx);
        assert_eq!(final_frame.1, 0.);
        assert!(!final_frame.2);
        window
            .update(cx, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |_, window, cx| {
                assert_eq!(window.simulate_next_frame(cx), 0)
            })
            .unwrap();
    }

    #[gpui::test]
    fn markers_retarget_from_current_position_and_snap_on_layout_change(cx: &mut TestAppContext) {
        let window = probe(cx);
        window.update(cx, |p, _, _| p.target = 100.).unwrap();
        assert_eq!(sample(window, cx).3, 0.);
        cx.executor().advance_clock(Duration::from_millis(60));
        let position = sample(window, cx).3;
        assert!(position > 0. && position < 100.);
        window.update(cx, |p, _, _| p.target = 200.).unwrap();
        assert_eq!(sample(window, cx).3, position);
        window
            .update(cx, |p, _, _| {
                p.target = 50.;
                p.immediate = true;
            })
            .unwrap();
        assert_eq!(sample(window, cx).3, 50.);
    }

    #[gpui::test]
    fn reduced_motion_settles_in_flight_work_and_stops_frames(cx: &mut TestAppContext) {
        let window = probe(cx);
        window
            .update(cx, |p, _, _| {
                p.key = 2;
                p.target = 100.;
            })
            .unwrap();
        sample(window, cx);
        cx.executor().advance_clock(Duration::from_millis(40));
        assert!(sample(window, cx).1 < 1.);
        cx.update(|cx| cx.set_reduce_motion(true));
        assert_eq!(sample(window, cx), (1., 1., true, 100.));
        window.update(cx, |p, _, _| p.present = false).unwrap();
        assert_eq!(sample(window, cx), (1., 0., false, 100.));
        window
            .update(cx, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |_, window, cx| {
                assert_eq!(window.simulate_next_frame(cx), 0)
            })
            .unwrap();
    }

    struct RevealProbe {
        progress: f32,
    }

    impl Render for RevealProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use gpui_base::TestSupportExt as _;
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(gpui_base::motion::MotionReveal::new(
                    "reveal",
                    self.progress,
                    div().h(px(40.)).w(px(100.)).into_any_element(),
                ))
                .child(
                    div()
                        .id("after-reveal")
                        .test_support()
                        .h(px(10.))
                        .w(px(100.)),
                )
        }
    }

    #[gpui::test]
    fn notice_space_starts_full_and_collapses_without_a_measurement_jump(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(100.), px(100.)), |_, _| RevealProbe {
            progress: 1.,
        });
        for (progress, height) in [(1., 40.), (0.75, 30.), (0.5, 20.), (0., 0.), (1., 40.)] {
            window
                .update(cx, |probe, window, _| {
                    probe.progress = progress;
                    window.refresh();
                })
                .unwrap();
            cx.run_until_parked();
            window
                .update(cx, |_, window, _| {
                    let bounds = gpui_base::test_support::find(window, &[], &"after-reveal".into())
                        .unwrap()
                        .bounds();
                    assert_eq!(bounds.origin.y, px(height));
                })
                .unwrap();
        }
    }

    struct ExitFocusProbe {
        outside: FocusHandle,
        retired: FocusHandle,
    }

    impl Render for ExitFocusProbe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let retired = div().id("retired").size(px(20.)).track_focus(&self.retired);
            let card = inert(
                div().id("exit-card").child(retired),
                "exit-focus",
                window,
                cx,
            )
            .into_any_element();
            div()
                .size_full()
                .child(div().id("outside").size(px(20.)).track_focus(&self.outside))
                .child(overlay(card, 0.5))
        }
    }

    #[gpui::test]
    fn exiting_controls_are_skipped_when_focus_moves_into_them(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(100.), px(100.)), |window, cx| {
            window.activate_window();
            let outside = cx.focus_handle().tab_stop(true);
            outside.focus(window, cx);
            ExitFocusProbe {
                outside,
                retired: cx.focus_handle().tab_stop(true),
            }
        });
        cx.run_until_parked();
        window
            .update(cx, |probe, window, cx| {
                probe.retired.focus(window, cx);
                window.refresh();
            })
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |probe, window, _| {
                assert!(probe.outside.is_focused(window))
            })
            .unwrap();
        window
            .update(cx, |_, window, cx| window.focus_prev(cx))
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |probe, window, _| {
                assert!(probe.outside.is_focused(window))
            })
            .unwrap();
    }

    #[gpui::test]
    fn retained_display_expires_without_waiting_for_a_visible_frame(cx: &mut TestAppContext) {
        let exit = cx.update(|cx| Exit::new(7, cx));
        assert!(!cx.update(|cx| exit.expired(cx)));
        cx.executor().advance_clock(EXIT_DURATION);
        assert!(cx.update(|cx| exit.expired(cx)));
        let exit = cx.update(|cx| Exit::new(8, cx));
        cx.update(|cx| cx.set_reduce_motion(true));
        assert!(cx.update(|cx| exit.expired(cx)));
    }
}
