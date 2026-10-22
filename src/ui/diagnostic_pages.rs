//! Shared form and table primitives for the original diagnostic pages.
use super::*;
use gpui::prelude::*;
use gpui_base::TestSupportExt as _;

impl RdmApp {
    pub(super) fn diagnostic_fade(color: u32, background: u32, opacity: f32) -> Hsla {
        let channel = |shift: u32| {
            let first=((color >> shift) & 255) as f32;
            let second=((background >> shift) & 255) as f32;
            (first*opacity+second*(1.-opacity)).round() as u32
        };
        rgb((channel(16)<<16)|(channel(8)<<8)|channel(0)).into()
    }
    pub(super) fn diagnostic_input(&self, state: &Entity<InputState>, height: f32, cx: &App) -> Input {
        Styled::h(Input::new(state), px(height)).text_size(px(14.)).px(px(11.))
            .bg(if self.dark(cx) { Self::diagnostic_fade(0xffffff, 0x1e1e1e, 0.1) } else { rgb(0xffffff).into() })
            .border_color(if self.dark(cx) { Hsla::transparent_black() } else { rgb(0xe0e0e6).into() })
    }

    pub(super) fn diagnostic_button(&self, button: Button) -> Button {
        let mut content = div().gap(px(6.)).text_size(px(14.));
        button.content_style(content.style().clone(), gpui_component::Size::Size(px(18.)))
    }

    pub(super) fn diagnostic_outline(&self, button: Button, cx: &App) -> Button {
        self.diagnostic_button(button).with_variant(ButtonVariant::Default).outline()
            .border_1().border_color(if self.dark(cx) { rgba(0xffffff3d) } else { rgb(0xe0e0e6) })
            .bg(Hsla::transparent_black())
            .text_color(if self.dark(cx) { rgba(0xffffffd1) } else { rgb(0x333639) })
    }

    pub(super) fn diagnostic_separator(&self, cx: &App) -> AnyElement {
        div().absolute().right_0().top(px(12.)).bottom(px(12.)).w(px(1.))
            .bg(self.diagnostic_border(cx)).into_any_element()
    }

    pub(super) fn diagnostic_filter_width(&self, cx: &App) -> f32 {
        #[cfg(target_os = "macos")]
        {
            let mut families = self.pref.general.font_family.clone();
            if families.is_empty() {
                families.push(self.preference_font("general", ".SystemUIFont", cx).family.to_string());
            }
            // Medium Naive input: 11px padding each side, 20px clear suffix,
            // and the two border pixels, in addition to the native 20ch input.
            text_control_width::intrinsic(&families, 14., 20) + 44.
        }
        #[cfg(not(target_os = "macos"))]
        { let _ = cx; 220. }
    }

    pub(super) fn diagnostic_field(&self, label: String, control: AnyElement) -> AnyElement {
        div().flex().flex_col().pb(px(18.))
            .child(div().h(px(21.)).pb(px(5.)).pl(px(2.)).text_size(px(13.))
                .line_height(px(16.25)).font_weight(FontWeight(450.)).text_color(rgb(0x717880)).child(label))
            .child(div().h(px(32.)).flex().items_center().child(control))
            .into_any_element()
    }

    pub(super) fn diagnostic_border(&self, cx: &App) -> Hsla {
        if self.dark(cx) { rgba(0xffffff17).into() } else { rgb(0xefeff5).into() }
    }

    pub(super) fn diagnostic_empty(&self, cx: &App) -> AnyElement {
        div().size_full().flex().flex_col().items_center().justify_center().gap(px(8.))
            .text_color(rgb(if self.dark(cx) { 0x606060 } else { 0xc2c2c2 }))
            .child(icon("empty").size(px(40.)))
            .child(if self.lang["name"] == "简体中文" { "无数据" } else { "No Data" })
            .into_any_element()
    }

    pub(super) fn diagnostic_header_background(&self, sorted: bool, cx: &App) -> Hsla {
        rgb(match (self.dark(cx), sorted) {
            (false, false) => 0xfafafc, (false, true) => 0xf3f3f7,
            (true, false) => 0x26262a, (true, true) => 0x333337,
        }).into()
    }

    pub(super) fn diagnostic_row(
        &self, prefix: &'static str, index: usize, count: usize, cells: [String; 4],
        widths: [Option<f32>; 4], sorted: bool, striped: bool, cx: &App,
    ) -> AnyElement {
        let dark = self.dark(cx);
        let stripe = striped && index % 2 == 1;
        let normal = rgb(match (dark, stripe) {
            (false, false) => 0xffffff, (false, true) => 0xfafafc,
            (true, false) => 0x18181c, (true, true) => 0x242427,
        });
        let first = if sorted && !stripe { rgb(if dark { 0x26262a } else { 0xf7f7fa }) } else { normal };
        let mut row = div().id(format!("{prefix}-row-{index}")).test_support()
            .role(Role::Label).aria_label(cells.join("\t"))
            .relative().flex().w_full().flex_shrink_0().text_size(px(14.)).line_height(px(21.))
            .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
            .text_color(if dark { rgba(0xffffffd1) } else { rgb(0x333639) });
        for (j, (text, width)) in cells.into_iter().zip(widths).enumerate() {
            let cell = div().id(format!("{prefix}-cell-{index}-{j}")).test_support()
                .role(Role::Label).aria_label(text.clone()).min_w_0().py(px(12.)).px(px(12.))
                .when(striped && index + 1 < count, |cell| cell.border_b_1().border_color(self.diagnostic_border(cx)))
                .when_some(width, |cell, width| cell.w(px(width)).flex_shrink_0())
                .when(width.is_none(), |cell| cell.flex_1())
                .bg(if j == 0 { first } else { normal })
                .when(j != 2, |cell| cell.text_center())
                .when(j == 1, |cell| cell.truncate())
                .child(text);
            row = row.child(cell);
        }
        row.into_any_element()
    }

    pub(super) fn diagnostic_sort_header(
        &self, id: &'static str, label: String, order: Option<bool>, height: f32,
        cx: &Context<Self>, handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        self.diagnostic_button(self.btn(id, "", "", cx, handler)).relative().w(px(180.)).h(px(height)).p_0()
            .ghost().rounded(px(0.)).border_0().font_weight(FontWeight::MEDIUM)
            .text_color(if self.dark(cx) { rgba(0xffffffe6) } else { rgb(0x1f2225) })
            .bg(self.diagnostic_header_background(order.is_some(), cx))
            .child(self.diagnostic_separator(cx))
            .child(div().w_full().pr(px(20.)).text_center().child(label))
            .child(div().absolute().right(px(12.)).top(px((height-15.)/2.)).size(px(15.))
                .flex().items_center().justify_center()
                .text_color(if order.is_some() { cx.theme().primary } else if self.dark(cx) { rgba(0xffffff61).into() } else { rgb(0xc2c2c2).into() })
                .child(icon(if order == Some(true) { "entry-sort-up" } else { "entry-sort-down" }).size(px(15.))))
            .into_any_element()
    }
}
