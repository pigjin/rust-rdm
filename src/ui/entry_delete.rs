use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;
use std::{cell::Cell, rc::Rc};

pub(super) struct EntryDelete {
    key: Vec<u8>,
    row: Entry,
    _token: gpui_base::DeferredPopover,
}

impl RdmApp {
    fn toggle_entry_delete(&mut self, id: u64, row: Entry, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else { return };
        if self.tabs[i]
            .entry_delete
            .as_ref()
            .is_some_and(|pending| pending.row.id == row.id)
        {
            self.tabs[i].entry_delete = None;
        } else if let Some(key) = self.tabs[i].current_key.clone() {
            self.tabs[i].entry_delete = Some(EntryDelete {
                key,
                row,
                _token: gpui_base::GlobalState::register_deferred_popover(cx),
            });
        }
        cx.notify();
    }
    fn confirm_entry_delete(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else { return };
        if let Some(pending) = self.tabs[i].entry_delete.take()
            && self.tabs[i].current_key.as_ref() == Some(&pending.key)
        {
            self.remove_entry(id, pending.row, window, cx);
        }
        cx.notify();
    }
    pub(super) fn render_entry_delete(
        &self,
        id: u64,
        row: &Entry,
        kind: &KeyType,
        index: usize,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let name = match kind {
            KeyType::List => format!(
                "#{}",
                display_bytes(&row.id).parse::<usize>().unwrap_or(index) + 1
            ),
            KeyType::Hash | KeyType::Stream => display_bytes(&row.id),
            _ => display_bytes(&row.value),
        };
        if name.is_empty() {
            return div().into_any_element();
        }
        let row = row.clone();
        let button = self
            .connection_icon_button(
                format!("entry-delete-{index}"),
                "delete",
                self.tr("interface.delete_row"),
                18.,
                cx,
                {
                    let row = row.clone();
                    move |app, _, cx| app.toggle_entry_delete(id, row.clone(), cx)
                },
            )
            .icon(icon("delete").size(px(20.)))
            .allow_icon_overflow();
        let trigger = Rc::new(Cell::new(Bounds::default()));
        let measured = trigger.clone();
        let mut popup = gpui_base::Popup::new(format!("entry-delete-popup-{index}"), button)
            .w(px(18.))
            .h(px(18.))
            .flex_shrink_0()
            .anchor(Anchor::BottomRight)
            .offset(px(10.))
            .on_position(move |_, bounds| measured.set(bounds));
        let tab = &self.tabs[self.tab_index(id).unwrap()];
        if tab.entry_delete.as_ref().is_some_and(|pending| {
            pending.row.id == row.id && tab.current_key.as_ref() == Some(&pending.key)
        }) {
            let name = if name.chars().count() > 80 {
                format!("{}...", name.chars().take(77).collect::<String>())
            } else {
                name
            };
            let text = self
                .tr("dialogue.remove_tip")
                .replace("{type}", "")
                .replace("{name}", &name)
                .trim_start()
                .to_owned();
            let color: Hsla = if self.dark(cx) {
                rgba(0xffffffd1).into()
            } else {
                rgb(0x333639).into()
            };
            let surface: Hsla = rgb(if self.dark(cx) { 0x2c2c32 } else { 0xffffff }).into();
            let width = window
                .text_system()
                .shape_line(
                    text.clone().into(),
                    px(14.),
                    &[TextRun {
                        len: text.len(),
                        font: self.preference_font("general", ".SystemUIFont", cx),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width
                .as_f32()
                + 58.;
            let cancel = self.tr("common.cancel");
            let confirm = self.tr("common.confirm");
            let label_width = |label: &str| {
                window
                    .text_system()
                    .shape_line(
                        label.to_owned().into(),
                        px(14.),
                        &[TextRun {
                            len: label.len(),
                            font: self.preference_font("general", ".SystemUIFont", cx),
                            color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width
                    .as_f32()
                    + 16.
            };
            let cancel_width = label_width(&cancel);
            let confirm_width = label_width(&confirm);
            let body = div()
                .id("entry-delete-confirmation")
                .test_support()
                .relative()
                .occlude()
                .w(px(width))
                .py(px(8.))
                .px(px(14.))
                .rounded(px(3.))
                .text_size(px(14.))
                .line_height(px(21.))
                .text_color(color)
                .bg(surface)
                .shadow(super::notices::shadows(self.dark(cx)))
                .on_mouse_down_out(cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                    if !trigger.get().contains(&event.position) {
                        if let Some(i) = app.tab_index(id) {
                            app.tabs[i].entry_delete = None;
                            cx.notify();
                        }
                    }
                }))
                .child(
                    div()
                        .id("entry-delete-confirmation-body")
                        .test_support()
                        .flex()
                        .items_center()
                        .h(px(22.))
                        .gap(px(8.))
                        .child(
                            icon("notice-warning")
                                .size(px(22.))
                                .flex_shrink_0()
                                .text_color(rgb(if self.dark(cx) { 0xf2c97d } else { 0xf0a020 })),
                        )
                        .child(text),
                )
                .child(
                    div()
                        .id("entry-delete-confirmation-actions")
                        .test_support()
                        .flex()
                        .items_center()
                        .justify_end()
                        .mt(px(8.))
                        .gap(px(8.))
                        .h(px(28.))
                        .child(
                            self.btn("entry-delete-cancel", cancel, "", cx, move |app, _, cx| {
                                if let Some(i) = app.tab_index(id) {
                                    app.tabs[i].entry_delete = None;
                                    cx.notify();
                                }
                            })
                            .h(px(28.))
                            .w(px(cancel_width))
                            .px(px(7.))
                            .rounded(px(3.))
                            .text_color(color)
                            .bg(Hsla::transparent_black())
                            .border_color(if self.dark(cx) {
                                Hsla::from(rgba(0xffffff17))
                            } else {
                                rgb(0xe0e0e6).into()
                            }),
                        )
                        .child(
                            self.btn(
                                "entry-delete-confirm",
                                confirm,
                                "",
                                cx,
                                move |app, w, cx| app.confirm_entry_delete(id, w, cx),
                            )
                            .primary()
                            .h(px(28.))
                            .w(px(confirm_width))
                            .px(px(7.))
                            .rounded(px(3.))
                            .bg(rgb(0xd33a31))
                            .border_color(rgb(0xd33a31))
                            .text_color(rgb(if self.dark(cx) { 0x000000 } else { 0xffffff })),
                        ),
                )
                .child(
                    icon("entry-delete-arrow")
                        .w(px(12.))
                        .h(px(6.))
                        .absolute()
                        .bottom(px(-6.))
                        .right(px(8.))
                        .text_color(surface),
                );
            popup = popup.content(body);
        }
        popup.into_any_element()
    }
}
