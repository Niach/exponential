//! EXP-1021 — the `picker-icon` styleguide entry: `IconPicker(set)` — the curated grid as the surface's BODY.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::Div;

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-icon";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render() -> Div {
    column(vec![
        demo("icon — the board set (96 glyphs), the grid inside the popover", |window, cx| {
            crate::picker::icon_picker::icon_picker(
                crate::picker::icon_picker::IconSet::Board,
                Some("rocket".to_string()),
                chip("sg-picker-icon", "rocket", cx),
                inert(),
            )
            .id("sg-picker-icon-surface")
            .render(window, cx)
        }),
        demo("icon — the DEVICE set: one picker, two sets, never a fork", |window, cx| {
            crate::picker::icon_picker::icon_picker(
                crate::picker::icon_picker::IconSet::Device,
                Some("server".to_string()),
                chip("sg-picker-icon-device", "server", cx),
                inert(),
            )
            .id("sg-picker-icon-device-surface")
            .render(window, cx)
        }),
    ])
}
