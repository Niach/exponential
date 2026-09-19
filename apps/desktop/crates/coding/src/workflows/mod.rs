//! The workflow ENGINE (EXP-982): a deterministic, level-triggered
//! orchestrator for a started workflow. The orchestrator is CODE, not an
//! agent — same architecture as [`crate::automations`], whose module doc is
//! the authority on the shape: a PURE [`evaluate`] over a plain-data
//! [`Snapshot`] (no IO, no clock reads, no randomness — `now_ms` comes IN),
//! and two HOSTS that do every side effect: the desktop GUI's
//! `ui::workflow_host` and the CLI daemon's worker.
//!
//! Single writer = `workflows.device_id`: each host only ever evaluates
//! workflows whose device id equals ITS OWN (the GUI and the daemon have
//! distinct ids), which is the double-run defence, exactly as for
//! automations.
//!
//! P3 supports `start_on: landed` only: a node starts when ALL its blockers
//! landed, so EVERY node bases on the integration branch. The engine runs
//! each node exactly once (Kahn from the roots), lands reviewed PRs into the
//! integration branch in order (the merge train), then opens ONE final PR
//! integration → default.
//!
//! The rule order in [`evaluate`] IS the contract, and it is fixture-tested
//! as DATA: `crates/coding/tests/fixtures/workflows/*.json`, each
//! `{name, snapshot, expected}`, replayed by `tests/workflow_engine.rs`.

pub mod branch;
pub mod state;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use branch::{delete_integration_branch, ensure_integration_branch};
pub use state::{read_states, write_states, WorkflowState, WORKFLOW_ENGINE_KEY};

/// The evaluation cadence both hosts beat at (the automations host's).
pub const BEAT_SECONDS: u64 = 30;

/// The launch options a workflow node runs with — the ONE resolution both
/// hosts use. The workflow may PIN an agent/model/effort/subagent model and
/// an account; every unpinned field falls back to the device's own launch
/// defaults. Plan mode is forced OFF: an unattended run must never park at
/// the plan-approval card waiting for a human who is not watching.
pub fn launch_options(
    settings: &crate::Settings,
    agent: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
    subagent_model: Option<&str>,
    account: Option<&str>,
) -> crate::LaunchOptions {
    let mut options = crate::automations::launch_options(settings, agent, model, effort);
    if let Some(subagent_model) = subagent_model.filter(|value| !value.is_empty()) {
        options.subagent_model = subagent_model.to_string();
    }
    if let Some(account) = account.filter(|value| !value.is_empty()) {
        options.account = Some(account.to_string());
    }
    options
}

/// The note a node carries while it waits for an answer.
pub const NOTE_NEEDS_ANSWER: &str = "Needs an answer";
/// The note a node carries while its agent is rate limited.
pub const NOTE_RATE_LIMITED: &str = "Rate limited";
/// The note a node carries when its run ended with nothing to review.
pub const NOTE_NO_PULL_REQUEST: &str = "The run ended without a pull request";
/// What a rate-limited run is told once its window reset.
pub const NUDGE_RATE_LIMIT_RESET: &str =
    "The rate limit has reset. Continue where you left off.";

/// The workflow row, as plain data.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFacts {
    pub id: String,
    /// contract `wfStatus` — a raw wire word (an unknown one decides nothing).
    pub status: String,
    /// contract `wfGate`; `none` = only the contract needs a person.
    pub gate: String,
    pub integration_branch: String,
    #[serde(default)]
    pub final_pr_url: Option<String>,
    /// `launch.maxParallel`, or the contract default.
    pub max_parallel: usize,
}

/// One `workflow_nodes` row, as plain data.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeFacts {
    pub id: String,
    /// The node's representative issue (a compound node's PARENT).
    pub issue_id: String,
    /// A compound node's sub-issues; empty for a plain node.
    #[serde(default)]
    pub member_issue_ids: Vec<String>,
    /// contract `wfNodeKind` / `wfNodeState` — raw wire words.
    pub kind: String,
    pub state: String,
    #[serde(default)]
    pub wave: i64,
    #[serde(default)]
    pub lane: i64,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub attempt: i64,
    /// Only its PRESENCE matters — the human gate's stamp.
    #[serde(default)]
    pub approved_at: Option<String>,
}

