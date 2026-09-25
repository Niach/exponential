//! EXP-982: the desktop GUI's workflow-engine host — the CLI daemon
//! worker's twin, per signed-in account and shaped exactly like
//! [`crate::automation_host`].
//!
//! The engine ([`coding::workflows`]) is PURE: this host snapshots its inputs
//! on the foreground (the workflows bound to THIS device, their nodes, the
//! `blocks` edges between them, the representative issues' PR states, the
//! coding sessions, the live engine handles, the clock) and runs
//! `read_states` → `evaluate` → execute on the background executor, because
//! every decision is tRPC, git or a channel send.
//!
//! SINGLE WRITER: only workflows whose `device_id` equals this app's steer
//! device id are evaluated. The GUI and the CLI daemon have distinct device
//! ids, which is the double-run defence — exactly as for automations.
//!
//! In-flight bookkeeping is per PROCESS, not persisted: a node the host is
//! mid-start (or mid-land) on is left alone for the whole pass, and a crash
//! simply re-decides from the synced rows next launch, which is what
//! level-triggered means. What DOES persist (`coding::workflows::state`) is
//! only what must happen at most once across restarts: a nudge already sent
//! and a branch already deleted.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{App, AppContext as _, Global};

use coding::workflows::events::{self, Outcome, TrpcEventSink, WorkflowEventSink as _};
use coding::workflows::{
    self, Decision, IssueFacts, NodeFacts, SessionFacts, Snapshot, WorkflowFacts,
};
use coding::{LaunchOptions, LaunchOrigin};

use crate::coding_flow::{self, CodingHub, LocalSessions, SessionSubject};
use crate::queries;

/// The evaluation cadence — the automations host's beat, and the collection
/// watches cover every synced change in realtime anyway.
const BEAT: Duration = Duration::from_secs(workflows::BEAT_SECONDS);
/// The nudge-poll granularity inside the beat loop (device_sync's TICK).
const TICK: Duration = Duration::from_secs(1);

/// What a node whose PR no longer merges is told.
fn conflict_prompt(integration_branch: &str) -> String {
    format!(
        "The integration branch moved and your pull request no longer merges. Run git fetch \
origin, git merge origin/{integration_branch}, resolve the conflicts, push, then end the run \
again."
    )
}

#[derive(Default)]
struct WorkflowHostState {
    /// Stop flag per account (sign-out flips it; the loop retires itself).
    by_account: HashMap<String, Arc<AtomicBool>>,
    /// The shape watches per account.
    watch_by_account: HashMap<String, Vec<gpui::Subscription>>,
    /// A synced-row change asked for an off-cadence evaluation.
    eval_soon: Arc<AtomicBool>,
    /// Node ids with a start/land/resume this process has not finished, and
    /// workflow ids with a final PR in flight. Shared with the background
    /// pass, which is the only writer.
    in_flight: Arc<Mutex<HashSet<String>>>,
    final_pr_in_flight: Arc<Mutex<HashSet<String>>>,
}

struct WorkflowHostGlobal(gpui::Entity<WorkflowHostState>);
impl Global for WorkflowHostGlobal {}

fn state(cx: &mut App) -> gpui::Entity<WorkflowHostState> {
    if let Some(global) = cx.try_global::<WorkflowHostGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| WorkflowHostState::default());
    cx.set_global(WorkflowHostGlobal(entity.clone()));
    entity
}

