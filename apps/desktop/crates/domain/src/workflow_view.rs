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

const STATE_LABELS: [(&str, &str); 10] = [
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

/// The ONE caption under a node: the bare STATE label once the workflow has
/// started (`Running`, `In review`, `Landed`), nothing at all in a draft. The
/// kind and the risk are the node panel's (EXP-1014: no `Leaf`, no
/// `Contract · high risk` beside the chips — the chip names the issue, the
/// caption says only what is happening to it).
pub fn workflow_node_caption(node: CaptionNode<'_>, workflow_status: &str) -> String {
    if workflow_status == "draft" {
        return String::new();
    }
    workflow_node_state_label(node.state)
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeNode<'a> {
    pub id: &'a str,
    pub issue_id: &'a str,
    pub member_issue_ids: Vec<&'a str>,
    /// EXP-983: engine-written serialization edges (`after_node_ids`).
    pub after_node_ids: Vec<&'a str>,
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
    /// EXP-983: not a `blocks` relation but a SERIALIZATION edge the engine
    /// added after two siblings' work collided: `to` merges `from` in first.
    pub serial: bool,
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
            serial: false,
        });
    }
    // Serialization edges, unless a real edge already joins the pair.
    let known: HashSet<&str> = nodes.iter().map(|node| node.id).collect();
    for node in nodes {
        for from in &node.after_node_ids {
            if !known.contains(from) || *from == node.id {
                continue;
            }
            let key = format!("{from}\n{}", node.id);
            if !seen.insert(key) {
                continue;
            }
            edges.push(WorkflowEdge {
                from: (*from).to_string(),
                to: node.id.to_string(),
                cycle: false,
                serial: true,
            });
        }
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
/// The strip over the graph that lists the runs that are up, one tap away.
pub const RUNNING_NOW_LABEL: &str = "Running now";

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
    // EXP-983: every `start_on` runs now — the mode never blocks a start.
    let _ = workflow.start_on;
    None
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
/// `Queued`; one no review approved yet says so (EXP-1010: the agent review
/// is the only gate, a person may approve by hand).
pub fn workflow_merge_train(nodes: &[TrainNode<'_>]) -> Vec<TrainEntry> {
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
            } else if !node.approved {
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
/// EXP-984: a `proposed` node was never admitted, so it is not part of the
/// run and nothing waits for it.
pub fn workflow_final_pr_caption(
    node_states: &[&str],
    final_pr_state: Option<&str>,
    final_pr_number: Option<i64>,
) -> Option<String> {
    let real: Vec<&str> = node_states
        .iter()
        .copied()
        .filter(|state| *state != "proposed")
        .collect();
    if real.is_empty() {
        return None;
    }
    let all_in = real
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

// ── Speculative starts (EXP-983) ────────────────────────────────────────────

/// How an edge is drawn. Grey solid is the default; the others say something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowEdgeStyle {
    Plain,
    Cycle,
    Stale,
    Landed,
    Speculative,
}

impl WorkflowEdgeStyle {
    /// The fixture's wire word for this style.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowEdgeStyle::Plain => "plain",
            WorkflowEdgeStyle::Cycle => "cycle",
            WorkflowEdgeStyle::Stale => "stale",
            WorkflowEdgeStyle::Landed => "landed",
            WorkflowEdgeStyle::Speculative => "speculative",
        }
    }
}

/// The states a node occupies once its run exists — a dependent in one of
/// them started BEFORE its blocker landed, which is what dashes an edge.
const STARTED_STATES: [&str; 4] = ["running", "waiting", "in_review", "updating"];

/// - `Cycle` (red): inside a blocking cycle.
/// - `Stale` (red): upstream moved and the dependent is merging it in (`to`
///   is `updating`).
/// - `Landed` (green): the blocker landed.
/// - `Speculative` (dashed): the dependent started before its blocker landed,
///   or the edge is a serialization edge.
/// - `Plain` (grey): nothing to say yet.
pub fn workflow_edge_style(
    edge: &WorkflowEdge,
    from_state: &str,
    to_state: &str,
) -> WorkflowEdgeStyle {
    if edge.cycle {
        return WorkflowEdgeStyle::Cycle;
    }
    if from_state == "landed" {
        return WorkflowEdgeStyle::Landed;
    }
    if to_state == "updating" {
        return WorkflowEdgeStyle::Stale;
    }
    if edge.serial || STARTED_STATES.contains(&to_state) {
        return WorkflowEdgeStyle::Speculative;
    }
    WorkflowEdgeStyle::Plain
}

/// The node panel's line once a node announced its contract.
pub const CONTRACT_PUBLISHED_LABEL: &str = "Contract published";
/// The node panel's line over the `after_node_ids` chips.
pub const MERGES_IN_FIRST_LABEL: &str = "Merges in first";

// ── Review gate, dynamic graphs, budgets, metrics (EXP-984) ─────────────────

/// A `proposed` node's two decisions, and the sentence that explains it.
pub const ADMIT_NODE_LABEL: &str = "Admit";
pub const DISMISS_NODE_LABEL: &str = "Dismiss";
pub const PROPOSED_NODE_NOTE: &str =
    "Filed during the run. Admit it into the workflow or dismiss it.";
/// The node panel's agent-review block and the detail's counters section.
/// EXP-1014: a workflow configures NOTHING on its screen any more — the
/// launch is two models (`coding::workflows::launch`), so the per-phase and
/// review-model launch rows (and their labels) are gone.
pub const AGENT_REVIEW_TITLE: &str = "Agent review";
pub const METRICS_TITLE: &str = "Metrics";

/// What the review line reads off `workflow_nodes.review`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewLine<'a> {
    /// contract `wfReviewVerdict` — `approve` / `request_changes`.
    pub verdict: &'a str,
    pub round: i64,
    /// `Some(passed)` when the reviewer RAN a check; `None` = opinion only.
    pub oracle: Option<bool>,
    /// The node's `approved_at` is set: the verdict cleared it (EXP-1010).
    pub approved: bool,
}

