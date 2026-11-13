//! The specimen registry: one file per id (the `views.json` desktop
//! capture's `specimen:<id>`), registered here by fixed name.

use gpui::{App, Div, Window};

pub(crate) mod ask_dock;
pub(crate) mod call_controls;
pub(crate) mod call_pill;
pub(crate) mod chat_header;
pub(crate) mod chat_list;
pub(crate) mod chat_menus;
pub(crate) mod chat_selection;
pub(crate) mod chat_transcript;
pub(crate) mod ci_run;
pub(crate) mod code_block;
pub(crate) mod device_readiness;
pub(crate) mod dock_card;
pub(crate) mod feedback_states;
pub(crate) mod mention_pill;
pub(crate) mod needs_you;
pub(crate) mod phone_board_menu;
pub(crate) mod phone_page_header;
pub(crate) mod phone_tab_bar;
pub(crate) mod plan_card;
pub(crate) mod plan_page;
pub(crate) mod pr_checks;
pub(crate) mod pr_row;
pub(crate) mod repo_ci;
pub(crate) mod repo_runs;
pub(crate) mod results_tab;
pub(crate) mod runner_settings;
pub(crate) mod schedule_picker;
pub(crate) mod sealed_locked;
pub(crate) mod session_row;
pub(crate) mod trace;
pub(crate) mod voice_call;
pub(crate) mod voice_call_overlay;

/// A specimen: the entry's element, built with the window and the app so it
/// can be the REAL component (EXP-1063) — every glass recipe takes the theme
/// from `cx`, a select or a nav stack needs an entity, and
/// `window.use_keyed_state` keeps that entity alive across frames.
pub(crate) type Render = fn(&mut Window, &mut App) -> Div;

/// One entry: its id (the capture's), the issue that owns it, its specimen.
pub(crate) struct Entry {
    pub id: &'static str,
    pub owner: &'static str,
    pub render: Render,
}

/// The entry registered as `id` (`EXP_DEV_SCREEN=specimen:<id>` draws it
/// full-window for a capture).
pub(crate) fn find(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}

/// Every entry, by id.
pub(crate) const ENTRIES: &[Entry] = &[
    Entry { id: ask_dock::ID, owner: ask_dock::OWNER, render: ask_dock::render },
    Entry { id: call_controls::ID, owner: call_controls::OWNER, render: call_controls::render },
    Entry { id: call_pill::ID, owner: call_pill::OWNER, render: call_pill::render },
    Entry { id: chat_header::ID, owner: chat_header::OWNER, render: chat_header::render },
    Entry { id: chat_list::ID, owner: chat_list::OWNER, render: chat_list::render },
    Entry { id: chat_menus::ID, owner: chat_menus::OWNER, render: chat_menus::render },
    Entry { id: chat_selection::ID, owner: chat_selection::OWNER, render: chat_selection::render },
    Entry { id: chat_transcript::ID, owner: chat_transcript::OWNER, render: chat_transcript::render },
    Entry { id: ci_run::ID, owner: ci_run::OWNER, render: ci_run::render },
    Entry { id: code_block::ID, owner: code_block::OWNER, render: code_block::render },
    Entry { id: device_readiness::ID, owner: device_readiness::OWNER, render: device_readiness::render },
    Entry { id: dock_card::ID, owner: dock_card::OWNER, render: dock_card::render },
    Entry { id: feedback_states::ID, owner: feedback_states::OWNER, render: feedback_states::render },
    Entry { id: mention_pill::ID, owner: mention_pill::OWNER, render: mention_pill::render },
    Entry { id: needs_you::ID, owner: needs_you::OWNER, render: needs_you::render },
    Entry { id: phone_board_menu::ID, owner: phone_board_menu::OWNER, render: phone_board_menu::render },
    Entry { id: phone_page_header::ID, owner: phone_page_header::OWNER, render: phone_page_header::render },
    Entry { id: phone_tab_bar::ID, owner: phone_tab_bar::OWNER, render: phone_tab_bar::render },
    Entry { id: plan_card::ID, owner: plan_card::OWNER, render: plan_card::render },
    Entry { id: plan_page::ID, owner: plan_page::OWNER, render: plan_page::render },
    Entry { id: pr_checks::ID, owner: pr_checks::OWNER, render: pr_checks::render },
    Entry { id: pr_row::ID, owner: pr_row::OWNER, render: pr_row::render },
    Entry { id: repo_ci::ID, owner: repo_ci::OWNER, render: repo_ci::render },
    Entry { id: repo_runs::ID, owner: repo_runs::OWNER, render: repo_runs::render },
    Entry { id: results_tab::ID, owner: results_tab::OWNER, render: results_tab::render },
    Entry { id: runner_settings::ID, owner: runner_settings::OWNER, render: runner_settings::render },
    Entry { id: schedule_picker::ID, owner: schedule_picker::OWNER, render: schedule_picker::render },
    Entry { id: sealed_locked::ID, owner: sealed_locked::OWNER, render: sealed_locked::render },
    Entry { id: session_row::ID, owner: session_row::OWNER, render: session_row::render },
    Entry { id: trace::ID, owner: trace::OWNER, render: trace::render },
    Entry { id: voice_call::ID, owner: voice_call::OWNER, render: voice_call::render },
    Entry { id: voice_call_overlay::ID, owner: voice_call_overlay::OWNER, render: voice_call_overlay::render },
];