/// A guard that frees its in-flight key however the work ends.
struct InFlight {
    set: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl InFlight {
    fn claim(set: &Arc<Mutex<HashSet<String>>>, key: &str) -> Option<InFlight> {
        let mut guard = set.lock().ok()?;
        if !guard.insert(key.to_string()) {
            return None;
        }
        Some(InFlight {
            set: Arc::clone(set),
            key: key.to_string(),
        })
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.set.lock() {
            guard.remove(&self.key);
        }
    }
}

fn snapshot_of(set: &Arc<Mutex<HashSet<String>>>) -> HashSet<String> {
    set.lock().map(|guard| guard.clone()).unwrap_or_default()
}

/// Start the host for `account` (from `session::connect_account`, beside
/// [`crate::automation_host::start_automation_host`]). Restarting for the
/// same account replaces the old loop.
pub fn start_workflow_host(account: &api::Account, cx: &mut App) {
    let state_entity = state(cx);
    let account_id = account.id.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let (eval_soon, in_flight, final_pr_in_flight) = state_entity.update(cx, |state, _| {
        if let Some(previous) = state.by_account.insert(account_id.clone(), stop.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        (
            state.eval_soon.clone(),
            Arc::clone(&state.in_flight),
            Arc::clone(&state.final_pr_in_flight),
        )
    });

    let watches = watch_collections(stop.clone(), eval_soon.clone(), cx);
    state_entity.update(cx, |state, _| {
        if watches.is_empty() {
            state.watch_by_account.remove(&account_id);
        } else {
            state.watch_by_account.insert(account_id.clone(), watches);
        }
    });

    cx.spawn(async move |cx| {
        // Evaluate on the first tick: a workflow started while this device
        // was offline must move as soon as the rows land.
        let mut ticks_since_beat = u32::MAX / 2;
        loop {
            cx.background_executor().timer(TICK).await;
            if stop.load(Ordering::SeqCst) {
                return;
            }
            ticks_since_beat = ticks_since_beat.saturating_add(1);
            let nudged = eval_soon.swap(false, Ordering::SeqCst);
            if !nudged && Duration::from_secs(ticks_since_beat as u64) < BEAT {
                continue;
            }
            ticks_since_beat = 0;

            // ONE foreground snapshot — every entity read happens here.
            let Some(passes) = cx.update(|cx| {
                snapshot_for(&account_id, &in_flight, &final_pr_in_flight, cx)
            }) else {
                // Account switched away — retire; connect_account restarts.
                return;
            };
            if passes.is_empty() {
                continue;
            }

            for pass in passes {
                let in_flight = Arc::clone(&in_flight);
                let final_pr_in_flight = Arc::clone(&final_pr_in_flight);
                let orders = cx
                    .background_executor()
                    .spawn(async move { run_pass(pass, &in_flight, &final_pr_in_flight) })
                    .await;
                // The foreground half of the pass: the resumes it queued go
                // out whether or not it also started anything.
                cx.update(|cx| {
                    drain_resumes(cx);
                    for start in orders.starts {
                        launch_node(start, cx);
                    }
                    // EXP-984: the reviewer runs the pass decided on.
                    for review in orders.reviews {
                        launch_review(review, cx);
                    }
                });
            }
        }
    })
    .detach();
}

/// Stop `account_id`'s host (from the sign-out paths).
pub fn stop_workflow_host(account_id: &str, cx: &mut App) {
    let state = state(cx);
    state.update(cx, |state, _| {
        if let Some(stop) = state.by_account.remove(account_id) {
            stop.store(true, Ordering::SeqCst);
        }
        state.watch_by_account.remove(account_id);
    });
}

/// Watch the collections an evaluation reads. Evaluation is idempotent and
/// every write is gated by the in-flight sets, so an eager pass costs one
/// settings read.
fn watch_collections(
    stop: Arc<AtomicBool>,
    eval_soon: Arc<AtomicBool>,
    cx: &mut App,
) -> Vec<gpui::Subscription> {
    let Some(collections) = sync::Store::try_global(cx).map(|store| store.collections().clone())
    else {
        return Vec::new(); // headless tests — the beat cadence still runs
    };
    vec![
        watch_flag(&collections.workflows, &stop, &eval_soon, cx),
        watch_flag(&collections.workflow_nodes, &stop, &eval_soon, cx),
        watch_flag(&collections.coding_sessions, &stop, &eval_soon, cx),
        watch_flag(&collections.issue_relations, &stop, &eval_soon, cx),
    ]
}

fn watch_flag<T: 'static>(
    entity: &gpui::Entity<T>,
    stop: &Arc<AtomicBool>,
    eval_soon: &Arc<AtomicBool>,
    cx: &mut App,
) -> gpui::Subscription {
    let stop = stop.clone();
    let eval_soon = eval_soon.clone();
    cx.observe(entity, move |_, _| {
        if !stop.load(Ordering::SeqCst) {
            eval_soon.store(true, Ordering::SeqCst);
        }
    })
}

// ---------------------------------------------------------------------------
// The foreground snapshot
// ---------------------------------------------------------------------------

/// Everything ONE workflow's pass needs, read on the foreground.
struct Pass {
    trpc: Arc<api::TrpcClient>,
    settings_path: PathBuf,
    repos_root: PathBuf,
    device_id: String,
    /// The workflow's repository and the board its token mint resolves the
    /// default branch through (the first node's issue's board).
    repository_id: Option<String>,
    board_id: Option<String>,
    /// What the node runs launch with, already layered over this machine's
    /// defaults.
    options: LaunchOptions,
    /// `workflows.decisions` as synced — every node prompt carries it.
    decisions: String,
    name: String,
    snapshot: Snapshot,
    /// The node's representative issue id, by node id — a start needs it.
    issue_of_node: HashMap<String, String>,
    /// A compound node's member issues, by node id.
    members_of_node: HashMap<String, Vec<String>>,
    /// Live engine handles by session id: `steer` and `kill` are channel
    /// sends, so the background pass can drive them directly.
    engines: HashMap<String, engine::EngineSession>,
    /// Session ids whose run this process does NOT host (another device, or
    /// a run this app did not launch) — they can only be reported on.
    session_is_local: HashSet<String>,
    /// EXP-983: the node pairs worth a `git merge-tree` this beat.
    candidates: Vec<(String, String)>,
    /// EXP-983: `node id → its head branch`, for the collision test and for
    /// naming a conflicting pair.
    branch_of: HashMap<String, String>,
    /// EXP-984: the workflow's team — a reviewer run is a builtin action
    /// run, and those rows are keyed by team.
    team_id: String,
    /// EXP-984: `node id → its latest review`, whose findings the author is
    /// handed verbatim.
    review_of: HashMap<String, domain::rows::WorkflowNodeReview>,
    /// EXP-984: `node id → whether its run announced a contract`, the input
    /// to the `contractChanges` metric.
    checkpointed: HashSet<String>,
    /// EXP-984: `reviewer session id → live` off the synced rows, for every
    /// reviewer run this device recorded; a row that has not synced is
    /// absent. What settles a review that ended without a verdict.
    review_session_live: HashMap<String, bool>,
    /// `node id → the LIVE run on its review branch`, whatever id was
    /// recorded: a resumed reviewer is followed, a stray one adopted, and
    /// neither is doubled ([`workflows::live_pending_reviews_on_branches`]).
    review_live_on_branch: HashMap<String, String>,
}

/// One node the pass decided to start — executed on the foreground, where
/// the window and the launch plumbing live.
struct StartOrder {
    workflow_id: String,
    workflow_name: String,
    decisions: String,
    /// EXP-983: the workflow's start mode and the issues this node builds
    /// on — the two extra prompt lines.
    start_on: String,
    blockers: Vec<String>,
    /// The branch this node is cut from: the integration branch, a blocker's
    /// own branch, or a synthetic base of several (EXP-983).
    integration_branch: String,
    node_id: String,
    issue_id: String,
    member_issue_ids: Vec<String>,
    /// Resolved on the background pass for a COMPOUND node only — a batch
    /// launch names its repository itself (an issue launch resolves it
    /// inside `prepare`).
    repo: Option<api::repositories::IssueRepository>,
    options: LaunchOptions,
    trpc: Arc<api::TrpcClient>,
    in_flight: Option<Arc<InFlight>>,
}

/// One node whose PR would not merge and whose run has ENDED: the resume
/// happens on the foreground.
struct ResumeOrder {
    session_id: String,
    prompt: String,
    in_flight: Option<Arc<InFlight>>,
}

/// EXP-984: the in-flight key one node's REVIEW claims — deliberately
/// prefixed, so it can never be mistaken for the node's own start/land claim.
const REVIEW_CLAIM_PREFIX: &str = "review:";

/// EXP-984 — one agent review the pass decided to start. Like a node start
/// it is launched on the FOREGROUND, where the window and the launch
/// plumbing live; unlike one it is an ACTION run (the hidden `Review node`
/// builtin) in a throwaway worktree of its own.
struct ReviewOrder {
    workflow_id: String,
    node_id: String,
    team_id: String,
    repo: api::repositories::IssueRepository,
    identifier: String,
    /// The node's own pushed branch, what the review is cut from.
    node_branch: String,
    /// What the node itself was based on — the diff the reviewer reads.
    base_branch: String,
    /// The local, never-pushed branch this review works on.
    review_branch: String,
    /// The node's review round when this review was decided: a run that
    /// ends with the round still there submitted no verdict.
    round_at_launch: i64,
    adversarial: bool,
    options: LaunchOptions,
    settings_path: PathBuf,
    device_id: String,
    in_flight: Option<Arc<InFlight>>,
}

/// What one pass hands back to the foreground.
#[derive(Default)]
struct PassOrders {
    starts: Vec<StartOrder>,
    reviews: Vec<ReviewOrder>,
}

fn snapshot_for(
    account_id: &str,
    in_flight: &Arc<Mutex<HashSet<String>>>,
    final_pr_in_flight: &Arc<Mutex<HashSet<String>>>,
    cx: &mut App,
) -> Option<Vec<Pass>> {
    let account = queries::active_account(cx)?;
    if account.id != account_id {
        return None;
    }
    let auth = crate::session::AuthContext::global(cx);
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let settings_path = coding::Settings::default_path(&auth.data_dir);
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let hub = CodingHub::global(cx);
    let sessions = LocalSessions::global(cx);

    let settings = hub.read(cx).settings.clone();
    let repos_root = settings.repos_root_path();
    let local = sessions.read(cx);
    let workflows_rows = collections.workflows.read(cx);
    let node_rows = collections.workflow_nodes.read(cx);
    let issue_rows = collections.issues.read(cx);
    let session_rows = collections.coding_sessions.read(cx);
    let relation_rows = collections.issue_relations.read(cx);

    let now_ms = chrono::Local::now().timestamp_millis();
    let claimed = snapshot_of(in_flight);
    let final_claimed = snapshot_of(final_pr_in_flight);

    let mut passes = Vec::new();
    for workflow in workflows_rows.iter() {
        // SINGLE WRITER: another machine's (or the daemon's) workflow.
        if workflow.device_id.as_deref() != Some(device_id.as_str()) {
            continue;
        }
        let status = workflow.status_wire();
        if !matches!(status, "running" | "paused" | "cancelled") {
            continue;
        }
        let Some(integration_branch) = workflow.integration_branch.clone() else {
            continue; // no branch to land on: nothing to evaluate
        };
        // EXP-1029: the stored launch of ANY vintage → the two models every
        // run of this workflow reads. Effort is the device's own default.
        // The normalizer reads the RAW jsonb, never a round trip through the
        // wire struct: ONE ill-typed legacy key there (an old `maxParallel`
        // stored as a string) would drop the WHOLE launch to the defaults.
        let launch = workflows::launch::normalize_workflow_launch(
            workflow.launch.as_ref().unwrap_or(&serde_json::Value::Null),
        );
        let engine_state = workflows::read_states(&settings_path, &device_id)
            .get(&workflow.id)
            .cloned()
            .unwrap_or_default();
        let mut nodes = Vec::new();
        // EXP-983: the identifier names a node's synthetic base, and the
        // branches + globs feed the collision pre-filter.
        let mut identifier: HashMap<String, String> = HashMap::new();
        let mut git_nodes: Vec<workflows::NodeGit> = Vec::new();
        let mut issue_of_node = HashMap::new();
        let mut members_of_node = HashMap::new();
        let mut issues: HashMap<String, IssueFacts> = HashMap::new();
        let mut sessions_facts: HashMap<String, SessionFacts> = HashMap::new();
        let mut engines = HashMap::new();
        let mut session_is_local = HashSet::new();
        let mut edge_nodes = Vec::new();
        let mut board_id = None;
        let mut review_of: HashMap<String, domain::rows::WorkflowNodeReview> = HashMap::new();
        let mut checkpointed: HashSet<String> = HashSet::new();
        // EXP-984: a REVIEWER run still up keeps its node out of the review
        // rule — its session id is the one this device recorded when it
        // started that review.
        let mut review_in_flight: HashSet<String> = claimed
            .iter()
            .filter_map(|key| key.strip_prefix(REVIEW_CLAIM_PREFIX).map(str::to_string))
            .collect();
        let mut review_session_live: HashMap<String, bool> = HashMap::new();
        for (node_id, session_id) in &engine_state.review_runs {
            let Some(row) = session_rows.get(session_id.as_str()) else {
                continue; // not synced yet: neither live nor ended
            };
            let live = matches!(row.status.as_deref(), Some("running" | "in_review"));
            review_session_live.insert(session_id.clone(), live);
            if live {
                review_in_flight.insert(node_id.clone());
            }
        }
        for node in node_rows.iter() {
            if node.workflow_id.as_deref() != Some(workflow.id.as_str()) {
                continue;
            }
            let Some(issue_id) = node.issue_id.clone() else {
                continue;
            };
            // EXP-984: a node nobody admitted is not part of the run at all.
            if node.state_wire() == domain::contract::WF_NODE_STATE_PROPOSED {
                continue;
            }
            if let Some(review) = node.review_facts() {
                review_of.insert(node.id.clone(), review);
            }
            if node.checkpoint_at.is_some() {
                checkpointed.insert(node.id.clone());
            }
            let members = node.member_ids();
            // A plain node's head is its issue's branch; a COMPOUND one runs
            // as a batch, whose branch only the session row knows.
            let mut branch = None;
            if let Some(issue) = issue_rows.get(&issue_id) {
                board_id.get_or_insert_with(|| issue.board_id.clone());
                identifier.insert(node.id.clone(), issue.identifier.clone());
                // The issue's branch is stamped when its PR opens; before
                // that a started run is already pushing to the launcher's
                // conventional name (which the tips then confirm).
                branch = issue.branch.clone().or_else(|| {
                    node.session_id
                        .is_some()
                        .then(|| workflows::conventional_branch(&issue.identifier))
                });
                issues.insert(
                    issue_id.clone(),
                    IssueFacts {
                        pr_state: issue.pr_state.clone(),
                    },
                );
            }
            if let Some(session_id) = node.session_id.as_deref() {
                if let Some(row) = session_rows.get(session_id) {
                    sessions_facts.insert(session_id.to_string(), session_facts(row));
                    if !members.is_empty() {
                        branch = row.branch.clone().or(branch);
                    }
                }
                if let Some(live) = local.session_for_id(session_id) {
                    engines.insert(session_id.to_string(), live.host.session.clone());
                    session_is_local.insert(session_id.to_string());
                }
            }
            git_nodes.push(workflows::NodeGit {
                id: node.id.clone(),
                branch: branch.clone(),
                touches: node.touches.clone(),
                state: node.state_wire().to_string(),
            });
            edge_nodes.push((
                node.id.clone(),
                issue_id.clone(),
                members.clone(),
                node.after_ids(),
            ));
            issue_of_node.insert(node.id.clone(), issue_id.clone());
            members_of_node.insert(node.id.clone(), members.clone());
            nodes.push(NodeFacts {
                id: node.id.clone(),
                issue_id,
                member_issue_ids: members,
                kind: node.kind_wire().to_string(),
                state: node.state_wire().to_string(),
                risk: node.risk_wire().to_string(),
                wave: node.wave_index() as i64,
                lane: node.lane_index() as i64,
                session_id: node.session_id.clone(),
                attempt: node.attempt.unwrap_or(0),
                approved_at: node.approved_at.clone(),
                checkpoint_at: node.checkpoint_at.clone(),
                base_branch: node.base_branch.clone(),
                after_node_ids: node.after_ids(),
                branch,
                // EXP-984: the review gate's counter and verdict, and what
                // the node may spend.
                review_round: node.review_count(),
                review: node.review_facts().map(|review| workflows::ReviewFacts {
                    verdict: review.verdict,
                    round: review.round,
                    head: review.head,
                }),
                updated_at_ms: node
                    .updated_at
                    .as_deref()
                    .and_then(workflows::parse_wire_timestamp_ms),
            });
        }
        if nodes.is_empty() {
            continue;
        }
        let edges = workflow_edges(&edge_nodes, relation_rows.iter());
        let candidates = workflows::conflict_candidates(&git_nodes, &edges);
        let branch_of: HashMap<String, String> = git_nodes
            .iter()
            .filter_map(|node| Some((node.id.clone(), node.branch.clone()?)))
            .collect();
        // A reviewer that was resumed runs on the SAME review branch under
        // a new session id: the branch, not the recorded id, says which
        // nodes are being reviewed right now. Only the PENDING round's
        // reviewer counts: an older round's lingering run is not one.
        let review_round_of: HashMap<String, i64> = nodes
            .iter()
            .map(|node| (node.id.clone(), node.review_round))
            .collect();
        let review_live_on_branch = workflows::live_pending_reviews_on_branches(
            &workflow.id,
            &identifier,
            &review_round_of,
            live_session_branches(session_rows.iter()),
        );
        review_in_flight.extend(review_live_on_branch.keys().cloned());
        passes.push(Pass {
            trpc: Arc::clone(&trpc),
            settings_path: settings_path.clone(),
            repos_root: repos_root.clone(),
            device_id: device_id.clone(),
            repository_id: workflow.repository_id.clone(),
            board_id,
            options: workflows::launch_options(
                &settings,
                Some(launch.agent.as_str()),
                // The per-node model rides on each decision; the subagents
                // inside every run take the CHEAP one.
                None,
                None,
                Some(launch.model.as_str()),
                launch.account.as_deref(),
            ),
            decisions: workflow.decisions.clone().unwrap_or_default(),
            name: workflow.name.clone().unwrap_or_default(),
            snapshot: Snapshot {
                workflow: WorkflowFacts {
                    id: workflow.id.clone(),
                    status: status.to_string(),
                    integration_branch,
                    final_pr_url: workflow.final_pr_url.clone(),
                    // EXP-1029: not a launch field any more.
                    max_parallel: domain::contract::WORKFLOW_MAX_PARALLEL_DEFAULT,
                    start_on: workflow
                        .start_on
                        .clone()
                        .unwrap_or_else(|| workflows::START_ON_LANDED.to_string()),
                    launch: launch.clone(),
                },
                nodes,
                edges,
                issues,
                sessions: sessions_facts,
                // The engine only creates the branch it cannot see; the host
                // half is idempotent, so "not known yet" is safe to retry.
                integration_branch_exists: false,
                in_flight: claimed.clone(),
                final_pr_in_flight: final_claimed.contains(&workflow.id),
                nudged: engine_state.nudged.clone(),
                // EXP-983: the git facts are filled on the background pass,
                // where the clone and the token live.
                tips: HashMap::new(),
                propagated: engine_state.propagated.clone(),
                synthetic: engine_state.synthetic.clone(),
                conflicts: HashSet::new(),
                identifier,
                // EXP-984: the review gate's at-most-once bookkeeping. The
                // heads themselves are filled with the tips, below.
                review_in_flight,
                pr_head: HashMap::new(),
                reviewed_head: engine_state.reviewed_head.clone(),
                findings_sent: engine_state.findings_sent.clone(),
                land_refused: engine_state.land_refused.clone(),
                now_ms,
            },
            issue_of_node,
            members_of_node,
            engines,
            session_is_local,
            candidates,
            branch_of,
            team_id: workflow.team_id.clone().unwrap_or_default(),
            review_of,
            checkpointed,
            review_session_live,
            review_live_on_branch,
        });
    }
    Some(passes)
}

/// This team's LIVE runs as `(session id, branch)`, oldest first — what the
/// branch-based reviewer lookup reads.
fn live_session_branches<'a>(
    rows: impl Iterator<Item = &'a domain::rows::CodingSession>,
) -> Vec<(String, String)> {
    let mut live: Vec<&domain::rows::CodingSession> = rows
        .filter(|row| {
            row.branch.is_some() && matches!(row.status.as_deref(), Some("running" | "in_review"))
        })
        .collect();
    live.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    live.into_iter()
        .filter_map(|row| Some((row.id.clone(), row.branch.clone()?)))
        .collect()
}

