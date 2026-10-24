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
            "notification" => self.error_notification(record["text"].as_str().unwrap().into()),
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
    pub(super) fn close_message(&mut self, id: u64) {
        self.cancel_csv_transfer(id);
        self.destroy_message(id);
    }
    pub(super) fn destroy_message(&mut self, id: u64) {
        self.messages.retain(|notice| notice.id != id);
    }
    pub(super) fn error_notification(&mut self, text: String) {
        self.next_notice_id += 1;
        if self.notifications.len() == 5 {
            self.notifications.remove(0);
        }
        self.notifications.push(ErrorNotification {
            id: self.next_notice_id,
            text,
        });
    }
    pub(super) fn expire_messages(&mut self) -> bool {
        let count = self.messages.len();
        let now = Instant::now();
        self.messages
            .retain(|notice| notice.expires.is_none_or(|expires| expires > now));
        self.messages.len() != count
    }
    fn hover_message(&mut self, id: u64, hovered: bool) {
        if let Some(notice) = self.messages.iter_mut().find(|notice| notice.id == id) {
            if !matches!(notice.kind, MessageKind::Loading) {
                notice.expires = (!hovered).then(|| Instant::now() + Duration::from_secs(3));
            }
        }
    }
    pub(super) fn render_notices(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
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
            .items_center()
            .gap(px(8.));
        for notice in &self.messages {
            let id = notice.id;
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
            messages = messages.child(
                div()
                    .id(("message", id))
                    .test_support()
                    .relative()
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
                                    app.close_message(id);
                                    cx.notify();
                                })),
                        )
                    }),
            );
        }
        let mut notifications = div()
            .absolute()
            .bottom(px(50. - border))
            .right(px(16. - border))
            .w(px(365.))
            .flex()
            .flex_col()
            .gap(px(12.));
        for notice in &self.notifications {
            let id = notice.id;
            notifications = notifications.child(
                div()
                    .id(("error-notification", id))
                    .test_support()
                    .relative()
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
                                    app.notifications.retain(|notice| notice.id != id);
                                    cx.notify();
                                },
                            )
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
                    ),
            );
        }
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
