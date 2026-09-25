//! EXP-1082 — the workflow's AUDIT TRAIL (the synced `workflow_events`
//! shape): what the runner device's engine did, newest first. Declared here
//! so the collection has a surface; the rows are drawn by a later issue, so
//! today this renders an empty container.

#![allow(dead_code)]

use domain::rows::WorkflowEventRow;
use gpui::{div, App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window};

/// The event list of one workflow.
#[derive(IntoElement)]
pub(crate) struct WorkflowEventList {
    events: Vec<WorkflowEventRow>,
}

impl WorkflowEventList {
    pub(crate) fn new(events: &[WorkflowEventRow]) -> Self {
        Self {
            events: events.to_vec(),
        }
    }
}

impl RenderOnce for WorkflowEventList {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // EXP-1064 fills the rows.
        div()
            .flex()
            .flex_col()
            .children(Vec::<gpui::AnyElement>::new())
    }
}