/// The `blocks` edges between a workflow's nodes — the ONE rule, shared with
/// every client's graph ([`domain::workflow_view::workflow_edges`]).
fn workflow_edges<'a>(
    nodes: &[(String, String, Vec<String>, Vec<String>)],
    relations: impl Iterator<Item = &'a domain::rows::IssueRelation>,
) -> Vec<(String, String)> {
    let edge_nodes: Vec<domain::workflow_view::EdgeNode<'_>> = nodes
        .iter()
        .map(|(id, issue_id, members, after)| domain::workflow_view::EdgeNode {
            id,
            issue_id,
            member_issue_ids: members.iter().map(String::as_str).collect(),
            // EXP-983: a serialization edge blocks the same way a `blocks`
            // relation does — the engine's own ordering, honoured here.
            after_node_ids: after.iter().map(String::as_str).collect(),
        })
        .collect();
    let edge_relations: Vec<domain::workflow_view::EdgeRelation<'_>> = relations
        .map(|relation| domain::workflow_view::EdgeRelation {
            kind: relation.kind.as_deref().unwrap_or_default(),
            issue_id: &relation.issue_id,
            related_issue_id: &relation.related_issue_id,
        })
        .collect();
    domain::workflow_view::workflow_edges(&edge_nodes, &edge_relations, &[])
        .into_iter()
        .map(|edge| (edge.from, edge.to))
        .collect()
}

/// One synced `coding_sessions` row as the engine reads it. `blocked`
/// (EXP-804 jsonb `{kind, agent, window, resetsAt, since}`) is orthogonal to
/// the status: a walled run still reads `running`.
fn session_facts(row: &domain::rows::CodingSession) -> SessionFacts {
    let status = row.status.as_deref().unwrap_or("");
    let blocked = row.blocked.as_ref().filter(|value| !value.is_null());
    SessionFacts {
        live: matches!(status, "running" | "in_review"),
        needs_input: row.needs_input.unwrap_or(false),
        blocked: blocked.is_some(),
        blocked_resets_at_ms: blocked
            .and_then(|value| value.get("resetsAt"))
            .and_then(parse_resets_at),
        agent_busy: row.agent_busy.unwrap_or(false),
    }
}

