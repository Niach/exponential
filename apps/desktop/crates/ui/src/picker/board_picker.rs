//! EXP-1029 contract — the board picker: every row draws the board's ICON and
//! COLOUR (`icons::option_icon` + `settings::parse_hex_color`), everywhere.
//! Consumers: the composer, the create-issue dialog, move-to-board, the
//! widget/board settings. EXP-1021 fills the rows.

use gpui::AnyElement;

use domain::rows::Board;

use crate::icons::board_icon;
use crate::settings::parse_hex_color;

use super::{OnPickerChange, Picker, PickerItem};

/// The board rows → picker items: the board's stored glyph (or its
/// attribute-derived fallback) in the board's own colour, muted when it has
/// none — the ONE board row, everywhere a board is picked.
pub(crate) fn board_items(boards: &[Board]) -> Vec<PickerItem<String>> {
    boards
        .iter()
        .map(|board| {
            PickerItem::new(board.id.clone(), board.name.clone())
                .icon(board_icon(board))
                .color(
                    board
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or_else(|| theme::tokens::MUTED_FOREGROUND.to_hsla()),
                )
        })
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
