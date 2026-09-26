//! EXP-1082/EXP-1064 — the workflow's AUDIT TRAIL (the synced
//! `workflow_events` shape): what the runner device's engine did, newest
//! first. One flat row per line: the kind's concept glyph, the node's
//! identifier (when the caller resolves it), the message on one line and the
//! local `HH:mm` at the trailing edge. A line that names a run opens it.
//! Mounted by the workflow screen (a later node); an empty trail draws
//! nothing.

#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use domain::rows::WorkflowEventRow;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};

/// Opens the run a line names (`session_id`).
pub(crate) type OpenSession = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The event list of one workflow.
#[derive(IntoElement)]
pub(crate) struct WorkflowEventList {
    events: Vec<WorkflowEventRow>,
    /// node id → its issue identifier, the row's mono lead-in.
    node_label: HashMap<String, String>,
    on_open_session: Option<OpenSession>,
}

impl WorkflowEventList {
    pub(crate) fn new(events: &[WorkflowEventRow]) -> Self {
        Self {
            events: events.to_vec(),
            node_label: HashMap::new(),
            on_open_session: None,
        }
    }

    /// Resolves a line's `node_id` to the identifier it leads with.
    pub(crate) fn node_labels(mut self, node_label: HashMap<String, String>) -> Self {
        self.node_label = node_label;
        self
    }

    /// Makes a line that names a run clickable.
    pub(crate) fn on_open_session(mut self, on_open: OpenSession) -> Self {
        self.on_open_session = Some(on_open);
        self
    }
}

/// A kind's concept glyph (contract `wfEventKind`); an unknown newer kind
/// reads as information.
pub(crate) fn event_icon(kind: &str) -> ExpIcon {
    match kind {
        // The ONE table ×4 (web `WORKFLOW_EVENT_GLYPH`, iOS/Android the same).
        "node_started" => registry::ACTION_RUN,
        "retrying" => registry::RUN_RESUME,
        "review_started" | "review_verdict" | "review_no_verdict" => registry::CODING_IN_REVIEW,
        "landed" | "completed" => registry::UI_CHECK,
        "final_pr_opened" | "final_pr_reopened" => registry::PR_OPEN,
        "failed" | "gave_up" => registry::UI_WARNING,
        "account_picked" | "account_switched" => registry::NAV_ACCOUNT,
        "waiting_reset" => registry::UI_CLOCK,
        "question_asked" | "question_answered" => registry::UI_HELP,
        "cancelled" | "skipped" => registry::UI_CLOSE,
        _ => registry::UI_INFO,
    }
}

/// The kinds that mean something went wrong: painted amber (web
/// `WARNING_KINDS`, the same four).
pub(crate) fn event_is_warning(kind: &str) -> bool {
    matches!(
        kind,
        "failed" | "gave_up" | "review_no_verdict" | "waiting_reset"
    )
}

/// The trail's order: newest `at` first, ties on the id (descending).
pub(crate) fn newest_first(events: &mut [WorkflowEventRow]) {
    events.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
}

/// `at` as local wall-clock `HH:mm`; empty when it does not parse.
pub(crate) fn local_time(at: Option<&str>) -> String {
    at.and_then(crate::inbox::parse_timestamp)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

impl RenderOnce for WorkflowEventList {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let WorkflowEventList {
            mut events,
            node_label,
            on_open_session,
        } = self;
        newest_first(&mut events);
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        let row_hover = theme.list_hover;
        let amber = theme::tokens::YELLOW.to_hsla();
        let rows = events.into_iter().enumerate().map(|(index, event)| {
            let kind = event.kind.clone().unwrap_or_default();
            let glyph_tint = if event_is_warning(&kind) { amber } else { muted };
            let identifier = event
                .node_id
                .as_deref()
                .and_then(|id| node_label.get(id))
                .cloned();
            let message = event
                .message
                .as_deref()
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let open = event
                .session_id
                .clone()
                .zip(on_open_session.clone());
            crate::surface::flat_row()
                .id(("workflow-event", index))
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Icon::from(event_icon(&kind)).xsmall().text_color(glyph_tint)),
                )
                .children(identifier.map(|identifier| {
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(SharedString::from(identifier))
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(foreground)
                        .child(SharedString::from(message)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(local_time(event.at.as_deref()))),
                )
                .when_some(open, |row, (session_id, on_open)| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(row_hover))
                        .on_click(move |_, window, cx| on_open(&session_id, window, cx))
                })
        });
        div().flex().flex_col().min_w_0().children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, at: &str) -> WorkflowEventRow {
        serde_json::from_value(serde_json::json!({ "id": id, "at": at })).unwrap()
    }

    #[test]
    fn orders_newest_first_then_by_id() {
        let mut events = vec![
            row("a", "2026-09-25T10:00:00Z"),
            row("c", "2026-09-25T11:00:00Z"),
            row("b", "2026-09-25T11:00:00Z"),
        ];
        newest_first(&mut events);
        assert_eq!(
            events.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
    }

    #[test]
    fn maps_every_kind_to_a_concept() {
        // `ExpIcon` is not `PartialEq`: compare the SVG paths they name.
        let path = |icon: ExpIcon| gpui_component::IconNamed::path(icon);
        assert_eq!(path(event_icon("node_started")), path(registry::ACTION_RUN));
        assert_eq!(path(event_icon("landed")), path(registry::UI_CHECK));
        assert_eq!(path(event_icon("gave_up")), path(registry::UI_WARNING));
        assert_eq!(path(event_icon("something_new")), path(registry::UI_INFO));
    }

    #[test]
    fn only_the_four_warning_kinds_tint_amber() {
        for kind in ["failed", "gave_up", "review_no_verdict", "waiting_reset"] {
            assert!(event_is_warning(kind), "{kind}");
        }
        for kind in ["node_started", "landed", "cancelled", "something_new"] {
            assert!(!event_is_warning(kind), "{kind}");
        }
    }

    #[test]
    fn an_unparseable_stamp_has_no_time() {
        assert_eq!(local_time(Some("nope")), "");
        assert_eq!(local_time(None), "");
        assert_eq!(local_time(Some("2026-09-25T10:00:00Z")).len(), 5);
    }
}
