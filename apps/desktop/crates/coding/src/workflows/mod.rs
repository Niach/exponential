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
//! The engine runs each node exactly once (Kahn from the roots), lands
//! reviewed PRs into the integration branch in order (the merge train), then
//! opens ONE final PR integration → default.
//!
//! EXP-983 makes the starts SPECULATIVE: all three `start_on` modes are live,
//! so a dependent may start before its blockers landed. It then bases on
//! THEIR work — one unlanded blocker means that blocker's branch, several
//! mean a synthetic base the host merges them into — upstream movement
//! propagates by MERGE (never a rebase, never a force-push), two siblings
//! whose work collides get a SERIALIZATION edge, and the train still lands in
//! topological order.
//!
//! EXP-984 closes the loop: under the `agent` gate every node's pushed branch
//! gets a REVIEWER run (adversarial, and never on the author's own model, for
//! a `risk: high` node) whose `request_changes` findings go back to the author
//! at most three rounds; a node that spends more than its budget is paused for
//! a person; and a follow-up node nobody admitted (`proposed`) is treated as
//! absent from the run entirely.
//!
//! The rule order in [`evaluate`] IS the contract, and it is fixture-tested
//! as DATA: `crates/coding/tests/fixtures/workflows/*.json`, each
//! `{name, snapshot, expected}`, replayed by `tests/workflow_engine.rs`.

pub mod base;
pub mod branch;
pub mod facts;
pub mod state;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use base::{
    build_base, delete_remote_branch, engine_worktree, merge_conflicts, movement_note,
    remote_tips, BaseOutcome,
};
pub use branch::{delete_integration_branch, engine_clone, ensure_integration_branch};
pub use facts::{
    confine_branches_to_tips, conflict_candidates, conventional_branch, detect_conflicts,
    prune_conflict_cache, NodeGit,
};
pub use state::{read_states, write_states, WorkflowState, WORKFLOW_ENGINE_KEY};

/// The `exp/*` branches one workflow's tips call covers — every branch the
/// engine can name lives under it (issue branches, batch branches, the
/// integration branch and the synthetic bases).
pub const TIPS_PATTERN: &str = "exp/*";

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
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
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
    /// EXP-984: `launch.reviewModel` — what an agent review runs on. Absent
    /// = the author's own model, unless the node is adversarial.
    #[serde(default)]
    pub review_model: Option<String>,
    /// EXP-984: `launch.model` — what the node's AUTHOR runs on. A high-risk
    /// node is never reviewed by the same model that wrote it.
    #[serde(default)]
    pub author_model: Option<String>,
    /// contract `wfStartOn` (`contract|pr_open|landed`). An absent or unknown
    /// word reads as `landed`: the conservative mode, which never starts a
    /// node on work that is not in yet.
    #[serde(default = "start_on_landed")]
    pub start_on: String,
}

fn start_on_landed() -> String {
    START_ON_LANDED.to_string()
}

impl Default for WorkflowFacts {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: String::new(),
            gate: String::new(),
            integration_branch: String::new(),
            final_pr_url: None,
            max_parallel: 0,
            start_on: start_on_landed(),
            review_model: None,
            author_model: None,
        }
    }
}

/// contract `wfStartOn` — a dependent starts once its blockers LANDED.
pub const START_ON_LANDED: &str = "landed";
/// A dependent starts once its blockers' pull requests are OPEN.
pub const START_ON_PR_OPEN: &str = "pr_open";
/// A dependent starts once its blockers announced their CONTRACT.
pub const START_ON_CONTRACT: &str = "contract";

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
    /// contract `wfRisk` — only `high` decides anything here: it makes the
    /// node's agent review ADVERSARIAL, on a model other than the author's.
    #[serde(default)]
    pub risk: String,
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
    /// EXP-983: only its PRESENCE matters — the run announced its contract
    /// (`exponential_workflows_checkpoint`), which releases its dependents
    /// under `start_on: contract`.
    #[serde(default)]
    pub checkpoint_at: Option<String>,
    /// The branch this node's run was cut from, as reported at its start.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// EXP-983: the engine's own serialization edges — nodes this one merges
    /// in first, because their work collided.
    #[serde(default)]
    pub after_node_ids: Vec<String>,
    /// The node's OWN head branch: its representative issue's synced
    /// `branch`, the launcher's conventional `exp/<IDENTIFIER>`, or a
    /// compound node's batch branch. The hosts only fill it once origin
    /// really HAS that branch ([`facts::confine_branches_to_tips`]), so
    /// `None` means nothing may base on this node yet.
    #[serde(default)]
    pub branch: Option<String>,
    /// EXP-984: how many agent reviews were SUBMITTED (the round cap).
    #[serde(default)]
    pub review_round: i64,
    /// EXP-984: the latest verdict — only the two fields the engine decides
    /// on (the findings themselves are the host's to deliver).
    #[serde(default)]
    pub review: Option<ReviewFacts>,
    /// EXP-984: what this node may spend before it is paused for a person.
    #[serde(default)]
    pub budget: Option<BudgetFacts>,
}

