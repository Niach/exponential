//! EXP-1029 contract — PLACEHOLDER `toast` (2 General components). EXP-1031 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.

use gpui::{div, App, Div, ParentElement as _, Window};

pub(crate) const ID: &str = "toast";
pub(crate) const OWNER: &str = "EXP-1031";

/// The one-line body every placeholder renders until its owner fills it.
pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    div().child(format!("{ID} — placeholder, {OWNER} fills this entry."))
}
