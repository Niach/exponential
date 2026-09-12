//! EXP-850 §3/§7 — the `workflow` wire payload and the ONE caption derived
//! from it.
//!
//! A claude `Workflow` tool call runs a script that fans agents out over
//! phases; the CLI reports its progress on `system/task_progress` frames the
//! engine folds into [`WorkflowState`] and publishes as a latest-wins
//! `workflow` event, one per workflow id (the `Workflow` call's own
//! `tool_use_id`, so clients patch the card onto the tool row they already
//! hold).
//!
//! [`workflow_caption`] is the Rust mirror of
//! `packages/domain-contract/src/workflow-caption.ts`, byte-locked by
//! `packages/domain-contract/fixtures/workflow-caption.json` — the same file
//! the web, iOS and Android mirrors replay. The caption is what a session row
//! renders as its second line (`coding_sessions.agent_caption`, §8) and what
//! the steer view's working strip says while a workflow runs.

use serde::{Deserialize, Serialize};

/// The segment separator: space, MIDDLE DOT (U+00B7), space — byte-identical
/// to the TS `WORKFLOW_CAPTION_SEPARATOR`.
pub const WORKFLOW_CAPTION_SEPARATOR: &str = " \u{b7} ";

/// How many phases one `workflow` frame may carry (the relay's zod cap).
pub const WORKFLOW_PHASES_MAX: usize = 32;
/// How many agents one `workflow` frame may carry (the relay's zod cap).
pub const WORKFLOW_AGENTS_MAX: usize = 64;

/// Contract `workflowStatus`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Stopped,
}

impl WorkflowStatus {
    /// Every value, in contract order.
    pub const ALL: [WorkflowStatus; 4] = [
        WorkflowStatus::Running,
        WorkflowStatus::Completed,
        WorkflowStatus::Failed,
        WorkflowStatus::Stopped,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowStatus::Running => "running",
            WorkflowStatus::Completed => "completed",
            WorkflowStatus::Failed => "failed",
            WorkflowStatus::Stopped => "stopped",
        }
    }

    /// A CLI task status folded onto the contract vocabulary: `completed` and
    /// `failed` keep their word, every other terminal one (`stopped`,
    /// `cancelled`, `killed`) is a stop, and anything else is still running.
    pub fn from_task_status(status: &str) -> WorkflowStatus {
        match status {
            "completed" => WorkflowStatus::Completed,
            "failed" => WorkflowStatus::Failed,
            "stopped" | "cancelled" | "canceled" | "killed" | "aborted" | "timeout" => {
                WorkflowStatus::Stopped
            }
            _ => WorkflowStatus::Running,
        }
    }

    /// Whether the workflow is over — the card stops ticking and the caption
    /// drops off the session row.
    pub fn is_terminal(self) -> bool {
        !matches!(self, WorkflowStatus::Running)
    }
}

/// Contract `workflowAgentState`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentState {
    #[default]
    Queued,
    Running,
    Done,
    Error,
}

impl WorkflowAgentState {
    /// Every value, in contract order.
    pub const ALL: [WorkflowAgentState; 4] = [
        WorkflowAgentState::Queued,
        WorkflowAgentState::Running,
        WorkflowAgentState::Done,
        WorkflowAgentState::Error,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowAgentState::Queued => "queued",
            WorkflowAgentState::Running => "running",
            WorkflowAgentState::Done => "done",
            WorkflowAgentState::Error => "error",
        }
    }

    /// The CLI's own `workflow_agent.state` plus whether it carries a
    /// `startedAt`: `start` without one is still QUEUED (the workflow named
    /// the agent before it had a process), `start` with one is running.
    pub fn from_cli(state: &str, started: bool) -> WorkflowAgentState {
        match state {
            "done" => WorkflowAgentState::Done,
            "error" => WorkflowAgentState::Error,
            "start" | "running" if started => WorkflowAgentState::Running,
            _ => WorkflowAgentState::Queued,
        }
    }

    /// Done or errored — the numerator of the caption's `{done}/{total}`.
    pub fn is_finished(self) -> bool {
        matches!(self, WorkflowAgentState::Done | WorkflowAgentState::Error)
    }
}

