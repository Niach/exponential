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
pub const FINAL_PR_TITLE: &str = "Final pull request";
/// EXP-1032 — merging the final pull request is the run's ONE human review,
/// so the action sits on the chip that IS that pull request, and confirms
/// with this sentence. Byte-identical ×4.
pub const MERGE_FINAL_PR_LABEL: &str = "Merge";
pub const MERGE_FINAL_PR_CONFIRM: &str =
    "The workflow's branch is squash-merged into the default branch and the run is done.";
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
    None
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

// ── Review gate, dynamic graphs (EXP-984) ─────────────────

/// A `proposed` node's two decisions, and the sentence that explains it.
pub const ADMIT_NODE_LABEL: &str = "Admit";
pub const DISMISS_NODE_LABEL: &str = "Dismiss";
pub const PROPOSED_NODE_NOTE: &str =
    "Filed during the run. Admit it into the workflow or dismiss it.";
/// The node panel's read-only line: what THIS node's run spawns on
/// (`model_for_node`).
pub const NODE_MODEL_LABEL: &str = "Model";
/// EXP-1014: the chip of a node whose issue row has not synced yet — the
/// identifier slot shows the first 8 characters of the issue id, the title
/// this line. Byte-identical ×4.
pub const NODE_UNSYNCED_TITLE: &str = "Not synced yet";

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

// ---------------------------------------------------------------------------
// EXP-1082 — the node DISPLAY vocabulary (contract `wfNodeDisplayState`)
// ---------------------------------------------------------------------------

/// The five words a node chip shows, whatever its internal `wfNodeState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowNodeDisplayState {
    Queued,
    Running,
    Done,
    Failed,
    Skipped,
}

impl WorkflowNodeDisplayState {
    /// Contract `wfNodeDisplayState`.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowNodeDisplayState::Queued => crate::contract::WF_NODE_DISPLAY_STATE_QUEUED,
            WorkflowNodeDisplayState::Running => crate::contract::WF_NODE_DISPLAY_STATE_RUNNING,
            WorkflowNodeDisplayState::Done => crate::contract::WF_NODE_DISPLAY_STATE_DONE,
            WorkflowNodeDisplayState::Failed => crate::contract::WF_NODE_DISPLAY_STATE_FAILED,
            WorkflowNodeDisplayState::Skipped => crate::contract::WF_NODE_DISPLAY_STATE_SKIPPED,
        }
    }

    /// The chip's caption.
    pub fn label(self) -> &'static str {
        match self {
            WorkflowNodeDisplayState::Queued => "Queued",
            WorkflowNodeDisplayState::Running => "Running",
            WorkflowNodeDisplayState::Done => "Done",
            WorkflowNodeDisplayState::Failed => "Failed",
            WorkflowNodeDisplayState::Skipped => "Skipped",
        }
    }
}

/// An internal node state, as a person reads it. An unknown state (a newer
/// server) reads Queued.
pub fn workflow_node_display_state(state: &str) -> WorkflowNodeDisplayState {
    match state {
        "running" | "waiting" | "in_review" | "updating" => WorkflowNodeDisplayState::Running,
        "landed" => WorkflowNodeDisplayState::Done,
        "failed" => WorkflowNodeDisplayState::Failed,
        "skipped" => WorkflowNodeDisplayState::Skipped,
        // proposed / blocked / ready, and anything newer.
        _ => WorkflowNodeDisplayState::Queued,
    }
}

/// The badge on a node whose run waits on a person.
pub const NEEDS_YOU_LABEL: &str = "needs you";

/// One node as the strip reads it (EXP-1082 declares, a later issue draws).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripNodeInput {
    pub id: String,
    pub identifier: String,
    pub state: String,
    pub wave: i64,
    pub lane: i64,
    pub members: usize,
    pub live: bool,
    pub needs_you: bool,
    pub note: Option<String>,
}

/// One chip of the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeChip {
    pub id: String,
    pub title: String,
    pub display: WorkflowNodeDisplayState,
    pub caption: String,
    pub stacked: bool,
    pub members: usize,
    pub live: bool,
    pub needs_you: bool,
}

/// One wave column of the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripWave {
    pub wave: i64,
    pub nodes: Vec<NodeChip>,
}

