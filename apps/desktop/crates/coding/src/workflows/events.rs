//! EXP-1082 — the workflow host's AUDIT sink. After every [`Decision`] a
//! host executes it asks [`event_for`] whether that deserves a line in the
//! workflow's synced `workflow_events` trail, and hands a `Some` to its
//! [`WorkflowEventSink`] (in production [`TrpcEventSink`] →
//! `workflows.appendEvent`). EXP-1064 fills the mapping; a refusal the
//! engine repeats every beat (a land refusal, a skipped start) writes
//! nothing, so the trail only says what CHANGED. The message never names the
//! node: the UI resolves `node_id`.

use std::sync::Arc;

use api::TrpcClient;

use super::{Decision, ReviewRunEnd};

/// One audit line (the `workflows.appendEvent` wire; `kind` = a contract
/// `wfEventKind` value).
pub use api::workflows::WorkflowEvent;

/// What the host did with one decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Executed.
    Done,
    /// Deliberately not executed this pass (already in flight, nothing to do).
    Skipped,
    /// Tried and failed; the sentence the host logged.
    Failed(String),
    /// Handed to a LAUNCH SITE that records the real outcome later (the
    /// desktop host queues `StartNode` / `StartReview` orders for its
    /// foreground). The pass never hands this to the sink — one decision,
    /// ONE audit line, on both hosts — and [`event_for`] answers `None` to
    /// it whatever EXP-1064 maps.
    Queued,
}

/// Where a host records its audit lines.
pub trait WorkflowEventSink {
    fn record(&self, event: WorkflowEvent);
}

/// The trail's per-line cap (the server clamps too).
const MESSAGE_MAX: usize = 500;

fn event(
    workflow_id: &str,
    node_id: Option<&str>,
    session_id: Option<&str>,
    kind: &str,
    message: String,
) -> Option<WorkflowEvent> {
    let message = if message.chars().count() > MESSAGE_MAX {
        let mut cut: String = message.chars().take(MESSAGE_MAX - 1).collect();
        cut.push('…');
        cut
    } else {
        message
    };
    Some(WorkflowEvent {
        workflow_id: workflow_id.to_string(),
        node_id: node_id.map(str::to_string),
        session_id: session_id.map(str::to_string),
        kind: kind.to_string(),
        message,
    })
}

/// ` · account <a>` when the start named an agent profile.
fn with_account(message: &str, account: Option<&String>) -> String {
    match account {
        Some(account) => format!("{message} · account {account}"),
        None => message.to_string(),
    }
}

/// The audit line one executed decision of `workflow_id` produces, if any
/// (kinds = contract `wfEventKind`).
pub fn event_for(workflow_id: &str, decision: &Decision, outcome: &Outcome) -> Option<WorkflowEvent> {
    match (decision, outcome) {
        // A queued order is recorded ONCE, at its launch site (`Outcome::Queued`
        // never reaches the sink either way); a skip changed nothing.
        (_, Outcome::Skipped | Outcome::Queued) => None,
        (
            Decision::StartNode {
                node_id,
                attempt,
                base_branch,
                account,
                ..
            },
            Outcome::Done,
        ) => {
            let (kind, message) = if *attempt <= 1 {
                ("node_started", format!("Started on {base_branch}"))
            } else {
                ("retrying", format!("Retrying (attempt {attempt}) on {base_branch}"))
            };
            event(workflow_id, Some(node_id), None, kind, with_account(&message, account.as_ref()))
        }
        (Decision::StartNode { node_id, .. }, Outcome::Failed(err)) => event(
            workflow_id,
            Some(node_id),
            None,
            "failed",
            format!("Start failed: {err}"),
        ),
        (Decision::StartReview { node_id, account, .. }, Outcome::Done) => event(
            workflow_id,
            Some(node_id),
            None,
            "review_started",
            with_account("Started the review", account.as_ref()),
        ),
        (Decision::StartReview { node_id, .. }, Outcome::Failed(err)) => event(
            workflow_id,
            Some(node_id),
            None,
            "failed",
            format!("Review start failed: {err}"),
        ),
        (Decision::LandNode { node_id }, Outcome::Done) => event(
            workflow_id,
            Some(node_id),
            None,
            "landed",
            "Landed into the integration branch".to_string(),
        ),
        (Decision::SetNodeState { node_id, state, note }, Outcome::Done) if state == "failed" => {
            event(
                workflow_id,
                Some(node_id),
                None,
                "failed",
                note.clone()
                    .filter(|note| !note.trim().is_empty())
                    .unwrap_or_else(|| "Failed".to_string()),
            )
        }
        (Decision::OpenFinalPr, Outcome::Done) => event(
            workflow_id,
            None,
            None,
            "final_pr_opened",
            "Opened the final pull request".to_string(),
        ),
        (Decision::DeleteIntegrationBranch, Outcome::Done) => event(
            workflow_id,
            None,
            None,
            "cancelled",
            "Cancelled: deleted the integration branch".to_string(),
        ),
        _ => None,
    }
}

