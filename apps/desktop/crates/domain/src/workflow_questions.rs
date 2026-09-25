//! EXP-1082 — the OPEN QUESTIONS of a workflow: every live run of it parked
//! on `exponential_sessions_ask_parent`. The question lives on the run's own
//! row, `coding_sessions.pending_question` (jsonb `{question, askedAt}`,
//! synced); the workflow view lists them so a person answers where the work
//! is. The ONE rule, ×4 (web `lib/workflows/open-questions.ts`, iOS
//! `WorkflowQuestions`, Android `WorkflowQuestions`): a live run (`running`
//! / `in_review`) of the workflow, on a NODE, with a non-blank `question`;
//! a planner run's question (no node) reaches the person through the
//! notification and its own composer instead. An open question never
//! changes a node's state, only the badge.

use crate::rows::CodingSession;

/// One question a workflow run waits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowOpenQuestion {
    pub node_id: Option<String>,
    pub session_id: String,
    pub question: String,
    /// ISO-8601 UTC, as the run stamped it.
    pub asked_at: Option<String>,
}

/// The open questions of `workflow_id`'s live runs, oldest first.
pub fn workflow_open_questions(
    sessions: &[CodingSession],
    workflow_id: &str,
) -> Vec<WorkflowOpenQuestion> {
    let mut open: Vec<WorkflowOpenQuestion> = sessions
        .iter()
        .filter(|session| session.workflow_id.as_deref() == Some(workflow_id))
        .filter(|session| session.workflow_node_id.is_some())
        .filter(|session| matches!(session.status.as_deref(), Some("running" | "in_review")))
        .filter_map(|session| {
            let pending = pending_object(session.pending_question.as_ref()?)?;
            let question = pending.get("question")?.as_str()?.trim();
            if question.is_empty() {
                return None;
            }
            Some(WorkflowOpenQuestion {
                node_id: session.workflow_node_id.clone(),
                session_id: session.id.clone(),
                question: question.to_string(),
                asked_at: pending
                    .get("askedAt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect();
    open.sort_by(|left, right| {
        left.asked_at
            .cmp(&right.asked_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    open
}

/// The jsonb as an object: the row deserializer is tolerant, so the value
/// may still be the JSON TEXT of the object (Electric's wire form).
fn pending_object(value: &serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    match value {
        serde_json::Value::Object(map) => Some(map.clone()),
        serde_json::Value::String(text) => serde_json::from_str::<serde_json::Value>(text)
            .ok()?
            .as_object()
            .cloned(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_open_question_of_each_live_run_of_the_workflow() {
        let row = |id: &str, workflow: &str, status: &str, question: Option<&str>| {
            serde_json::from_value::<CodingSession>(serde_json::json!({
                "id": id,
                "status": status,
                "workflow_id": workflow,
                "workflow_node_id": format!("node-{id}"),
                "pending_question": question.map(|q| serde_json::json!({
                    "question": q,
                    "askedAt": "2026-09-25T10:00:00Z"
                }).to_string()),
            }))
            .unwrap()
        };
        let sessions = vec![
            row("a", "wf", "running", Some("Which schema?")),
            row("b", "wf", "running", None),
            row("c", "wf", "ended", Some("Stale")),
            row("d", "other", "running", Some("Not ours")),
        ];
        assert_eq!(
            workflow_open_questions(&sessions, "wf"),
            vec![WorkflowOpenQuestion {
                node_id: Some("node-a".to_string()),
                session_id: "a".to_string(),
                question: "Which schema?".to_string(),
                asked_at: Some("2026-09-25T10:00:00Z".to_string()),
            }]
        );
    }

    /// A planner run (no node) and a blank question are left out; the rest
    /// reads oldest first.
    #[test]
    fn skips_node_less_and_blank_questions_and_orders_by_asked_at() {
        let row = |id: &str, node: Option<&str>, question: &str, at: &str| {
            serde_json::from_value::<CodingSession>(serde_json::json!({
                "id": id,
                "status": "in_review",
                "workflow_id": "wf",
                "workflow_node_id": node,
                "pending_question": { "question": question, "askedAt": at },
            }))
            .unwrap()
        };
        let sessions = vec![
            row("b", Some("node-b"), "Later?", "2026-09-25T11:00:00Z"),
            row("plan", None, "Runner device?", "2026-09-25T09:00:00Z"),
            row("a", Some("node-a"), "Earlier?", "2026-09-25T10:00:00Z"),
            row("blank", Some("node-c"), "   ", "2026-09-25T09:30:00Z"),
        ];
        let ids: Vec<String> = workflow_open_questions(&sessions, "wf")
            .into_iter()
            .map(|question| question.session_id)
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }
}
