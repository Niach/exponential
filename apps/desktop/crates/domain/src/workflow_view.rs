//! EXP-981 — what every client SAYS about a workflow.
//!
//! The graph's geometry is the server's (`wave`/`lane` on the synced nodes);
//! this is the rest — bands, captions, the edges between nodes — mirrored ×4
//! (web `apps/web/src/lib/workflow-view.ts`, iOS `WorkflowView.swift`, Android
//! `WorkflowView.kt`) and locked by the contract fixture
//! `packages/domain-contract/fixtures/workflow-view.json`. All strings
//! byte-identical.

use std::collections::{HashMap, HashSet};

/// The three list bands a workflow's status lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowBand {
    Running,
    Draft,
    Done,
}

impl WorkflowBand {
    /// The fixture's wire word for this band.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowBand::Running => "running",
            WorkflowBand::Draft => "draft",
            WorkflowBand::Done => "done",
        }
    }

    /// The band's heading.
    pub fn title(self) -> &'static str {
        match self {
            WorkflowBand::Running => "Running",
            WorkflowBand::Draft => "Draft",
            WorkflowBand::Done => "Done",
        }
    }
}

/// The list's three bands, in order. Flat rows under each, no row buttons.
pub const WORKFLOW_BANDS: [WorkflowBand; 3] = [
    WorkflowBand::Running,
    WorkflowBand::Draft,
    WorkflowBand::Done,
];

pub const WORKFLOWS_TITLE: &str = "Workflows";
pub const WORKFLOWS_EMPTY_TITLE: &str = "No workflows yet";
pub const WORKFLOWS_EMPTY_BODY: &str =
    "Select backlog issues on a board and choose Create workflow to plan them as one parallel run.";
pub const PLAN_WORKFLOW_LABEL: &str = "Plan";
pub const DELETE_WORKFLOW_LABEL: &str = "Delete workflow";
/// The bulk bar's play menu (EXP-981), in order.
pub const START_AS_BATCH_LABEL: &str = "Start as batch";
pub const START_AS_STACK_LABEL: &str = "Start as stack";
pub const CREATE_WORKFLOW_LABEL: &str = "Create workflow…";

/// `paused` is a running workflow someone held; `cancelled` is over. An
/// unknown status (a newer server) lands in Done rather than vanishing.
pub fn workflow_band(status: &str) -> WorkflowBand {
    match status {
        "running" | "paused" => WorkflowBand::Running,
        "draft" => WorkflowBand::Draft,
        _ => WorkflowBand::Done,
    }
}

/// The counters `workflows.metrics` carries that the list and header read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowShape {
    pub nodes: usize,
    pub depth: usize,
    pub width: usize,
    /// One entry per blocking cycle: its issue identifiers.
    pub cycles: Vec<Vec<String>>,
}

/// `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`.
pub fn workflow_shape_line(metrics: &WorkflowShape) -> String {
    let nodes = if metrics.nodes == 1 {
        "1 node".to_string()
    } else {
        format!("{} nodes", metrics.nodes)
    };
    format!("{nodes} · depth {} · width {}", metrics.depth, metrics.width)
}

/// `None` while the workflow could start; else the cycles spelled out.
pub fn workflow_cycle_note(metrics: &WorkflowShape) -> Option<String> {
    if metrics.cycles.is_empty() {
        return None;
    }
    let spelled = metrics
        .cycles
        .iter()
        .map(|keys| keys.join(", "))
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "These issues block each other in a cycle: {spelled}. Remove one relation to start."
    ))
}

const STATE_LABELS: [(&str, &str); 11] = [
    ("proposed", "Proposed"),
    ("blocked", "Blocked"),
    ("ready", "Ready"),
    ("running", "Running"),
    ("waiting", "Waiting"),
    ("in_review", "In review"),
    ("updating", "Updating"),
    ("landed", "Landed"),
    ("failed", "Failed"),
    ("skipped", "Skipped"),
    ("paused", "Paused"),
];