/// The half of `workflow_nodes.review` the engine reads.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFacts {
    /// contract `wfReviewVerdict` — `approve` / `request_changes`.
    pub verdict: String,
    /// The round it was submitted in; a verdict from an older round than the
    /// node's counter is history, not a pending instruction.
    #[serde(default)]
    pub round: i64,
}

/// `workflow_nodes.budget` — whole minutes and whole tokens, each optional.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetFacts {
    #[serde(default)]
    pub minutes: Option<i64>,
    #[serde(default)]
    pub tokens: Option<u64>,
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
    /// EXP-984: when the run started, as ms epoch — the minutes budget's
    /// clock. `None` = unknown, and then minutes bound nothing.
    #[serde(default)]
    pub started_at_ms: Option<i64>,
    /// EXP-984: the tokens the run has spent, when the host can see a
    /// counter at all. `None` = unknown, and then tokens bound nothing.
    #[serde(default)]
    pub tokens_used: Option<u64>,
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
    /// Host fact (EXP-983): `git ls-remote` for every branch this snapshot
    /// names, refreshed each beat. A branch that is absent does not exist on
    /// origin yet.
    #[serde(default)]
    pub tips: HashMap<String, String>,
    /// Host fact (EXP-983, persisted): `node id → upstream branch → the tip
    /// that node was last TOLD to merge` — what keeps one movement from
    /// being announced twice.
    #[serde(default)]
    pub propagated: HashMap<String, HashMap<String, String>>,
    /// Host fact (EXP-983, persisted): `synthetic base branch → the
    /// (branch, sha) pairs it was last built from`. A difference is a
    /// refresh.
    #[serde(default)]
    pub synthetic: HashMap<String, Vec<(String, String)>>,
    /// Host fact (EXP-983): unordered node-id pairs whose branches conflict
    /// at the CURRENT tips (`git merge-tree --write-tree`).
    #[serde(default)]
    pub conflicts: HashSet<(String, String)>,
    /// Host fact: `node id → its representative issue's identifier`, which
    /// names the node's synthetic base branch.
    #[serde(default)]
    pub identifier: HashMap<String, String>,
    /// Host fact (EXP-984): node ids whose REVIEWER run is live or starting.
    /// One review per node at a time, whatever its branch does meanwhile.
    #[serde(default)]
    pub review_in_flight: HashSet<String>,
    /// Host fact (EXP-984): `node id → the tip of its own branch` — what a
    /// review would be run against.
    #[serde(default)]
    pub pr_head: HashMap<String, String>,
    /// Host fact (EXP-984, persisted): `node id → the head the last review
    /// ran against`. A review happens once per head, never once per beat.
    #[serde(default)]
    pub reviewed_head: HashMap<String, String>,
    /// Host fact (EXP-984, persisted): `node id → the highest review round
    /// whose findings were already delivered to its author`.
    #[serde(default)]
    pub findings_sent: HashMap<String, i64>,
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
    /// EXP-983: build (or refresh) a node's SYNTHETIC base — the branch its
    /// two-or-more unlanded blockers are merged into, in the engine's own
    /// scratch worktree. `sources` are branch names, sorted.
    #[serde(rename_all = "camelCase")]
    BuildBase {
        node_id: String,
        base_branch: String,
        sources: Vec<String>,
    },
    /// Report `running` FIRST, then launch the node's run locally, cut from
    /// `base_branch` (EXP-983: the integration branch only when no blocker
    /// is still unlanded).
    #[serde(rename_all = "camelCase")]
    StartNode {
        node_id: String,
        attempt: i64,
        base_branch: String,
    },
    /// EXP-983: tell a live (or resume an ended) run that the branch it
    /// builds on moved to `sha`, so it merges it in. The host writes the
    /// note from the git range itself — zero agent tokens.
    #[serde(rename_all = "camelCase")]
    MergeUpstream {
        node_id: String,
        session_id: String,
        base_branch: String,
        sha: String,
    },
    /// EXP-983: two siblings' work collided — `node_id` merges `after` in
    /// first. `workflows.reportNode({afterNodeIds})`, a whole-array replace.
    #[serde(rename_all = "camelCase")]
    SetSerialEdge { node_id: String, after: Vec<String> },
    /// EXP-984: start the hidden `builtin:review-node` run against this
    /// node's pushed branch. `model` `None` = the device's own default;
    /// `adversarial` is a `risk: high` node, reviewed by a model that is
    /// never the author's.
    #[serde(rename_all = "camelCase")]
    StartReview {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        adversarial: bool,
    },
    /// EXP-984: hand the latest review's findings to the node's AUTHOR, once
    /// per round. `session_id` = the live run to steer; `None` = its run
    /// ended and the host resumes it with the same text.
    #[serde(rename_all = "camelCase")]
    SendFindings {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    /// EXP-984: the node spent more than its budget — end the run and report
    /// it `paused`, which notifies the workflow's creator.
    #[serde(rename_all = "camelCase")]
    PauseNode {
        node_id: String,
        session_id: String,
        note: String,
    },
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
    /// EXP-983: a synthetic base nothing is building on any more.
    #[serde(rename_all = "camelCase")]
    DeleteBase { base_branch: String },
}