/// A FOREGROUND launch's own line (the desktop host's `LaunchAudit`): the
/// start/review decision carries its workflow.
pub fn launch_event_for(decision: &Decision, outcome: &Outcome) -> Option<WorkflowEvent> {
    match decision {
        Decision::StartNode { workflow_id, .. } | Decision::StartReview { workflow_id, .. } => {
            event_for(workflow_id, decision, outcome)
        }
        _ => None,
    }
}

/// The audit line a settled reviewer run produces, if any (both hosts'
/// `settle_review_runs` loop). A verdict writes none here: the review
/// submission is its own line.
pub fn event_for_review_end(workflow_id: &str, end: &ReviewRunEnd) -> Option<WorkflowEvent> {
    match end {
        ReviewRunEnd::Verdict { .. } => None,
        ReviewRunEnd::Followed { node_id, session_id } => event(
            workflow_id,
            Some(node_id),
            Some(session_id),
            "following_resume",
            "The review was resumed as another run; following it".to_string(),
        ),
        ReviewRunEnd::Adopted { node_id, session_id } => event(
            workflow_id,
            Some(node_id),
            Some(session_id),
            "adopting_run",
            "Adopting a live review run that was not started by this host".to_string(),
        ),
        ReviewRunEnd::Retry { node_id, failures } => event(
            workflow_id,
            Some(node_id),
            None,
            "review_no_verdict",
            format!("The review run ended without a verdict ({failures}); trying again"),
        ),
        ReviewRunEnd::GaveUp { node_id, note } => {
            event(workflow_id, Some(node_id), None, "gave_up", note.clone())
        }
    }
}

/// The production sink: `workflows.appendEvent`, best effort — a refusal is
/// logged, never surfaced (the trail must not stall the engine).
#[derive(Clone)]
pub struct TrpcEventSink {
    trpc: Arc<TrpcClient>,
}

impl TrpcEventSink {
    pub fn new(trpc: Arc<TrpcClient>) -> Self {
        Self { trpc }
    }
}