const KIND_LABELS: [(&str, &str); 3] = [
    ("contract", "Contract"),
    ("leaf", "Leaf"),
    ("integration", "Integration"),
];

/// An unknown state (a newer server) renders its raw wire word.
pub fn workflow_node_state_label(state: &str) -> String {
    STATE_LABELS
        .iter()
        .find(|(wire, _)| *wire == state)
        .map_or_else(|| state.to_string(), |(_, label)| (*label).to_string())
}

pub fn workflow_node_kind_label(kind: &str) -> String {
    KIND_LABELS
        .iter()
        .find(|(wire, _)| *wire == kind)
        .map_or_else(|| kind.to_string(), |(_, label)| (*label).to_string())
}

/// The tone a node's state paints in. `waiting` is the ONLY amber one (and
/// the only one that pushes): amber means "a person is needed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowNodeTone {
    Muted,
    Active,
    Amber,
    Success,
    Danger,
}

impl WorkflowNodeTone {
    /// The fixture's wire word for this tone.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowNodeTone::Muted => "muted",
            WorkflowNodeTone::Active => "active",
            WorkflowNodeTone::Amber => "amber",
            WorkflowNodeTone::Success => "success",
            WorkflowNodeTone::Danger => "danger",
        }
    }
}

pub fn workflow_node_tone(state: &str) -> WorkflowNodeTone {
    match state {
        "waiting" => WorkflowNodeTone::Amber,
        "failed" => WorkflowNodeTone::Danger,
        "landed" => WorkflowNodeTone::Success,
        "running" | "updating" | "in_review" => WorkflowNodeTone::Active,
        _ => WorkflowNodeTone::Muted,
    }
}

/// What a caption needs off the node row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptionNode<'a> {
    pub kind: &'a str,
    pub state: &'a str,
    pub risk: &'a str,
}

/// The ONE caption under a node. A draft has no states worth reading yet, so
/// it names the plan (`Contract`, `Leaf · high risk`); a started workflow
/// names the state, prefixed by the kind only for the two special nodes
/// (`Contract · Running`, `In review`).
pub fn workflow_node_caption(node: CaptionNode<'_>, workflow_status: &str) -> String {
    if workflow_status == "draft" {
        let kind = workflow_node_kind_label(node.kind);
        return if node.risk == "high" {
            format!("{kind} · high risk")
        } else {
            kind
        };
    }
    let state = workflow_node_state_label(node.state);
    if node.kind == "leaf" {
        state
    } else {
        format!("{} · {state}", workflow_node_kind_label(node.kind))
    }
}

/// `EXP-14 +3` for a compound node (a parent run as one batch with its
/// sub-issues), the bare identifier otherwise.
pub fn workflow_node_title(identifier: &str, member_count: usize) -> String {
    if member_count > 0 {
        format!("{identifier} +{member_count}")
    } else {
        identifier.to_string()
    }
}

/// What an edge needs off a node row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeNode<'a> {
    pub id: &'a str,
    pub issue_id: &'a str,
    pub member_issue_ids: Vec<&'a str>,
}

/// What an edge needs off an `issue_relations` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeRelation<'a> {
    pub kind: &'a str,
    pub issue_id: &'a str,
    pub related_issue_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    /// Inside a blocking cycle (`metrics.cycleEdges`): drawn red.
    pub cycle: bool,
}

