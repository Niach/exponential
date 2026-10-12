//! EXP-1240 (EXP-1259 D12) — THE Work card: ONE card for a claude workflow, a
//! task list and a subagent, replacing the workflow card, the task list block
//! and the subagent group rows.
//!
//! This is the pure MODEL the card draws; the ui crate paints it. Rust mirror
//! of `packages/domain-contract/src/work-card.ts`, byte-locked by
//! `packages/domain-contract/fixtures/work-card.json` (the same file web, iOS
//! and Android replay).
//!
//! The card: a HEADER (kind icon, name, kind word, progress, elapsed, a
//! spinner while running), a SEGMENT BAR (one segment per phase or task,
//! never for a subagent) and ROWS. Finished leading phases FOLD into one row;
//! the running (or failed) phase lists its lanes one line each; a done phase
//! after it is one collapsed row, a pending phase a bare title. No report
//! text, no token counts, no JSON.

use serde::{Deserialize, Serialize};

use crate::frames::{TaskListEntry, TaskListStatus};
use crate::workflow::WorkflowState;

/// Space, MIDDLE DOT, space — the TS `WORK_CARD_SEPARATOR`.
pub const WORK_CARD_SEPARATOR: &str = " \u{b7} ";

/// The fixture's `geometry`, in px.
pub mod geometry {
    pub const HEADER: f32 = 40.0;
    pub const ROW: f32 = 32.0;
    pub const LANE: f32 = 28.0;
    pub const LANE_INDENT: f32 = 28.0;
    pub const SEGMENT_HEIGHT: f32 = 3.0;
    pub const SEGMENT_GAP: f32 = 4.0;
}

/// The fixture's `copy`.
mod copy {
    pub const WORKFLOW: &str = "Workflow";
    pub const TASKS: &str = "Tasks";
    pub const SUBAGENT: &str = "Subagent";
    pub const PHASE: &str = "Phase";
    pub const LOOSE_PHASE: &str = "Agents";
    pub const AGENT: &str = "Agent";
    pub const LANE: &str = "lane";
    pub const LANES: &str = "lanes";
    pub const CALL: &str = "call";
    pub const CALLS: &str = "calls";
    pub const RUNNING: &str = "running";
    pub const DONE: &str = "done";
    pub const FAILED: &str = "failed";
}

/// The fixture's `icons` (concept names, packages/icons/icons.json).
pub mod icons {
    pub const WORKFLOW: &str = "session-tree";
    pub const TASKS: &str = "ui-checklist";
    pub const SUBAGENT: &str = "coding-subagent";
}

/// One agent of a workflow, as the card reads it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkCardWorkflowAgent {
    pub index: u32,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub phase_index: Option<u32>,
    #[serde(default)]
    pub model: Option<String>,
    /// Contract `workflowAgentState`: queued | running | done | error.
    pub state: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct WorkCardPhaseInput {
    pub index: u32,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WorkCardTask {
    pub content: String,
    /// pending | in_progress | completed
    pub status: String,
}

/// The TS `WorkCardInput`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkCardInput {
    #[serde(rename_all = "camelCase")]
    Workflow {
        name: String,
        /// Contract `workflowStatus`.
        status: String,
        #[serde(default)]
        phases: Option<Vec<WorkCardPhaseInput>>,
        #[serde(default)]
        agents: Option<Vec<WorkCardWorkflowAgent>>,
        #[serde(default)]
        elapsed_ms: Option<u64>,
    },
    Tasks { tasks: Vec<WorkCardTask> },
    #[serde(rename_all = "camelCase")]
    Subagent {
        #[serde(default)]
        title: Option<String>,
        /// running | done | error
        state: String,
        #[serde(default)]
        tool_calls: Option<u64>,
        #[serde(default)]
        elapsed_ms: Option<u64>,
    },
}

impl WorkCardInput {
    /// The card input of a `workflow` payload; `elapsed_ms` = the client's
    /// clock against the workflow's start (the payload carries none).
    pub fn from_workflow(workflow: &WorkflowState, elapsed_ms: Option<u64>) -> Self {
        WorkCardInput::Workflow {
            name: workflow.name.clone(),
            status: workflow.status.as_str().to_string(),
            phases: Some(
                workflow
                    .phases
                    .iter()
                    .map(|phase| WorkCardPhaseInput {
                        index: phase.index,
                        title: phase.title.clone(),
                    })
                    .collect(),
            ),
            agents: Some(
                workflow
                    .agents
                    .iter()
                    .map(|agent| WorkCardWorkflowAgent {
                        index: agent.index,
                        label: Some(agent.label.clone()),
                        phase_index: agent.phase_index,
                        model: agent.model.clone(),
                        state: agent.state.as_str().to_string(),
                        duration_ms: agent.duration_ms,
                    })
                    .collect(),
            ),
            elapsed_ms,
        }
    }

