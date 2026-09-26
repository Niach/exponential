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
//! The engine runs each node exactly once (Kahn from the roots), lands each
//! node's pull request into the integration branch once its run ended green
//! (the merge train, in topological order), reviews the landed result in
//! WAVES, then opens ONE final PR integration → default.
//!
//! EXP-983 makes the starts SPECULATIVE: a dependent may start before its
//! blockers landed. It then bases on THEIR work — one unlanded blocker means
//! that blocker's branch, several mean a synthetic base the host merges them
//! into — and two siblings whose work collides get a SERIALIZATION edge.
//!
//! EXP-1106 keeps the agents ASLEEP: upstream movement propagates by a
//! MECHANICAL merge the host makes in its own worktree (never a rebase, never
//! a force-push), debounced so several moves become one, and only into a
//! branch whose run has ENDED — a live run works against the tip it started
//! on ("pull, not push": it asks for a newer upstream with
//! `exponential_workflows_request_upstream` and merges it itself). A clean
//! merge wakes nobody; only a CONFLICT wakes the run, once per tip, resumed
//! while its cache is warm ([`IDLE_RESUME_MS`]) and started FRESH with a brief
//! after that. Wakes of any kind go out [`MAX_WAKES_PER_PASS`] at a time.
//!
//! EXP-1103: reviews happen in WAVES over the LANDED result, not per node.
//! By default ONE wave at the end; a graph deeper than
//! [`REVIEW_WAVE_DEEP_DEPTH`] layers adds a wave after the contract layer and
//! every [`REVIEW_WAVE_STRIDE`]th layer after it, and later layers neither
//! start nor land before it cleared. A wave = one reviewer per landed node's
//! diff (in parallel, blind to the author's reasoning), then ONE fix run on
//! the integration branch for everything requested, then the wave clears
//! (`approved_at` stamped on its nodes by the server). No loops: what the fix
//! run leaves open is carried into the final pull request, the one place a
//! person reviews. Nobody ever waits for a person inside a run, and
//! `waiting` is never written: every hold is the node's own state plus a
//! note. A follow-up node nobody admitted (`proposed`) is absent from the run.
//!
//! The rule order in [`evaluate`] IS the contract, and it is fixture-tested
//! as DATA: `crates/coding/tests/fixtures/workflows/*.json`, each
//! `{name, snapshot, expected}`, replayed by `tests/workflow_engine.rs`.

pub mod base;
pub mod branch;
// EXP-1082: the host's audit-trail sink (`workflow_events`).
pub mod events;
pub mod facts;
// EXP-1029: the two-model launch every node run and review reads from.
pub mod launch;
pub mod state;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use base::{
    build_base, delete_remote_branch, engine_worktree, fetch_origin,
    is_ancestor as git_is_ancestor, land_branch, merge_conflicts, merge_into_branch,
    movement_note, remote_tips, upstream_note, BaseOutcome, MergeOutcome,
};
pub use branch::{delete_integration_branch, engine_clone, ensure_integration_branch};
pub use facts::{
    confine_branches_to_tips, conflict_candidates, conventional_branch, detect_base_conflicts,
    detect_conflicts, prune_conflict_cache, refresh_merged, stamp_tips_seen, NodeGit,
};
pub use state::{
    conflict_key, final_pr_close_handled, hold_resume, merge_resuming, merge_settled,
    pending_launches, prune_launched, release_resume, WorkflowState, WorkflowStore,
    WORKFLOW_ENGINE_KEY,
};

/// The `exp/*` branches one workflow's tips call covers — every branch the
/// engine can name lives under it (issue branches, batch branches, the
/// integration branch, the synthetic bases and the fix branches).
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
    let mut options = crate::automations::launch_options(settings, agent, model, effort, None);
    if let Some(subagent_model) = subagent_model.filter(|value| !value.is_empty()) {
        options.subagent_model = subagent_model.to_string();
    }
    if let Some(account) = account.filter(|value| !value.is_empty()) {
        options.account = Some(account.to_string());
    }
    options
}

/// The note a node carries while its agent is rate limited (EXP-1065: a
/// hold keeps the node's own state — `waiting` is never written — and says
/// why here).
pub const NOTE_RATE_LIMITED: &str = "Waiting for the account's reset";
/// The two hold notes engines before EXP-1065 wrote next to `waiting`;
/// recognised so the mirror clears them once the hold is over.
const LEGACY_HOLD_NOTES: [&str; 2] = ["Needs an answer", "Rate limited"];
/// The note a node carries when its run ended with nothing to review.
pub const NOTE_NO_PULL_REQUEST: &str = "The run ended without a pull request";
/// EXP-1007: the note a node carries when its start never came up on the
/// runner twice (the host reported `running`, no run ever named itself).
pub const NOTE_START_NEVER_CAME_UP: &str = "The run never came up on the runner";
/// The note a node carries while its run merges the integration branch in
/// after GitHub refused to land its pull request (the host's own note, the
/// refusal's reason, wins when it is already on the row).
pub const NOTE_MERGE_REFUSED: &str = "Merging the integration branch in";
/// The note a node carries when its reviewer runs kept ending with no verdict.
pub const NOTE_REVIEW_NO_VERDICT: &str = "The review runs ended without a verdict";
/// EXP-1103: the prefix of the note an unstarted node carries while a review
/// wave before its layer has not cleared; the layer number (1-based) follows.
pub const NOTE_WAVE_GATE_PREFIX: &str = "Waiting for the review wave after layer ";
/// How long a `running` node with no session yet is the host's in-flight
/// start before the engine reads it as a start that never came up (a host
/// that died between its `running` report and its `session_id` report).
pub const START_GRACE_MS: i64 = 10 * 60_000;

/// EXP-1010: how long a failed launch rests before its one free retry.
pub const RETRY_BACKOFF_MS: i64 = 2 * 60_000;
/// How long a session the host is RESUMING reads as live for the engine: the
/// interval between the resume and the moment the node names the new run
/// (the server re-points `session_id` on `codingSessions.start`).
pub const RESUME_GRACE_MS: i64 = 5 * 60_000;
/// How many reviewer runs may end without a verdict before the node stops
/// re-launching them and says so in its note.
pub const MAX_REVIEW_RUN_FAILURES: i64 = 3;
/// What a rate-limited run is told once its window reset.
pub const NUDGE_RATE_LIMIT_RESET: &str =
    "The rate limit has reset. Continue where you left off.";

/// EXP-1106 rule 4: a base whose tip moved is merged only once it stood still
/// this long — several moves inside the window become ONE merge (or one
/// wake). Three beats.
pub const UPSTREAM_DEBOUNCE_MS: i64 = 90_000;
/// EXP-1106 rule 3: an ended run is RESUMED (its prompt cache is warm) when
/// it ended less than this long ago; later it is started FRESH with a brief
/// the host writes, instead of reloading a long transcript uncached.
pub const IDLE_RESUME_MS: i64 = 5 * 60_000;
/// EXP-1106 rule 4: how many agent wakes (a nudge, a conflict, a refused
/// land) one pass may send — the rest wait for the next beat, so a host
/// restart or a limit reset never wakes every run in the same second.
pub const MAX_WAKES_PER_PASS: usize = 2;
/// EXP-1106 rule 2: the `Nudge` key of the one-time notice a run gets once
/// its first dependent started on its work.
pub const NUDGE_KEY_FREEZE: &str = "freeze";
/// What a run is told once its first dependent started on its branch.
pub const NUDGE_FREEZE: &str = "A dependent node started on your branch: your contract is FROZEN \
for it. Finish without changing what you announced; if you must change it, say exactly what in \
your summary. Before you push, run git pull --no-rebase (the scheduler may have merged upstream \
into your branch while you were idle).";
/// EXP-1103: a graph with MORE than this many layers gets review waves
/// between layers; up to it, one wave at the end only.
pub const REVIEW_WAVE_DEEP_DEPTH: i64 = 3;
/// EXP-1103: in a deep graph, a wave follows the contract layer (layer 0)
/// and every this-many-th layer after it.
pub const REVIEW_WAVE_STRIDE: i64 = 3;

/// The workflow row, as plain data.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFacts {
    pub id: String,
    /// contract `wfStatus` — a raw wire word (an unknown one decides nothing).
    pub status: String,
    pub integration_branch: String,
    #[serde(default)]
    pub final_pr_url: Option<String>,
    /// EXP-1059: contract `prState` of the final PR (`open`/`closed`/
    /// `merged`), `None` while there is none. `closed` = someone closed it
    /// WITHOUT merging: the engine reopens it once (rule 9).
    #[serde(default)]
    pub final_pr_state: Option<String>,
    /// Contract `workflow.maxParallelDefault` — how many node runs may be
    /// live at once. Not a launch field any more (EXP-1029): the hosts fill
    /// it from the contract.
    pub max_parallel: usize,
    /// EXP-1029: THE launch, normalized — the agent, the optional account and
    /// the two models every node run and every agent review read
    /// ([`launch::model_for_node`] / [`launch::review_model_for`]). Built by
    /// the hosts from the stored jsonb; a fixture may write the raw jsonb
    /// here and it normalizes the same way.
    #[serde(default, deserialize_with = "deserialize_launch")]
    pub launch: launch::WorkflowLaunch,
}

/// The stored `launch` jsonb of ANY vintage → the strict launch.
fn deserialize_launch<'de, D>(deserializer: D) -> Result<launch::WorkflowLaunch, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(launch::normalize_workflow_launch(&raw))
}

impl Default for WorkflowFacts {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: String::new(),
            integration_branch: String::new(),
            final_pr_url: None,
            final_pr_state: None,
            max_parallel: 0,
            launch: launch::WorkflowLaunch::default(),
        }
    }
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
    /// Only its PRESENCE matters — EXP-1103: the review WAVE that covered
    /// this landed node cleared (the server stamps it on `clearReviewWave`);
    /// a pre-wave approval reads the same way.
    #[serde(default)]
    pub approved_at: Option<String>,
    /// EXP-983: only its PRESENCE matters — the run announced its contract
    /// (`exponential_workflows_checkpoint`), which releases its dependents
    /// (EXP-1066: the one start rule).
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
    /// EXP-984: how many agent reviews were SUBMITTED for this node.
    #[serde(default)]
    pub review_round: i64,
    /// The latest verdict. EXP-1103: on a LANDED node it is the review wave's
    /// verdict on this node's diff; `Some` = this node was reviewed in the
    /// wave, and [`changes_requested`] decides whether the fix run gets it.
    #[serde(default)]
    pub review: Option<ReviewFacts>,
    /// The row's `updated_at` as ms epoch — a `running` node with no session
    /// is the host's in-flight start for [`START_GRACE_MS`] after it. `None`
    /// = unknown, which holds.
    #[serde(default)]
    pub updated_at_ms: Option<i64>,
    /// EXP-1071: the row's current note — the mirror writes a hold note only
    /// when it changes, and clears its OWN notes once the hold is over.
    #[serde(default)]
    pub note: Option<String>,
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
    /// The commit the reviewer judged (`git rev-parse HEAD` in its worktree).
    /// An approval of a head that is not the pull request's CURRENT head is
    /// stale: it clears nothing, and the new head gets its own review.
    #[serde(default)]
    pub head: Option<String>,
    /// EXP-1065: the reviewer's own checks. A FAILED oracle under an
    /// `approve` reads exactly like `request_changes` ([`changes_requested`]).
    #[serde(default)]
    pub oracle: Option<OracleFacts>,
}

/// The half of `workflow_nodes.review.oracle` the engine reads.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OracleFacts {
    #[serde(default)]
    pub passed: Option<bool>,
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
    /// EXP-1106: when an ENDED run ended (`ended_at`, else `updated_at`), as
    /// ms epoch — what decides between resuming it warm and starting fresh.
    /// `None` on a live row, or unknown, which reads as long ago.
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
}

/// EXP-1103: a review wave's FIX run, found by its branch
/// (`exp/wf-<id8>-fix-w<wave>`) on the synced `coding_sessions` rows.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FixRunFacts {
    pub wave: i64,
    pub session_id: String,
    /// Status `running` or `in_review` — still up.
    #[serde(default)]
    pub live: bool,
    /// Host fact: what the run pushed is IN the integration branch already
    /// (its branch tip is an ancestor of the integration tip, or it pushed
    /// nothing and its branch is not up at all). Nothing left to land.
    #[serde(default)]
    pub landed: bool,
}

/// EXP-1103: `runs` (the fix runs found by branch on the synced rows) plus
/// the ones this host launched whose row has not synced yet — `stored` =
/// `WorkflowState::fix_runs`, `pending` = [`pending_launches`]. A pending
/// launch reads LIVE with nothing landed, and replaces the wave's synced
/// entry (an earlier run of the same wave that ended).
pub fn with_pending_fix_runs(
    mut runs: Vec<FixRunFacts>,
    stored: &HashMap<i64, String>,
    pending: &HashSet<String>,
) -> Vec<FixRunFacts> {
    for (wave, session_id) in stored {
        if !pending.contains(session_id) {
            continue;
        }
        runs.retain(|fix| fix.wave != *wave);
        runs.push(FixRunFacts {
            wave: *wave,
            session_id: session_id.clone(),
            live: true,
            landed: false,
        });
    }
    runs.sort_by_key(|fix| fix.wave);
    runs
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
    /// Host fact (EXP-1059, persisted `WorkflowState::final_pr_close_handled`):
    /// the CURRENT closed episode of the final PR was already put to the
    /// server (`workflows.reopenFinalPr` answered). The host clears it once
    /// the PR reads `open` again, so EVERY close reaches the server once.
    #[serde(default)]
    pub final_pr_close_handled: bool,
    /// Host fact (persisted): `(session id, key)` pairs already nudged — a
    /// reset stamp for a rate-limit nudge, [`NUDGE_KEY_FREEZE`] for the
    /// freeze notice.
    #[serde(default)]
    pub nudged: HashSet<(String, String)>,
    /// Host fact (EXP-983): `git ls-remote` for every branch this snapshot
    /// names, refreshed each beat. A branch that is absent does not exist on
    /// origin yet.
    #[serde(default)]
    pub tips: HashMap<String, String>,
    /// Host fact (EXP-1106): `node id → base branch → the tip of it that is
    /// IN the node's branch` — merged by the host or cut at it, re-derived
    /// by ancestry when the store was lost. A base whose current tip is not
    /// here has moved.
    #[serde(default)]
    pub merged: HashMap<String, HashMap<String, String>>,
    /// Host fact (EXP-1106, persisted): `node id → base branch → the tip the
    /// run was already WOKEN for` because merging it conflicts — one
    /// collision wakes the agent once.
    #[serde(default)]
    pub woken: HashMap<String, HashMap<String, String>>,
    /// Host fact (EXP-1106): `(node id, upstream branch)` pairs whose
    /// branches do NOT merge cleanly at the upstream's current tip (`git
    /// merge-tree`). Such a move wakes the run instead of being merged by the
    /// host. FEED-54: keyed per upstream, so a collision with a serialized
    /// sibling never flags the base.
    #[serde(default)]
    pub base_conflicts: HashSet<(String, String)>,
    /// Host fact (FEED-54): `(node id, upstream branch)` pairs whose current
    /// upstream tip git says is NOT in the node's branch
    /// ([`facts::refresh_merged`]). Only such a pair has moved: a pair git
    /// could not answer, or one the host never tested, is not movement.
    #[serde(default)]
    pub behind: HashSet<(String, String)>,
    /// Host fact (EXP-1106): `branch → ms epoch the host first saw its
    /// CURRENT tip` — the debounce clock. An absent branch reads as seen
    /// just now, which holds one window.
    #[serde(default)]
    pub tip_seen_ms: HashMap<String, i64>,
    /// Host fact (EXP-1103): the review waves' fix runs, found by branch.
    #[serde(default)]
    pub fix_runs: Vec<FixRunFacts>,
    /// Host fact (EXP-984): node ids whose reviewer runs kept ending without
    /// a verdict past [`MAX_REVIEW_RUN_FAILURES`] — a wave treats them as
    /// reviewed with nothing to fix, rather than waiting for ever.
    #[serde(default)]
    pub review_gave_up: HashSet<String>,
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
    /// One review per node at a time.
    #[serde(default)]
    pub review_in_flight: HashSet<String>,
    /// Host fact (EXP-984): `node id → the tip of its own branch` — what a
    /// land refusal is keyed on and what the contract-change metric reads.
    #[serde(default)]
    pub pr_head: HashMap<String, String>,
    /// Host fact (persisted): `node id → the head of its pull request when
    /// GitHub last REFUSED to merge it`. While the head has not moved the
    /// node stays `updating` — its run is merging the trunk in — and the
    /// train does not ask GitHub again.
    #[serde(default)]
    pub land_refused: HashMap<String, String>,
    pub now_ms: i64,
}

