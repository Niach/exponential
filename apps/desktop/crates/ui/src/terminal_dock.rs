//! Bottom terminal-dock placeholder (masterplan-v3 §3.3 / §3.10).
//!
//! §3.3: "bottom dock = the multi-tab terminal (§06), a `TabPanel` of terminal
//! panels." The real terminal (alacritty + PTY tee + JetBrains-style tabs)
//! is Phase 4's; Phase 1 only needs the dock to exist, render at compact
//! density, and start **collapsed** (the dock keeps its 29px toggle strip when
//! closed, so the user can open it). This panel is the single placeholder tab
//! inside that dock — Phase 4 replaces it with real `TerminalPanel`s and bumps
//! the workspace layout version so stale persisted layouts rebuild.

use gpui::{
    div, App, FocusHandle, Focusable, IntoElement, ParentElement, Render, Styled, Window,
};
use gpui_component::{
    dock::{Panel, PanelControl, PanelEvent},
    v_flex, ActiveTheme as _, Icon, IconName,
};

/// Stable serialization name for the panel registry (§3.3).
pub const PANEL_NAME: &str = "TerminalDock";

/// Empty placeholder panel for the bottom terminal dock.
pub struct TerminalDockPanel {
    focus_handle: FocusHandle,
}

impl TerminalDockPanel {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for TerminalDockPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Terminal"
    }

    /// The dock placeholder cannot be closed — the terminal dock is fixed
    /// chrome (collapse/expand is the Dock's own toggle, not a tab close).
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for TerminalDockPanel {}

impl Focusable for TerminalDockPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalDockPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .text_color(cx.theme().muted_foreground)
            .child(Icon::new(IconName::SquareTerminal))
            .child(div().text_sm().child("No terminal sessions"))
    }
}