/// The live states a node occupies while it holds a parallelism slot.
fn is_active(state: &str) -> bool {
    matches!(state, "running" | "waiting" | "updating")
}

/// The states a person owns: the engine never makes or unmakes them.
fn is_final(state: &str) -> bool {
    matches!(state, "landed" | "skipped")
}

/// One node's state AFTER this pass's mirror — what the later rules read.
struct Mirrored<'a> {
    node: &'a NodeFacts,
    state: String,
    /// EXP-983: the branch this node would start on, resolved while it was
    /// mirrored `ready`. `None` for every node that is not about to start.
    base: Option<String>,
}

/// EXP-983 — the prefix every synthetic base of one workflow shares. The
/// dash matters: `exp/wf-<id8>/base-…` could not exist beside the branch
/// `exp/wf-<id8>` (a git ref is a file, so it cannot also be a directory).
fn synthetic_prefix(workflow_id: &str) -> String {
    let id8: String = workflow_id.chars().take(8).collect();
    format!("exp/wf-{id8}-base-")
}

/// The synthetic base of ONE node: `exp/wf-<id8>-base-<IDENT>`.
fn synthetic_base(workflow_id: &str, identifier: &str) -> String {
    format!("{}{identifier}", synthetic_prefix(workflow_id))
}

/// EXP-984 — the LOCAL, never-pushed branch one agent review works on:
/// `exp/wf-<id8>-review-<IDENT>-r<round>`. It is cut from the node's own
/// pushed branch, and it goes with the review's worktree at the end.
pub fn review_branch(workflow_id: &str, identifier: &str, round: i64) -> String {
    let id8: String = workflow_id.chars().take(8).collect();
    format!("exp/wf-{id8}-review-{identifier}-r{round}")
}

/// Whether a branch is one of this workflow's synthetic bases.
fn is_synthetic(workflow_id: &str, branch: &str) -> bool {
    branch.starts_with(&synthetic_prefix(workflow_id))
}

/// A blocker satisfies the workflow's start mode (EXP-983 rule 1). `skipped`
/// releases under every mode: there is nothing left to wait for.
fn blocker_releases(start_on: &str, blocker: &NodeFacts) -> bool {
    if blocker.state == "skipped" || blocker.state == "landed" {
        return true;
    }
    match start_on {
        START_ON_PR_OPEN => matches!(blocker.state.as_str(), "in_review" | "updating"),
        START_ON_CONTRACT => {
            blocker.checkpoint_at.is_some()
                || matches!(blocker.state.as_str(), "in_review" | "updating")
        }
        // `landed` and anything a newer server invents: landed only.
        _ => false,
    }
}

/// A blocker whose work is not in the integration branch yet — what a
/// speculative start has to base on.
fn is_unlanded(state: &str) -> bool {
    !is_final(state)
}

/// The rule cascade, in order (the module doc names the hosts that run it):
/// 0. no integration branch → create it, and NOTHING else this pass;
/// 1. mirror every node's state off its session, PR and blockers, the
///    blockers read through the workflow's START MODE (also while `paused`);
/// 2. start `ready` nodes up to `max_parallel`, each on the base its
///    unlanded blockers dictate (building a synthetic one first);
/// 3. refresh a synthetic base whose sources moved; 4. tell a run that the
///    branch under it moved; 5. serialize two siblings whose work collided;
/// 6. nudge a run whose rate limit reset; 7. the agent review gate — ONE
///    review start per pass, and a round's findings said exactly once;
/// 8. pause a node that went over its budget; 9. land ONE cleared node in
///    TOPOLOGICAL order (the merge train); 10. open the final PR once
///    everything is in; 11. drop a synthetic base nothing builds on any more.
/// A `cancelled` workflow ends its runs and drops its branches;
/// `draft`/`done` decide nothing.
///
/// EXP-984: a node in state `proposed` is treated as ABSENT — a follow-up
/// nobody admitted is not part of the run, so it is never mirrored, started,
/// landed or waited for, and its edges do not exist.
pub fn evaluate(snapshot: &Snapshot) -> Vec<Decision> {
    if snapshot.nodes.iter().any(|node| node.state == STATE_PROPOSED) {
        return evaluate_admitted(&without_proposed(snapshot));
    }
    evaluate_admitted(snapshot)
}

