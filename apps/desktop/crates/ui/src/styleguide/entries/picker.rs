//! EXP-1029 contract — PLACEHOLDER `picker` (2 General components). EXP-1021 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.

use gpui::{div, Div, ParentElement as _};

pub(crate) const ID: &str = "picker";
pub(crate) const OWNER: &str = "EXP-1021";

/// The one-line body every placeholder renders until its owner fills it.
pub(crate) fn render() -> Div {
    div().child(format!("{ID} — placeholder, {OWNER} fills this entry."))
}
