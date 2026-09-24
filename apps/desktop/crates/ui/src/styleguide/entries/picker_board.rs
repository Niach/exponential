//! EXP-1021 — the `picker-board` styleguide entry: the board picker — every row the board's glyph in its colour.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::Div;

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-board";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render() -> Div {
    column(vec![
        demo(
            "board — searchable; the glyph is the board's, tinted by its colour",
            |window, cx| {
                let boards = super::picker::demo_boards();
                crate::picker::board_picker::board_picker(
                    &boards,
                    Some("board-1".to_string()),
                    chip("sg-picker-board", "Mobile Bugs", cx),
                    inert(),
                )
                .id("sg-picker-board-surface")
                .render(window, cx)
            },
        ),
        demo(
            "optional — an action's `board` input: the clearing row is a ROW, picked while nothing is",
            |window, cx| {
                let boards = super::picker::demo_boards();
                crate::picker::board_picker::optional_board_picker(
                    &boards,
                    None,
                    chip("sg-picker-board-optional", "Select board…", cx),
                    inert(),
                )
                .id("sg-picker-board-optional-surface")
                .render(window, cx)
            },
        ),
    ])
}
