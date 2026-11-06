use super::*;
use gpui::prelude::FluentBuilder as _;
use gpui_base::TestSupportExt as _;
use gpui_component::spinner::Spinner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MessageKind {
    Success,
    Warning,
    Info,
    Error,
    Loading,
}
pub(super) struct MessageNotice {
    pub id: u64,
    pub text: String,
    pub kind: MessageKind,
    pub closable: bool,
    expires: Option<Instant>,
}
pub(super) struct ErrorNotification {
    pub id: u64,
    pub text: String,
}

pub(super) fn shadows(dark: bool) -> Vec<BoxShadow> {
    let values = if dark {
        [
            (3., 6., -4., 0.24),
            (6., 12., 0., 0.16),
            (9., 18., 8., 0.10),
        ]
    } else {
        [
            (3., 6., -4., 0.12),
            (6., 16., 0., 0.08),
            (9., 28., 8., 0.05),
        ]
    };
    values
        .into_iter()
        .map(|(y, blur, spread, alpha)| {
            // CSS uses half the radius as Gaussian sigma. WebKit narrows its
            // CSS shadow kernel by 0.88 before the three box blur passes.
            // https://www.w3.org/TR/css-backgrounds-3/#shadow-blur
            // WebKit/Source/WebCore/platform/graphics/ShadowBlur.cpp
            let sigma = blur * 0.5;
            let sigma = if cfg!(target_os = "macos") {
                sigma * 0.88
            } else {
                sigma
            };
            BoxShadow::new(px(0.), px(y), Hsla::black().opacity(alpha))
                .blur_radius(px(sigma))
                .spread_radius(px(spread))
        })
        .collect()
}