/// What the engine reads off a node's representative issue.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IssueFacts {
    /// `open` / `merged` / `closed`; `None` = no pull request yet.
    #[serde(default)]
    pub pr_state: Option<String>,
}

/// What the engine reads off a node's coding session.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFacts {
    /// Status `running` or `in_review` — the run is still up.
    #[serde(default)]
    pub live: bool,
    #[serde(default)]
    pub needs_input: bool,
    /// EXP-804: walled by a rate limit (the run STAYS `running`).
    #[serde(default)]
    pub blocked: bool,
    /// When the wall lifts, if the agent told us.
    #[serde(default)]
    pub blocked_resets_at_ms: Option<i64>,
    /// EXP-848: mid-turn right now.
    #[serde(default)]
    pub agent_busy: bool,
}

/// One evaluation pass's inputs — everything, including the clock.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub workflow: WorkflowFacts,
    pub nodes: Vec<NodeFacts>,
    /// `(from, to)` = `from` blocks `to`; the hosts build them with
    /// [`domain::workflow_view::workflow_edges`] off the synced `blocks`
    /// relations.
    #[serde(default)]
    pub edges: Vec<(String, String)>,
    /// Keyed by the node's representative issue id.
    #[serde(default)]
    pub issues: HashMap<String, IssueFacts>,
    #[serde(default)]
    pub sessions: HashMap<String, SessionFacts>,
    /// Host fact: `origin/<integration_branch>` is up.
    #[serde(default)]
    pub integration_branch_exists: bool,
    /// Host fact: node ids with a start/land/resume the host has not
    /// finished. They are left ALONE for the whole pass.
    #[serde(default)]
    pub in_flight: HashSet<String>,
    #[serde(default)]
    pub final_pr_in_flight: bool,
    /// Host fact: `(session id, resets_at_ms)` pairs already nudged.
    #[serde(default)]
    pub nudged: HashSet<(String, i64)>,
    pub now_ms: i64,
}

/// What one pass asks the host to do. Every side effect lives in the host.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    /// Cut `<integration_branch>` from the repository's default branch and
    /// push it, in the engine's OWN scratch clone.
    EnsureIntegrationBranch,
    /// `workflows.reportNode` — never for a `landed`/`skipped` node.
    #[serde(rename_all = "camelCase")]
    SetNodeState {
        node_id: String,
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// Report `running` FIRST, then launch the node's run locally.
    #[serde(rename_all = "camelCase")]
    StartNode { node_id: String, attempt: i64 },
    /// The merge train's one step: `workflows.landNode`.
    #[serde(rename_all = "camelCase")]
    LandNode { node_id: String },
    /// Steer a live run once, deduped by `key`.
    #[serde(rename_all = "camelCase")]
    Nudge {
        session_id: String,
        key: String,
        text: String,
    },
    /// `workflows.openFinalPr` — integration branch → the default branch.
    OpenFinalPr,
    /// Cancelled: end a live run.
    #[serde(rename_all = "camelCase")]
    KillSession { session_id: String },
    /// Cancelled and quiet: `git push origin --delete <integration_branch>`.
    DeleteIntegrationBranch,
}

/// The live states a node occupies while it holds a parallelism slot.
fn is_active(state: &str) -> bool {
    matches!(state, "running" | "waiting" | "updating")
}

/// The states a person owns: the engine never makes or unmakes them.
fn is_final(state: &str) -> bool {
    matches!(state, "landed" | "skipped")
}

/// One node's state AFTER this pass's mirror — what rules 2-5 read.
struct Mirrored<'a> {
    node: &'a NodeFacts,
    state: String,
}

