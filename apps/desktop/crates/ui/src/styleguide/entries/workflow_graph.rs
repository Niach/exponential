//! EXP-1014 — the `workflow-graph` entry (4 Views): ONE fixed sample of the
//! workflow screen's graph, drawn by the screen's own code
//! (`crate::workflow_view::styleguide_sample_graph`) rather than by a copy
//! of it, so the entry cannot drift from the surface it documents.
//!
//! The sample: a landed CONTRACT node, three leaves after it (a compound one
//! drawn as a deck of chips, one with a live run, one landed), an
//! INTEGRATION node they all block, and the final pull request after the
//! last wave — its chip carrying the Merge action, since that pull request
//! is open (EXP-1032). The running node is the picked one — the accent ring
//! outside its chip.

use gpui::{div, App, Div, IntoElement, ParentElement as _, RenderOnce, Window};

pub(crate) const ID: &str = "workflow-graph";
pub(crate) const OWNER: &str = "EXP-1014";

/// The entry's demo. The section API hands an entry no `App`, and a themed
/// element needs one, so the sample is a component: gpui gives it the app at
/// render time.
pub(crate) fn render() -> Div {
    div().child(WorkflowGraphSample)
}

#[derive(IntoElement)]
struct WorkflowGraphSample;

impl RenderOnce for WorkflowGraphSample {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::workflow_view::styleguide_sample_graph(cx)
    }
}
