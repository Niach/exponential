//! `ui` — every gpui view (masterplan-v3 §3.1 / §04).
//!
//! A 1:1 mirror of the web app built out of gpui-component widgets: `sidebar`,
//! `issue_list` (virtualized), `issue_detail`, `markdown_editor` +
//! `mention_popover`, `filter_bar`/`pills`, `create_issue_dialog`,
//! `create_project`/`create_workspace`, `inbox`, `my_issues`, `settings/*`,
//! `account`, `diff_view`, `run_bar`. Lands across Phases 1–5.
//!
//! Dependency rule (§3.1): lower crates never depend on `ui` (no back-edges).
//!
//! Phase-1 state: the §4 app shell — [`Workspace`] (the `DockArea`) with the
//! non-collapsible [`sidebar`] in the left dock (EXP-1 #8), an empty center
//! `TabPanel` area, a collapsed bottom terminal dock placeholder, and
//! per-window `DockAreaState` persistence (§3.3). Screens land in Phase 3.

mod actions;
mod sidebar;
mod terminal_dock;
mod workspace;

pub use actions::*;
pub use workspace::Workspace;

use gpui::{App, AppContext as _};
use gpui_component::dock::register_panel;

/// Register the panel-name → constructor registry entries (§3.3:
/// "`DockArea::load(state)` reconstructs panels by name"). Must run once at
/// bootstrap, after `gpui_component::init(cx)` and before any window opens.
pub fn init(cx: &mut App) {
    register_panel(cx, sidebar::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| sidebar::SidebarPanel::new(window, cx)))
    });
    register_panel(cx, terminal_dock::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| terminal_dock::TerminalDockPanel::new(window, cx)))
    });
}
