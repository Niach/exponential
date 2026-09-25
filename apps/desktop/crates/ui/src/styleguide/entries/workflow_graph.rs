//! EXP-1014 → EXP-1085 — the `workflow-graph` entry (4 Views): ONE fixed
//! sample of the workflow page's top, drawn by the page's own code
//! (`crate::workflow_view::styleguide_sample_graph`) rather than by a copy
//! of it, so the entry cannot drift from the surface it documents.
//!
//! The sample: the header caption line and the node strip — `All`, then a
//! landed contract node, three nodes in the next wave (a compound one drawn
//! as a deck, one with its agent mid-turn, one that needs you) and a failed
//! one after them. The running node is the picked one.

use gpui::{div, App, Div, ParentElement as _, Window};

pub(crate) const ID: &str = "workflow-graph";
pub(crate) const OWNER: &str = "EXP-1014";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    div().child(crate::workflow_view::styleguide_sample_graph(cx))
}