/// The wall's reset stamp as ms epoch — a number already, or the ISO string
/// the agent reported. Anything else means "we do not know when", and the
/// engine then simply never nudges.
fn parse_resets_at(value: &serde_json::Value) -> Option<i64> {
    if let Some(number) = value.as_i64() {
        // Seconds vs milliseconds: a plausible epoch in seconds is ~1e9.
        return Some(if number < 100_000_000_000 { number * 1000 } else { number });
    }
    let raw = value.as_str()?;
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

// ---------------------------------------------------------------------------
// The pass (background executor)
// ---------------------------------------------------------------------------

/// Evaluate ONE workflow and execute everything that is not a launch,
/// returning the starts the foreground must do.
fn run_pass(
    pass: Pass,
    in_flight: &Arc<Mutex<HashSet<String>>>,
    final_pr_in_flight: &Arc<Mutex<HashSet<String>>>,
) -> PassOrders {
    let workflow_id = pass.snapshot.workflow.id.clone();
    let branch = pass.snapshot.workflow.integration_branch.clone();
    let mut snapshot = pass.snapshot.clone();
    // Rule 0's host half is idempotent AND cheap after the first pass (the
    // branch fetch is a cache hit), so it runs here rather than round-
    // tripping a decision: everything below then sees a real branch.
    if snapshot.workflow.status == "running" {
        match ensure_branch(&pass.trpc, &pass.repos_root, &pass.repository_id, pass.board_id.as_deref(), &branch) {
            Ok(()) => snapshot.integration_branch_exists = true,
            Err(err) => {
                log::warn!("[workflows] {workflow_id}: integration branch {branch} — {err}");
                return PassOrders::default();
            }
        }
    }
    // EXP-983: what moved and what collides — one ls-remote per repository
    // per beat, plus the merge-tree tests a quiet beat does not need.
    let repo = engine_repo(&pass);
    let mut state = read_state(&pass, &workflow_id);
    if let Some(repo) = repo.as_ref() {
        snapshot.tips = git_tips(repo);
        snapshot.conflicts = workflows::detect_conflicts(
            &repo.clone_path,
            &pass.candidates,
            &pass.branch_of,
            &snapshot.tips,
            &mut state.conflicts,
            Some(&repo.url),
        );
        workflows::prune_conflict_cache(
            &mut state.conflicts,
            &pass.candidates,
            &pass.branch_of,
            &snapshot.tips,
        );
    }
    // Unconditional: a beat that could not read the remote has no branches
    // to speculate on at all, which leaves those nodes blocked.
    workflows::confine_branches_to_tips(&mut snapshot);
    // The host bookkeeping the engine reads, settled against this beat's
    // rows and tips: a reviewer run that ended without a verdict releases
    // its head (bounded), a session this host is resuming reads live, and a
    // land refusal is forgotten once the head moved.
    let review_round_of: HashMap<String, i64> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.review_round))
        .collect();
    for outcome in workflows::settle_review_runs(
        &mut state,
        &review_round_of,
        &pass.review_live_on_branch,
        |session_id| pass.review_session_live.get(session_id).copied(),
        snapshot.now_ms,
    ) {
        match outcome {
            workflows::ReviewRunEnd::Verdict { .. } => {}
            workflows::ReviewRunEnd::Followed { node_id, session_id } => log::info!(
                "[workflows] {workflow_id}: the review of {node_id} was resumed as {session_id}; following it"
            ),
            workflows::ReviewRunEnd::Adopted { node_id, session_id } => log::info!(
                "[workflows] {workflow_id}: adopting the live review {session_id} of {node_id}"
            ),
            workflows::ReviewRunEnd::Retry { node_id, failures } => log::warn!(
                "[workflows] {workflow_id}: the review of {node_id} ended without a verdict ({failures}); trying again"
            ),
            workflows::ReviewRunEnd::GaveUp { node_id, note } => {
                log::warn!("[workflows] {workflow_id}: {node_id} — {note}");
                let mut report = api::workflows::NodeReport::new(&node_id, "in_review");
                report.note = api::patch::Patch::Set(one_line(&note));
                report_node(&pass.trpc, &report);
            }
        }
    }
    snapshot.reviewed_head = state.reviewed_head.clone();
    // FEED-49: the map as this pass READ it — a person's resume may have
    // taken a hold since (off the foreground, while the git calls above
    // ran), and the write-back below must keep it.
    let resuming_before = state.resuming.clone();
    workflows::apply_resuming(&mut snapshot, &mut state.resuming, &state.review_runs);
    workflows::prune_land_refused(&mut state.land_refused, &snapshot.pr_head);
    snapshot.land_refused = state.land_refused.clone();
    {
        let settled = state.clone();
        update_state(&pass, &workflow_id, move |persisted| {
            persisted.conflicts = settled.conflicts;
            persisted.reviewed_head = settled.reviewed_head;
            persisted.review_runs = settled.review_runs;
            persisted.review_rounds = settled.review_rounds;
            persisted.review_failures = settled.review_failures;
            workflows::merge_resuming(&mut persisted.resuming, &resuming_before, settled.resuming);
            persisted.land_refused = settled.land_refused;
        });
    }

    // EXP-984: the counters only this device can see, batched into ONE
    // report at the end of the pass.
    let mut metrics: std::collections::BTreeMap<String, u32> = Default::default();
    tally_contract_changes(&pass, &snapshot, &workflow_id, &mut metrics);

    let decisions = workflows::evaluate(&snapshot);
    let mut orders = PassOrders::default();
    let mut resumes: Vec<ResumeOrder> = Vec::new();
    // A base this pass could NOT put up. Nothing may be cut from it: the
    // node waits for the next beat rather than starting on a wrong base.
    let mut unbuilt: HashSet<String> = HashSet::new();
    let sink = TrpcEventSink::new(Arc::clone(&pass.trpc));
    for decision in decisions {
        // EXP-1082: what the host did with it, for the audit trail.
        let decided = decision.clone();
        let outcome: Outcome = 'decision: {
            match decision {
                // Already done above — the engine still emits it when the host
                // could not confirm the branch, and then there is nothing else.
                Decision::EnsureIntegrationBranch => {}
                Decision::SetNodeState {
                    node_id,
                    state,
                    note,
                } => {
                    let mut report = api::workflows::NodeReport::new(&node_id, &state);
                    report.note = match note {
                        Some(note) => api::patch::Patch::Set(note),
                        None => api::patch::Patch::Null,
                    };
                    report_node(&pass.trpc, &report);
                }
                // EXP-983: the synthetic base a speculative start needs, built
                // (or merged forward) in the engine's own scratch worktree.
                Decision::BuildBase {
                    node_id,
                    base_branch,
                    sources,
                    ..
                } => {
                    let Some(repo) = repo.as_ref() else {
                        unbuilt.insert(base_branch);
                        break 'decision Outcome::Skipped;
                    };
                    if !build_base(&pass, repo, &workflow_id, &node_id, &base_branch, &sources) {
                        unbuilt.insert(base_branch);
                        break 'decision Outcome::Failed("the base did not build".to_string());
                    }
                }
                Decision::StartNode {
                    node_id,
                    attempt,
                    base_branch,
                    model,
                    workflow_id: _,
                    role,
                    account,
                } => {
                    // The base did not go up this pass: never cut from it.
                    if unbuilt.contains(&base_branch) {
                        break 'decision Outcome::Skipped;
                    }
                    let Some(issue_id) = pass.issue_of_node.get(&node_id).cloned() else {
                        break 'decision Outcome::Skipped;
                    };
                    let members = pass
                        .members_of_node
                        .get(&node_id)
                        .cloned()
                        .unwrap_or_default();
                    // A COMPOUND node runs as one batch, which needs the
                    // repository's full name the way the launcher resolves it.
                    let repo = if members.is_empty() {
                        None
                    } else {
                        match api::repositories::for_issue(&pass.trpc, &issue_id) {
                            Ok(Some(repo)) => Some(repo),
                            Ok(None) | Err(_) => {
                                log::warn!(
                                    "[workflows] {workflow_id}: node {node_id} has no repository"
                                );
                                break 'decision Outcome::Skipped;
                            }
                        }
                    };
                    let Some(claim) = InFlight::claim(in_flight, &node_id) else {
                        break 'decision Outcome::Skipped;
                    };
                    // Persist BEFORE the launch (the automations rule): a crash
                    // between here and the spawn leaves a `running` node the
                    // next pass re-decides, never a silent double start.
                    let mut report = api::workflows::NodeReport::new(&node_id, "running");
                    report.attempt = Some(attempt);
                    report.base_branch = api::patch::Patch::Set(base_branch.clone());
                    report.note = api::patch::Patch::Null;
                    if !report_node(&pass.trpc, &report) {
                        break 'decision Outcome::Skipped;
                    }
                    // EXP-983: the run is cut AT this tip, so it already has it —
                    // recording that is what keeps the first beat quiet.
                    if let Some(sha) = snapshot.tips.get(&base_branch).cloned() {
                        remember_propagated(&pass, &workflow_id, &node_id, &base_branch, &sha);
                    }
                    // EXP-1002: the node's PHASE picks the model; everything
                    // else (agent, effort, account, subagent model) is the
                    // workflow's own launch configuration — the review's rule.
                    let mut options = pass.options.clone();
                    if let Some(model) = model {
                        options.model = model;
                    }
                    // EXP-1082: the row names its workflow, node and role;
                    // EXP-1005's rotation may pick the account.
                    options.workflow = Some(workflows::WorkflowMembership {
                        workflow_id: workflow_id.clone(),
                        node_id: node_id.clone(),
                        role,
                    });
                    if let Some(account) = account.or_else(|| start_account(&options, snapshot.now_ms)) {
                        options.account = Some(account);
                    }
                    orders.starts.push(StartOrder {
                        workflow_id: workflow_id.clone(),
                        workflow_name: pass.name.clone(),
                        decisions: pass.decisions.clone(),
                        start_on: snapshot.workflow.start_on.clone(),
                        blockers: blocker_identifiers(&snapshot, &node_id),
                        integration_branch: base_branch,
                        node_id,
                        issue_id,
                        member_issue_ids: members,
                        repo,
                        options,
                        trpc: Arc::clone(&pass.trpc),
                        in_flight: Some(Arc::new(claim)),
                    });
                }
                Decision::LandNode { node_id } => {
                    let Some(claim) = InFlight::claim(in_flight, &node_id) else {
                        break 'decision Outcome::Skipped;
                    };
                    land(&pass, &snapshot, &node_id, &branch, claim, &mut resumes);
                }
                // EXP-983: the branch under a run moved. A live run takes the
                // text where it stands; an ended one is resumed with it.
                Decision::MergeUpstream {
                    node_id,
                    session_id,
                    base_branch,
                    sha,
                } => {
                    let note = match repo.as_ref() {
                        Some(repo) => movement_note(repo, &pass, &snapshot, &node_id, &base_branch, &sha),
                        None => sha.clone(),
                    };
                    let text = coding::prompt::upstream_moved_prompt(&base_branch, &note);
                    remember_propagated(&pass, &workflow_id, &node_id, &base_branch, &sha);
                    // EXP-984: a merge-in is counted when it is DELIVERED, not
                    // when it is decided.
                    let mut delivered = || {
                        *metrics
                            .entry(api::workflows::COUNTER_MERGE_INS.to_string())
                            .or_default() += 1;
                    };
                    if let Some(engine) = pass.engines.get(&session_id) {
                        engine.steer(text);
                        delivered();
                        break 'decision Outcome::Skipped;
                    }
                    if pass.session_is_local.contains(&session_id) {
                        break 'decision Outcome::Skipped; // a live run this app hosts but cannot reach
                    }
                    delivered();
                    remember_resuming(&pass, &workflow_id, &session_id, snapshot.now_ms);
                    resumes.push(ResumeOrder {
                        session_id,
                        prompt: text,
                        in_flight: None,
                    });
                }
                // EXP-984: the agent review of one node — an ACTION run in a
                // throwaway worktree, launched on the foreground.
                Decision::StartReview {
                    node_id,
                    model,
                    adversarial,
                    workflow_id: _,
                    role,
                    account,
                } => {
                    let Some(order) = review_order(
                        &pass,
                        &snapshot,
                        &node_id,
                        model,
                        adversarial,
                        role,
                        account,
                        in_flight,
                    ) else {
                        break 'decision Outcome::Skipped;
                    };
                    // The head this review runs against is recorded NOW: the
                    // next beat must not start a second review of the same push
                    // while this one is still coming up.
                    if let Some(sha) = snapshot.pr_head.get(&node_id).cloned() {
                        update_state(&pass, &workflow_id, |state| {
                            state.reviewed_head.insert(node_id.clone(), sha.clone());
                        });
                    }
                    orders.reviews.push(order);
                }
                // EXP-984: the review asked for changes — its findings go to the
                // node's AUTHOR verbatim, once per round.
                Decision::SendFindings {
                    node_id,
                    session_id,
                } => {
                    let Some(review) = pass.review_of.get(&node_id) else {
                        break 'decision Outcome::Skipped;
                    };
                    let text = coding::prompt::review_findings_prompt(review.round, &review.findings);
                    match session_id {
                        // The engine only names a session that is LIVE: steer it
                        // where it stands, or say nothing at all this pass (a
                        // live run must never be resumed into a second one).
                        Some(session_id) => match pass.engines.get(&session_id) {
                            Some(engine) => engine.steer(text),
                            None => break 'decision Outcome::Skipped,
                        },
                        // Its run ENDED: re-enter it with the findings as its
                        // first message, exactly like the conflict path.
                        None => {
                            let Some(session_id) = pass
                                .snapshot
                                .nodes
                                .iter()
                                .find(|node| node.id == node_id)
                                .and_then(|node| node.session_id.clone())
                            else {
                                break 'decision Outcome::Skipped;
                            };
                            remember_resuming(&pass, &workflow_id, &session_id, snapshot.now_ms);
                            resumes.push(ResumeOrder {
                                session_id,
                                prompt: text,
                                in_flight: None,
                            });
                        }
                    }
                    let round = review.round;
                    update_state(&pass, &workflow_id, |state| {
                        state.findings_sent.insert(node_id.clone(), round);
                    });
                }
                // EXP-983: the collision the engine decided to serialize. The
                // state rides along unchanged — `reportNode` always takes one.
                Decision::SetSerialEdge {
                    node_id,
                    state,
                    after,
                } => {
                    let mut report = api::workflows::NodeReport::new(&node_id, state);
                    report.after_node_ids = Some(after);
                    report_node(&pass.trpc, &report);
                }
                Decision::Nudge {
                    session_id,
                    key,
                    text,
                } => {
                    if let Some(engine) = pass.engines.get(&session_id) {
                        engine.steer(text);
                        remember_nudge(&pass, &workflow_id, &session_id, &key);
                    }
                }
                Decision::OpenFinalPr => {
                    let Some(claim) = InFlight::claim(final_pr_in_flight, &workflow_id) else {
                        break 'decision Outcome::Skipped;
                    };
                    match api::workflows::open_final_pr(&pass.trpc, &workflow_id) {
                        Ok(url) => log::info!("[workflows] {workflow_id}: final pull request {url}"),
                        Err(err) => {
                            log::warn!("[workflows] {workflow_id}: final PR — {err}");
                            break 'decision Outcome::Failed(err.to_string());
                        }
                    }
                    drop(claim);
                }
                Decision::KillSession { session_id } => {
                    if let Some(engine) = pass.engines.get(&session_id) {
                        engine.kill("ended");
                    }
                }
                Decision::DeleteIntegrationBranch => {
                    if already_deleted(&pass, &workflow_id) {
                        break 'decision Outcome::Skipped;
                    }
                    match workflows::delete_integration_branch(
                        &pass.trpc,
                        &pass.repos_root,
                        pass.repository_id.as_deref().unwrap_or_default(),
                        pass.board_id.as_deref(),
                        &branch,
                    ) {
                        Ok(()) => {
                            log::info!("[workflows] {workflow_id}: deleted {branch}");
                            remember_branch_deleted(&pass, &workflow_id);
                        }
                        Err(err) => {
                            log::warn!("[workflows] {workflow_id}: delete {branch} — {err}");
                            break 'decision Outcome::Failed(err.to_string());
                        }
                    }
                }
                // EXP-983: a synthetic base nothing builds on any more.
                Decision::DeleteBase { base_branch } => {
                    let Some(repo) = repo.as_ref() else {
                        break 'decision Outcome::Skipped;
                    };
                    if read_state(&pass, &workflow_id)
                        .bases_deleted
                        .contains(&base_branch)
                    {
                        break 'decision Outcome::Skipped;
                    }
                    match workflows::delete_remote_branch(
                        &repo.clone_path,
                        &base_branch,
                        Some(&repo.url),
                    ) {
                        Ok(()) => {
                            log::info!("[workflows] {workflow_id}: deleted {base_branch}");
                            update_state(&pass, &workflow_id, |state| {
                                state.bases_deleted.insert(base_branch.clone());
                                state.synthetic.remove(&base_branch);
                            });
                        }
                        Err(err) => {
                            log::warn!("[workflows] {workflow_id}: delete {base_branch} — {err}");
                            break 'decision Outcome::Failed(err.to_string());
                        }
                    }
                }
            }
            Outcome::Done
        };
        if let Some(event) = events::event_for(&decided, &outcome) {
            sink.record(event);
        }
    }
    // A conflicted node whose run ENDED needs the foreground to relaunch it.
    for resume in resumes {
        RESUME_QUEUE.with_lock(resume);
    }
    // EXP-984: one metrics report per beat, never one per decision.
    if !metrics.is_empty() {
        if let Err(err) = api::workflows::report_metrics(&pass.trpc, &workflow_id, &metrics) {
            log::warn!("[workflows] {workflow_id}: reportMetrics — {err}");
        }
    }
    orders
}

