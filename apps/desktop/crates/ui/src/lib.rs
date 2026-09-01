//! `ui` — every gpui view (masterplan-v3 §3.1 / §04).
//!
//! A 1:1 mirror of the web app built out of gpui-component widgets: `sidebar`,
//! `issue_list` (virtualized), `issue_detail`, `markdown_editor` +
//! `mention_popover`, `filter_bar`/`pills`, `create_issue_dialog`,
//! `create_board`/`create_team`, `inbox`, `my_issues`, `settings/*`,
//! `account`, `diff_view`, `actions_view`. Lands across Phases 1–5.
//!
//! Dependency rule (§3.1): lower crates never depend on `ui` (no back-edges).
//!
//! Phase-3 state: the §4 app shell — [`Shell`] (the `DockArea`) with the
//! non-collapsible [`sidebar`] in the left dock (live team
//! picker + nav rows + board rows), the [`screens`] panel in the center
//! (per-window [`navigation`] routing: board / issue detail / my-issues /
//! inbox / settings / account — §4.2), the virtualized [`issue_list`] core
//! with inline status/priority dropdowns (§4.6), a collapsed bottom terminal
//! dock, per-window `DockAreaState` persistence (§3.3), plus the [`login`]
//! surface + [`session`] wiring (the §5 state machine: the shell renders
//! login whenever the session is not `Synced`). The Phase-2 [`debug_board`]
//! stays reachable behind `EXP_DEV_BOARD=1`.

mod action_editor_dialog;
mod agent_login;
mod action_run;
mod action_suggestions;
mod actions;
mod actions_view;
mod active_filter_pills;
mod automation_host;
mod automations_view;
mod app_title_bar;
mod attachments_row;
mod automation_dialog;
mod automation_editor;
mod board;
mod board_form;
pub mod coding_flow;
mod coding_selects;
mod comment_attachments;
mod comments;
mod commit_graph;
mod controls;
mod create_action_dialog;
mod create_issue_dialog;
mod create_board_dialog;
mod create_team_dialog;
mod debug_board;
mod dev_ready;
mod description_editor;
mod emoji;
mod emoji_picker;
pub mod diff;
mod file_tree;
mod file_viewer;
mod filter_bar;
mod filter_popover;
mod getting_started;
mod graceful_stop;
mod github_connect;
mod icons;
mod image_preview;
mod inbox;
pub mod issue_detail;
mod issue_files;
mod issue_header;
mod issue_list;
mod join_team;
mod launch_options;
pub mod licenses;
mod login;
mod device_settings;
mod devices_view;
mod device_sync;
mod machines;
#[cfg(target_os = "macos")]
pub mod macos_blur;
pub mod markdown;
mod mention_input;
mod wysiwyg;
mod native_dialog;
mod navigation;
mod oauth;
mod onboarding;
pub mod os_notifications;
mod pickers;
mod pr_diff;
mod pr_merge;
mod queries;
mod repo_resolver;
mod screens;
mod scroll_pane;
mod search_sheet;
mod session;
mod session_registry;
mod settings;
mod sidebar;
mod surface;
mod source_control;
mod start_coding_dialog;
mod steer_viewer;
pub mod steer_wiring;
mod support_thread;
mod terminal_dock;
mod worktree_prune;
mod trunk_sync;
mod timeline;
mod title_bar;
mod undock;
mod usage_bar;
mod undocked_terminal;
mod update;
mod user_avatar;
mod window_frame;
mod window_hooks;
#[cfg(target_os = "linux")]
mod x11_dialog_type;
pub mod window_size;
mod shell;

pub use actions::*;
pub use icons::ExpIcon;
pub use navigation::{navigate, Screen};
pub use oauth::handle_open_urls;
pub use dev_ready::install_dev_ready_probe;
pub use update::check_for_updates;
pub use session::{
    bootstrap as bootstrap_session, sign_out_active, upgrade_required_handler, AuthContext,
};
pub use session_registry::install_end_observer as install_session_end_observer;
pub use shell::Shell;
pub use window_hooks::{set_open_shell_window_hook, set_window_opened_hook};

use gpui::{App, AppContext as _};
use gpui_component::dock::register_panel;

/// Register the panel-name → constructor registry entries (§3.3:
/// "`DockArea::load(state)` reconstructs panels by name") and the App-global
/// navigation action handlers (§4.2). Must run once at bootstrap, after
/// `gpui_component::init(cx)` and before any window opens.
pub fn init(cx: &mut App) {
    navigation::init(cx);
    // EXP-105: quit-time sweep ending every coding_sessions row this process
    // launched — without it a closed IDE ghosts the "coding now" badge on
    // every client until the server staleness sweep catches it.
    coding_flow::install_quit_hook(cx);
    // EXP-65 multi-window undock: the observable registry the screens panel
    // and terminal dock filter against.
    undock::init(cx);
    // EXP-284: native dialog windows — Escape/Enter bindings + the
    // dialog-window → opener registry every dialog opens through.
    native_dialog::init(cx);
    // §4.5 seam: the issue-detail description edits through the vendored
    // WYSIWYG editor (EXP-261; factory installed before any window can
    // render a detail). The block editor stays for the comment composer.
    description_editor::install(cx);
    // EXP-261: the vendored editor's key bindings (its own action set,
    // scoped to the "WysiwygMarkdownEditor" key context so they can never
    // shadow the classic block editor's bindings).
    cx.bind_keys(gpui_markdown_editor::default_key_bindings());
    // ⌘K quick-open (§4.2 IssueSearchSheet): global OpenSearch handler +
    // keybinding.
    search_sheet::init(cx);
    // (The EXP-48 J/K issue-switcher bindings were removed in EXP-268 —
    // bare-letter shortcuts kept eating typed letters in editors.)
    // Bulk select: cmd-a/ctrl-a select-all +
    // escape clear, scoped to the issue list's key context.
    issue_list::init(cx);
    // Create-flow dialog actions (§4.2): NewIssue (board filter bar),
    // NewBoard (sidebar `+`), CreateTeam (team picker).
    create_issue_dialog::init(cx);
    create_board_dialog::init(cx);
    create_team_dialog::init(cx);
    // §4.2 accept-invite fallback: "Join team…" in the footer account
    // menu (the exponential://invite/<token> deep link routes through oauth.rs).
    join_team::init(cx);
    register_panel(cx, shell::CENTER_PANEL_NAME, |_, _, _, window, cx| {
        Box::new(cx.new(|cx| shell::CenterPanel::new(window, cx)))
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