/// The rule cascade, in order (the module doc names the hosts that run it):
/// 0. no integration branch → create it, and NOTHING else this pass;
/// 1. mirror every node's state off its session, PR and blockers (also while
///    `paused`); 2. start `ready` nodes up to `max_parallel`; 3. nudge a run
///    whose rate limit reset; 4. land ONE cleared node (the merge train);
/// 5. open the final PR once everything is in; 6. a `cancelled` workflow
///    ends its runs and drops its branch. `draft`/`done` decide nothing.
pub fn evaluate(snapshot: &Snapshot) -> Vec<Decision> {
    let status = snapshot.workflow.status.as_str();
    if status == "cancelled" {
        return cancel_decisions(snapshot);
    }
    if status != "running" && status != "paused" {
        return Vec::new();
    }

    // (0) Nothing can base on a branch that does not exist yet.
    if status == "running" && !snapshot.integration_branch_exists {
        return vec![Decision::EnsureIntegrationBranch];
    }

    let mut decisions = Vec::new();
    let order = ordered_nodes(snapshot);
    let blockers = blockers_by_node(snapshot);

    // (1) The mirror. A node the host is busy with, and a node a person
    // already resolved, are both left exactly where they are.
    let mut mirrored: Vec<Mirrored<'_>> = Vec::with_capacity(order.len());
    for node in &order {
        if is_final(&node.state) || snapshot.in_flight.contains(&node.id) {
            mirrored.push(Mirrored {
                node,
                state: node.state.clone(),
            });
            continue;
        }
        let Some(desired) = desired_state(snapshot, node, &blockers) else {
            // An unknown session (the row has not synced yet): decide
            // nothing rather than failing a run that is very much alive.
            mirrored.push(Mirrored {
                node,
                state: node.state.clone(),
            });
            continue;
        };
        match desired {
            Desired::Land => {
                decisions.push(Decision::LandNode {
                    node_id: node.id.clone(),
                });
                mirrored.push(Mirrored {
                    node,
                    state: node.state.clone(),
                });
            }
            Desired::State { state, note } => {
                if state != node.state {
                    decisions.push(Decision::SetNodeState {
                        node_id: node.id.clone(),
                        state: state.clone(),
                        note,
                    });
                }
                mirrored.push(Mirrored { node, state });
            }
        }
    }

    if status != "running" {
        // Paused: the mirror keeps reading true, nothing new starts or lands.
        return decisions;
    }

    // (2) Starts, in landing order, up to the workflow's parallelism.
    let mut active = mirrored
        .iter()
        .filter(|entry| {
            is_active(&entry.state) || snapshot.in_flight.contains(&entry.node.id)
        })
        .count();
    for entry in &mirrored {
        if active >= snapshot.workflow.max_parallel {
            break;
        }
        if entry.state != "ready" || snapshot.in_flight.contains(&entry.node.id) {
            continue;
        }
        decisions.push(Decision::StartNode {
            node_id: entry.node.id.clone(),
            attempt: entry.node.attempt + 1,
        });
        active += 1;
    }

    // (3) A wall that lifted: tell the run to carry on, exactly once.
    for entry in &mirrored {
        if entry.state != "waiting" {
            continue;
        }
        let Some(session_id) = entry.node.session_id.as_deref() else {
            continue;
        };
        let Some(session) = snapshot.sessions.get(session_id) else {
            continue;
        };
        if !session.live || !session.blocked || session.needs_input {
            continue;
        }
        let Some(resets_at) = session.blocked_resets_at_ms else {
            continue;
        };
        if resets_at > snapshot.now_ms {
            continue;
        }
        if snapshot
            .nudged
            .contains(&(session_id.to_string(), resets_at))
        {
            continue;
        }
        decisions.push(Decision::Nudge {
            session_id: session_id.to_string(),
            key: resets_at.to_string(),
            text: NUDGE_RATE_LIMIT_RESET.to_string(),
        });
    }

    // (4) The merge train: ONE land per pass, and only while the host is not
    // already landing one. The train is strict order among CLEARED nodes —
    // a node still waiting for a person never blocks a cleared one behind it.
    let landing = mirrored.iter().any(|entry| {
        snapshot.in_flight.contains(&entry.node.id)
            && (entry.state == "in_review" || entry.state == "updating")
    });
    if !landing {
        if let Some(entry) = mirrored.iter().find(|entry| {
            entry.state == "in_review" && is_cleared(&snapshot.workflow.gate, entry.node)
        }) {
            decisions.push(Decision::LandNode {
                node_id: entry.node.id.clone(),
            });
        }
    }

    // (5) Everything is in: the ONE final pull request.
    let all_in = !mirrored.is_empty()
        && mirrored.iter().all(|entry| is_final(&entry.state));
    let any_landed = mirrored.iter().any(|entry| entry.state == "landed");
    if all_in
        && any_landed
        && snapshot.workflow.final_pr_url.is_none()
        && !snapshot.final_pr_in_flight
    {
        decisions.push(Decision::OpenFinalPr);
    }

    decisions
}

