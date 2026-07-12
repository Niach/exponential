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
//! Phase-3 state: the §4 app shell — [`Workspace`] (the `DockArea`) with the
//! non-collapsible [`sidebar`] in the left dock (live workspace
//! picker + nav rows + project rows), the [`screens`] panel in the center
//! (per-window [`navigation`] routing: board / issue detail / my-issues /
//! inbox / settings / account — §4.2), the virtualized [`issue_list`] core
//! with inline status/priority dropdowns (§4.6), a collapsed bottom terminal
//! dock, per-window `DockAreaState` persistence (§3.3), plus the [`login`]
//! surface + [`session`] wiring (the §5 state machine: the workspace renders
//! login whenever the session is not `Synced`). The Phase-2 [`debug_board`]
//! stays reachable behind `EXP_DEV_BOARD=1`.

mod actions;
mod active_filter_pills;
mod attachments_row;
mod board;
pub mod coding_flow;
mod coding_selects;
mod comments;
mod create_issue_dialog;
mod create_project_dialog;
mod create_workspace_dialog;
mod debug_board;
mod description_editor;
pub mod diff;
mod feedback;
mod file_tree;
mod file_viewer;
mod filter_bar;
mod filter_popover;
mod flow_lanes;
mod flow_view;
mod git_bar;
mod github_connect;
mod icons;
mod image_preview;
mod inbox;
mod issue_changes;
pub mod issue_detail;
mod issue_list;
mod join_workspace;
mod login;
pub mod markdown;
mod mention_input;
mod navigation;
mod oauth;
mod properties_panel;
mod queries;
mod release_add_issues_dialog;
mod release_create_dialog;
mod repo_resolver;
mod run_bar;
mod screens;
mod scroll_pane;
mod search_sheet;
mod session;
mod settings;
mod sidebar;
mod source_control;
mod start_coding_dialog;
pub mod steer_wiring;
mod terminal_dock;
mod timeline;
mod top_bar;
mod undock;
mod undocked_terminal;
mod update;
mod workspace;
mod workspace_heal;

pub use actions::*;
pub use icons::ExpIcon;
pub use navigation::{navigate, Screen};
pub use oauth::handle_open_urls;
pub use update::check_for_updates;
pub use session::{bootstrap as bootstrap_session, sign_out_active, AuthContext};
pub use workspace::Workspace;

use gpui::{App, AppContext as _};
use gpui_component::dock::register_panel;

/// Register the panel-name → constructor registry entries (§3.3:
/// "`DockArea::load(state)` reconstructs panels by name") and the App-global
/// navigation action handlers (§4.2). Must run once at bootstrap, after
/// `gpui_component::init(cx)` and before any window opens.
pub fn init(cx: &mut App) {
    navigation::init(cx);
    // EXP-65 multi-window undock: the observable registry the screens panel
    // and terminal dock filter against.
    undock::init(cx);
    // §4.5 seam: the issue-detail description edits through the real GFM
    // block editor (factory installed before any window can render a detail).
    description_editor::install(cx);
    // ⌘K quick-open (§4.2 IssueSearchSheet): global OpenSearch handler +
    // keybinding.
    search_sheet::init(cx);
    // EXP-48 issue switcher: J/K bindings scoped to the detail's key context
    // (guarded against focused editables — see issue_detail::init).
    issue_detail::init(cx);
    // Bulk select (release rework Phase 7): cmd-a/ctrl-a select-all +
    // escape clear, scoped to the issue list's key context.
    issue_list::init(cx);
    // Create-flow dialog actions (§4.2): NewIssue (board filter bar),
    // NewProject (sidebar `+`), CreateWorkspace (workspace picker).
    create_issue_dialog::init(cx);
    create_project_dialog::init(cx);
    create_workspace_dialog::init(cx);
    // §4.2 accept-invite fallback: "Join workspace…" in the footer account
    // menu (the exponential://invite/<token> deep link routes through oauth.rs).
    join_workspace::init(cx);
    // The sidebar Feedback item joins + opens the public feedback board
    // IN-APP (v6 self-service `workspaceMembers.join`, mirroring the web join
    // gate), falling back to the cloud `/feedback` page in the system browser
    // when the board is unavailable (signed out / self-hosted instance).
    feedback::init(cx);
    register_panel(cx, workspace::CENTER_PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| workspace::CenterPanel::new(window, cx)))
    });
    register_panel(cx, screens::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| screens::ScreensPanel::new(window, cx)))
    });
    // Terminal dock: panel registration (cold shell-tab restore, §6.13) +
    // the cmd-t/cmd-w/ctrl-tab keybindings scoped to the dock.
    terminal_dock::init(cx);
    // EXP-71: shadow Root's window-wide tab/shift-tab focus-cycle bindings
    // inside the terminal so they reach the PTY (shift+tab = Claude modes).
    terminal::init(cx);
    register_panel(cx, debug_board::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| debug_board::DebugBoardPanel::new(window, cx)))
    });
}
