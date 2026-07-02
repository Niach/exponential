//! Workspace-chrome gpui actions (masterplan-v3 §3.6 "Actions & keymap").
//!
//! Phase-1 note: the sidebar dispatches these as **placeholders** — nothing
//! handles them yet. Phase 3 wires them to real navigation/dialogs (board,
//! inbox, search sheet, create-project/-workspace dialogs, settings) and
//! Phase 2 wires the account ones (sign in/out). Declaring them now keeps the
//! chrome honest (every affordance dispatches a typed action, never an inline
//! closure doing view surgery) and lets the §3.6 keymap/menubar bind them.

use gpui::actions;

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
        /// Sidebar footer: send feedback (EXP-1 #10).
        SendFeedback,
        /// Workspace picker: switch workspace (EXP-1 #1).
        SelectWorkspace,
        /// Workspace picker: create a new workspace.
        CreateWorkspace,
        /// Footer account dropdown: open settings (EXP-1 #11).
        OpenSettings,
        /// Footer account dropdown: sign out (Phase 2 auth).
        SignOut,
    ]
);
