//! Workspace-chrome gpui actions (masterplan-v3 §3.6 "Actions & keymap").
//!
//! Phase-3 state: the navigation actions are handled by App-global listeners
//! registered in [`crate::navigation::init`] (they resolve the active window
//! and swap the center screen — §4.2). Dialog/sheet actions (create
//! project/workspace, search, feedback) get their handlers with their
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
        /// Sidebar "Projects" group header `+`: create project.
        NewProject,
        /// Board filter bar "New Issue" (§4.2): open the create-issue dialog
        /// (handler lands with the dialog).
        NewIssue,
        /// Sidebar footer: send feedback.
        SendFeedback,
        /// Workspace picker: create a new workspace.
        CreateWorkspace,
        /// Footer account dropdown: open settings.
        OpenSettings,
        /// Footer account dropdown: open the Account screen (§4.2
        /// integrations + notification prefs).
        OpenAccount,
        /// Footer account dropdown: join a workspace by invite link/token
        /// (§4.2 accept-invite fallback — desktop can't catch the browser's
        /// `/invite/<token>` click).
        JoinWorkspace,
        /// Footer account dropdown: sign out (Phase 2 auth).
        SignOut,
        /// Open the Source Control tool + changes screen (branch chip menu,
        /// commit button).
        OpenSourceControl,
        /// Branch chip menu: manual freshness sync of the active project's
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

/// Top-bar branch chip menu: check out a local branch on the active
/// project's trunk clone.
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct SwitchBranch {
    pub branch: String,
}

/// Sidebar project row / anywhere that opens a project board (§4.2).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenProject {
    pub project_id: String,
}

/// Open an issue's full-page detail (row click, #IDENT pills, inbox cards).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct OpenIssue {
    pub issue_id: String,
}

/// Switch the window's active workspace. Dispatched from the top bar's
/// merged project picker (EXP-69) for project-less workspaces; workspaces
/// with projects switch implicitly via [`OpenProject`].
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct SwitchWorkspace {
    pub workspace_id: String,
}
