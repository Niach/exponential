//! EXP-1029 contract — every IDE styleguide entry under the four sections,
//! one file each, registered here by fixed name. A leaf fills ITS file.

use gpui::{App, Div, Window};

pub(crate) mod picker;
pub(crate) mod picker_board;
pub(crate) mod picker_issue;
pub(crate) mod picker_action;
pub(crate) mod picker_account;
pub(crate) mod picker_device;
pub(crate) mod picker_assignee;
pub(crate) mod picker_icon;
pub(crate) mod picker_status;
pub(crate) mod picker_priority;
pub(crate) mod picker_label;
pub(crate) mod picker_mcp;
pub(crate) mod picker_repository;
pub(crate) mod sub_shell;
pub(crate) mod menu;
pub(crate) mod toast;
pub(crate) mod jump_to_bottom;
pub(crate) mod composer_dialog;
pub(crate) mod issue_context_menu;
pub(crate) mod session_tree;
pub(crate) mod pr_graph_badge;
pub(crate) mod device_settings;
pub(crate) mod blocked_start_dialog;
pub(crate) mod stack_merge_choice_dialog;
pub(crate) mod readiness_checklist;
pub(crate) mod results_guide;
pub(crate) mod mcp_app_views;

/// EXP-1030: whether `id`'s file is still the one-line placeholder rather
/// than a demo. None is any more: EXP-1031 filled the last one (`toast`).
/// The check stays so a future entry can land as a placeholder first.
pub(crate) fn is_placeholder(_id: &str) -> bool {
    false
}

/// A demo: the entry's element, built with the window and the app so it
/// can be the REAL component (EXP-1063) — every glass recipe takes the theme
/// from `cx`, a select or a nav stack needs an entity, and
/// `window.use_keyed_state` keeps that entity alive across frames.
pub(crate) type Render = fn(&mut Window, &mut App) -> Div;

/// One entry: its id (the section index's), its owner, its demo.
pub(crate) struct Entry {
    pub id: &'static str,
    pub owner: &'static str,
    pub render: Render,
}

/// Every entry, in the section index's order.
pub(crate) const ENTRIES: &[Entry] = &[
    Entry { id: picker::ID, owner: picker::OWNER, render: picker::render },
    Entry { id: picker_board::ID, owner: picker_board::OWNER, render: picker_board::render },
    Entry { id: picker_issue::ID, owner: picker_issue::OWNER, render: picker_issue::render },
    Entry { id: picker_action::ID, owner: picker_action::OWNER, render: picker_action::render },
    Entry { id: picker_account::ID, owner: picker_account::OWNER, render: picker_account::render },
    Entry { id: picker_device::ID, owner: picker_device::OWNER, render: picker_device::render },
    Entry { id: picker_assignee::ID, owner: picker_assignee::OWNER, render: picker_assignee::render },
    Entry { id: picker_icon::ID, owner: picker_icon::OWNER, render: picker_icon::render },
    Entry { id: picker_status::ID, owner: picker_status::OWNER, render: picker_status::render },
    Entry { id: picker_priority::ID, owner: picker_priority::OWNER, render: picker_priority::render },
    Entry { id: picker_label::ID, owner: picker_label::OWNER, render: picker_label::render },
    Entry { id: picker_mcp::ID, owner: picker_mcp::OWNER, render: picker_mcp::render },
    Entry { id: picker_repository::ID, owner: picker_repository::OWNER, render: picker_repository::render },
    Entry { id: sub_shell::ID, owner: sub_shell::OWNER, render: sub_shell::render },
    Entry { id: menu::ID, owner: menu::OWNER, render: menu::render },
    Entry { id: toast::ID, owner: toast::OWNER, render: toast::render },
    Entry { id: jump_to_bottom::ID, owner: jump_to_bottom::OWNER, render: jump_to_bottom::render },
    Entry { id: composer_dialog::ID, owner: composer_dialog::OWNER, render: composer_dialog::render },
    Entry { id: issue_context_menu::ID, owner: issue_context_menu::OWNER, render: issue_context_menu::render },
    Entry { id: session_tree::ID, owner: session_tree::OWNER, render: session_tree::render },
    Entry { id: pr_graph_badge::ID, owner: pr_graph_badge::OWNER, render: pr_graph_badge::render },
    Entry { id: device_settings::ID, owner: device_settings::OWNER, render: device_settings::render },
    Entry { id: blocked_start_dialog::ID, owner: blocked_start_dialog::OWNER, render: blocked_start_dialog::render },
    Entry { id: stack_merge_choice_dialog::ID, owner: stack_merge_choice_dialog::OWNER, render: stack_merge_choice_dialog::render },
    Entry { id: readiness_checklist::ID, owner: readiness_checklist::OWNER, render: readiness_checklist::render },
    Entry { id: results_guide::ID, owner: results_guide::OWNER, render: results_guide::render },
    Entry { id: mcp_app_views::ID, owner: mcp_app_views::OWNER, render: mcp_app_views::render },
];