/// EXP-984 — the `contractChanges` counter: a node that ALREADY announced its
/// contract moved its branch again, which is what every dependent then has to
/// merge in. The first tip seen after a checkpoint is the checkpoint itself,
/// so it is recorded and not counted.
fn tally_contract_changes(
    pass: &Pass,
    snapshot: &Snapshot,
    workflow_id: &str,
    metrics: &mut std::collections::BTreeMap<String, u32>,
) {
    let mut seen = read_state(pass, workflow_id).checkpoint_tips;
    let mut changed = 0_u32;
    let mut dirty = false;
    for node in &snapshot.nodes {
        if !pass.checkpointed.contains(&node.id) {
            continue;
        }
        let Some(sha) = snapshot.pr_head.get(&node.id) else {
            continue;
        };
        match seen.get(&node.id) {
            Some(previous) if previous == sha => continue,
            Some(_) => changed += 1,
            None => {}
        }
        seen.insert(node.id.clone(), sha.clone());
        dirty = true;
    }
    if changed > 0 {
        *metrics
            .entry(api::workflows::COUNTER_CONTRACT_CHANGES.to_string())
            .or_default() += changed;
    }
    if dirty {
        update_state(pass, workflow_id, |state| {
            state.checkpoint_tips = seen.clone()
        });
    }
}

/// EXP-984 — everything one review start needs, resolved on the background
/// pass. `None` = something has not synced yet; the next beat re-decides.
#[allow(clippy::too_many_arguments)]
fn review_order(
    pass: &Pass,
    snapshot: &Snapshot,
    node_id: &str,
    model: Option<String>,
    adversarial: bool,
    role: workflows::WfSessionRole,
    account: Option<String>,
    in_flight: &Arc<Mutex<HashSet<String>>>,
) -> Option<ReviewOrder> {
    let node = snapshot.nodes.iter().find(|node| node.id == node_id)?;
    let identifier = snapshot.identifier.get(node_id)?.clone();
    let node_branch = pass.branch_of.get(node_id)?.clone();
    let repo = match api::repositories::for_issue(&pass.trpc, &node.issue_id) {
        Ok(Some(repo)) => repo,
        Ok(None) | Err(_) => {
            log::warn!("[workflows] review of {node_id}: the node has no repository");
            return None;
        }
    };
    let claim = InFlight::claim(in_flight, &format!("{REVIEW_CLAIM_PREFIX}{node_id}"))?;
    let round = node.review_round + 1;
    let mut options = pass.options.clone();
    // The reviewer's model is the engine's pick; everything else (agent,
    // effort, account) is the workflow's own launch configuration.
    if let Some(model) = model {
        options.model = model;
    }
    // EXP-1082: the reviewer's row names its workflow node.
    options.workflow = Some(workflows::WorkflowMembership {
        workflow_id: snapshot.workflow.id.clone(),
        node_id: node_id.to_string(),
        role,
    });
    if let Some(account) = account.or_else(|| start_account(&options, snapshot.now_ms)) {
        options.account = Some(account);
    }
    Some(ReviewOrder {
        workflow_id: snapshot.workflow.id.clone(),
        node_id: node_id.to_string(),
        team_id: pass.team_id.clone(),
        repo,
        identifier: identifier.clone(),
        node_branch,
        base_branch: node
            .base_branch
            .clone()
            .unwrap_or_else(|| snapshot.workflow.integration_branch.clone()),
        review_branch: workflows::review_branch(&snapshot.workflow.id, &identifier, round),
        round_at_launch: node.review_round,
        adversarial,
        options,
        settings_path: pass.settings_path.clone(),
        device_id: pass.device_id.clone(),
        in_flight: Some(Arc::new(claim)),
    })
}

