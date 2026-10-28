//! EXP-1029 contract — every IDE styleguide entry under the four sections,
//! one file each, registered here by fixed name. A leaf fills ITS file.

use gpui::{App, Div, Window};

/// The typed pickers' shared demo kit, not an entry (VAPP-93).
pub(crate) mod picker;
/// The Threads v2 entries' fixture rows + demo pull requests (EXP-1294).
pub(crate) mod fixture_rows;
pub(crate) mod picker_board;
pub(crate) mod picker_issue;
pub(crate) mod picker_account;
pub(crate) mod picker_device;
pub(crate) mod picker_assignee;
pub(crate) mod picker_icon;
pub(crate) mod picker_status;
pub(crate) mod picker_priority;
pub(crate) mod picker_label;
pub(crate) mod picker_mcp;
pub(crate) mod picker_repository;
pub(crate) mod scope_picker;
pub(crate) mod schedule_picker;
pub(crate) mod search_list;
pub(crate) mod menu_submenu;
pub(crate) mod toast;
pub(crate) mod jump_to_bottom;
pub(crate) mod strip_chip;
pub(crate) mod composer_dialog;
pub(crate) mod issue_context_menu;
pub(crate) mod session_row;
pub(crate) mod pr_row;
pub(crate) mod pr_graph_badge;
pub(crate) mod device_settings;
// NEW-59: the Work screen picker.
pub(crate) mod work_screen;
pub(crate) mod stack_merge_choice_dialog;
pub(crate) mod draft_leave_dialog;
pub(crate) mod readiness_checklist;
pub(crate) mod device_readiness;
// A-16: the Devices row + its attention card.
pub(crate) mod device_attention;
pub(crate) mod results_guide;
pub(crate) mod mcp_app_views;
pub(crate) mod run_status_row;
pub(crate) mod session_thread;
pub(crate) mod composer_options;
pub(crate) mod work_card;
pub(crate) mod thread_parts;
pub(crate) mod ask_dock;
pub(crate) mod dock_card;
pub(crate) mod plan_card;
pub(crate) mod script_card;
pub(crate) mod comment_card;
pub(crate) mod team_agents_settings;
pub(crate) mod voice_call;
// S-3: the round call buttons.
pub(crate) mod call_controls;
pub(crate) mod voice_call_overlay;
// NEW-67: Settings → Voice.
pub(crate) mod voice_settings;
pub(crate) mod meeting_consent;
pub(crate) mod computer_use_banner;
pub(crate) mod feedback_states;
pub(crate) mod trace;
pub(crate) mod code_block;
pub(crate) mod agent_home;
pub(crate) mod work_tabs_strip;
pub(crate) mod getting_started;
// Thread 58cb32df (#EXP-1373): Chats.
pub(crate) mod chat_list;
pub(crate) mod phone_tab_bar;
pub(crate) mod phone_board_menu;
pub(crate) mod phone_page_header;
pub(crate) mod chat_transcript;
pub(crate) mod chat_menus;
// #EXP-1388: the Chats multi-selection.
pub(crate) mod chat_selection;
// Thread e4a04d22: mention pills in chats.
pub(crate) mod mention_pill;

/// EXP-1030: whether `id`'s file is still the one-line placeholder rather
/// than a demo. None is left since NEW-77 filled `voice-call-overlay`.
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