/// (6) A cancelled workflow: end every live run, then drop the branch it
/// would have landed on. Nothing ever reached the default branch.
fn cancel_decisions(snapshot: &Snapshot) -> Vec<Decision> {
    let mut decisions = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for node in ordered_nodes(snapshot) {
        let Some(session_id) = node.session_id.as_deref() else {
            continue;
        };
        if !snapshot
            .sessions
            .get(session_id)
            .is_some_and(|session| session.live)
        {
            continue;
        }
        if seen.insert(session_id) {
            decisions.push(Decision::KillSession {
                session_id: session_id.to_string(),
            });
        }
    }
    if decisions.is_empty() && snapshot.integration_branch_exists {
        decisions.push(Decision::DeleteIntegrationBranch);
    }
    decisions
}

/// What the mirror wants for one node.
enum Desired {
    State { state: String, note: Option<String> },
    /// The PR merged behind our back — the server just marks the node landed.
    Land,
}

fn state(state: &str) -> Desired {
    Desired::State {
        state: state.to_string(),
        note: None,
    }
}

fn state_with_note(name: &str, note: &str) -> Desired {
    Desired::State {
        state: name.to_string(),
        note: Some(note.to_string()),
    }
}

/// Rule 1 for ONE node. `None` = the node names a session this device has
/// not synced yet; the pass leaves it alone.
fn desired_state(
    snapshot: &Snapshot,
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
) -> Option<Desired> {
    let Some(session_id) = node.session_id.as_deref() else {
        // Not started yet: the blockers decide.
        let ready = blockers
            .get(node.id.as_str())
            .map(|ids| {
                ids.iter().all(|id| {
                    snapshot
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == *id)
                        .is_some_and(|candidate| is_final(&candidate.state))
                })
            })
            .unwrap_or(true);
        return Some(state(if ready { "ready" } else { "blocked" }));
    };
    let session = snapshot.sessions.get(session_id)?;
    let pr_state = snapshot
        .issues
        .get(node.issue_id.as_str())
        .and_then(|issue| issue.pr_state.as_deref());
    if session.live {
        // A node merging the trunk in stays `updating` for as long as the
        // agent is actually working on it.
        if node.state == "updating" && session.agent_busy {
            return Some(state("updating"));
        }
        if pr_state == Some("open") {
            return Some(state("in_review"));
        }
        if session.needs_input {
            return Some(state_with_note("waiting", NOTE_NEEDS_ANSWER));
        }
        if session.blocked {
            return Some(state_with_note("waiting", NOTE_RATE_LIMITED));
        }
        return Some(state("running"));
    }
    match pr_state {
        Some("open") => Some(state("in_review")),
        Some("merged") => Some(Desired::Land),
        // The run ended with nothing to review: one free retry, then a
        // person's call.
        _ if node.attempt < 1 => Some(state("ready")),
        _ => Some(state_with_note("failed", NOTE_NO_PULL_REQUEST)),
    }
}

/// A node is cleared for the train when no person is owed an approval, or
/// one already gave it. Mirrors `domain::workflow_view` (and the server,
/// which enforces it).
fn is_cleared(gate: &str, node: &NodeFacts) -> bool {
    !domain::workflow_view::workflow_node_needs_approval(gate, &node.kind)
        || node.approved_at.is_some()
}

