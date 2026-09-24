//! EXP-1029 contract — PLACEHOLDER `workflow-graph` (4 Views). EXP-1014 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.

use gpui::{div, Div, ParentElement as _};

pub(crate) const ID: &str = "workflow-graph";
pub(crate) const OWNER: &str = "EXP-1014";

/// The one-line body every placeholder renders until its owner fills it.
pub(crate) fn render() -> Div {
    div().child(format!("{ID} — placeholder, {OWNER} fills this entry."))
}