/// One phase of a workflow script (`workflow_phase` progress entries, latest
/// per index).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkflowPhase {
    pub index: u32,
    pub title: String,
}

/// One agent of a workflow (`workflow_agent` progress entries, latest per
/// index). Every free-text field is cut to the contract's
/// `steerWorking.previewMax` by the publisher.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowAgent {
    pub index: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_index: Option<u32>,
    /// The agent's own id — the `task_id` a `task_started` for the SAME agent
    /// carries (EXP-856: a `SendMessage` to a live agent starts a second copy
    /// under this very id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: WorkflowAgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_tool_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The `workflow` event's payload — latest-wins per [`WorkflowState::id`]
/// everywhere (the relay's `workflow:{id}` slot key, the journal's keyed
/// fold, the feed's side map).
///
/// Field order here IS serialization order and the relay's zod is declared in
/// the same one — never reorder (the frames tests are byte-exact).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowState {
    /// The `Workflow` tool call's `tool_use_id` — the SAME id as its `tool`
    /// row, which is what clients patch the card onto.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: WorkflowStatus,
    pub phases: Vec<WorkflowPhase>,
    pub agents: Vec<WorkflowAgent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
}

impl WorkflowState {
    /// Agents in `done` or `error` — the caption's numerator.
    pub fn done_agents(&self) -> usize {
        self.agents.iter().filter(|agent| agent.state.is_finished()).count()
    }

    /// The agent whose phase names the caption: the RUNNING one with the
    /// highest index, else the highest-index agent of any state.
    pub fn lead_agent(&self) -> Option<&WorkflowAgent> {
        let mut running: Option<&WorkflowAgent> = None;
        let mut latest: Option<&WorkflowAgent> = None;
        for agent in &self.agents {
            if latest.is_none_or(|held| agent.index >= held.index) {
                latest = Some(agent);
            }
            if agent.state == WorkflowAgentState::Running
                && running.is_none_or(|held| agent.index >= held.index)
            {
                running = Some(agent);
            }
        }
        running.or(latest)
    }

    /// The phase title of an agent, when it names one this workflow holds.
    pub fn phase_title(&self, agent: &WorkflowAgent) -> Option<&str> {
        let index = agent.phase_index?;
        self.phases
            .iter()
            .find(|phase| phase.index == index)
            .map(|phase| phase.title.trim())
            .filter(|title| !title.is_empty())
    }
}