/// The entry registered as `id` (EXP-1259: `EXP_DEV_SCREEN=styleguide:<id>`
/// draws it full-window for a capture).
pub(crate) fn find(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

/// Every entry, in the section index's order.
pub(crate) const ENTRIES: &[Entry] = &[
    Entry { id: picker_board::ID, owner: picker_board::OWNER, render: picker_board::render },
    Entry { id: picker_issue::ID, owner: picker_issue::OWNER, render: picker_issue::render },
    Entry { id: picker_account::ID, owner: picker_account::OWNER, render: picker_account::render },
    Entry { id: picker_device::ID, owner: picker_device::OWNER, render: picker_device::render },
    Entry { id: picker_assignee::ID, owner: picker_assignee::OWNER, render: picker_assignee::render },
    Entry { id: picker_icon::ID, owner: picker_icon::OWNER, render: picker_icon::render },
    Entry { id: picker_status::ID, owner: picker_status::OWNER, render: picker_status::render },
    Entry { id: picker_priority::ID, owner: picker_priority::OWNER, render: picker_priority::render },
    Entry { id: picker_label::ID, owner: picker_label::OWNER, render: picker_label::render },
    Entry { id: picker_mcp::ID, owner: picker_mcp::OWNER, render: picker_mcp::render },
    Entry { id: picker_repository::ID, owner: picker_repository::OWNER, render: picker_repository::render },
    Entry { id: scope_picker::ID, owner: scope_picker::OWNER, render: scope_picker::render },
    Entry { id: schedule_picker::ID, owner: schedule_picker::OWNER, render: schedule_picker::render },
    Entry { id: search_list::ID, owner: search_list::OWNER, render: search_list::render },
    Entry { id: menu_submenu::ID, owner: menu_submenu::OWNER, render: menu_submenu::render },
    Entry { id: toast::ID, owner: toast::OWNER, render: toast::render },
    Entry { id: jump_to_bottom::ID, owner: jump_to_bottom::OWNER, render: jump_to_bottom::render },
    Entry { id: strip_chip::ID, owner: strip_chip::OWNER, render: strip_chip::render },
    Entry { id: mention_pill::ID, owner: mention_pill::OWNER, render: mention_pill::render },
    Entry { id: call_controls::ID, owner: call_controls::OWNER, render: call_controls::render },
    Entry { id: feedback_states::ID, owner: feedback_states::OWNER, render: feedback_states::render },
    Entry { id: trace::ID, owner: trace::OWNER, render: trace::render },
    Entry { id: code_block::ID, owner: code_block::OWNER, render: code_block::render },
    Entry { id: composer_dialog::ID, owner: composer_dialog::OWNER, render: composer_dialog::render },
    Entry { id: issue_context_menu::ID, owner: issue_context_menu::OWNER, render: issue_context_menu::render },
    Entry { id: session_row::ID, owner: session_row::OWNER, render: session_row::render },
    Entry { id: pr_row::ID, owner: pr_row::OWNER, render: pr_row::render },
    Entry { id: pr_graph_badge::ID, owner: pr_graph_badge::OWNER, render: pr_graph_badge::render },
    Entry { id: device_settings::ID, owner: device_settings::OWNER, render: device_settings::render },
    Entry { id: work_screen::ID, owner: work_screen::OWNER, render: work_screen::render },
    Entry { id: stack_merge_choice_dialog::ID, owner: stack_merge_choice_dialog::OWNER, render: stack_merge_choice_dialog::render },
    Entry { id: draft_leave_dialog::ID, owner: draft_leave_dialog::OWNER, render: draft_leave_dialog::render },
    Entry { id: readiness_checklist::ID, owner: readiness_checklist::OWNER, render: readiness_checklist::render },
    Entry { id: device_readiness::ID, owner: device_readiness::OWNER, render: device_readiness::render },
    Entry { id: device_attention::ID, owner: device_attention::OWNER, render: device_attention::render },
    Entry { id: results_guide::ID, owner: results_guide::OWNER, render: results_guide::render },
    Entry { id: mcp_app_views::ID, owner: mcp_app_views::OWNER, render: mcp_app_views::render },
    Entry { id: run_status_row::ID, owner: run_status_row::OWNER, render: run_status_row::render },
    Entry { id: session_thread::ID, owner: session_thread::OWNER, render: session_thread::render },
    Entry { id: composer_options::ID, owner: composer_options::OWNER, render: composer_options::render },
    Entry { id: work_card::ID, owner: work_card::OWNER, render: work_card::render },
    Entry { id: thread_parts::ID, owner: thread_parts::OWNER, render: thread_parts::render },
    Entry { id: ask_dock::ID, owner: ask_dock::OWNER, render: ask_dock::render },
    Entry { id: dock_card::ID, owner: dock_card::OWNER, render: dock_card::render },
    Entry { id: plan_card::ID, owner: plan_card::OWNER, render: plan_card::render },
    Entry { id: script_card::ID, owner: script_card::OWNER, render: script_card::render },
    Entry { id: comment_card::ID, owner: comment_card::OWNER, render: comment_card::render },
    Entry { id: team_agents_settings::ID, owner: team_agents_settings::OWNER, render: team_agents_settings::render },
    Entry { id: voice_call_overlay::ID, owner: voice_call_overlay::OWNER, render: voice_call_overlay::render },
    Entry { id: voice_call::ID, owner: voice_call::OWNER, render: voice_call::render },
    Entry { id: computer_use_banner::ID, owner: computer_use_banner::OWNER, render: computer_use_banner::render },
    Entry { id: voice_settings::ID, owner: voice_settings::OWNER, render: voice_settings::render },
    Entry { id: meeting_consent::ID, owner: meeting_consent::OWNER, render: meeting_consent::render },
    Entry { id: agent_home::ID, owner: agent_home::OWNER, render: agent_home::render },
    Entry { id: work_tabs_strip::ID, owner: work_tabs_strip::OWNER, render: work_tabs_strip::render },
    Entry { id: getting_started::ID, owner: getting_started::OWNER, render: getting_started::render },
    Entry { id: chat_list::ID, owner: chat_list::OWNER, render: chat_list::render },
    Entry { id: phone_tab_bar::ID, owner: phone_tab_bar::OWNER, render: phone_tab_bar::render },
    Entry { id: phone_board_menu::ID, owner: phone_board_menu::OWNER, render: phone_board_menu::render },
    Entry { id: phone_page_header::ID, owner: phone_page_header::OWNER, render: phone_page_header::render },
    Entry { id: chat_transcript::ID, owner: chat_transcript::OWNER, render: chat_transcript::render },
    Entry { id: chat_menus::ID, owner: chat_menus::OWNER, render: chat_menus::render },
    Entry { id: chat_selection::ID, owner: chat_selection::OWNER, render: chat_selection::render },
];