/// The edges between a workflow's nodes, from the synced `blocks` relations: a
/// relation between ANY two covered issues of two different nodes (the
/// server's `nodeEdges`). One edge per node pair, ordered by (from, to) node
/// id. `cycle_edges` = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
pub fn workflow_edges(
    nodes: &[EdgeNode<'_>],
    relations: &[EdgeRelation<'_>],
    cycle_edges: &[String],
) -> Vec<WorkflowEdge> {
    let mut node_of: HashMap<&str, &str> = HashMap::new();
    for node in nodes {
        node_of.insert(node.issue_id, node.id);
        for member in &node.member_issue_ids {
            node_of.insert(member, node.id);
        }
    }
    let on_cycle: HashSet<&str> = cycle_edges.iter().map(String::as_str).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut edges: Vec<WorkflowEdge> = Vec::new();
    for relation in relations {
        if relation.kind != "blocks" {
            continue;
        }
        let (Some(from), Some(to)) = (
            node_of.get(relation.issue_id),
            node_of.get(relation.related_issue_id),
        ) else {
            continue;
        };
        if from == to {
            continue;
        }
        let key = format!("{from}\n{to}");
        if !seen.insert(key.clone()) {
            continue;
        }
        edges.push(WorkflowEdge {
            from: (*from).to_string(),
            to: (*to).to_string(),
            cycle: on_cycle.contains(key.as_str()),
        });
    }
    edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    edges
}

// ── Running a workflow (EXP-982) ────────────────────────────────────────────

pub const START_WORKFLOW_LABEL: &str = "Start";
pub const PAUSE_WORKFLOW_LABEL: &str = "Pause";
pub const RESUME_WORKFLOW_LABEL: &str = "Resume";
pub const CANCEL_WORKFLOW_LABEL: &str = "Cancel workflow";
pub const CANCEL_WORKFLOW_CONFIRM: &str =
    "Its live runs end and its branch is deleted. Nothing reached the default branch.";
pub const APPROVE_NODE_LABEL: &str = "Approve and land";
pub const WITHDRAW_APPROVAL_LABEL: &str = "Withdraw approval";
pub const MERGE_TRAIN_TITLE: &str = "Merge train";
pub const MERGE_TRAIN_EMPTY: &str = "Nothing is waiting to land.";
pub const FINAL_PR_TITLE: &str = "Final pull request";
pub const RETRY_NODE_LABEL: &str = "Retry";
pub const SKIP_NODE_LABEL: &str = "Skip";
pub const SKIP_NODE_CONFIRM: &str =
    "Its dependents go on without it. The node's work is not part of the final pull request.";
/// The node panel's run + PR affordances.
pub const OPEN_RUN_LABEL: &str = "Open run";

/// What the Start blocker rule reads off the workflow row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartableWorkflow<'a> {
    pub status: &'a str,
    pub device_id: Option<&'a str>,
    pub repository_id: Option<&'a str>,
    pub start_on: &'a str,
}

/// Why Start is disabled, or `None` when the draft can start. One reason, the
/// most fundamental first; the server refuses with the same sentences.
pub fn workflow_start_blocker(
    workflow: StartableWorkflow<'_>,
    metrics: &WorkflowShape,
) -> Option<String> {
    if workflow.status != "draft" {
        return Some("The workflow has already started.".to_string());
    }
    if metrics.nodes == 0 {
        return Some("The workflow has no issues.".to_string());
    }
    if let Some(cycle) = workflow_cycle_note(metrics) {
        return Some(cycle);
    }
    if workflow.repository_id.is_none() {
        return Some("The workflow's repository is gone.".to_string());
    }
    if workflow.device_id.is_none() {
        return Some("Pick the device that runs this workflow first.".to_string());
    }
    if workflow.start_on != "landed" {
        return Some("Only \"When landed\" starts are available yet.".to_string());
    }
    None
}

/// A node lands without a person only when the workflow has no gate AND it is
/// not the contract (always human-gated). Mirrors the server.
pub fn workflow_node_needs_approval(gate: &str, kind: &str) -> bool {
    kind == "contract" || gate != "none"
}

/// What the merge train reads off a node row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainNode<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub state: &'a str,
    pub wave: i64,
    pub lane: i64,
    /// The `approved_at` stamp; only its presence matters.
    pub approved: bool,
}

/// Where one node stands in the merge train.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainStep {
    Next,
    Queued,
    NeedsApproval,
    Updating,
}

impl TrainStep {
    /// The fixture's wire word for this step.
    pub fn as_wire(self) -> &'static str {
        match self {
            TrainStep::Next => "next",
            TrainStep::Queued => "queued",
            TrainStep::NeedsApproval => "needs-approval",
            TrainStep::Updating => "updating",
        }
    }
}

