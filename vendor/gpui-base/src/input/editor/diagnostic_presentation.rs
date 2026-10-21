//! Host-owned diagnostic surfaces and timing. The engine owns pointer lifetime;
//! the host renders its hover and any problem-navigation view zone.
use crate::input::{DiagnosticEntry, EditorState, InputBaseState, InputModeKind};
use gpui::{AnyElement, App, Bounds, Context, Entity, Pixels, Task, Window};
use std::{rc::Rc, time::Duration};

pub type DiagnosticHoverRenderer =
    Rc<dyn Fn(Entity<EditorState>, Vec<DiagnosticEntry>, &mut Window, &mut App) -> AnyElement>;
pub type DiagnosticNavigationHandler =
    Rc<dyn Fn(&mut EditorState, bool, &mut Window, &mut Context<EditorState>)>;
pub type DiagnosticDismissHandler =
    Rc<dyn Fn(&mut EditorState, &mut Window, &mut Context<EditorState>) -> bool>;
pub type DiagnosticsChangedHandler = Rc<dyn Fn(&mut EditorState, &mut Context<EditorState>)>;

#[derive(Clone)]
pub struct DiagnosticPresentation {
    pub attached: Rc<dyn Fn()>,
    pub show_delay: Duration,
    pub hide_delay: Duration,
    pub render_hover: DiagnosticHoverRenderer,
    pub navigate: DiagnosticNavigationHandler,
    pub dismiss: DiagnosticDismissHandler,
    pub changed: DiagnosticsChangedHandler,
}

gpui::actions!(
    input,
    [NextDiagnostic, PreviousDiagnostic, ShowDiagnosticHover]
);
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-k cmd-i"
            } else {
                "ctrl-k ctrl-i"
            },
            ShowDiagnosticHover,
            Some("Input && mode == editor"),
        ),
        gpui::KeyBinding::new("alt-f8", NextDiagnostic, Some("Input && mode == editor")),
        gpui::KeyBinding::new(
            "alt-shift-f8",
            PreviousDiagnostic,
            Some("Input && mode == editor"),
        ),
    ]);
}

impl EditorState {
    /// Opens immediately at the caret, or requests focus when already visible.
    pub(crate) fn show_diagnostic_hover(
        &mut self,
        _: &ShowDiagnosticHover,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diagnostic_presentation.is_none() {
            cx.propagate();
            return;
        }
        if self.diagnostic_popover.is_some() {
            self.keep_diagnostic_hover_open();
            self.diagnostic_hover_focus_requested = true;
            cx.notify();
            return;
        }
        let offset = self.cursor();
        let entries: Vec<_> = self
            .mode
            .diagnostics()
            .map(|set| set.range(offset..offset + 1).cloned().collect())
            .unwrap_or_default();
        if entries.is_empty() {
            return;
        }
        self.diagnostic_show_task = Task::ready(());
        self.keep_diagnostic_hover_open();
        self.diagnostic_hover_candidate.clear();
        self.diagnostic_popover = entries.first().cloned().map(Rc::new);
        self.diagnostic_hover_entries = entries;
        self.diagnostic_hover_keyboard = true;
        self.diagnostic_hover_focus_requested = false;
        self.diagnostic_hover_focused = false;
        cx.notify();
    }
    pub fn diagnostic_hover_from_keyboard(&self) -> bool {
        self.diagnostic_hover_keyboard
    }
    /// The renderer consumes a request once and then focuses its own surface.
    pub fn take_diagnostic_hover_focus_request(&mut self) -> bool {
        let requested = std::mem::take(&mut self.diagnostic_hover_focus_requested);
        self.diagnostic_hover_focused |= requested;
        requested
    }
    pub fn set_diagnostic_presentation(&mut self, presentation: DiagnosticPresentation) {
        self.diagnostic_presentation = Some(presentation);
    }
    pub fn editor_font(&self) -> Option<(gpui::Font, Pixels)> {
        self.last_editor_font.clone()
    }
    pub fn diagnostic_presentation(&self) -> Option<&DiagnosticPresentation> {
        self.diagnostic_presentation.as_ref()
    }
    pub fn diagnostic_hover_entries(&self, first: &DiagnosticEntry) -> Vec<DiagnosticEntry> {
        if self.diagnostic_hover_entries.is_empty() {
            vec![first.clone()]
        } else {
            self.diagnostic_hover_entries.clone()
        }
    }
    pub fn set_diagnostic_popover_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.diagnostic_popup_bounds = Some(bounds);
    }
    pub fn keep_diagnostic_hover_open(&mut self) {
        self.diagnostic_hide_task = Task::ready(());
        self.diagnostic_hide_pending = false;
    }
    pub fn diagnostics_did_change(&mut self, cx: &mut Context<Self>) {
        if let Some(presentation) = self.diagnostic_presentation.clone() {
            (presentation.changed)(self, cx);
        }
    }
    pub(crate) fn next_diagnostic(
        &mut self,
        _: &NextDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(presentation) = self.diagnostic_presentation.clone() {
            (presentation.navigate)(self, true, window, cx);
        } else {
            cx.propagate();
        }
    }
    pub(crate) fn previous_diagnostic(
        &mut self,
        _: &PreviousDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(presentation) = self.diagnostic_presentation.clone() {
            (presentation.navigate)(self, false, window, cx);
        } else {
            cx.propagate();
        }
    }
}