/// contract `wfNodeState` — a follow-up node awaiting a person's decision.
const STATE_PROPOSED: &str = "proposed";

/// The snapshot WITHOUT the nodes nobody admitted, and without the edges
/// that name them. The hosts already drop them; this is the defensive half.
fn without_proposed(snapshot: &Snapshot) -> Snapshot {
    let mut admitted = snapshot.clone();
    admitted.nodes.retain(|node| node.state != STATE_PROPOSED);
    let known: HashSet<&str> = admitted.nodes.iter().map(|node| node.id.as_str()).collect();
    admitted
        .edges
        .retain(|(from, to)| known.contains(from.as_str()) && known.contains(to.as_str()));
    admitted
}

fn evaluate_admitted(snapshot: &Snapshot) -> Vec<Decision> {
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
        // EXP-984: a node PAUSED over its budget is settled until a person
        // retries it — the mirror would otherwise read its still-open pull
        // request and put it straight back into the run.
        if is_final(&node.state)
            || node.state == STATE_PAUSED
            || snapshot.in_flight.contains(&node.id)
        {
            mirrored.push(Mirrored {
                node,
                state: node.state.clone(),
                base: None,
            });
            continue;
        }
        let Some(desired) = desired_state(snapshot, node, &blockers) else {
            // An unknown session (the row has not synced yet): decide
            // nothing rather than failing a run that is very much alive.
            mirrored.push(Mirrored {
                node,
                state: node.state.clone(),
                base: None,
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
                    base: None,
                });
            }
            Desired::State { state, note } => {
                // EXP-983: a node is only really `ready` when the branch it
                // would be cut from exists — a blocker without a branch yet
                // leaves it `blocked` for another beat.
                let base = (state == "ready")
                    .then(|| start_base(snapshot, node, &blockers))
                    .flatten();
                let state = if state == "ready" && base.is_none() {
                    "blocked".to_string()
                } else {
                    state
                };
                if state != node.state {
                    decisions.push(Decision::SetNodeState {
                        node_id: node.id.clone(),
                        state: state.clone(),
                        note,
                    });
                }
                mirrored.push(Mirrored { node, state, base });
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
        let Some(base) = entry.base.clone() else {
            continue;
        };
        // A synthetic base has to exist before anything can be cut from it.
        if is_synthetic(&snapshot.workflow.id, &base) && !snapshot.tips.contains_key(&base) {
            decisions.push(Decision::BuildBase {
                node_id: entry.node.id.clone(),
                sources: synthetic_sources(snapshot, entry.node, &blockers),
                base_branch: base.clone(),
            });
        }
        decisions.push(Decision::StartNode {
            node_id: entry.node.id.clone(),
            attempt: entry.node.attempt + 1,
            base_branch: base,
        });
        active += 1;
    }

    // (3) A synthetic base whose sources moved is MERGED forward, never
    // recreated: a started node's history has to stay an ancestor of it.
    for entry in &mirrored {
        if is_final(&entry.state) || snapshot.in_flight.contains(&entry.node.id) {
            continue;
        }
        let Some(base) = entry.node.base_branch.as_deref() else {
            continue;
        };
        if !is_synthetic(&snapshot.workflow.id, base) {
            continue;
        }
        let sources = synthetic_sources(snapshot, entry.node, &blockers);
        let built: Vec<(String, String)> = sources
            .iter()
            .filter_map(|branch| {
                snapshot
                    .tips
                    .get(branch)
                    .map(|sha| (branch.clone(), sha.clone()))
            })
            .collect();
        if built.is_empty() || snapshot.synthetic.get(base) == Some(&built) {
            continue;
        }
        decisions.push(Decision::BuildBase {
            node_id: entry.node.id.clone(),
            base_branch: base.to_string(),
            sources,
        });
    }

    // (4) The branch a run builds on moved: it merges the movement in. ONE
    // decision per dependent per pass, and only at a turn boundary.
    for entry in &mirrored {
        if snapshot.in_flight.contains(&entry.node.id) {
            continue;
        }
        let Some(base) = entry.node.base_branch.clone() else {
            continue;
        };
        let Some(session_id) = steerable_session(snapshot, entry) else {
            continue;
        };
        let Some(sha) = snapshot.tips.get(&base) else {
            continue;
        };
        if told(snapshot, &entry.node.id, &base) == Some(sha.as_str()) {
            continue;
        }
        decisions.push(Decision::MergeUpstream {
            node_id: entry.node.id.clone(),
            session_id,
            base_branch: base,
            sha: sha.clone(),
        });
        if entry.state != "updating" {
            decisions.push(Decision::SetNodeState {
                node_id: entry.node.id.clone(),
                state: "updating".to_string(),
                note: None,
            });
        }
    }

    // (5) Two siblings whose work collides are SERIALIZED: the later one
    // merges the earlier one in and records the edge, so the train and the
    // graph both know about it.
    decisions.extend(serialization_decisions(snapshot, &mirrored));

    // (6) A wall that lifted: tell the run to carry on, exactly once.
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

    // (7) EXP-984: the agent review gate — one reviewer run per node whose
    // pull request is up, and a round's findings handed over exactly once.
    decisions.extend(review_decisions(snapshot, &mirrored));

    // (8) EXP-984: a node that spent more than it was given stops there and
    // waits for a person (`resolveNode retry` puts it back in the run).
    decisions.extend(budget_decisions(snapshot, &mirrored));

    // (9) The merge train: ONE land per pass, and only while the host is not
    // already landing one. The train is strict order among LANDABLE nodes —
    // a node still waiting for a person, for a blocker or for the sibling it
    // has to merge in never blocks a landable one behind it.
    let landing = mirrored.iter().any(|entry| {
        snapshot.in_flight.contains(&entry.node.id)
            && (entry.state == "in_review" || entry.state == "updating")
    });
    if !landing {
        let state_of: HashMap<&str, &str> = mirrored
            .iter()
            .map(|entry| (entry.node.id.as_str(), entry.state.as_str()))
            .collect();
        if let Some(entry) = mirrored.iter().find(|entry| {
            entry.state == "in_review"
                && is_cleared(&snapshot.workflow.gate, entry.node)
                && is_landable(entry.node, &blockers, &state_of)
        }) {
            decisions.push(Decision::LandNode {
                node_id: entry.node.id.clone(),
            });
        }
    }

    // (10) Everything is in: the ONE final pull request.
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

    // (11) A synthetic base nothing is building on any more: the branch goes,
    // so a repository's list stays the issues' branches plus the one
    // integration branch.
    let mut live_bases: HashSet<&str> = HashSet::new();
    for entry in &mirrored {
        if is_final(&entry.state) {
            continue;
        }
        if let Some(base) = entry.node.base_branch.as_deref() {
            live_bases.insert(base);
        }
        if let Some(base) = entry.base.as_deref() {
            live_bases.insert(base);
        }
    }
    for base in known_bases(snapshot) {
        if !live_bases.contains(base.as_str()) {
            decisions.push(Decision::DeleteBase { base_branch: base });
        }
    }

    decisions
}

