//! EXP-1020 — `sub-shell` (2 General components): the settings row that
//! slides a child page in place of the whole card.
//!
//! The IDE's implementation is `crate::sub_shell`: `SubShellNav` holds the
//! page stack on the OWNING view (gpui retains no element state between
//! frames), `sub_shell_row` is the row, and `SubShellHost` renders the card
//! at rest or the open page with `controls::back_glyph` on top. Live users:
//! the device settings dialog and Settings → Agents, both for "Workflow
//! settings".

use gpui::{div, Div, ParentElement as _};

pub(crate) const ID: &str = "sub-shell";
pub(crate) const OWNER: &str = "EXP-1020";

/// The form, as the four clients agreed it. `render` takes no `App`, so this
/// entry names the shape rather than painting a themed row; the live control
/// is one screen away in Settings → Agents.
pub(crate) fn render() -> Div {
    div()
        .child("Sub-shell navigation — a row with a chevron; opening it replaces the WHOLE card")
        .child("with its page, back glyph on top. Not a nested card, not a dialog.")
        .child("A sub-shell inside a page goes one level deeper; back returns exactly one.")
        .child("Row: [glyph] Label … value (`Opus · Fable`) [chevron]")
        .child("IDE: ui::sub_shell (SubShellNav / sub_shell_row / SubShellHost)")
}