pub fn workflow_train_step_label(step: TrainStep) -> &'static str {
    match step {
        TrainStep::Next => "Landing next",
        TrainStep::Queued => "Queued",
        TrainStep::NeedsApproval => "Needs approval",
        TrainStep::Updating => "Merging the trunk in",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainEntry {
    pub id: String,
    pub step: TrainStep,
}

/// The merge train: every node whose PR is up (`in_review`, or `updating`
/// while it merges the trunk in), in landing order (wave, then lane). The
/// FIRST node that is cleared to land is `Next`; cleared ones behind it are
/// `Queued`; one still waiting for a person says so.
pub fn workflow_merge_train(nodes: &[TrainNode<'_>], gate: &str) -> Vec<TrainEntry> {
    let mut waiting: Vec<&TrainNode<'_>> = nodes
        .iter()
        .filter(|node| node.state == "in_review" || node.state == "updating")
        .collect();
    waiting.sort_by(|a, b| {
        a.wave
            .cmp(&b.wave)
            .then_with(|| a.lane.cmp(&b.lane))
            .then_with(|| a.id.cmp(b.id))
    });
    let mut next_taken = false;
    waiting
        .into_iter()
        .map(|node| {
            let step = if node.state == "updating" {
                TrainStep::Updating
            } else if workflow_node_needs_approval(gate, node.kind) && !node.approved {
                TrainStep::NeedsApproval
            } else if next_taken {
                TrainStep::Queued
            } else {
                next_taken = true;
                TrainStep::Next
            };
            TrainEntry {
                id: node.id.to_string(),
                step,
            }
        })
        .collect()
}

/// The final-PR node's caption, or `None` while the node is not drawn: it
/// appears once every node landed (or was skipped), after the last wave.
pub fn workflow_final_pr_caption(
    node_states: &[&str],
    final_pr_state: Option<&str>,
    final_pr_number: Option<i64>,
) -> Option<String> {
    if node_states.is_empty() {
        return None;
    }
    let all_in = node_states
        .iter()
        .all(|state| *state == "landed" || *state == "skipped");
    if !all_in && final_pr_number.is_none() {
        return None;
    }
    let Some(number) = final_pr_number else {
        return Some("Opening the pull request".to_string());
    };
    let label = match final_pr_state {
        Some("merged") => "Merged",
        Some("closed") => "Closed",
        _ => "Open",
    };
    Some(format!("#{number} · {label}"))
}

/// A list row's secondary text: the shape line, led by the status word for
/// the two statuses a band alone does not tell apart.
pub fn workflow_row_subtitle(status: &str, metrics: &WorkflowShape) -> String {
    let shape = workflow_shape_line(metrics);
    match status {
        "paused" => format!("Paused · {shape}"),
        "cancelled" => format!("Cancelled · {shape}"),
        _ => shape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `workflow-view.test.ts`, iOS `WorkflowViewTests`, Android
    /// `WorkflowViewTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/workflow-view.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureMetrics {
        nodes: usize,
        depth: usize,
        width: usize,
        cycles: Vec<Vec<String>>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureBand {
        status: String,
        band: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureShapeLine {
        metrics: FixtureMetrics,
        line: String,
        cycle_note: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCaptionNode {
        kind: String,
        state: String,
        risk: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCaption {
        node: FixtureCaptionNode,
        workflow_status: String,
        caption: String,
        tone: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureTitle {
        identifier: String,
        members: usize,
        title: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEdgeNode {
        id: String,
        issue_id: String,
        member_issue_ids: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEdgeRelation {
        #[serde(rename = "type")]
        kind: String,
        issue_id: String,
        related_issue_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureExpectedEdge {
        from: String,
        to: String,
        cycle: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEdgeCase {
        name: String,
        nodes: Vec<FixtureEdgeNode>,
        relations: Vec<FixtureEdgeRelation>,
        #[serde(default)]
        cycle_edges: Vec<String>,
        expected: Vec<FixtureExpectedEdge>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureStartWorkflow {
        status: String,
        device_id: Option<String>,
        repository_id: Option<String>,
        start_on: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureStartBlocker {
        workflow: FixtureStartWorkflow,
        metrics: FixtureMetrics,
        blocker: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureTrainNode {
        id: String,
        kind: String,
        state: String,
        wave: i64,
        lane: i64,
        approved_at: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureTrainEntry {
        id: String,
        step: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureTrainCase {
        name: String,
        gate: String,
        nodes: Vec<FixtureTrainNode>,
        expected: Vec<FixtureTrainEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureFinalPr {
        states: Vec<String>,
        final_pr_state: Option<String>,
        final_pr_number: Option<i64>,
        caption: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRowSubtitle {
        status: String,
        metrics: FixtureMetrics,
        subtitle: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        bands: Vec<FixtureBand>,
        shape_lines: Vec<FixtureShapeLine>,
        captions: Vec<FixtureCaption>,
        titles: Vec<FixtureTitle>,
        edges: Vec<FixtureEdgeCase>,
        start_blockers: Vec<FixtureStartBlocker>,
        trains: Vec<FixtureTrainCase>,
        train_step_labels: HashMap<String, String>,
        final_pr: Vec<FixtureFinalPr>,
        row_subtitles: Vec<FixtureRowSubtitle>,
    }

    impl FixtureMetrics {
        fn shape(&self) -> WorkflowShape {
            WorkflowShape {
                nodes: self.nodes,
                depth: self.depth,
                width: self.width,
                cycles: self.cycles.clone(),
            }
        }
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the workflow-view fixture parses")
    }

    #[test]
    fn workflow_view_contract_fixture() {
        let fixture = fixture();

        for case in &fixture.bands {
            assert_eq!(
                workflow_band(&case.status).as_wire(),
                case.band,
                "status: {}",
                case.status
            );
        }

        for case in &fixture.shape_lines {
            let metrics = case.metrics.shape();
            assert_eq!(workflow_shape_line(&metrics), case.line);
            assert_eq!(workflow_cycle_note(&metrics), case.cycle_note);
        }

        for case in &fixture.captions {
            let node = CaptionNode {
                kind: &case.node.kind,
                state: &case.node.state,
                risk: &case.node.risk,
            };
            assert_eq!(
                workflow_node_caption(node, &case.workflow_status),
                case.caption
            );
            assert_eq!(workflow_node_tone(&case.node.state).as_wire(), case.tone);
        }

        for case in &fixture.titles {
            assert_eq!(
                workflow_node_title(&case.identifier, case.members),
                case.title
            );
        }

        for case in &fixture.edges {
            let nodes: Vec<EdgeNode<'_>> = case
                .nodes
                .iter()
                .map(|node| EdgeNode {
                    id: &node.id,
                    issue_id: &node.issue_id,
                    member_issue_ids: node.member_issue_ids.iter().map(String::as_str).collect(),
                })
                .collect();
            let relations: Vec<EdgeRelation<'_>> = case
                .relations
                .iter()
                .map(|relation| EdgeRelation {
                    kind: &relation.kind,
                    issue_id: &relation.issue_id,
                    related_issue_id: &relation.related_issue_id,
                })
                .collect();
            let got = workflow_edges(&nodes, &relations, &case.cycle_edges);
            let want: Vec<WorkflowEdge> = case
                .expected
                .iter()
                .map(|edge| WorkflowEdge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    cycle: edge.cycle,
                })
                .collect();
            assert_eq!(got, want, "case: {}", case.name);
        }

        // ── EXP-982 ──────────────────────────────────────────────────────
        for (index, case) in fixture.start_blockers.iter().enumerate() {
            let workflow = StartableWorkflow {
                status: &case.workflow.status,
                device_id: case.workflow.device_id.as_deref(),
                repository_id: case.workflow.repository_id.as_deref(),
                start_on: &case.workflow.start_on,
            };
            assert_eq!(
                workflow_start_blocker(workflow, &case.metrics.shape()),
                case.blocker,
                "startBlockers[{index}]"
            );
        }

        for case in &fixture.trains {
            let nodes: Vec<TrainNode<'_>> = case
                .nodes
                .iter()
                .map(|node| TrainNode {
                    id: &node.id,
                    kind: &node.kind,
                    state: &node.state,
                    wave: node.wave,
                    lane: node.lane,
                    approved: node.approved_at.is_some(),
                })
                .collect();
            let got = workflow_merge_train(&nodes, &case.gate);
            let want: Vec<(&str, &str)> = case
                .expected
                .iter()
                .map(|entry| (entry.id.as_str(), entry.step.as_str()))
                .collect();
            let seen: Vec<(&str, &str)> = got
                .iter()
                .map(|entry| (entry.id.as_str(), entry.step.as_wire()))
                .collect();
            assert_eq!(seen, want, "case: {}", case.name);
        }

        for (step, label) in &fixture.train_step_labels {
            let parsed = [
                TrainStep::Next,
                TrainStep::Queued,
                TrainStep::NeedsApproval,
                TrainStep::Updating,
            ]
            .into_iter()
            .find(|candidate| candidate.as_wire() == step)
            .unwrap_or_else(|| panic!("unknown train step: {step}"));
            assert_eq!(workflow_train_step_label(parsed), label);
        }

        for (index, case) in fixture.final_pr.iter().enumerate() {
            let states: Vec<&str> = case.states.iter().map(String::as_str).collect();
            assert_eq!(
                workflow_final_pr_caption(
                    &states,
                    case.final_pr_state.as_deref(),
                    case.final_pr_number
                ),
                case.caption,
                "finalPr[{index}]"
            );
        }

        for case in &fixture.row_subtitles {
            assert_eq!(
                workflow_row_subtitle(&case.status, &case.metrics.shape()),
                case.subtitle,
                "status: {}",
                case.status
            );
        }
    }

    /// The gate rule the train and the node panel share: the contract always
    /// needs a person, everything else only under a gate.
    #[test]
    fn the_contract_always_needs_a_person() {
        assert!(workflow_node_needs_approval("none", "contract"));
        assert!(!workflow_node_needs_approval("none", "leaf"));
        assert!(!workflow_node_needs_approval("none", "integration"));
        assert!(workflow_node_needs_approval("human", "leaf"));
        assert!(workflow_node_needs_approval("agent", "leaf"));
    }

    /// The train is strict order only among CLEARED nodes: a node waiting for
    /// a person never blocks a cleared one behind it.
    #[test]
    fn an_unapproved_node_never_holds_the_one_behind_it() {
        let nodes = [
            TrainNode {
                id: "c",
                kind: "contract",
                state: "in_review",
                wave: 0,
                lane: 0,
                approved: false,
            },
            TrainNode {
                id: "l",
                kind: "leaf",
                state: "in_review",
                wave: 1,
                lane: 0,
                approved: false,
            },
        ];
        let train = workflow_merge_train(&nodes, "none");
        assert_eq!(
            train
                .iter()
                .map(|entry| (entry.id.as_str(), entry.step))
                .collect::<Vec<_>>(),
            vec![("c", TrainStep::NeedsApproval), ("l", TrainStep::Next)]
        );
    }

    /// The bands render in the fixture's order under their own titles.
    #[test]
    fn the_bands_keep_their_order_and_titles() {
        let titles: Vec<&str> = WORKFLOW_BANDS.iter().map(|band| band.title()).collect();
        assert_eq!(titles, ["Running", "Draft", "Done"]);
        let wires: Vec<&str> = WORKFLOW_BANDS.iter().map(|band| band.as_wire()).collect();
        assert_eq!(wires, ["running", "draft", "done"]);
    }

    /// An unknown state/kind (a newer server) renders its raw wire word
    /// rather than an empty caption.
    #[test]
    fn unknown_states_and_kinds_render_their_wire_word() {
        assert_eq!(workflow_node_state_label("brand-new"), "brand-new");
        assert_eq!(workflow_node_kind_label("brand-new"), "brand-new");
        assert_eq!(workflow_node_tone("brand-new"), WorkflowNodeTone::Muted);
    }
}
