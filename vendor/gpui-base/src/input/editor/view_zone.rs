//! An application-owned visual block between editor display rows.
//! The block belongs to layout, not the buffer or its undo history.
use crate::input::{BufferPoint, EditorState, InputBaseState, InputModeKind, RopeExt as _};
use gpui::{AnyElement, App, Context, Pixels, Window, px};
use std::rc::Rc;

#[derive(Clone)]
pub struct EditorViewZone {
    /// Byte offset whose wrapped display row precedes the block.
    pub after: usize,
    pub height: Pixels,
    pub right_inset: Pixels,
    pub render: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewZoneGeometry {
    pub after_row: usize,
    pub height: Pixels,
    pub line_height: Pixels,
}

impl ViewZoneGeometry {
    pub(crate) fn top(self) -> Pixels {
        self.line_height * (self.after_row + 1)
    }
    pub(crate) fn content_y(self, plain: Pixels) -> Pixels {
        plain
            + if plain >= self.top() {
                self.height
            } else {
                px(0.)
            }
    }
    pub(crate) fn text_y(self, content: Pixels) -> Pixels {
        if content <= self.top() {
            content
        } else if content < self.top() + self.height {
            self.top()
        } else {
            content - self.height
        }
    }
}

impl EditorState {
    pub fn set_view_zone(&mut self, zone: Option<EditorViewZone>, cx: &mut Context<Self>) {
        self.view_zone = zone;
        cx.notify();
    }
    pub fn view_zone_after(&self) -> Option<usize> {
        self.view_zone.as_ref().map(|zone| zone.after)
    }
    pub fn has_view_zone(&self) -> bool {
        self.view_zone.is_some()
    }
}

impl<M: InputModeKind> InputBaseState<M> {
    pub(crate) fn view_zone_geometry(&self, line_height: Pixels) -> Option<ViewZoneGeometry> {
        let zone = self.view_zone.as_ref()?;
        let point = self.text.offset_to_point(zone.after.min(self.text.len()));
        let row = self
            .display_map
            .buffer_pos_to_display_pos(BufferPoint::new(point.row, point.column))
            .row;
        Some(ViewZoneGeometry {
            after_row: row,
            height: zone.height.max(px(0.)),
            line_height,
        })
    }
    pub(crate) fn content_y_for_row(&self, row: usize, line_height: Pixels) -> Pixels {
        let plain = line_height * row;
        self.view_zone_geometry(line_height)
            .map_or(plain, |zone| zone.content_y(plain))
    }
}
