//! EXP-1082 — the workflow host's AUDIT sink. After every [`Decision`] a
//! host executes it asks [`event_for`] whether that deserves a line in the
//! workflow's synced `workflow_events` trail, and hands a `Some` to its
//! [`WorkflowEventSink`] (in production [`TrpcEventSink`] →
//! `workflows.appendEvent`). The mapping itself is EXP-1064's: today
//! [`event_for`] decides nothing, so the seam is wired but silent.

use std::sync::Arc;

use api::TrpcClient;

use super::Decision;

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

/// The audit line one executed decision produces, if any. EXP-1064 fills the
/// mapping (node_started, review_started, landed, failed, …); `None` until
/// then.
pub fn event_for(_decision: &Decision, outcome: &Outcome) -> Option<WorkflowEvent> {
    if matches!(outcome, Outcome::Queued) {
        return None;
    }
    None
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
    use super::*;

    #[test]
    fn event_for_is_silent_until_exp_1064() {
        assert_eq!(event_for(&Decision::OpenFinalPr, &Outcome::Done), None);
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
        assert_eq!(event_for(&start, &Outcome::Queued), None);
    }
}
