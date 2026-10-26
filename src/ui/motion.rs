use super::*;
use gpui_base::motion::{Presence, Transition, TransitionId};

pub(super) fn enter(id: impl Into<TransitionId>, window: &mut Window, cx: &mut App) -> f32 {
    Presence::new(id, true)
        .transition(Transition::new(Duration::from_millis(160)))
        .sample(window, cx)
        .progress
}

pub(super) fn overlay(
    id: impl Into<TransitionId>,
    content: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .opacity(enter(id, window, cx))
        .child(content)
        .into_any_element()
}

#[cfg(all(test, feature = "ui-qa"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    struct Probe {
        page: u64,
        progress: f32,
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.progress = enter(ElementId::from(("page", self.page)), window, cx);
            div().size_full().opacity(self.progress)
        }
    }

    #[gpui::test]
    fn entry_continues_across_redraws_and_replays_for_a_new_page(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(100.), px(100.)), |_, _| Probe {
            page: 1,
            progress: 0.,
        });
        cx.run_until_parked();
        let sample = |cx: &mut TestAppContext| {
            window.update(cx, |_, window, _| window.refresh()).unwrap();
            cx.run_until_parked();
            window.update(cx, |probe, _, _| probe.progress).unwrap()
        };
        assert_eq!(sample(cx), 0.);
        cx.executor().advance_clock(Duration::from_millis(80));
        let middle = sample(cx);
        assert!(middle > 0. && middle < 1.);
        assert_eq!(sample(cx), middle, "redraws must not restart an entry");
        cx.executor().advance_clock(Duration::from_millis(80));
        assert_eq!(sample(cx), 1.);
        window.update(cx, |probe, _, _| probe.page = 2).unwrap();
        assert_eq!(sample(cx), 0.);
        cx.executor().advance_clock(Duration::from_millis(160));
        assert_eq!(sample(cx), 1.);
        window.update(cx, |probe, _, _| probe.page = 1).unwrap();
        assert_eq!(sample(cx), 0., "returning to an unmounted page must replay");
        cx.update(|cx| cx.set_reduce_motion(true));
        assert_eq!(
            sample(cx),
            1.,
            "reducing motion mid-entry must settle immediately"
        );
    }

    #[gpui::test]
    fn reduced_motion_is_immediate_and_does_not_schedule_frames(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let window = cx.open_window(size(px(100.), px(100.)), |_, _| Probe {
            page: 1,
            progress: 0.,
        });
        cx.run_until_parked();
        window
            .update(cx, |probe, window, cx| {
                assert_eq!(probe.progress, 1.);
                assert_eq!(window.simulate_next_frame(cx), 0);
            })
            .unwrap();
    }
}
