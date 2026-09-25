//! EXP-1021 — the `picker-issue` styleguide entry: the issue picker — `IDENT Title` rows, single or multi.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::Div;

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-issue";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render() -> Div {
    column(vec![
        demo("issue — single (a relation, a duplicate, a stack)", |window, cx| {
            let issues = super::picker::demo_issues();
            crate::picker::issue_picker::issue_picker(
                &issues,
                None,
                chip("sg-picker-issue", "Pick an issue", cx),
                inert(),
            )
            .id("sg-picker-issue-surface")
            .render(window, cx)
        }),
        demo("issue — multi (a batch): the picked rows are the highlight", |window, cx| {
            let issues = super::picker::demo_issues();
            crate::picker::issue_picker::issue_multi_picker(
                &issues,
                vec!["issue-0".to_string()],
                chip("sg-picker-issues", "EXP-1021 +1", cx),
                inert(),
            )
            .id("sg-picker-issues-surface")
            .render(window, cx)
        }),
    ])
}
