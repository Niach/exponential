//! #EXP-1388: `chat-selection` (Special components): the Chats list's
//! MULTI-SELECTION (`fixtures/chats.json` SELECTION + ARCHIVE), the twin of
//! web `ChatSelectionSpecimen`. The IDE's Chats page paints the same pieces
//! ([`crate::chat_list`]): the Archived entry first (the plain ui-archive
//! glyph centred in the disc column, the count in the time slot), then
//! `listCases[0]`'s rows in the issue row's selectable frame (the checkbox
//! cell, two selected on the flat `list_active` fill, their lead discs one
//! notch stronger), over THE bar (the issue
//! list's bulk bar: capsule, bare count; EXP-1676: web's specimen bar, full
//! width with bare glyphs) holding `chat_selection_actions` for those two. #EXP-1553: the
//! whole list like web's specimen: the Agents section
//! (`agentRowCases[0]`), then the Chats section with the rows above.

use gpui::{prelude::FluentBuilder as _, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::chats::{chat_sections, chat_selection_actions, ChatList, ChatSectionKey, ChatListView, ChatSelectionRow, CHATS};

use crate::chat_list::{
    agent_chat_row, chat_archived_row, chat_lead_disc, chat_row,
    chat_section_header, chat_selectable, chat_selection_bar,
};

pub(crate) const ID: &str = "chat-selection";
pub(crate) const OWNER: &str = "EXP-1388";

/// The specimen column (the web island's 390px). EXP-1676: its bar = web's
/// specimen bar (`BulkBarCapsule iconOnly` at `w-full`): the full-width
/// capsule, bare glyphs at the trailing edge.
const COLUMN_W: f32 = 390.;

const CHATS_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/chats.json");

/// `listCases[0]`'s expected rows, the archived count of the archive case,
/// and the two selected rows (the running one and the unread one).
fn model() -> (ChatList, usize, Vec<String>) {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    let list: ChatList = serde_json::from_value(fixture["listCases"][0]["expected"].clone()).expect("list");
    let archived = fixture["listCases"]
        .as_array()
        .and_then(|cases| cases.iter().find_map(|case| case["expected"]["archived"].as_u64().filter(|n| *n > 0)))
        .unwrap_or(2) as usize;
    let running = list.rows.iter().find(|row| row.busy && !row.needs_you);
    let unread = list.rows.iter().find(|row| row.unread_count > 0);
    let selected = running.into_iter().chain(unread).map(|row| row.session_id.clone()).collect();
    (list, archived, selected)
}

fn selection_rows(list: &ChatList, selected: &[String]) -> Vec<ChatSelectionRow> {
    list.rows
        .iter()
        .filter(|row| selected.contains(&row.session_id))
        .map(|row| ChatSelectionRow {
            session_id: row.session_id.clone(),
            root_status: if row.busy { "running" } else { "idle" }.to_string(),
            unread_count: row.unread_count,
            can_steer: true,
            can_delete: true,
        })
        .collect()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let (list, archived, selected) = model();
    let actions = chat_selection_actions(&selection_rows(&list, &selected), ChatListView::List);
    let (_, agent_rows) = super::chat_list::model();
    // #FEED-191: the list's sections; the Archived entry heads the last one.
    let mut rows: Vec<AnyElement> = Vec::new();
    for section in chat_sections(&list.rows) {
        rows.push(chat_section_header(section.key.title(), cx));
        if section.key == ChatSectionKey::Chats {
            rows.push(
                chat_selectable(
                    SharedString::from("chat-selection-archived"),
                    "",
                    None,
                    |_, _, _| {},
                    chat_archived_row(archived, cx),
                    cx,
                )
                .into_any_element(),
            );
        }
        for row in &section.rows {
            let is_selected = selected.contains(&row.session_id);
            rows.push(
                chat_selectable(
                    SharedString::from(format!("chat-selection-{}", row.session_id)),
                    &row.session_id,
                    Some((is_selected, true)),
                    |_, _, _| {},
                    chat_row(row, chat_lead_disc(row.lead, None, is_selected, cx), cx),
                    cx,
                )
                .into_any_element(),
            );
        }
    }
    v_flex()
        .w(px(COLUMN_W))
        .py_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        // The list pane: the bar docks over its bottom like the page's.
        .child(
            v_flex()
                .relative()
                .w_full()
                .pb(px(72.))
                .when(!agent_rows.is_empty(), |pane| {
                    pane.child(chat_section_header(&CHATS.copy.sections.agents, cx))
                        .children(agent_rows.iter().map(|row| agent_chat_row(row, cx).into_any_element()))
                })
                .children(rows)
                .child(crate::issue_list::bulk_bar_dock(chat_selection_bar(
                    selected.len(),
                    actions,
                    false,
                    false,
                    true,
                    |_, _, _| {},
                    None,
                    cx,
                ))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::chats::ChatSelectionAction;

    /// Two rows selected: one unread (Mark read), one running (Stop).
    #[test]
    fn the_specimen_selects_two_rows_with_the_full_bar() {
        let (list, archived, selected) = model();
        assert!(archived > 0);
        assert_eq!(selected.len(), 2);
        let actions: Vec<ChatSelectionAction> = chat_selection_actions(&selection_rows(&list, &selected), ChatListView::List)
            .into_iter()
            .map(|(action, _)| action)
            .collect();
        assert_eq!(
            actions,
            [ChatSelectionAction::MarkRead, ChatSelectionAction::Archive, ChatSelectionAction::Delete]
        );
    }
}