/// Every node in landing order — (wave, lane, id), the ONE order the mirror,
/// the starts and the train all walk.
fn ordered_nodes(snapshot: &Snapshot) -> Vec<&NodeFacts> {
    let mut nodes: Vec<&NodeFacts> = snapshot.nodes.iter().collect();
    nodes.sort_by(|a, b| {
        a.wave
            .cmp(&b.wave)
            .then_with(|| a.lane.cmp(&b.lane))
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes
}

/// `node id → the node ids blocking it`. An edge naming a node outside the
/// workflow is dropped (it cannot be waited on).
fn blockers_by_node(snapshot: &Snapshot) -> HashMap<&str, Vec<&str>> {
    let known: HashSet<&str> = snapshot.nodes.iter().map(|node| node.id.as_str()).collect();
    let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in &snapshot.edges {
        if !known.contains(from.as_str()) || !known.contains(to.as_str()) {
            continue;
        }
        map.entry(to.as_str()).or_default().push(from.as_str());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, state: &str, wave: i64, lane: i64) -> NodeFacts {
        NodeFacts {
            id: id.to_string(),
            issue_id: format!("issue-{id}"),
            kind: "leaf".to_string(),
            state: state.to_string(),
            wave,
            lane,
            ..NodeFacts::default()
        }
    }

    fn running(nodes: Vec<NodeFacts>) -> Snapshot {
        Snapshot {
            workflow: WorkflowFacts {
                id: "wf-1".to_string(),
                status: "running".to_string(),
                gate: "none".to_string(),
                integration_branch: "exp/wf-abcdef12".to_string(),
                final_pr_url: None,
                max_parallel: 3,
            },
            nodes,
            integration_branch_exists: true,
            now_ms: 1_000,
            ..Snapshot::default()
        }
    }

    /// Rule 0 short-circuits the WHOLE pass: nothing may base on a branch
    /// that is not up yet.
    #[test]
    fn a_missing_integration_branch_is_the_only_decision() {
        let mut snapshot = running(vec![node("a", "blocked", 0, 0)]);
        snapshot.integration_branch_exists = false;
        assert_eq!(evaluate(&snapshot), vec![Decision::EnsureIntegrationBranch]);
    }

    /// A draft or a finished workflow is inert; a paused one still mirrors
    /// but starts nothing.
    #[test]
    fn only_a_started_workflow_decides_anything() {
        for status in ["draft", "done", "brand-new"] {
            let mut snapshot = running(vec![node("a", "blocked", 0, 0)]);
            snapshot.workflow.status = status.to_string();
            assert!(evaluate(&snapshot).is_empty(), "status {status}");
        }
        let mut paused = running(vec![node("a", "blocked", 0, 0)]);
        paused.workflow.status = "paused".to_string();
        assert_eq!(
            evaluate(&paused),
            vec![Decision::SetNodeState {
                node_id: "a".to_string(),
                state: "ready".to_string(),
                note: None,
            }],
            "a paused workflow keeps reading true, it just starts nothing"
        );
    }

    /// The engine never makes or unmakes a person's call.
    #[test]
    fn landed_and_skipped_nodes_are_left_alone() {
        let snapshot = running(vec![node("a", "landed", 0, 0), node("b", "skipped", 0, 1)]);
        // Both are final, at least one landed: the pass goes straight to the
        // final pull request without touching either row.
        assert_eq!(evaluate(&snapshot), vec![Decision::OpenFinalPr]);
    }

    /// A node the host is mid-start on is untouched for the whole pass, and
    /// it still holds its parallelism slot.
    #[test]
    fn an_in_flight_node_holds_its_slot_and_its_state() {
        let mut snapshot = running(vec![node("a", "ready", 0, 0), node("b", "ready", 0, 1)]);
        snapshot.workflow.max_parallel = 1;
        snapshot.in_flight.insert("a".to_string());
        assert!(
            evaluate(&snapshot).is_empty(),
            "the in-flight start fills the only slot"
        );
    }
}
