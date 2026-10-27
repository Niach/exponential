//! #EXP-1388: `chat-selection` (Special components): the Chats list's
//! MULTI-SELECTION (`fixtures/chats.json` SELECTION + ARCHIVE), the twin of
//! web `ChatSelectionSpecimen`. The IDE's Chats page paints the same pieces
//! ([`crate::chat_list`]): the Archived entry first (ui-archive in the lead
//! disc, the count in the time slot), then `listCases[0]`'s rows, each led by
//! the issue list's checkbox (two selected), over THE bar (the issue list's bulk
//! capsule) holding `chat_selection_actions` for those two.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::chats::{chat_selection_actions, ChatList, ChatListView, ChatSelectionRow};

use crate::chat_list::{chat_archived_row, chat_lead_disc, chat_row, chat_selectable, chat_selection_bar};

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
    let row_frame = |selected_row: bool, row: Div, cx: &App| {
        div()
            .w_full()
            .px_2()
            .rounded_lg()
            .when_selected(selected_row, cx)
            .child(row)
    };
    v_flex()
        .w(px(COLUMN_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(
            v_flex()
                .w_full()
                .child(row_frame(false, chat_selectable(None, "", chat_archived_row(archived, cx), cx), cx))
                .children(list.rows.iter().map(|row| {
                    let is_selected = selected.contains(&row.session_id);
                    row_frame(
                        is_selected,
                        chat_selectable(Some(is_selected), &row.session_id, chat_row(row, chat_lead_disc(row.lead, None, cx), cx), cx),
                        cx,
                    )
                })),
        )
        .child(
            h_flex()
                .w_full()
                .justify_center()
                .child(chat_selection_bar(selected.len(), actions, false, |_, _, _| {}, None, cx)),
        )
}

trait SelectedExt {
    fn when_selected(self, selected: bool, cx: &App) -> Self;
}

impl SelectedExt for Div {
    /// A selected row's fill (the Chats page's `list_active`).
    fn when_selected(self, selected: bool, cx: &App) -> Self {
        if selected {
            self.bg(cx.theme().list_active)
        } else {
            self
        }
    }
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
            [ChatSelectionAction::MarkRead, ChatSelectionAction::Archive, ChatSelectionAction::Stop, ChatSelectionAction::Delete]
        );
    }
}