/// The node panel's one line about the latest agent review:
/// `Approved · round 1 · checks passed`, `Approved · round 1`,
/// `Changes requested · round 2 · checks failed`, `Changes requested · round 3`.
/// EXP-1010: an approval with no oracle CLEARS the node, so `advisory` shows
/// only when it did not (`approved` false).
pub fn workflow_review_line(review: ReviewLine<'_>) -> String {
    let verdict = if review.verdict == "approve" {
        "Approved"
    } else {
        "Changes requested"
    };
    let mut parts = vec![verdict.to_string(), format!("round {}", review.round)];
    match review.oracle {
        Some(true) => parts.push("checks passed".to_string()),
        Some(false) => parts.push("checks failed".to_string()),
        // An approval that did not clear the node is advisory; a request
        // for changes needs no such word.
        None if review.verdict == "approve" && !review.approved => {
            parts.push("advisory".to_string())
        }
        None => {}
    }
    parts.join(" · ")
}

/// One label/value row of the detail's Metrics section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricRow {
    pub label: String,
    pub value: String,
}

/// One counter off `workflows.metrics`. Anything that is not a number (an
/// older server, a garbled blob) reads as zero rather than dropping the row.
fn metric_count(metrics: &serde_json::Value, key: &str) -> f64 {
    metrics
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

/// A counter as text, the way the other clients' `${n}` renders it: whole
/// numbers carry no decimal point.
fn metric_text(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// The detail's Metrics section for a STARTED workflow, in this order. A row
/// appears only when it has something to say, except the critical path, which
/// always does.
pub fn workflow_metric_rows(metrics: &serde_json::Value) -> Vec<MetricRow> {
    let row = |label: &str, value: String| MetricRow {
        label: label.to_string(),
        value,
    };
    let count = |key: &str| metric_count(metrics, key);
    let mut rows = vec![row(
        "Critical path",
        format!(
            "{} waves for {} nodes",
            metric_text(count("depth")),
            metric_text(count("nodes"))
        ),
    )];
    let landed = count("landed");
    if landed > 0.0 {
        rows.push(row("Landed", metric_text(landed)));
    }
    let merge_ins = count("mergeIns");
    let changes = count("contractChanges");
    if merge_ins > 0.0 {
        rows.push(if changes > 0.0 {
            row(
                "Merge-ins per contract change",
                format!("{:.1}", merge_ins / changes),
            )
        } else {
            row("Merge-ins", metric_text(merge_ins))
        });
    }
    let escalations = count("escalations");
    if escalations > 0.0 {
        rows.push(row(
            "Escalations",
            format!(
                "{} ({} duplicate)",
                metric_text(escalations),
                metric_text(count("duplicateEscalations"))
            ),
        ));
    }
    let minutes = count("operatorMinutes");
    if minutes > 0.0 {
        rows.push(row("Operator minutes", metric_text(minutes)));
    }
    let rounds = count("reviewRounds");
    if rounds > 0.0 {
        rows.push(row("Review rounds", metric_text(rounds)));
    }
    let by_oracle = count("defectsByOracle");
    let by_agent = count("defectsByAgentReview");
    if by_oracle + by_agent > 0.0 {
        rows.push(row(
            "Defects found",
            format!(
                "{} by checks · {} by agent review",
                metric_text(by_oracle),
                metric_text(by_agent)
            ),
        ));
    }
    rows
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
        #[serde(default)]
        after_node_ids: Vec<String>,
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
        #[serde(default)]
        serial: bool,
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
    struct FixtureStyledEdge {
        cycle: bool,
        serial: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEdgeStyle {
        edge: FixtureStyledEdge,
        from_state: String,
        to_state: String,
        style: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureOracle {
        passed: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureReview {
        verdict: String,
        round: i64,
        oracle: Option<FixtureOracle>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureReviewLine {
        review: FixtureReview,
        approved: bool,
        line: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureMetricRow {
        label: String,
        value: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureMetricRows {
        /// The raw `workflows.metrics` blob — counters and garbage alike.
        metrics: serde_json::Value,
        rows: Vec<FixtureMetricRow>,
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
        /// EXP-983.
        edge_styles: Vec<FixtureEdgeStyle>,
        /// EXP-984.
        review_lines: Vec<FixtureReviewLine>,
        metric_rows: Vec<FixtureMetricRows>,
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
                    after_node_ids: node.after_node_ids.iter().map(String::as_str).collect(),
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
                    serial: edge.serial,
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
            let got = workflow_merge_train(&nodes);
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

        // ── EXP-983 ──────────────────────────────────────────────────────
        for (index, case) in fixture.edge_styles.iter().enumerate() {
            let edge = WorkflowEdge {
                from: "a".to_string(),
                to: "b".to_string(),
                cycle: case.edge.cycle,
                serial: case.edge.serial,
            };
            assert_eq!(
                workflow_edge_style(&edge, &case.from_state, &case.to_state).as_wire(),
                case.style,
                "edgeStyles[{index}]"
            );
        }

        // ── EXP-984 ──────────────────────────────────────────────────────
        for (index, case) in fixture.review_lines.iter().enumerate() {
            assert_eq!(
                workflow_review_line(ReviewLine {
                    verdict: &case.review.verdict,
                    round: case.review.round,
                    oracle: case.review.oracle.as_ref().map(|oracle| oracle.passed),
                    approved: case.approved,
                }),
                case.line,
                "reviewLines[{index}]"
            );
        }

        for (index, case) in fixture.metric_rows.iter().enumerate() {
            let want: Vec<MetricRow> = case
                .rows
                .iter()
                .map(|row| MetricRow {
                    label: row.label.clone(),
                    value: row.value.clone(),
                })
                .collect();
            assert_eq!(
                workflow_metric_rows(&case.metrics),
                want,
                "metricRows[{index}]"
            );
        }
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
                approved: true,
            },
        ];
        let train = workflow_merge_train(&nodes);
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

    /// EXP-1014 — the caption is the STATE and nothing else: a draft node
    /// carries none at all, and neither kind nor risk ever prefixes one.
    #[test]
    fn a_caption_never_names_the_kind_or_the_risk() {
        let node = |kind: &'static str, state: &'static str, risk: &'static str| CaptionNode {
            kind,
            state,
            risk,
        };
        assert_eq!(workflow_node_caption(node("contract", "blocked", "high"), "draft"), "");
        assert_eq!(workflow_node_caption(node("leaf", "ready", "low"), "draft"), "");
        assert_eq!(
            workflow_node_caption(node("contract", "running", "high"), "running"),
            "Running"
        );
        assert_eq!(
            workflow_node_caption(node("integration", "in_review", "medium"), "paused"),
            "In review"
        );
    }
}