impl<M: InputModeKind> InputBaseState<M> {
    pub(crate) fn cancel_diagnostic_hover(&mut self) -> bool {
        let changed = self.diagnostic_popover.take().is_some();
        self.diagnostic_hover_candidate.clear();
        self.diagnostic_hover_entries.clear();
        self.diagnostic_hover_keyboard = false;
        self.diagnostic_hover_focus_requested = false;
        self.diagnostic_hover_focused = false;
        self.diagnostic_popup_bounds = None;
        self.diagnostic_hide_pending = false;
        self.diagnostic_show_task = Task::ready(());
        self.diagnostic_hide_task = Task::ready(());
        changed
    }
    pub(crate) fn hide_diagnostic_after_delay(&mut self, cx: &mut Context<Self>) {
        self.diagnostic_hover_candidate.clear();
        self.diagnostic_show_task = Task::ready(());
        let Some(presentation) = &self.diagnostic_presentation else {
            return;
        };
        if self.diagnostic_popover.is_none() || self.diagnostic_hide_pending {
            return;
        }
        self.diagnostic_hide_pending = true;
        let delay = presentation.hide_delay;
        self.diagnostic_hide_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |state, cx| {
                if state.cancel_diagnostic_hover() {
                    cx.notify();
                }
            });
        });
    }
    pub(crate) fn update_diagnostic_hover(&mut self, offset: usize, cx: &mut Context<Self>) {
        let Some(presentation) = &self.diagnostic_presentation else {
            return;
        };
        let entries: Vec<_> = self
            .mode
            .diagnostics()
            .map(|set| set.range(offset..offset + 1).cloned().collect())
            .unwrap_or_default();
        if entries.is_empty() {
            self.hide_diagnostic_after_delay(cx);
            return;
        }
        self.diagnostic_hide_task = Task::ready(());
        self.diagnostic_hide_pending = false;
        if self.diagnostic_hover_entries == entries || self.diagnostic_hover_candidate == entries {
            return;
        }
        self.diagnostic_hover_candidate = entries.clone();
        self.diagnostic_hover_keyboard = false;
        self.diagnostic_hover_focused = false;
        let delay = presentation.show_delay;
        self.diagnostic_show_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |state, cx| {
                state.diagnostic_popover = entries.first().cloned().map(Rc::new);
                state.diagnostic_hover_entries = entries;
                state.diagnostic_hover_candidate.clear();
                cx.notify();
            });
        });
    }
}
