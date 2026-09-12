//! EXP-850 §3 — the WORKFLOW card's pure derivations.
//!
//! A claude `Workflow` tool call fans agents out over phases and reports its
//! progress as a latest-wins `workflow` event ([`steer::WorkflowState`],
//! keyed by the call's own id). The transcript renders that state IN PLACE OF
//! the `Workflow` tool row ([`crate::steer_viewer`] looks it up by
//! `call_id`); this module owns what the card SAYS — the phase strip's
//! counts, each agent's meta line and its one detail line — so the shape is
//! unit-tested instead of read off a running agent.
//!
//! The caption above the card is not here: it is `steer::workflow_caption`,
//! the ×4 function byte-locked by
//! `packages/domain-contract/fixtures/workflow-caption.json`.

use steer::{WorkflowAgent, WorkflowAgentState, WorkflowState, WorkflowStatus};

/// The bucket agents that name no declared phase fall into (and the only
/// bucket of a workflow that declared none at all).
pub(crate) const UNPHASED_TITLE: &str = "Agents";

/// One column of the card's phase strip.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PhaseCount {
    pub(crate) title: String,
    pub(crate) queued: usize,
    pub(crate) running: usize,
    pub(crate) done: usize,
    pub(crate) error: usize,
}

impl PhaseCount {
    pub(crate) fn total(&self) -> usize {
        self.queued + self.running + self.done + self.error
    }

    /// `2 done · 1 running` — the counts that are non-zero, in state order.
    pub(crate) fn caption(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.done > 0 {
            parts.push(format!("{} done", self.done));
        }
        if self.running > 0 {
            parts.push(format!("{} running", self.running));
        }
        if self.queued > 0 {
            parts.push(format!("{} queued", self.queued));
        }
        if self.error > 0 {
            parts.push(format!("{} failed", self.error));
        }
        parts.join(steer::WORKFLOW_CAPTION_SEPARATOR)
    }
}

/// The phase strip: every DECLARED phase in index order, then (when any agent
/// names none of them) one trailing [`UNPHASED_TITLE`] bucket. A phase with
/// no agents still renders — the script declared it, and a strip that grew a
/// column mid-run would jump.
pub(crate) fn phase_counts(state: &WorkflowState) -> Vec<PhaseCount> {
    let mut phases: Vec<&steer::WorkflowPhase> = state.phases.iter().collect();
    phases.sort_by_key(|phase| phase.index);
    let mut counts: Vec<PhaseCount> = phases
        .iter()
        .map(|phase| PhaseCount {
            title: phase.title.trim().to_string(),
            ..PhaseCount::default()
        })
        .collect();
    let mut unphased = PhaseCount {
        title: UNPHASED_TITLE.to_string(),
        ..PhaseCount::default()
    };
    for agent in &state.agents {
        let slot = agent
            .phase_index
            .and_then(|index| phases.iter().position(|phase| phase.index == index))
            .and_then(|position| counts.get_mut(position))
            .unwrap_or(&mut unphased);
        match agent.state {
            WorkflowAgentState::Queued => slot.queued += 1,
            WorkflowAgentState::Running => slot.running += 1,
            WorkflowAgentState::Done => slot.done += 1,
            WorkflowAgentState::Error => slot.error += 1,
        }
    }
    if unphased.total() > 0 {
        counts.push(unphased);
    }
    counts
}

/// An agent's own name: its label, or `Agent {index}` for an unlabelled one.
pub(crate) fn agent_label(agent: &WorkflowAgent) -> String {
    let label = agent.label.trim();
    if label.is_empty() {
        format!("Agent {}", agent.index)
    } else {
        label.to_string()
    }
}