/// EXP-1082 — contract `wfSessionRole`: what a workflow run is FOR,
/// stamped onto `coding_sessions.workflow_role` at start.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum WfSessionRole {
    Author,
    Review,
    BaseMerge,
    Plan,
    Replan,
}

impl WfSessionRole {
    /// The contract wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            WfSessionRole::Author => "author",
            WfSessionRole::Review => "review",
            WfSessionRole::BaseMerge => "base_merge",
            WfSessionRole::Plan => "plan",
            WfSessionRole::Replan => "replan",
        }
    }
    /// The inverse of [`Self::as_str`]; `None` for a value this build does
    /// not know.
    pub fn parse(value: &str) -> Option<Self> {
        [
            WfSessionRole::Author,
            WfSessionRole::Review,
            WfSessionRole::BaseMerge,
            WfSessionRole::Plan,
            WfSessionRole::Replan,
        ]
        .into_iter()
        .find(|role| role.as_str() == value)
    }
}

/// EXP-1082 — the workflow membership a launch carries into its session row
/// (`LaunchOptions::workflow` → `codingSessions.start({workflowId,
/// workflowNodeId, workflowRole})`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowMembership {
    pub workflow_id: String,
    /// `None` for a workflow-level run with no node: the `plan` / `replan`
    /// planner runs of a draft (the Plan-workflow frame names the workflow
    /// and the role only). Node runs always carry one.
    pub node_id: Option<String>,
    pub role: WfSessionRole,
}

impl WorkflowMembership {
    /// Decode the optional wire keys (the relay's `start_session` frame): a
    /// membership needs the workflow id and a known role; the node is
    /// optional (a planner run has none). An unknown role drops it.
    pub fn from_wire(
        workflow_id: Option<&str>,
        node_id: Option<&str>,
        role: Option<&str>,
    ) -> Option<Self> {
        let role = WfSessionRole::parse(role?)?;
        Some(Self {
            workflow_id: workflow_id.filter(|id| !id.is_empty())?.to_string(),
            node_id: node_id.filter(|id| !id.is_empty()).map(str::to_string),
            role,
        })
    }

    /// The `codingSessions.start` keys; a `None` node stays off the wire.
    pub fn wire(&self) -> api::coding_sessions::WorkflowStart<'_> {
        api::coding_sessions::WorkflowStart {
            workflow_id: Some(&self.workflow_id),
            workflow_node_id: self.node_id.as_deref(),
            workflow_role: Some(self.role.as_str()),
        }
    }
}

