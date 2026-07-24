//! Team-chrome gpui actions (masterplan-v3 §3.6 "Actions & keymap").
//!
//! Phase-3 state: the navigation actions are handled by App-global listeners
//! registered in [`crate::navigation::init`] (they resolve the active window
//! and swap the center screen — §4.2). Dialog/sheet actions (create
//! board/team, search) get their handlers with their
//! dialogs; the account ones (`SignOut`) were wired in Phase 2. Every chrome
//! affordance dispatches a typed action, never an inline closure doing view
//! surgery, so the §3.6 keymap/menubar can bind them.

use gpui::{actions, Action};
use serde::Deserialize;

actions!(
    exp,
    [
        /// Sidebar nav: open the My Issues view.
        OpenMyIssues,
        /// Sidebar nav: open the Inbox view.
        OpenInbox,
        /// Sidebar nav: open the search sheet (⌘K in Phase 3).
        OpenSearch,
        /// Window chrome / keymap: pop the per-window back stack (§8.11 —
        /// `cmd-[` / `Alt+Left`; the back button dispatches this too).
        GoBack,
        /// Sidebar "Boards" group header `+`: create board.
        NewBoard,
        /// Board filter bar "New Issue" (§4.2): open the create-issue dialog
        /// (handler lands with the dialog).
        NewIssue,
        /// Team picker: create a new team.
        CreateTeam,
        /// Footer account dropdown: open settings.
        OpenSettings,
        /// Footer account dropdown: open the Account screen (§4.2
        /// integrations + notification prefs).
        OpenAccount,
        /// Footer account dropdown: join a team by invite link/token
        /// (§4.2 accept-invite fallback — desktop can't catch the browser's
        /// `/invite/<token>` click).
        JoinTeam,
        /// Footer account dropdown: sign out (Phase 2 auth).
        SignOut,
        /// Open the Source Control tool + changes screen (branch chip menu,
        /// commit button).
        OpenSourceControl,
        /// Branch chip menu: manual freshness sync of the active board's
        /// trunk (fetch + ff-only catch-up) — the compact bar has no standing
        /// "Up to date" button.
        SyncNow,
        /// Issue detail (EXP-48): swap to the NEXT issue in the active
        /// list's filtered ordering (`j`, scoped to the detail's key
        /// context).
        NextIssue,
        /// Issue detail (EXP-48): swap to the PREVIOUS issue (`k`).
        PrevIssue,
    ]
);


/// Sidebar board row / anywhere that opens a board view (§4.2).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenBoard {
    pub board_id: String,
}

/// Open an issue's full-page detail (row click, #IDENT pills, inbox cards).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenIssue {
    pub issue_id: String,
}

/// Switch the window's active team. Dispatched from the top bar's
/// merged board picker (EXP-69) for board-less teams; teams
/// with boards switch implicitly via [`OpenBoard`].
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct SwitchTeam {
    pub team_id: String,
}
