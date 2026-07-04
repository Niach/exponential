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
//! non-collapsible [`sidebar`] in the left dock (EXP-1 #8, live workspace
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
mod comments;
mod create_issue_dialog;
mod create_project_dialog;
mod create_workspace_dialog;
mod debug_board;
mod description_editor;
pub mod diff;
mod file_tree;
mod file_viewer;
mod filter_bar;
mod filter_popover;
mod git_bar;
mod icons;
mod inbox;
mod issue_changes;
pub mod issue_detail;
mod issue_list;
mod join_workspace;
mod login;
pub mod markdown;
mod mention_input;
mod my_issues;
mod navigation;
mod oauth;
mod properties_panel;
mod queries;
mod repo_resolver;
mod run_bar;
mod screens;
mod search_sheet;
mod session;
mod settings;
mod sidebar;
mod source_control;
pub mod steer_wiring;
mod terminal_dock;
mod timeline;
mod workspace;

pub use actions::*;
pub use icons::ExpIcon;
pub use navigation::{navigate, Screen};
pub use oauth::handle_open_urls;
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
    // §4.5 seam: the issue-detail description edits through the real GFM
    // block editor (factory installed before any window can render a detail).
    description_editor::install(cx);
    // ⌘K quick-open (§4.2 IssueSearchSheet): global OpenSearch handler +
    // keybinding.
    search_sheet::init(cx);
    // Create-flow dialog actions (§4.2): NewIssue (board filter bar),
    // NewProject (sidebar `+`), CreateWorkspace (workspace picker).
    create_issue_dialog::init(cx);
    create_project_dialog::init(cx);
    create_workspace_dialog::init(cx);
    // §4.2 accept-invite fallback: "Join workspace…" in the footer account
    // menu (the exp://invite/<token> deep link routes through oauth.rs).
    join_workspace::init(cx);
    // EXP-1 #10: the sidebar Feedback item opens the public feedback project
    // in the system browser (the §4.8 browser-path decision; the embedded JS
    // widget is an explicit desktop non-goal for v1). `/feedback` redirects
    // server-side to the current feedback project.
    cx.on_action(|_: &SendFeedback, cx| {
        let url = queries::active_account(cx)
            .map(|account| {
                format!("{}/feedback", account.instance_url.trim_end_matches('/'))
            })
            .unwrap_or_else(|| "https://app.exponential.at/feedback".to_string());
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = api::opener::open_in_browser(&url) {
                    log::warn!("[ui] feedback: browser open failed: {err}");
                }
            })
            .detach();
    });
    register_panel(cx, sidebar::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| sidebar::SidebarPanel::new(window, cx)))
    });
    register_panel(cx, screens::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| screens::ScreensPanel::new(window, cx)))
    });
    // Terminal dock: panel registration (cold shell-tab restore, §6.13) +
    // the cmd-t/cmd-w/ctrl-tab keybindings scoped to the dock.
    terminal_dock::init(cx);
    register_panel(cx, debug_board::PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| debug_board::DebugBoardPanel::new(window, cx)))
    });
}
