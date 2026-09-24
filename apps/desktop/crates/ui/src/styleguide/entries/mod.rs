//! EXP-1029 contract — every IDE styleguide entry under the four sections,
//! one file each, registered here by fixed name. A leaf fills ITS file.

use gpui::Div;

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
pub(crate) mod sub_shell;
pub(crate) mod toast;
pub(crate) mod composer_dialog;
pub(crate) mod session_tree;
pub(crate) mod device_settings;
pub(crate) mod workflow_graph;

/// One entry: its id (the section index's), its owner, its demo.
pub(crate) struct Entry {
    pub id: &'static str,
    pub owner: &'static str,
    pub render: fn() -> Div,
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
    Entry { id: sub_shell::ID, owner: sub_shell::OWNER, render: sub_shell::render },
    Entry { id: toast::ID, owner: toast::OWNER, render: toast::render },
    Entry { id: composer_dialog::ID, owner: composer_dialog::OWNER, render: composer_dialog::render },
    Entry { id: session_tree::ID, owner: session_tree::OWNER, render: session_tree::render },
    Entry { id: device_settings::ID, owner: device_settings::OWNER, render: device_settings::render },
    Entry { id: workflow_graph::ID, owner: workflow_graph::OWNER, render: workflow_graph::render },
];