/// The node strip IS the graph: waves left to right (only the waves that hold
/// a node), lanes top to bottom within a wave, ties by id. The edges are the
/// mini-graph popover's business; the strip only orders. A compound node is
/// `stacked`; the caption is the node's note while it has one, else its
/// display label.
pub fn workflow_node_strip(nodes: &[StripNodeInput], _edges: &[(String, String)]) -> Vec<StripWave> {
    let mut sorted: Vec<&StripNodeInput> = nodes.iter().collect();
    sorted.sort_by(|a, b| {
        a.wave
            .cmp(&b.wave)
            .then_with(|| a.lane.cmp(&b.lane))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut waves: Vec<StripWave> = Vec::new();
    for node in sorted {
        let display = workflow_node_display_state(&node.state);
        let caption = node
            .note
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| display.label().to_string());
        let chip = NodeChip {
            id: node.id.clone(),
            title: workflow_node_title(&node.identifier, node.members),
            display,
            caption,
            stacked: node.members > 0,
            members: node.members,
            live: node.live,
            needs_you: node.needs_you,
        };
        match waves.last_mut() {
            Some(last) if last.wave == node.wave => last.nodes.push(chip),
            _ => waves.push(StripWave {
                wave: node.wave,
                nodes: vec![chip],
            }),
        }
    }
    waves
}

/// One node as the header caption counts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderNode {
    pub state: String,
    pub members: usize,
}

/// The one line under the workflow's name. A draft counts its ISSUES
/// (members included): `Draft · 8 issues`. A started workflow names its
/// runner and counts NODES: `on MacBook · 5 of 8 done · 2 running` (the
/// running tail only while something runs). Over, it counts what happened:
/// `Done · 2 done · 1 skipped`. A `proposed` node was never admitted and is
/// not counted.
pub fn workflow_header_caption(
    status: &str,
    nodes: &[HeaderNode],
    device_label: Option<&str>,
) -> String {
    let admitted: Vec<&HeaderNode> = nodes.iter().filter(|node| node.state != "proposed").collect();
    if status == "draft" {
        let issues: usize = admitted.iter().map(|node| 1 + node.members).sum();
        return if issues == 1 {
            "Draft · 1 issue".to_string()
        } else {
            format!("Draft · {issues} issues")
        };
    }
    let tally = |display: WorkflowNodeDisplayState| {
        admitted
            .iter()
            .filter(|node| workflow_node_display_state(&node.state) == display)
            .count()
    };
    let done = tally(WorkflowNodeDisplayState::Done);
    if status == "running" || status == "paused" {
        let mut parts = Vec::new();
        if let Some(device) = device_label {
            parts.push(format!("on {device}"));
        }
        parts.push(format!("{done} of {} done", admitted.len()));
        let running = tally(WorkflowNodeDisplayState::Running);
        if running > 0 {
            parts.push(format!("{running} running"));
        }
        return parts.join(" · ");
    }
    let word = match status {
        "draft" => "Draft".to_string(),
        "done" => "Done".to_string(),
        "failed" => "Failed".to_string(),
        "cancelled" => "Cancelled".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    };
    let mut parts = vec![word, format!("{done} done")];
    let failed = tally(WorkflowNodeDisplayState::Failed);
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let skipped = tally(WorkflowNodeDisplayState::Skipped);
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    parts.join(" · ")
}

/// The workflow header's ONE primary action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowPrimaryAction {
    PickDevice,
    Start,
    Pause,
    Resume,
    ReviewFinalPr,
}

impl WorkflowPrimaryAction {
    /// The fixture's wire word.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowPrimaryAction::PickDevice => "pick_device",
            WorkflowPrimaryAction::Start => "start",
            WorkflowPrimaryAction::Pause => "pause",
            WorkflowPrimaryAction::Resume => "resume",
            WorkflowPrimaryAction::ReviewFinalPr => "review_final_pr",
        }
    }
}

/// The header's ONE primary button: a draft without a runner picks one, a
/// draft starts, running pauses, paused resumes, done reviews the final PR.
/// Failed and cancelled offer nothing; Stop and Delete live in the overflow.
pub fn workflow_primary_action(
    status: &str,
    device_label: Option<&str>,
) -> Option<WorkflowPrimaryAction> {
    match status {
        "draft" if device_label.is_some() => Some(WorkflowPrimaryAction::Start),
        "draft" => Some(WorkflowPrimaryAction::PickDevice),
        "running" => Some(WorkflowPrimaryAction::Pause),
        "paused" => Some(WorkflowPrimaryAction::Resume),
        "done" => Some(WorkflowPrimaryAction::ReviewFinalPr),
        _ => None,
    }
}

