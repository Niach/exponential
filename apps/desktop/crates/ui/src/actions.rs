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
        /// Sidebar nav: open the My Issues view (EXP-1 #3).
        OpenMyIssues,
        /// Sidebar nav: open the Inbox view (EXP-1 #3).
        OpenInbox,
        /// Sidebar nav: open the search sheet (EXP-1 #3, ⌘K in Phase 3).
        OpenSearch,
        /// Sidebar "Projects" group header `+` (EXP-1 #2): create project.
        NewProject,
        /// Board filter bar "New Issue" (§4.2): open the create-issue dialog
        /// (handler lands with the dialog).
        NewIssue,
        /// Sidebar footer: send feedback (EXP-1 #10).
        SendFeedback,
        /// Workspace picker: create a new workspace.
        CreateWorkspace,
        /// Footer account dropdown: open settings (EXP-1 #11).
        OpenSettings,
        /// Footer account dropdown: open the Account screen (§4.2
        /// integrations + notification prefs).
        OpenAccount,
        /// Footer account dropdown: sign out (Phase 2 auth).
        SignOut,
    ]
);

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

/// Workspace picker: switch the window's active workspace (EXP-1 #1).
#[derive(Clone, Action, PartialEq, Eq, Deserialize)]
#[action(namespace = exp, no_json)]
pub struct SwitchWorkspace {
    pub workspace_id: String,
}
