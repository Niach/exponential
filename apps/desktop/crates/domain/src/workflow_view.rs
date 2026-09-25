//! EXP-981 — what every client SAYS about a workflow.
//!
//! The graph's geometry is the server's (`wave`/`lane` on the synced nodes);
//! this is the rest — bands, captions, the edges between nodes — mirrored ×4
//! (web `apps/web/src/lib/workflow-view.ts`, iOS `WorkflowView.swift`, Android
//! `WorkflowView.kt`) and locked by the contract fixture
//! `packages/domain-contract/fixtures/workflow-view.json`. All strings
//! byte-identical. The page these words land on (EXP-1084/1085/1086) is a
//! node STRIP picker (`All`, then the chips in DAG order) over the work FACE
//! toggle (Issue · Runs · Changes · Results): a selected node's lines below
//! read on its Issue face, not in a separate node panel.

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

// ── Review gate, dynamic graphs (EXP-984) ─────────────────

/// A `proposed` node's two decisions, and the sentence that explains it.
pub const ADMIT_NODE_LABEL: &str = "Admit";
pub const DISMISS_NODE_LABEL: &str = "Dismiss";
pub const PROPOSED_NODE_NOTE: &str =
    "Filed during the run. Admit it into the workflow or dismiss it.";
/// EXP-1014: the chip of a node whose issue row has not synced yet — the
/// identifier slot shows the first 8 characters of the issue id, the title
/// this line. Byte-identical ×4.
pub const NODE_UNSYNCED_TITLE: &str = "Not synced yet";

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
/// `stacked`; the caption is the node's note while it has one, a `proposed`
/// node's [`PROPOSED_NODE_NOTE`] (why its menu offers Admit / Dismiss), else
/// its display label.
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
            .unwrap_or_else(|| {
                if node.state == "proposed" {
                    PROPOSED_NODE_NOTE.to_string()
                } else {
                    display.label().to_string()
                }
            });
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
/// draft starts; a running or paused workflow whose final PR is OPEN reviews
/// it (the ONE human review — the workflow is `done` only once it merged),
/// else running pauses and paused resumes; done reviews the merged final PR.
/// Failed and cancelled offer nothing; Stop and Delete live in the overflow.
pub fn workflow_primary_action(
    status: &str,
    device_label: Option<&str>,
    final_pr_state: Option<&str>,
) -> Option<WorkflowPrimaryAction> {
    match status {
        "draft" if device_label.is_some() => Some(WorkflowPrimaryAction::Start),
        "draft" => Some(WorkflowPrimaryAction::PickDevice),
        "running" | "paused" if final_pr_state == Some("open") => {
            Some(WorkflowPrimaryAction::ReviewFinalPr)
        }
        "running" => Some(WorkflowPrimaryAction::Pause),
        "paused" => Some(WorkflowPrimaryAction::Resume),
        "done" => Some(WorkflowPrimaryAction::ReviewFinalPr),
        _ => None,
    }
}