/// The page's own words, byte-identical ×4 (fixture `pageLabels`).
pub const ALL_NODES_LABEL: &str = "All";
pub const DECISIONS_LABEL: &str = "Decisions";
/// The overflow's ending verb — `workflows.cancel` behind
/// [`CANCEL_WORKFLOW_CONFIRM`].
pub const STOP_WORKFLOW_LABEL: &str = "Stop";
/// The `pick_device` primary button.
pub const PICK_DEVICE_LABEL: &str = "Pick device";
/// A draft's overflow entry that re-picks the runner.
pub const RUNS_ON_LABEL: &str = "Runs on";
pub const REVIEW_FINAL_PR_LABEL: &str = "Review final PR";

/// One entry of the header's overflow menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowOverflowItem {
    Plan,
    RunsOn,
    Stop,
    Delete,
}

impl WorkflowOverflowItem {
    /// The fixture's wire word.
    pub fn as_wire(self) -> &'static str {
        match self {
            WorkflowOverflowItem::Plan => "plan",
            WorkflowOverflowItem::RunsOn => "runs_on",
            WorkflowOverflowItem::Stop => "stop",
            WorkflowOverflowItem::Delete => "delete",
        }
    }
}

/// The header's overflow menu, in order: a draft plans, re-picks its runner
/// and can be deleted; a running or paused one stops; anything else deletes.
pub fn workflow_overflow_menu(status: &str) -> Vec<WorkflowOverflowItem> {
    match status {
        "draft" => vec![
            WorkflowOverflowItem::Plan,
            WorkflowOverflowItem::RunsOn,
            WorkflowOverflowItem::Delete,
        ],
        "running" | "paused" => vec![WorkflowOverflowItem::Stop],
        _ => vec![WorkflowOverflowItem::Delete],
    }
}

/// A node chip's menu entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeChipAction {
    Retry,
    Skip,
    Admit,
    Dismiss,
}

impl NodeChipAction {
    /// The fixture's wire word.
    pub fn as_wire(self) -> &'static str {
        match self {
            NodeChipAction::Retry => "retry",
            NodeChipAction::Skip => "skip",
            NodeChipAction::Admit => "admit",
            NodeChipAction::Dismiss => "dismiss",
        }
    }

    /// The menu entry's label.
    pub fn label(self) -> &'static str {
        match self {
            NodeChipAction::Retry => RETRY_NODE_LABEL,
            NodeChipAction::Skip => SKIP_NODE_LABEL,
            NodeChipAction::Admit => ADMIT_NODE_LABEL,
            NodeChipAction::Dismiss => DISMISS_NODE_LABEL,
        }
    }
}

