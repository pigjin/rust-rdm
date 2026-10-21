use super::dialogs::DialogKind;
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

pub(super) fn menu_action(
    label: impl Into<SharedString>,
    weak: WeakEntity<RdmApp>,
    handler: impl Fn(&mut RdmApp, &mut Window, &mut Context<RdmApp>) + 'static,
) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::new(localized(&label)).on_click(move |_, window, cx| {
        let _ = weak.update(cx, |app, cx| handler(app, window, cx));
    })
}
impl RdmApp {
    pub(super) fn btn(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        ico: &str,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        let label = label.into();
        let id = id.into();
        let filled = matches!(
            id.as_ref(),
            "dialog-save" | "save-entry" | "save-value" | "publish"
        ) || id.as_ref().starts_with("radio-");
        let mut b = Button::new(id)
            .with_size(gpui_component::Size::Size(px(26.6667)))
            .font_weight(FontWeight::NORMAL)
            .when(!label.is_empty(), |button| button.label(localized(&label)))
            .when(!filled, |button| button.outline())
            .on_click(cx.listener(move |app, _, w, cx| handler(app, w, cx)))
            .h(px(32.))
            .px(px(12.));
        if !ico.is_empty() {
            b = b.icon(icon(ico).size(px(18.)));
        }
        b
    }
    pub(super) fn icon_btn(
        &self,
        id: impl Into<SharedString>,
        ico: &str,
        tip: impl Into<SharedString>,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        self.btn(id, "", ico, cx, handler)
            .ghost()
            .compact()
            .px(px(3.))
            .tooltip(tip)
            .w(px(28.))
            .flex_shrink_0()
    }
    fn column_header(
        &self,
        id: u64,
        key: &'static str,
        label: String,
        width: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(width))
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(label)
            .child(self.resize_handle(id, key, width, cx))
            .into_any_element()
    }
    pub(super) fn resize_handle(
        &self,
        id: u64,
        key: &'static str,
        width: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!("resize-{key}"))
            .test_support()
            .absolute()
            .right_0()
            .top_0()
            .h_full()
            .w(px(6.))
            .cursor(CursorStyle::ResizeColumn)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |a, event: &MouseDownEvent, _, cx| {
                    a.column_drag = Some((id, key.into(), event.position.x.as_f32(), width));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }
    pub(super) fn render_app(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.prepare_key_selectors(window, cx);
        let title = self.render_title(window, cx);
        let ribbon = self.render_ribbon(cx);
        let content = if self.nav == Nav::Log {
            self.render_log(cx)
        } else {
            let sidebar = if self.nav == Nav::Server {
                self.render_connections(cx)
            } else {
                self.render_browser(window, cx)
            };
            let main = if self.nav == Nav::Server {
                self.render_empty(cx)
            } else {
                self.render_content(window, cx)
            };
            self.render_sidebar(sidebar, main, window, cx)
        };
        let general_font = self.preference_font("general", ".SystemUIFont", cx);
        let mut root = div()
            .id("rdm-root")
            .key_context("Rdm")
            .capture_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && app.cancel_tab_close(cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|app, event, window, cx| app.number_key(event, window, cx)))
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .font(gpui::font(cx.theme().font_family.clone()))
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(
                !cfg!(target_os = "windows") && !window.is_fullscreen(),
                |root| {
                    root.border_1()
                        .border_color(cx.theme().border)
                        .rounded(px(10.))
                },
            )
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(14.))
            .capture_key_down(cx.listener(|a, event, w, cx| a.cli_key(event, w, cx)))
            .on_mouse_move(cx.listener(|a, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    if a.column_drag.take().is_some() {
                        cx.notify();
                    }
                    return;
                }
                if let Some((id, key, x, width)) = a.column_drag.clone() {
                    if let Some(i) = a.tab_index(id) {
                        a.tabs[i].column_widths.insert(
                            key,
                            (width + event.position.x.as_f32() - x).clamp(60., 1800.),
                        );
                        let count = a.tabs[i].entry_list.item_count();
                        a.tabs[i].entry_list.splice(0..count, count);
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|a, _, _, cx| {
                    a.column_drag = None;
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|app, _: &NewConnection, w, cx| {
                app.open_dialog(DialogKind::Connection(None), w, cx)
            }))
            .on_action(cx.listener(|app, _: &PreferencesAction, w, cx| {
                app.open_dialog(DialogKind::Preferences, w, cx)
            }))
            .on_action(cx.listener(|app, _: &CancelDialog, w, cx| {
                if app.cancel_tab_close(cx) {
                    return;
                }
                if app.dismiss_entry_picker(w, cx) {
                    return;
                }
                if let Some(id) = app.active {
                    app.close_database_picker(id, w, cx);
                    app.close_type_picker(id, cx);
                    app.close_footer_picker(id, w, cx);
                }
                if let Some(i) = app.index() {
                    app.tabs[i].entry_delete = None;
                }
                app.close_dialog(w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &SaveValue, w, cx| {
                // The original binds save on its Monaco editors, rather than forms.
                let enabled = if app.editing_entry() {
                    app.entry_editor_focused(w, cx)
                } else if app.dialog.is_none() && app.nav == Nav::Browser {
                    app.index().is_some_and(|i| {
                        app.tabs[i].pane == Pane::Detail
                            && app.tabs[i].editor.focus_handle(cx).contains_focused(w, cx)
                    })
                } else {
                    false
                };
                if enabled {
                    app.save_active(w, cx);
                }
            }))
            .on_action(
                cx.listener(|app, action: &diagnostic_ui::SaveFromMessage, w, cx| {
                    let owns_editor = if app.editing_entry() {
                        app.entry_editor_id()
                            .is_some_and(|editor| editor.as_u64() == action.editor)
                    } else if app.dialog.is_none() && app.nav == Nav::Browser {
                        app.index().is_some_and(|i| {
                            app.tabs[i].pane == Pane::Detail
                                && app.tabs[i].editor.entity_id().as_u64() == action.editor
                        })
                    } else {
                        false
                    };
                    if owns_editor {
                        app.save_active(w, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|app, _: &gpui_component::input::Copy, window, cx| {
                    if !app.copy_csv_selection(window, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|app, _: &CloseTab, w, cx| {
                if let Some(id) = app.active {
                    app.request_tab_close(id, false, w, cx);
                }
            }))
            .on_action(cx.listener(|app, _: &Refresh, w, cx| {
                if let Some(id) = app.active {
                    app.scan_keys(id, true, false, w, cx);
                    app.reload_detail(id, true, w, cx);
                }
            }))
            .on_action(cx.listener(|app, _: &FocusFilter, w, cx| {
                if let Some(i) = app.index() {
                    app.tabs[i]
                        .filter
                        .update(cx, |input, cx| input.focus(w, cx));
                }
            }))
            .child(title)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .font(general_font)
                    .text_size(px(self.general_font_size()))
                    .child(ribbon)
                    .child(div().flex_1().min_w_0().child(content)),
            )
            .child(self.sidebar_events(cx));
        if self.busy > self.entry_conversions {
            root = root.child(
                div()
                    .id("app-loading-indicator")
                    .test_support()
                    .absolute()
                    .bottom(px(3.))
                    .right(px(8.))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{}…", localized("加载中"))),
            );
        }
        if let Some((release, description, url)) = &self.update_notice {
            let download = url.clone();
            root = root.child(
                div()
                    .absolute()
                    .top(px(50.))
                    .right(px(20.))
                    .w(px(400.))
                    .p_4()
                    .rounded(px(4.))
                    .shadow_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{} - {}",
                        self.tr("dialogue.upgrade.title"),
                        release.version
                    )))
                    .child(div().py_3().child(description.clone()))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                self.btn(
                                    "update-skip",
                                    self.tr("dialogue.upgrade.skip"),
                                    "",
                                    cx,
                                    |a, _, cx| {
                                        if let Some((release, _, _)) = a.update_notice.take() {
                                            a.pref.general.skip_version = release.version;
                                            a.save_preferences();
                                        }
                                        cx.notify();
                                    },
                                )
                                .small(),
                            )
                            .child(
                                self.btn(
                                    "update-later",
                                    self.tr("dialogue.upgrade.later"),
                                    "",
                                    cx,
                                    |a, _, cx| {
                                        a.update_notice = None;
                                        cx.notify();
                                    },
                                )
                                .small(),
                            )
                            .child(
                                self.btn(
                                    "update-download",
                                    self.tr("dialogue.upgrade.download_now"),
                                    "",
                                    cx,
                                    move |_, _, cx| cx.open_url(&download),
                                )
                                .small()
                                .primary(),
                            ),
                    ),
            );
        }
        if self.dialog.is_some() && !self.editing_entry() {
            root = root.child(self.render_dialog(window, cx));
        }
        if !self.pref.behavior.welcomed {
            root = root.child(self.render_welcome(cx));
        }
        for warning in &self.tab_close_warnings {
            root = root.child(self.render_tab_close_warning(warning, cx));
        }
        root = root.child(self.render_notices(window, cx));
        root.into_any_element()
    }
    fn render_welcome(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("welcome-notice")
            .absolute()
            .bottom(px(50.))
            .right(px(16.))
            .w(px(365.))
            .pl(px(24.))
            .pr(px(16.))
            .py(px(16.))
            .rounded(px(4.))
            .bg(cx.theme().background)
            .shadow_lg()
            .occlude()
            .font(gpui::font(".SystemUIFont"))
            .text_size(px(14.))
            .line_height(px(21.))
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight(500.))
                    .mb(px(12.))
                    .child(self.tr("dialogue.welcome.title")),
            )
            .child(div().child(self.tr("dialogue.welcome.content")))
            .child(div().h(px(12.)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(12.))
                    .child(
                        self.btn(
                            "welcome-reject",
                            self.tr("dialogue.welcome.reject"),
                            "",
                            cx,
                            |app, _, cx| app.choose_welcome(false, cx),
                        )
                        .ghost()
                        .bg(cx.theme().muted)
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        self.btn(
                            "welcome-accept",
                            self.tr("dialogue.welcome.accept"),
                            "",
                            cx,
                            |app, _, cx| app.choose_welcome(true, cx),
                        )
                        .ghost()
                        .bg(cx.theme().primary.opacity(0.16))
                        .text_color(cx.theme().primary),
                    ),
            )
            .into_any_element()
    }
    fn render_title(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let caption = if self.nav == Nav::Browser {
            self.index()
                .map(|i| format!("- {}", self.tabs[i].conn.name))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut row = div()
            .id("titlebar")
            .test_support()
            .flex()
            .items_end()
            .h(px(39.))
            .flex_shrink_0()
            .bg(self.sidebar_color(cx))
            .border_b_1()
            .border_color(self.split_color(cx))
            .on_mouse_down(MouseButton::Left, |event, w, _| {
                if event.click_count == 2 {
                    w.zoom_window();
                } else {
                    w.start_window_move();
                }
            })
            .child(
                div()
                    .id("titlebar-title")
                    .test_support()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .pl(px(if cfg!(target_os = "macos") { 70. } else { 10. }))
                    .pr(px(10.))
                    .w(px(self.pref.behavior.aside_width + 50. - 4.))
                    .h(px(32.))
                    .self_center()
                    .flex_shrink_0()
                    .child(
                        img(ImageSource::Resource(Resource::Embedded(
                            "images/icon.png".into(),
                        )))
                        .size(px(32.)),
                    )
                    .child(
                        div()
                            .min_w(px(68.))
                            .flex_shrink_0()
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .child("Tiny RDM"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(caption),
                    ),
            );
        if self.nav == Nav::Browser {
            row = row.child(self.render_connection_tabs(window, cx));
        }
        row.child(div().w(px(15.)).min_w(px(15.)).flex_grow(1.))
            .into_any_element()
    }
    fn render_ribbon(&self, cx: &Context<Self>) -> AnyElement {
        let rail = div()
            .flex()
            .flex_col()
            .w(px(50.))
            .flex_shrink_0()
            .h_full()
            .pt(px(5.))
            .bg(self.ribbon_color(cx))
            .border_r_1()
            .border_color(self.split_color(cx));
        let mut navigation = div().flex().flex_col().gap(px(2.)).pr(px(3.));
        for (nav, ico, label) in [
            (Nav::Browser, "database", "ribbon.browser"),
            (Nav::Server, "server", "ribbon.server"),
            (Nav::Log, "record", "ribbon.log"),
        ] {
            if nav == Nav::Browser && self.tabs.is_empty() {
                continue;
            }
            let selected = self.nav == nav;
            navigation = navigation.child(
                div()
                    .relative()
                    .group("ribbon-item")
                    .h(px(40.))
                    .w_full()
                    .hover(|row| row.bg(rgba(0x0000000d)))
                    .child(
                        self.icon_btn(
                            format!("nav-{ico}"),
                            ico,
                            self.tr(label),
                            cx,
                            move |app, _, cx| {
                                app.nav = nav;
                                cx.notify();
                            },
                        )
                        .text()
                        .with_size(gpui_component::Size::Size(px(29.3333)))
                        .icon(icon(&format!("{ico}-thick")).size(px(22.)))
                        .w_full()
                        .h(px(40.))
                        .text_color(if selected {
                            cx.theme().primary
                        } else if self.dark(cx) {
                            Hsla::from(rgba(0xffffff85))
                        } else {
                            Hsla::from(rgb(0x767c82))
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(9.6))
                            .bottom(px(9.6))
                            .w(px(3.))
                            .rounded(px(999.))
                            .bg(cx.theme().primary)
                            .when(!selected, |bar| {
                                bar.invisible()
                                    .group_hover("ribbon-item", |bar| bar.visible())
                            }),
                    ),
            );
        }
        let weak = cx.weak_entity();
        let preferences = self.tr("menu.preferences");
        let about = self.tr("menu.about");
        let chinese = matches!(self.current_language().as_str(), "zh" | "zh-cn");
        let guide = if chinese {
            "https://tinyrdm.com/zh/guide/"
        } else {
            "https://tinyrdm.com/guide/"
        };
        let report = self.tr("menu.report_bug");
        let guide_label = self.tr("menu.user_guide");
        let update_label = self.tr("menu.check_update");
        let menu = self
            .connection_icon_button(
                "settings",
                "config",
                self.tr("menu.preferences"),
                18.,
                cx,
                |_, _, _| {},
            )
            .icon(icon("config").size(px(22.)))
            .with_size(gpui_component::Size::Size(px(29.3333)))
            .allow_icon_overflow()
            .size(px(18.))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                menu.tiny_rdm_style(true)
                    .item(
                        menu_action(preferences.clone(), weak.clone(), |app, w, cx| {
                            app.open_dialog(DialogKind::Preferences, w, cx)
                        })
                        .icon(icon("config")),
                    )
                    .item(PopupMenuItem::link(
                        report.clone(),
                        "https://github.com/tiny-craft/tiny-rdm/issues",
                    ))
                    .item(PopupMenuItem::link(guide_label.clone(), guide))
                    .item(menu_action(
                        update_label.clone(),
                        weak.clone(),
                        |app, w, cx| app.check_updates(w, cx),
                    ))
                    .separator()
                    .item(menu_action(about.clone(), weak.clone(), |app, w, cx| {
                        app.open_dialog(DialogKind::About, w, cx)
                    }))
            })
            .offset(px(6.));
        let social = if chinese {
            self.connection_icon_button(
                "wechat",
                "qrcode",
                self.tr("ribbon.wechat_official"),
                18.,
                cx,
                |app, w, cx| app.open_dialog(DialogKind::Community, w, cx),
            )
            .icon(icon("qrcode").size(px(22.)))
            .with_size(gpui_component::Size::Size(px(29.3333)))
        } else {
            self.connection_icon_button(
                "twitter",
                "twitter",
                self.tr("ribbon.follow_x"),
                18.,
                cx,
                |_, _, cx| cx.open_url("https://twitter.com/LykinHuang"),
            )
            .icon(icon("twitter").size(px(22.)))
            .with_size(gpui_component::Size::Size(px(29.3333)))
        };
        let footer = div()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(10.))
            .pb(px(15.))
            .child(div().my(px(10.)).ml(px(1.)).child(menu))
            .child(
                social
                    .allow_icon_overflow()
                    .size(px(18.))
                    .my(px(10.))
                    .ml(px(1.)),
            )
            .child(
                self.connection_icon_button(
                    "github",
                    "github",
                    self.tr("ribbon.github"),
                    18.,
                    cx,
                    |_, _, cx| cx.open_url("https://github.com/tiny-craft/tiny-rdm"),
                )
                .icon(icon("github").size(px(22.)))
                .with_size(gpui_component::Size::Size(px(29.3333)))
                .allow_icon_overflow()
                .size(px(18.))
                .my(px(10.))
                .ml(px(1.)),
            );
        rail.child(navigation)
            .child(div().flex_1())
            .child(footer)
            .into_any_element()
    }
    fn render_empty(&self, cx: &Context<Self>) -> AnyElement {
        let language = self.current_language();
        let sponsor = crate::update::Sponsor::select(&self.sponsors, &language);
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .gap_0()
            .child(
                icon("empty")
                    .size(px(40.))
                    .mb(px(8.))
                    .text_color(rgb(if self.dark(cx) { 0x606060 } else { 0xc2c2c2 })),
            )
            .child(
                div()
                    .mb(px(12.))
                    .text_size(px(14.))
                    .text_color(if self.dark(cx) {
                        Hsla::from(rgba(0xffffff61))
                    } else {
                        Hsla::from(rgb(0xc2c2c2))
                    })
                    .child(self.tr("interface.empty_server_content")),
            )
            .child(self.btn(
                "empty-new",
                self.tr("interface.new_conn"),
                "addlink",
                cx,
                |app, w, cx| app.open_dialog(DialogKind::Connection(None), w, cx),
            ))
            .when_some(sponsor, |container, sponsor| {
                let url = sponsor.link.clone();
                container.child(
                    self.btn(
                        "sponsor-link",
                        sponsor.name.clone(),
                        "",
                        cx,
                        move |_, _, cx| cx.open_url(&url),
                    )
                    .text()
                    .text_color(cx.theme().muted_foreground)
                    .mt(px(20.))
                    .h_auto()
                    .p_0(),
                )
            })
            .into_any_element()
    }
    pub(super) fn connection_icon_button(
        &self,
        id: impl Into<SharedString>,
        ico: &str,
        tip: String,
        size: f32,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        Button::new(SharedString::from(id.into()))
            .text()
            .with_size(gpui_component::Size::Size(px(26.6667)))
            .icon(icon(ico).size(px(20.)))
            .size(px(size))
            .p_0()
            .flex_shrink_0()
            .tooltip(tip)
            .text_color(if self.dark(cx) {
                Hsla::from(rgba(0xffffffd1))
            } else {
                Hsla::from(rgb(0x333639))
            })
            .on_click(cx.listener(move |app, _, window, cx| {
                handler(app, window, cx);
                cx.stop_propagation();
            }))
    }
    pub(super) fn search_clear_button(
        &self,
        id: &'static str,
        group: &'static str,
        focused: bool,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        Button::new(id)
            .text()
            .with_size(gpui_component::Size::Size(px(16.)))
            .icon(icon("input-clear").size(px(16.)))
            .content_style(
                StyleRefinement::default(),
                gpui_component::Size::Size(px(16.)),
            )
            .size(px(16.))
            .p_0()
            .flex_shrink_0()
            .tab_stop(false)
            .text_color(Hsla::from(if self.dark(cx) {
                rgba(0xffffff61)
            } else {
                rgb(0xc2c2c2)
            }))
            .opacity(if focused { 1. } else { 0. })
            .group_hover(group, |style| style.opacity(1.))
            .hover_style(StyleRefinement::default().bg(rgba(0x00000000)))
            .active_style(StyleRefinement::default().bg(rgba(0x00000000)))
            .on_click(cx.listener(move |app, _, window, cx| {
                handler(app, window, cx);
                cx.stop_propagation();
            }))
    }
    pub(super) fn disabled_button_opacity(&self, disabled: bool, cx: &App) -> f32 {
        if !disabled {
            1.
        } else if self.dark(cx) {
            0.38
        } else {
            0.5
        }
    }
    pub(super) fn search_match_tag(
        &self,
        tag_id: &'static str,
        close_id: &'static str,
        label: String,
        width: f32,
        input_focus: FocusHandle,
        tooltip: super::match_tooltip::MatchTarget,
        window: &Window,
        cx: &Context<Self>,
        edit: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        clear: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let close = Button::new(close_id)
            .text()
            .with_size(gpui_component::Size::Size(px(12.)))
            .size(px(12.))
            .p_0()
            .flex_shrink_0()
            .icon(icon("search-match-close").size(px(16.)))
            .content_style(
                StyleRefinement::default(),
                gpui_component::Size::Size(px(16.)),
            )
            .allow_icon_overflow()
            .rounded(px(3.))
            .tab_stop(false)
            .text_color(Hsla::from(if self.dark(cx) {
                rgba(0xffffff85)
            } else {
                rgb(0x666666)
            }))
            .on_click(cx.listener(move |app, _, window, cx| {
                clear(app, window, cx);
                cx.stop_propagation();
            }));
        let tag = div()
            .id(tag_id)
            .test_support()
            .on_hover(cx.listener(move |app, entered, w, cx| app.match_tooltip_hover(tooltip, *entered, w, cx)))
            .flex()
            .items_center()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .flex_shrink_0()
            .h(px(22.))
            .px(px(6.))
            .border_1()
            .border_color(if self.dark(cx) {
                rgb(0x515151)
            } else {
                rgb(0xe0e0e6)
            })
            .gap(px(4.))
            .bg(if self.dark(cx) {
                rgba(0x00000000)
            } else {
                rgba(0xfafafcff)
            })
            .rounded(px(4.))
            .text_color(Hsla::from(if self.dark(cx) {
                rgba(0xffffffd1)
            } else {
                rgb(0x333639)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                    // The original NInput focuses its text field when a
                    // prefix is pressed. Keep the frame's generic focus
                    // handler from taking that focus on the first click.
                    window.prevent_default();
                    input_focus.focus(window, cx);
                    cx.stop_propagation();
                    if event.click_count == 2 {
                        edit(app, window, cx);
                    }
                }),
            )
            .child(
                div()
                    .id(format!("{tag_id}-label"))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(12.))
                    .line_height(px(12.))
                    .h(px(12.))
                    .child({
                        #[cfg(target_os = "macos")]
                        { super::text::CssLabel { text: label, text_id: format!("{tag_id}-text") }.into_any_element() }
                        #[cfg(not(target_os = "macos"))]
                        { label.into_any_element() }
                    }),
            )
            .child(close);
        self.render_match_tooltip(tooltip, tag.into_any_element(), width, window, cx)
    }
    pub(super) fn search_input(
        &self,
        state: &Entity<InputState>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Input {
        let focused = state.focus_handle(cx).is_focused(window);
        let background: Hsla = if !self.dark(cx) {
            rgb(0xffffff).into()
        } else if focused {
            rgba(0xd33a311a).into()
        } else {
            rgba(0xffffff1a).into()
        };
        Input::new(state)
            .small()
            .text_size(px(14.))
            .cleanable(false)
            .focus_bordered(false)
            .bg(background)
            // Naive's dark input has a transparent idle border; its fill
            // composites over the preceding tag's border in an input group.
            .border_color(if self.dark(cx) {
                Hsla::transparent_black()
            } else {
                cx.theme().border
            })
            .when(focused, |input| input.border_color(rgb(0xd33a31)))
    }
    pub(super) fn search_input_ring(
        &self,
        state: &Entity<InputState>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        state.focus_handle(cx).is_focused(window).then(|| {
            // CSS outset shadows leave the translucent input's interior clear.
            div()
                .absolute()
                .left(px(-2.))
                .right(px(-2.))
                .top(px(-2.))
                .bottom(px(-2.))
                .border_2()
                .border_color(rgba(0xd33a3133))
                .rounded(px(2.))
                .into_any_element()
        })
    }
    pub(super) fn connection_node_color(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0xceced0 } else { 0x333639 }).into()
    }
    pub(super) fn connection_mark_color(&self, value: &str) -> Option<Hsla> {
        if value.is_empty() {
            return None;
        }
        let digits = value.strip_prefix('#').unwrap_or(value);
        let digits: String = digits.chars().take_while(char::is_ascii_hexdigit).collect();
        let raw = if value.len() < 6 {
            0
        } else {
            u32::from_str_radix(&digits, 16).unwrap_or(0)
        };
        let channel = |shift: u32| -> u32 { (((raw >> shift) & 255u32) * 3 + 2) / 4 };
        Some(rgb((channel(16) << 16) | (channel(8) << 8) | channel(0)).into())
    }
    fn render_connections(&self, cx: &Context<Self>) -> AnyElement {
        let import_label = self.tr("interface.import_conn");
        let export_label = self.tr("interface.export_conn");
        let weak = cx.weak_entity();
        let sidebar = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(self.sidebar_color(cx))
            .border_r_1()
            .border_color(self.split_color(cx));
        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(8.))
            .py(px(3.))
            .h(px(37.359375))
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(self.connection_icon_button(
                "new-conn",
                "addlink-thick",
                self.tr("interface.new_conn"),
                24.,
                cx,
                |app, w, cx| app.open_dialog(DialogKind::Connection(None), w, cx),
            ))
            .child(self.connection_icon_button(
                "new-group",
                "addgroup-thick",
                self.tr("interface.new_group"),
                24.,
                cx,
                |app, w, cx| app.open_dialog(DialogKind::Group(None), w, cx),
            ))
            .child(
                div()
                    .w(px(16.640625))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(div().w(px(1.)).h(px(14.)).bg(rgb(0xaaaaab))),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Styled::h(Input::new(&self.conn_filter), px(32.))
                        .text_size(px(14.))
                        .px(px(11.))
                        .cleanable(true)
                        .prefix(icon("filter").size(px(20.)).text_color(if self.dark(cx) {
                            Hsla::from(rgba(0xffffff61))
                        } else {
                            Hsla::from(rgb(0xc2c2c2))
                        })),
                ),
            )
            .child(
                self.connection_icon_button(
                    "connections-more",
                    "more-thick",
                    self.tr("interface.more_action"),
                    24.,
                    cx,
                    |_, _, _| {},
                )
                .dropdown_menu_with_anchor(Anchor::BottomRight, move |m, _, _| {
                    m.tiny_rdm_style(false)
                        .item(
                            menu_action(import_label.clone(), weak.clone(), |a, w, cx| {
                                a.connection_file(false, w, cx)
                            })
                            .icon(icon("import")),
                        )
                        .item(
                            menu_action(export_label.clone(), weak.clone(), |a, w, cx| {
                                a.connection_file(true, w, cx)
                            })
                            .icon(icon("export")),
                        )
                })
                .offset(px(6.)),
            );
        let filter = text_input(&self.conn_filter, cx).to_lowercase();
        let mut list = div()
            .id("connection-list")
            .track_focus(&self.connection_focus)
            .track_scroll(&self.connection_scroll)
            .key_context("ConnectionTree")
            .on_key_down(
                cx.listener(|app, event, window, cx| app.connection_key(event, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| app.connection_focus.focus(window, cx)),
            )
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(3.))
            .text_size(px(self.general_font_size()))
            .text_color(self.connection_node_color(cx))
            .on_drop(cx.listener(|a, item: &ConnectionDrag, _, cx| {
                a.move_connection(item, crate::order::Destination::Root, cx);
                cx.stop_propagation();
            }));
        for (group_index, c) in self.connections.iter().enumerate() {
            if c.category != "group" {
                list = list.child(self.connection_row(c, 0., &filter, cx));
                continue;
            }
            let key = format!("{}/", c.name);
            let selected = self.connection_selected.as_deref() == Some(key.as_str());
            let explicit_open = self.groups_open.contains(&c.name);
            let open = explicit_open;
            let toggle_name = c.name.clone();
            let click_name = c.name.clone();
            let click_key = key.clone();
            let right_key = key.clone();
            let drop_name = c.name.clone();
            let weak = cx.weak_entity();
            let edit_name = c.name.clone();
            let delete_name = c.name.clone();
            let matched = !filter.is_empty() && c.name.to_lowercase().contains(&filter);
            let label = div()
                .flex_1()
                .min_w_0()
                .h(px(self.general_font_size() * 1.5 + 1.))
                .line_height(px(self.general_font_size() * 1.5))
                .truncate()
                .border_b_1()
                .border_color(if matched {
                    if self.dark(cx) {
                        Hsla::from(rgba(0xffffff61))
                    } else {
                        Hsla::from(rgb(0xc2c2c2))
                    }
                } else {
                    Hsla::transparent_black()
                })
                .when(self.general_font_size() != 0., |label| {
                    label.child(c.name.clone())
                });
            let mut content = div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .pl(px(4.))
                .pr(px(6.))
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .mr(px(8.))
                        .flex_shrink_0()
                        .child(folder_icon(explicit_open, cx)),
                )
                .child(label);
            if selected {
                let rename = c.name.clone();
                let delete = c.name.clone();
                content = content.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .mr(px(5.))
                        .child(self.connection_icon_button(
                            format!("group-rename-{group_index}"),
                            "config",
                            self.tr("interface.rename_conn_group"),
                            18.,
                            cx,
                            move |a, w, cx| {
                                a.open_dialog(DialogKind::Group(Some(rename.clone())), w, cx)
                            },
                        ))
                        .child(self.connection_icon_button(
                            format!("group-delete-{group_index}"),
                            "delete",
                            self.tr("interface.remove_conn_group"),
                            18.,
                            cx,
                            move |a, w, cx| {
                                a.open_dialog(DialogKind::DeleteGroup(delete.clone()), w, cx)
                            },
                        )),
                );
            }
            let group = div()
                .id(("group", group_index))
                .flex()
                .items_center()
                .h(px(30.))
                .cursor_pointer()
                .rounded(px(3.))
                .when(selected, |row| row.bg(cx.theme().primary.opacity(0.1)))
                .when(
                    !selected && self.connection_pending.as_deref() == Some(key.as_str()),
                    |row| row.bg(self.connection_hover_color(cx)),
                )
                .when(!selected, |row| {
                    row.hover(|row| row.bg(self.connection_hover_color(cx)))
                })
                .on_drag(
                    ConnectionDrag {
                        name: c.name.clone(),
                        group: true,
                    },
                    |drag: &ConnectionDrag, _, _, cx| cx.new(|_| drag.clone()),
                )
                .on_drop(cx.listener(move |a, drag: &ConnectionDrag, _, cx| {
                    let destination = if drag.group {
                        crate::order::Destination::Before {
                            name: drop_name.clone(),
                            group: true,
                        }
                    } else {
                        crate::order::Destination::Group(drop_name.clone())
                    };
                    a.move_connection(drag, destination, cx);
                    cx.stop_propagation();
                }))
                .on_click(cx.listener(move |a, event: &ClickEvent, _, cx| {
                    a.connection_selected = Some(click_key.clone());
                    a.connection_pending = Some(click_key.clone());
                    if event.click_count() == 2 && !a.groups_open.insert(click_name.clone()) {
                        a.groups_open.remove(&click_name);
                    }
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |a, _, _, cx| {
                        a.connection_selected = Some(right_key.clone());
                        a.connection_pending = Some(right_key.clone());
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id(("group-switcher", group_index))
                        .w(px(24.))
                        .h(px(30.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(!c.connections.is_empty(), |switcher| {
                            switcher
                                .on_click(cx.listener(move |a, _, _, cx| {
                                    if !a.groups_open.insert(toggle_name.clone()) {
                                        a.groups_open.remove(&toggle_name);
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .child(
                                    icon(if open {
                                        "tree-switcher-down"
                                    } else {
                                        "tree-switcher-right"
                                    })
                                    .size(px(14.))
                                    .text_color(
                                        if self.dark(cx) {
                                            Hsla::from(rgba(0xffffff85))
                                        } else {
                                            Hsla::from(rgb(0x767c82))
                                        },
                                    ),
                                )
                        }),
                )
                .child(content)
                .context_menu(move |m, _, _| {
                    m.tiny_rdm_style(true)
                        .item(
                            menu_action("重命名分组", weak.clone(), {
                                let name = edit_name.clone();
                                move |a, w, cx| {
                                    a.open_dialog(DialogKind::Group(Some(name.clone())), w, cx)
                                }
                            })
                            .icon(icon("edit")),
                        )
                        .item(
                            menu_action("删除分组", weak.clone(), {
                                let name = delete_name.clone();
                                move |a, w, cx| {
                                    a.open_dialog(DialogKind::DeleteGroup(name.clone()), w, cx)
                                }
                            })
                            .icon(icon("delete")),
                        )
                });
            list = list.child(group);
            if open {
                for child in &c.connections {
                    let mut child = child.clone();
                    child.group = c.name.clone();
                    list = list.child(self.connection_row(&child, 24., &filter, cx));
                }
            }
        }
        if self.connections.is_empty() {
            list = list.child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(icon("empty").size(px(40.)).text_color(if self.dark(cx) {
                        rgb(0x606060)
                    } else {
                        rgb(0xc2c2c2)
                    }))
                    .child(
                        div()
                            .mt(px(8.))
                            .line_height(px(21.))
                            .text_color(if self.dark(cx) {
                                Hsla::from(rgba(0xffffff61))
                            } else {
                                Hsla::from(rgb(0xc2c2c2))
                            })
                            .child(self.tr("interface.empty_server_list")),
                    ),
            );
        }
        sidebar.child(list).child(toolbar).into_any_element()
    }
    fn connection_row(
        &self,
        c: &Connection,
        indent: f32,
        filter: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let key = format!("{}/{}", c.group, c.name);
        let selected = self.connection_selected.as_deref() == Some(key.as_str());
        let connected = self.tabs.iter().any(|t| t.conn.name == c.name);
        let matched = !filter.is_empty() && c.name.to_lowercase().contains(filter);
        let color = self
            .connection_mark_color(&c.mark_color)
            .unwrap_or(self.connection_node_color(cx));
        let prefix = if connected {
            img(ImageSource::Resource(Resource::Embedded(
                format!(
                    "icons/{}-connected.svg",
                    if c.cluster.enable {
                        "cluster"
                    } else {
                        "server"
                    }
                )
                .into(),
            )))
            .size(px(20.))
            .into_any_element()
        } else {
            icon(if c.cluster.enable {
                "cluster"
            } else {
                "server"
            })
            .size(px(20.))
            .text_color(color)
            .into_any_element()
        };
        let label = div()
            .id(format!("font-label-{key}"))
            .test_support()
            .flex_1()
            .min_w_0()
            .h(px(self.general_font_size() * 1.5 + 1.))
            .line_height(px(self.general_font_size() * 1.5))
            .truncate()
            .border_b_1()
            .border_color(if matched {
                if self.dark(cx) {
                    Hsla::from(rgba(0xffffff61))
                } else {
                    Hsla::from(rgb(0xc2c2c2))
                }
            } else {
                Hsla::transparent_black()
            })
            .text_color(color)
            .when(!c.mark_color.is_empty(), |label| {
                label.font_weight(FontWeight(450.))
            })
            .when(self.general_font_size() != 0., |label| {
                label.child(c.name.clone())
            });
        let mut content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .pl(px(4.))
            .pr(px(6.))
            .child(
                div()
                    .w(px(20.))
                    .h(px(20.))
                    .mr(px(8.))
                    .flex_shrink_0()
                    .child(prefix),
            )
            .child(label);
        if selected {
            let open = c.clone();
            let edit = c.clone();
            let delete = c.name.clone();
            let close = c.name.clone();
            let mut tools = div().flex().items_center().gap(px(3.)).mr(px(5.));
            tools = if connected {
                tools.child(self.connection_icon_button(
                    format!("connection-close-{key}"),
                    "unlink",
                    self.tr("interface.disconnect"),
                    18.,
                    cx,
                    move |a, w, cx| {
                        if let Some(t) = a.tabs.iter().find(|t| t.conn.name == close) {
                            a.close_tab(t.id, w, cx);
                        }
                    },
                ))
            } else {
                tools.child(self.connection_icon_button(
                    format!("connection-open-{key}"),
                    "connect",
                    self.tr("interface.open_connection"),
                    18.,
                    cx,
                    move |a, w, cx| a.open_connection(open.clone(), w, cx),
                ))
            };
            tools = tools.child(self.connection_icon_button(
                format!("connection-edit-{key}"),
                "config",
                self.tr("interface.edit_conn"),
                18.,
                cx,
                move |a, w, cx| a.edit_connection(edit.clone(), w, cx),
            ));
            if !connected {
                tools = tools.child(self.connection_icon_button(
                    format!("connection-delete-{key}"),
                    "delete",
                    self.tr("interface.remove_conn"),
                    18.,
                    cx,
                    move |a, w, cx| {
                        a.open_dialog(DialogKind::DeleteConnection(delete.clone()), w, cx)
                    },
                ));
            }
            content = content.child(tools);
        }
        let selected_key = key.clone();
        let context_key = key.clone();
        let target = c.name.clone();
        let config = c.clone();
        let config2 = c.clone();
        let weak = cx.weak_entity();
        div()
            .id(SharedString::from(format!("conn-{key}")))
            .flex()
            .items_center()
            .h(px(30.))
            .cursor_pointer()
            .pl(px(indent))
            .rounded(px(3.))
            .when(selected, |row| row.bg(cx.theme().primary.opacity(0.1)))
            .when(
                !selected && self.connection_pending.as_deref() == Some(key.as_str()),
                |row| row.bg(self.connection_hover_color(cx)),
            )
            .when(!selected, |row| {
                row.hover(|row| row.bg(self.connection_hover_color(cx)))
            })
            .on_drag(
                ConnectionDrag {
                    name: c.name.clone(),
                    group: false,
                },
                |drag: &ConnectionDrag, _, _, cx| cx.new(|_| drag.clone()),
            )
            .on_drop(cx.listener(move |a, drag: &ConnectionDrag, _, cx| {
                a.move_connection(
                    drag,
                    crate::order::Destination::Before {
                        name: target.clone(),
                        group: false,
                    },
                    cx,
                );
                cx.stop_propagation();
            }))
            .on_click(cx.listener(move |a, event: &ClickEvent, w, cx| {
                a.connection_selected = Some(selected_key.clone());
                a.connection_pending = Some(selected_key.clone());
                if event.click_count() == 2 {
                    a.open_connection(config.clone(), w, cx);
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |a, _, _, cx| {
                    a.connection_selected = Some(context_key.clone());
                    a.connection_pending = Some(context_key.clone());
                    cx.notify();
                }),
            )
            .child(div().w(px(24.)).flex_shrink_0())
            .child(content)
            .context_menu(move |m, _, _| {
                let mut m = m.tiny_rdm_style(true);
                let c = config2.clone();
                let edit = c.clone();
                let duplicate = c.clone();
                let delete = c.clone();
                let disconnect = c.name.clone();
                let w = weak.clone();
                if connected {
                    m = m.item(
                        menu_action("断开连接", w.clone(), move |a, win, cx| {
                            if let Some(t) = a.tabs.iter().find(|t| t.conn.name == disconnect) {
                                a.close_tab(t.id, win, cx);
                            }
                        })
                        .icon(icon("unlink")),
                    );
                } else {
                    m = m.item(
                        menu_action("打开连接", w.clone(), move |a, win, cx| {
                            a.open_connection(c.clone(), win, cx)
                        })
                        .icon(icon("connect")),
                    );
                }
                m.item(
                    menu_action("编辑连接配置", w.clone(), move |a, win, cx| {
                        a.edit_connection(edit.clone(), win, cx)
                    })
                    .icon(icon("config")),
                )
                .item(
                    menu_action("复制连接", w.clone(), move |a, win, cx| {
                        a.duplicate_connection(duplicate.clone(), win, cx)
                    })
                    .icon(icon("copylink")),
                )
                .separator()
                .item(
                    menu_action("删除连接", w, move |a, win, cx| {
                        a.open_dialog(DialogKind::DeleteConnection(delete.name.clone()), win, cx)
                    })
                    .icon(icon("delete")),
                )
            })
            .into_any_element()
    }
    fn move_connection(
        &mut self,
        item: &ConnectionDrag,
        destination: crate::order::Destination,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) =
            crate::order::move_item(&mut self.connections, &item.name, item.group, destination)
        {
            self.toast(error.to_string(), true);
        } else {
            for connection in flatten(&self.connections) {
                if let Some(tab) = self
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.conn.name == connection.name)
                {
                    tab.conn.group = connection.group;
                }
            }
            self.save_connections();
        }
        cx.notify();
    }
    fn render_content(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(i) = self.index() else {
            return self.render_empty(cx);
        };
        let t = &self.tabs[i];
        let tabs = self.render_content_nav(i, window, cx);
        let page = match t.pane {
            Pane::Status => self.render_status(i, window, cx),
            Pane::Detail => self.render_detail(i, window, cx),
            Pane::Cli => self.render_cli(i, cx),
            Pane::Slow => self.render_slow(i, cx),
            Pane::Monitor => self.render_stream(i, false, cx),
            Pane::Pubsub => self.render_stream(i, true, cx),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(tabs)
            .child(div().flex_1().min_h_0().overflow_hidden().child(page))
            .into_any_element()
    }
    fn render_status(&self, i: usize, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let t = &self.tabs[i];
        let id = t.id;
        let get = |section, key| info_value(&t.info, section, key);
        let dark = self.dark(cx);
        let card_color = if dark { rgb(0x212121) } else { rgb(0xfafafa) };
        let border: Hsla = if dark {
            rgba(0xffffff17).into()
        } else {
            rgb(0xefeff5).into()
        };
        let title_color: Hsla = if dark {
            rgba(0xffffffe6).into()
        } else {
            rgb(0x1f2225).into()
        };
        let value_color: Hsla = if dark {
            rgba(0xffffffd1).into()
        } else {
            rgb(0x333639).into()
        };
        let muted: Hsla = if dark {
            rgba(0xffffff85).into()
        } else {
            rgb(0x767c82).into()
        };
        let font = self.preference_font("general", ".SystemUIFont", cx);
        let mut title_font = font.clone();
        title_font.weight = FontWeight::MEDIUM;
        let title_width = super::text::script_width(&t.conn.name, title_font, 18., window);
        let mut main = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .flex_1()
            .min_w_0()
            .text_size(px(18.))
            .line_height(px(27.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(title_color)
            .child(
                div()
                    .id("status-server-name")
                    .test_support()
                    .w(px(title_width))
                    .h(px(27.))
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(status_text(
                        t.conn.name.clone(),
                        18.,
                        27.,
                        FontWeight::MEDIUM,
                        "status-server-name-text",
                        font.clone(),
                        window,
                        cx,
                    )),
            );
        let valkey = get("Server", "valkey_version");
        let redis = get("Server", "redis_version");
        let version = if valkey.is_empty() { redis } else { valkey };
        if !version.is_empty() {
            main = main.child(status_tag(
                "version",
                format!("v{version}"),
                dark,
                font.clone(),
                window,
                cx,
            ));
        }
        for (name, value) in [
            ("mode", get("Server", "redis_mode")),
            ("role", get("Replication", "role")),
        ] {
            if !value.is_empty() {
                main = main.child(status_tag(name, value, dark, font.clone(), window, cx));
            }
        }
        let head = div()
            .id("status-header-content")
            .test_support()
            .flex()
            .items_center()
            .h(px(28.))
            .child(main)
            .child(self.refresh_control(id, RefreshTarget::Status, cx));
        let uptime = get("Server", "uptime_in_seconds")
            .parse::<u64>()
            .unwrap_or_default();
        let total = t
            .info
            .get("Keyspace")
            .into_iter()
            .flat_map(|keys| keys.iter())
            .filter(|(key, _)| {
                key.strip_prefix("db")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
            })
            .filter_map(|(_, value)| {
                value
                    .split(',')
                    .next()?
                    .strip_prefix("keys=")?
                    .parse::<u64>()
                    .ok()
            })
            .sum::<u64>();
        let client_grid=t.client_grid_bounds.clone();
        let parent=cx.entity_id();
        let mut summary = div()
            .id("status-summary-grid")
            .test_support()
            .relative()
            .flex()
            .gap(px(5.))
            .min_w(px(500.))
            .child(canvas(move|bounds,_,cx|{if client_grid.replace(bounds)!=bounds {cx.notify(parent);}},|_,_,_,_|{}).absolute().inset_0());
        for (index, (label, value)) in [
            (self.tr("status.uptime"), self.duration_text(uptime)),
            (
                self.tr("status.connected_clients"),
                t.info
                    .get("Clients")
                    .and_then(|v| v.get("connected_clients"))
                    .cloned()
                    .unwrap_or_else(|| "0".into()),
            ),
            (self.tr("status.total_keys"), total.to_string()),
            (
                self.tr("status.memory_used"),
                super::status_values::format_bytes(
                    super::status_values::info_integer(&t.info, "Memory", "used_memory"),
                    2,
                    " ",
                ),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let (numeric, suffix) = if matches!(index, 0 | 3) {
                value
                    .split_once(' ')
                    .map(|(a, b)| (a.to_string(), Some(b.to_string())))
                    .unwrap_or((value.clone(), None))
            } else {
                (value.clone(), None)
            };
            let mut number = div()
                .id(format!("status-stat-row-{index}"))
                .test_support()
                .mt(px(4.))
                .h(px(36.))
                .flex()
                .items_start()
                .text_size(px(24.))
                .line_height(px(36.))
                .text_color(value_color)
                .font_weight(FontWeight::NORMAL)
                .child(
                    div()
                        .id(format!("status-value-{index}"))
                        .test_support()
                        .role(Role::Label)
                        .aria_label(value.clone())
                        .flex()
                        .gap(px(4.))
                        .child(status_text(
                            numeric,
                            24.,
                            36.,
                            FontWeight::NORMAL,
                            &format!("status-numeric-{index}"),
                            font.clone(),
                            window,
                            cx,
                        ))
                        .when_some(suffix, |v, s| {
                            v.child(status_text(
                                s,
                                24.,
                                36.,
                                FontWeight::NORMAL,
                                &format!("status-unit-{index}"),
                                font.clone(),
                                window,
                                cx,
                            ))
                        }),
                );
            if index == 1 {
                let arrow_width = super::text::script_width("↘", font.clone(), 14., window) + 16.;
                let button_top=status_inline_baseline("0",font.clone(),24.,36.,window)
                    - (7. + status_inline_baseline("↘",font.clone(),14.,14.,window));
                let mut content_style = div().text_size(px(14.)).line_height(px(14.));
                let button=Button::new("show-clients").label("↘").ghost().compact()
                    .w(px(arrow_width)).min_w(px(arrow_width)).h(px(28.))
                    .px(px(8.)).py_0().text_size(px(14.)).line_height(px(14.)).text_color(value_color)
                    .content_style(content_style.style().clone(),gpui_component::Size::Size(px(14.)));
                number=number.child(div().ml(px(4.)).mt(px(button_top)).child(
                    self.render_client_trigger(i,button,arrow_width,window,cx),
                ));
            }
            summary = summary.child(
                div()
                    .id(format!("status-stat-{index}"))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(format!("status-stat-label-{index}"))
                            .test_support()
                            .h(px(21.))
                            .text_size(px(14.))
                            .line_height(px(21.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(muted)
                            .child(status_text(
                                label,
                                14.,
                                21.,
                                FontWeight::NORMAL,
                                &format!("status-label-text-{index}"),
                                font.clone(),
                                window,
                                cx,
                            )),
                    )
                    .child(number),
            );
        }
        let chart = |label: String,
                     color: u32,
                     field: fn(&Sample) -> f64,
                     second: Option<fn(&Sample) -> f64>| {
            let mut series = vec![super::status_chart::Series {
                label,
                color,
                values: t.samples.iter().map(field).collect(),
            }];
            if let Some(field) = second {
                series.push(super::status_chart::Series {
                    label: self.tr("status.act_network_output"),
                    color: 0x36a2eb,
                    values: t.samples.iter().map(field).collect(),
                });
            }
            div()
                .id(format!("status-chart-slot-{color:06x}"))
                .test_support()
                .flex_1()
                .min_w_0()
                .h_full()
                .p(px(10.))
                .child(super::status_chart::StatusChart {
                    id: format!("{color:06x}"),
                    labels: t.samples.iter().map(|s| s.time.clone()).collect(),
                    series,
                    bytes_axis: second.is_some() || color == 0x9966ff,
                    dark,
                })
        };
        let mut charts = div()
            .id("status-scroll")
            .flex_1()
            .min_h_0()
            .relative()
            .flex()
            .flex_col();
        if t.info_details {
            charts = charts.child(self.render_status_info(i, window, cx));
        } else {
            // Naive UI's full-height tab pane extends one pixel beyond its flex parent.
            charts = charts.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom(px(-1.))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .child(chart(self.tr("status.act_cmd"), 0xff6384, |s| s.ops, None))
                            .child(chart(
                                self.tr("status.connected_clients"),
                                0xff9f40,
                                |s| s.clients,
                                None,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .child(chart(
                                self.tr("status.memory_used"),
                                0x9966ff,
                                |s| s.memory,
                                None,
                            ))
                            .child(chart(
                                self.tr("status.act_network_input"),
                                0x4bc0c0,
                                |s| s.input,
                                Some(|s| s.output),
                            )),
                    ),
            );
        }
        let mut tabs = div()
            .relative()
            .flex()
            .gap(px(36.))
            .px(px(20.))
            .h(px(49.))
            .flex_shrink_0()
            .border_b_1()
            .border_color(border);
        for (key, label, details) in [
            ("activity", "活动状态", false),
            ("status-info", "状态信息", true),
        ] {
            let active = t.info_details == details;
            let mut tab_font = font.clone();
            tab_font.weight = if active {
                FontWeight(450.)
            } else {
                FontWeight::NORMAL
            };
            let label = localized(label);
            let width = super::text::script_width(&label, tab_font.clone(), 16., window);
            tabs = tabs.child(
                div()
                    .relative()
                    .h(px(48.))
                    .w(px(width))
                    .flex_shrink_0()
                    .child(
                        self.btn(key, "", "", cx, move |a, _, cx| {
                            if let Some(i) = a.tab_index(id) {
                                a.tabs[i].info_details = details;
                            }
                            cx.notify();
                        })
                        .accessibility_label(label.clone())
                        .child(status_text(
                            label,
                            16.,
                            20.8,
                            tab_font.weight,
                            &format!("status-tab-text-{key}"),
                            font.clone(),
                            window,
                            cx,
                        ))
                        .ghost()
                        .w_full()
                        .h(px(48.))
                        .p_0()
                        .text_size(px(16.))
                        .line_height(px(20.8))
                        .font_weight(tab_font.weight)
                        .text_color(if active {
                            cx.theme().primary
                        } else {
                            title_color
                        }),
                    )
                    .when(active, |v| {
                        v.child(
                            div()
                                .absolute()
                                .left_0()
                                .bottom(px(-1.))
                                .w(px(width.ceil()))
                                .h(px(2.))
                                .bg(cx.theme().primary),
                        )
                    }),
            );
        }
        if t.info_details {
            tabs = tabs.child(div().absolute().right(px(10.)).top(px(8.)).child(self.render_status_info_filter(i, window, cx)));
        }
        div()
            .id("status-root")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .p(px(5.))
            .gap(px(5.))
            .child(
                div()
                    .id("status-summary-card")
                    .test_support()
                    .flex_shrink_0()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(border)
                    .bg(card_color)
                    .child(div().pt(px(19.)).pb(px(20.)).px(px(24.)).child(head))
                    .child(div().px(px(24.)).pb(px(20.)).child(summary)),
            )
            .child(
                div()
                    .id("status-activity-card")
                    .test_support()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(border)
                    .bg(card_color)
                    .overflow_hidden()
                    .child(tabs)
                    .child(charts),
            )
            .into_any_element()
    }
    pub(super) fn save_status_interval(
        &mut self,
        id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let name = self.tabs[i].conn.name.clone();
        let seconds = if self.tabs[i].status_auto {
            self.tabs[i].status_seconds
        } else {
            -1.
        };
        // The frontend profile changes before the original integer RPC runs.
        // A rejected fraction still survives connection close/reopen in memory.
        self.status_refresh_profiles.insert(name.clone(), seconds);
        if seconds.fract() != 0. {
            return;
        }
        let Some(mut connection) = flatten(&self.connections)
            .into_iter()
            .find(|c| c.name == name)
        else {
            return;
        };
        let changed = connection.refresh_interval != seconds as i64;
        self.tabs[i].conn.refresh_interval = seconds as i64;
        if changed {
            connection.refresh_interval = seconds as i64;
            if let Err(error) =
                crate::storage::save_connection(&mut self.connections, Some(&name), connection)
            {
                self.toast(error.to_string(), true);
            } else {
                self.save_connections();
            }
        }
        cx.notify();
    }
    fn duration_text(&self, seconds: u64) -> String {
        let (value, unit) = super::status_values::uptime(seconds);
        format!("{value} {}", self.tr(unit))
    }
    fn render_detail(&self, i: usize, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let t = &self.tabs[i];
        let Some(d) = &t.detail else {
            return div()
                .flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_color(cx.theme().muted_foreground)
                .child(self.tr("interface.nonexist_tab_content"))
                .into_any_element();
        };
        let toolbar = self.render_key_toolbar(i, window, cx);
        let body = if matches!(d.kind, KeyType::String | KeyType::Json) {
            self.render_scalar_value(i, window, cx)
        } else {
            self.render_entries(i, window, cx)
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .child(toolbar)
            .child(div().flex_1().min_h_0().child(body))
            .when(
                matches!(d.kind, KeyType::String | KeyType::Json),
                |element| element.child(div().h(px(5.)).flex_shrink_0()),
            )
            .child(self.render_value_footer(i, window, cx))
            .when(self.entry_panel_fullscreen(), |root| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .child(self.render_entry_panel(window, cx)),
                )
            })
            .into_any_element()
    }
    fn render_slow(&self, i: usize, cx: &Context<Self>) -> AnyElement {
        let t = &self.tabs[i];
        let id = t.id;
        let client_width = *t.column_widths.get("slow-client").unwrap_or(&200.);
        let command_width = t.column_widths.get("slow-command").copied();
        let keyword = text_input(&t.slow_keyword, cx);
        let client = text_input(&t.slow_client, cx);
        let mut logs = t
            .slow
            .iter()
            .filter(|log| {
                (keyword.is_empty() || log.args.contains(&keyword))
                    && (client.is_empty() || log.name == client || log.address == client)
            })
            .collect::<Vec<_>>();
        logs.sort_by_key(|log| log.time);
        if !t.slow_ascending {
            logs.reverse();
        }
        let mut list = div().id("slow-list").overflow_y_scroll().flex_1().min_h_0();
        for (index, log) in logs.into_iter().enumerate() {
            let time = chrono::DateTime::from_timestamp(log.time, 0)
                .map(|d| {
                    d.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            let cost = if log.duration < 1_000_000 {
                format!("{} ms", log.duration / 1000)
            } else {
                format!("{} s", log.duration / 1_000_000)
            };
            let client = match (log.name.is_empty(), log.address.is_empty()) {
                (true, _) => log.address.clone(),
                (_, true) => log.name.clone(),
                _ => format!("{} - {}", log.name, log.address),
            };
            list = list.child(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .py_2()
                    .gap_2()
                    .when(index % 2 == 1, |d| d.bg(cx.theme().muted))
                    .child(div().w(px(180.)).flex_shrink_0().text_center().child(time))
                    .child(
                        div()
                            .w(px(client_width))
                            .flex_shrink_0()
                            .text_center()
                            .truncate()
                            .child(client),
                    )
                    .child(
                        div()
                            .when_some(command_width, |cell, width| {
                                cell.w(px(width)).flex_shrink_0()
                            })
                            .when(command_width.is_none(), |cell| cell.flex_1())
                            .min_w_0()
                            .font_family(
                                self.pref
                                    .cli
                                    .font_family
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| "monospace".into()),
                            )
                            .child(log.args.clone()),
                    )
                    .child(div().w(px(100.)).flex_shrink_0().text_center().child(cost)),
            );
        }
        let weak = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .child(self.tr("slog.limit"))
                    .child(div().w(px(120.)).child(Input::new(&t.slow_limit).small()))
                    .child(self.tr("slog.filter"))
                    .child(
                        div()
                            .w(px(220.))
                            .child(Input::new(&t.slow_keyword).small().cleanable(true)),
                    )
                    .child(self.refresh_control(id, RefreshTarget::Slow, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(cx.theme().muted)
                    .child(
                        self.btn(
                            "slow-sort-time",
                            format!(
                                "{} {}",
                                self.tr("slog.exec_time"),
                                if t.slow_ascending { "▴" } else { "▾" }
                            ),
                            "",
                            cx,
                            move |a, _, cx| {
                                if let Some(i) = a.tab_index(id) {
                                    a.tabs[i].slow_ascending = !a.tabs[i].slow_ascending;
                                    cx.notify();
                                }
                            },
                        )
                        .ghost()
                        .w(px(180.))
                        .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .relative()
                            .w(px(client_width))
                            .flex_shrink_0()
                            .child(
                                gpui_component::popover::Popover::new("slow-client-filter")
                                    .trigger(
                                        Button::new("slow-client-header")
                                            .label(self.tr("slog.client"))
                                            .ghost()
                                            .w(px(client_width)),
                                    )
                                    .content(move |_, _, cx| {
                                        weak.update(cx, |a, _| {
                                            let Some(i) = a.tab_index(id) else {
                                                return div().into_any_element();
                                            };
                                            div()
                                                .w(px(200.))
                                                .child(
                                                    Input::new(&a.tabs[i].slow_client)
                                                        .small()
                                                        .cleanable(true),
                                                )
                                                .into_any_element()
                                        })
                                        .unwrap_or_else(|_| div().into_any_element())
                                    }),
                            )
                            .child(self.resize_handle(id, "slow-client", client_width, cx)),
                    )
                    .child(
                        div()
                            .when_some(command_width, |cell, width| {
                                cell.w(px(width)).flex_shrink_0()
                            })
                            .when(command_width.is_none(), |cell| cell.flex_1())
                            .child(
                                self.column_header(
                                    id,
                                    "slow-command",
                                    self.tr("slog.cmd"),
                                    command_width.unwrap_or(
                                        (self.viewport_width
                                            - self.pref.behavior.aside_width
                                            - 350.
                                            - client_width)
                                            .max(100.),
                                    ),
                                    cx,
                                ),
                            ),
                    )
                    .child(
                        div()
                            .w(px(100.))
                            .text_center()
                            .child(self.tr("slog.cost_time")),
                    ),
            )
            .child(list)
            .into_any_element()
    }
    fn render_stream(&self, i: usize, pubsub: bool, cx: &Context<Self>) -> AnyElement {
        let t = &self.tabs[i];
        let id = t.id;
        let stream = if pubsub { &t.pubsub } else { &t.monitor };
        let running = stream.as_ref().is_some_and(|s| s.running);
        let auto_last = stream.as_ref().is_none_or(|s| s.auto_last);
        let mut controls = div().flex().items_center().gap_2().child(self.btn(
            "stream-start",
            self.tr(if pubsub {
                if running {
                    "pubsub.unsubscribe"
                } else {
                    "pubsub.subscribe"
                }
            } else if running {
                "monitor.stop"
            } else {
                "monitor.start"
            }),
            if running {
                "pause"
            } else if pubsub {
                "subscribe"
            } else {
                "play"
            },
            cx,
            move |a, w, cx| a.stream_toggle(id, pubsub, w, cx),
        ));
        if !pubsub {
            controls = controls
                .child(self.icon_btn(
                    "stream-copy",
                    "copy",
                    self.tr("monitor.copy_log"),
                    cx,
                    move |a, _, cx| {
                        if let Some(i) = a.tab_index(id)
                            && let Some(s) = &a.tabs[i].monitor
                        {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                s.lines
                                    .iter()
                                    .map(StreamLine::export)
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            ));
                        }
                    },
                ))
                .child(self.icon_btn(
                    "stream-export",
                    "export",
                    self.tr("monitor.save_log"),
                    cx,
                    move |a, w, cx| a.export_stream(id, w, cx),
                ));
        }
        controls = controls
            .child(
                self.icon_btn(
                    "stream-last",
                    "bottom",
                    self.tr("monitor.always_show_last"),
                    cx,
                    move |a, _, cx| {
                        if let Some(i) = a.tab_index(id) {
                            let state = if pubsub {
                                &mut a.tabs[i].pubsub
                            } else {
                                &mut a.tabs[i].monitor
                            };
                            if let Some(s) = state {
                                s.auto_last = !s.auto_last;
                                cx.notify();
                            }
                        }
                    },
                )
                .selected(auto_last),
            )
            .child(div().flex_1())
            .child(self.icon_btn(
                "stream-clear",
                "delete",
                self.tr(if pubsub {
                    "pubsub.clear"
                } else {
                    "monitor.clean_log"
                }),
                cx,
                move |a, _, cx| {
                    if let Some(i) = a.tab_index(id) {
                        let state = if pubsub {
                            &mut a.tabs[i].pubsub
                        } else {
                            &mut a.tabs[i].monitor
                        };
                        if let Some(s) = state {
                            s.lines.clear();
                            s.list.reset(0);
                            cx.notify();
                        }
                    }
                },
            ));
        let mut content = div()
            .flex()
            .flex_col()
            .size_full()
            .p_3()
            .gap_2()
            .child(controls);
        if !pubsub {
            content = content
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().muted_foreground)
                        .child(self.tr("monitor.warning")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.tr("monitor.search"))
                        .child(Input::new(&t.stream_filter).cleanable(true)),
                );
        }
        if pubsub {
            content = content.child(
                div()
                    .flex()
                    .bg(cx.theme().muted)
                    .py_2()
                    .child(
                        div()
                            .w(px(200.))
                            .text_center()
                            .child(self.tr("pubsub.time")),
                    )
                    .child(
                        div()
                            .w(px(200.))
                            .text_center()
                            .child(self.tr("pubsub.channel")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_center()
                            .child(self.tr("pubsub.message")),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .text_center()
                            .child(self.tr("interface.action")),
                    ),
            );
        }
        let filter = if pubsub {
            String::new()
        } else {
            text_input(&t.stream_filter, cx)
        };
        if let Some(s) = stream {
            let rows = s
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| filter.is_empty() || line.text.contains(&filter))
                .map(|(n, _)| n)
                .collect::<Vec<_>>();
            let count = rows.len();
            if s.list.item_count() != count {
                s.list.reset(count);
                if s.auto_last && count > 0 {
                    s.list.scroll_to_reveal_item(count - 1);
                }
            }
            content = content.child(
                list(
                    s.list.clone(),
                    cx.processor(move |a, index, _, cx| {
                        let Some(i) = a.tab_index(id) else {
                            return div().into_any_element();
                        };
                        let stream = if pubsub {
                            &a.tabs[i].pubsub
                        } else {
                            &a.tabs[i].monitor
                        };
                        let Some(line) = stream
                            .as_ref()
                            .and_then(|s| rows.get(index).and_then(|n| s.lines.get(*n)))
                        else {
                            return div().into_any_element();
                        };
                        let message = line.text.clone();
                        if pubsub {
                            div()
                                .flex()
                                .items_start()
                                .py_2()
                                .border_b_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .w(px(200.))
                                        .flex_shrink_0()
                                        .text_center()
                                        .child(line.time.clone()),
                                )
                                .child(
                                    div()
                                        .w(px(200.))
                                        .flex_shrink_0()
                                        .text_center()
                                        .child(line.channel.clone().unwrap_or_default()),
                                )
                                .child(div().flex_1().min_w_0().child(line.text.clone()))
                                .child(a.icon_btn(
                                    format!("message-copy-{index}"),
                                    "copy",
                                    a.tr("interface.copy_value"),
                                    cx,
                                    move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            message.clone(),
                                        ))
                                    },
                                ))
                                .into_any_element()
                        } else {
                            div()
                                .min_h(px(25.))
                                .font_family("Menlo")
                                .text_size(px(12.))
                                .child(format!("> {}", line.text))
                                .into_any_element()
                        }
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        } else {
            content = content.child(div().flex_1());
        }
        if pubsub {
            let weak = cx.weak_entity();
            let history = t.pub_history.clone();
            let channel = div()
                .w(px(200.))
                .child(Input::new(&t.pub_channel))
                .context_menu(move |mut menu, _, _| {
                    for channel in &history {
                        let channel = channel.clone();
                        menu = menu.item(menu_action(
                            channel.clone(),
                            weak.clone(),
                            move |a, w, cx| {
                                if let Some(i) = a.tab_index(id) {
                                    set_input(&a.tabs[i].pub_channel, channel.clone(), w, cx);
                                }
                            },
                        ));
                    }
                    menu
                });
            content = content
                .child(
                    self.tr("pubsub.receive_message").replace(
                        "{total}",
                        &stream
                            .as_ref()
                            .map(|s| s.lines.len())
                            .unwrap_or_default()
                            .to_string(),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(channel)
                        .child(Input::new(&t.pub_message).when_some(
                            t.pub_received.as_ref(),
                            |input, (n, _)| {
                                input.suffix(div().text_color(rgb(0x18a058)).child(n.to_string()))
                            },
                        ))
                        .child(
                            self.btn(
                                "publish",
                                self.tr("pubsub.publish"),
                                "publish",
                                cx,
                                move |a, w, cx| a.publish(id, w, cx),
                            )
                            .disabled(text_input(&t.pub_channel, cx).is_empty()),
                        ),
                );
        }
        content.into_any_element()
    }
    fn render_log(&self, cx: &Context<Self>) -> AnyElement {
        let filter = text_input(&self.log_filter, cx).to_lowercase();
        let logs = self
            .service
            .history
            .try_lock()
            .map(|history| {
                history
                    .iter()
                    .rev()
                    .filter(|l| {
                        filter.is_empty()
                            || l.command.to_lowercase().contains(&filter)
                            || l.server.to_lowercase().contains(&filter)
                    })
                    .skip(self.log_page * 100)
                    .take(100)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut list = div()
            .id("operation-log-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for log in logs {
            list = list.child(
                div()
                    .flex()
                    .gap_2()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().w(px(180.)).child(log.time))
                    .child(
                        div()
                            .w(px(120.))
                            .child(format!("{} / db{}", log.server, log.db)),
                    )
                    .child(div().w(px(90.)).child(format!("{:.2} ms", log.elapsed_ms)))
                    .child(div().flex_1().child(log.command))
                    .when(!log.error.is_empty(), |d| {
                        d.text_color(cx.theme().danger).child(log.error)
                    }),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_2()
                    .child(Input::new(&self.log_filter).cleanable(true))
                    .child(self.btn("clear-logs", "清空", "clear", cx, |a, w, cx| {
                        a.job(
                            w,
                            cx,
                            |s| {
                                s.history.lock().clear();
                                Ok(())
                            },
                            |_, _, _, _| {},
                        )
                    })),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .p_2()
                    .gap_2()
                    .child(self.btn("log-prev", "‹", "", cx, |a, _, cx| {
                        a.log_page = a.log_page.saturating_sub(1);
                        cx.notify();
                    }))
                    .child((self.log_page + 1).to_string())
                    .child(self.btn("log-next", "›", "", cx, |a, _, cx| {
                        a.log_page += 1;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    pub(super) fn change_format(
        &mut self,
        id: u64,
        decode: Option<String>,
        format: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self.tab_index(id) {
            let t = &mut self.tabs[i];
            if let Some(v) = decode {
                t.decode = v;
                if format.is_none() {
                    t.format = "Auto".into();
                }
            }
            if let Some(v) = format {
                t.format = v;
            }
            t.entry_list
                .splice(0..t.entry_list.item_count(), t.entry_list.item_count());
            self.prepare_values(id, window, cx);
            cx.notify();
        }
    }
    pub(super) fn remove_entry(
        &mut self,
        id: u64,
        row: Entry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let t = &self.tabs[i];
        let Some(d) = &t.detail else {
            return;
        };
        let c = t.conn.clone();
        let db = t.db;
        let key = d.key.clone();
        let requested_key = key.clone();
        let kind = d.kind.clone();
        let removed_label = match kind {
            KeyType::List => format!(
                "#{}",
                display_bytes(&row.id).parse::<usize>().unwrap_or(0) + 1
            ),
            KeyType::Hash | KeyType::Stream => display_bytes(&row.id),
            _ => display_bytes(&row.value),
        };
        let message = self
            .tr("dialogue.delete.success")
            .replace("{key}", &removed_label);
        self.job(
            window,
            cx,
            move |s| s.edit_entry(&c, db, &key, &kind, &row, &row, true),
            move |a, _, w, cx| {
                a.toast(message, false);
                if let Some(i) = a.tab_index(id)
                    && a.tabs[i].db == db
                    && a.tabs[i].current_key.as_ref() == Some(&requested_key)
                {
                    a.reload_detail(id, true, w, cx);
                }
            },
        );
    }
}
fn status_inline_baseline(text:&str,font:Font,size:f32,height:f32,window:&Window)->f32 {
    let run=TextRun{len:text.len(),font,color:Hsla::white(),background_color:None,underline:None,strikethrough:None};
    let line=window.text_system().shape_line(text.to_string().into(),px(size),&[run],None);
    // Same CSS integer font-box/leading calculation as CssLabel; the inline
    // client's baseline includes the centered 14px line plus 7px top padding.
    ((height.floor()+line.ascent.as_f32().round()-line.descent.as_f32().round())/2.).floor()
}
pub(super) fn status_text(
    text: String,
    size: f32,
    height: f32,
    weight: FontWeight,
    id: &str,
    mut font: Font,
    window: &Window,
    _cx: &App,
) -> AnyElement {
    font.weight = weight;
    let width = super::text::script_width(&text, font.clone(), size, window);
    let label = div()
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(size))
        .line_height(px(height))
        .font(font);
    #[cfg(target_os = "macos")]
    let label = label.child(super::text::CssLabel {
        text,
        text_id: id.to_string(),
    });
    #[cfg(not(target_os = "macos"))]
    let label = label.child(text);
    label.into_any_element()
}
fn status_tag(name: &str, text: String, dark: bool, mut font: Font, window: &Window, cx: &App) -> AnyElement {
    font.weight = FontWeight::MEDIUM;
    let width = super::text::script_width(&text, font.clone(), 12., window) + 14.;
    div()
        .id(format!("status-tag-{name}"))
        .test_support()
        .relative()
        .h(px(22.))
        .w(px(width))
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(7.))
        .text_size(px(12.))
        .line_height(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .rounded(px(4.))
        .bg(if dark {
            Hsla::transparent_black()
        } else {
            cx.theme().primary.opacity(0.1)
        })
        .text_color(cx.theme().primary)
        .child(status_text(
            text,
            12.,
            12.,
            FontWeight::MEDIUM,
            &format!("status-tag-text-{name}"),
            font.clone(),
            window,
            cx,
        ))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().primary.opacity(0.3)),
        )
        .into_any_element()
}
pub(super) fn type_mark(kind: &KeyType, short: bool, small: bool, inverse: bool) -> AnyElement {
    let (c, b) = kind.colors();
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(if small { 22. } else { 32. }))
        .min_w(px(if short {
            22.
        } else if small {
            0.
        } else {
            65.
        }))
        .px(px(if short {
            0.
        } else if small {
            5.
        } else {
            12.
        }))
        .rounded(px(4.))
        .border_1()
        .border_color(rgba(0xddddddcc))
        .bg(rgb(if inverse { c } else { b }))
        .text_color(rgb(if inverse { b } else { c }))
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .child(if short {
            kind.short().to_string()
        } else {
            kind.name().to_string()
        })
        .into_any_element()
}
#[derive(Clone)]
pub(super) struct TreeRow {
    pub key: Option<Vec<u8>>,
    pub label: String,
    pub path: String,
    pub depth: usize,
    pub kind: KeyType,
}
#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    keys: Vec<KeyEntry>,
    count: usize,
}
pub(super) fn key_separator(tab: &Tab) -> &str {
    if tab.conn.key_separator.is_empty() {
        ":"
    } else {
        &tab.conn.key_separator
    }
}
pub(super) fn tree_rows(tab: &Tab, filter: &str, all: bool) -> Vec<TreeRow> {
    let base_depth = usize::from(tab.check_mode);
    let mut rows = Vec::new();
    if tab.check_mode {
        rows.push(TreeRow {
            key: None,
            label: format!("db{}", tab.db),
            path: String::new(),
            depth: 0,
            kind: KeyType::None,
        });
        if !tab.key_root_expanded && !all {
            return rows;
        }
    }
    let sep = key_separator(tab);
    if tab.flat {
        rows.extend(
            tab.keys
                .iter()
                .filter(|k| display_bytes(&k.key).contains(filter))
                .map(|k| TreeRow {
                    key: Some(k.key.clone()),
                    label: display_bytes(&k.key),
                    path: String::new(),
                    depth: base_depth,
                    kind: k.kind.clone(),
                }),
        );
        return rows;
    }
    let mut root = Node::default();
    for k in &tab.keys {
        let label = display_bytes(&k.key);
        let parts = label.split(sep).collect::<Vec<_>>();
        let mut n = &mut root;
        n.count += 1;
        for p in &parts[..parts.len().saturating_sub(1)] {
            n = n.children.entry(p.to_string()).or_default();
            n.count += 1;
        }
        n.keys.push(k.clone());
    }
    fn matches(n: &Node, filter: &str) -> bool {
        filter.is_empty()
            || n.keys
                .iter()
                .any(|k| display_bytes(&k.key).contains(filter))
            || n.children.values().any(|n| matches(n, filter))
    }
    fn visit(
        n: &Node,
        path: String,
        depth: usize,
        tab: &Tab,
        filter: &str,
        all: bool,
        out: &mut Vec<TreeRow>,
    ) {
        let mut folders = n.children.iter().collect::<Vec<_>>();
        folders.sort_by(|(a, _), (b, _)| natural_compare(a, b));
        for (label, node) in folders {
            if !matches(node, filter) {
                continue;
            }
            let path = format!("{path}{label}{}", key_separator(tab));
            out.push(TreeRow {
                key: None,
                label: format!("{label} ({})", node.count),
                path: path.clone(),
                depth,
                kind: KeyType::None,
            });
            if all || !filter.is_empty() || tab.expanded.contains(&path) {
                visit(node, path, depth + 1, tab, filter, all, out);
            }
        }
        for k in &n.keys {
            if !display_bytes(&k.key).contains(filter) {
                continue;
            }
            out.push(TreeRow {
                key: Some(k.key.clone()),
                label: display_bytes(&k.key)
                    .rsplit(key_separator(tab))
                    .next()
                    .unwrap_or_default()
                    .into(),
                path: path.clone(),
                depth,
                kind: k.kind.clone(),
            });
        }
    }
    visit(
        &root,
        String::new(),
        base_depth,
        tab,
        filter,
        all,
        &mut rows,
    );
    rows
}