/// The synthetic bases this workflow is known to have pushed: what the host
/// built, plus anything matching the prefix it can still see on origin.
fn known_bases(snapshot: &Snapshot) -> Vec<String> {
    let mut bases: Vec<String> = snapshot
        .synthetic
        .keys()
        .chain(snapshot.tips.keys())
        .filter(|branch| is_synthetic(&snapshot.workflow.id, branch))
        .cloned()
        .collect();
    bases.sort();
    bases.dedup();
    bases
}

/// The branches a node's synthetic base is built from: its still-unlanded
/// blockers' branches AND the integration branch, sorted — the same list the
/// refresh compares against what the host last built.
fn synthetic_sources(
    snapshot: &Snapshot,
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
) -> Vec<String> {
    let mut sources: Vec<String> = unlanded_blockers(snapshot, node, blockers)
        .into_iter()
        .filter_map(|blocker| blocker.branch.clone())
        .collect();
    sources.push(snapshot.workflow.integration_branch.clone());
    sources.sort();
    sources.dedup();
    sources
}

/// The blockers whose work is not in the integration branch yet.
fn unlanded_blockers<'a>(
    snapshot: &'a Snapshot,
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
) -> Vec<&'a NodeFacts> {
    blockers
        .get(node.id.as_str())
        .map(|ids| {
            let mut found: Vec<&NodeFacts> = ids
                .iter()
                .filter_map(|id| snapshot.nodes.iter().find(|node| node.id == *id))
                .filter(|blocker| is_unlanded(&blocker.state))
                .collect();
            found.sort_by(|a, b| a.id.cmp(&b.id));
            found
        })
        .unwrap_or_default()
}

