//! EXP-1029 contract — the board picker: every row draws the board's ICON and
//! COLOUR (`icons::option_icon` + `settings::parse_hex_color`), everywhere.
//! Consumers: the composer, the create-issue dialog, move-to-board, the
//! widget/board settings. EXP-1021 fills the rows.

use gpui::AnyElement;

use domain::rows::Board;

use super::{OnPickerChange, Picker, PickerItem};

/// The board rows → picker items (icon + colour). Contract stub.
pub(crate) fn board_items(boards: &[Board]) -> Vec<PickerItem<String>> {
    boards
        .iter()
        .map(|board| PickerItem::new(board.id.clone(), board.name.clone()))
        .collect()
}

pub(crate) fn board_picker(
    boards: &[Board],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(board_items(boards), value, trigger, on_change)
        .search(true)
        .empty_text("No boards")
}
