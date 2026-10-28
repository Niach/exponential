//! #EXP-1388: `chat-selection` (Special components): the Chats list's
//! MULTI-SELECTION (`fixtures/chats.json` SELECTION + ARCHIVE), the twin of
//! web `ChatSelectionSpecimen`. The IDE's Chats page paints the same pieces
//! ([`crate::chat_list`]): the Archived entry first (the plain ui-archive
//! glyph centred in the disc column, the count in the time slot), then
//! `listCases[0]`'s rows in the issue row's selectable frame (the checkbox
//! cell, two selected on the flat `list_active` fill, their lead discs one
//! notch stronger), over THE bar (the issue
//! list's bulk bar: capsule, bare count, labeled buttons, the hairline before
//! Delete) holding `chat_selection_actions` for those two.

use gpui::{px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::chats::{chat_selection_actions, ChatList, ChatListView, ChatSelectionRow};

use crate::chat_list::{
    chat_archived_row, chat_bar_label_min_width, chat_lead_disc, chat_row, chat_selectable, chat_selection_bar,
};

pub(crate) const ID: &str = "chat-selection";
pub(crate) const OWNER: &str = "EXP-1388";

/// The specimen column (the web island's 390px is the phone; the IDE page
/// column is wider, the bar needs its labels).
const COLUMN_W: f32 = 560.;

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
    // The page's rule at the specimen's width.
    let labels = px(COLUMN_W) >= chat_bar_label_min_width(&actions.iter().map(|(action, _)| *action).collect::<Vec<_>>());
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
                .child(chat_selectable(
                    SharedString::from("chat-selection-archived"),
                    "",
                    None,
                    |_, _, _| {},
                    chat_archived_row(archived, cx),
                    cx,
                ))
                .children(list.rows.iter().map(|row| {
                    let is_selected = selected.contains(&row.session_id);
                    chat_selectable(
                        SharedString::from(format!("chat-selection-{}", row.session_id)),
                        &row.session_id,
                        Some((is_selected, true)),
                        |_, _, _| {},
                        chat_row(row, chat_lead_disc(row.lead, None, is_selected, cx), cx),
                        cx,
                    )
                }))
                .child(crate::issue_list::bulk_bar_dock(chat_selection_bar(
                    selected.len(),
                    actions,
                    false,
                    labels,
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
