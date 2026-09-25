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
}

/// Where a host records its audit lines.
pub trait WorkflowEventSink {
    fn record(&self, event: WorkflowEvent);
}

/// The audit line one executed decision produces, if any. EXP-1064 fills the
/// mapping (node_started, review_started, landed, failed, …); `None` until
/// then.
pub fn event_for(_decision: &Decision, _outcome: &Outcome) -> Option<WorkflowEvent> {
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
}