/// EXP-1082 — the account-rotation seam before every ENGINE start: the
/// profile with the most headroom, or `None` (the workflow's own launch
/// account stands). EXP-1005: feed it `agent_usage::collect_now` — this host
/// holds no per-profile usage today, so the slice is empty.
fn start_account(options: &LaunchOptions, now_ms: i64) -> Option<String> {
    coding::account_rotation::pick_start_account(
        &[],
        options.agent,
        Some(options.model.as_str()).filter(|model| !model.is_empty()),
        now_ms,
    )
}

fn ensure_branch(
    trpc: &api::TrpcClient,
    repos_root: &std::path::Path,
    repository_id: &Option<String>,
    board_id: Option<&str>,
    branch: &str,
) -> Result<(), String> {
    let Some(repository_id) = repository_id.as_deref() else {
        return Err("the workflow's repository is gone".to_string());
    };
    workflows::ensure_integration_branch(trpc, repos_root, repository_id, board_id, branch)
}

/// The engine's clone of the workflow's repository, with a JIT token on it —
/// EXP-983's git half runs entirely inside it.
struct EngineRepo {
    clone_path: PathBuf,
    url: coding::git_worktree::TokenUrl,
}

fn engine_repo(pass: &Pass) -> Option<EngineRepo> {
    let repository_id = pass.repository_id.as_deref()?;
    match workflows::engine_clone(
        &pass.trpc,
        &pass.repos_root,
        repository_id,
        pass.board_id.as_deref(),
        &pass.snapshot.workflow.integration_branch,
    ) {
        Ok((clone_path, url, _)) => Some(EngineRepo { clone_path, url }),
        Err(err) => {
            log::warn!("[workflows] the engine clone is unavailable: {err}");
            None
        }
    }
}

/// `git ls-remote` for every `exp/*` branch — the ONE view of what moved.
fn git_tips(repo: &EngineRepo) -> HashMap<String, String> {
    match workflows::remote_tips(&repo.clone_path, workflows::TIPS_PATTERN, Some(&repo.url)) {
        Ok(tips) => tips,
        Err(err) => {
            log::warn!("[workflows] ls-remote failed: {err}");
            HashMap::new()
        }
    }
}

/// Build (or refresh) one node's synthetic base, answering whether the base
/// is now UP. A CONFLICT is not an error: nothing is pushed, the node waits,
/// and the engine serializes the two blockers on the next pass.
fn build_base(
    pass: &Pass,
    repo: &EngineRepo,
    workflow_id: &str,
    node_id: &str,
    base_branch: &str,
    sources: &[String],
) -> bool {
    let workspace = workflows::engine_worktree(&repo.clone_path, workflow_id);
    match workflows::build_base(
        &repo.clone_path,
        &workspace,
        base_branch,
        &pass.snapshot.workflow.integration_branch,
        sources,
        Some(&repo.url),
    ) {
        Ok(workflows::BaseOutcome::Built(built)) => {
            log::info!("[workflows] {workflow_id}: built {base_branch}");
            update_state(pass, workflow_id, |state| {
                state.synthetic.insert(base_branch.to_string(), built.clone());
                state.bases_deleted.remove(base_branch);
            });
            true
        }
        Ok(workflows::BaseOutcome::Conflict { left, right }) => {
            let mut report = api::workflows::NodeReport::new(node_id, "waiting");
            report.note = api::patch::Patch::Set(one_line(&conflicting_blockers_note(
                pass, &left, &right,
            )));
            report_node(&pass.trpc, &report);
            false
        }
        Err(err) => {
            // Visible on the node, like a conflict: a `ready` node whose
            // base never comes up would otherwise sit there without a word.
            log::warn!("[workflows] {workflow_id}: base {base_branch} — {err}");
            let mut report = api::workflows::NodeReport::new(node_id, "waiting");
            report.note = api::patch::Patch::Set(one_line(&format!(
                "Its base {base_branch} could not be built: {err}"
            )));
            report_node(&pass.trpc, &report);
            false
        }
    }
}

/// Why a node is waiting when its blockers cannot both be merged in.
fn conflicting_blockers_note(pass: &Pass, left: &str, right: &str) -> String {
    let name = |branch: &str| {
        pass.branch_of
            .iter()
            .find(|(_, candidate)| candidate.as_str() == branch)
            .and_then(|(node_id, _)| pass.snapshot.identifier.get(node_id).cloned())
            .unwrap_or_else(|| branch.to_string())
    };
    format!(
        "Its blockers {} and {} conflict; one has to merge the other in",
        name(left),
        name(right)
    )
}