/// What a node chip's menu offers: Retry / Skip on a `failed` node, Admit /
/// Dismiss on a `proposed` one (a follow-up filed mid-run), nothing else.
pub fn node_chip_menu(state: &str) -> Vec<NodeChipAction> {
    match state {
        "failed" => vec![NodeChipAction::Retry, NodeChipAction::Skip],
        "proposed" => vec![NodeChipAction::Admit, NodeChipAction::Dismiss],
        _ => Vec::new(),
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
    struct Fixture {
        bands: Vec<FixtureBand>,
        shape_lines: Vec<FixtureShapeLine>,
        captions: Vec<FixtureCaption>,
        titles: Vec<FixtureTitle>,
        edges: Vec<FixtureEdgeCase>,
        start_blockers: Vec<FixtureStartBlocker>,
        final_pr: Vec<FixtureFinalPr>,
        row_subtitles: Vec<FixtureRowSubtitle>,
        /// EXP-983.
        edge_styles: Vec<FixtureEdgeStyle>,
        /// EXP-984.
        review_lines: Vec<FixtureReviewLine>,
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
            };
            assert_eq!(
                workflow_start_blocker(workflow, &case.metrics.shape()),
                case.blocker,
                "startBlockers[{index}]"
            );
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
    }

    /// The bands render in the fixture's order under their own titles.
    #[test]
    fn the_bands_keep_their_order_and_titles() {
        let titles: Vec<&str> = WORKFLOW_BANDS.iter().map(|band| band.title()).collect();
        assert_eq!(titles, ["Running", "Draft", "Done"]);
        let wires: Vec<&str> = WORKFLOW_BANDS.iter().map(|band| band.as_wire()).collect();
        assert_eq!(wires, ["running", "draft", "done"]);
    }

    /// The strings the ×4 clients say WORD FOR WORD (web
    /// `lib/workflow-view.ts`, iOS `WorkflowView.swift`, Android
    /// `WorkflowView.kt`): the final-PR merge pair and the node panel's two
    /// EXP-1014 lines. A drift here is a drift in the product's voice.
    #[test]
    fn the_shared_labels_are_byte_identical() {
        assert_eq!(MERGE_FINAL_PR_LABEL, "Merge");
        assert_eq!(
            MERGE_FINAL_PR_CONFIRM,
            "The workflow's branch is squash-merged into the default branch and the run is done."
        );
        assert_eq!(NODE_MODEL_LABEL, "Model");
        assert_eq!(NODE_UNSYNCED_TITLE, "Not synced yet");
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

/// EXP-1082 — the display vocabulary, replayed off the contract fixture.
#[cfg(test)]
mod display_tests {
    use super::*;
    use serde::Deserialize;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/workflow-view.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        display_states: Vec<DisplayCase>,
        needs_you_label: String,
        node_strips: Vec<StripCase>,
        header_captions: Vec<HeaderCase>,
        primary_actions: Vec<ActionCase>,
        chip_menus: Vec<MenuCase>,
        overflow_menus: Vec<MenuStatusCase>,
        page_labels: PageLabels,
    }

    #[derive(Deserialize)]
    struct MenuStatusCase {
        status: String,
        menu: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PageLabels {
        all_nodes: String,
        decisions: String,
        stop: String,
        pick_device: String,
        runs_on: String,
        review_final_pr: String,
    }

    #[derive(Deserialize)]
    struct DisplayCase {
        state: String,
        display: String,
        caption: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct StripNodeCase {
        id: String,
        identifier: String,
        state: String,
        wave: i64,
        lane: i64,
        members: usize,
        live: bool,
        needs_you: bool,
        #[serde(default)]
        note: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ChipCase {
        id: String,
        title: String,
        display: String,
        caption: String,
        stacked: bool,
        members: usize,
        live: bool,
        needs_you: bool,
    }

    #[derive(Deserialize)]
    struct WaveCase {
        wave: i64,
        nodes: Vec<ChipCase>,
    }

    #[derive(Deserialize)]
    struct StripCase {
        name: String,
        #[serde(default)]
        skip: bool,
        nodes: Vec<StripNodeCase>,
        edges: Vec<(String, String)>,
        strip: Vec<WaveCase>,
    }

    #[derive(Deserialize)]
    struct HeaderNodeCase {
        state: String,
        members: usize,
    }

    #[derive(Deserialize)]
    struct HeaderCase {
        name: String,
        #[serde(default)]
        skip: bool,
        status: String,
        device: Option<String>,
        nodes: Vec<HeaderNodeCase>,
        caption: String,
    }

    #[derive(Deserialize)]
    struct ActionCase {
        status: String,
        device: Option<String>,
        action: Option<String>,
    }

    #[derive(Deserialize)]
    struct MenuCase {
        state: String,
        menu: Vec<String>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("workflow-view.json parses")
    }

    #[test]
    fn display_states_match_the_fixture() {
        let cases = fixture().display_states;
        assert!(!cases.is_empty());
        for case in cases {
            let display = workflow_node_display_state(&case.state);
            assert_eq!(display.as_wire(), case.display, "state {}", case.state);
            assert_eq!(display.label(), case.caption, "state {}", case.state);
        }
    }

    #[test]
    fn display_states_cover_the_contract() {
        let ours: Vec<&str> = [
            WorkflowNodeDisplayState::Queued,
            WorkflowNodeDisplayState::Running,
            WorkflowNodeDisplayState::Done,
            WorkflowNodeDisplayState::Failed,
            WorkflowNodeDisplayState::Skipped,
        ]
        .into_iter()
        .map(WorkflowNodeDisplayState::as_wire)
        .collect();
        assert_eq!(ours, crate::contract::WF_NODE_DISPLAY_STATE_VALUES);
    }

    #[test]
    fn needs_you_label_matches_the_fixture() {
        assert_eq!(NEEDS_YOU_LABEL, fixture().needs_you_label);
    }

    #[test]
    fn node_strips_match_the_fixture() {
        for case in fixture().node_strips.into_iter().filter(|case| !case.skip) {
            let nodes: Vec<StripNodeInput> = case
                .nodes
                .into_iter()
                .map(|node| StripNodeInput {
                    id: node.id,
                    identifier: node.identifier,
                    state: node.state,
                    wave: node.wave,
                    lane: node.lane,
                    members: node.members,
                    live: node.live,
                    needs_you: node.needs_you,
                    note: node.note,
                })
                .collect();
            let got: Vec<(i64, Vec<(String, String, &str, String, bool, usize, bool, bool)>)> =
                workflow_node_strip(&nodes, &case.edges)
                    .into_iter()
                    .map(|wave| {
                        (
                            wave.wave,
                            wave.nodes
                                .into_iter()
                                .map(|chip| {
                                    (
                                        chip.id,
                                        chip.title,
                                        chip.display.as_wire(),
                                        chip.caption,
                                        chip.stacked,
                                        chip.members,
                                        chip.live,
                                        chip.needs_you,
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();
            let want: Vec<(i64, Vec<(String, String, &str, String, bool, usize, bool, bool)>)> =
                case.strip
                    .iter()
                    .map(|wave| {
                        (
                            wave.wave,
                            wave.nodes
                                .iter()
                                .map(|chip| {
                                    (
                                        chip.id.clone(),
                                        chip.title.clone(),
                                        chip.display.as_str(),
                                        chip.caption.clone(),
                                        chip.stacked,
                                        chip.members,
                                        chip.live,
                                        chip.needs_you,
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();
            assert_eq!(got, want, "case: {}", case.name);
        }
    }

    #[test]
    fn header_captions_match_the_fixture() {
        for case in fixture().header_captions.into_iter().filter(|case| !case.skip) {
            let nodes: Vec<HeaderNode> = case
                .nodes
                .into_iter()
                .map(|node| HeaderNode {
                    state: node.state,
                    members: node.members,
                })
                .collect();
            assert_eq!(
                workflow_header_caption(&case.status, &nodes, case.device.as_deref()),
                case.caption,
                "case: {}",
                case.name
            );
        }
    }

    #[test]
    fn primary_actions_match_the_fixture() {
        for case in fixture().primary_actions {
            assert_eq!(
                workflow_primary_action(&case.status, case.device.as_deref())
                    .map(WorkflowPrimaryAction::as_wire),
                case.action.as_deref(),
                "status {}",
                case.status
            );
        }
    }

    #[test]
    fn chip_menus_match_the_fixture() {
        for case in fixture().chip_menus {
            let got: Vec<&str> = node_chip_menu(&case.state)
                .into_iter()
                .map(NodeChipAction::as_wire)
                .collect();
            assert_eq!(got, case.menu, "state {}", case.state);
        }
    }

    #[test]
    fn overflow_menus_match_the_fixture() {
        let cases = fixture().overflow_menus;
        assert!(!cases.is_empty());
        for case in cases {
            let got: Vec<&str> = workflow_overflow_menu(&case.status)
                .into_iter()
                .map(WorkflowOverflowItem::as_wire)
                .collect();
            assert_eq!(got, case.menu, "status {}", case.status);
        }
    }

    #[test]
    fn page_labels_match_the_fixture() {
        let labels = fixture().page_labels;
        assert_eq!(ALL_NODES_LABEL, labels.all_nodes);
        assert_eq!(DECISIONS_LABEL, labels.decisions);
        assert_eq!(STOP_WORKFLOW_LABEL, labels.stop);
        assert_eq!(PICK_DEVICE_LABEL, labels.pick_device);
        assert_eq!(RUNS_ON_LABEL, labels.runs_on);
        assert_eq!(REVIEW_FINAL_PR_LABEL, labels.review_final_pr);
    }
}