/// The header's status glyph, as the node display state it reads like:
/// draft → queued, running/paused → running, done → done, failed → failed,
/// cancelled → skipped, anything newer → queued. Locked ×4.
pub fn workflow_status_glyph(status: &str) -> WorkflowNodeDisplayState {
    match status {
        "running" | "paused" => WorkflowNodeDisplayState::Running,
        "done" => WorkflowNodeDisplayState::Done,
        "failed" => WorkflowNodeDisplayState::Failed,
        "cancelled" => WorkflowNodeDisplayState::Skipped,
        _ => WorkflowNodeDisplayState::Queued,
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
/// A node on the Changes face whose issue has no pull request yet.
pub const NO_CHANGES_LABEL: &str = "No changes yet";
/// The Dismiss confirm on a `proposed` node (the Skip confirm's shape).
pub const DISMISS_NODE_CONFIRM: &str = "The node is removed from the workflow.";

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

// ---------------------------------------------------------------------------
// EXP-1084 — the page's PICKER model (fixture `selection`, mirrored ×4: web
// `lib/workflow-selection.ts`, iOS/Android `WorkflowSelection`)
// ---------------------------------------------------------------------------

/// What the node strip has picked. The strip is `All` (position 0) then every
/// node in DAG order; an EMPTY `ids` IS `All`.
///
/// The rules: a click picks exactly that node (a click on the picked chip
/// keeps it; `All` or an unknown node = `All`); cmd/ctrl-click toggles one
/// node in or out, in DAG order (the last one out = `All`); shift-click picks
/// the DAG range from the anchor (the last plain or toggling click) to the
/// node, the anchor staying; a step moves ONE position from the cursor (the
/// last clicked or stepped node, else the last picked one) through `All` +
/// the nodes, clamped, landing on one node or `All`; pruning drops nodes that
/// left, keeping anchor/cursor when they stay, else the first picked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowSelection {
    /// The picked nodes, in DAG order. Empty = `All`.
    pub ids: Vec<String>,
    /// Where a shift-click range starts: the last plain or toggling click.
    pub anchor: Option<String>,
    /// The node the keyboard steps from: the last one clicked or stepped to.
    pub cursor: Option<String>,
}

impl WorkflowSelection {
    pub fn is_all(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|picked| picked == id)
    }

    /// The picked ids, in the order given (the strip's DAG order).
    pub fn picked_in<'a>(&self, order: &'a [String]) -> Vec<&'a String> {
        order.iter().filter(|id| self.contains(id)).collect()
    }

    /// The one picked node, when exactly one is.
    pub fn single(&self) -> Option<&str> {
        (self.ids.len() == 1).then(|| self.ids[0].as_str())
    }

    /// Back to `All`.
    pub fn select_all(&mut self) {
        *self = Self::default();
    }

    /// Exactly one node, anchored and cursored on it.
    pub fn only(id: &str) -> Self {
        Self {
            ids: vec![id.to_string()],
            anchor: Some(id.to_string()),
            cursor: Some(id.to_string()),
        }
    }

    /// A plain click. `None` = the `All` chip.
    pub fn click(&mut self, order: &[String], id: Option<&str>) {
        *self = match id {
            Some(id) if order.iter().any(|node| node == id) => Self::only(id),
            _ => Self::default(),
        };
    }

    /// Cmd/ctrl-click: one node in or out, in DAG order; the last one out
    /// is `All`.
    pub fn toggle(&mut self, order: &[String], id: &str) {
        if !order.iter().any(|node| node == id) {
            self.select_all();
            return;
        }
        let mut picked: Vec<String> = self.ids.clone();
        if let Some(index) = picked.iter().position(|node| node == id) {
            picked.remove(index);
        } else {
            picked.push(id.to_string());
        }
        let ids: Vec<String> = order
            .iter()
            .filter(|node| picked.contains(node))
            .cloned()
            .collect();
        *self = if ids.is_empty() {
            Self::default()
        } else {
            Self {
                ids,
                anchor: Some(id.to_string()),
                cursor: Some(id.to_string()),
            }
        };
    }

    /// Shift-click: the DAG range from the anchor to `id`; the anchor stays.
    /// Without an anchor it anchors on the node.
    pub fn extend(&mut self, order: &[String], id: &str) {
        let Some(to) = order.iter().position(|node| node == id) else {
            self.select_all();
            return;
        };
        let (anchor, from) = match self
            .anchor
            .as_deref()
            .and_then(|anchor| order.iter().position(|node| node == anchor).map(|at| (anchor, at)))
        {
            Some((anchor, at)) => (anchor.to_string(), at),
            None => (id.to_string(), to),
        };
        let (low, high) = (from.min(to), from.max(to));
        *self = Self {
            ids: order[low..=high].to_vec(),
            anchor: Some(anchor),
            cursor: Some(id.to_string()),
        };
    }

    /// ←/→ (and k/j): ONE step from the cursor over `All` + the DAG order,
    /// clamped at both ends, landing on exactly one node or `All`.
    pub fn step(&mut self, order: &[String], delta: i64) {
        let cursor = if self.ids.is_empty() {
            None
        } else {
            match self.cursor.as_deref() {
                Some(cursor) if self.contains(cursor) => Some(cursor),
                _ => self.ids.last().map(String::as_str),
            }
        };
        let position = cursor
            .and_then(|cursor| order.iter().position(|node| node == cursor))
            .map_or(0, |index| index as i64 + 1);
        let next = (position + delta).clamp(0, order.len() as i64) as usize;
        *self = if next == 0 {
            Self::default()
        } else {
            Self::only(&order[next - 1])
        };
    }

    /// Drops the nodes that left the workflow (a replan folded them away).
    pub fn prune(&mut self, order: &[String]) {
        let ids: Vec<String> = order
            .iter()
            .filter(|node| self.contains(node))
            .cloned()
            .collect();
        if ids.len() == self.ids.len() {
            return;
        }
        if ids.is_empty() {
            self.select_all();
            return;
        }
        let keep = |held: &Option<String>| match held {
            Some(id) if ids.contains(id) => Some(id.clone()),
            _ => Some(ids[0].clone()),
        };
        let anchor = keep(&self.anchor);
        let cursor = keep(&self.cursor);
        *self = Self { ids, anchor, cursor };
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
    struct Fixture {
        bands: Vec<FixtureBand>,
        shape_lines: Vec<FixtureShapeLine>,
        titles: Vec<FixtureTitle>,
        edges: Vec<FixtureEdgeCase>,
        start_blockers: Vec<FixtureStartBlocker>,
        final_pr: Vec<FixtureFinalPr>,
        row_subtitles: Vec<FixtureRowSubtitle>,
        /// EXP-983.
        edge_styles: Vec<FixtureEdgeStyle>,
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
    /// `WorkflowView.kt`): the final-PR merge pair and the unsynced chip's
    /// EXP-1014 title. A drift here is a drift in the product's voice.
    #[test]
    fn the_shared_labels_are_byte_identical() {
        assert_eq!(MERGE_FINAL_PR_LABEL, "Merge");
        assert_eq!(
            MERGE_FINAL_PR_CONFIRM,
            "The workflow's branch is squash-merged into the default branch and the run is done."
        );
        assert_eq!(NODE_UNSYNCED_TITLE, "Not synced yet");
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
        status_glyphs: Vec<GlyphCase>,
        proposed_node_note: String,
        selection: Vec<SelectionCase>,
    }

    #[derive(Deserialize)]
    struct GlyphCase {
        status: String,
        display: String,
    }

    #[derive(Deserialize)]
    struct SelectionState {
        ids: Vec<String>,
        anchor: Option<String>,
        cursor: Option<String>,
    }

    impl SelectionState {
        fn model(&self) -> WorkflowSelection {
            WorkflowSelection {
                ids: self.ids.clone(),
                anchor: self.anchor.clone(),
                cursor: self.cursor.clone(),
            }
        }
    }

    #[derive(Deserialize)]
    struct SelectionOp {
        kind: String,
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        delta: Option<i64>,
    }

    #[derive(Deserialize)]
    struct SelectionCase {
        name: String,
        order: Vec<String>,
        current: SelectionState,
        op: SelectionOp,
        expected: SelectionState,
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
        no_changes: String,
        dismiss_node_confirm: String,
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
    #[serde(rename_all = "camelCase")]
    struct ActionCase {
        status: String,
        device: Option<String>,
        action: Option<String>,
        #[serde(default)]
        final_pr_state: Option<String>,
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
                workflow_primary_action(
                    &case.status,
                    case.device.as_deref(),
                    case.final_pr_state.as_deref()
                )
                .map(WorkflowPrimaryAction::as_wire),
                case.action.as_deref(),
                "status {} finalPrState {:?}",
                case.status,
                case.final_pr_state
            );
        }
    }

    #[test]
    fn status_glyphs_match_the_fixture() {
        let cases = fixture().status_glyphs;
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                workflow_status_glyph(&case.status).as_wire(),
                case.display,
                "status {}",
                case.status
            );
        }
    }

    #[test]
    fn proposed_node_note_matches_the_fixture() {
        assert_eq!(PROPOSED_NODE_NOTE, fixture().proposed_node_note);
    }

    #[test]
    fn selection_matches_the_fixture() {
        let cases = fixture().selection;
        assert!(!cases.is_empty());
        for case in cases {
            let mut model = case.current.model();
            let id = case.op.id.as_deref();
            match case.op.kind.as_str() {
                "click" => model.click(&case.order, id),
                "toggle" => model.toggle(&case.order, id.expect("toggle id")),
                "extend" => model.extend(&case.order, id.expect("extend id")),
                "step" => model.step(&case.order, case.op.delta.expect("step delta")),
                "prune" => model.prune(&case.order),
                other => panic!("unknown selection op {other}"),
            }
            assert_eq!(model, case.expected.model(), "case: {}", case.name);
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
        assert_eq!(NO_CHANGES_LABEL, labels.no_changes);
        assert_eq!(DISMISS_NODE_CONFIRM, labels.dismiss_node_confirm);
    }
}