/// The identifiers of the issues one node builds on — the prompt's
/// "You build on the work of …" line.
fn blocker_identifiers(snapshot: &Snapshot, node_id: &str) -> Vec<String> {
    let mut names: Vec<String> = snapshot
        .edges
        .iter()
        .filter(|(_, to)| to == node_id)
        .filter_map(|(from, _)| snapshot.identifier.get(from).cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The note a moved branch carries: the host's own summary of the range
/// between what the node was last told and where the branch is now.
fn movement_note(
    repo: &EngineRepo,
    pass: &Pass,
    snapshot: &Snapshot,
    node_id: &str,
    base_branch: &str,
    sha: &str,
) -> String {
    let from = snapshot
        .propagated
        .get(node_id)
        .and_then(|branches| branches.get(base_branch).cloned())
        .or_else(|| {
            // Never told anything yet: what the node's OWN branch is missing.
            pass.branch_of
                .get(node_id)
                .map(|branch| format!("refs/remotes/origin/{branch}"))
        });
    match from {
        Some(from) => {
            workflows::movement_note(&repo.clone_path, &from, sha, Some(&repo.url))
        }
        None => sha.to_string(),
    }
}

fn read_state(pass: &Pass, workflow_id: &str) -> workflows::WorkflowState {
    workflows::read_states(&pass.settings_path, &pass.device_id)
        .get(workflow_id)
        .cloned()
        .unwrap_or_default()
}

/// Read-modify-write one workflow's persisted engine state.
fn update_state(pass: &Pass, workflow_id: &str, edit: impl FnOnce(&mut workflows::WorkflowState)) {
    let mut states = workflows::read_states(&pass.settings_path, &pass.device_id);
    edit(states.entry(workflow_id.to_string()).or_default());
    if let Err(err) = workflows::write_states(&pass.settings_path, &pass.device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

fn remember_propagated(
    pass: &Pass,
    workflow_id: &str,
    node_id: &str,
    branch: &str,
    sha: &str,
) {
    update_state(pass, workflow_id, |state| {
        state
            .propagated
            .entry(node_id.to_string())
            .or_default()
            .insert(branch.to_string(), sha.to_string());
    });
}

fn report_node(trpc: &api::TrpcClient, report: &api::workflows::NodeReport) -> bool {
    match api::workflows::report_node(trpc, report) {
        Ok(()) => true,
        Err(err) => {
            log::warn!(
                "[workflows] reportNode {} → {} failed: {err}",
                report.node_id,
                report.state
            );
            false
        }
    }
}

/// The merge train's one step and what a refusal means (see the engine's
/// module doc): the gate saying "not yet" is nothing to do, anything else is
/// GitHub refusing the merge — the node has to merge the trunk in.
fn land(
    pass: &Pass,
    snapshot: &Snapshot,
    node_id: &str,
    branch: &str,
    claim: InFlight,
    resumes: &mut Vec<ResumeOrder>,
) {
    let outcome = match api::workflows::land_node(&pass.trpc, node_id) {
        Ok(outcome) => outcome,
        Err(err) => {
            log::warn!("[workflows] landNode {node_id} failed: {err}");
            return;
        }
    };
    if outcome.merged {
        log::info!("[workflows] landed {node_id}");
        // EXP-983: the dependents the server just retargeted onto the
        // integration branch. Their `base_branch` moved with them, so the
        // NEXT pass tells them to merge it in (rule 4, keyed by its tip) —
        // the level-triggered path, and the only one that cannot double up.
        if !outcome.retargeted.is_empty() {
            log::info!(
                "[workflows] retargeted onto {branch}: {}",
                outcome.retargeted.join(", ")
            );
        }
        return;
    }
    if outcome.is_waiting() {
        return; // the gate, or a workflow that stopped running
    }
    let reason = outcome.reason.unwrap_or_else(|| "GitHub refused the merge".to_string());
    let mut report = api::workflows::NodeReport::new(node_id, "updating");
    report.note = api::patch::Patch::Set(reason.chars().take(500).collect());
    report_node(&pass.trpc, &report);
    // The head GitHub refused: the engine holds the node `updating` until
    // it moves, instead of asking GitHub again every beat.
    let workflow_id = snapshot.workflow.id.clone();
    if let Some(head) = snapshot.pr_head.get(node_id).cloned() {
        let node = node_id.to_string();
        update_state(pass, &workflow_id, move |state| {
            state.land_refused.insert(node, head);
        });
    }
    let Some(session_id) = snapshot
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.session_id.clone())
    else {
        return;
    };
    let prompt = conflict_prompt(branch);
    // A LIVE run takes the text where it stands; an ended one has to be
    // resumed, which only the foreground can do.
    if let Some(engine) = pass.engines.get(&session_id) {
        engine.steer(prompt);
        return;
    }
    if !pass.session_is_local.contains(&session_id) {
        remember_resuming(pass, &workflow_id, &session_id, snapshot.now_ms);
        resumes.push(ResumeOrder {
            session_id,
            prompt,
            in_flight: Some(Arc::new(claim)),
        });
    }
}

/// The run this host is about to RESUME reads as live to the engine until
/// the node names the new run (or the grace passes), so its ended row is
/// neither resumed twice nor told anything meanwhile.
fn remember_resuming(pass: &Pass, workflow_id: &str, session_id: &str, now_ms: i64) {
    let session_id = session_id.to_string();
    update_state(pass, workflow_id, move |state| {
        state.resuming.insert(session_id, now_ms);
    });
}

/// FEED-49: the same hold for a PERSON's resume of a node run — the Resume
/// button, a relay resume, an account switch (`account_switch`,
/// `action_run::resume_run_on_account`). The run is ended before the server
/// re-points the node at its successor (`codingSessions.start`, EXP-906),
/// and a pass in between read the ended row and re-decided the node: a
/// fresh start took the node's `session_id`, and the resumed run answered
/// "not a workflow node" to every node tool. Take it BEFORE the run is
/// ended. A run no node names is not a node run: nothing is written.
pub(crate) fn hold_person_resume(session_id: &str, cx: &App) {
    let Some((settings_path, device_id, nodes)) = person_resume_context(cx) else {
        return;
    };
    let mut states = workflows::read_states(&settings_path, &device_id);
    let now_ms = chrono::Local::now().timestamp_millis();
    let Some(workflow_id) = workflows::hold_resume(&mut states, nodes, session_id, now_ms) else {
        return;
    };
    log::info!("[workflows] {workflow_id}: holding node run {session_id} for a person's resume");
    if let Err(err) = workflows::write_states(&settings_path, &device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

/// The hold [`hold_person_resume`] took, released: the resume was refused
/// after the run ended, so the engine decides the node now, not after the
/// grace.
pub(crate) fn release_person_resume(session_id: &str, cx: &App) {
    let Some((settings_path, device_id, _)) = person_resume_context(cx) else {
        return;
    };
    let mut states = workflows::read_states(&settings_path, &device_id);
    if !workflows::release_resume(&mut states, session_id) {
        return;
    }
    if let Err(err) = workflows::write_states(&settings_path, &device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

/// Where the engine state lives on this machine, plus every synced
/// `(workflow id, session id)` node pair. `None` before sign-in/sync.
fn person_resume_context(cx: &App) -> Option<(PathBuf, String, Vec<(String, String)>)> {
    let store = sync::Store::try_global(cx)?;
    let nodes: Vec<(String, String)> = store
        .collections()
        .workflow_nodes
        .read(cx)
        .iter()
        .filter_map(|row| Some((row.workflow_id.clone()?, row.session_id.clone()?)))
        .collect();
    let auth = crate::session::AuthContext::global(cx);
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let settings_path = coding::Settings::default_path(&auth.data_dir);
    Some((settings_path, device_id, nodes))
}

fn remember_nudge(pass: &Pass, workflow_id: &str, session_id: &str, key: &str) {
    let Ok(resets_at) = key.parse::<i64>() else {
        return;
    };
    let mut states = workflows::read_states(&pass.settings_path, &pass.device_id);
    states
        .entry(workflow_id.to_string())
        .or_default()
        .nudged
        .insert((session_id.to_string(), resets_at));
    if let Err(err) = workflows::write_states(&pass.settings_path, &pass.device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

fn already_deleted(pass: &Pass, workflow_id: &str) -> bool {
    workflows::read_states(&pass.settings_path, &pass.device_id)
        .get(workflow_id)
        .is_some_and(|state| state.branch_deleted)
}

fn remember_branch_deleted(pass: &Pass, workflow_id: &str) {
    let mut states = workflows::read_states(&pass.settings_path, &pass.device_id);
    states.entry(workflow_id.to_string()).or_default().branch_deleted = true;
    if let Err(err) = workflows::write_states(&pass.settings_path, &pass.device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

/// The resumes one pass produced, handed to the foreground by the next tick.
/// A plain mutex rather than a channel: the queue is drained whole, and a
/// dropped order is simply re-decided next beat.
struct ResumeQueue(Mutex<Vec<ResumeOrder>>);

impl ResumeQueue {
    fn with_lock(&self, order: ResumeOrder) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push(order);
        }
    }

    fn drain(&self) -> Vec<ResumeOrder> {
        self.0.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default()
    }
}

static RESUME_QUEUE: std::sync::LazyLock<ResumeQueue> =
    std::sync::LazyLock::new(|| ResumeQueue(Mutex::new(Vec::new())));

// ---------------------------------------------------------------------------
// The launch (foreground)
// ---------------------------------------------------------------------------

/// Re-enter the ended runs the pass decided have to merge the integration
/// branch in. The in-flight claim rides along until the resume is handed
/// over, so the next pass does not queue the same node twice.
fn drain_resumes(cx: &mut App) {
    for resume in RESUME_QUEUE.drain() {
        let keep = resume.in_flight;
        crate::action_run::resume_run_on_account(
            resume.session_id,
            None,
            false,
            LaunchOrigin::Local,
            None,
            Some(resume.prompt),
            cx,
        );
        drop(keep);
    }
}

fn launch_node(order: StartOrder, cx: &mut App) {
    let StartOrder {
        workflow_id,
        workflow_name,
        decisions,
        start_on,
        blockers,
        integration_branch,
        node_id,
        issue_id,
        member_issue_ids,
        repo,
        options,
        trpc,
        in_flight,
    } = order;
    let run = coding::WorkflowRun {
        workflow_id: workflow_id.clone(),
        name: workflow_name,
        decisions,
        start_on,
        blockers,
    };
    // A compound node is ONE batch run over its parent plus its members;
    // a plain node is an ordinary issue run. Both cut from the integration
    // branch and both carry the `## Workflow` section.
    let prepared = match repo {
        None => build_issue_start(&issue_id, &integration_branch, run, options, cx),
        Some(repo) => build_batch_start(
            &issue_id,
            &member_issue_ids,
            repo,
            &integration_branch,
            run,
            options,
            cx,
        ),
    };
    let Some((request, deps, subject)) = prepared else {
        fail_node_async(&trpc, &node_id, "The run could not be prepared on this machine", cx);
        return;
    };
    let Some(window) = crate::steer_wiring::find_team_window(cx) else {
        fail_node_async(&trpc, &node_id, "No Exponential window is open on the runner", cx);
        return;
    };
    let node = node_id.clone();
    cx.spawn(async move |cx| {
        let mut hold = in_flight;
        let prepared = cx
            .background_executor()
            .spawn(async move { coding::prepare(&request, &deps) })
            .await;
        // EXP-1007: a window that closed between the pass and this point
        // dropped the prepared run on the floor — the node stayed `running`
        // with no session and was started again every grace. Now it fails
        // with a note like every other launch error.
        let trpc_for_lost_window = Arc::clone(&trpc);
        let node_for_lost_window = node.clone();
        let updated = window.update(cx, |_, window, cx| match prepared {
            Ok(coding::Prepared::Ready(ready)) => {
                let session_id = ready.session_id.clone();
                let subject = match subject {
                    SessionSubject::Issue(id) => SessionSubject::Issue(id),
                    other => other,
                };
                match coding_flow::spawn_into_window(ready, subject, window, cx) {
                    Ok(()) => {
                        let trpc = Arc::clone(&trpc);
                        let node = node.clone();
                        // The in-flight claim rides along until the node
                        // names its session: a pass in between would read a
                        // `running` node with no session and start it again.
                        let hold = hold.take();
                        cx.background_executor()
                            .spawn(async move {
                                let mut report =
                                    api::workflows::NodeReport::new(&node, "running");
                                report.session_id = api::patch::Patch::Set(session_id);
                                report_node(&trpc, &report);
                                drop(hold);
                            })
                            .detach();
                    }
                    Err(err) => fail_node_async(&trpc, &node, &err, cx),
                }
            }
            Ok(coding::Prepared::Disabled(reason)) => {
                fail_node_async(&trpc, &node, &reason.message(), cx)
            }
            Err(err) => fail_node_async(&trpc, &node, &err.to_string(), cx),
        });
        if updated.is_err() {
            cx.background_executor()
                .spawn(async move {
                    fail_node(
                        &trpc_for_lost_window,
                        &node_for_lost_window,
                        "The Exponential window closed before the run could start",
                    )
                })
                .detach();
        }
        drop(hold);
    })
    .detach();
}

/// EXP-984 — start ONE agent review: the hidden `Review node` builtin, in a
/// throwaway worktree cut from the node's pushed branch. A failure is not a
/// node failure (the node's own work is fine): it only drops the recorded
/// head, so the next beat tries the review again.
fn launch_review(order: ReviewOrder, cx: &mut App) {
    let Some(deps) = coding_flow::build_action_deps(cx) else {
        log::warn!("[workflows] review of {}: not signed in", order.node_id);
        return;
    };
    let Some(window) = crate::steer_wiring::find_team_window(cx) else {
        log::warn!("[workflows] review of {}: no window open", order.node_id);
        return;
    };
    let ReviewOrder {
        workflow_id,
        node_id,
        team_id,
        repo,
        identifier,
        node_branch,
        base_branch,
        review_branch,
        round_at_launch,
        adversarial,
        options,
        settings_path,
        device_id,
        in_flight,
    } = order;
    let request = coding::PrepareRequest::Action(coding::ActionLaunchRequest {
        action_id: domain::contract::BUILTIN_REVIEW_NODE_ID.to_string(),
        run_id: coding::new_run_id(),
        action_name: api::actions::builtin_action_name(
            domain::contract::BUILTIN_REVIEW_NODE_ID,
        )
        .unwrap_or("Review node")
        .to_string(),
        team_id,
        body: String::new(),
        repo: Some(coding::RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        }),
        inputs: Vec::new(),
        kind: coding::ActionRunKind::ReviewNode {
            node_id: node_id.clone(),
            identifier,
            branch: node_branch,
            base_branch,
            review_branch,
            adversarial,
        },
        trigger: None,
        automation_id: None,
        device_label: coding::default_device_label(),
        origin: LaunchOrigin::Local,
        options,
        prompt: None,
    });
    let failed_node = node_id.clone();
    let failed_workflow = workflow_id.clone();
    let failed_settings = settings_path.clone();
    let failed_device = device_id.clone();
    cx.spawn(async move |cx| {
        let hold = in_flight;
        let prepared = cx
            .background_executor()
            .spawn(async move { coding::prepare(&request, &deps) })
            .await;
        let _ = window.update(cx, |_, window, cx| match prepared {
            Ok(coding::Prepared::Ready(ready)) => {
                let session_id = ready.session_id.clone();
                let subject = SessionSubject::Action(session_id.clone());
                match coding_flow::spawn_into_window(ready, subject, window, cx) {
                    Ok(()) => remember_review_run(
                        &settings_path,
                        &device_id,
                        &workflow_id,
                        &node_id,
                        &session_id,
                        round_at_launch,
                    ),
                    Err(err) => {
                        log::warn!("[workflows] review of {node_id} — {err}");
                        forget_reviewed_head(
                            &failed_settings,
                            &failed_device,
                            &failed_workflow,
                            &failed_node,
                        );
                    }
                }
            }
            Ok(coding::Prepared::Disabled(reason)) => {
                log::warn!("[workflows] review of {node_id} — {}", reason.message());
                forget_reviewed_head(
                    &failed_settings,
                    &failed_device,
                    &failed_workflow,
                    &failed_node,
                );
            }
            Err(err) => {
                log::warn!("[workflows] review of {node_id} — {err}");
                forget_reviewed_head(
                    &failed_settings,
                    &failed_device,
                    &failed_workflow,
                    &failed_node,
                );
            }
        });
        drop(hold);
    })
    .detach();
}

/// Remember which run reviews a node (so a live review is never doubled)
/// and the node's round when it was launched (so a run that ends with the
/// round unchanged is known to have submitted nothing).
fn remember_review_run(
    settings_path: &std::path::Path,
    device_id: &str,
    workflow_id: &str,
    node_id: &str,
    session_id: &str,
    round_at_launch: i64,
) {
    edit_states(settings_path, device_id, workflow_id, |state| {
        state
            .review_runs
            .insert(node_id.to_string(), session_id.to_string());
        state
            .review_rounds
            .insert(node_id.to_string(), round_at_launch);
    });
}

/// A review that never came up reviewed nothing: drop the head it claimed so
/// the next beat starts one again.
fn forget_reviewed_head(
    settings_path: &std::path::Path,
    device_id: &str,
    workflow_id: &str,
    node_id: &str,
) {
    edit_states(settings_path, device_id, workflow_id, |state| {
        state.reviewed_head.remove(node_id);
    });
}

/// Read-modify-write one workflow's persisted engine state, off a Pass.
fn edit_states(
    settings_path: &std::path::Path,
    device_id: &str,
    workflow_id: &str,
    edit: impl FnOnce(&mut workflows::WorkflowState),
) {
    let mut states = workflows::read_states(settings_path, device_id);
    edit(states.entry(workflow_id.to_string()).or_default());
    if let Err(err) = workflows::write_states(settings_path, device_id, &states) {
        log::warn!("[workflows] state write failed: {err}");
    }
}

fn build_issue_start(
    issue_id: &str,
    integration_branch: &str,
    run: coding::WorkflowRun,
    options: LaunchOptions,
    cx: &mut App,
) -> Option<(coding::PrepareRequest, coding::CodingDeps, SessionSubject)> {
    let (mut request, deps) =
        coding_flow::build_launch(issue_id, LaunchOrigin::Local, options, false, None, None, cx)?;
    request.base_branch = Some(integration_branch.to_string());
    request.workflow = Some(run);
    Some((
        coding::PrepareRequest::Issue(request),
        deps,
        SessionSubject::Issue(issue_id.to_string()),
    ))
}

fn build_batch_start(
    issue_id: &str,
    member_issue_ids: &[String],
    repo: api::repositories::IssueRepository,
    integration_branch: &str,
    run: coding::WorkflowRun,
    options: LaunchOptions,
    cx: &mut App,
) -> Option<(coding::PrepareRequest, coding::CodingDeps, SessionSubject)> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let issues = collections.issues.read(cx);
    let boards = collections.boards.read(cx);
    let parent = issues.get(issue_id)?.clone();
    let board = boards.get(&parent.board_id)?;
    let mut specs = Vec::new();
    for id in std::iter::once(&issue_id.to_string()).chain(member_issue_ids.iter()) {
        let Some(issue) = issues.get(id) else {
            continue;
        };
        specs.push(coding::BatchIssueSpec {
            issue_id: issue.id.clone(),
            issue_identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            status: issue.status,
        });
    }
    if specs.len() < 2 {
        return None; // a member that has not synced — retry next beat
    }
    let team_id = board.team_id.clone();
    let batch_id = coding::new_batch_id();
    let request = coding::BatchLaunchRequest {
        batch_id: batch_id.clone(),
        team_id,
        board_id: Some(parent.board_id.clone()),
        repo: coding::RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        },
        issues: specs,
        device_label: coding::default_device_label(),
        origin: LaunchOrigin::Local,
        options,
        prompt: None,
        base_branch: Some(integration_branch.to_string()),
        workflow: Some(run),
    };
    let deps = coding_flow::build_batch_deps(cx)?;
    Some((
        coding::PrepareRequest::Batch(request),
        deps,
        SessionSubject::Batch(batch_id),
    ))
}

/// A node whose run never reached the agent: the state says so, in one
/// sentence, so the node panel can offer Retry or Skip.
fn fail_node(trpc: &Arc<api::TrpcClient>, node_id: &str, reason: &str) {
    let mut report = api::workflows::NodeReport::new(node_id, "failed");
    report.note = api::patch::Patch::Set(one_line(reason));
    report_node(trpc, &report);
}

fn fail_node_async(trpc: &Arc<api::TrpcClient>, node_id: &str, reason: &str, cx: &mut App) {
    let trpc = Arc::clone(trpc);
    let node_id = node_id.to_string();
    let reason = reason.to_string();
    cx.background_executor()
        .spawn(async move { fail_node(&trpc, &node_id, &reason) })
        .detach();
}

/// One line, ≤500 chars — the column's bound.
fn one_line(text: &str) -> String {
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flattened.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The conflict text a node gets when the integration branch moved
    /// under its pull request.
    #[test]
    fn the_conflict_prompt_names_the_branch_to_merge() {
        assert_eq!(
            conflict_prompt("exp/wf-abcdef12"),
            "The integration branch moved and your pull request no longer merges. Run git fetch \
origin, git merge origin/exp/wf-abcdef12, resolve the conflicts, push, then end the run again."
        );
    }

    /// A note is one line and bounded — the column takes 500 chars.
    #[test]
    fn a_note_is_one_bounded_line() {
        assert_eq!(one_line("  two\n  lines  "), "two lines");
        assert_eq!(one_line(&"x".repeat(900)).len(), 500);
    }

    /// The in-flight claim is exclusive and frees itself: a node the host is
    /// already starting is never started twice, and a finished order never
    /// leaves the node stuck out of the next pass.
    #[test]
    fn an_in_flight_claim_is_exclusive_and_self_freeing() {
        let set: Arc<Mutex<HashSet<String>>> = Arc::default();
        let first = InFlight::claim(&set, "node-1").expect("the first claim wins");
        assert!(InFlight::claim(&set, "node-1").is_none());
        assert!(InFlight::claim(&set, "node-2").is_some());
        assert_eq!(snapshot_of(&set), ["node-1".to_string()].into());
        drop(first);
        assert!(snapshot_of(&set).is_empty());
    }
}
