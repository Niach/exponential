//! EXP-1082 — the OPEN QUESTIONS of a workflow: every live run of it parked
//! on `exponential_sessions_ask_parent`. The question lives on the run's own
//! row, `coding_sessions.pending_question` (jsonb `{question, askedAt}`,
//! synced); the workflow view lists them so a person answers where the work
//! is. Declared here; EXP-1065 implements the reader.

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

/// The open questions of `workflow_id`'s live runs. STUB (EXP-1065).
pub fn workflow_open_questions(
    _sessions: &[CodingSession],
    _workflow_id: &str,
) -> Vec<WorkflowOpenQuestion> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "EXP-1065"]
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
}