/// EXP-1082 — the ONE glue both hosts (`ui::workflow_host`, the CLI daemon)
/// run before an ENGINE start (a node's author run, a review, a wave's fix
/// run): stamp the decision's membership onto the launch and the decision's
/// own account, when it names one. The account PICK itself happens exactly
/// once, inside `coding::prepare` (EXP-1005: every launch on the device,
/// engine starts included, off the usage cache).
///
/// Starts only: a RESUME keeps its RECORDED account (EXP-906) and never comes
/// through here; moving a run off a spent login is the mid-run switch
/// (EXP-1005's `pick_rotation_target`), not a start pick.
pub fn apply_engine_start(
    options: &mut crate::LaunchOptions,
    membership: WorkflowMembership,
    decision_account: Option<String>,
) {
    options.workflow = Some(membership);
    if let Some(account) = decision_account {
        options.account = Some(account);
    }
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
        /// EXP-1082: the workflow this run belongs to, stamped onto its
        /// `coding_sessions` row (`workflow_id`) at start.
        workflow_id: String,
        /// EXP-1082: contract `wfSessionRole` — what the run is FOR.
        role: WfSessionRole,
        /// EXP-1082 (EXP-1005 fills it): the agent profile the host starts
        /// on, from `account_rotation::pick_start_account`; `None` = the
        /// device's own default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// Report `running` FIRST, then launch the node's run locally, cut from
    /// `base_branch` (EXP-983: the integration branch only when no blocker
    /// is still unlanded).
    #[serde(rename_all = "camelCase")]
    StartNode {
        node_id: String,
        attempt: i64,
        base_branch: String,
        /// EXP-1029: the model this node's run spawns on ([`node_model`]) —
        /// the launch's STRONG model for a `contract`/`integration` node and
        /// for any `risk: high` one, else its cheap `model`. Always `Some`
        /// (the launch always names both); an absent one would mean the
        /// device's own default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// EXP-1082: the workflow this run belongs to, stamped onto its
        /// `coding_sessions` row (`workflow_id`) at start.
        workflow_id: String,
        /// EXP-1082: contract `wfSessionRole` — what the run is FOR.
        role: WfSessionRole,
        /// EXP-1082 (EXP-1005 fills it): the agent profile the host starts
        /// on, from `account_rotation::pick_start_account`; `None` = the
        /// device's own default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// EXP-1106 rule 1: the branch a node builds on moved and its run is not
    /// live — the HOST merges `origin/<base_branch>` at `sha` into the node's
    /// own `branch` in its scratch worktree and pushes. Zero agent tokens; a
    /// clean merge wakes nobody (the host pre-tested it: a conflict is a
    /// [`Self::WakeRun`] instead).
    #[serde(rename_all = "camelCase")]
    MergeBase {
        node_id: String,
        branch: String,
        base_branch: String,
        sha: String,
    },
    /// EXP-1106 rule 3: wake a node's ENDED (or idle) run for something only
    /// its agent can do. `mode` says how: steer the live run where it stands,
    /// resume the ended one while its cache is warm, or start a FRESH run of
    /// the node with a brief the host writes (the diff so far, the reason,
    /// the node's notes) instead of reloading a long transcript.
    #[serde(rename_all = "camelCase")]
    WakeRun {
        node_id: String,
        session_id: String,
        mode: WakeMode,
        reason: WakeReason,
    },
    /// EXP-983: two siblings' work collided — `node_id` merges `after` in
    /// first. `workflows.reportNode({afterNodeIds})`, a whole-array replace;
    /// `state` = the node's state AFTER this pass's mirror (the report always
    /// takes one, and the pre-pass row may already be behind).
    #[serde(rename_all = "camelCase")]
    SetSerialEdge {
        node_id: String,
        state: String,
        after: Vec<String>,
    },
    /// EXP-1103: start the hidden `builtin:review-node` run against ONE
    /// landed node's diff on the integration branch, as part of review
    /// `wave`. EXP-1029: `model` is the launch's STRONG model for EVERY
    /// review ([`review_model`]); `adversarial` only flags the node's own
    /// `risk: high` to the review PROMPT.
    #[serde(rename_all = "camelCase")]
    StartReview {
        node_id: String,
        wave: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        adversarial: bool,
        /// EXP-1082: the workflow this run belongs to, stamped onto its
        /// `coding_sessions` row (`workflow_id`) at start.
        workflow_id: String,
        /// EXP-1082: contract `wfSessionRole` — what the run is FOR.
        role: WfSessionRole,
        /// EXP-1082 (EXP-1005 fills it): the agent profile the host starts
        /// on; `None` = the device's own default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// EXP-1103: every reviewer of `wave` answered and some asked for
    /// changes — start the ONE fix run of the wave (the hidden
    /// `builtin:fix-review-findings`) on `branch`, cut from the integration
    /// branch, with every requesting node's findings. `node_ids` = the nodes
    /// whose findings it carries, in landing order.
    #[serde(rename_all = "camelCase")]
    StartFix {
        wave: i64,
        node_ids: Vec<String>,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        workflow_id: String,
        role: WfSessionRole,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// EXP-1103: the wave's fix run ended — fast-forward the integration
    /// branch to what it pushed on `branch` (the host, mechanically), then
    /// drop the branch.
    #[serde(rename_all = "camelCase")]
    LandFix { wave: i64, branch: String },
    /// EXP-1103: the wave is done — `workflows.clearReviewWave` stamps
    /// `approved_at` on `node_ids` (its landed, still unstamped nodes), which
    /// releases the layers behind it and, for the last wave, the final PR.
    #[serde(rename_all = "camelCase")]
    ClearWave { wave: i64, node_ids: Vec<String> },
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
    /// EXP-1059: `workflows.reopenFinalPr` — the final PR was closed without
    /// merging. The server reopens it ONCE and records a later close as a
    /// person's decision; once it ANSWERED, the host remembers
    /// `final_pr_close_handled` for this closed episode.
    ReopenFinalPr,
    /// EXP-1059: `workflows.cancelUnshipped` — every node was skipped, so
    /// nothing reached the integration branch and there is no final PR to
    /// open: the workflow ends `cancelled` with the `nothing shipped` note.
    CancelUnshipped,
    /// Cancelled: end a live run.
    #[serde(rename_all = "camelCase")]
    KillSession { session_id: String },
    /// Cancelled and quiet: `git push origin --delete <integration_branch>`.
    DeleteIntegrationBranch,
    /// EXP-983: a synthetic base nothing is building on any more.
    #[serde(rename_all = "camelCase")]
    DeleteBase { base_branch: String },
}

impl Decision {
    /// EXP-1106: whether executing this decision spends an agent turn — the
    /// wakes the per-pass budget ([`MAX_WAKES_PER_PASS`]) and the
    /// `agentWakes` metric count.
    pub fn is_wake(&self) -> bool {
        matches!(self, Decision::WakeRun { .. } | Decision::Nudge { .. })
    }
}

/// EXP-1106 rule 3 — how a run is woken.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WakeMode {
    /// The run is live and idle: the text lands as its next message.
    Steer,
    /// The run ended less than [`IDLE_RESUME_MS`] ago: re-enter it (warm).
    Resume,
    /// The run ended longer ago: a FRESH run of the node with a brief.
    Fresh,
}

/// EXP-1106 — why a run is woken.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WakeReason {
    /// Merging `base_branch` at `sha` into the node's branch conflicts.
    #[serde(rename_all = "camelCase")]
    UpstreamConflict { base_branch: String, sha: String },
}

/// EXP-1106 rule 3 — the mode a run is woken in, off its synced row. `None`
/// = not now: a live run mid-turn or parked on a question (the text would
/// land inside its work, or become the answer).
pub fn wake_mode(session: &SessionFacts, now_ms: i64) -> Option<WakeMode> {
    if session.live {
        return (!session.agent_busy && !session.needs_input).then_some(WakeMode::Steer);
    }
    Some(match session.ended_at_ms {
        Some(ended_at) if now_ms - ended_at < IDLE_RESUME_MS => WakeMode::Resume,
        _ => WakeMode::Fresh,
    })
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
    format!("{}{round}", review_branch_prefix(workflow_id, identifier))
}

/// Every review of one node runs on a branch under this prefix,
/// `exp/wf-<id8>-review-<IDENT>-r` — the round follows, so `EXP-10`'s
/// prefix never matches `EXP-103`'s branches.
pub fn review_branch_prefix(workflow_id: &str, identifier: &str) -> String {
    let id8: String = workflow_id.chars().take(8).collect();
    format!("exp/wf-{id8}-review-{identifier}-r")
}

/// The round a review branch of `identifier`'s node carries, `None` for any
/// other branch (another node, another workflow, a synthetic base, a prefix
/// with no round).
pub fn review_branch_round(workflow_id: &str, identifier: &str, branch: &str) -> Option<i64> {
    let rest = branch.strip_prefix(review_branch_prefix(workflow_id, identifier).as_str())?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// The LIVE reviewer run of every node, found by its BRANCH rather than by
/// the session id the host recorded: `node id → session id`. A resume — a
/// person's "Switch account", the Resume button — ends the recorded run and
/// starts another on the SAME review branch, and a host that only knew the
/// recorded id read that end as a review that never submitted, released the
/// head and started a third reviewer beside the resumed one (workflow
/// 3b828f50: five live reviews of one node). `live_rows` = this team's live
/// `(session id, branch)` rows OLDEST FIRST; the newest match wins.
///
/// Only the reviewer of each node's PENDING round counts: a review launches
/// on `-r<review_round + 1>` and the server moves `review_round` up to that
/// number when its verdict lands, so a live run on `-r<review_round>` or
/// lower already submitted and is only lingering. Taking one in as the
/// current reviewer held the node in `review_in_flight` and, once it exited,
/// counted its end as a review that never submitted. Such a stray is
/// ignored, never ended: it is finishing on its own.
pub fn live_pending_reviews_on_branches(
    workflow_id: &str,
    identifier: &HashMap<String, String>,
    review_round_of: &HashMap<String, i64>,
    live_rows: impl IntoIterator<Item = (String, String)>,
) -> HashMap<String, String> {
    let mut found = HashMap::new();
    for (session_id, branch) in live_rows {
        let matched = identifier.iter().find_map(|(node_id, ident)| {
            let round = review_branch_round(workflow_id, ident, &branch)?;
            let pending = review_round_of.get(node_id)? + 1;
            (round == pending).then_some(node_id)
        });
        if let Some(node_id) = matched {
            found.insert(node_id.clone(), session_id);
        }
    }
    found
}

/// Whether a branch is one of this workflow's synthetic bases.
fn is_synthetic(workflow_id: &str, branch: &str) -> bool {
    branch.starts_with(&synthetic_prefix(workflow_id))
}

/// EXP-1103 — the LOCAL, never-pushed-by-the-agent branch one review wave's
/// fix run works on: `exp/wf-<id8>-fix-w<wave>`, cut from the integration
/// branch; the host fast-forwards the integration branch to it at the end.
pub fn fix_branch(workflow_id: &str, wave: i64) -> String {
    format!("{}{wave}", fix_branch_prefix(workflow_id))
}

fn fix_branch_prefix(workflow_id: &str) -> String {
    let id8: String = workflow_id.chars().take(8).collect();
    format!("exp/wf-{id8}-fix-w")
}

/// The wave a fix branch of this workflow belongs to; `None` for any other
/// branch.
pub fn fix_branch_wave(workflow_id: &str, branch: &str) -> Option<i64> {
    let rest = branch.strip_prefix(fix_branch_prefix(workflow_id).as_str())?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// EXP-1103 — the layers a review wave follows, for a graph `depth` layers
/// deep (layers are the nodes' `wave` indexes, 0-based): always the last
/// one; a graph deeper than [`REVIEW_WAVE_DEEP_DEPTH`] also reviews after the
/// contract layer and after every [`REVIEW_WAVE_STRIDE`]th layer after it.
/// Empty for no layers at all.
pub fn review_points(depth: i64) -> Vec<i64> {
    if depth <= 0 {
        return Vec::new();
    }
    let last = depth - 1;
    let mut points = Vec::new();
    if depth > REVIEW_WAVE_DEEP_DEPTH {
        let mut layer = 0;
        while layer < last {
            points.push(layer);
            layer += REVIEW_WAVE_STRIDE;
        }
    }
    points.push(last);
    points
}

/// One review wave as this pass sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WaveView {
    /// The layer it follows (a review point).
    index: i64,
    /// Every node up to and including this layer is final: the wave is DUE.
    settled: bool,
    /// The landed nodes this wave covers (layers after the previous point up
    /// to this one), in landing order.
    members: Vec<String>,
    /// Settled, and every member carries `approved_at`: nothing left to do.
    cleared: bool,
}

/// The waves of this snapshot, off the mirrored states.
fn wave_views(snapshot: &Snapshot, state_of: &HashMap<&str, &str>) -> Vec<WaveView> {
    let depth = snapshot.nodes.iter().map(|node| node.wave + 1).max().unwrap_or(0);
    let mut previous = -1;
    let mut views = Vec::new();
    for point in review_points(depth) {
        let mut settled = true;
        let mut members = Vec::new();
        let mut cleared = true;
        for node in ordered_nodes(snapshot) {
            let state = state_of.get(node.id.as_str()).copied().unwrap_or(node.state.as_str());
            if node.wave <= point && !is_final(state) {
                settled = false;
            }
            if node.wave > previous && node.wave <= point && state == "landed" {
                if node.approved_at.is_none() {
                    cleared = false;
                }
                members.push(node.id.clone());
            }
        }
        views.push(WaveView {
            index: point,
            settled,
            members,
            cleared: settled && cleared,
        });
        previous = point;
    }
    views
}

/// The first review point BEFORE `layer` that has not cleared: the wave a
/// node of that layer waits for (to start and to land). `None` = free.
fn wave_gate(views: &[WaveView], layer: i64) -> Option<i64> {
    views
        .iter()
        .find(|view| view.index < layer && !view.cleared)
        .map(|view| view.index)
}

fn wave_gate_note(point: i64) -> String {
    format!("{NOTE_WAVE_GATE_PREFIX}{}", point + 1)
}

/// EXP-1103 rule 7 — the review waves. For the one wave that is due and not
/// cleared: reviewers for its members that have no verdict yet (up to
/// `max_parallel` starts per pass, in landing order); once every member
/// answered, the fix run for whatever was requested (started, then landed
/// when it ended), then the wave clears. One round, no loops.
fn wave_decisions(snapshot: &Snapshot, views: &[WaveView]) -> Vec<Decision> {
    let mut decisions = Vec::new();
    let Some(view) = views.iter().find(|view| view.settled && !view.cleared) else {
        return decisions;
    };
    let node = |id: &str| snapshot.nodes.iter().find(|node| node.id == id);
    let answered = |id: &str| {
        node(id).is_some_and(|node| {
            node.approved_at.is_some()
                || node.review.is_some()
                || snapshot.review_gave_up.contains(&node.id)
        })
    };
    let mut started = 0;
    for id in &view.members {
        if answered(id) || snapshot.review_in_flight.contains(id) {
            continue;
        }
        if started >= snapshot.workflow.max_parallel {
            break;
        }
        let Some(member) = node(id) else {
            continue;
        };
        decisions.push(Decision::StartReview {
            node_id: id.clone(),
            wave: view.index,
            model: review_model(snapshot),
            adversarial: member.risk == RISK_HIGH,
            workflow_id: snapshot.workflow.id.clone(),
            role: WfSessionRole::Review,
            account: None,
        });
        started += 1;
    }
    if started > 0 || !view.members.iter().all(|id| answered(id) || snapshot.review_in_flight.contains(id)) {
        return decisions;
    }
    if view.members.iter().any(|id| snapshot.review_in_flight.contains(id) && !answered(id)) {
        return decisions;
    }
    // Every member answered. What still asks for changes goes to ONE fix run.
    let requested: Vec<String> = view
        .members
        .iter()
        .filter(|id| {
            node(id).is_some_and(|node| {
                node.approved_at.is_none()
                    && node.review.as_ref().is_some_and(changes_requested)
            })
        })
        .cloned()
        .collect();
    let unstamped: Vec<String> = view
        .members
        .iter()
        .filter(|id| node(id).is_some_and(|node| node.approved_at.is_none()))
        .cloned()
        .collect();
    if requested.is_empty() {
        decisions.push(Decision::ClearWave {
            wave: view.index,
            node_ids: unstamped,
        });
        return decisions;
    }
    let branch = fix_branch(&snapshot.workflow.id, view.index);
    match snapshot.fix_runs.iter().find(|fix| fix.wave == view.index) {
        None => decisions.push(Decision::StartFix {
            wave: view.index,
            node_ids: requested,
            branch,
            model: review_model(snapshot),
            workflow_id: snapshot.workflow.id.clone(),
            role: WfSessionRole::Review,
            account: None,
        }),
        Some(fix) if fix.live => {}
        Some(fix) if !fix.landed => decisions.push(Decision::LandFix {
            wave: view.index,
            branch,
        }),
        Some(_) => decisions.push(Decision::ClearWave {
            wave: view.index,
            node_ids: unstamped,
        }),
    }
    decisions
}

/// A blocker releases its dependents (EXP-983 rule 1) once it announced its
/// CONTRACT, put its pull request up, landed or was skipped. EXP-1066: this
/// is the ONE start rule (the old start modes are gone): dependents always
/// start on the blockers' contract. A checkpoint counts only while the
/// blocker is AT WORK: a retried or failed blocker's old announcement
/// releases nothing (the server nulls it on retry too).
fn blocker_releases(blocker: &NodeFacts) -> bool {
    match blocker.state.as_str() {
        "in_review" | "updating" | "landed" | "skipped" => true,
        "running" | "waiting" => blocker.checkpoint_at.is_some(),
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
/// 1. mirror every node's state off its session, PR, blockers (a blocker
///    releasing on its contract, its PR or its landing) and the review waves
///    before its layer (also while `paused`);
/// 2. start `ready` nodes up to `max_parallel`, each on the base its
///    unlanded blockers dictate (building a synthetic one first);
/// 3. refresh a synthetic base whose sources moved;
/// 4. a base that moved under an ENDED run is merged in by the host —
///    debounced, once per tip; a conflict wakes the run instead, once;
/// 5. serialize two siblings whose work collided (the later merges the
///    earlier in, mechanically too);
/// 6. nudge a run whose rate limit reset, and tell a run ONCE that its first
///    dependent started on its work;
/// 7. the review waves: reviewers, the fix run, the clearance;
/// 8. land ONE node whose run ended with its pull request up, in TOPOLOGICAL
///    order and behind every wave before its layer (the merge train);
/// 9. open the final PR once everything is in and every wave cleared (or
///    reopen a final PR closed without merging, once; or cancel a workflow
///    whose every node was skipped);
/// 10. drop a synthetic base nothing builds on any more.
/// Last, the wake BUDGET: at most [`MAX_WAKES_PER_PASS`] of the pass's
/// nudges and wakes go out; the rest are re-decided next beat.
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
    // EXP-1103: the wave gate an UNSTARTED node reads is the waves as the
    // rows stand (a wave cannot clear inside the mirror).
    let row_states: HashMap<&str, &str> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.state.as_str()))
        .collect();
    let row_waves = wave_views(snapshot, &row_states);

    // (1) The mirror. A node the host is busy with, and a node a person
    // already resolved, are both left exactly where they are.
    let mut mirrored: Vec<Mirrored<'_>> = Vec::with_capacity(order.len());
    for node in &order {
        if is_final(&node.state) || snapshot.in_flight.contains(&node.id) {
            mirrored.push(Mirrored {
                node,
                state: node.state.clone(),
                base: None,
            });
            continue;
        }
        let Some(desired) = desired_state(snapshot, node, &blockers, &row_waves) else {
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
                // EXP-1071: ONE write per real change. A hold note is written
                // when it appears or changes, cleared once the hold is over
                // (only the engine's own notes: a review note the server
                // wrote stays with an unchanged state), and a state change
                // always writes.
                let note_changed = match note.as_deref() {
                    Some(next) if is_engine_hold_note(next) => node.note.as_deref() != Some(next),
                    Some(_) => false,
                    None => node.note.as_deref().is_some_and(is_engine_hold_note),
                };
                if state != node.state || note_changed {
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
                workflow_id: snapshot.workflow.id.clone(),
                role: WfSessionRole::BaseMerge,
                account: None,
            });
        }
        decisions.push(Decision::StartNode {
            node_id: entry.node.id.clone(),
            attempt: entry.node.attempt + 1,
            base_branch: base,
            model: node_model(&snapshot.workflow, &entry.node.kind, &entry.node.risk),
            workflow_id: snapshot.workflow.id.clone(),
            role: WfSessionRole::Author,
            account: None,
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
        // EXP-1071: while two of its sources collide the base cannot be
        // rebuilt — the mirror carries the reason on the node; rule 5
        // serializes the pair. Asking every beat was the flap.
        if conflicting_blockers(snapshot, entry.node, &blockers).is_some() {
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
            workflow_id: snapshot.workflow.id.clone(),
            role: WfSessionRole::BaseMerge,
            account: None,
        });
    }

    // (4) EXP-1106: the branch a node builds on moved. Merged by the HOST
    // into the node's branch — once the tip stood still for the debounce
    // window and only while the run is not live (a live run pulls itself).
    // ONE decision per node per pass.
    for entry in &mirrored {
        if is_final(&entry.state) || snapshot.in_flight.contains(&entry.node.id) {
            continue;
        }
        let Some(base) = entry.node.base_branch.clone() else {
            continue;
        };
        decisions.extend(upstream_decision(snapshot, entry, &base));
    }

    // (5) Two siblings whose work collides are SERIALIZED: the later one
    // merges the earlier one in and records the edge, so the train and the
    // graph both know about it.
    decisions.extend(serialization_decisions(snapshot, &mirrored));

    // (6) A wall that lifted: tell the run to carry on, exactly once. Keyed
    // on the RUN (EXP-1065: a walled node keeps its own state). And the
    // freeze notice: a run whose first dependent started on its work hears
    // it once.
    for entry in &mirrored {
        if is_final(&entry.state) {
            continue;
        }
        let Some(session_id) = entry.node.session_id.as_deref() else {
            continue;
        };
        let Some(session) = snapshot.sessions.get(session_id) else {
            continue;
        };
        if !session.live || session.needs_input {
            continue;
        }
        if session.blocked {
            let Some(resets_at) = session.blocked_resets_at_ms else {
                continue;
            };
            if resets_at > snapshot.now_ms {
                continue;
            }
            let key = resets_at.to_string();
            if snapshot.nudged.contains(&(session_id.to_string(), key.clone())) {
                continue;
            }
            decisions.push(Decision::Nudge {
                session_id: session_id.to_string(),
                key,
                text: NUDGE_RATE_LIMIT_RESET.to_string(),
            });
            continue;
        }
        if session.agent_busy {
            continue;
        }
        let dependents_started = snapshot
            .edges
            .iter()
            .filter(|(from, _)| from == &entry.node.id)
            .any(|(_, to)| {
                snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == *to)
                    .is_some_and(|node| node.session_id.is_some() || is_final(&node.state))
            });
        if !dependents_started
            || snapshot
                .nudged
                .contains(&(session_id.to_string(), NUDGE_KEY_FREEZE.to_string()))
        {
            continue;
        }
        decisions.push(Decision::Nudge {
            session_id: session_id.to_string(),
            key: NUDGE_KEY_FREEZE.to_string(),
            text: NUDGE_FREEZE.to_string(),
        });
    }

    // (7) EXP-1103: the review waves, off the states after this mirror.
    let state_of: HashMap<&str, &str> = mirrored
        .iter()
        .map(|entry| (entry.node.id.as_str(), entry.state.as_str()))
        .collect();
    let views = wave_views(snapshot, &state_of);
    decisions.extend(wave_decisions(snapshot, &views));

    // (8) The merge train: ONE land per pass, and only while the host is not
    // already landing one. The train is strict order among LANDABLE nodes —
    // a node still waiting for its run, for a blocker, for the sibling it
    // has to merge in or for a review wave never blocks a landable one
    // behind it. A land the mirror already asked for (a pull request merged
    // behind our back) IS this pass's land.
    let landing = decisions
        .iter()
        .any(|decision| matches!(decision, Decision::LandNode { .. }))
        || mirrored.iter().any(|entry| {
            snapshot.in_flight.contains(&entry.node.id)
                && (entry.state == "in_review" || entry.state == "updating")
        });
    if !landing {
        if let Some(entry) = mirrored.iter().find(|entry| {
            entry.state == "in_review"
                && is_cleared(snapshot, entry.node)
                && wave_gate(&views, entry.node.wave).is_none()
                && is_landable(entry.node, &blockers, &state_of)
        }) {
            decisions.push(Decision::LandNode {
                node_id: entry.node.id.clone(),
            });
        }
    }

    // (9) Everything is in and reviewed: the ONE final pull request.
    let all_in = !mirrored.is_empty()
        && mirrored.iter().all(|entry| is_final(&entry.state));
    let any_landed = mirrored.iter().any(|entry| entry.state == "landed");
    if all_in && !snapshot.final_pr_in_flight {
        if !any_landed {
            // EXP-1059: every node skipped — nothing shipped, nothing to
            // review. The server ends the workflow `cancelled` with its
            // note; the next pass then sweeps the branch like any cancel.
            decisions.push(Decision::CancelUnshipped);
        } else if snapshot.workflow.final_pr_url.is_none() {
            // EXP-1103: not before the last review wave cleared.
            if views.iter().all(|view| view.cleared) {
                decisions.push(Decision::OpenFinalPr);
            }
        } else if snapshot.workflow.final_pr_state.as_deref() == Some("closed")
            && !snapshot.final_pr_close_handled
        {
            // EXP-1059: closed without merging — every close goes to the
            // server once: the first is reopened, a later one is recorded
            // as a person's decision and the workflow waits for a member.
            decisions.push(Decision::ReopenFinalPr);
        }
    }

    // (10) A synthetic base nothing is building on any more: the branch goes,
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

    budget_wakes(decisions)
}

