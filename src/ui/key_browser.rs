use super::dialogs::DialogKind;
use super::views::{TreeRow, menu_action, tree_rows, type_mark};
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Selection {
    Folder(String),
    Key(Vec<u8>),
}
impl TreeRow {
    fn selection(&self) -> Selection {
        self.key
            .clone()
            .map(Selection::Key)
            .unwrap_or_else(|| Selection::Folder(self.path.clone()))
    }
}

impl RdmApp {
    pub(super) fn update_key_filter(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.tabs[i];
        let value = text_input(&tab.filter, cx);
        tab.key_filter_epoch += 1;
        let epoch = tab.key_filter_epoch;
        let leading = tab
            .key_filter_changed
            .is_none_or(|time| time.elapsed() >= Duration::from_secs(1));
        tab.key_filter_changed = Some(Instant::now());
        if leading {
            tab.key_filter = value.clone();
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update_in(cx, |app, _, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].key_filter_epoch == epoch
                {
                    app.tabs[i].key_filter = value;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn search_keys(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let value = text_input(&self.tabs[i].filter, cx).trim().to_string();
        set_input(&self.tabs[i].filter, value.clone(), window, cx);
        if value.is_empty() {
            return;
        }
        self.tabs[i].key_match = value;
        self.tabs[i].key_filter.clear();
        self.tabs[i].key_filter_epoch += 1;
        set_input(&self.tabs[i].filter, "", window, cx);
        self.scan_keys(id, true, false, window, cx);
    }
    fn clear_key_search(
        &mut self,
        id: u64,
        edit_match: bool,
        clear_filter: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        self.tabs[i].match_tooltips.cancel(super::match_tooltip::MatchKind::Key);
        let tab = &mut self.tabs[i];
        let had_match = !tab.key_match.is_empty();
        if edit_match {
            set_input(&tab.filter, tab.key_match.clone(), window, cx);
        } else if clear_filter {
            set_input(&tab.filter, "", window, cx);
        }
        tab.key_match.clear();
        tab.key_filter = text_input(&tab.filter, cx);
        tab.key_filter_epoch += 1;
        if had_match {
            self.scan_keys(id, true, false, window, cx);
        }
        cx.notify();
    }
    fn browser_selection(
        &mut self,
        id: u64,
        selection: Selection,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(i) = self.tab_index(id) {
            self.tabs[i].key_selection = Some(selection.clone());
            self.tabs[i].key_focus.focus(window, cx);
            if activate && let Selection::Key(key) = selection {
                self.select_key(id, key, window, cx);
            }
            cx.notify();
        }
    }
    fn toggle_key_folder(&mut self, id: u64, path: &str, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.tabs[i];
        if path.is_empty() {
            tab.key_root_expanded = !tab.key_root_expanded;
        } else {
            let expand = !tab.expanded.contains(path);
            let nodes = tree_rows(tab, "", true);
            let mut path = path.to_string();
            loop {
                if expand {
                    tab.expanded.insert(path.clone());
                } else {
                    tab.expanded.remove(&path);
                }
                let Some(index) = nodes.iter().position(|n| n.key.is_none() && n.path == path)
                else {
                    break;
                };
                let depth = nodes[index].depth;
                let children = nodes[index + 1..]
                    .iter()
                    .take_while(|n| n.depth > depth)
                    .filter(|n| n.depth == depth + 1)
                    .collect::<Vec<_>>();
                if children.len() != 1 || children[0].key.is_some() {
                    break;
                }
                path = children[0].path.clone();
            }
        }
        cx.notify();
    }
    fn check_key_node(&mut self, id: u64, selection: &Selection, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &mut self.tabs[i];
        let keys = ordered_keys_for_selection(tab, selection, &tab.key_filter);
        let uncheck = keys.iter().all(|key| tab.selected.contains(key));
        if uncheck {
            let removed = keys.into_iter().collect::<HashSet<_>>();
            tab.selected.retain(|key| !removed.contains(key));
        } else {
            tab.selected.extend(keys);
        }
        cx.notify();
    }
    fn new_browser_key(
        &mut self,
        id: u64,
        cx_selection: Option<Selection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = cx_selection.or_else(|| {
            self.tab_index(id)
                .and_then(|i| self.tabs[i].key_selection.clone())
        });
        self.open_dialog(DialogKind::NewKey, window, cx);
        if let Some(Selection::Folder(mut prefix)) = selection {
            if let Some(i) = self.tab_index(id) {
                let separator = &self.tabs[i].conn.key_separator;
                if !prefix.is_empty() && !prefix.ends_with(separator) {
                    prefix.push_str(separator);
                }
            }
            self.set_dialog_field("key", prefix, window, cx);
        }
    }
    fn delete_browser_selection(
        &mut self,
        id: u64,
        selection: Option<Selection>,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let selection = selection.or_else(|| self.tabs[i].key_selection.clone());
        if !checked && let Some(Selection::Key(key)) = &selection {
            self.open_dialog(DialogKind::DeleteKey(key.clone()), window, cx);
            return;
        }
        let keys = if checked {
            self.tabs[i].selected.iter().cloned().collect::<Vec<_>>()
        } else {
            match &selection {
                Some(Selection::Key(key)) => vec![key.clone()],
                _ => Vec::new(),
            }
        };
        self.open_dialog(DialogKind::DeleteKeys, window, cx);
        if checked || matches!(selection, Some(Selection::Key(_))) {
            self.set_dialog_keys(keys);
        } else {
            let pattern = match selection {
                Some(Selection::Folder(prefix)) => format!("{prefix}*"),
                _ => "*".into(),
            };
            self.set_dialog_field("pattern", pattern, window, cx);
        }
    }
    fn reload_browser_selection(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        if let Some(Selection::Key(key)) = self.tabs[i].key_selection.clone() {
            if self.tabs[i].current_key.as_ref() == Some(&key) {
                self.reload_detail(id, true, window, cx);
            } else {
                self.select_key(id, key, window, cx);
            }
        }
    }
    fn browser_key(
        &mut self,
        id: u64,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else {
            return;
        };
        let tab = &self.tabs[i];
        if tab.keys_loading {
            return;
        }
        let nodes = tree_rows(tab, &tab.key_filter, false);
        let Some(selected) = &tab.key_selection else {
            return;
        };
        let Some(index) = nodes.iter().position(|row| &row.selection() == selected) else {
            return;
        };
        let row = &nodes[index];
        let key = event.keystroke.key.as_str();
        let ctrl = if cfg!(target_os = "macos") {
            event.keystroke.modifiers.platform
        } else {
            event.keystroke.modifiers.control
        };
        let expanded = row.key.is_none()
            && if row.path.is_empty() {
                tab.key_root_expanded
            } else {
                tab.expanded.contains(&row.path) || !tab.key_filter.is_empty()
            };
        let mut next = None;
        match key {
            "up" if index > 0 => next = Some(index - 1),
            "down" if index + 1 < nodes.len() => next = Some(index + 1),
            "left" => {
                if expanded {
                    self.toggle_key_folder(id, &row.path, cx);
                } else if let Some(parent) =
                    nodes[..index].iter().rposition(|n| n.depth < row.depth)
                {
                    next = Some(parent);
                }
            }
            "right" => {
                if row.key.is_none() && !expanded {
                    self.toggle_key_folder(id, &row.path, cx);
                } else if index + 1 < nodes.len() {
                    next = Some(index + 1);
                }
            }
            "c" if ctrl => {
                if let Some(key) = &row.key {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        String::from_utf8_lossy(key).into_owned(),
                    ));
                    self.toast(self.tr("interface.copy_succ"), false);
                }
            }
            "delete" => self.delete_browser_selection(id, None, false, window, cx),
            "f5" => self.reload_browser_selection(id, window, cx),
            "r" if ctrl => self.reload_browser_selection(id, window, cx),
            "up" | "down" => {}
            _ => return,
        }
        if let Some(next) = next {
            self.browser_selection(id, nodes[next].selection(), true, window, cx);
            self.tabs[i]
                .key_scroll
                .scroll_to_item(next, ScrollStrategy::Nearest);
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
    fn browser_tool_button(
        &self,
        id: &'static str,
        asset: &str,
        tip: String,
        size: f32,
        icon_size: f32,
        bordered: bool,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        let button = self
            .connection_icon_button(id, asset, tip, size, cx, handler)
            .icon(icon(asset).size(px(icon_size)))
            .content_style(
                StyleRefinement::default(),
                gpui_component::Size::Size(px(icon_size)),
            )
            .rounded(px(3.));
        if bordered {
            button
                .with_variant(ButtonVariant::Default)
                .outline()
                .bg(rgba(0x00000000))
                .w(px(34.))
                .h(px(28.))
        } else {
            button.with_size(gpui_component::Size::Size(px(24.)))
        }
    }
    pub(super) fn render_browser(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let Some(i) = self.index() else {
            return div().into_any_element();
        };
        let tab = &self.tabs[i];
        let id = tab.id;
        let (selector, selector_width) = self.render_type_selector(id, window, cx);
        let exact = self
            .browser_tool_button(
                "exact-match",
                "spellcheck",
                self.tr("dialogue.filter.exact_match_tip"),
                22.,
                14.,
                false,
                cx,
                move |app, w, cx| {
                    if let Some(i) = app.tab_index(id) {
                        app.tabs[i].exact = !app.tabs[i].exact;
                        if !app.tabs[i].key_match.is_empty() {
                            app.scan_keys(id, true, false, w, cx);
                        }
                        cx.notify();
                    }
                },
            )
            .w(px(24.))
            .h(px(22.))
            // The original tag's inline 14px SVG starts 3px below its top.
            .content_style(
                StyleRefinement::default().relative().top(px(-1.)),
                gpui_component::Size::Size(px(14.)),
            )
            .rounded(px(3.))
            .bg::<Hsla>(if tab.exact {
                rgb(0xd33a31).into()
            } else {
                rgba(0x00000000).into()
            })
            .text_color(if tab.exact {
                Hsla::from(rgb(0xffffff))
            } else if self.dark(cx) {
                Hsla::from(rgba(0xffffffd1))
            } else {
                Hsla::from(rgb(0x333639))
            });
        // Match the source's 3px padding, 4px gap, 16px clear and 24px exact tag.
        // The real input border consumes 1px; reserve the remaining 46px in text.
        let suffix = div()
            .id("key-filter-suffix")
            .test_support()
            .flex()
            .items_center()
            .w(px(40.))
            .min_w(px(40.))
            .flex_shrink_0()
            .child(
                div()
                    .id("key-filter-clear-slot")
                    .test_support()
                    .size(px(16.))
                    .flex_shrink_0()
                    .when(!text_input(&tab.filter, cx).is_empty(), |slot| {
                        slot.child(self.search_clear_button(
                            "clear-key-filter",
                            "key-filter-input",
                            tab.filter.focus_handle(cx).is_focused(window),
                            cx,
                            move |app, w, cx| app.clear_key_search(id, false, true, w, cx),
                        ))
                    }),
            )
            .child(exact)
            .absolute()
            .right(px(3.))
            .top(px(3.))
            .h(px(22.));
        let filter = self
            .search_input(&tab.filter, window, cx)
            .w_full()
            .min_w_0()
            .pl(px(2.))
            .pr(px(46.))
            .rounded(px(0.));
        let match_label = tab.key_match.replace(['\n', '\r'], " ");
        let match_width = if match_label.is_empty() {
            0.
        } else {
            let run = TextRun {
                len: match_label.len(),
                font: self.preference_font("general", ".SystemUIFont", cx),
                color: self.connection_node_color(cx),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            (window
                .text_system()
                .shape_line(match_label.clone().into(), px(12.), &[run], None)
                .width
                .as_f32()
                + 30.)
                // The original prefix is at most half the wrapper's content
                // width, excluding its 3px padding on each side.
                .min(((self.pref.behavior.aside_width - 126. - selector_width) * 0.5).max(31.))
        };
        let filter = if tab.key_match.is_empty() {
            filter
        } else {
            filter.prefix(self.search_match_tag(
                "key-search-match",
                "clear-key-match",
                match_label,
                match_width,
                tab.filter.focus_handle(cx),
                super::match_tooltip::MatchTarget { tab_id: id, kind: super::match_tooltip::MatchKind::Key, left: 61. + selector_width, width: match_width },
                window,
                cx,
                move |app, w, cx| app.clear_key_search(id, true, false, w, cx),
                move |app, w, cx| app.clear_key_search(id, false, false, w, cx),
            ))
        };
        let filter = Styled::h(filter, px(28.));
        let header = div()
            .id("key-browser-header")
            .test_support()
            .h(px(38.171875))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(8.))
            .py(px(3.))
            .gap(px(3.))
            .child(
                div()
                    .id("key-filter-group")
                    .test_support()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(selector)
                    .child(
                        div()
                            .id("key-filter-input-wrap")
                            .test_support()
                            .group("key-filter-input")
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .ml(px(-1.))
                            .h(px(28.))
                            .children(self.search_input_ring(&tab.filter, window, cx))
                            .child(filter)
                            .child(suffix),
                    )
                    .child(
                        self.browser_tool_button(
                            "filter-keys",
                            "search-thick",
                            self.tr("interface.filter_key"),
                            28.,
                            16.,
                            true,
                            cx,
                            move |app, w, cx| app.search_keys(id, w, cx),
                        )
                        .ml(px(-1.))
                        .rounded_l(px(0.))
                        .disabled(
                            !tab.key_match.is_empty()
                                && text_input(&tab.filter, cx).trim().is_empty(),
                        )
                        .opacity(self.disabled_button_opacity(
                            !tab.key_match.is_empty()
                                && text_input(&tab.filter, cx).trim().is_empty(),
                            cx,
                        )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .child(
                        self.browser_tool_button(
                            "refresh-keys",
                            "refresh-thick",
                            self.tr("interface.reload"),
                            28.,
                            18.,
                            true,
                            cx,
                            move |app, w, cx| app.scan_keys(id, true, false, w, cx),
                        )
                        .rounded_r(px(0.)),
                    )
                    .child(
                        self.browser_tool_button(
                            "new-key",
                            "plus-thick",
                            self.tr("interface.new_key"),
                            28.,
                            18.,
                            true,
                            cx,
                            move |app, w, cx| app.new_browser_key(id, None, w, cx),
                        )
                        .rounded_l(px(0.)),
                    ),
            );
        let nodes = Arc::new(tree_rows(tab, &tab.key_filter, false));
        let render_nodes = nodes.clone();
        let list = uniform_list(
            "key-tree",
            nodes.len(),
            cx.processor(move |app, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|ix| app.browser_row(id, ix, &render_nodes[ix], cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&tab.key_scroll)
        .flex_1()
        .min_h_0();
        let progress = if !tab.key_match.is_empty() || tab.total == 0 {
            1.
        } else {
            (tab.keys.len() as f32 / tab.total as f32).min(1.)
        };
        div()
            .id("browser-sidebar")
            .test_support()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .min_w_0()
            .bg(self.sidebar_color(cx))
            .border_r_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(
                div().h(px(2.)).flex_shrink_0().child(
                    div()
                        .h_full()
                        .w(relative(progress))
                        .when(progress < 1., |bar| bar.bg(rgb(0xd33a31))),
                ),
            )
            .child(
                div()
                    .id("key-browser-tree")
                    .test_support()
                    .track_focus(&tab.key_focus)
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .text_size(px(self.general_font_size()))
                    .text_color(self.connection_node_color(cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _, w, cx| {
                            if let Some(i) = app.tab_index(id) {
                                app.tabs[i].key_focus.focus(w, cx);
                            }
                        }),
                    )
                    .on_key_down(
                        cx.listener(move |app, event, w, cx| app.browser_key(id, event, w, cx)),
                    )
                    .child(list)
                    .when(tab.keys_loading, |tree| {
                        tree.child(
                            div()
                                .id("key-browser-loading")
                                .test_support()
                                .role(Role::Label)
                                .aria_label(localized("加载中"))
                                .absolute()
                                .inset_0()
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                                .bg(self.sidebar_color(cx).opacity(0.85))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(gpui_component::spinner::Spinner::new()
                                    .with_size(gpui_component::Size::Size(px(24.)))
                                    .color(rgb(0xd33a31).into())),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(37.))
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(self.browser_footer(id, window, cx)),
            )
            .into_any_element()
    }
    fn browser_row(&self, id: u64, index: usize, row: &TreeRow, cx: &Context<Self>) -> AnyElement {
        let tab = &self.tabs[self.tab_index(id).unwrap()];
        let selection = row.selection();
        let selected = tab.key_selection.as_ref() == Some(&selection);
        let folder = row.key.is_none();
        let database = folder && row.path.is_empty();
        let expanded = folder
            && if database {
                tab.key_root_expanded
            } else {
                tab.expanded.contains(&row.path) || !tab.key_filter.is_empty()
            };
        let selection_click = selection.clone();
        let selection_right = selection.clone();
        let path = row.path.clone();
        let weak = cx.weak_entity();
        let menu_selection = selection.clone();
        let mut content = div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .pl(px(4.))
            .pr(px(6.))
            .h_full();
        if tab.check_mode {
            let keys = keys_for_selection(tab, &selection, &tab.key_filter);
            let count = keys.iter().filter(|k| tab.selected.contains(*k)).count();
            let checked = !keys.is_empty() && count == keys.len();
            let partial = count > 0 && !checked;
            let check_selection = selection.clone();
            let check_weak = cx.weak_entity();
            let mut check = gpui_base::Checkbox::new(("key-check", index))
                .checked(checked)
                .indeterminate(partial)
                .accessibility_label(row.label.clone())
                .w(px(16.))
                .h(px(16.))
                .mr(px(8.))
                .flex_shrink_0()
                .rounded(px(2.))
                .border_1()
                .border_color(if checked || partial {
                    rgb(0xd33a31)
                } else if self.dark(cx) {
                    rgb(0x606060)
                } else {
                    rgb(0xe0e0e6)
                })
                .bg(if checked || partial {
                    rgb(0xd33a31).into()
                } else if self.dark(cx) {
                    Hsla::transparent_black()
                } else {
                    rgb(0xffffff).into()
                })
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0xffffff))
                .on_change(move |_, _, _, cx| {
                    let _ = check_weak
                        .update(cx, |app, cx| app.check_key_node(id, &check_selection, cx));
                    cx.stop_propagation();
                });
            if checked {
                check = check.child(icon("key-checkbox-check").size(px(14.)));
            } else if partial {
                check = check.child(div().w(px(8.)).h(px(2.)).bg(rgb(0xffffff)));
            }
            content = content.child(check);
        }
        content = content
            .child(
                div()
                    .id(("key-prefix", index))
                    .test_support()
                    .w(px(if folder { 20. } else { 22. }))
                    .h(px(if folder { 20. } else { 22. }))
                    .mr(px(8.))
                    .flex_shrink_0()
                    .child(if folder {
                        if database {
                            icon("database").size(px(20.)).into_any_element()
                        } else {
                            img(ImageSource::Resource(Resource::Embedded(
                                format!(
                                    "icons/layer-{}.svg",
                                    if self.dark(cx) { "dark" } else { "light" }
                                )
                                .into(),
                            )))
                            .size(px(20.))
                            .into_any_element()
                        }
                    } else {
                        type_mark(&row.kind, true, true, selected)
                    }),
            )
            .child(
                div()
                    .id(("key-label", index))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h(px(self.general_font_size() * 1.5 + 1.))
                    .line_height(px(self.general_font_size() * 1.5))
                    .border_b_1()
                    .border_color(Hsla::transparent_black())
                    .truncate()
                    .when(self.general_font_size() != 0., |label| {
                        label.child(row.label.clone())
                    }),
            );
        if selected && !tab.check_mode && !database {
            let mut suffix = div()
                .flex()
                .items_center()
                .gap(px(3.))
                .mr(px(5.))
                .h(px(18.))
                .flex_shrink_0();
            if folder {
                suffix = suffix
                    .child(
                        self.connection_icon_button(
                            format!("key-refresh-{index}"),
                            "refresh",
                            self.tr("interface.reload"),
                            18.,
                            cx,
                            move |app, w, cx| app.scan_keys(id, true, false, w, cx),
                        )
                        .disabled(!tab.scan.end),
                    )
                    .child(self.connection_icon_button(
                        format!("key-new-{index}"),
                        "add",
                        self.tr("interface.new_key"),
                        18.,
                        cx,
                        {
                            let selection = selection.clone();
                            move |app, w, cx| {
                                app.new_browser_key(id, Some(selection.clone()), w, cx)
                            }
                        },
                    ));
            }
            suffix = suffix.child(self.connection_icon_button(
                format!("key-delete-{index}"),
                "delete",
                self.tr(if folder {
                    "interface.batch_delete_key"
                } else {
                    "interface.remove_key"
                }),
                18.,
                cx,
                {
                    let selection = selection.clone();
                    move |app, w, cx| {
                        app.delete_browser_selection(id, Some(selection.clone()), false, w, cx)
                    }
                },
            ));
            content = content.child(suffix);
        } else if row.key.as_ref().is_some_and(|key| contains_binary(key)) && !selected {
            content = content.child(icon("binary").size(px(20.)).mr(px(5.)).flex_shrink_0());
        }
        let node = div()
            .id(("key-row", index))
            .test_support()
            .flex()
            .items_center()
            .h(px(30.))
            .rounded(px(3.))
            .cursor_pointer()
            .when(selected, |node| node.bg(rgba(0xd33a311a)))
            .when(!selected, |node| {
                node.hover(|style| style.bg(self.connection_hover_color(cx)))
            })
            .child(div().w(px(row.depth as f32 * 24.)).h_full().flex_shrink_0())
            .child(
                div()
                    .id(("key-switcher", index))
                    .test_support()
                    .w(px(24.))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(folder, |switcher| {
                        switcher.child(
                            icon(if expanded {
                                "tree-switcher-down"
                            } else {
                                "tree-switcher-right"
                            })
                            .size(px(14.)),
                        )
                    })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if folder {
                            app.toggle_key_folder(id, &path, cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(content)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let path = row.path.clone();
                    let check_mode = tab.check_mode;
                    move |app, event: &MouseDownEvent, w, cx| {
                        if folder && !check_mode && event.click_count == 2 {
                            app.toggle_key_folder(id, &path, cx);
                        }
                        if let Some(i) = app.tab_index(id) {
                            app.tabs[i].key_focus.focus(w, cx);
                        }
                    }
                }),
            )
            .on_click(cx.listener(move |app, _, w, cx| {
                app.browser_selection(id, selection_click.clone(), true, w, cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, _, w, cx| {
                    app.browser_selection(id, selection_right.clone(), false, w, cx)
                }),
            );
        let menu_labels = [
            self.tr(if folder {
                "interface.new_key"
            } else {
                "interface.reload"
            }),
            self.tr(if folder {
                "interface.copy_path"
            } else {
                "interface.copy_key"
            }),
            self.tr(if folder {
                "interface.batch_delete_key"
            } else {
                "interface.remove_key"
            }),
        ];
        let node = if database {
            node.into_any_element()
        } else {
            node.context_menu(move |menu, _, _| {
                let menu = menu.tiny_rdm_style(true);
                let folder = matches!(menu_selection, Selection::Folder(_));
                let selection = menu_selection.clone();
                let menu = menu.item(
                    menu_action(menu_labels[0].clone(), weak.clone(), move |app, w, cx| {
                        if folder {
                            app.new_browser_key(id, Some(selection.clone()), w, cx);
                        } else {
                            app.reload_browser_selection(id, w, cx);
                        }
                    })
                    .icon(icon(if folder { "add" } else { "refresh" })),
                );
                let copy = menu_selection.clone();
                let del = menu_selection.clone();
                menu.item(
                    menu_action(menu_labels[1].clone(), weak.clone(), move |app, _, cx| {
                        let text = match &copy {
                            Selection::Folder(path) => path
                                .strip_suffix(super::views::key_separator(
                                    &app.tabs[app.tab_index(id).unwrap()],
                                ))
                                .unwrap_or(path)
                                .to_string(),
                            Selection::Key(key) => String::from_utf8_lossy(key).into_owned(),
                        };
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        app.toast(app.tr("interface.copy_succ"), false);
                    })
                    .icon(icon("copylink")),
                )
                .separator()
                .item(
                    menu_action(menu_labels[2].clone(), weak.clone(), move |app, w, cx| {
                        app.delete_browser_selection(id, Some(del.clone()), false, w, cx)
                    })
                    .icon(icon("delete")),
                )
            })
            .into_any_element()
        };
        div().px(px(3.)).h(px(30.)).child(node).into_any_element()
    }
    pub(super) fn render_warning_dialog(&self, message: String, cx: &Context<Self>) -> AnyElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000066))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| app.close_dialog(window, cx)),
            )
            .child(
                div()
                    .id("key-delete-warning")
                    .test_support()
                    .w(px(446.))
                    .px(px(28.))
                    .pt(px(16.))
                    .pb(px(20.))
                    .rounded(px(4.))
                    .bg(if self.dark(cx) { rgb(0x2c2c32).into() } else { cx.theme().background })
                    .shadow_lg()
                    .font({
                        let mut font = gpui::font(".SystemUIFont");
                        font.fallbacks = Some(FontFallbacks::default());
                        font
                    })
                    .text_size(px(14.))
                    .text_color(if self.dark(cx){rgba(0xffffffd1)}else{rgb(0x333639)})
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("key-delete-warning-title")
                            .test_support()
                            .flex()
                            .items_center()
                            .h(px(28.))
                            .gap(px(4.))
                            .text_size(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if self.dark(cx) {
                                rgba(0xffffffe6)
                            } else {
                                rgb(0x1f2225)
                            })
                            .child(
                                icon("notice-warning")
                                    .size(px(28.))
                                    .text_color(rgb(if self.dark(cx){0xf2c97d}else{0xf0a020})),
                            )
                            .child(self.tr("common.warning")),
                    )
                    .child(
                        div()
                            .id("key-delete-warning-content")
                            .test_support()
                            .mt(px(8.))
                            .mb(px(16.))
                            .min_h(px(21.))
                            .line_height(px(21.))
                            .child(message),
                    )
                    .child(
                        div()
                            .id("key-delete-warning-actions")
                            .test_support()
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .h(px(28.))
                            .child(
                                self.btn(
                                    "dialog-cancel",
                                    self.tr("common.cancel"),
                                    "",
                                    cx,
                                    |app, w, cx| app.close_dialog(w, cx),
                                )
                                .content_style(div().text_size(px(14.)).style().clone(),gpui_component::Size::Size(px(18.)))
                                .text_color(rgb(0x333639))
                                .border_color(if self.dark(cx){rgba(0xffffff3d)}else{rgb(0xe0e0e6)})
                                .h(px(28.))
                                .px(px(7.))
                                .when(self.dark(cx), |button| button.bg(Hsla::transparent_black()).text_color(rgba(0xffffffd1)))
                                .rounded(px(4.)),
                            )
                            .child(
                                self.btn(
                                    "dialog-save",
                                    self.tr("common.confirm"),
                                    "",
                                    cx,
                                    |app, w, cx| app.submit_dialog(w, cx),
                                )
                                .content_style(div().text_size(px(14.)).style().clone(),gpui_component::Size::Size(px(18.)))
                                .warning()
                                .bg(rgb(if self.dark(cx){0xf2c97d}else{0xf0a020}))
                                .text_color(rgb(if self.dark(cx){0}else{0xffffff}))
                                .h(px(28.))
                                .px(px(8.))
                                .rounded(px(4.))
                                .border_color(rgb(if self.dark(cx){0xf2c97d}else{0xf0a020})),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn browser_footer(&self, id: u64, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let tab = &self.tabs[self.tab_index(id).unwrap()];
        let mut footer = div()
            .id("key-browser-footer")
            .test_support()
            .h(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(8.))
            .py(px(3.));
        if tab.check_mode {
            for (button, asset, tip, kind) in [
                (
                    "export-checked",
                    "export-thick",
                    "interface.export_checked",
                    DialogKind::Export,
                ),
                (
                    "ttl-checked",
                    "timer-thick",
                    "interface.ttl_checked",
                    DialogKind::Ttl,
                ),
                (
                    "delete-checked",
                    "delete-thick",
                    "interface.delete_checked",
                    DialogKind::DeleteKeys,
                ),
            ] {
                footer = footer.child(
                    self.browser_tool_button(
                        button,
                        asset,
                        self.tr(tip),
                        24.,
                        20.,
                        false,
                        cx,
                        move |app, w, cx| {
                            if matches!(kind, DialogKind::DeleteKeys) {
                                app.delete_browser_selection(id, None, true, w, cx);
                            } else {
                                app.open_dialog(kind.clone(), w, cx);
                            }
                        },
                    )
                    .disabled(tab.selected.is_empty()),
                );
            }
            return footer
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_center()
                        .mx(px(5.))
                        .truncate()
                        .child(format!(
                            "{} / {}",
                            tab.selected.len(),
                            tab.keys.len().max(tab.selected.len())
                        )),
                )
                .child(self.browser_tool_button(
                    "quit-check-mode",
                    "close",
                    self.tr("interface.quit_check_mode"),
                    24.,
                    20.,
                    false,
                    cx,
                    move |app, _, cx| {
                        if let Some(i) = app.tab_index(id) {
                            app.tabs[i].check_mode = false;
                            app.tabs[i].selected.clear();
                            cx.notify();
                        }
                    },
                ))
                .into_any_element();
        }
        footer = footer.child(self.render_database_selector(id, window, cx));
        footer = footer
            .child(
                self.browser_tool_button(
                    "load-more-keys",
                    "loadlist-thick",
                    self.tr("interface.load_more"),
                    24.,
                    21.,
                    false,
                    cx,
                    move |app, w, cx| app.scan_keys(id, false, false, w, cx),
                )
                .disabled(tab.scan.end || tab.keys_loading),
            )
            .child(
                self.browser_tool_button(
                    "load-all-keys",
                    "loadall-thick",
                    self.tr("interface.load_all"),
                    24.,
                    21.,
                    false,
                    cx,
                    move |app, w, cx| app.scan_keys(id, false, true, w, cx),
                )
                .disabled(tab.scan.end || tab.keys_loading),
            )
            .child(div().flex_1().min_w(px(10.)))
            .child(self.browser_tool_button(
                "check-mode",
                "checkbox-thick",
                self.tr("interface.check_mode"),
                24.,
                19.,
                false,
                cx,
                move |app, _, cx| {
                    if let Some(i) = app.tab_index(id) {
                        app.tabs[i].check_mode = true;
                        app.tabs[i].key_root_expanded = true;
                        cx.notify();
                    }
                },
            ));
        let weak = cx.weak_entity();
        let more_labels = [
            self.tr("interface.import_key"),
            self.tr("interface.batch_delete_key"),
            self.tr("interface.flush_db"),
            self.tr("interface.disconnect"),
        ];
        footer
            .child(
                self.browser_tool_button(
                    "database-more",
                    "more-thick",
                    self.tr("interface.more_action"),
                    24.,
                    20.,
                    false,
                    cx,
                    |_, _, _| {},
                )
                .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, _, _| {
                    menu.tiny_rdm_style(false)
                        .item(
                            menu_action(more_labels[0].clone(), weak.clone(), |app, w, cx| {
                                app.open_dialog(DialogKind::Import, w, cx)
                            })
                            .icon(icon("import")),
                        )
                        .separator()
                        .item(
                            menu_action(more_labels[1].clone(), weak.clone(), move |app, w, cx| {
                                let selection = app
                                    .tab_index(id)
                                    .and_then(|i| app.tabs[i].key_selection.clone())
                                    .filter(|s| matches!(s, Selection::Folder(_)))
                                    .unwrap_or_else(|| Selection::Folder(String::new()));
                                app.delete_browser_selection(id, Some(selection), false, w, cx)
                            })
                            .icon(icon("delete")),
                        )
                        .item(
                            menu_action(more_labels[2].clone(), weak.clone(), |app, w, cx| {
                                app.open_dialog(DialogKind::Flush, w, cx)
                            })
                            .icon(icon("delete")),
                        )
                        .separator()
                        .item(
                            menu_action(more_labels[3].clone(), weak.clone(), move |app, w, cx| {
                                app.close_tab(id, w, cx)
                            })
                            .icon(icon("unlink-thick")),
                        )
                })
                .offset(px(6.)),
            )
            .into_any_element()
    }
}

fn keys_for_selection(tab: &Tab, selection: &Selection, filter: &str) -> Vec<Vec<u8>> {
    match selection {
        Selection::Key(key) => vec![key.clone()],
        Selection::Folder(prefix) => tab
            .keys
            .iter()
            .filter(|entry| {
                let label = display_bytes(&entry.key);
                label.starts_with(prefix) && label.contains(filter)
            })
            .map(|entry| entry.key.clone())
            .collect(),
    }
}
// Build the ordered cascade only on a checkbox update. Rendering the visible
// folder checkmarks needs membership/counts and can avoid rebuilding the tree.
fn ordered_keys_for_selection(tab: &Tab, selection: &Selection, filter: &str) -> Vec<Vec<u8>> {
    match selection {
        Selection::Key(key) => vec![key.clone()],
        Selection::Folder(prefix) => tree_rows(tab, filter, true)
            .into_iter()
            .filter_map(|entry| entry.key)
            .filter(|key| display_bytes(key).starts_with(prefix))
            .collect(),
    }
}
pub(super) fn database_label(tab: &Tab, index: u32) -> String {
    let database = tab.databases.iter().find(|db| db.index == index);
    let mut name = database
        .filter(|db| !db.alias.is_empty())
        .map(|db| format!("{}[{}]", db.alias, index))
        .unwrap_or_else(|| format!("db{index}"));
    if index == tab.db {
        if tab.key_match.is_empty() {
            name.push_str(&format!(
                " ({}/{})",
                tab.keys.len(),
                tab.total.max(tab.keys.len() as u64)
            ));
        } else {
            name.push_str(&format!(" ({})", tab.keys.len()));
        }
    } else {
        name.push_str(&format!(" ({})", database.map(|db| db.keys).unwrap_or(0)));
    }
    name
}