/// EXP-983 rule 2 — the branch a node about to start is cut from: the
/// integration branch when every blocker is in, the ONE unlanded blocker's
/// branch, or a synthetic merge of several. `None` = it cannot start yet
/// (a blocker has no branch, or this node's identifier has not synced).
fn start_base(
    snapshot: &Snapshot,
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
) -> Option<String> {
    let unlanded = unlanded_blockers(snapshot, node, blockers);
    match unlanded.len() {
        0 => Some(snapshot.workflow.integration_branch.clone()),
        1 => unlanded[0].branch.clone(),
        _ => {
            // Every source has to be namable before the base can be built.
            if unlanded.iter().any(|blocker| blocker.branch.is_none()) {
                return None;
            }
            let identifier = snapshot.identifier.get(node.id.as_str())?;
            Some(synthetic_base(&snapshot.workflow.id, identifier))
        }
    }
}

/// The session a movement can be announced to: a LIVE run between turns, or
/// an ended one whose pull request is up (the host resumes that one). `None`
/// = say nothing this pass.
fn steerable_session(snapshot: &Snapshot, entry: &Mirrored<'_>) -> Option<String> {
    if !matches!(
        entry.state.as_str(),
        "running" | "waiting" | "in_review" | "updating"
    ) {
        return None;
    }
    let session_id = entry.node.session_id.as_deref()?;
    let session = snapshot.sessions.get(session_id)?;
    if session.live {
        // Mid-turn: the message would land inside the agent's own work.
        if session.agent_busy {
            return None;
        }
        return Some(session_id.to_string());
    }
    let pr_open = snapshot
        .issues
        .get(entry.node.issue_id.as_str())
        .and_then(|issue| issue.pr_state.as_deref())
        == Some("open");
    pr_open.then(|| session_id.to_string())
}

/// The tip a node was last TOLD to merge from one branch.
fn told<'a>(snapshot: &'a Snapshot, node_id: &str, branch: &str) -> Option<&'a str> {
    snapshot
        .propagated
        .get(node_id)?
        .get(branch)
        .map(String::as_str)
}

/// EXP-983 rule 5 — the serialization pass. A conflicting pair only counts
/// when the two share a dependent: work that never meets can differ forever.
fn serialization_decisions(snapshot: &Snapshot, mirrored: &[Mirrored<'_>]) -> Vec<Decision> {
    if snapshot.conflicts.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<&str, &Mirrored<'_>> = mirrored
        .iter()
        .map(|entry| (entry.node.id.as_str(), entry))
        .collect();
    // `node → its dependents`, to find the pairs whose work has to meet.
    let mut dependents: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (from, to) in &snapshot.edges {
        dependents
            .entry(from.as_str())
            .or_default()
            .insert(to.as_str());
    }
    // The pairs in a stable order, so one pass decides the same thing twice.
    let mut pairs: Vec<(&str, &str)> = snapshot
        .conflicts
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    pairs.sort();
    let mut decisions = Vec::new();
    let mut added: HashMap<&str, Vec<String>> = HashMap::new();
    for (left, right) in pairs {
        let (Some(left), Some(right)) = (by_id.get(left), by_id.get(right)) else {
            continue;
        };
        if is_final(&left.state) || is_final(&right.state) {
            continue;
        }
        let shared = dependents
            .get(left.node.id.as_str())
            .zip(dependents.get(right.node.id.as_str()))
            .is_some_and(|(a, b)| a.intersection(b).next().is_some());
        if !shared {
            continue;
        }
        // The LATER node merges the earlier one in: by lane, then identifier,
        // then id — the same tie-break the rest of the engine walks.
        let (earlier, later) = if order_key(snapshot, left.node) <= order_key(snapshot, right.node) {
            (left, right)
        } else {
            (right, left)
        };
        let Some(branch) = earlier.node.branch.clone() else {
            continue;
        };
        let already: HashSet<&str> = later
            .node
            .after_node_ids
            .iter()
            .map(String::as_str)
            .chain(
                added
                    .get(later.node.id.as_str())
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            )
            .collect();
        if !already.contains(earlier.node.id.as_str()) {
            let mut after: Vec<String> = already.iter().map(|id| (*id).to_string()).collect();
            after.push(earlier.node.id.clone());
            after.sort();
            added
                .entry(later.node.id.as_str())
                .or_default()
                .push(earlier.node.id.clone());
            decisions.push(Decision::SetSerialEdge {
                node_id: later.node.id.clone(),
                after,
            });
        }
        // The merge itself rides the propagation rule: once per tip, never
        // mid-turn, and the host resumes an ended run rather than steering.
        let Some(sha) = snapshot.tips.get(&branch) else {
            continue;
        };
        if told(snapshot, &later.node.id, &branch) == Some(sha.as_str()) {
            continue;
        }
        let Some(session_id) = steerable_session(snapshot, later) else {
            continue;
        };
        decisions.push(Decision::MergeUpstream {
            node_id: later.node.id.clone(),
            session_id,
            base_branch: branch,
            sha: sha.clone(),
        });
    }
    decisions
}

/// The (lane, identifier, id) tie-break rule 5 orders a colliding pair by.
fn order_key<'a>(snapshot: &'a Snapshot, node: &'a NodeFacts) -> (i64, &'a str, &'a str) {
    (
        node.lane,
        snapshot
            .identifier
            .get(node.id.as_str())
            .map(String::as_str)
            .unwrap_or(""),
        node.id.as_str(),
    )
}

