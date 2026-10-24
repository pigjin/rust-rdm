use super::dialogs::DialogKind;
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

impl RdmApp {
    pub(super) fn update_entry_filter(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else { return };
        let tab = &mut self.tabs[i];
        let value = text_input(&tab.row_filter, cx);
        tab.row_filter_epoch += 1;
        let epoch = tab.row_filter_epoch;
        let leading = tab
            .row_filter_changed
            .is_none_or(|time| time.elapsed() >= Duration::from_millis(500));
        tab.row_filter_changed = Some(Instant::now());
        if leading {
            tab.row_filter_value = value.clone();
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update_in(cx, |app, _, cx| {
                if let Some(i) = app.tab_index(id)
                    && app.tabs[i].row_filter_epoch == epoch
                {
                    app.tabs[i].row_filter_value = value;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn clear_entry_search(
        &mut self,
        id: u64,
        edit_match: bool,
        clear_input: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.tab_index(id) else { return };
        self.tabs[i].match_tooltips.cancel(super::match_tooltip::MatchKind::Entry);
        let tab = &mut self.tabs[i];
        let had_match = !tab.row_match.is_empty();
        if edit_match {
            set_input(&tab.row_filter, tab.row_match.clone(), window, cx);
        } else if clear_input {
            set_input(&tab.row_filter, "", window, cx);
        }
        tab.row_match.clear();
        tab.row_filter_value = text_input(&tab.row_filter, cx);
        tab.row_filter_epoch += 1;
        if had_match {
            self.reload_detail(id, true, window, cx);
        }
        cx.notify();
    }

    pub(super) fn entry_rows(&self, i: usize) -> Vec<Entry> {
        let tab = &self.tabs[i];
        let Some(detail) = &tab.detail else {
            return Vec::new();
        };
        let filter = &tab.row_filter_value;
        let mut rows = detail
            .rows
            .iter()
            .filter(|row| {
                if filter.is_empty() {
                    return true;
                }
                match detail.kind {
                    KeyType::Hash => display_bytes(&row.id).contains(filter),
                    KeyType::Stream => stream_display(row).contains(filter),
                    _ => row
                        .display_value
                        .clone()
                        .or_else(|| tab.row_values.get(&row.id).map(|value| value.text.clone()))
                        .unwrap_or_else(|| display_bytes(&row.value))
                        .contains(filter),
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        if detail.kind == KeyType::Hash
            && let Some(ascending) = tab.field_sort
        {
            rows.sort_by(|a, b| {
                let order = natural_compare(
                    &display_bytes(&a.id).to_lowercase(),
                    &display_bytes(&b.id).to_lowercase(),
                );
                if ascending { order } else { order.reverse() }
            });
        }
        if detail.kind == KeyType::ZSet
            && let Some(ascending) = tab.score_sort
        {
            rows.sort_by(|a, b| {
                let order = a
                    .score
                    .parse::<f64>()
                    .unwrap_or(0.)
                    .total_cmp(&b.score.parse::<f64>().unwrap_or(0.));
                if ascending { order } else { order.reverse() }
            });
        }
        rows
    }

    pub(super) fn entry_border(&self, cx: &App) -> Hsla {
        rgb(if self.dark(cx) { 0x2d2d30 } else { 0xefeff5 }).into()
    }
    fn entry_color(&self, header: bool, stripe: bool, sorting: bool, cx: &App) -> Hsla {
        let color = match (self.dark(cx), header, stripe, sorting) {
            (false, true, _, true) => 0xf3f3f7,
            (false, true, _, false) | (false, false, true, _) => 0xfafafc,
            (false, false, false, true) => 0xf7f7fa,
            (false, false, false, false) => 0xffffff,
            (true, true, _, true) => 0x2d2d31,
            (true, true, _, false) => 0x26262a,
            (true, false, true, _) => 0x242427,
            (true, false, false, true) => 0x26262a,
            (true, false, false, false) => 0x18181c,
        };
        rgb(color).into()
    }
    fn entry_cell(
        &self,
        key: impl Into<ElementId>,
        width: f32,
        last: bool,
        cx: &App,
    ) -> gpui_base::ObservedElement<Stateful<Div>> {
        let key = key.into();
        let hover_group = match &key {
            ElementId::NamedInteger(_, index) => {
                Some(SharedString::from(format!("entry-row-{index}")))
            }
            _ => None,
        };
        div()
            .id(key)
            .test_support()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .min_w_0()
            .px(px(8.))
            .flex()
            .items_center()
            .when(!last, |cell| {
                cell.border_r_1().border_color(self.entry_border(cx))
            })
            .when_some(hover_group, |cell, group| {
                cell.group_hover(group, |style| {
                    style.bg(self.entry_color(false, false, true, cx))
                })
            })
    }
    fn entry_header(
        &self,
        id: u64,
        key: &'static str,
        label: String,
        width: f32,
        sort: Option<Option<bool>>,
        resizable: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let cell = self
            .entry_cell(format!("heading-{key}"), width, false, cx)
            .relative()
            .border_b_1()
            .border_color(self.entry_border(cx))
            .justify_center()
            .bg(self.entry_color(true, false, sort.is_some_and(|v| v.is_some()), cx))
            .when(sort.is_some(), |cell| {
                cell.cursor_pointer()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if let Some(i) = app.tab_index(id) {
                            let order = if key == "entry-score" {
                                &mut app.tabs[i].score_sort
                            } else {
                                &mut app.tabs[i].field_sort
                            };
                            *order = match order {
                                None => Some(false),
                                Some(false) => Some(true),
                                Some(true) => None,
                            };
                            let count = app.tabs[i].entry_list.item_count();
                            app.tabs[i].entry_list.splice(0..count, count);
                            cx.notify();
                        }
                    }))
            })
            .child(div().flex_1().min_w_0().text_center().child(label))
            .when_some(sort, |cell, order| {
                cell.child(
                    div()
                        .w(px(19.))
                        .h(px(21.))
                        .pl(px(4.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_end()
                        .text_color(if order.is_some() {
                            cx.theme().primary
                        } else if self.dark(cx) {
                            rgba(0xffffff61).into()
                        } else {
                            rgb(0xc2c2c2).into()
                        })
                        .child(
                            icon(if order == Some(true) {
                                "entry-sort-up"
                            } else {
                                "entry-sort-down"
                            })
                            .size(px(15.)),
                        ),
                )
            })
            .when(resizable, |cell| {
                cell.child(self.entry_resize_handle(id, key, width, cx))
            });
        cell.into_any_element()
    }

    fn entry_resize_handle(
        &self,
        id: u64,
        key: &'static str,
        width: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("entry-resize-{key}"));
        let active = self
            .column_drag
            .as_ref()
            .is_some_and(|(owner, column, _, _)| *owner == id && column == key);
        div()
            .id(format!("resize-{key}"))
            .test_support()
            .group(group.clone())
            .absolute()
            .right(px(4.))
            .top_0()
            .bottom_0()
            .w(px(8.))
            .cursor(CursorStyle::ResizeColumn)
            .child(
                div()
                    .id(format!("resize-line-{key}"))
                    .test_support()
                    .absolute()
                    .left(px(4.))
                    .top(relative(0.25))
                    .h(relative(0.5))
                    .w(px(1.))
                    .bg(if active {
                        cx.theme().primary
                    } else {
                        self.entry_border(cx)
                    })
                    .group_hover(group, |style| style.bg(cx.theme().primary)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                    app.column_drag = Some((id, key.into(), event.position.x.as_f32(), width));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_entries(
        &self,
        i: usize,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        if self.entry_panel_fullscreen() {
            return div().size_full().into_any_element();
        }
        let tab = &self.tabs[i];
        let id = tab.id;
        let detail = tab.detail.as_ref().unwrap();
        let kind = detail.kind.clone();
        let stream = kind == KeyType::Stream;
        let editing = self.entry_panel_visible();
        let has_number = !stream;
        let has_field = kind == KeyType::Hash || stream;
        let has_score = kind == KeyType::ZSet && !editing;
        let show_value = !editing || kind != KeyType::Hash;
        let show_actions = !editing;
        let content_width = (self.viewport_width - self.pref.behavior.aside_width - 52.).max(170.);
        let available = (content_width - 10.) / if editing { 2. } else { 1. };
        let action_width = if stream { 80. } else { 120. };
        let fixed =
            if has_number { 80. } else { 0. } + if show_actions { action_width } else { 0. };
        let flexible = usize::from(has_field) + usize::from(show_value) + usize::from(has_score);
        let flexible_keys = [
            ("entry-field", has_field),
            ("entry-value", show_value),
            ("entry-score", has_score),
        ];
        let known = if editing {
            Vec::new()
        } else {
            flexible_keys
                .iter()
                .filter(|(_, visible)| *visible)
                .filter_map(|(key, _)| tab.column_widths.get(*key))
                .collect::<Vec<_>>()
        };
        let default_width = ((available - fixed - known.iter().map(|width| **width).sum::<f32>())
            / flexible.saturating_sub(known.len()).max(1) as f32)
            .max(80.);
        let column_width = |key: &str| {
            if editing {
                default_width
            } else {
                tab.column_widths.get(key).copied().unwrap_or(default_width)
            }
        };
        let field_width = column_width("entry-field");
        let value_width = column_width("entry-value");
        let score_width = column_width("entry-score");
        let table_width = fixed
            + if has_field { field_width } else { 0. }
            + if show_value { value_width } else { 0. }
            + if has_score { score_width } else { 0. };
        let header_height = if cfg!(target_os = "macos") || stream {
            34.
        } else {
            39.
        };
        let row_height = if stream {
            61.
        } else if cfg!(target_os = "macos") {
            33.
        } else {
            38.
        };
        let rows = Arc::new(self.entry_rows(i));
        let empty = rows.is_empty();
        if tab.entry_list.item_count() != rows.len() {
            tab.entry_list.reset(rows.len());
        }
        let filter = text_input(&tab.row_filter, cx);
        let search_label = self.tr("interface.full_search");
        let text_width = |text: String| {
            window
                .text_system()
                .shape_line(
                    text.clone().into(),
                    px(14.),
                    &[TextRun {
                        len: text.len(),
                        font: self.preference_font("general", ".SystemUIFont", cx),
                        color: cx.theme().foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width
                .as_f32()
        };
        let search_width = text_width(search_label.clone()) + 24.;
        #[cfg(target_os = "macos")]
        let input_width = {
            let mut families = self.pref.general.font_family.clone();
            if families.is_empty() {
                families.push(
                    self.preference_font("general", ".SystemUIFont", cx)
                        .family
                        .to_string(),
                );
            }
            // The clearable Naive input reserves a 20px suffix and 6px padding
            // on each side even when the clear icon is invisible.
            super::text_control_width::intrinsic(&families, 14., 20) + 32.
        };
        #[cfg(not(target_os = "macos"))]
        let input_width = 186_f32;
        let match_label = tab.row_match.replace(['\n', '\r'], " ");
        let match_width = if match_label.is_empty() { 0. } else {
            text::script_width(&match_label, self.preference_font("general", ".SystemUIFont", cx), 12., window) + 30.
        };
        // The prefix contributes to the original input's intrinsic width.
        // Its group can grow until it reaches half of the toolbar's content.
        let input_width = if match_label.is_empty() { input_width } else {
            let group_limit = ((window.viewport_size().width.as_f32() - self.pref.behavior.aside_width - 62.) * 0.5).max(0.);
            (input_width + match_width + 4.).min((group_limit - search_width + 1.).max(0.))
        };
        let mut input = gpui::Styled::h(
            self.search_input(&tab.row_filter, window, cx)
                .pl(px(5.))
                .pr(px(25.))
                .rounded_l(px(4.))
                .rounded_r(px(0.))
                .w_full()
                .min_w_0(),
            px(32.),
        );
        let filter_suffix = div()
            .id("entry-filter-suffix")
            .test_support()
            .w(px(20.))
            .h(px(16.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end()
            .child(
                div()
                    .id("entry-filter-clear-slot")
                    .test_support()
                    .size(px(16.))
                    .flex_shrink_0()
                    .when(!filter.is_empty(), |slot| {
                        slot.child(self.search_clear_button(
                            "clear-entry-filter",
                            "entry-filter-input",
                            tab.row_filter.focus_handle(cx).is_focused(window),
                            cx,
                            move |app, w, cx| app.clear_entry_search(id, false, true, w, cx),
                        ))
                    }),
            )
            .absolute()
            .right(px(6.))
            .top(px(8.));
        if !tab.row_match.is_empty() {
            let width = match_width.min(((input_width - 12.) * 0.5).max(0.));
            input = input.prefix(self.search_match_tag(
                "entry-search-match",
                "clear-entry-match",
                match_label,
                width,
                tab.row_filter.focus_handle(cx),
                super::match_tooltip::MatchTarget { tab_id: id, kind: super::match_tooltip::MatchKind::Entry, left: self.pref.behavior.aside_width + 62., width },
                window,
                cx,
                move |app, w, cx| app.clear_entry_search(id, true, false, w, cx),
                move |app, w, cx| app.clear_entry_search(id, false, false, w, cx),
            ));
        }
        let load_button = |key: &str, asset: &str, tip: &str| {
            let all = asset == "loadall";
            self.btn(key.to_string(), "", asset, cx, move |app, w, cx| {
                if all {
                    app.load_all_entries(id, w, cx)
                } else {
                    app.reload_detail(id, false, w, cx)
                }
            })
            .tooltip(self.tr(tip))
            .w(px(42.))
            .h(px(32.))
            .rounded(px(3.))
            .disabled(detail.end || tab.values_loading || tab.entries_loading)
        };
        let cancel_load = tab.entries_loading.then(|| self.btn("cancel-entry-load", self.tr("common.cancel"), "close", cx, move |app, _, cx| app.cancel_entry_load(id, cx)));
        let add_label = self.tr("interface.add_row");
        let add_width = text_width(add_label.clone()) + 48.;
        let mut toolbar = div()
            .id("entry-toolbar")
            .test_support()
            .flex()
            .items_center()
            .gap(px(5.))
            .h(px(32.))
            .flex_shrink_0()
            .px(px(5.))
            .child(
                div()
                    .id("entry-filter-group")
                    .test_support()
                    .overflow_hidden()
                    .flex()
                    .min_w_0()
                    .min_w_0()
                    .w(px(input_width + search_width - 1.))
                    .max_w(relative(0.5))
                    .child(
                        div()
                            .id("entry-filter")
                            .test_support()
                            .group("entry-filter-input")
                            .relative()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .h(px(32.))
                            .children(self.search_input_ring(&tab.row_filter, window, cx))
                            .child(input)
                            .child(filter_suffix),
                    )
                    .child(
                        self.btn("entry-search", search_label, "", cx, move |app, w, cx| {
                            app.search_entries(id, w, cx)
                        })
                        .px(px(11.))
                        .text_size(px(14.))
                        .line_height(px(14.))
                        .text_color(Hsla::from(if self.dark(cx) {
                            rgba(0xffffffd1)
                        } else {
                            rgb(0x333639)
                        }))
                        // Naive retains normal colors in its disabled state;
                        // only the whole button's opacity changes.
                        .border_color(cx.theme().border)
                        .bg(Hsla::transparent_black())
                        .hover_style(StyleRefinement::default()
                            .text_color(cx.theme().primary_hover)
                            .border_color(cx.theme().primary_hover)
                            .bg(Hsla::transparent_black()))
                        .active_style(StyleRefinement::default()
                            .text_color(cx.theme().primary_active)
                            .border_color(cx.theme().primary_active)
                            .bg(Hsla::transparent_black()))
                        .tab_stop(false)
                        .w(px(search_width))
                        .flex_shrink_0()
                        .h(px(32.))
                        .ml(px(-1.))
                        .rounded_l(px(0.))
                        .rounded_r(px(4.))
                        .disabled(!tab.row_match.is_empty() && filter.trim().is_empty())
                        .opacity(self.disabled_button_opacity(
                            !tab.row_match.is_empty() && filter.trim().is_empty(),
                            cx,
                        )),
                    ),
            )
            .child(div().flex_1().min_w_0());
        if let Some(cancel) = cancel_load { toolbar = toolbar.child(cancel); }
        if !stream {
            toolbar = toolbar
                .child(div().flex().children((0..2).map(|align| {
                    self.btn(
                        format!("entry-align-{align}"),
                        "",
                        if align == 0 {
                            "aligncenter"
                        } else {
                            "alignleft"
                        },
                        cx,
                        move |app, _, cx| {
                            if app.pref.editor.entry_text_align != align {
                                app.pref.editor.entry_text_align = align;
                                app.save_preferences();
                                cx.notify();
                            }
                        },
                    )
                    .icon(
                        icon(if align == 0 {
                            "aligncenter"
                        } else {
                            "alignleft"
                        })
                        .size(px(20.)),
                    )
                    .w(px(42.))
                    .h(px(32.))
                    .rounded(px(3.))
                    .when(
                        self.pref.editor.entry_text_align != align,
                        |button| {
                            button
                                .bg(Hsla::from(if self.dark(cx) {
                                    rgba(0xffffff14)
                                } else {
                                    rgba(0x80808014)
                                }))
                                .border_color(Hsla::transparent_black())
                        },
                    )
                })))
                .child(div().w(px(1.)).h(px(16.)).mx(px(8.)).bg(cx.theme().border));
        }
        toolbar = toolbar
            .child(
                div()
                    .flex()
                    .child(load_button(
                        "more-entries",
                        "loadlist",
                        "interface.load_more_entries",
                    ))
                    .child(load_button(
                        "all-entries",
                        "loadall",
                        "interface.load_all_entries",
                    )),
            )
            .child(
                self.btn("add-entry", "", "", cx, |app, w, cx| {
                    app.open_dialog(DialogKind::AddEntries, w, cx)
                })
                .accessibility_label(add_label.clone())
                .px(px(11.))
                .text_size(px(14.))
                .w(px(add_width))
                .flex_shrink_0()
                .h(px(32.))
                .rounded(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(icon("addlink").size(px(18.)))
                        .child(add_label),
                ),
            );
        let mut heading = div()
            .id("entry-heading")
            .test_support()
            .flex()
            .w(px(table_width))
            .h(px(header_height))
            .flex_shrink_0()
            .font_weight(FontWeight::MEDIUM)
            .bg(self.entry_color(true, false, false, cx));
        if has_number {
            heading = heading.child(
                self.entry_cell("heading-entry-number", 80., false, cx)
                    .border_b_1()
                    .border_color(self.entry_border(cx))
                    .justify_center()
                    .child("#"),
            );
        }
        if has_field {
            heading = heading.child(self.entry_header(
                id,
                "entry-field",
                if stream {
                    "ID".into()
                } else {
                    self.tr("common.field")
                },
                field_width,
                if stream { None } else { Some(tab.field_sort) },
                true,
                cx,
            ));
        }
        if show_value {
            heading = heading.child(self.entry_header(
                id,
                "entry-value",
                self.tr("common.value"),
                value_width,
                None,
                kind != KeyType::List && kind != KeyType::Set,
                cx,
            ));
        }
        if has_score {
            heading = heading.child(self.entry_header(
                id,
                "entry-score",
                self.tr("common.score"),
                score_width,
                Some(tab.score_sort),
                true,
                cx,
            ));
        }
        if show_actions {
            heading = heading.child(
                self.entry_cell("heading-entry-actions", action_width, true, cx)
                    .border_b_1()
                    .border_color(self.entry_border(cx))
                    .justify_center()
                    .child(self.tr("interface.action")),
            );
        }
        let list = gpui::list(
            tab.entry_list.clone(),
            cx.processor(move |app, index: usize, window, cx| {
                let tab = &app.tabs[app.tab_index(id).unwrap()];
                let row = rows[index].clone();
                let value = if stream {
                    stream_display(&row)
                } else {
                    row.display_value
                        .clone()
                        .or_else(|| tab.row_values.get(&row.id).map(|value| value.text.clone()))
                        .unwrap_or_else(|| display_bytes(&row.value))
                };
                let code = stream || matches!(tab.format.as_str(), "JSON" | "Unicode JSON");
                let center = app.pref.editor.entry_text_align == 0;
                let number = if kind == KeyType::List {
                    display_bytes(&row.id).parse::<usize>().unwrap_or(index) + 1
                } else {
                    index + 1
                };
                let bg = app.entry_color(false, index % 2 == 1, false, cx);
                let clicked = row.clone();
                let height = if code {
                    (value.lines().count().max(1) as f32 * 21. + 40.).max(row_height)
                } else {
                    row_height
                };
                let mut node = div()
                    .id(("entry", index))
                    .test_support()
                    .group(format!("entry-row-{index}"))
                    .flex()
                    .items_stretch()
                    .w(px(table_width))
                    .h(px(height))
                    .bg(bg)
                    .on_click(cx.listener(move |app, _, w, cx| {
                        if app.editing_entry() {
                            app.edit_row(clicked.clone(), w, cx);
                        }
                    }));
                if has_number {
                    node = node.child(
                        app.entry_cell(("entry-number", index), 80., false, cx)
                            .justify_center()
                            .when(app.editing_row(&row.id), |cell| {
                                cell.child(
                                    icon("entry-editing")
                                        .size(px(16.))
                                        .text_color(rgb(0xff0000)),
                                )
                            })
                            .when(!app.editing_row(&row.id), |cell| {
                                cell.child(number.to_string())
                            }),
                    );
                }
                if has_field {
                    node = node.child(
                        app.entry_cell(
                            ("entry-field", index),
                            field_width,
                            !show_value && !show_actions,
                            cx,
                        )
                        .bg(app.entry_color(
                            false,
                            index % 2 == 1,
                            kind == KeyType::Hash && tab.field_sort.is_some(),
                            cx,
                        ))
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .truncate()
                                .when(center || stream, |text| text.text_center())
                                .when(tab.removed_fields.contains(&row.id), |text| {
                                    text.line_through()
                                })
                                .child(display_bytes(&row.id)),
                        ),
                    );
                }
                if show_value {
                    node = node.child(
                        app.entry_cell(
                            ("entry-value", index),
                            value_width,
                            !has_score && !show_actions,
                            cx,
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .when(center && !code, |text| text.text_center())
                                .when(!code, |text| text.truncate())
                                .when(code, |text| text.my(px(20.)).font_family("Menlo"))
                                .when(tab.removed_fields.contains(&row.id), |text| {
                                    text.line_through()
                                })
                                .child(value.clone()),
                        ),
                    );
                }
                if has_score {
                    node = node.child(
                        app.entry_cell(("entry-score", index), score_width, !show_actions, cx)
                            .bg(app.entry_color(
                                false,
                                index % 2 == 1,
                                tab.score_sort.is_some(),
                                cx,
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .when(center, |text| text.text_center())
                                    .child(row.score.clone()),
                            ),
                    );
                }
                if show_actions {
                    let copied = display_bytes(&row.value);
                    let mut actions = div()
                        .id(("entry-actions", index))
                        .test_support()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .w_full()
                        .h(px(18.))
                        .child(
                            app.connection_icon_button(
                                format!("entry-copy-{index}"),
                                "copy",
                                app.tr("interface.copy_value"),
                                18.,
                                cx,
                                move |app, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copied.clone(),
                                    ));
                                    app.toast(app.tr("interface.copy_succ"), false);
                                    cx.notify();
                                },
                            )
                            .icon(icon("copy").size(px(20.)))
                            .allow_icon_overflow(),
                        );
                    if kind == KeyType::Hash {
                        let field = row.id.clone();
                        actions = actions.child(
                            app.connection_icon_button(
                                format!("entry-refresh-{index}"),
                                "refresh",
                                app.tr("interface.reload"),
                                18.,
                                cx,
                                move |app, w, cx| app.refresh_hash_field(id, field.clone(), w, cx),
                            )
                            .icon(icon("refresh").size(px(20.)))
                            .allow_icon_overflow(),
                        );
                    }
                    if !stream {
                        let edit = row.clone();
                        actions = actions.child(
                            app.connection_icon_button(
                                format!("entry-edit-{index}"),
                                "edit",
                                app.tr("interface.edit_row"),
                                18.,
                                cx,
                                move |app, w, cx| {
                                    app.open_dialog(DialogKind::EditEntry(edit.clone()), w, cx)
                                },
                            )
                            .icon(icon("edit").size(px(20.)))
                            .allow_icon_overflow()
                            .disabled(tab.values_loading),
                        );
                    }
                    actions =
                        actions.child(app.render_entry_delete(id, &row, &kind, index, window, cx));
                    node = node.child(
                        app.entry_cell(("entry-action-cell", index), action_width, true, cx)
                            .child(actions),
                    );
                }
                node.into_any_element()
            }),
        )
        .flex_1()
        .min_h_0();
        let empty_view = div()
            .id("entry-empty")
            .test_support()
            .w_full()
            .h(px(165.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .text_color(if self.dark(cx) {
                rgba(0xffffff3d)
            } else {
                rgba(0x00000040)
            })
            .child(icon("empty").size(px(48.)))
            .child(if self.pref.general.language.starts_with("zh") {
                "无数据"
            } else {
                "No Data"
            });
        let table = div()
            .id("entry-table-scroll")
            .test_support()
            .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_x_scroll()
            .child(heading)
            .when(empty, |table| table.child(empty_view))
            .when(!empty, |table| table.child(list.w(px(table_width))));
        let progress = if detail.length == 0 {
            0.
        } else {
            (detail.rows.len() as f32 / detail.length as f32).min(1.)
        };
        div()
            .id("entry-wrapper")
            .test_support()
            .flex()
            .flex_col()
            .size_full()
            .text_size(px(14.))
            .line_height(px(21.))
            .gap(px(5.))
            .pt(px(5.))
            .pb(px(5.))
            .child(toolbar)
            .child(
                div()
                    .id("entry-progress")
                    .test_support()
                    .h(px(2.))
                    .w_full()
                    .flex_shrink_0()
                    .bg(self.entry_border(cx))
                    .child(div().h_full().w(relative(progress)).bg(if detail.end {
                        Hsla::transparent_black()
                    } else {
                        cx.theme().primary
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .px(px(5.))
                    .child(table)
                    .when(editing, |body| {
                        body.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.render_entry_panel(window, cx)),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn stream_display(row: &Entry) -> String {
    if let Some(value) = &row.display_value {
        return value.clone();
    }
    row.fields
        .iter()
        .map(|(key, value)| format!("\"{}\":{}", display_bytes(key), display_bytes(value)))
        .collect::<Vec<_>>()
        .join(", ")
}