impl WorkflowEventSink for TrpcEventSink {
    fn record(&self, event: WorkflowEvent) {
        if let Err(err) = api::workflows::append_event(&self.trpc, &event) {
            log::warn!(
                "workflow {}: appending the {} event failed: {err}",
                event.workflow_id,
                event.kind
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::WfSessionRole;
    use super::*;

    fn start(attempt: i64, account: Option<&str>) -> Decision {
        Decision::StartNode {
            node_id: "n1".to_string(),
            attempt,
            base_branch: "exp/wf-1".to_string(),
            model: None,
            workflow_id: "w".to_string(),
            role: WfSessionRole::Author,
            account: account.map(str::to_string),
        }
    }

    fn review(account: Option<&str>) -> Decision {
        Decision::StartReview {
            node_id: "n1".to_string(),
            wave: 0,
            model: None,
            adversarial: false,
            workflow_id: "w".to_string(),
            role: WfSessionRole::Review,
            account: account.map(str::to_string),
        }
    }

    fn line(event: Option<WorkflowEvent>) -> Option<(Option<String>, Option<String>, String, String)> {
        event.map(|event| {
            assert_eq!(event.workflow_id, "w");
            (event.node_id, event.session_id, event.kind, event.message)
        })
    }

    fn at(node: &str, kind: &str, message: &str) -> Option<(Option<String>, Option<String>, String, String)> {
        Some((Some(node.to_string()), None, kind.to_string(), message.to_string()))
    }

    #[test]
    fn maps_every_decision_outcome() {
        let failed = |msg: &str| Outcome::Failed(msg.to_string());
        let set_state = |state: &str, note: Option<&str>| Decision::SetNodeState {
            node_id: "n1".to_string(),
            state: state.to_string(),
            note: note.map(str::to_string),
        };
        let cases: Vec<(Decision, Outcome, Option<(Option<String>, Option<String>, String, String)>)> = vec![
            (start(1, None), Outcome::Done, at("n1", "node_started", "Started on exp/wf-1")),
            (start(0, None), Outcome::Done, at("n1", "node_started", "Started on exp/wf-1")),
            (
                start(1, Some("work")),
                Outcome::Done,
                at("n1", "node_started", "Started on exp/wf-1 · account work"),
            ),
            (start(3, None), Outcome::Done, at("n1", "retrying", "Retrying (attempt 3) on exp/wf-1")),
            (
                start(2, Some("work")),
                Outcome::Done,
                at("n1", "retrying", "Retrying (attempt 2) on exp/wf-1 · account work"),
            ),
            (start(1, None), failed("no clone"), at("n1", "failed", "Start failed: no clone")),
            (start(1, None), Outcome::Skipped, None),
            (review(None), Outcome::Done, at("n1", "review_started", "Started the review")),
            (
                review(Some("alt")),
                Outcome::Done,
                at("n1", "review_started", "Started the review · account alt"),
            ),
            (review(None), failed("gone"), at("n1", "failed", "Review start failed: gone")),
            (review(None), Outcome::Skipped, None),
            (
                Decision::LandNode { node_id: "n1".to_string() },
                Outcome::Done,
                at("n1", "landed", "Landed into the integration branch"),
            ),
            (Decision::LandNode { node_id: "n1".to_string() }, failed("refused"), None),
            (Decision::LandNode { node_id: "n1".to_string() }, Outcome::Skipped, None),
            (set_state("failed", Some("Tests broke")), Outcome::Done, at("n1", "failed", "Tests broke")),
            (set_state("failed", None), Outcome::Done, at("n1", "failed", "Failed")),
            (set_state("running", None), Outcome::Done, None),
            (set_state("failed", None), failed("x"), None),
            (
                Decision::OpenFinalPr,
                Outcome::Done,
                Some((None, None, "final_pr_opened".to_string(), "Opened the final pull request".to_string())),
            ),
            (Decision::OpenFinalPr, failed("x"), None),
            (
                Decision::DeleteIntegrationBranch,
                Outcome::Done,
                Some((
                    None,
                    None,
                    "cancelled".to_string(),
                    "Cancelled: deleted the integration branch".to_string(),
                )),
            ),
            (Decision::DeleteIntegrationBranch, failed("x"), None),
            (Decision::EnsureIntegrationBranch, Outcome::Done, None),
            (
                Decision::MergeBase {
                    node_id: "n1".to_string(),
                    branch: "exp/EXP-1".to_string(),
                    base_branch: "b".to_string(),
                    sha: "abc".to_string(),
                },
                Outcome::Done,
                None,
            ),
            (
                Decision::SetSerialEdge {
                    node_id: "n1".to_string(),
                    state: "running".to_string(),
                    after: vec!["n2".to_string()],
                },
                Outcome::Done,
                None,
            ),
            (
                Decision::WakeRun {
                    node_id: "n1".to_string(),
                    session_id: "s".to_string(),
                    mode: super::super::WakeMode::Resume,
                    reason: super::super::WakeReason::UpstreamConflict {
                        base_branch: "b".to_string(),
                        sha: "abc".to_string(),
                    },
                },
                Outcome::Done,
                None,
            ),
            (
                Decision::Nudge {
                    session_id: "s".to_string(),
                    key: "k".to_string(),
                    text: "t".to_string(),
                },
                Outcome::Done,
                None,
            ),
            (Decision::KillSession { session_id: "s".to_string() }, Outcome::Done, None),
            (Decision::DeleteBase { base_branch: "b".to_string() }, Outcome::Done, None),
        ];
        for (decision, outcome, expected) in cases {
            assert_eq!(
                line(event_for("w", &decision, &outcome)),
                expected,
                "{decision:?} / {outcome:?}"
            );
        }
    }

    #[test]
    fn caps_the_message() {
        let long = "x".repeat(900);
        let event = event_for("w", &start(1, None), &Outcome::Failed(long)).unwrap();
        assert_eq!(event.message.chars().count(), MESSAGE_MAX);
    }

    #[test]
    fn a_launch_line_uses_the_decisions_own_workflow() {
        assert_eq!(
            line(launch_event_for(&start(1, None), &Outcome::Done)),
            at("n1", "node_started", "Started on exp/wf-1")
        );
        assert_eq!(launch_event_for(&Decision::OpenFinalPr, &Outcome::Done), None);
    }

    #[test]
    fn a_queued_start_writes_no_line() {
        // The desktop pass hands a start/review order to its launch site as
        // `Queued`; the launch records the real outcome, the pass nothing.
        assert_eq!(event_for("w", &start(1, None), &Outcome::Queued), None);
        assert_eq!(event_for("w", &review(None), &Outcome::Queued), None);
        assert_eq!(
            line(event_for("w", &start(1, None), &Outcome::Failed("x".to_string()))),
            at("n1", "failed", "Start failed: x")
        );
        assert_eq!(
            line(event_for("w", &Decision::LandNode { node_id: "n1".to_string() }, &Outcome::Done)),
            at("n1", "landed", "Landed into the integration branch")
        );
    }

    #[test]
    fn maps_every_review_run_end() {
        let with_session = |node: &str, session: &str, kind: &str, message: &str| {
            Some((Some(node.to_string()), Some(session.to_string()), kind.to_string(), message.to_string()))
        };
        let cases = vec![
            (ReviewRunEnd::Verdict { node_id: "n1".to_string() }, None),
            (
                ReviewRunEnd::Followed { node_id: "n1".to_string(), session_id: "s2".to_string() },
                with_session("n1", "s2", "following_resume", "The review was resumed as another run; following it"),
            ),
            (
                ReviewRunEnd::Adopted { node_id: "n1".to_string(), session_id: "s3".to_string() },
                with_session(
                    "n1",
                    "s3",
                    "adopting_run",
                    "Adopting a live review run that was not started by this host",
                ),
            ),
            (
                ReviewRunEnd::Retry { node_id: "n1".to_string(), failures: 2 },
                at("n1", "review_no_verdict", "The review run ended without a verdict (2); trying again"),
            ),
            (
                ReviewRunEnd::GaveUp { node_id: "n1".to_string(), note: "Gave up after 3 tries".to_string() },
                at("n1", "gave_up", "Gave up after 3 tries"),
            ),
        ];
        for (end, expected) in cases {
            assert_eq!(line(event_for_review_end("w", &end)), expected, "{end:?}");
        }
    }

    /// The invariant EXP-1064 relies on: a queued order is recorded ONCE, at
    /// its launch site, never again by the pass that queued it.
    #[test]
    fn a_queued_outcome_is_never_an_event() {
        let start = Decision::StartNode {
            node_id: "n".to_string(),
            attempt: 1,
            base_branch: "exp/wf-abcdef12".to_string(),
            model: None,
            workflow_id: "wf".to_string(),
            role: super::super::WfSessionRole::Author,
            account: None,
        };
        assert_eq!(event_for("wf", &start, &Outcome::Queued), None);
    }
}