/// EXP-1106 rule 4 — the wake budget: the first [`MAX_WAKES_PER_PASS`]
/// wakes of a pass stay, the rest go (the same snapshot re-decides them
/// next beat, staggered).
fn budget_wakes(decisions: Vec<Decision>) -> Vec<Decision> {
    let mut wakes = 0;
    decisions
        .into_iter()
        .filter(|decision| {
            if !decision.is_wake() {
                return true;
            }
            wakes += 1;
            wakes <= MAX_WAKES_PER_PASS
        })
        .collect()
}

/// EXP-1106 rule 4 for ONE node and ONE upstream branch (its base, or a
/// serialized sibling's branch): the host's mechanical merge, or the one
/// wake a conflict costs. `None` = nothing this pass.
fn upstream_decision(snapshot: &Snapshot, entry: &Mirrored<'_>, base: &str) -> Option<Decision> {
    let node = entry.node;
    let branch = node.branch.clone()?;
    let sha = snapshot.tips.get(base)?;
    if snapshot
        .merged
        .get(node.id.as_str())
        .and_then(|branches| branches.get(base))
        == Some(sha)
    {
        return None;
    }
    // FEED-54: only a tip git PROVED missing from the branch has moved. A
    // `merged` entry that is absent (never recorded, store lost, ancestry
    // unanswered) is not movement: reading it as one woke runs with an
    // "Upstream moved" notice about their own work.
    let key = (node.id.clone(), base.to_string());
    if !snapshot.behind.contains(&key) {
        return None;
    }
    // Debounced: a tip that moved inside the window is still moving.
    let seen = snapshot.tip_seen_ms.get(base).copied().unwrap_or(snapshot.now_ms);
    if snapshot.now_ms - seen < UPSTREAM_DEBOUNCE_MS {
        return None;
    }
    // A live run is never touched: it works against the tip it started on
    // and pulls a newer one itself when it asks for it.
    let session_id = node.session_id.as_deref()?;
    let session = snapshot.sessions.get(session_id)?;
    if session.live {
        return None;
    }
    if !snapshot.base_conflicts.contains(&key) {
        return Some(Decision::MergeBase {
            node_id: node.id.clone(),
            branch,
            base_branch: base.to_string(),
            sha: sha.clone(),
        });
    }
    if snapshot
        .woken
        .get(node.id.as_str())
        .and_then(|branches| branches.get(base))
        == Some(sha)
    {
        return None;
    }
    let mode = wake_mode(session, snapshot.now_ms)?;
    Some(Decision::WakeRun {
        node_id: node.id.clone(),
        session_id: session_id.to_string(),
        mode,
        reason: WakeReason::UpstreamConflict {
            base_branch: base.to_string(),
            sha: sha.clone(),
        },
    })
}

/// EXP-983 rule 5 — the serialization pass. A conflicting pair only counts
/// when the two share a dependent: work that never meets can differ forever.
fn serialization_decisions(snapshot: &Snapshot, mirrored: &[Mirrored<'_>]) -> Vec<Decision> {
    if snapshot.conflicts.is_empty() {
        return Vec::new();
    }
    let blockers = blockers_by_node(snapshot);
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
        // A pair the DAG already orders needs no edge: the dependent merges
        // its blocker in through rule 4 anyway, and an edge the OTHER way
        // round would make the two wait for each other for ever.
        if dag_related(&blockers, left.node.id.as_str(), right.node.id.as_str()) {
            continue;
        }
        // The LATER node merges the earlier one in: by wave, then lane, then
        // identifier, then id — the landing order the rest of the engine
        // walks, so the edge always points the way the train runs.
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
                state: later.state.clone(),
                after,
            });
        }
        // The merge itself rides rule 4: mechanical, debounced, once per
        // tip, never into a live run.
        if snapshot.in_flight.contains(&later.node.id) {
            continue;
        }
        decisions.extend(upstream_decision(snapshot, later, &branch));
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

/// The (wave, lane, identifier, id) tie-break rule 5 orders a colliding pair
/// by — the landing order first, so a serialization edge never points
/// against the train.
fn order_key<'a>(snapshot: &'a Snapshot, node: &'a NodeFacts) -> (i64, i64, &'a str, &'a str) {
    (
        node.wave,
        node.lane,
        snapshot
            .identifier
            .get(node.id.as_str())
            .map(String::as_str)
            .unwrap_or(""),
        node.id.as_str(),
    )
}

/// Whether one node is an ancestor of the other through `blocks` edges (the
/// serialization edges included): the DAG already orders such a pair.
fn dag_related(blockers: &HashMap<&str, Vec<&str>>, left: &str, right: &str) -> bool {
    is_ancestor(blockers, left, right) || is_ancestor(blockers, right, left)
}

/// `ancestor` blocks `node`, directly or through other nodes.
fn is_ancestor(blockers: &HashMap<&str, Vec<&str>>, ancestor: &str, node: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut frontier: Vec<&str> = vec![node];
    while let Some(current) = frontier.pop() {
        for blocker in blockers.get(current).into_iter().flatten() {
            if *blocker == ancestor {
                return true;
            }
            if seen.insert(blocker) {
                frontier.push(blocker);
            }
        }
    }
    false
}

/// contract `wfReviewVerdict`.
const VERDICT_REQUEST_CHANGES: &str = "request_changes";
/// contract `wfRisk` — the risk a node's own author called hard. It runs on
/// the strong model, and its review is ADVERSARIAL (said so in the prompt).
const RISK_HIGH: &str = "high";

/// EXP-1029: the model a node's run spawns on — the ONE rule, shared with
/// web `lib/workflow-launch.ts`.
pub fn node_model(workflow: &WorkflowFacts, kind: &str, risk: &str) -> Option<String> {
    Some(launch::model_for_node(&workflow.launch, kind, risk))
}

/// EXP-1065: a verdict that asks for work — `request_changes`, or an
/// `approve` the reviewer's own checks contradict (a FAILED oracle), which
/// the server treats the same way (`reviewOutcome`).
pub fn changes_requested(review: &ReviewFacts) -> bool {
    review.verdict == VERDICT_REQUEST_CHANGES
        || review
            .oracle
            .as_ref()
            .is_some_and(|oracle| oracle.passed == Some(false))
}

/// EXP-1029: EVERY agent review (and EXP-1103: the wave's fix run) runs on
/// the launch's STRONG model, whatever the node — the strong model is the
/// capable one, and a review is the one thing always worth it. The
/// `adversarial` flag still rides along to the prompt: it is the node's own
/// `risk: high`, not a model choice.
fn review_model(snapshot: &Snapshot) -> Option<String> {
    Some(launch::review_model_for(&snapshot.workflow.launch))
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

/// EXP-1103: a node is cleared for the train once its run ENDED with its
/// pull request up — no review gates a node any more (the waves review the
/// landed result). A run this device has not synced reads as still up.
fn is_cleared(snapshot: &Snapshot, node: &NodeFacts) -> bool {
    match node.session_id.as_deref() {
        Some(session_id) => snapshot
            .sessions
            .get(session_id)
            .is_some_and(|session| !session.live),
        // No run at all (a person opened the pull request by hand): nothing
        // to wait for.
        None => true,
    }
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
    waves: &[WaveView],
) -> Option<Desired> {
    let Some(session_id) = node.session_id.as_deref() else {
        // The host reported `running` and is still bringing the run up (its
        // `session_id` report lands a moment later): an in-flight start,
        // not a node to start again — for a bounded while, so a host that
        // died in between is recovered rather than waited on for ever.
        if node.state == "running" && start_in_grace(snapshot, node) {
            return Some(state("running"));
        }
        // EXP-1007: a start that never produced a run gets ONE free retry,
        // like a run that ended with nothing to review — `attempt` counts
        // the host's `running` reports, so the retry is the second. Past
        // that the node stops and says so, instead of a fresh start every
        // grace (each wiping the previous failure's note) until the
        // server's attempt bound rejects the report and it stalls mute. A
        // person's `retry` resets the count.
        if node.state == "running" && node.attempt > 1 {
            return Some(state_with_note("failed", NOTE_START_NEVER_CAME_UP));
        }
        if node.state == "failed" && node.attempt > 1 {
            // Its note (the host's launch error, or the one above) stays:
            // the mirror never rewrites an unchanged state.
            return Some(state("failed"));
        }
        // EXP-1010: the free retry waits out what made the launch fail. Two
        // starts a beat apart both meet the same transient error (a dropped
        // connection, a rate limit) and leave the node to a person for
        // nothing. An unknown stamp does not hold (the hosts always fill it).
        if node.state == "failed" && retry_in_backoff(snapshot, node) {
            return Some(state("failed"));
        }
        // Not started yet: the blockers decide (EXP-983 — a blocker releases
        // on its contract announcement, its pull request or its landing).
        let ready = blockers
            .get(node.id.as_str())
            .map(|ids| {
                ids.iter().all(|id| {
                    snapshot
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == *id)
                        .is_some_and(blocker_releases)
                })
            })
            .unwrap_or(true);
        if !ready {
            return Some(state("blocked"));
        }
        // EXP-1103: a deep graph reviews between layers, and a layer behind
        // a wave that has not cleared does not start on the unreviewed
        // result. Says why, once.
        if let Some(point) = wave_gate(waves, node.wave) {
            return Some(state_with_note("blocked", &wave_gate_note(point)));
        }
        // Two of its unlanded blockers collide at the current tips: the base
        // it would be cut from cannot be built until one merges the other in
        // (rule 5 arranges that), so it waits here, and says why, instead of
        // flipping ready ↔ waiting on every beat.
        if let Some((left, right)) = conflicting_blockers(snapshot, node, blockers) {
            // EXP-1065: not started = `blocked`, with the reason on the row.
            return Some(state_with_note(
                "blocked",
                &conflicting_blockers_note(snapshot, &left, &right),
            ));
        }
        return Some(state("ready"));
    };
    let pr_state = snapshot
        .issues
        .get(node.issue_id.as_str())
        .and_then(|issue| issue.pr_state.as_deref());
    // EXP-1007: a merged pull request lands the node WHATEVER its run is
    // doing. A merge does not always end the run (the team's
    // `endSessionsOnMerge` off, `pr_merge({endSessions: false})`, a run that
    // merged its OWN pull request), and a node that waited for the run to end
    // first sat `running` / `waiting` for ever with its code already in. The
    // server's `landNode` reads the merged state before its gate, and a
    // landed node is final, so the mirror never looks at it again.
    //
    // EXP-1032: asked BEFORE the session row, which this device may never
    // have synced (a run started on another machine, a row pruned since).
    // That used to leave the pass undecided for ever — the node sat
    // `running` with its code already merged, every other node landed, and
    // the workflow never reached its final pull request.
    if pr_state == Some(domain::contract::PR_STATE_MERGED) {
        return Some(Desired::Land);
    }
    let session = snapshot.sessions.get(session_id)?;
    // GitHub refused to land this head and the run is merging the trunk in:
    // the node stays `updating` until its pull request MOVES (live or not —
    // the host's resume of an ended run is what moves it), and the train
    // does not ask GitHub the same question every beat meanwhile.
    if pr_state == Some("open") {
        if let Some(refused) = snapshot.land_refused.get(node.id.as_str()) {
            let moved = snapshot
                .pr_head
                .get(node.id.as_str())
                .is_some_and(|current| current != refused);
            // A beat that cannot see the remote reads as "not moved yet".
            if !moved {
                return Some(state_with_note("updating", NOTE_MERGE_REFUSED));
            }
        }
    }
    // EXP-1065/EXP-1071: a STARTED node whose unlanded blockers collide holds
    // its own state and says why — the same note every beat, so the row is
    // written once (the base build that found the conflict used to write
    // `waiting`, and the mirror put the state back the next beat).
    let conflict = conflicting_blockers(snapshot, node, blockers)
        .map(|(left, right)| conflicting_blockers_note(snapshot, &left, &right));
    let hold = |name: &str, note: Option<String>| -> Desired {
        Desired::State {
            state: name.to_string(),
            note,
        }
    };
    if session.live {
        // A node merging the trunk in stays `updating` for as long as the
        // agent is actually working on it.
        if node.state == "updating" && session.agent_busy {
            return Some(hold("updating", conflict));
        }
        if pr_state == Some("open") {
            return Some(hold("in_review", conflict));
        }
        // An open question (`needs_input`) changes NOTHING here: the badge a
        // person sees comes off the run's own `pending_question`.
        if session.blocked {
            // EXP-1065: a wall keeps the node `running`; the note says why.
            return Some(hold(
                "running",
                conflict.or_else(|| Some(NOTE_RATE_LIMITED.to_string())),
            ));
        }
        return Some(hold("running", conflict));
    }
    match pr_state {
        Some("open") => Some(hold("in_review", conflict)),
        // The run ended with nothing to review: one free retry, then the
        // node fails and says so. `attempt` counts the starts so far (the
        // first run is attempt 1), so the retry is the second start.
        _ if node.attempt <= 1 => Some(state("ready")),
        _ => Some(state_with_note("failed", NOTE_NO_PULL_REQUEST)),
    }
}