/// EXP-850 §7 — the workflow caption, byte-identical to the TS
/// `workflowCaption` and its iOS/Android mirrors.
pub fn workflow_caption(workflow: &WorkflowState) -> String {
    let head = format!("Workflow {}", workflow.name.trim());
    let sep = WORKFLOW_CAPTION_SEPARATOR;
    let total = workflow.agents.len();
    match workflow.status {
        WorkflowStatus::Completed => {
            let noun = if total == 1 { "agent" } else { "agents" };
            format!("{head}{sep}done{sep}{total} {noun}")
        }
        WorkflowStatus::Failed => format!("{head}{sep}failed"),
        WorkflowStatus::Stopped => format!("{head}{sep}stopped"),
        WorkflowStatus::Running if total == 0 => format!("{head}{sep}starting"),
        WorkflowStatus::Running => {
            let caption = format!("{head}{sep}{}/{total} agents done", workflow.done_agents());
            match workflow
                .lead_agent()
                .and_then(|agent| workflow.phase_title(agent))
            {
                Some(phase) => format!("{caption}{sep}{phase}"),
                None => caption,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract fixture, byte-locked ×4 (web `workflow-caption.test.ts`,
    /// iOS `WorkflowCaptionTests`, Android `WorkflowCaptionTest`).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/workflow-caption.json");

    /// The fixture speaks the CAPTION's input (the TS `WorkflowCaptionInput`),
    /// which is deliberately narrower than the wire state: no id, no summary,
    /// no per-agent telemetry — nothing the caption reads. Widening the wire
    /// type's serde to swallow it would make a malformed `workflow` frame
    /// parse instead of being dropped, so the case is lifted INTO a
    /// [`WorkflowState`] here.
    #[derive(Deserialize)]
    struct CaseInput {
        name: String,
        status: String,
        #[serde(default)]
        phases: Vec<WorkflowPhase>,
        #[serde(default)]
        agents: Vec<CaseAgent>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CaseAgent {
        index: u32,
        #[serde(default)]
        label: String,
        #[serde(default)]
        phase_index: Option<u32>,
        state: String,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        workflow: CaseInput,
        expected: String,
    }

    impl CaseInput {
        fn state(self) -> WorkflowState {
            WorkflowState {
                id: "toolu_fixture".to_string(),
                name: self.name,
                status: WorkflowStatus::ALL
                    .into_iter()
                    .find(|status| status.as_str() == self.status)
                    .unwrap_or(WorkflowStatus::Running),
                phases: self.phases,
                agents: self
                    .agents
                    .into_iter()
                    .map(|agent| WorkflowAgent {
                        index: agent.index,
                        label: agent.label,
                        phase_index: agent.phase_index,
                        state: WorkflowAgentState::ALL
                            .into_iter()
                            .find(|state| state.as_str() == agent.state)
                            .unwrap_or(WorkflowAgentState::Queued),
                        ..WorkflowAgent::default()
                    })
                    .collect(),
                ..WorkflowState::default()
            }
        }
    }

    #[test]
    fn the_caption_matches_the_contract_fixture() {
        let cases: Vec<Case> = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert!(cases.len() >= 8, "the fixture must cover every status");
        for case in cases {
            let name = case.name;
            assert_eq!(
                workflow_caption(&case.workflow.state()),
                case.expected,
                "case {name}"
            );
        }
    }

    #[test]
    fn the_status_and_state_vocabularies_match_the_contract() {
        let statuses: Vec<&str> = WorkflowStatus::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(statuses, domain::contract::WORKFLOW_STATUS_VALUES);
        let states: Vec<&str> = WorkflowAgentState::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(states, domain::contract::WORKFLOW_AGENT_STATE_VALUES);
    }

    #[test]
    fn a_start_without_a_started_stamp_is_queued() {
        assert_eq!(
            WorkflowAgentState::from_cli("start", false),
            WorkflowAgentState::Queued
        );
        assert_eq!(
            WorkflowAgentState::from_cli("start", true),
            WorkflowAgentState::Running
        );
        assert_eq!(WorkflowAgentState::from_cli("done", false), WorkflowAgentState::Done);
        assert_eq!(WorkflowAgentState::from_cli("error", true), WorkflowAgentState::Error);
    }

    #[test]
    fn every_terminal_task_status_folds_onto_the_contract() {
        assert_eq!(
            WorkflowStatus::from_task_status("completed"),
            WorkflowStatus::Completed
        );
        assert_eq!(WorkflowStatus::from_task_status("failed"), WorkflowStatus::Failed);
        for status in ["stopped", "cancelled", "killed"] {
            assert_eq!(
                WorkflowStatus::from_task_status(status),
                WorkflowStatus::Stopped,
                "{status}"
            );
        }
        assert_eq!(WorkflowStatus::from_task_status("running"), WorkflowStatus::Running);
    }

    #[test]
    fn the_lead_agent_prefers_the_highest_running_index() {
        let workflow = WorkflowState {
            name: "w".to_string(),
            status: WorkflowStatus::Running,
            phases: vec![
                WorkflowPhase { index: 1, title: "Alpha".to_string() },
                WorkflowPhase { index: 2, title: "Beta".to_string() },
            ],
            agents: vec![
                WorkflowAgent {
                    index: 3,
                    phase_index: Some(2),
                    state: WorkflowAgentState::Done,
                    ..WorkflowAgent::default()
                },
                WorkflowAgent {
                    index: 2,
                    phase_index: Some(1),
                    state: WorkflowAgentState::Running,
                    ..WorkflowAgent::default()
                },
            ],
            ..WorkflowState::default()
        };
        let lead = workflow.lead_agent().expect("an agent");
        assert_eq!(lead.index, 2);
        assert_eq!(workflow.phase_title(lead), Some("Alpha"));
    }
}