    /// The card input of the run's `task_list` state.
    pub fn from_tasks(tasks: &[TaskListEntry]) -> Self {
        WorkCardInput::Tasks {
            tasks: tasks
                .iter()
                .map(|task| WorkCardTask {
                    content: task.content.clone(),
                    status: match task.status {
                        TaskListStatus::Pending => "pending",
                        TaskListStatus::InProgress => "in_progress",
                        TaskListStatus::Completed => "completed",
                    }
                    .to_string(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Done,
    Running,
    Pending,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct WorkCardSegment {
    pub state: WorkState,
    /// 0..1, how much of the segment is filled.
    pub fill: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WorkCardLane {
    pub label: String,
    pub model: Option<String>,
    /// queued | running | done | error
    pub state: String,
    pub duration: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkCardRow {
    Folded {
        label: String,
        lanes: String,
        duration: Option<String>,
    },
    Phase {
        title: String,
        state: WorkState,
        detail: Option<String>,
        duration: Option<String>,
        lanes: Vec<WorkCardLane>,
    },
    Task {
        label: String,
        state: WorkState,
    },
    Status {
        label: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkCardKind {
    Workflow,
    Tasks,
    Subagent,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkCard {
    pub kind: WorkCardKind,
    /// A concept icon (packages/icons/icons.json).
    pub icon: String,
    pub name: String,
    pub kind_word: Option<String>,
    pub progress: Option<String>,
    pub elapsed: Option<String>,
    pub running: bool,
    pub segments: Vec<WorkCardSegment>,
    pub rows: Vec<WorkCardRow>,
}

/// Coarse: `42s`, `48m`, `1h 12m`, `2h`. Header + folded rows.
pub fn work_duration_short(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if rest == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {rest}m")
    }
}

/// Fine: `42s`, `8m 29s`, `1h 12m`. Lane rows.
pub fn work_duration_long(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m {}s", seconds % 60);
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

/// `claude-opus-5-5` → `opus`; anything else as given; blank → `None`.
pub fn work_model_label(model: Option<&str>) -> Option<String> {
    let trimmed = model.unwrap_or("").trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("claude-") {
        let family: String = rest.chars().take_while(|c| c.is_ascii_lowercase()).collect();
        if !family.is_empty() {
            return Some(family);
        }
    }
    Some(trimmed.to_string())
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn settled(state: &str) -> bool {
    state == "done" || state == "error"
}

struct Phase<'a> {
    title: String,
    agents: Vec<&'a WorkCardWorkflowAgent>,
}

fn phase_state(phase: &Phase) -> WorkState {
    if phase.agents.is_empty() {
        return WorkState::Pending;
    }
    if phase.agents.iter().all(|agent| settled(&agent.state)) {
        if phase.agents.iter().any(|agent| agent.state == "error") {
            WorkState::Failed
        } else {
            WorkState::Done
        }
    } else {
        WorkState::Running
    }
}

fn phase_span_ms(phase: &Phase) -> Option<u64> {
    phase.agents.iter().filter_map(|agent| agent.duration_ms).max()
}

fn phase_detail(phase: &Phase) -> String {
    let count = |state: &str| phase.agents.iter().filter(|agent| agent.state == state).count();
    let mut parts = Vec::new();
    for (state, word) in [("done", "done"), ("error", "failed"), ("running", "running"), ("queued", "queued")] {
        let n = count(state);
        if n > 0 {
            parts.push(format!("{n} {word}"));
        }
    }
    parts.join(WORK_CARD_SEPARATOR)
}

fn lane(agent: &WorkCardWorkflowAgent) -> WorkCardLane {
    let label = agent.label.as_deref().unwrap_or("").trim();
    WorkCardLane {
        label: if label.is_empty() {
            format!("{} {}", copy::AGENT, agent.index)
        } else {
            label.to_string()
        },
        model: work_model_label(agent.model.as_deref()),
        state: agent.state.clone(),
        duration: match agent.duration_ms {
            Some(ms) if settled(&agent.state) => Some(work_duration_long(ms)),
            _ => None,
        },
    }
}

/// The phases the card draws: every DECLARED phase in index order (its
/// agents in index order), then one trailing `Agents` phase for the agents
/// naming none of them.
fn workflow_phases<'a>(
    phases: &[WorkCardPhaseInput],
    agents: &[&'a WorkCardWorkflowAgent],
) -> Vec<Phase<'a>> {
    let mut declared: Vec<&WorkCardPhaseInput> = phases.iter().collect();
    declared.sort_by_key(|phase| phase.index);
    let mut phases: Vec<Phase> = declared
        .iter()
        .map(|phase| Phase {
            title: if phase.title.trim().is_empty() {
                format!("{} {}", copy::PHASE, phase.index)
            } else {
                phase.title.trim().to_string()
            },
            agents: agents
                .iter()
                .copied()
                .filter(|agent| agent.phase_index == Some(phase.index))
                .collect(),
        })
        .collect();
    let loose: Vec<&WorkCardWorkflowAgent> = agents
        .iter()
        .copied()
        .filter(|agent| {
            agent
                .phase_index
                .is_none_or(|index| !declared.iter().any(|phase| phase.index == index))
        })
        .collect();
    if !loose.is_empty() {
        phases.push(Phase {
            title: copy::LOOSE_PHASE.to_string(),
            agents: loose,
        });
    }
    phases
}

fn sorted_agents(agents: &[WorkCardWorkflowAgent]) -> Vec<&WorkCardWorkflowAgent> {
    let mut agents: Vec<&WorkCardWorkflowAgent> = agents.iter().collect();
    agents.sort_by_key(|agent| agent.index);
    agents
}

fn workflow_card(
    name: &str,
    status: &str,
    phases: &[WorkCardPhaseInput],
    agents: &[WorkCardWorkflowAgent],
    elapsed_ms: Option<u64>,
) -> WorkCard {
    let agents = sorted_agents(agents);
    let phases = workflow_phases(phases, &agents);
    let states: Vec<WorkState> = phases.iter().map(phase_state).collect();
    let segments = phases
        .iter()
        .zip(&states)
        .map(|(phase, &state)| {
            let done = phase.agents.iter().filter(|agent| settled(&agent.state)).count();
            WorkCardSegment {
                state,
                fill: if phase.agents.is_empty() {
                    0.0
                } else {
                    done as f64 / phase.agents.len() as f64
                },
            }
        })
        .collect();

    let mut rows = Vec::new();
    let cursor = states.iter().take_while(|state| **state == WorkState::Done).count();
    if cursor > 0 {
        let folded = &phases[..cursor];
        let spans: Vec<u64> = folded.iter().filter_map(phase_span_ms).collect();
        rows.push(WorkCardRow::Folded {
            label: folded
                .iter()
                .map(|phase| phase.title.as_str())
                .collect::<Vec<_>>()
                .join(WORK_CARD_SEPARATOR),
            lanes: plural(
                folded.iter().map(|phase| phase.agents.len()).sum(),
                copy::LANE,
                copy::LANES,
            ),
            duration: (!spans.is_empty()).then(|| work_duration_short(spans.iter().sum())),
        });
    }
    for (phase, &state) in phases.iter().zip(&states).skip(cursor) {
        let span = phase_span_ms(phase);
        rows.push(match state {
            WorkState::Pending => WorkCardRow::Phase {
                title: phase.title.clone(),
                state,
                detail: None,
                duration: None,
                lanes: Vec::new(),
            },
            WorkState::Done => WorkCardRow::Phase {
                title: phase.title.clone(),
                state,
                detail: Some(plural(phase.agents.len(), copy::LANE, copy::LANES)),
                duration: span.map(work_duration_short),
                lanes: Vec::new(),
            },
            WorkState::Running | WorkState::Failed => WorkCardRow::Phase {
                title: phase.title.clone(),
                state,
                detail: Some(phase_detail(phase)),
                duration: if state == WorkState::Failed {
                    span.map(work_duration_short)
                } else {
                    None
                },
                lanes: phase.agents.iter().map(|agent| lane(agent)).collect(),
            },
        });
    }

    let done = agents.iter().filter(|agent| settled(&agent.state)).count();
    WorkCard {
        kind: WorkCardKind::Workflow,
        icon: icons::WORKFLOW.to_string(),
        name: name.trim().to_string(),
        kind_word: Some(copy::WORKFLOW.to_string()),
        progress: (!agents.is_empty()).then(|| format!("{done} / {}", agents.len())),
        elapsed: elapsed_ms.map(work_duration_short),
        running: status == "running",
        segments,
        rows,
    }
}

fn task_state(task: &WorkCardTask) -> WorkState {
    match task.status.as_str() {
        "completed" => WorkState::Done,
        "in_progress" => WorkState::Running,
        _ => WorkState::Pending,
    }
}

fn tasks_card(tasks: &[WorkCardTask]) -> WorkCard {
    let active: Vec<&WorkCardTask> = tasks.iter().filter(|task| task.status == "in_progress").collect();
    let shown: Vec<&WorkCardTask> = if active.is_empty() {
        tasks
            .iter()
            .filter(|task| task_state(task) == WorkState::Pending)
            .take(1)
            .collect()
    } else {
        active.clone()
    };
    let completed = tasks.iter().filter(|task| task.status == "completed").count();
    WorkCard {
        kind: WorkCardKind::Tasks,
        icon: icons::TASKS.to_string(),
        name: copy::TASKS.to_string(),
        kind_word: None,
        progress: (!tasks.is_empty()).then(|| format!("{completed} / {}", tasks.len())),
        elapsed: None,
        running: !active.is_empty(),
        segments: tasks
            .iter()
            .map(|task| {
                let state = task_state(task);
                WorkCardSegment {
                    state,
                    fill: match state {
                        WorkState::Done => 1.0,
                        WorkState::Running => 0.5,
                        _ => 0.0,
                    },
                }
            })
            .collect(),
        rows: shown
            .iter()
            .map(|task| WorkCardRow::Task {
                label: task.content.trim().to_string(),
                state: task_state(task),
            })
            .collect(),
    }
}

fn subagent_card(
    title: Option<&str>,
    state: &str,
    tool_calls: Option<u64>,
    elapsed_ms: Option<u64>,
) -> WorkCard {
    let calls = tool_calls.unwrap_or(0) as usize;
    let word = match state {
        "running" => copy::RUNNING,
        "error" => copy::FAILED,
        _ => copy::DONE,
    };
    let title = title.unwrap_or("").trim();
    WorkCard {
        kind: WorkCardKind::Subagent,
        icon: icons::SUBAGENT.to_string(),
        name: if title.is_empty() { copy::SUBAGENT.to_string() } else { title.to_string() },
        kind_word: Some(copy::SUBAGENT.to_string()),
        progress: Some(plural(calls, copy::CALL, copy::CALLS)),
        elapsed: elapsed_ms.map(work_duration_short),
        running: state == "running",
        segments: Vec::new(),
        rows: vec![WorkCardRow::Status {
            label: format!("{}{WORK_CARD_SEPARATOR}{word}", copy::SUBAGENT),
        }],
    }
}

/// The lanes behind every row of [`work_card`]'s output, ROW-ALIGNED with
/// its `rows`, each with its agent's `index`: the folded row = every lane of
/// the phases it folds (what its chevron opens in place), a phase row = its
/// agents' lanes (the ones a running or failed phase lists), any other row
/// none. A client maps the index to the lane's transcript.
pub fn work_card_lanes(input: &WorkCardInput) -> Vec<Vec<(u32, WorkCardLane)>> {
    let WorkCardInput::Workflow { phases, agents, .. } = input else {
        return work_card(input).rows.iter().map(|_| Vec::new()).collect();
    };
    let agents = sorted_agents(agents.as_deref().unwrap_or(&[]));
    let phases = workflow_phases(phases.as_deref().unwrap_or(&[]), &agents);
    let lanes = |phase: &Phase| -> Vec<(u32, WorkCardLane)> {
        phase.agents.iter().map(|agent| (agent.index, lane(agent))).collect()
    };
    let cursor = phases.iter().take_while(|phase| phase_state(phase) == WorkState::Done).count();
    let mut rows = Vec::new();
    if cursor > 0 {
        rows.push(phases[..cursor].iter().flat_map(lanes).collect());
    }
    rows.extend(phases[cursor..].iter().map(lanes));
    rows
}

/// The TS `workCard(input)`.
pub fn work_card(input: &WorkCardInput) -> WorkCard {
    match input {
        WorkCardInput::Workflow { name, status, phases, agents, elapsed_ms } => workflow_card(
            name,
            status,
            phases.as_deref().unwrap_or(&[]),
            agents.as_deref().unwrap_or(&[]),
            *elapsed_ms,
        ),
        WorkCardInput::Tasks { tasks } => tasks_card(tasks),
        WorkCardInput::Subagent { title, state, tool_calls, elapsed_ms } => {
            subagent_card(title.as_deref(), state, *tool_calls, *elapsed_ms)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract fixture, replayed ×4 (web `work-card.test.ts`, iOS, Android).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/work-card.json");

    #[derive(Deserialize)]
    struct Fixture {
        copy: serde_json::Value,
        icons: serde_json::Value,
        geometry: serde_json::Value,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: WorkCardInput,
        expected: WorkCard,
    }

    #[test]
    fn every_fixture_case_matches_the_whole_card() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        assert!(fixture.cases.len() >= 12);
        for case in fixture.cases {
            let mut actual = work_card(&case.input);
            assert_eq!(actual.segments.len(), case.expected.segments.len(), "{}", case.name);
            for (got, want) in actual.segments.iter_mut().zip(&case.expected.segments) {
                assert!((got.fill - want.fill).abs() < 1e-9, "{}: fill", case.name);
                got.fill = want.fill;
            }
            assert_eq!(actual, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        let strings = &fixture.copy;
        for (key, value) in [
            ("workflow", copy::WORKFLOW),
            ("tasks", copy::TASKS),
            ("subagent", copy::SUBAGENT),
            ("phase", copy::PHASE),
            ("loosePhase", copy::LOOSE_PHASE),
            ("agent", copy::AGENT),
            ("lane", copy::LANE),
            ("lanes", copy::LANES),
            ("call", copy::CALL),
            ("calls", copy::CALLS),
            ("running", copy::RUNNING),
            ("done", copy::DONE),
            ("failed", copy::FAILED),
        ] {
            assert_eq!(strings[key], value, "copy.{key}");
        }
        assert_eq!(fixture.icons["workflow"], icons::WORKFLOW);
        assert_eq!(fixture.icons["tasks"], icons::TASKS);
        assert_eq!(fixture.icons["subagent"], icons::SUBAGENT);
        let geometry = &fixture.geometry;
        for (key, value) in [
            ("header", geometry::HEADER),
            ("row", geometry::ROW),
            ("lane", geometry::LANE),
            ("laneIndent", geometry::LANE_INDENT),
            ("segmentHeight", geometry::SEGMENT_HEIGHT),
            ("segmentGap", geometry::SEGMENT_GAP),
        ] {
            assert_eq!(geometry[key].as_f64(), Some(value as f64), "geometry.{key}");
        }
    }

    /// The lanes behind the rows line up with the rows, and a phase row's
    /// lanes are exactly the ones it lists.
    #[test]
    fn the_lanes_are_row_aligned() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("the fixture parses");
        for case in &fixture.cases {
            let lanes = work_card_lanes(&case.input);
            assert_eq!(lanes.len(), case.expected.rows.len(), "{}", case.name);
            for (row, behind) in case.expected.rows.iter().zip(&lanes) {
                match row {
                    WorkCardRow::Phase { lanes, .. } if !lanes.is_empty() => assert_eq!(
                        &behind.iter().map(|(_, lane)| lane.clone()).collect::<Vec<_>>(),
                        lanes,
                        "{}",
                        case.name
                    ),
                    WorkCardRow::Folded { lanes, .. } => {
                        assert_eq!(&plural(behind.len(), copy::LANE, copy::LANES), lanes)
                    }
                    _ => {}
                }
            }
        }
        // The fixture's first case folds five lanes, agents 1..=5.
        let lanes = work_card_lanes(&fixture.cases[0].input);
        assert_eq!(lanes[0].iter().map(|(index, _)| *index).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
        assert_eq!(lanes[1].iter().map(|(index, _)| *index).collect::<Vec<_>>(), vec![6, 7, 8, 9]);
    }

    #[test]
    fn model_labels_fold_claude_ids() {
        assert_eq!(work_model_label(Some("claude-opus-5-5")).as_deref(), Some("opus"));
        assert_eq!(work_model_label(Some(" sonnet ")).as_deref(), Some("sonnet"));
        assert_eq!(work_model_label(Some("  ")), None);
        assert_eq!(work_model_label(None), None);
    }
}