/// EXP-1071: a note the ENGINE itself put on a node while it held — the
/// only notes the mirror may clear again (a server-written review note is
/// never touched while the state stands).
fn is_engine_hold_note(note: &str) -> bool {
    note == NOTE_RATE_LIMITED
        || LEGACY_HOLD_NOTES.contains(&note)
        || note.starts_with(NOTE_WAVE_GATE_PREFIX)
        || (note.starts_with("Its blockers ") && note.ends_with("one has to merge the other in"))
        || (note.starts_with("Its base ") && note.contains("could not be built"))
}

/// A launch that just failed is not retried for [`RETRY_BACKOFF_MS`] after
/// the row was last written (the failure report itself).
fn retry_in_backoff(snapshot: &Snapshot, node: &NodeFacts) -> bool {
    node.updated_at_ms
        .is_some_and(|updated_at| snapshot.now_ms - updated_at < RETRY_BACKOFF_MS)
}

/// A `running` node with no session yet is the host's own start for
/// [`START_GRACE_MS`] after the row was last written. An unknown stamp holds
/// (the hosts always fill it).
fn start_in_grace(snapshot: &Snapshot, node: &NodeFacts) -> bool {
    node.updated_at_ms
        .map_or(true, |updated_at| snapshot.now_ms - updated_at < START_GRACE_MS)
}

/// The first pair of this node's unlanded blockers that collide at the
/// current tips (a `conflicts` pair), sorted — the pair a synthetic base
/// build would refuse. `None` for a node with fewer than two, or with none
/// colliding.
fn conflicting_blockers(
    snapshot: &Snapshot,
    node: &NodeFacts,
    blockers: &HashMap<&str, Vec<&str>>,
) -> Option<(String, String)> {
    let unlanded = unlanded_blockers(snapshot, node, blockers);
    if unlanded.len() < 2 {
        return None;
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (index, left) in unlanded.iter().enumerate() {
        for right in unlanded.iter().skip(index + 1) {
            let pair = if left.id <= right.id {
                (left.id.clone(), right.id.clone())
            } else {
                (right.id.clone(), left.id.clone())
            };
            if snapshot.conflicts.contains(&pair) {
                pairs.push(pair);
            }
        }
    }
    pairs.sort();
    pairs.into_iter().next()
}

/// Why a node waits when two of its blockers collide — byte-identical to the
/// note the hosts write when the base build itself finds the conflict.
fn conflicting_blockers_note(snapshot: &Snapshot, left: &str, right: &str) -> String {
    let name = |node_id: &str| {
        snapshot
            .identifier
            .get(node_id)
            .cloned()
            .unwrap_or_else(|| node_id.to_string())
    };
    format!(
        "Its blockers {} and {} conflict; one has to merge the other in",
        name(left),
        name(right)
    )
}

/// Tolerant timestamp parse for the synced rows' `updated_at`/`started_at`:
/// Electric forwards Postgres `timestamptz` text (`2026-07-03 10:11:12.345+00`,
/// space separator, short offset), tRPC echoes RFC 3339; both read, an
/// offset-less form as UTC. `None` = not a timestamp.
pub fn parse_wire_timestamp_ms(raw: &str) -> Option<i64> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.timestamp_millis());
    }
    for format in [
        "%Y-%m-%d %H:%M:%S%.f%#z",
        "%Y-%m-%dT%H:%M:%S%.f%#z",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(parsed) = chrono::DateTime::parse_from_str(raw, format) {
            return Some(parsed.timestamp_millis());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Some(parsed.and_utc().timestamp_millis());
        }
    }
    None
}

/// Host bookkeeping, shared by both hosts: a session the host is RESUMING
/// (`state.resuming`, `session id → when`) reads as LIVE and MID-TURN for
/// [`RESUME_GRACE_MS`] — the interval between the resume and the moment the
/// node names the new run — so the ended row it still points at is neither
/// resumed twice nor told anything, and the node holds its state. Entries
/// past the grace, or no node points at any more, are dropped.
///
/// A hold on a REVIEWER run (`review_runs`, a person's resume of a review)
/// is kept the same way but marks nothing: its liveness is settled by
/// [`settle_review_runs`], which reads the hold there.
pub fn apply_resuming(
    snapshot: &mut Snapshot,
    resuming: &mut HashMap<String, i64>,
    review_runs: &HashMap<String, String>,
) {
    let named: HashSet<&str> = snapshot
        .nodes
        .iter()
        .filter_map(|node| node.session_id.as_deref())
        .collect();
    let reviewing: HashSet<&str> = review_runs.values().map(String::as_str).collect();
    let now_ms = snapshot.now_ms;
    resuming.retain(|session_id, at| {
        (named.contains(session_id.as_str()) || reviewing.contains(session_id.as_str()))
            && now_ms - *at < RESUME_GRACE_MS
    });
    for session_id in resuming.keys() {
        if !named.contains(session_id.as_str()) {
            continue; // a reviewer: settle_review_runs reads the hold
        }
        // Live and mid-turn, nothing else: the ended row's clock and wall
        // belong to the run that ended, not to the one coming up.
        snapshot.sessions.insert(
            session_id.clone(),
            SessionFacts {
                live: true,
                agent_busy: true,
                ..SessionFacts::default()
            },
        );
    }
}

/// Host bookkeeping: a refusal is forgotten once the pull request's head
/// MOVED past it (an unknown head this beat keeps it).
pub fn prune_land_refused(refused: &mut HashMap<String, String>, pr_head: &HashMap<String, String>) {
    refused.retain(|node_id, head| pr_head.get(node_id).map_or(true, |current| current == head));
}

/// What [`settle_review_runs`] found for one node's reviewer run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewRunEnd {
    /// The round advanced: a verdict landed. Nothing to do.
    Verdict { node_id: String },
    /// No verdict; the node is reviewed again next pass.
    Retry { node_id: String, failures: i64 },
    /// No verdict for the last time: the node counts as reviewed with
    /// nothing to fix (`Snapshot::review_gave_up`) and should say so
    /// (`note`).
    GaveUp { node_id: String, note: String },
    /// The recorded run ended but a run on the node's review branch is
    /// live — its resume. The node's record now names that run; nothing
    /// was counted.
    Followed { node_id: String, session_id: String },
    /// A live run on the node's review branch nobody recorded (a resume
    /// whose predecessor was already settled, a host restart). Recorded at
    /// the node's current round, so its end is settled like any other; the
    /// host hands in only the PENDING round's live reviewers
    /// ([`live_pending_reviews_on_branches`]), an older round's stray is
    /// never adopted.
    Adopted { node_id: String, session_id: String },
}