/// contract `wfNodeState` — a node held over its budget.
const STATE_PAUSED: &str = "paused";
/// contract `wfGate` — every node's pull request gets an AGENT review.
const GATE_AGENT: &str = "agent";
/// contract `wfReviewVerdict`.
const VERDICT_REQUEST_CHANGES: &str = "request_changes";
/// contract `wfRisk` — the risk that makes a review adversarial.
const RISK_HIGH: &str = "high";
/// The two contract claude models an adversarial review swaps between.
const MODEL_OPUS: &str = "opus";
const MODEL_FABLE: &str = "fable";

/// EXP-984 rule 7 — the agent review gate. ONE review start per pass, in
/// (wave, lane, id) order, and the findings of a round said exactly once.
/// Nothing here decides an approval: the SERVER does, and only a passing
/// oracle on a non-contract node counts.
fn review_decisions(snapshot: &Snapshot, mirrored: &[Mirrored<'_>]) -> Vec<Decision> {
    if snapshot.workflow.gate != GATE_AGENT {
        return Vec::new();
    }
    let mut decisions = Vec::new();
    for entry in mirrored {
        let node = entry.node;
        if entry.state != "in_review" || node.approved_at.is_some() {
            continue;
        }
        if node.review_round >= domain::contract::WORKFLOW_MAX_REVIEW_ROUNDS as i64 {
            continue; // it stopped bouncing; a person owns it now
        }
        if snapshot.review_in_flight.contains(&node.id) {
            continue;
        }
        // The author has not seen (or not finished with) the last round's
        // findings: reviewing again now would review the same code twice.
        if findings_pending(snapshot, node) {
            continue;
        }
        let Some(head) = snapshot.pr_head.get(&node.id) else {
            continue; // nothing pushed to review
        };
        if snapshot.reviewed_head.get(&node.id) == Some(head) {
            continue;
        }
        let adversarial = node.risk == RISK_HIGH;
        decisions.push(Decision::StartReview {
            node_id: node.id.clone(),
            model: review_model(snapshot, adversarial),
            adversarial,
        });
        break;
    }
    for entry in mirrored {
        let node = entry.node;
        // Deliberately NOT keyed on the node's state: the server sets
        // `updating`, and this pass's mirror may already have read the still
        // open pull request and put it back to `in_review`.
        let Some(review) = node.review.as_ref() else {
            continue;
        };
        if review.verdict != VERDICT_REQUEST_CHANGES || review.round != node.review_round {
            continue;
        }
        if findings_delivered(snapshot, node, review.round) {
            continue;
        }
        let Some(session_id) = node.session_id.as_deref() else {
            continue;
        };
        let live = snapshot
            .sessions
            .get(session_id)
            .is_some_and(|session| session.live);
        if live
            && snapshot
                .sessions
                .get(session_id)
                .is_some_and(|session| session.agent_busy)
        {
            continue; // mid-turn: the text would land inside its own work
        }
        decisions.push(Decision::SendFindings {
            node_id: node.id.clone(),
            session_id: live.then(|| session_id.to_string()),
        });
    }
    decisions
}

/// Whether the node's CURRENT round still owes its author something: the
/// findings were never delivered, or the run is mid-turn acting on them.
fn findings_pending(snapshot: &Snapshot, node: &NodeFacts) -> bool {
    let Some(review) = node.review.as_ref() else {
        return false;
    };
    if review.verdict != VERDICT_REQUEST_CHANGES || review.round != node.review_round {
        return false;
    }
    if !findings_delivered(snapshot, node, review.round) {
        return true;
    }
    node.session_id
        .as_deref()
        .and_then(|session_id| snapshot.sessions.get(session_id))
        .is_some_and(|session| session.live && session.agent_busy)
}

fn findings_delivered(snapshot: &Snapshot, node: &NodeFacts, round: i64) -> bool {
    snapshot
        .findings_sent
        .get(&node.id)
        .is_some_and(|sent| *sent >= round)
}

/// The model a review runs on: the workflow's pin, else the author's own.
/// An ADVERSARIAL review must never be the author's model, so an equal pick
/// swaps deterministically (`opus` ↔ `fable`; anything else → `opus`).
fn review_model(snapshot: &Snapshot, adversarial: bool) -> Option<String> {
    let author = snapshot.workflow.author_model.as_deref();
    let picked = snapshot.workflow.review_model.as_deref().or(author);
    if !adversarial || picked != author {
        return picked.map(str::to_string);
    }
    Some(
        match author {
            Some(MODEL_OPUS) => MODEL_FABLE,
            Some(MODEL_FABLE) => MODEL_OPUS,
            _ => MODEL_OPUS,
        }
        .to_string(),
    )
}

/// EXP-984 rule 8 — the budget. A live run that spent more minutes (or more
/// tokens, where the host can count them) than the node was given is ended
/// and the node parked for a person.
fn budget_decisions(snapshot: &Snapshot, mirrored: &[Mirrored<'_>]) -> Vec<Decision> {
    let mut decisions = Vec::new();
    for entry in mirrored {
        let node = entry.node;
        if !matches!(entry.state.as_str(), "running" | "updating") {
            continue;
        }
        if snapshot.in_flight.contains(&node.id) {
            continue;
        }
        let Some(budget) = node.budget.as_ref() else {
            continue;
        };
        let Some(session_id) = node.session_id.as_deref() else {
            continue;
        };
        let Some(session) = snapshot.sessions.get(session_id) else {
            continue;
        };
        if !session.live {
            continue;
        }
        let Some(note) = over_budget(budget, session, snapshot.now_ms) else {
            continue;
        };
        decisions.push(Decision::PauseNode {
            node_id: node.id.clone(),
            session_id: session_id.to_string(),
            note,
        });
    }
    decisions
}

/// The note a run that went over its budget carries, or `None` while it is
/// still inside it. Minutes are read first: they always apply, while tokens
/// only do where the host can see a counter at all.
fn over_budget(budget: &BudgetFacts, session: &SessionFacts, now_ms: i64) -> Option<String> {
    if let (Some(limit), Some(started_at)) = (budget.minutes, session.started_at_ms) {
        let minutes = (now_ms - started_at) / 60_000;
        if limit > 0 && minutes > limit {
            return Some(format!("Over budget: {minutes} of {limit} minutes"));
        }
    }
    if let (Some(limit), Some(used)) = (budget.tokens, session.tokens_used) {
        if limit > 0 && used > limit {
            return Some(format!("Over budget: {used} of {limit} tokens"));
        }
    }
    None
}

/// EXP-983 rule 6 — the train's topological gate: a node lands only once
/// every blocker AND every node it has to merge in first is in.
fn is_landable(
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
    state_of: &HashMap<&str, &str>,
) -> bool {
    let settled = |id: &str| {
        state_of
            .get(id)
            // A node outside the workflow cannot be waited on.
            .map_or(true, |state| is_final(state))
    };
    blockers
        .get(node.id.as_str())
        .map_or(true, |ids| ids.iter().all(|id| settled(id)))
        && node.after_node_ids.iter().all(|id| settled(id))
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
    if decisions.is_empty() {
        // EXP-983: the synthetic bases go with it — a cancelled workflow
        // leaves no branch of its own behind.
        for base in known_bases(snapshot) {
            decisions.push(Decision::DeleteBase { base_branch: base });
        }
        if snapshot.integration_branch_exists {
            decisions.push(Decision::DeleteIntegrationBranch);
        }
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
        // Not started yet: the blockers decide, read through the workflow's
        // START MODE (EXP-983 — `landed` waits for the merge, `pr_open` for
        // the pull request, `contract` for the announcement).
        let ready = blockers
            .get(node.id.as_str())
            .map(|ids| {
                ids.iter().all(|id| {
                    snapshot
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == *id)
                        .is_some_and(|candidate| {
                            blocker_releases(&snapshot.workflow.start_on, candidate)
                        })
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
                start_on: START_ON_LANDED.to_string(),
                review_model: None,
                author_model: None,
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

    /// EXP-984 — a review branch is this workflow's, names its node's issue
    /// and its round, and can never collide with the synthetic bases (which
    /// carry `-base-`).
    #[test]
    fn a_review_branch_names_the_workflow_the_issue_and_the_round() {
        assert_eq!(
            review_branch("abcdef12-3456-7890-abcd-ef1234567890", "EXP-42", 2),
            "exp/wf-abcdef12-review-EXP-42-r2"
        );
        assert!(!is_synthetic(
            "abcdef12-3456-7890-abcd-ef1234567890",
            &review_branch("abcdef12-3456-7890-abcd-ef1234567890", "EXP-42", 2)
        ));
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