/// The muted meta line beside an agent: model, tokens, tool calls, duration —
/// whichever the publisher reported, in that order. Empty when it reported
/// none of them.
pub(crate) fn agent_meta(agent: &WorkflowAgent) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(model) = agent.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        parts.push(model.to_string());
    }
    if let Some(tokens) = agent.tokens.filter(|tokens| *tokens > 0) {
        parts.push(format!(
            "{} tokens",
            crate::session_rows::format_tokens(tokens)
        ));
    }
    if let Some(calls) = agent.tool_calls.filter(|calls| *calls > 0) {
        parts.push(format!(
            "{calls} tool {}",
            if calls == 1 { "call" } else { "calls" }
        ));
    }
    if let Some(duration) = agent.duration_ms.filter(|ms| *ms > 0) {
        parts.push(crate::session_rows::format_duration(duration as i64));
    }
    parts.join(steer::WORKFLOW_CAPTION_SEPARATOR)
}

/// The ONE detail line an agent row carries: its error when it failed, its
/// result preview when it finished, the tool it is in while it runs.
pub(crate) fn agent_detail(agent: &WorkflowAgent) -> Option<String> {
    let pick = |text: &Option<String>| {
        text.as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    match agent.state {
        WorkflowAgentState::Error => pick(&agent.error).or_else(|| pick(&agent.result_preview)),
        WorkflowAgentState::Done => pick(&agent.result_preview),
        WorkflowAgentState::Running | WorkflowAgentState::Queued => {
            pick(&agent.last_tool_summary).or_else(|| pick(&agent.last_tool))
        }
    }
}

/// The card's own status line under the name: the workflow's summary when it
/// finished, else `{done}/{total} agents`.
pub(crate) fn card_status(state: &WorkflowState) -> String {
    if state.status.is_terminal() {
        if let Some(summary) = state
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
        {
            return summary.to_string();
        }
    }
    let verb = match state.status {
        WorkflowStatus::Running => "agents done",
        WorkflowStatus::Completed => "agents done",
        WorkflowStatus::Failed => "agents done · failed",
        WorkflowStatus::Stopped => "agents done · stopped",
    };
    format!("{}/{} {verb}", state.done_agents(), state.agents.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(index: u32, phase: Option<u32>, state: WorkflowAgentState) -> WorkflowAgent {
        WorkflowAgent {
            index,
            label: format!("alpha:{index}"),
            phase_index: phase,
            state,
            ..WorkflowAgent::default()
        }
    }

    fn state(agents: Vec<WorkflowAgent>, phases: &[(u32, &str)]) -> WorkflowState {
        WorkflowState {
            id: "toolu_1".to_string(),
            name: "wire-probe".to_string(),
            status: WorkflowStatus::Running,
            phases: phases
                .iter()
                .map(|(index, title)| steer::WorkflowPhase {
                    index: *index,
                    title: title.to_string(),
                })
                .collect(),
            agents,
            ..WorkflowState::default()
        }
    }

    /// §3: one column per declared phase, in index order, counting the four
    /// states — and a phase with no agents still draws.
    #[test]
    fn the_phase_strip_counts_every_state_per_phase() {
        let state = state(
            vec![
                agent(1, Some(1), WorkflowAgentState::Done),
                agent(2, Some(1), WorkflowAgentState::Error),
                agent(3, Some(2), WorkflowAgentState::Running),
                agent(4, Some(2), WorkflowAgentState::Queued),
            ],
            &[(2, "Beta"), (1, "Alpha"), (3, "Gamma")],
        );
        let counts = phase_counts(&state);
        assert_eq!(
            counts.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
            vec!["Alpha", "Beta", "Gamma"]
        );
        assert_eq!(counts[0].done, 1);
        assert_eq!(counts[0].error, 1);
        assert_eq!(counts[0].caption(), "1 done · 1 failed");
        assert_eq!(counts[1].running, 1);
        assert_eq!(counts[1].queued, 1);
        assert_eq!(counts[1].caption(), "1 running · 1 queued");
        assert_eq!(counts[2].total(), 0);
        assert_eq!(counts[2].caption(), "");
    }

    /// §3: an agent naming no known phase lands in one trailing bucket, which
    /// is the ONLY bucket of a phase-less workflow.
    #[test]
    fn unphased_agents_get_their_own_bucket() {
        let counts = phase_counts(&state(
            vec![
                agent(1, Some(1), WorkflowAgentState::Done),
                agent(2, Some(9), WorkflowAgentState::Running),
            ],
            &[(1, "Alpha")],
        ));
        assert_eq!(
            counts.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
            vec!["Alpha", UNPHASED_TITLE]
        );
        assert_eq!(counts[1].running, 1);

        let counts = phase_counts(&state(
            vec![agent(1, None, WorkflowAgentState::Queued)],
            &[],
        ));
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].title, UNPHASED_TITLE);
        assert_eq!(counts[0].queued, 1);
    }

    /// §3: the agent row's meta line holds only what the publisher reported.
    #[test]
    fn an_agent_row_reports_what_the_wire_carried() {
        let mut row = agent(1, Some(1), WorkflowAgentState::Done);
        row.model = Some("claude-haiku-4-5-20251001".to_string());
        row.tokens = Some(9_629);
        row.tool_calls = Some(1);
        row.duration_ms = Some(1_075);
        assert_eq!(
            agent_meta(&row),
            "claude-haiku-4-5-20251001 · 9.6k tokens · 1 tool call · 1s"
        );
        // Nothing reported: nothing said.
        assert_eq!(agent_meta(&agent(2, None, WorkflowAgentState::Queued)), "");
        // Zeroes are silence too (an agent that made no calls).
        let mut quiet = agent(3, None, WorkflowAgentState::Running);
        quiet.tool_calls = Some(0);
        quiet.tokens = Some(0);
        assert_eq!(agent_meta(&quiet), "");
        assert_eq!(agent_label(&row), "alpha:1");
        let mut unlabelled = agent(7, None, WorkflowAgentState::Queued);
        unlabelled.label = "  ".to_string();
        assert_eq!(agent_label(&unlabelled), "Agent 7");
    }

    /// §3: the detail line follows the state — error, result, or the tool the
    /// agent is in.
    #[test]
    fn the_detail_line_follows_the_agents_state() {
        let mut running = agent(1, None, WorkflowAgentState::Running);
        running.last_tool = Some("Bash".to_string());
        running.last_tool_summary = Some("cargo test -p ui".to_string());
        assert_eq!(agent_detail(&running).as_deref(), Some("cargo test -p ui"));
        running.last_tool_summary = None;
        assert_eq!(agent_detail(&running).as_deref(), Some("Bash"));

        let mut done = agent(2, None, WorkflowAgentState::Done);
        done.result_preview = Some("ok".to_string());
        done.last_tool = Some("Bash".to_string());
        assert_eq!(agent_detail(&done).as_deref(), Some("ok"));

        let mut failed = agent(3, None, WorkflowAgentState::Error);
        failed.error = Some("exit 1".to_string());
        failed.result_preview = Some("ok".to_string());
        assert_eq!(agent_detail(&failed).as_deref(), Some("exit 1"));

        assert_eq!(agent_detail(&agent(4, None, WorkflowAgentState::Queued)), None);
    }

    /// §3: the summary replaces the progress line once the workflow is over.
    #[test]
    fn the_card_status_becomes_the_summary_when_it_finishes() {
        let mut done = state(
            vec![
                agent(1, None, WorkflowAgentState::Done),
                agent(2, None, WorkflowAgentState::Done),
            ],
            &[],
        );
        assert_eq!(card_status(&done), "2/2 agents done");
        done.status = WorkflowStatus::Completed;
        done.summary = Some("Dynamic workflow \"probe\" completed".to_string());
        assert_eq!(card_status(&done), "Dynamic workflow \"probe\" completed");
        // …and a terminal workflow with NO summary still says where it got to.
        done.summary = None;
        done.status = WorkflowStatus::Failed;
        assert_eq!(card_status(&done), "2/2 agents done · failed");
    }
}
