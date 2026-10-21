# Tiny RDM menu appearance

Based on Apache-2.0 gpui-component 0.7.3 from crates.io.

The optional `PopupMenu::tiny_rdm_style(context_labels)` preset preserves the
menu action and keyboard implementation while matching Tiny RDM dropdown
geometry, colors, shadows, dividers and icon sizes. The default component
appearance is unchanged. Dropdown triggers can additionally specify an offset.

`Button::allow_icon_overflow()` supports the original footer icons that extend
beyond their compact hit area. Default button clipping is unchanged.

`Button::content_style(style, icon_size)` is exposed for application-owned
content presentation. The desktop key toolbar uses 14px text, a 6px icon gap
and 18px icons without changing button interaction or other callers' defaults.

`Button::hover_style` and `Button::active_style` refine the existing state
presentation. They let the key toolbar match the original colors without
registering a second hover handler; unset refinements preserve existing styles.

`Editor::editor_style` and `Editor::editor_paddings` pass optional presentation
overrides through `Input`. The JSON view supplies its own syntax resolver,
gutter layout, colors and zero canvas padding. Other editors retain the shared
theme and size-derived padding.