/// Host bookkeeping (EXP-984), shared by both hosts: every reviewer run this
/// host recorded (`state.review_runs`, with the node's round at its launch in
/// `state.review_rounds`) whose session ENDED is settled. A round that
/// advanced meant a verdict; one that did not means the reviewer never
/// submitted (a wall, a crash), so the review runs again, at most
/// [`MAX_REVIEW_RUN_FAILURES`] times per node.
/// `session_live(id)` = `Some(live)` off the synced row, `None` while the row
/// has not synced (left alone).
///
/// A reviewer that was RESUMED is not an ended review: `live_on_branch`
/// ([`live_pending_reviews_on_branches`]) names the live run on every node's review
/// branch, and an ended record with one is re-pointed at it (`Followed`).
/// Between a person's resume and the successor's sync the record is under a
/// hold (`state.resuming`, [`RESUME_GRACE_MS`]) and waits. A live reviewer
/// no record names is taken in (`Adopted`).
pub fn settle_review_runs(
    state: &mut WorkflowState,
    review_round_of: &HashMap<String, i64>,
    live_on_branch: &HashMap<String, String>,
    session_live: impl Fn(&str) -> Option<bool>,
    now_ms: i64,
) -> Vec<ReviewRunEnd> {
    let mut ended: Vec<(String, String)> = state
        .review_runs
        .iter()
        .filter(|(_, session_id)| session_live(session_id) == Some(false))
        .map(|(node_id, session_id)| (node_id.clone(), session_id.clone()))
        .collect();
    ended.sort();
    let mut outcomes = Vec::new();
    for (node_id, session_id) in ended {
        if let Some(successor) = live_on_branch
            .get(&node_id)
            .filter(|successor| **successor != session_id)
        {
            state.resuming.remove(&session_id);
            state
                .review_runs
                .insert(node_id.clone(), successor.clone());
            outcomes.push(ReviewRunEnd::Followed {
                node_id,
                session_id: successor.clone(),
            });
            continue;
        }
        if state
            .resuming
            .get(&session_id)
            .is_some_and(|at| now_ms - *at < RESUME_GRACE_MS)
        {
            continue; // a person's resume: the successor has not synced yet
        }
        state.resuming.remove(&session_id);
        state.review_runs.remove(&node_id);
        let launched_round = state.review_rounds.remove(&node_id).unwrap_or(0);
        let current_round = review_round_of.get(&node_id).copied().unwrap_or(0);
        if current_round > launched_round {
            state.review_failures.remove(&node_id);
            outcomes.push(ReviewRunEnd::Verdict { node_id });
            continue;
        }
        let failures = state.review_failures.entry(node_id.clone()).or_insert(0);
        *failures += 1;
        let failures = *failures;
        if failures < MAX_REVIEW_RUN_FAILURES {
            outcomes.push(ReviewRunEnd::Retry { node_id, failures });
        } else {
            outcomes.push(ReviewRunEnd::GaveUp {
                node_id,
                note: format!("{NOTE_REVIEW_NO_VERDICT} ({failures} runs)"),
            });
        }
    }
    let mut untracked: Vec<(&String, &String)> = live_on_branch
        .iter()
        .filter(|(node_id, _)| !state.review_runs.contains_key(*node_id))
        .collect();
    untracked.sort();
    for (node_id, session_id) in untracked {
        state
            .review_runs
            .insert(node_id.clone(), session_id.clone());
        state.review_rounds.insert(
            node_id.clone(),
            review_round_of.get(node_id).copied().unwrap_or(0),
        );
        outcomes.push(ReviewRunEnd::Adopted {
            node_id: node_id.clone(),
            session_id: session_id.clone(),
        });
    }
    outcomes
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

    #[test]
    fn wf_session_role_matches_the_contract() {
        let ours: Vec<&str> = [
            WfSessionRole::Author,
            WfSessionRole::Review,
            WfSessionRole::BaseMerge,
            WfSessionRole::Plan,
            WfSessionRole::Replan,
        ]
        .into_iter()
        .map(WfSessionRole::as_str)
        .collect();
        assert_eq!(ours, domain::contract::WF_SESSION_ROLE_VALUES);
        for value in domain::contract::WF_SESSION_ROLE_VALUES {
            let role = WfSessionRole::parse(value).expect("known role");
            assert_eq!(
                serde_json::to_value(role).unwrap(),
                serde_json::json!(value)
            );
        }
    }

    #[test]
    fn membership_needs_a_workflow_and_a_role_the_node_is_optional() {
        assert_eq!(
            WorkflowMembership::from_wire(Some("wf"), Some("n"), Some("review")),
            Some(WorkflowMembership {
                workflow_id: "wf".to_string(),
                node_id: Some("n".to_string()),
                role: WfSessionRole::Review,
            })
        );
        let plan = WorkflowMembership::from_wire(Some("wf"), None, Some("plan")).unwrap();
        assert_eq!(plan.node_id, None);
        assert_eq!(plan.role, WfSessionRole::Plan);
        assert_eq!(plan.wire().workflow_node_id, None);
        assert_eq!(WorkflowMembership::from_wire(None, Some("n"), Some("author")), None);
        assert_eq!(WorkflowMembership::from_wire(Some("wf"), Some("n"), None), None);
        assert_eq!(WorkflowMembership::from_wire(Some("wf"), Some("n"), Some("boss")), None);
    }

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

    const WF: &str = "wf-1";
    const INTEGRATION: &str = "exp/wf-abcdef12";

    fn running(nodes: Vec<NodeFacts>) -> Snapshot {
        Snapshot {
            workflow: WorkflowFacts {
                id: WF.to_string(),
                status: "running".to_string(),
                integration_branch: INTEGRATION.to_string(),
                final_pr_url: None,
                final_pr_state: None,
                max_parallel: 3,
                launch: launch::WorkflowLaunch::default(),
            },
            nodes,
            integration_branch_exists: true,
            now_ms: 1_000,
            ..Snapshot::default()
        }
    }

    /// A landed node: its run ended, its pull request merged.
    fn landed(id: &str, wave: i64, lane: i64) -> NodeFacts {
        let mut node = node(id, "landed", wave, lane);
        node.session_id = Some(format!("s-{id}"));
        node
    }

    fn ended(ended_at_ms: Option<i64>) -> SessionFacts {
        SessionFacts {
            live: false,
            ended_at_ms,
            ..SessionFacts::default()
        }
    }

    /// A started node on `base` with its branch up and its run ended a long
    /// time ago — the shape rule 4 acts on.
    fn started_on(id: &str, base: &str, snapshot: &mut Snapshot) {
        let mut facts = node(id, "in_review", 1, 0);
        facts.session_id = Some(format!("s-{id}"));
        facts.base_branch = Some(base.to_string());
        facts.branch = Some(format!("exp/EXP-{id}"));
        snapshot.tips.insert(format!("exp/EXP-{id}"), format!("sha-{id}"));
        // FEED-54: git says the base's current tip is not in the branch; a
        // tip `merged` already records still reads as nothing to do.
        snapshot.behind.insert((id.to_string(), base.to_string()));
        snapshot.sessions.insert(format!("s-{id}"), ended(Some(0)));
        snapshot.issues.insert(
            format!("issue-{id}"),
            IssueFacts { pr_state: Some("open".to_string()) },
        );
        snapshot.nodes.push(facts);
    }

    /// Whether a pass merges upstream or wakes a run. A leaf with its pull
    /// request up may LAND in the same pass (EXP-1103: no per-node review),
    /// which is not what the debounce tests are about.
    fn touches_upstream(decisions: &[Decision]) -> bool {
        decisions
            .iter()
            .any(|decision| decision.is_wake() || matches!(decision, Decision::MergeBase { .. }))
    }

    fn of_kind<'a>(decisions: &'a [Decision], test: fn(&Decision) -> bool) -> Vec<&'a Decision> {
        decisions.iter().filter(|decision| test(decision)).collect()
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

    /// The engine never makes or unmakes a person's call — and once every
    /// node is final the LAST wave reviews what landed before the final
    /// pull request opens.
    #[test]
    fn landed_and_skipped_nodes_are_left_alone_and_the_final_wave_reviews_them() {
        let snapshot = running(vec![landed("a", 0, 0), node("b", "skipped", 0, 1)]);
        assert_eq!(
            evaluate(&snapshot),
            vec![Decision::StartReview {
                node_id: "a".to_string(),
                wave: 0,
                model: Some("fable".to_string()),
                adversarial: false,
                workflow_id: WF.to_string(),
                role: WfSessionRole::Review,
                account: None,
            }]
        );
        // Cleared (the wave stamped it): the final pull request, and nothing
        // touches either row.
        let mut cleared = snapshot.clone();
        cleared.nodes[0].approved_at = Some("2026-09-25T12:00:00Z".to_string());
        assert_eq!(evaluate(&cleared), vec![Decision::OpenFinalPr]);
    }

    /// EXP-1059 (§7): a final PR closed WITHOUT merging is reopened ONCE.
    #[test]
    fn a_closed_final_pr_is_reopened_once() {
        let mut snapshot = running(vec![landed("a", 0, 0), node("b", "skipped", 0, 1)]);
        snapshot.nodes[0].approved_at = Some("t".to_string());
        snapshot.workflow.final_pr_url = Some("https://gh/pr/9".to_string());
        snapshot.workflow.final_pr_state = Some("closed".to_string());
        assert_eq!(evaluate(&snapshot), vec![Decision::ReopenFinalPr]);
        let mut in_flight = snapshot.clone();
        in_flight.final_pr_in_flight = true;
        assert!(evaluate(&in_flight).is_empty());
        let mut handled = snapshot.clone();
        handled.final_pr_close_handled = true;
        assert!(evaluate(&handled).is_empty());
        for state in ["open", "merged"] {
            let mut other = snapshot.clone();
            other.workflow.final_pr_state = Some(state.to_string());
            assert!(evaluate(&other).is_empty(), "state {state}");
        }
    }

    /// EXP-1059 (§7): every node skipped = nothing shipped — the workflow is
    /// cancelled with a note instead of opening an empty final PR.
    #[test]
    fn an_all_skipped_workflow_is_cancelled_with_a_note() {
        let snapshot = running(vec![node("a", "skipped", 0, 0), node("b", "skipped", 0, 1)]);
        assert_eq!(evaluate(&snapshot), vec![Decision::CancelUnshipped]);
        let mut proposed = snapshot.clone();
        proposed.nodes.push(node("c", "proposed", 1, 0));
        assert_eq!(evaluate(&proposed), vec![Decision::CancelUnshipped]);
        let mut paused = snapshot.clone();
        paused.workflow.status = "paused".to_string();
        assert!(evaluate(&paused).is_empty());
        let mut cancelled = snapshot.clone();
        cancelled.workflow.status = "cancelled".to_string();
        assert_eq!(evaluate(&cancelled), vec![Decision::DeleteIntegrationBranch]);
    }

    /// EXP-1007: a merge that left the run UP still lands the node —
    /// whatever the live run looks like, in every state a run can hold a
    /// node in.
    #[test]
    fn a_merged_pull_request_lands_the_node_while_its_run_is_live() {
        let sessions = [
            SessionFacts { live: true, ..SessionFacts::default() },
            SessionFacts { live: true, agent_busy: true, ..SessionFacts::default() },
            SessionFacts { live: true, needs_input: true, ..SessionFacts::default() },
            SessionFacts { live: true, blocked: true, ..SessionFacts::default() },
            SessionFacts::default(),
        ];
        for state in ["running", "waiting", "in_review", "updating"] {
            for session in &sessions {
                let mut a = node("a", state, 0, 0);
                a.session_id = Some("s-a".to_string());
                a.attempt = 1;
                let mut snapshot = running(vec![a, node("b", "ready", 0, 1)]);
                snapshot.sessions.insert("s-a".to_string(), session.clone());
                snapshot.issues.insert(
                    "issue-a".to_string(),
                    IssueFacts { pr_state: Some("merged".to_string()) },
                );
                let decisions = evaluate(&snapshot);
                let lands = of_kind(&decisions, |d| matches!(d, Decision::LandNode { .. }));
                assert_eq!(
                    lands,
                    vec![&Decision::LandNode { node_id: "a".to_string() }],
                    "{state} {session:?}: {decisions:?}"
                );
                assert!(
                    !decisions.iter().any(|decision| matches!(
                        decision,
                        Decision::SetNodeState { node_id, .. } if node_id == "a"
                    )),
                    "a merged node is landed, never re-mirrored: {decisions:?}"
                );
            }
        }
    }

    /// The land is asked for ONCE: not while the host is already on it.
    #[test]
    fn a_merged_node_with_a_live_run_lands_once() {
        let mut a = node("a", "running", 0, 0);
        a.session_id = Some("s-a".to_string());
        let mut landing = running(vec![a]);
        landing.sessions.insert(
            "s-a".to_string(),
            SessionFacts { live: true, agent_busy: true, ..SessionFacts::default() },
        );
        landing.issues.insert(
            "issue-a".to_string(),
            IssueFacts { pr_state: Some("merged".to_string()) },
        );
        landing.in_flight.insert("a".to_string());
        assert!(
            !evaluate(&landing)
                .iter()
                .any(|decision| matches!(decision, Decision::LandNode { .. })),
            "the host is mid-land"
        );
    }

    /// EXP-984/EXP-1103 — a review branch and a fix branch are this
    /// workflow's, name their round or wave, and never collide with the
    /// synthetic bases (which carry `-base-`).
    #[test]
    fn review_and_fix_branches_name_the_workflow_and_the_round() {
        let wf = "abcdef12-3456-7890-abcd-ef1234567890";
        assert_eq!(review_branch(wf, "EXP-42", 2), "exp/wf-abcdef12-review-EXP-42-r2");
        assert!(!is_synthetic(wf, &review_branch(wf, "EXP-42", 2)));
        assert_eq!(fix_branch(wf, 3), "exp/wf-abcdef12-fix-w3");
        assert!(!is_synthetic(wf, &fix_branch(wf, 3)));
        assert_eq!(fix_branch_wave(wf, "exp/wf-abcdef12-fix-w3"), Some(3));
        assert_eq!(fix_branch_wave(wf, "exp/wf-abcdef12-fix-w"), None);
        assert_eq!(fix_branch_wave(wf, "exp/wf-00000000-fix-w3"), None);
        assert_eq!(fix_branch_wave(wf, "exp/wf-abcdef12-base-EXP-3"), None);
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

    /// A `running` node whose `session_id` report has not landed is the
    /// host's own start: it holds its slot and is never started again inside
    /// the grace; past it the start is read as one that never came up.
    #[test]
    fn a_running_node_without_a_session_is_an_in_flight_start_inside_the_grace() {
        let mut fresh = node("a", "running", 0, 0);
        fresh.attempt = 1;
        fresh.updated_at_ms = Some(1_000);
        let mut snapshot = running(vec![fresh, node("b", "ready", 0, 1)]);
        snapshot.workflow.max_parallel = 1;
        snapshot.now_ms = 1_000 + START_GRACE_MS - 1;
        assert!(evaluate(&snapshot).is_empty(), "the slot is a's");

        snapshot.now_ms = 1_000 + START_GRACE_MS;
        let decisions = evaluate(&snapshot);
        assert_eq!(
            decisions[0],
            Decision::SetNodeState {
                node_id: "a".to_string(),
                state: "ready".to_string(),
                note: None,
            }
        );
        assert!(
            decisions.contains(&Decision::StartNode {
                node_id: "a".to_string(),
                attempt: 2,
                base_branch: INTEGRATION.to_string(),
                model: Some("opus".to_string()),
                workflow_id: WF.to_string(),
                role: WfSessionRole::Author,
                account: None,
            }),
            "past the grace the start is retried: {decisions:?}"
        );
    }

    /// EXP-1007: the second start that never came up is the last automatic
    /// one — the node fails with a note instead of a start every grace.
    #[test]
    fn a_start_that_never_came_up_twice_fails_instead_of_retrying_for_ever() {
        let mut stale = node("a", "running", 0, 0);
        stale.attempt = 2;
        stale.updated_at_ms = Some(1_000);
        let mut snapshot = running(vec![stale]);
        snapshot.now_ms = 1_000 + START_GRACE_MS;
        assert_eq!(
            evaluate(&snapshot),
            vec![Decision::SetNodeState {
                node_id: "a".to_string(),
                state: "failed".to_string(),
                note: Some(NOTE_START_NEVER_CAME_UP.to_string()),
            }]
        );
    }

    /// EXP-1007: a launch the host could not make (`failed`, no run) is
    /// retried once, after a rest; a second failure stays for a person.
    #[test]
    fn a_failed_launch_is_retried_once_then_left_for_a_person() {
        let mut first = node("a", "failed", 0, 0);
        first.attempt = 1;
        let snapshot = running(vec![first]);
        assert!(evaluate(&snapshot)
            .iter()
            .any(|decision| matches!(decision, Decision::StartNode { attempt: 2, .. })));

        let mut resting = node("a", "failed", 0, 0);
        resting.attempt = 1;
        resting.updated_at_ms = Some(1_000);
        let mut snapshot = running(vec![resting]);
        snapshot.now_ms = 1_000 + RETRY_BACKOFF_MS - 1;
        assert!(evaluate(&snapshot).is_empty(), "the failure rests first");
        snapshot.now_ms = 1_000 + RETRY_BACKOFF_MS;
        assert!(evaluate(&snapshot)
            .iter()
            .any(|decision| matches!(decision, Decision::StartNode { attempt: 2, .. })));

        let mut second = node("a", "failed", 0, 0);
        second.attempt = 2;
        assert!(evaluate(&running(vec![second])).is_empty());
    }

    /// EXP-1029: ONE model policy — the cheap model writes the leaves, the
    /// strong one the contract and integration nodes and every `risk: high`
    /// node, whatever its kind; every review and fix run on the strong one.
    #[test]
    fn the_strong_model_takes_the_contract_nodes_and_every_review() {
        let mut contract_node = node("a", "ready", 0, 0);
        contract_node.kind = "contract".to_string();
        let snapshot = running(vec![contract_node, node("b", "ready", 0, 1)]);
        let started: Vec<(String, Option<String>)> = evaluate(&snapshot)
            .into_iter()
            .filter_map(|decision| match decision {
                Decision::StartNode { node_id, model, .. } => Some((node_id, model)),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![
                ("a".to_string(), Some("fable".to_string())),
                ("b".to_string(), Some("opus".to_string()))
            ]
        );
        let facts = &running(vec![]).workflow;
        assert_eq!(node_model(facts, "integration", "low").as_deref(), Some("fable"));
        assert_eq!(node_model(facts, "leaf", RISK_HIGH).as_deref(), Some("fable"));
        assert_eq!(node_model(facts, "leaf", "medium").as_deref(), Some("opus"));
        assert_eq!(review_model(&running(vec![])).as_deref(), Some("fable"));
        let mut codex = running(vec![]);
        codex.workflow.launch =
            launch::normalize_workflow_launch(&serde_json::json!({ "agent": "codex" }));
        assert_eq!(review_model(&codex).as_deref(), Some("gpt-5.6-luna"));
    }

    /// The DAG relation rule 5 skips: an ancestor through any chain of
    /// blockers, in either direction, never a mere sibling.
    #[test]
    fn dag_related_follows_blocker_chains_both_ways() {
        let snapshot = Snapshot {
            nodes: vec![
                node("a", "running", 0, 0),
                node("b", "running", 1, 0),
                node("c", "running", 2, 0),
                node("d", "running", 0, 1),
            ],
            edges: vec![
                ("a".to_string(), "b".to_string()),
                ("b".to_string(), "c".to_string()),
            ],
            ..Snapshot::default()
        };
        let blockers = blockers_by_node(&snapshot);
        assert!(dag_related(&blockers, "a", "c"));
        assert!(dag_related(&blockers, "c", "a"));
        assert!(!dag_related(&blockers, "a", "d"));
    }

    // ── EXP-1106: mechanical merges, wakes, debounce, stagger ─────────────

    /// A base that moved under an ENDED run is merged by the host — no
    /// agent wakes — and a tip already in the branch is nothing at all.
    #[test]
    fn a_clean_upstream_move_is_merged_by_the_host_without_a_wake() {
        let mut snapshot = running(vec![landed("c", 0, 0)]);
        snapshot.nodes[0].approved_at = Some("t".to_string());
        started_on("a", INTEGRATION, &mut snapshot);
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i2".to_string());
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        snapshot.now_ms = UPSTREAM_DEBOUNCE_MS;
        let decisions = evaluate(&snapshot);
        assert_eq!(
            of_kind(&decisions, |d| matches!(d, Decision::MergeBase { .. })),
            vec![&Decision::MergeBase {
                node_id: "a".to_string(),
                branch: "exp/EXP-a".to_string(),
                base_branch: INTEGRATION.to_string(),
                sha: "sha-i2".to_string(),
            }]
        );
        assert!(!decisions.iter().any(Decision::is_wake), "{decisions:?}");
        // Already in: quiet.
        snapshot
            .merged
            .insert("a".to_string(), [(INTEGRATION.to_string(), "sha-i2".to_string())].into());
        assert!(!evaluate(&snapshot)
            .iter()
            .any(|d| matches!(d, Decision::MergeBase { .. })));
    }

    /// A conflicting move wakes the run ONCE: resumed while its cache is
    /// warm, fresh with a brief after that; the recorded wake is never
    /// repeated for the same tip.
    #[test]
    fn a_conflicting_upstream_move_wakes_the_run_once() {
        let mut snapshot = running(vec![]);
        started_on("a", INTEGRATION, &mut snapshot);
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i2".to_string());
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        snapshot.base_conflicts.insert(("a".to_string(), INTEGRATION.to_string()));
        snapshot.now_ms = UPSTREAM_DEBOUNCE_MS;
        snapshot.sessions.insert("s-a".to_string(), ended(Some(snapshot.now_ms - 1_000)));
        let wake = Decision::WakeRun {
            node_id: "a".to_string(),
            session_id: "s-a".to_string(),
            mode: WakeMode::Resume,
            reason: WakeReason::UpstreamConflict {
                base_branch: INTEGRATION.to_string(),
                sha: "sha-i2".to_string(),
            },
        };
        let decisions = evaluate(&snapshot);
        assert_eq!(of_kind(&decisions, Decision::is_wake), vec![&wake]);
        assert!(!decisions.iter().any(|d| matches!(d, Decision::MergeBase { .. })));
        // Ended long ago: a fresh run with a brief instead of a cold resume.
        snapshot.sessions.insert("s-a".to_string(), ended(Some(snapshot.now_ms - IDLE_RESUME_MS)));
        assert!(matches!(
            of_kind(&evaluate(&snapshot), Decision::is_wake)[0],
            Decision::WakeRun { mode: WakeMode::Fresh, .. }
        ));
        // Woken for this tip already: nothing more until it moves again.
        snapshot
            .woken
            .insert("a".to_string(), [(INTEGRATION.to_string(), "sha-i2".to_string())].into());
        assert!(!evaluate(&snapshot).iter().any(Decision::is_wake));
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i3".to_string());
        assert_eq!(of_kind(&evaluate(&snapshot), Decision::is_wake).len(), 1);
    }

    /// FEED-54: a pair git never proved behind is not movement. No
    /// `merged` entry (never recorded, store lost, ancestry unanswered) and
    /// no `behind` fact means nothing, however old the tip reads.
    #[test]
    fn an_unknown_merged_entry_is_not_movement() {
        let mut snapshot = running(vec![]);
        started_on("a", INTEGRATION, &mut snapshot);
        snapshot.behind.clear();
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i2".to_string());
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        snapshot.base_conflicts.insert(("a".to_string(), INTEGRATION.to_string()));
        snapshot.now_ms = UPSTREAM_DEBOUNCE_MS;
        assert!(snapshot.merged.is_empty());
        assert!(!touches_upstream(&evaluate(&snapshot)));
        // Git proves it: now the move is real.
        snapshot.behind.insert(("a".to_string(), INTEGRATION.to_string()));
        assert!(touches_upstream(&evaluate(&snapshot)));
    }

    /// FEED-54: a collision with a SIBLING's branch never marks the base as
    /// conflicting. The base's clean move is merged by the host, no wake.
    #[test]
    fn a_sibling_conflict_does_not_flag_the_base() {
        let mut snapshot = running(vec![]);
        started_on("a", INTEGRATION, &mut snapshot);
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i2".to_string());
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        snapshot.base_conflicts.insert(("a".to_string(), "exp/EXP-b".to_string()));
        snapshot.now_ms = UPSTREAM_DEBOUNCE_MS;
        let decisions = evaluate(&snapshot);
        assert!(!decisions.iter().any(Decision::is_wake), "{decisions:?}");
        assert!(decisions.iter().any(|d| matches!(d, Decision::MergeBase { .. })), "{decisions:?}");
    }

    /// Three quick moves become one merge: while the tip keeps moving inside
    /// the window nothing happens, and one decision follows once it rests.
    /// A live run is never touched at all.
    #[test]
    fn quick_upstream_moves_are_debounced_into_one() {
        let mut snapshot = running(vec![]);
        started_on("a", INTEGRATION, &mut snapshot);
        snapshot.now_ms = 100_000;
        for (sha, seen) in [("sha-i2", 40_000), ("sha-i3", 70_000), ("sha-i4", 90_000)] {
            snapshot.tips.insert(INTEGRATION.to_string(), sha.to_string());
            snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), seen);
            assert!(!touches_upstream(&evaluate(&snapshot)), "{sha} still moving");
        }
        // A tip the host has not dated reads as just seen: it waits too.
        snapshot.tip_seen_ms.clear();
        assert!(!touches_upstream(&evaluate(&snapshot)));
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 90_000);
        snapshot.now_ms = 90_000 + UPSTREAM_DEBOUNCE_MS;
        // Merged forward first, then landed, in that order (its PR is up).
        assert_eq!(
            evaluate(&snapshot),
            vec![
                Decision::MergeBase {
                    node_id: "a".to_string(),
                    branch: "exp/EXP-a".to_string(),
                    base_branch: INTEGRATION.to_string(),
                    sha: "sha-i4".to_string(),
                },
                Decision::LandNode { node_id: "a".to_string() },
            ]
        );
        snapshot
            .sessions
            .insert("s-a".to_string(), SessionFacts { live: true, ..SessionFacts::default() });
        assert!(!touches_upstream(&evaluate(&snapshot)), "a live run pulls itself");
    }

    /// EXP-1106 rule 3: the wake mode by idle time — steer a live idle run,
    /// resume an ended one inside the TTL, start fresh past it; never a run
    /// mid-turn or parked on a question.
    #[test]
    fn the_wake_mode_follows_the_idle_time() {
        let now = 10 * 60_000;
        let live = SessionFacts { live: true, ..SessionFacts::default() };
        assert_eq!(wake_mode(&live, now), Some(WakeMode::Steer));
        let busy = SessionFacts { live: true, agent_busy: true, ..SessionFacts::default() };
        assert_eq!(wake_mode(&busy, now), None);
        let asking = SessionFacts { live: true, needs_input: true, ..SessionFacts::default() };
        assert_eq!(wake_mode(&asking, now), None);
        assert_eq!(wake_mode(&ended(Some(now - IDLE_RESUME_MS + 1)), now), Some(WakeMode::Resume));
        assert_eq!(wake_mode(&ended(Some(now - IDLE_RESUME_MS)), now), Some(WakeMode::Fresh));
        assert_eq!(wake_mode(&ended(None), now), Some(WakeMode::Fresh));
    }

    /// A host restart (or a reset) with many runs to wake sends them out a
    /// few at a time: the budget caps nudges and wakes alike, per pass.
    #[test]
    fn wakes_go_out_a_few_per_pass() {
        let mut snapshot = running(vec![]);
        for id in ["a", "b", "c", "d"] {
            started_on(id, INTEGRATION, &mut snapshot);
            snapshot.base_conflicts.insert((id.to_string(), INTEGRATION.to_string()));
        }
        // Two more runs walled and reset: nudges compete for the same budget.
        for id in ["e", "f"] {
            let mut facts = node(id, "running", 1, 5);
            facts.session_id = Some(format!("s-{id}"));
            snapshot.sessions.insert(
                format!("s-{id}"),
                SessionFacts {
                    live: true,
                    blocked: true,
                    blocked_resets_at_ms: Some(0),
                    ..SessionFacts::default()
                },
            );
            snapshot.nodes.push(facts);
        }
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-i2".to_string());
        snapshot.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        snapshot.now_ms = UPSTREAM_DEBOUNCE_MS;
        let decisions = evaluate(&snapshot);
        let wakes = of_kind(&decisions, Decision::is_wake);
        assert_eq!(wakes.len(), MAX_WAKES_PER_PASS, "{decisions:?}");
        // The order is the cascade's: the upstream wakes first, in landing
        // order; the nudges wait their turn.
        assert!(matches!(wakes[0], Decision::WakeRun { node_id, .. } if node_id == "a"));
        assert!(matches!(wakes[1], Decision::WakeRun { node_id, .. } if node_id == "b"));
    }

    /// EXP-1106 rule 2: a run hears ONCE that a dependent started on its
    /// work — while it is live and idle, never mid-turn.
    #[test]
    fn the_freeze_notice_is_said_once() {
        let mut contract = node("c", "running", 0, 0);
        contract.kind = "contract".to_string();
        contract.session_id = Some("s-c".to_string());
        contract.checkpoint_at = Some("t".to_string());
        let mut leaf = node("l", "running", 1, 0);
        leaf.session_id = Some("s-l".to_string());
        let mut snapshot = running(vec![contract, leaf]);
        snapshot.edges.push(("c".to_string(), "l".to_string()));
        snapshot
            .sessions
            .insert("s-c".to_string(), SessionFacts { live: true, ..SessionFacts::default() });
        snapshot
            .sessions
            .insert("s-l".to_string(), SessionFacts { live: true, ..SessionFacts::default() });
        let notice = Decision::Nudge {
            session_id: "s-c".to_string(),
            key: NUDGE_KEY_FREEZE.to_string(),
            text: NUDGE_FREEZE.to_string(),
        };
        assert_eq!(evaluate(&snapshot), vec![notice.clone()]);
        snapshot.nudged.insert(("s-c".to_string(), NUDGE_KEY_FREEZE.to_string()));
        assert!(evaluate(&snapshot).is_empty());
        // Mid-turn: not now. No dependent started: not at all.
        snapshot.nudged.clear();
        snapshot.sessions.get_mut("s-c").unwrap().agent_busy = true;
        assert!(evaluate(&snapshot).is_empty());
        snapshot.sessions.get_mut("s-c").unwrap().agent_busy = false;
        snapshot.nodes[1].session_id = None;
        snapshot.nodes[1].state = "blocked".to_string();
        assert!(!evaluate(&snapshot).contains(&notice));
    }

    // ── EXP-1103: review waves ────────────────────────────────────────────

    /// One wave at the end by default; a graph deeper than three layers adds
    /// one after the contract layer and every third layer after it.
    #[test]
    fn review_points_follow_the_depth() {
        assert_eq!(review_points(0), Vec::<i64>::new());
        assert_eq!(review_points(1), vec![0]);
        assert_eq!(review_points(2), vec![1]);
        assert_eq!(review_points(3), vec![2]);
        assert_eq!(review_points(4), vec![0, 3]);
        assert_eq!(review_points(5), vec![0, 3, 4]);
        assert_eq!(review_points(7), vec![0, 3, 6]);
        assert_eq!(review_points(8), vec![0, 3, 6, 7]);
    }

    /// The whole wave, beat by beat: reviewers for every landed node (a few
    /// per pass), one fix run for what they requested, its landing, the
    /// clearance, then the final pull request. One round, never a loop.
    #[test]
    fn a_wave_reviews_fixes_once_and_clears() {
        let mut snapshot = running(vec![landed("a", 0, 0), landed("b", 0, 1), landed("c", 1, 0)]);
        snapshot.workflow.max_parallel = 2;
        snapshot.edges.push(("a".to_string(), "c".to_string()));
        let review = |node_id: &str| Decision::StartReview {
            node_id: node_id.to_string(),
            wave: 1,
            model: Some("fable".to_string()),
            adversarial: false,
            workflow_id: WF.to_string(),
            role: WfSessionRole::Review,
            account: None,
        };
        // Reviewers go out max_parallel at a time, in landing order.
        assert_eq!(evaluate(&snapshot), vec![review("a"), review("b")]);
        snapshot.review_in_flight.insert("a".to_string());
        snapshot.review_in_flight.insert("b".to_string());
        assert_eq!(evaluate(&snapshot), vec![review("c")]);
        // Verdicts land: a approves, b requests changes, c's reviewers gave up.
        snapshot.review_in_flight.clear();
        snapshot.nodes[0].review = Some(ReviewFacts { verdict: "approve".to_string(), round: 1, ..ReviewFacts::default() });
        snapshot.nodes[1].review = Some(ReviewFacts { verdict: "request_changes".to_string(), round: 1, ..ReviewFacts::default() });
        snapshot.review_gave_up.insert("c".to_string());
        let fix = Decision::StartFix {
            wave: 1,
            node_ids: vec!["b".to_string()],
            branch: fix_branch(WF, 1),
            model: Some("fable".to_string()),
            workflow_id: WF.to_string(),
            role: WfSessionRole::Review,
            account: None,
        };
        assert_eq!(evaluate(&snapshot), vec![fix]);
        // The fix run is up: wait. Ended: land what it pushed. Landed: clear.
        snapshot.fix_runs.push(FixRunFacts { wave: 1, session_id: "s-fix".to_string(), live: true, landed: false });
        assert!(evaluate(&snapshot).is_empty());
        snapshot.fix_runs[0].live = false;
        assert_eq!(evaluate(&snapshot), vec![Decision::LandFix { wave: 1, branch: fix_branch(WF, 1) }]);
        snapshot.fix_runs[0].landed = true;
        assert_eq!(
            evaluate(&snapshot),
            vec![Decision::ClearWave {
                wave: 1,
                node_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            }]
        );
        // Stamped: the final pull request, and no second round however the
        // verdict reads.
        for node in &mut snapshot.nodes {
            node.approved_at = Some("t".to_string());
        }
        assert_eq!(evaluate(&snapshot), vec![Decision::OpenFinalPr]);
    }

    /// A wave nobody asked anything of clears without a fix run; an approved
    /// oracle failure counts as a request.
    #[test]
    fn a_wave_with_nothing_requested_clears_at_once() {
        let mut snapshot = running(vec![landed("a", 0, 0)]);
        snapshot.nodes[0].review = Some(ReviewFacts { verdict: "approve".to_string(), round: 1, ..ReviewFacts::default() });
        assert_eq!(
            evaluate(&snapshot),
            vec![Decision::ClearWave { wave: 0, node_ids: vec!["a".to_string()] }]
        );
        snapshot.nodes[0].review.as_mut().unwrap().oracle = Some(OracleFacts { passed: Some(false) });
        assert!(matches!(evaluate(&snapshot)[0], Decision::StartFix { .. }));
    }

    /// In a DEEP graph the wave after the contract layer gates the layers
    /// behind it: an unstarted node waits (and says why, once), a finished
    /// one does not land, until the wave cleared.
    #[test]
    fn a_mid_graph_wave_gates_the_layers_behind_it() {
        // Four layers: contract, two leaf layers, integration. `l` (layer 1)
        // finished on the contract; `n` (layer 1) is released by it and
        // would start; `m` and `i` wait on their blockers anyway.
        let mut snapshot = running(vec![landed("c", 0, 0), node("n", "blocked", 1, 1), node("m", "blocked", 2, 0), node("i", "blocked", 3, 0)]);
        snapshot.edges.extend([
            ("c".to_string(), "l".to_string()),
            ("c".to_string(), "n".to_string()),
            ("l".to_string(), "m".to_string()),
            ("m".to_string(), "i".to_string()),
        ]);
        started_on("l", INTEGRATION, &mut snapshot);
        snapshot.merged.insert("l".to_string(), [(INTEGRATION.to_string(), "sha-c".to_string())].into());
        snapshot.tips.insert(INTEGRATION.to_string(), "sha-c".to_string());
        let decisions = evaluate(&snapshot);
        // The contract's wave is due (layer 0 is final) and not cleared: it
        // reviews c; n stays blocked with the gate's note; l does not land.
        assert!(decisions.iter().any(|d| matches!(d, Decision::StartReview { node_id, wave: 0, .. } if node_id == "c")), "{decisions:?}");
        assert!(decisions.contains(&Decision::SetNodeState {
            node_id: "n".to_string(),
            state: "blocked".to_string(),
            note: Some(wave_gate_note(0)),
        }), "{decisions:?}");
        assert!(!decisions.iter().any(|d| matches!(d, Decision::LandNode { .. } | Decision::StartNode { .. })), "{decisions:?}");
        assert!(is_engine_hold_note(&wave_gate_note(0)));
        // Cleared: l lands, n starts, and the note goes with the hold.
        snapshot.nodes.iter_mut().find(|n| n.id == "c").unwrap().approved_at = Some("t".to_string());
        snapshot.nodes.iter_mut().find(|n| n.id == "n").unwrap().note = Some(wave_gate_note(0));
        let decisions = evaluate(&snapshot);
        assert!(decisions.contains(&Decision::LandNode { node_id: "l".to_string() }), "{decisions:?}");
        assert!(decisions.contains(&Decision::SetNodeState { node_id: "n".to_string(), state: "ready".to_string(), note: None }), "{decisions:?}");
        assert!(decisions.iter().any(|d| matches!(d, Decision::StartNode { node_id, .. } if node_id == "n")), "{decisions:?}");
        assert!(!decisions.iter().any(|d| matches!(d, Decision::StartReview { .. })));
    }

    /// EXP-1102: the store is a CACHE. Wiped mid-run, the next pass decides
    /// the same things off the synced rows and git — at most one extra
    /// mechanical merge (a no-op) once the debounce window passed.
    #[test]
    fn a_wiped_store_converges_within_one_beat() {
        let mut cached = running(vec![landed("c", 0, 0)]);
        cached.nodes[0].approved_at = Some("t".to_string());
        started_on("a", INTEGRATION, &mut cached);
        cached.tips.insert(INTEGRATION.to_string(), "sha-c".to_string());
        cached.tip_seen_ms.insert(INTEGRATION.to_string(), 0);
        cached.merged.insert("a".to_string(), [(INTEGRATION.to_string(), "sha-c".to_string())].into());
        cached.now_ms = UPSTREAM_DEBOUNCE_MS;
        let expected = evaluate(&cached);
        assert_eq!(expected, vec![Decision::LandNode { node_id: "a".to_string() }]);

        // The store is gone: no merged cache, no tip clock, no woken record.
        // The tip IS in the branch, so git never calls the pair behind.
        let mut bare = cached.clone();
        bare.merged.clear();
        bare.behind.clear();
        bare.tip_seen_ms.clear();
        bare.woken.clear();
        assert_eq!(evaluate(&bare), expected, "the same pass, the tip reads as just seen");
        // FEED-54: one beat later, still nothing: an unknown `merged` entry
        // is not movement, so no merge (and no notice) is decided.
        bare.tip_seen_ms.insert(INTEGRATION.to_string(), bare.now_ms);
        bare.now_ms += UPSTREAM_DEBOUNCE_MS;
        assert_eq!(evaluate(&bare), expected);
        // And the host's ancestry check refills the cache from git alone.
        bare.merged = cached.merged.clone();
        assert_eq!(evaluate(&bare), expected);
    }

    // ── host bookkeeping ─────────────────────────────────────────────────

    /// A reviewer run that ended: a round that advanced was a verdict; one
    /// that did not is retried, up to the cap, after which the node gets a
    /// note.
    #[test]
    fn ended_review_runs_settle_into_verdicts_retries_and_a_cap() {
        let mut state = WorkflowState::default();
        state.review_runs.insert("a".to_string(), "r-a".to_string());
        state.review_rounds.insert("a".to_string(), 1);
        state.review_runs.insert("b".to_string(), "r-b".to_string());
        state.review_rounds.insert("b".to_string(), 0);
        state.review_runs.insert("c".to_string(), "r-c".to_string());
        let rounds: HashMap<String, i64> =
            [("a".to_string(), 2), ("b".to_string(), 0), ("c".to_string(), 0)].into();
        let live = |id: &str| match id {
            "r-a" | "r-b" => Some(false),
            _ => None,
        };
        let outcomes = settle_review_runs(&mut state, &rounds, &HashMap::new(), live, 0);
        assert_eq!(
            outcomes,
            vec![
                ReviewRunEnd::Verdict { node_id: "a".to_string() },
                ReviewRunEnd::Retry { node_id: "b".to_string(), failures: 1 },
            ]
        );
        assert!(!state.review_runs.contains_key("a"));
        assert!(!state.review_runs.contains_key("b"));
        assert!(state.review_runs.contains_key("c"));
        for expected_failures in [2, 3] {
            state.review_runs.insert("b".to_string(), "r-b2".to_string());
            state.review_rounds.insert("b".to_string(), 0);
            let outcomes = settle_review_runs(
                &mut state,
                &rounds,
                &HashMap::new(),
                |id| match id {
                    "r-b2" => Some(false),
                    _ => None,
                },
                0,
            );
            if expected_failures < MAX_REVIEW_RUN_FAILURES {
                assert_eq!(outcomes, vec![ReviewRunEnd::Retry { node_id: "b".to_string(), failures: expected_failures }]);
            } else {
                assert_eq!(
                    outcomes,
                    vec![ReviewRunEnd::GaveUp {
                        node_id: "b".to_string(),
                        note: format!("{NOTE_REVIEW_NO_VERDICT} (3 runs)"),
                    }]
                );
                assert_eq!(state.review_failures["b"], MAX_REVIEW_RUN_FAILURES);
            }
        }
    }

    /// A live run on a node's PENDING review branch is that node's reviewer,
    /// whatever id the host recorded: the prefix names the workflow and the
    /// exact issue (`EXP-10` never claims `EXP-103`'s branch), the round
    /// follows it, and the newest live row wins.
    #[test]
    fn live_reviews_are_found_by_their_branch() {
        let wf = "abcdef12-3456-7890-abcd-ef1234567890";
        let identifier: HashMap<String, String> = [
            ("n10".to_string(), "EXP-10".to_string()),
            ("n103".to_string(), "EXP-103".to_string()),
        ]
        .into();
        let rounds: HashMap<String, i64> = [("n10".to_string(), 1), ("n103".to_string(), 1)].into();
        let rows = vec![
            ("s-older".to_string(), review_branch(wf, "EXP-10", 2)),
            ("s-103".to_string(), review_branch(wf, "EXP-103", 2)),
            ("s-other-wf".to_string(), "exp/wf-00000000-review-EXP-10-r2".to_string()),
            ("s-base".to_string(), "exp/wf-abcdef12-base-EXP-10".to_string()),
            ("s-noround".to_string(), "exp/wf-abcdef12-review-EXP-10-r".to_string()),
            ("s-new".to_string(), review_branch(wf, "EXP-10", 2)),
        ];
        let found = live_pending_reviews_on_branches(wf, &identifier, &rounds, rows);
        assert_eq!(
            found,
            [
                ("n10".to_string(), "s-new".to_string()),
                ("n103".to_string(), "s-103".to_string()),
            ]
            .into()
        );
    }

    /// A reviewer launches on `-r<review_round + 1>` and its verdict moves the
    /// node up to that round: a live run on the node's current round or lower
    /// already submitted and only lingers; it is neither adopted nor followed.
    #[test]
    fn a_live_older_round_reviewer_is_not_adopted() {
        let wf = "abcdef12-3456-7890-abcd-ef1234567890";
        let identifier: HashMap<String, String> = [
            ("n1".to_string(), "EXP-1".to_string()),
            ("n2".to_string(), "EXP-2".to_string()),
        ]
        .into();
        assert_eq!(review_branch_round(wf, "EXP-1", &review_branch(wf, "EXP-1", 3)), Some(3));
        assert_eq!(review_branch_round(wf, "EXP-1", &review_branch(wf, "EXP-10", 3)), None);
        let rounds: HashMap<String, i64> = [("n1".to_string(), 2), ("n2".to_string(), 0)].into();
        let rows = vec![
            ("s-n1-old".to_string(), review_branch(wf, "EXP-1", 1)),
            ("s-n1-landed".to_string(), review_branch(wf, "EXP-1", 2)),
            ("s-n2-pending".to_string(), review_branch(wf, "EXP-2", 1)),
        ];
        let pending = live_pending_reviews_on_branches(wf, &identifier, &rounds, rows.clone());
        assert_eq!(pending, [("n2".to_string(), "s-n2-pending".to_string())].into());
        let mut state = WorkflowState::default();
        let live = |id: &str| match id {
            "s-n1-old" | "s-n1-landed" | "s-n2-pending" | "s-n1-next" => Some(true),
            _ => None,
        };
        let outcomes = settle_review_runs(&mut state, &rounds, &pending, live, 0);
        assert_eq!(
            outcomes,
            vec![ReviewRunEnd::Adopted { node_id: "n2".to_string(), session_id: "s-n2-pending".to_string() }]
        );
        assert!(!state.review_runs.contains_key("n1"));
        let mut rows = rows;
        rows.push(("s-n1-next".to_string(), review_branch(wf, "EXP-1", 3)));
        let pending = live_pending_reviews_on_branches(wf, &identifier, &rounds, rows);
        let outcomes = settle_review_runs(&mut state, &rounds, &pending, live, 0);
        assert_eq!(
            outcomes,
            vec![ReviewRunEnd::Adopted { node_id: "n1".to_string(), session_id: "s-n1-next".to_string() }]
        );
        assert_eq!(state.review_rounds["n1"], 2);
    }

    /// A person's account switch ENDS the recorded reviewer and resumes it
    /// on the same branch: the ended record is re-pointed at the live
    /// resume, nothing is counted; a live reviewer no record names is
    /// adopted; a resume whose successor has not synced yet is under a hold
    /// and waits, then settles once the grace passed.
    #[test]
    fn a_resumed_reviewer_is_followed_not_counted_and_a_stray_one_adopted() {
        let mut state = WorkflowState::default();
        state.review_runs.insert("a".to_string(), "r-a1".to_string());
        state.review_rounds.insert("a".to_string(), 1);
        state.review_runs.insert("h".to_string(), "r-h1".to_string());
        state.review_rounds.insert("h".to_string(), 0);
        state.resuming.insert("r-h1".to_string(), 9_000);
        let rounds: HashMap<String, i64> =
            [("a".to_string(), 1), ("b".to_string(), 2), ("h".to_string(), 0)].into();
        let live_on_branch: HashMap<String, String> = [
            ("a".to_string(), "r-a2".to_string()),
            ("b".to_string(), "r-b".to_string()),
        ]
        .into();
        let live = |id: &str| match id {
            "r-a1" | "r-h1" => Some(false),
            "r-a2" | "r-b" => Some(true),
            _ => None,
        };
        let outcomes = settle_review_runs(&mut state, &rounds, &live_on_branch, live, 10_000);
        assert_eq!(
            outcomes,
            vec![
                ReviewRunEnd::Followed { node_id: "a".to_string(), session_id: "r-a2".to_string() },
                ReviewRunEnd::Adopted { node_id: "b".to_string(), session_id: "r-b".to_string() },
            ]
        );
        assert_eq!(state.review_runs["a"], "r-a2");
        assert_eq!(state.review_rounds["a"], 1);
        assert!(state.review_failures.is_empty());
        assert_eq!(state.review_runs["h"], "r-h1", "held until the grace passes");
        let outcomes = settle_review_runs(&mut state, &rounds, &HashMap::new(), live, 9_000 + RESUME_GRACE_MS);
        assert_eq!(outcomes, vec![ReviewRunEnd::Retry { node_id: "h".to_string(), failures: 1 }]);
        assert!(!state.resuming.contains_key("r-h1"));
        let outcomes = settle_review_runs(&mut state, &rounds, &live_on_branch, live, 20_000);
        assert!(outcomes.is_empty());
    }

    /// A hold on a REVIEWER run survives `apply_resuming` (no node names a
    /// reviewer) without marking any session live.
    #[test]
    fn a_reviewer_hold_survives_the_resume_pruning() {
        let mut snapshot = running(vec![node("a", "in_review", 0, 0)]);
        snapshot.now_ms = 10_000;
        let mut resuming: HashMap<String, i64> =
            [("r-a".to_string(), 9_000), ("s-gone".to_string(), 9_000)].into();
        let review_runs: HashMap<String, String> = [("a".to_string(), "r-a".to_string())].into();
        apply_resuming(&mut snapshot, &mut resuming, &review_runs);
        assert_eq!(resuming.keys().collect::<Vec<_>>(), ["r-a"]);
        assert!(!snapshot.sessions.contains_key("r-a"));
    }

    /// A session the host is resuming reads as live and mid-turn until the
    /// node names the new run or the grace passes; both prune the entry.
    #[test]
    fn a_resuming_session_reads_live_and_busy_inside_the_grace() {
        let mut old = node("a", "updating", 0, 0);
        old.session_id = Some("s-old".to_string());
        let mut snapshot = running(vec![old]);
        snapshot.sessions.insert("s-old".to_string(), SessionFacts::default());
        snapshot.now_ms = 10_000;
        let mut resuming: HashMap<String, i64> = [
            ("s-old".to_string(), 9_000),
            ("s-gone".to_string(), 9_000),
            ("s-stale".to_string(), 10_000 - RESUME_GRACE_MS),
        ]
        .into();
        apply_resuming(&mut snapshot, &mut resuming, &HashMap::new());
        let facts = &snapshot.sessions["s-old"];
        assert!(facts.live && facts.agent_busy);
        assert_eq!(resuming.keys().collect::<Vec<_>>(), ["s-old"]);
        assert!(evaluate(&snapshot).is_empty());
        snapshot.nodes[0].session_id = Some("s-new".to_string());
        apply_resuming(&mut snapshot, &mut resuming, &HashMap::new());
        assert!(resuming.is_empty());
    }

    /// A refusal is forgotten once the head moved, kept while it has not or
    /// cannot be seen.
    #[test]
    fn land_refusals_are_pruned_by_a_moved_head() {
        let mut refused: HashMap<String, String> = [
            ("a".to_string(), "sha-a1".to_string()),
            ("b".to_string(), "sha-b1".to_string()),
            ("c".to_string(), "sha-c1".to_string()),
        ]
        .into();
        let heads: HashMap<String, String> = [
            ("a".to_string(), "sha-a1".to_string()),
            ("b".to_string(), "sha-b2".to_string()),
        ]
        .into();
        prune_land_refused(&mut refused, &heads);
        let mut kept: Vec<&String> = refused.keys().collect();
        kept.sort();
        assert_eq!(kept, ["a", "c"]);
    }

    /// Both wire forms of a timestamp read, and garbage reads as none.
    #[test]
    fn wire_timestamps_parse_in_both_forms() {
        assert_eq!(parse_wire_timestamp_ms("2026-07-03T10:11:12.345Z"), Some(1_783_073_472_345));
        assert_eq!(parse_wire_timestamp_ms("2026-07-03 10:11:12.345+00"), Some(1_783_073_472_345));
        assert_eq!(parse_wire_timestamp_ms("2026-07-03 10:11:12.345"), Some(1_783_073_472_345));
        assert_eq!(parse_wire_timestamp_ms("yesterday"), None);
    }
}