impl RdmApp {
    #[cfg(feature = "ui-qa")]
    pub(super) fn qa_show_notice(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let oracle: Json =
            serde_json::from_str(include_str!("../../tests/fixtures/notice-source.json")).unwrap();
        let record = oracle["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["name"] == name)
            .unwrap();
        self.pref.general.theme = if record["dark"].as_bool().unwrap() {
            "dark"
        } else {
            "light"
        }
        .into();
        self.apply_theme(window, cx);
        if record["modal"].as_bool().unwrap() {
            self.pref.general.allow_track = true;
            self.open_dialog(dialogs::DialogKind::Preferences, window, cx);
        }
        match record["type"].as_str().unwrap() {
            "notification" => self.error_notification(record["text"].as_str().unwrap().into(), cx),
            "stack" => {
                self.toast("第一条", false);
                self.toast("第二条", true);
            }
            kind => self.toast(record["text"].as_str().unwrap(), kind == "error"),
        }
        cx.notify();
    }
    pub(super) fn add_message(&mut self, text: String, kind: MessageKind) -> u64 {
        self.next_notice_id += 1;
        let id = self.next_notice_id;
        self.messages.push(MessageNotice {
            id,
            text,
            kind,
            closable: false,
            expires: (!matches!(kind, MessageKind::Loading))
                .then(|| Instant::now() + Duration::from_secs(3)),
        });
        id
    }
    pub(super) fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.add_message(
            text.into(),
            if error {
                MessageKind::Error
            } else {
                MessageKind::Success
            },
        );
    }
    pub(super) fn loading_message(&mut self, text: String) -> u64 {
        self.add_message(text, MessageKind::Loading)
    }
    pub(super) fn csv_loading_message(&mut self) -> u64 {
        let id = self.loading_message(String::new());
        self.messages.last_mut().unwrap().closable = true;
        id
    }
    pub(super) fn close_message(&mut self, id: u64, cx: &App) {
        self.cancel_csv_transfer(id);
        self.destroy_message(id, cx);
    }
    pub(super) fn destroy_message(&mut self, id: u64, cx: &App) {
        if let Some(index) = self.messages.iter().position(|notice| notice.id == id) {
            let notice = self.messages.remove(index);
            self.exiting_messages.push(motion::Exit::new(notice, cx));
        }
    }
    pub(super) fn dismiss_notification(&mut self, id: u64, cx: &App) {
        if let Some(index) = self.notifications.iter().position(|notice| notice.id == id) {
            let notice = self.notifications.remove(index);
            self.exiting_notifications.push(motion::Exit::new(notice, cx));
        }
    }
    pub(super) fn error_notification(&mut self, text: String, cx: &App) {
        self.next_notice_id += 1;
        if self.notifications.len() == 5 {
            self.dismiss_notification(self.notifications[0].id, cx);
        }
        self.notifications.push(ErrorNotification {
            id: self.next_notice_id,
            text,
        });
    }
    pub(super) fn expire_messages(&mut self, cx: &App) -> bool {
        let now = Instant::now();
        let expired: Vec<_> = self.messages.iter()
            .filter(|notice| notice.expires.is_some_and(|expires| expires <= now))
            .map(|notice| notice.id).collect();
        for id in &expired { self.destroy_message(*id, cx); }
        !expired.is_empty()
    }
    fn hover_message(&mut self, id: u64, hovered: bool) {
        if let Some(notice) = self.messages.iter_mut().find(|notice| notice.id == id) {
            if !matches!(notice.kind, MessageKind::Loading) {
                notice.expires = (!hovered).then(|| Instant::now() + Duration::from_secs(3));
            }
        }
    }
    pub(super) fn render_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.dark(cx);
        let border = if !cfg!(target_os = "windows") && !window.is_fullscreen() {
            1.
        } else {
            0.
        };
        let mut messages = div()
            .absolute()
            .bottom(px(50. - border))
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center();
        let mut visible_messages: Vec<_> = self.messages.iter().map(|notice| (notice, true))
            .chain(self.exiting_messages.iter().map(|exit| (&exit.value, false))).collect();
        visible_messages.sort_by_key(|(notice, _)| notice.id);
        let mut finished_messages = Vec::new();
        for (notice, present) in visible_messages {
            let id = notice.id;
            let sample = motion::presence(ElementId::from(("message-enter", id)), present, window, cx);
            if !sample.should_render() { finished_messages.push(id); continue; }
            let progress = sample.progress;
            let (name, color) = match notice.kind {
                MessageKind::Success => ("notice-success", if dark { 0x63e2b7 } else { 0x18a058 }),
                MessageKind::Warning => ("notice-warning", if dark { 0xf2c97d } else { 0xf0a020 }),
                MessageKind::Info => ("notice-info", if dark { 0x70c0e8 } else { 0x2080f0 }),
                MessageKind::Error => ("notice-error", if dark { 0xe88080 } else { 0xd03050 }),
                MessageKind::Loading => ("notice-success", 0xd33a31),
            };
            let graphic = if matches!(notice.kind, MessageKind::Loading) {
                Spinner::new()
                    .with_size(gpui_component::Size::Size(px(20.)))
                    .color(rgb(color).into())
                    .into_any_element()
            } else {
                icon(name)
                    .size(px(20.))
                    .text_color(rgb(color))
                    .into_any_element()
            };
            let card = div()
                    .id(("message", id))
                    .test_support()
                    .relative()
                    .top(px(6. * (1. - progress)))
                    .opacity(progress)
                    .occlude()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(20.))
                    .py(px(10.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(if dark {
                        Hsla::from(rgba(0xffffffd1))
                    } else {
                        Hsla::from(rgb(0x333639))
                    })
                    .bg(rgb(if dark { 0x48484e } else { 0xffffff }))
                    .rounded(px(4.))
                    .shadow(shadows(dark))
                    .on_hover(cx.listener(move |app, hovered, _, cx| {
                        app.hover_message(id, *hovered);
                        cx.notify();
                    }))
                    .child(graphic)
                    .child(if notice.text.is_empty() {
                        div().size_0().into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .children(notice.text.lines().map(|line| text::ScriptLabel {
                                text: line.to_owned(),
                            }))
                            .into_any_element()
                    })
                    .when(notice.closable, |message| {
                        message.child(
                            div()
                                .id(("message-close", id))
                                .test_support()
                                .size(px(16.))
                                .flex_shrink_0()
                                .cursor_pointer()
                                .text_color(if dark {
                                    Hsla::white().opacity(0.52)
                                } else {
                                    rgb(0x666666).into()
                                })
                                .hover(|close| {
                                    close
                                        .bg(if dark {
                                            rgba(0xffffff1a)
                                        } else {
                                            rgba(0x0000000d)
                                        })
                                        .rounded(px(3.))
                                })
                                .child(icon("notice-close").size_full())
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.close_message(id, cx);
                                    cx.notify();
                                })),
                        )
                    })
                .when(!present, |card| card.child(motion::shield()))
                .into_any_element();
            messages = messages.child(notice_row(("message-reveal", id).into(), if present { 1. } else { progress }, 8., card));
        }
        let mut notifications = div()
            .absolute()
            .bottom(px(50. - border))
            .right(px(16. - border))
            .w(px(365.))
            .flex()
            .flex_col();
        let mut visible_notifications: Vec<_> = self.notifications.iter().map(|notice| (notice, true))
            .chain(self.exiting_notifications.iter().map(|exit| (&exit.value, false))).collect();
        visible_notifications.sort_by_key(|(notice, _)| notice.id);
        let mut finished_notifications = Vec::new();
        for (notice, present) in visible_notifications {
            let id = notice.id;
            let sample = motion::presence(ElementId::from(("notification-enter", id)), present, window, cx);
            if !sample.should_render() { finished_notifications.push(id); continue; }
            let progress = sample.progress;
            let card = div()
                    .id(("error-notification", id))
                    .test_support()
                    .relative()
                    .left(px(8. * (1. - progress)))
                    .opacity(progress)
                    .occlude()
                    .w_full()
                    .px(px(16.))
                    .py(px(16.))
                    .bg(rgb(if dark { 0x48484e } else { 0xffffff }))
                    .text_color(if dark {
                        Hsla::white().opacity(0.82)
                    } else {
                        rgb(0x333639).into()
                    })
                    .rounded(px(4.))
                    .shadow(shadows(dark))
                    .child(div().absolute().top(px(16.)).left(px(16.)).child(
                        icon("notice-error").size(px(28.)).text_color(rgb(if dark {
                            0xe88080
                        } else {
                            0xd03050
                        })),
                    ))
                    .child(
                        div()
                            .ml(px(40.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .pr(px(20.))
                                    .text_size(px(16.))
                                    .line_height(px(24.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if dark {
                                        Hsla::white().opacity(0.9)
                                    } else {
                                        rgb(0x1f2225).into()
                                    })
                                    .child(self.tr("common.error")),
                            )
                            .child(
                                div()
                                    .mt(px(12.))
                                    .text_size(px(14.))
                                    .line_height(px(21.))
                                    .font_weight(FontWeight::NORMAL)
                                    .child(notice.text.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(12.))
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(if dark {
                                        Hsla::white().opacity(0.52)
                                    } else {
                                        rgb(0x767c82).into()
                                    })
                                    .child("Please see console output for more detail"),
                            ),
                    )
                    .child(
                        div().absolute().top(px(16.)).right(px(16.)).child(
                            self.icon_btn(
                                format!("close-notification-{id}"),
                                "close",
                                "",
                                cx,
                                move |app, _, cx| {
                                    app.dismiss_notification(id, cx);
                                    cx.notify();
                                },
                            ).tab_stop(present)
                            .icon(icon("notice-close").size(px(16.)))
                            .with_size(gpui_component::Size::Size(px(20.)))
                            .size(px(20.))
                            .p_0()
                            .text_color(if dark {
                                Hsla::white().opacity(0.52)
                            } else {
                                rgb(0x666666).into()
                            }),
                        ),
                    )
                .when(!present, |card| card.child(motion::shield()))
                .into_any_element();
            notifications = notifications.child(notice_row(("notification-reveal", id).into(), if present { 1. } else { progress }, 12., card));
        }
        self.exiting_messages.retain_mut(|exit| {
            exit.keep_visible(cx);
            !finished_messages.contains(&exit.value.id)
        });
        self.exiting_notifications.retain_mut(|exit| {
            exit.keep_visible(cx);
            !finished_notifications.contains(&exit.value.id)
        });
        div()
            .absolute()
            .inset_0()
            .children([
                messages.into_any_element(),
                notifications.into_any_element(),
            ])
            .into_any_element()
    }
}

fn notice_row(id: ElementId, progress: f32, gap: f32, card: AnyElement) -> AnyElement {
    let row = div().w_full().pt(px(gap)).flex().justify_center().child(card).into_any_element();
    gpui_base::motion::MotionReveal::new(id, progress, row).into_any_element()
}
