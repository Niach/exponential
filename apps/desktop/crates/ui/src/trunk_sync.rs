//! The headless trunk-sync engine (EXP-253 — the gutted successor of the
//! top-bar git cluster). Nothing renders it: the rail drives its lifecycle
//! every render (`ensure_loaded`) and paints a status badge + "synced Xm
//! ago" tooltip off its state; the Source Control screen reads its trunk
//! snapshot and conflict state. The IDE is **master-only + autopull** now —
//! branch switching, committing, pushing and publishing are gone (the editor
//! is view-only; changes only ever arrive via PRs), so the engine's whole
//! job is keeping the trunk clone fresh and surfacing conflicts.
//!
//! Scope follows the window's navigation: a board view or an issue detail
//! resolves to that board's primary repo; other screens keep the last board
//! (so the badge stays live everywhere). On first resolve the engine kicks
//! the lifecycle — auto-clone when `<clone>/.git` is missing, else a
//! freshness sync (fetch + ff-only catch-up) — then reads the trunk state.
//!
//! **Auto-sync**: a [`clone_manager::AUTO_SYNC_INTERVAL`] timer plus a
//! window-focus observer call [`TrunkSync::maybe_auto_sync`], debounced
//! through [`clone_manager::should_fetch`] and skipped while a sync is in
//! flight or an Action tab is alive for this repo (never fast-forward the
//! tree under Claude's feet; EXP-346 pulled promptless AgentShell tabs OUT
//! of that hold-off — they live for entire work days and were parking the
//! trunk stale). A PR merging is a sync trigger of its own (EXP-346,
//! [`TrunkSync::check_merge_autopull`]): a fresh `pr_merged_at` on any of
//! this repo's synced issues fires an immediate debounce-bypassing pull, so
//! every IDE lands on the new master right after a merge instead of on the
//! next timer tick. The background pass is
//! [`clone_manager::auto_sync`]: fetch → fast-forward ONLY when clean +
//! behind-only; anything else is a loud-but-quiet Skipped outcome. Background
//! failures collapse into one sticky badge (cleared on the next success) —
//! separate from `op_error`, which belongs to user-clicked ops.
//!
//! **The one escape hatch** ([`TrunkSync::hard_reset`], surfaced in Source
//! Control behind a confirm): abort any in-progress rebase/merge, fetch,
//! force-checkout the default branch and `reset --hard origin/<default>` —
//! discarding local TRACKED changes (untracked files survive; this is a
//! recover-the-trunk hatch, not a nuke). Tokens come from
//! [`coding::token_cache`]. An ordinary conflict is otherwise left in place
//! (never auto-aborted) and re-derived from disk for the badge.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{App, Entity, SharedString, Subscription, Window};
use sync::Store;
use terminal::TabKind;

use coding::scm;
use coding::{
    clone_manager, clone_path, git_worktree, trunk_state, AutoSyncOutcome, CloneEvent, Settings,
    TrunkState,
};

use crate::navigation::{self, Navigation};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::session::AuthContext;

/// The trunk repo a resolved board points at. All owned/`Send` so the whole
/// struct can ride onto the background executor for a git op.
#[derive(Clone)]
struct RepoInfo {
    /// `repositories.id` — the input to the token cache's mint.
    repository_id: String,
    /// `owner/name` — the clone-root key + the remote's redaction anchor.
    full_name: String,
    /// The trunk's server-reported default branch — the hard-reset target
    /// (fallback: the checked-out branch). `None` when the server omitted it
    /// (never `main`).
    default_branch: Option<String>,
    /// `<repos_root>` — the clone-root prefix (`clone_manager::ensure` joins
    /// `full_name` onto it).
    repos_root: PathBuf,
    /// `<repos_root>/<owner>/<name>` — the trunk clone root.
    clone: PathBuf,
    /// Whether `<clone>/.git` exists (gates the auto-clone vs. fetch path).
    clone_exists: bool,
}

/// Which git op a [`TrunkSync::start_sync`] runs on the background executor.
/// All token ops route through `coding::token_cache` and re-read the trunk on
/// completion. (EXP-253 deleted the Push/Publish/GetLatest transport modes;
/// EXP-509 brought ONE deliberate write back: the uncommitted-row
/// commit-and-push affordance.)
#[derive(Clone, PartialEq, Eq)]
enum SyncMode {
    /// Auto-clone the missing trunk (streams `git clone --progress` %).
    Clone,
    /// User freshness pass (refresh button / board open): fetch + the same
    /// ff-only catch-up AutoSync runs.
    Fetch,
    /// [`SyncMode::Fetch`] under the live-task hold-off: fetch only, NO
    /// ff catch-up — an Action tab is working on this repo's
    /// clone, and the fetch is harmless but the working-tree update would
    /// move the tree under Claude's feet (same rule as the AutoSync
    /// hold-off, which skips the whole pass).
    FetchOnly,
    /// Background pass: fetch → ff-only when clean & behind-only, else skip
    /// (+ the EXP-465 landed-worktree/stale-branch prune policy).
    AutoSync,
    /// The escape hatch: abort any in-progress rebase/merge, fetch,
    /// force-checkout the default branch, `reset --hard origin/<default>`.
    HardReset,
    /// EXP-509: push local work upstream — `git add -A` + `git commit`
    /// (only when the tree is dirty; `message` is required then) followed by
    /// `git push origin <branch>`. `identity` is the signed-in account's
    /// (name, email), applied only when the clone resolves no `user.email`.
    CommitPush {
        message: Option<String>,
        identity: Option<(String, String)>,
    },
}

/// Foreground-marshaled progress of a background git op. The worker streams
/// these through a [`flume`] channel; the drain applies them with `cx`
/// (`recv_async` off the gpui foreground, then `this.update`).
enum SyncMsg {
    /// A `git clone` lifecycle event (badge tooltip percentage).
    Clone(CloneEvent),
    /// A user-op failure detail (token already scrubbed) → `op_error`.
    Failed(String),
    /// The background auto-sync pass finished → `auto_sync_error` bookkeeping
    /// (NEVER `op_error` — one sticky badge, no error flooding).
    AutoSyncDone(Result<AutoSyncOutcome, String>),
    /// The terminal on-disk trunk read (always sent last; `Err` keeps the
    /// prior state). A conflict engaged by a failed rebase surfaces HERE.
    Trunk(Result<TrunkState, String>),
}

/// Load gate — a scope change resets it; `Idle` kicks one background resolve.
enum Load {
    Idle,
    Loading,
    Ready,
}

/// The headless trunk-sync engine (see module docs).
pub struct TrunkSync {
    nav: Entity<Navigation>,
    /// The shared per-window repo resolver — one `repositories.list` fetch
    /// for the whole window instead of a per-engine call.
    repo_resolver: Entity<RepoResolver>,
    /// The board scope the loaded state below belongs to (`None` = no board
    /// resolved yet).
    board_id: Option<String>,
    load: Load,
    /// The resolved trunk repo (`None` = no repo linked to the board).
    repo: Option<RepoInfo>,
    /// Repo-resolution problem (no repo linked / `repositories.list` failed).
    repo_error: Option<SharedString>,
    /// The on-disk trunk state (branch + dirty + upstream + ahead/behind +
    /// conflict).
    trunk: TrunkState,
    /// A clone/fetch/reset is in flight (badge spinner; blocks a second op).
    syncing: bool,
    /// `git clone --progress` percentage while cloning (`None` otherwise) —
    /// the rail tooltip surfaces it.
    clone_progress: Option<u8>,
    /// The last USER-op failure — the badge's error state.
    op_error: Option<SharedString>,
    /// The last BACKGROUND auto-sync failure — the sticky badge, cleared on
    /// the next successful sync.
    auto_sync_error: Option<SharedString>,
    /// When the last successful sync finished (feeds "synced Xm ago" and the
    /// [`clone_manager::should_fetch`] debounce).
    last_synced: Option<Instant>,
    /// Whether the in-flight job already reported a failure (so the trailing
    /// `Trunk(Ok)` read does not stamp `last_synced` / clear the badge).
    job_failed: bool,
    /// Scope generation — bumped on every scope change so a stale background
    /// job's marshaled messages (and the old timer loop) are ignored.
    generation: u64,
    /// Bumped whenever fresh on-disk git state lands here (sync completion,
    /// local re-read, initial scope read) — a cheap change signal other
    /// surfaces (the Source Control history pane, EXP-67) compare instead of
    /// diffing trunk snapshots.
    sync_seq: u64,
    /// The newest `pr_merged_at` stamp seen across synced issues of this
    /// repo's boards (EXP-346 merge-reactive autopull). A later stamp means
    /// a PR merged while we watch → pull NOW, not on the next timer tick.
    merge_stamp: Option<String>,
    /// Whether `merge_stamp` holds this scope's baseline yet — the first
    /// post-ready read only seeds it (the board-open fetch already covers
    /// startup freshness) and must not fire a sync itself.
    merge_stamp_seeded: bool,
    /// A merge landed and its pull has not started yet. Sticky through
    /// in-flight ops and the Action hold-off (retried every render and on
    /// the auto-sync timer); consumed — and the [`clone_manager::FETCH_DEBOUNCE`]
    /// bypassed — by the next [`Self::maybe_auto_sync`] that actually runs.
    pending_merge_sync: bool,
    /// The finished/PR-merged issue ids of this repo's boards as of the last
    /// look (EXP-465 auto-clean) — a NEW member means an issue just turned
    /// done/cancelled/merged and its worktree may be prunable now.
    finished_ids: std::collections::HashSet<String>,
    /// Whether `finished_ids` holds this scope's baseline yet — the first
    /// post-ready read only seeds it.
    finished_seeded: bool,
    /// An issue newly finished and the prune-carrying auto-sync has not
    /// started yet — same stickiness/consumption as `pending_merge_sync`.
    pending_prune_sync: bool,
    /// The doctor's last-seen git verdict — the EXP-366 recovery edge: when
    /// git flips missing→present ("Check tools" after installing it), the
    /// failed/never-attempted clone retries WITHOUT the user finding the
    /// Source Control refresh button. Machine state, not scope state — never
    /// reset on scope change.
    git_ok_seen: Option<bool>,
    _subscriptions: Vec<Subscription>,
}

impl TrunkSync {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let hub = crate::coding_flow::CodingHub::global(cx);
        let subscriptions = vec![
            // EXP-366: retry the trunk clone on the doctor's git
            // missing→present edge — every recovery path ("Check tools", a
            // settings save, the onboarding tools step) funnels through
            // `refresh_doctor`, which notifies the hub.
            cx.observe_in(&hub, window, |this: &mut Self, hub, window, cx| {
                let Some(report) = hub.read(cx).doctor.report.as_ref() else {
                    return;
                };
                let git_ok = report.git.ok;
                let was = this.git_ok_seen.replace(git_ok);
                let retry = git_recovery_retry(
                    was,
                    git_ok,
                    this.syncing,
                    this.repo.as_ref().is_some_and(|repo| repo.clone_exists),
                    this.sync_error().is_some(),
                );
                if retry {
                    this.refresh(window, cx);
                }
            }),
            // Scope follows navigation (board / issue-detail → board).
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The issue→board join reads synced rows.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // Re-render when the shared repo resolution lands / changes.
            cx.observe(&repo_resolver, |_, _, cx| cx.notify()),
            // Window focus is an auto-sync trigger (debounced).
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.maybe_auto_sync(window, cx);
                }
            }),
        ];
        Self {
            nav,
            repo_resolver,
            board_id: None,
            load: Load::Idle,
            repo: None,
            repo_error: None,
            trunk: TrunkState::empty(),
            syncing: false,
            clone_progress: None,
            op_error: None,
            auto_sync_error: None,
            last_synced: None,
            job_failed: false,
            generation: 0,
            sync_seq: 0,
            merge_stamp: None,
            merge_stamp_seeded: false,
            pending_merge_sync: false,
            finished_ids: Default::default(),
            finished_seeded: false,
            pending_prune_sync: false,
            git_ok_seen: None,
            _subscriptions: subscriptions,
        }
    }

    /// Monotonic stamp of the last fresh on-disk read (see the field doc).
    pub(crate) fn sync_seq(&self) -> u64 {
        self.sync_seq
    }

    /// The scope: the window's active board (screen scope with the
    /// last-board fallback) — populated on EVERY screen so the Source
    /// Control surfaces and the rail badge stay live.
    fn scope_board_id(&self, cx: &App) -> Option<String> {
        navigation::active_board_id(&self.nav, cx)
    }

    /// Why the trunk needs the user (the rail's amber badge + tooltip): a
    /// paused rebase/merge, a detached HEAD, local commits, or a dirty working
    /// tree. Every one of these blocks the ff-only autopull, so without
    /// surfacing the trunk parks stale silently and forever (EXP-346 — a
    /// diverged trunk showed NOTHING while auto-sync skipped it on every
    /// pass). The fix is always the Source Control screen's hatch (or
    /// finishing the conflict).
    pub(crate) fn attention(&self) -> Option<SharedString> {
        attention_reason(&self.trunk)
    }

    /// Whether a sync op is in flight — the rail badge's "syncing" state.
    pub(crate) fn is_syncing(&self) -> bool {
        self.syncing
    }

    /// `git clone --progress` percentage while the trunk is cloning — the
    /// rail tooltip surfaces it.
    pub(crate) fn clone_progress(&self) -> Option<u8> {
        self.clone_progress
    }

    /// The sticky sync-failure detail (user op OR background auto-sync) —
    /// the rail badge's "error" state.
    pub(crate) fn sync_error(&self) -> Option<SharedString> {
        self.op_error.clone().or_else(|| self.auto_sync_error.clone())
    }

    /// When the last successful sync finished — the rail tooltip's
    /// "synced Xm ago" stamp.
    pub(crate) fn last_synced(&self) -> Option<Instant> {
        self.last_synced
    }

    /// The checked-out branch as of the last on-disk read (empty until read).
    pub(crate) fn branch(&self) -> &str {
        &self.trunk.branch
    }

    /// The resolved trunk clone root on disk (`None` while unresolved or on
    /// a repo-less board).
    pub(crate) fn clone_dir(&self) -> Option<PathBuf> {
        self.repo
            .as_ref()
            .filter(|repo| repo.clone_exists)
            .map(|repo| repo.clone.clone())
    }

    /// Freshness fetch + trunk re-read (the Source Control refresh button).
    /// While an Action tab is alive on this repo's clone the
    /// pass degrades to fetch-only — the ff working-tree update is held off
    /// exactly like the AutoSync one (never move the tree under Claude's
    /// feet).
    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.start_sync(self.fetch_mode(window, cx), cx);
    }

    /// The freshness-pass mode under the live-task hold-off: [`SyncMode::Fetch`]
    /// normally, [`SyncMode::FetchOnly`] while [`Self::repo_tasks_alive`].
    fn fetch_mode(&self, window: &Window, cx: &App) -> SyncMode {
        if self.repo_tasks_alive(window, cx) {
            SyncMode::FetchOnly
        } else {
            SyncMode::Fetch
        }
    }

    /// The escape hatch (EXP-253): abort any in-progress rebase/merge, fetch,
    /// force-checkout the default branch and `reset --hard origin/<default>`.
    /// Discards local TRACKED changes; untracked files survive. Source
    /// Control gates it behind an explicit confirm dialog. A reset that
    /// can't run right now says so — a silently dropped destructive op
    /// would read as success.
    pub(crate) fn hard_reset(&mut self, cx: &mut gpui::Context<Self>) {
        if self.syncing {
            self.op_error =
                Some("A sync is already running. Try the reset again in a moment.".into());
            cx.notify();
            return;
        }
        self.start_sync(SyncMode::HardReset, cx);
    }

    /// EXP-509: commit-and-push the trunk's local work (the history pane's
    /// uncommitted-row affordance): `git add -A` + `git commit -m <message>`
    /// when the tree is dirty, then `git push origin <branch>`. Refused while
    /// an op is in flight or a rebase/merge sits paused — and the worker
    /// re-derives both from disk before writing anything.
    pub(crate) fn commit_push(&mut self, message: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.syncing {
            self.op_error =
                Some("A sync is already running. Try the push again in a moment.".into());
            cx.notify();
            return;
        }
        if self.trunk.conflict.is_some() {
            self.op_error = Some("Resolve the paused rebase/merge before pushing.".into());
            cx.notify();
            return;
        }
        // The signed-in account backs `git commit` when the clone has no
        // identity of its own (clones never configure one; agents ride their
        // CLI's environment). Fallback name: the email's local part.
        let identity = crate::queries::active_account(cx).map(|account| {
            let name = account
                .name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| {
                    account.email.split('@').next().unwrap_or("user").to_string()
                });
            (name, account.email)
        });
        self.start_sync(SyncMode::CommitPush { message, identity }, cx);
    }

    /// The on-disk trunk state as of the last read (EXP-509 — the history
    /// pane's uncommitted row reads dirty/ahead/conflict off it).
    pub(crate) fn trunk(&self) -> &TrunkState {
        &self.trunk
    }

    /// Debounced background sync trigger (timer tick + window focus + the
    /// EXP-346 merge trigger): no-op while an op is in flight, before the
    /// clone exists, inside the [`clone_manager::FETCH_DEBOUNCE`] window
    /// (bypassed while a merge-triggered pull is pending — a fresh merge
    /// must land now, not after the debounce), or while an Action tab is
    /// alive for this repo (never move the tree under Claude's feet — the
    /// pending flag survives the hold-off and fires once it lifts).
    fn maybe_auto_sync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.syncing {
            return;
        }
        let Some(repo) = &self.repo else {
            return;
        };
        if !repo.clone_exists {
            return;
        }
        if !self.pending_merge_sync
            && !self.pending_prune_sync
            && self
                .last_synced
                .is_some_and(|last| !clone_manager::should_fetch(last))
        {
            return;
        }
        if self.repo_tasks_alive(window, cx) {
            return;
        }
        let (repository_id, default_branch) =
            (repo.repository_id.clone(), repo.default_branch.clone());
        // EXP-465: every auto-sync pass carries the full prune policy — the
        // engine decides what is actually landed and removable — gated on the
        // same shape readiness as the sibling triggers
        // ([`Self::check_merge_autopull`], [`Self::check_finished_autoclean`]):
        // not-yet-ready collections derive EMPTY keep/merged/finished sets,
        // and git truth alone calls a fresh 0-ahead worktree of an OPEN issue
        // "landed" debris. A not-ready pass syncs without pruning.
        let collections = Store::global(cx).collections().clone();
        let policy = if carry_prune_policy(
            collections.issues.read(cx).is_ready(),
            collections.boards.read(cx).is_ready(),
        ) {
            Some(crate::worktree_prune::prune_policy_for_repo(
                &repository_id,
                default_branch,
                window,
                cx,
            ))
        } else {
            None
        };
        // Consume the triggers only once the pass is actually spawned —
        // `start_sync_with_prune` still no-ops without a resolved tRPC client,
        // and dropping a flag there would park the post-merge pull / prune
        // until a later edge re-raised it. A policy-less (not-ready) pass
        // keeps the prune trigger alive for the same reason.
        let carried_prune = policy.is_some();
        if self.start_sync_with_prune(SyncMode::AutoSync, policy, cx) {
            self.pending_merge_sync = false;
            if carried_prune {
                self.pending_prune_sync = false;
            }
        }
    }

    /// Merge-reactive autopull (EXP-346): the rail re-renders on every issue
    /// sync, so this runs per render — when a NEW `pr_merged_at` stamp
    /// appears on this repo's issues, pull immediately instead of waiting
    /// out the timer + debounce ("when a PR is merged, every IDE is on the
    /// newest master"). The first read after the shapes are ready only seeds
    /// the baseline; [`stamp_is_newer`] makes a first-ever merge
    /// (`None → Some`) trigger too.
    fn check_merge_autopull(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.repo.as_ref().is_some_and(|repo| repo.clone_exists) {
            return;
        }
        let collections = Store::global(cx).collections().clone();
        if !collections.issues.read(cx).is_ready() || !collections.boards.read(cx).is_ready() {
            return;
        }
        let stamp = self.latest_merge_stamp(cx);
        if !self.merge_stamp_seeded {
            self.merge_stamp_seeded = true;
            self.merge_stamp = stamp;
        } else if stamp_is_newer(stamp.as_deref(), self.merge_stamp.as_deref()) {
            self.merge_stamp = stamp;
            self.pending_merge_sync = true;
        }
        if self.pending_merge_sync {
            self.maybe_auto_sync(window, cx);
        }
    }

    /// The newest `pr_merged_at` across synced issues whose board points at
    /// the scoped repo. A batch PR stamps all its linked issues in one sync
    /// batch — `max` coalesces that to a single trigger.
    fn latest_merge_stamp(&self, cx: &App) -> Option<String> {
        let repo = self.repo.as_ref()?;
        let collections = Store::global(cx).collections().clone();
        let boards = collections.boards.read(cx);
        let repo_boards: Vec<&str> = boards
            .iter()
            .filter(|board| {
                board.repository_id.as_deref() == Some(repo.repository_id.as_str())
            })
            .map(|board| board.id.as_str())
            .collect();
        let issues = collections.issues.read(cx);
        issues
            .iter()
            .filter(|issue| repo_boards.contains(&issue.board_id.as_str()))
            .filter_map(|issue| issue.pr_merged_at.clone())
            .max()
    }

    /// The synced issue ids of this repo's boards that are FINISHED
    /// (done/cancelled/duplicate) or PR-merged — the EXP-465 auto-clean
    /// trigger's watch set. Ids (not a max-timestamp): edits to an
    /// already-finished issue must not re-fire the debounce bypass.
    fn finished_issue_ids(&self, cx: &App) -> std::collections::HashSet<String> {
        let Some(repo) = &self.repo else {
            return Default::default();
        };
        let collections = Store::global(cx).collections().clone();
        let boards = collections.boards.read(cx);
        let repo_boards: Vec<&str> = boards
            .iter()
            .filter(|board| {
                board.repository_id.as_deref() == Some(repo.repository_id.as_str())
            })
            .map(|board| board.id.as_str())
            .collect();
        collections
            .issues
            .read(cx)
            .iter()
            .filter(|issue| repo_boards.contains(&issue.board_id.as_str()))
            .filter(|issue| {
                // The dual-written enum ANCHOR (EXP-314) — this runs per
                // render, so it must stay a cheap enum test, not a per-issue
                // team-status resolution.
                issue.pr_state.as_deref() == Some(domain::contract::PR_STATE_MERGED)
                    || matches!(
                        issue.status,
                        domain::enums::IssueStatus::Done
                            | domain::enums::IssueStatus::Cancelled
                            | domain::enums::IssueStatus::Duplicate
                    )
            })
            .map(|issue| issue.id.clone())
            .collect()
    }

    /// Finished-reactive auto-clean (EXP-465, the sibling of
    /// [`Self::check_merge_autopull`]): when an issue of this repo NEWLY
    /// turns done/cancelled (or its PR merges), run the auto-sync pass — and
    /// its worktree prune — now instead of waiting out the timer + debounce.
    /// The first ready read only seeds the baseline; a shrink (reopened
    /// issue) re-baselines without firing.
    fn check_finished_autoclean(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.repo.as_ref().is_some_and(|repo| repo.clone_exists) {
            return;
        }
        let collections = Store::global(cx).collections().clone();
        if !collections.issues.read(cx).is_ready() || !collections.boards.read(cx).is_ready() {
            return;
        }
        let current = self.finished_issue_ids(cx);
        if finished_set_fires(self.finished_seeded, &self.finished_ids, &current) {
            self.pending_prune_sync = true;
        }
        self.finished_seeded = true;
        self.finished_ids = current;
        if self.pending_prune_sync {
            self.maybe_auto_sync(window, cx);
        }
    }

    /// Whether a live Action tab is working inside this repo's clone (or one
    /// of its worktrees) — the sync hold-off (action runs execute ON the
    /// trunk clone or a PR worktree, so an ff under them would move the tree
    /// under Claude's feet; EXP-259 deleted the ClaudeTask kind this also
    /// used to match). AutoSync skips its whole pass; a user Fetch degrades
    /// to fetch-only. Shell AND AgentShell tabs deliberately do NOT hold
    /// sync off (EXP-346 pulled AgentShell back out of the EXP-325 hold-off):
    /// both are user-attended sessions that live for entire work days, so
    /// holding sync off for them parked the trunk stale indefinitely — and an
    /// ff under an interactive session is the same event as the manual pull
    /// it replaces.
    pub(crate) fn repo_tasks_alive(&self, window: &Window, cx: &App) -> bool {
        self.repo_tabs_alive(window, cx, |kind| matches!(kind, TabKind::Action(_)))
    }

    /// [`Self::repo_tasks_alive`] widened to promptless agent shells — NOT a
    /// sync hold-off, only the hard-reset confirm's "a session is live in
    /// this clone" warning (a reset moves the tree under a live Claude no
    /// matter which tab kind hosts it).
    pub(crate) fn repo_agents_alive(&self, window: &Window, cx: &App) -> bool {
        self.repo_tabs_alive(window, cx, |kind| {
            matches!(kind, TabKind::Action(_) | TabKind::AgentShell)
        })
    }

    /// Whether any running terminal tab of a matching kind has its cwd inside
    /// this repo's trunk clone or one of its session worktrees.
    fn repo_tabs_alive(
        &self,
        window: &Window,
        cx: &App,
        kind_matches: impl Fn(&TabKind) -> bool,
    ) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        let Some(manager) = crate::coding_flow::window_terminal_manager(window, cx) else {
            return false;
        };
        let worktrees = git_worktree::worktrees_dir(&repo.clone);
        manager.read(cx).tabs().iter().any(|tab| {
            kind_matches(&tab.kind)
                && tab.is_running()
                && tab
                    .cwd
                    .as_deref()
                    .is_some_and(|cwd| cwd.starts_with(&repo.clone) || cwd.starts_with(&worktrees))
        })
    }

    /// Render-time load gate: a scope change resets, `Idle` kicks one
    /// background resolve of the board's trunk repo + its on-disk state,
    /// then the lifecycle (auto-clone / freshness fetch) and the per-scope
    /// auto-sync timer loop.
    ///
    /// LIFECYCLE DRIVER: the RAIL calls this every render — nothing else
    /// keeps the engine alive. If a future refactor ever stops rendering the
    /// rail while signed in, auto-clone/auto-sync silently die with it.
    pub(crate) fn ensure_loaded(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // team, shared by all trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        let scope = self.scope_board_id(cx);
        if scope != self.board_id {
            self.board_id = scope;
            self.load = Load::Idle;
            self.repo = None;
            self.repo_error = None;
            self.trunk = TrunkState::empty();
            self.syncing = false;
            self.clone_progress = None;
            self.op_error = None;
            self.auto_sync_error = None;
            self.last_synced = None;
            self.job_failed = false;
            self.merge_stamp = None;
            self.merge_stamp_seeded = false;
            self.pending_merge_sync = false;
            self.finished_ids = Default::default();
            self.finished_seeded = false;
            self.pending_prune_sync = false;
            // Kill the previous scope's timer loop + in-flight job tail.
            self.generation += 1;
        }
        // Merge-reactive autopull (EXP-346) — every render, not just while a
        // load is pending (the rail re-renders on issue sync, which is what
        // carries a fresh `pr_merged_at` in).
        self.check_merge_autopull(window, cx);
        // Finished-reactive auto-clean (EXP-465) — same cadence.
        self.check_finished_autoclean(window, cx);
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        // Read the shared resolution rather than firing our own network call.
        let meta = match self.repo_resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Loading => return, // the resolver observer re-renders us
            RepoLookup::Found(repo) => repo,
            RepoLookup::NotFound => {
                self.load = Load::Ready;
                self.repo = None;
                self.repo_error = Some("No repository linked to this board.".into());
                return;
            }
            RepoLookup::Error(message) => {
                log::warn!("[ui] trunk sync: repo resolution failed: {message}");
                self.load = Load::Ready;
                self.repo = None;
                self.repo_error = Some("Repository unavailable".into());
                return;
            }
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();
        let repos_root = Settings::load(&Settings::default_path(&data_dir)).repos_root_path();

        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;

        // The per-scope auto-sync loop: tick every AUTO_SYNC_INTERVAL, try a
        // debounced sync, and keep the "synced Xm ago" stamp fresh. Dies with
        // the generation (scope change) or the window.
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(clone_manager::AUTO_SYNC_INTERVAL)
                .await;
            let alive = this
                .update_in(cx, |this, window, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    this.maybe_auto_sync(window, cx);
                    cx.notify(); // refresh the synced-ago label
                    true
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            let board = board_id.clone();
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let clone = clone_path(&repos_root, &meta.full_name);
                    let clone_exists = clone.join(".git").exists();
                    // Read the on-disk trunk up front so an existing clone
                    // paints before the fetch.
                    let trunk = if clone_exists {
                        trunk_state::read(&clone).ok()
                    } else {
                        None
                    };
                    (
                        RepoInfo {
                            repository_id: meta.repository_id,
                            full_name: meta.full_name,
                            default_branch: meta.default_branch,
                            repos_root,
                            clone,
                            clone_exists,
                        },
                        trunk,
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation
                    || this.board_id.as_deref() != Some(board.as_str())
                {
                    return; // superseded by a scope change
                }
                this.load = Load::Ready;
                let (repo, trunk) = resolved;
                if let Some(trunk) = trunk {
                    this.trunk = trunk;
                }
                this.sync_seq += 1;
                let clone_exists = repo.clone_exists;
                this.repo = Some(repo);
                this.repo_error = None;
                // Auto-clone a missing trunk, else a freshness sync on
                // board open (fetch + ff when cleanly behind-only — the
                // trunk must never open stale when it could be current;
                // fetch-only under the live-task hold-off).
                this.start_sync(
                    if clone_exists {
                        this.fetch_mode(window, cx)
                    } else {
                        SyncMode::Clone
                    },
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }

    /// Spawn a background git op: token via the cache, run it, and re-read
    /// the trunk. Progress marshals to the foreground through a [`flume`]
    /// channel drained here. No-op while another op is in flight (one trunk
    /// op at a time) or off a resolved repo.
    fn start_sync(&mut self, mode: SyncMode, cx: &mut gpui::Context<Self>) {
        self.start_sync_with_prune(mode, None, cx);
    }

    /// [`Self::start_sync`] with the auto-sync pass's worktree-prune policy
    /// (`None` for every user-triggered op — pruning rides ONLY the
    /// background pass, whose trigger has the Window needed for the
    /// live-tab check). Returns whether the op was actually spawned, so a
    /// caller holding a one-shot trigger can keep it across a no-op.
    fn start_sync_with_prune(
        &mut self,
        mode: SyncMode,
        prune_policy: Option<coding::PrunePolicy>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.syncing {
            return false;
        }
        let Some(repo) = self.repo.clone() else {
            return false;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return false;
        };
        // A user freshness pass on a missing clone RE-ATTEMPTS the clone —
        // the old error+Retry chrome is gone, so refresh is the retry path
        // after a failed auto-clone.
        let mode = if matches!(mode, SyncMode::Fetch | SyncMode::FetchOnly) && !repo.clone_exists {
            SyncMode::Clone
        } else {
            mode
        };

        self.syncing = true;
        self.job_failed = false;
        if !matches!(mode, SyncMode::AutoSync) {
            // A background pass must not clear a user-op error the user has
            // not acted on yet.
            self.op_error = None;
        }
        if matches!(mode, SyncMode::Clone) {
            self.clone_progress = Some(0);
        }
        cx.notify();

        let generation = self.generation;
        let (tx, rx) = flume::unbounded::<SyncMsg>();

        // Foreground drain — applies each marshaled message with `cx`. The
        // loop ends when the worker drops its sender.
        cx.spawn(async move |this, cx| {
            while let Ok(msg) = rx.recv_async().await {
                if this
                    .update(cx, |this, cx| this.apply_sync_msg(generation, msg, cx))
                    .is_err()
                {
                    break; // the engine was dropped (window closed)
                }
            }
        })
        .detach();

        // Background worker — token + git op + trunk read (argv-only git).
        cx.background_executor()
            .spawn(async move {
                run_sync_worker(mode, &trpc, &repo, prune_policy.as_ref(), &tx);
            })
            .detach();
        true
    }

    /// Apply one marshaled [`SyncMsg`] on the foreground. Stale messages (a
    /// superseded scope) are dropped by the generation guard.
    fn apply_sync_msg(&mut self, generation: u64, msg: SyncMsg, cx: &mut gpui::Context<Self>) {
        if generation != self.generation {
            return; // superseded scope — ignore the old job's tail
        }
        match msg {
            SyncMsg::Clone(CloneEvent::Started) => {
                self.syncing = true;
                self.clone_progress = Some(0);
            }
            SyncMsg::Clone(CloneEvent::Progress(percent)) => {
                self.clone_progress = Some(percent);
            }
            SyncMsg::Clone(CloneEvent::Done) => {
                self.clone_progress = None;
                if let Some(repo) = &mut self.repo {
                    repo.clone_exists = true;
                }
            }
            SyncMsg::Clone(CloneEvent::Failed(detail)) => {
                // EXP-366: this is often the FIRST git the app ever runs
                // (missing git lands here) — without the log line a Windows
                // build (no console) leaves zero trace of why nothing cloned.
                log::warn!("[ui] trunk sync: clone failed: {detail}");
                self.syncing = false;
                self.clone_progress = None;
                self.job_failed = true;
                self.op_error = Some(detail.into());
            }
            SyncMsg::Failed(detail) => {
                log::warn!("[ui] trunk sync: op failed: {detail}");
                self.job_failed = true;
                self.op_error = Some(detail.into());
            }
            SyncMsg::AutoSyncDone(Ok(_outcome)) => {
                self.auto_sync_error = None;
            }
            SyncMsg::AutoSyncDone(Err(detail)) => {
                log::warn!("[ui] trunk sync: auto-sync failed: {detail}");
                // ONE sticky badge — never op_error, never a strip.
                self.job_failed = true;
                self.auto_sync_error = Some(detail.into());
            }
            SyncMsg::Trunk(Ok(trunk)) => {
                self.trunk = trunk;
                self.syncing = false;
                self.clone_progress = None;
                self.sync_seq += 1;
                if let Some(repo) = &mut self.repo {
                    repo.clone_exists = true;
                }
                if !self.job_failed {
                    self.last_synced = Some(Instant::now());
                    self.auto_sync_error = None;
                }
            }
            SyncMsg::Trunk(Err(_)) => {
                // Keep the last good state; a missing/corrupt clone stays on
                // whatever the resolve read (or `empty`).
                self.syncing = false;
                self.clone_progress = None;
            }
        }
        cx.notify();
    }
}

/// Whether a freshly read `pr_merged_at` is NEWER than the scope's baseline
/// (the EXP-346 merge trigger). Compares parsed instants, not the raw text:
/// Electric forwards Postgres `timestamptz` in whatever form the server
/// encodes (`2026-07-03 10:11:12.345+00`, RFC 3339 from tRPC, trailing
/// fractional zeros trimmed), and a lexicographic compare is only
/// decimal-correct by accident of that trimming. Unparseable text falls back
/// to the string order — never worse than the compare it replaced. A first
/// stamp (`None → Some`) counts as newer; a stamp never disappears.
/// Whether a doctor report should trigger the EXP-366 clone retry: only on
/// the git missing→present EDGE (`was == Some(false)`, never on the first
/// report or a steady green), never while an op is in flight, and only when
/// there is actually something to redo (no clone yet, or a sticky error).
/// Pure for the truth-table test below.
fn git_recovery_retry(
    was: Option<bool>,
    now_ok: bool,
    syncing: bool,
    clone_exists: bool,
    has_error: bool,
) -> bool {
    was == Some(false) && now_ok && !syncing && (!clone_exists || has_error)
}

fn stamp_is_newer(stamp: Option<&str>, baseline: Option<&str>) -> bool {
    match (stamp, baseline) {
        (Some(stamp), Some(baseline)) => {
            match (
                crate::inbox::parse_timestamp(stamp),
                crate::inbox::parse_timestamp(baseline),
            ) {
                (Some(fresh), Some(seen)) => fresh > seen,
                _ => stamp > baseline,
            }
        }
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// The pure core of [`TrunkSync::attention`] (unit-tested without a window):
/// the first reason the ff-only autopull is parked, in precedence order.
/// Mirrors [`TrunkState::ff_eligible`]'s refusals — every state that gate
/// rejects must have a voice here, or the trunk goes stale in silence.
fn attention_reason(trunk: &TrunkState) -> Option<SharedString> {
    if trunk.conflict.is_some() {
        return Some("a rebase/merge is paused with conflicts".into());
    }
    // A detached HEAD (`# branch.head (detached)`) has no branch to
    // fast-forward, so `ff_eligible` refuses it forever — silently, until
    // someone checks the default branch back out (the Source Control hatch
    // does exactly that).
    if trunk.branch.starts_with('(') {
        return Some("not on a branch. Auto-pull is paused.".into());
    }
    if trunk.ahead > 0 && trunk.has_upstream {
        let noun = if trunk.ahead == 1 { "commit" } else { "commits" };
        return Some(
            format!("{} local {noun} not on origin. Auto-pull is paused.", trunk.ahead).into(),
        );
    }
    if trunk.dirty {
        return Some("local changes in the working tree. Auto-pull is paused.".into());
    }
    None
}

/// The "synced Xm ago" pair: the short form + the tooltip sentence.
pub(crate) fn synced_ago_labels(elapsed: Duration) -> (SharedString, SharedString) {
    let secs = elapsed.as_secs();
    if secs < 60 {
        return ("now".into(), "Last synced just now".into());
    }
    let minutes = secs / 60;
    if minutes < 60 {
        let noun = if minutes == 1 { "minute" } else { "minutes" };
        return (
            format!("{minutes}m").into(),
            format!("Last synced {minutes} {noun} ago").into(),
        );
    }
    let hours = minutes / 60;
    let noun = if hours == 1 { "hour" } else { "hours" };
    (
        format!("{hours}h").into(),
        format!("Last synced {hours} {noun} ago").into(),
    )
}

/// The background side of [`TrunkSync::start_sync`]: token via the
/// process-wide cache, the git op (argv-only), then a trunk re-read —
/// streaming [`SyncMsg`]s to the foreground drain. A conflict left by a
/// failed rebase is picked up by the trailing trunk read, never auto-aborted
/// (except by the explicit HardReset hatch).
fn run_sync_worker(
    mode: SyncMode,
    trpc: &api::TrpcClient,
    repo: &RepoInfo,
    prune_policy: Option<&coding::PrunePolicy>,
    tx: &flume::Sender<SyncMsg>,
) {
    // A pre-transport failure (mint or ambient-auth install), surfaced
    // through whichever channel the badge reads. (Kinds copied out — the
    // closure must not capture `mode`, the match below consumes it.)
    let is_clone = matches!(mode, SyncMode::Clone);
    let is_auto_sync = matches!(mode, SyncMode::AutoSync);
    let send_failure = |detail: String| {
        let _ = tx.send(if is_clone {
            SyncMsg::Clone(CloneEvent::Failed(detail.clone()))
        } else if is_auto_sync {
            SyncMsg::AutoSyncDone(Err(detail.clone()))
        } else {
            SyncMsg::Failed(detail.clone())
        });
        let _ = tx.send(SyncMsg::Trunk(Err(detail)));
    };

    // Token via the cache (re-mints only near the REAL expiry; never
    // persisted/logged; reaches disk only as the clone's credential file).
    let minted = match coding::token_cache().get_or_mint(trpc, &repo.repository_id) {
        Ok(minted) => minted,
        Err(err) => {
            send_failure(err.to_string());
            return;
        }
    };
    let url = minted.url;
    let expires_at = minted.expires_at.as_deref();
    let clone: &Path = &repo.clone;

    // Ambient-auth install before any transport (EXP-73): downgrade-guarded,
    // so this 120-s/on-focus writer can never clobber a fresher token the
    // refresher installed (the exact postmortem failure). Clone mode installs
    // inside `clone_manager::ensure` — no `.git` exists yet here.
    if !is_clone {
        if let Err(err) = coding::git_credentials::ensure(clone, &url, expires_at) {
            send_failure(err.to_string());
            return;
        }
    }

    match mode {
        SyncMode::Clone => {
            let progress_tx = tx.clone();
            let mut on_event = move |event: CloneEvent| {
                let _ = progress_tx.send(SyncMsg::Clone(event));
            };
            // A clone failure already streamed `CloneEvent::Failed` through
            // the callback — nothing more to send here.
            let _ = clone_manager::ensure(
                &repo.repos_root,
                &repo.full_name,
                &url,
                expires_at,
                &mut on_event,
            );
        }
        SyncMode::Fetch => {
            // A freshness pass is fetch + the same ff-only catch-up AutoSync
            // runs: a refresh that KNOWS the tree is cleanly behind-only must
            // fast-forward it (dirty/diverged trees surface for the hatch).
            if let Err(err) = clone_manager::auto_sync(clone, &url) {
                let _ = tx.send(SyncMsg::Failed(err.to_string()));
            }
        }
        SyncMode::FetchOnly => {
            // The freshness pass under the live-task hold-off: fetch is
            // harmless, but the ff catch-up would move the working tree
            // under the running Claude task / Action — held off exactly
            // like the AutoSync pass.
            if let Err(err) = clone_manager::fetch(clone, &url) {
                let _ = tx.send(SyncMsg::Failed(err.to_string()));
            }
        }
        SyncMode::AutoSync => {
            let outcome = clone_manager::auto_sync(clone, &url).map_err(|err| err.to_string());
            let _ = tx.send(SyncMsg::AutoSyncDone(outcome));
            // EXP-465: reclaim landed worktrees + stale branches (their
            // ignored build caches are the disk cost). The policy was
            // derived on the foreground; the engine adds the git truth.
            // Best-effort, but no longer silent — regressions were invisible
            // under the old discarded result.
            if let Some(policy) = prune_policy {
                let report = coding::prune::prune_landed(clone, policy);
                if report.blocked_by_launch {
                    // EXP-478: a coding launch held the clone's gate — the
                    // 120s cadence retries on its own.
                    log::info!(
                        "[ui] auto-prune {}: skipped — a coding launch is in progress",
                        repo.full_name,
                    );
                } else if !report.is_empty() {
                    log::info!(
                        "[ui] auto-prune {}: removed worktrees {:?}, deleted branches {:?}, skipped {:?}",
                        repo.full_name,
                        report.removed_worktrees,
                        report.deleted_branches,
                        report.skipped,
                    );
                }
            }
        }
        SyncMode::HardReset => {
            // The escape hatch: abort an engaged rebase/merge first (best
            // effort — a plain dirty tree has nothing to abort), fetch, then
            // force-reset to the remote default branch (fallback: whatever is
            // checked out — never a fabricated `main`).
            if let Some(conflict) = scm::detect_conflict(clone) {
                if let Err(err) = scm::abort_conflict(clone, conflict.kind) {
                    log::warn!("[ui] hard reset: abort_conflict failed: {err}");
                }
            }
            let branch = repo
                .default_branch
                .clone()
                .filter(|name| !name.is_empty())
                .or_else(|| trunk_state::read(clone).ok().map(|state| state.branch))
                .unwrap_or_default();
            if branch.is_empty() || branch.starts_with('(') {
                let _ = tx.send(SyncMsg::Failed("No branch to reset to".to_string()));
            } else {
                let result = clone_manager::fetch(clone, &url)
                    .map_err(|err| err.to_string())
                    .and_then(|()| {
                        scm::hard_reset_to_remote(clone, &branch).map_err(|err| err.to_string())
                    });
                if let Err(err) = result {
                    let _ = tx.send(SyncMsg::Failed(err));
                }
            }
        }
        SyncMode::CommitPush { message, identity } => {
            // Re-derive conflict + trunk from DISK — a write op must never
            // trust the foreground snapshot the dialog was opened from.
            if scm::detect_conflict(clone).is_some() {
                let _ = tx.send(SyncMsg::Failed(
                    "A rebase/merge is paused — resolve it before pushing.".to_string(),
                ));
            } else {
                match trunk_state::read(clone) {
                    Ok(state) if state.branch.is_empty() || state.branch.starts_with('(') => {
                        let _ = tx.send(SyncMsg::Failed(
                            "Not on a branch — nothing to push.".to_string(),
                        ));
                    }
                    Ok(state) if !state.has_upstream => {
                        let _ = tx.send(SyncMsg::Failed(
                            "The branch has no upstream to push to.".to_string(),
                        ));
                    }
                    Ok(state) => {
                        let committed = if state.dirty {
                            match &message {
                                Some(message) => scm::stage_all_and_commit(
                                    clone,
                                    message,
                                    identity
                                        .as_ref()
                                        .map(|(name, email)| (name.as_str(), email.as_str())),
                                )
                                .map_err(|err| err.to_string()),
                                // The tree dirtied between the confirm and
                                // the run — a silent commit with no message
                                // would be worse than a retry.
                                None => Err(
                                    "The working tree changed — push again to include a \
                                     commit message."
                                        .to_string(),
                                ),
                            }
                        } else {
                            Ok(())
                        };
                        let result = committed.and_then(|()| {
                            scm::push(clone, &url, &state.branch).map_err(|err| err.to_string())
                        });
                        if let Err(err) = result {
                            let _ = tx.send(SyncMsg::Failed(err));
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(SyncMsg::Failed(err.to_string()));
                    }
                }
            }
        }
    }

    // Always re-derive the trunk from disk: a paused rebase engages conflict
    // mode even though the op returned an error.
    let trunk = trunk_state::read(clone).map_err(|err| err.to_string());
    let _ = tx.send(SyncMsg::Trunk(trunk));
}

/// EXP-465: whether the finished-set watch fires the auto-clean — only after
/// the baseline seeded, and only on a NEW member. A shrink (reopened issue)
/// re-baselines silently, and edits to already-finished issues change the
/// set not at all — neither may bypass the fetch debounce.
fn finished_set_fires(
    seeded: bool,
    previous: &std::collections::HashSet<String>,
    current: &std::collections::HashSet<String>,
) -> bool {
    seeded && current.difference(previous).next().is_some()
}

/// EXP-465: whether an auto-sync pass may carry a prune policy — only once
/// the issue AND board shapes reached their first `up-to-date`, the same
/// readiness the sibling triggers gate on. Anything less derives empty
/// policy sets and the engine would decide every `exp/` worktree by git
/// truth alone — force-removing fresh worktrees of open issues.
fn carry_prune_policy(issues_ready: bool, boards_ready: bool) -> bool {
    issues_ready && boards_ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::scm::{ConflictKind, ConflictState};

    fn trunk(branch: &str, ahead: u32, dirty: bool) -> TrunkState {
        TrunkState {
            branch: branch.to_string(),
            ahead,
            behind: 0,
            conflict: None,
            syncing: false,
            dirty,
            dirty_files: u32::from(dirty),
            has_upstream: true,
        }
    }

    #[test]
    fn finished_set_fires_only_on_a_new_member_after_seeding() {
        use std::collections::HashSet;
        let set = |ids: &[&str]| -> HashSet<String> {
            ids.iter().map(|id| id.to_string()).collect()
        };
        // The seeding read never fires, however many ids it carries.
        assert!(!finished_set_fires(false, &set(&[]), &set(&["a", "b"])));
        // No change → no fire; a NEW member fires.
        assert!(!finished_set_fires(true, &set(&["a"]), &set(&["a"])));
        assert!(finished_set_fires(true, &set(&["a"]), &set(&["a", "b"])));
        // A shrink (reopened issue) re-baselines without firing…
        assert!(!finished_set_fires(true, &set(&["a", "b"]), &set(&["a"])));
        // …and only a member NEW versus the shrunk baseline fires again.
        assert!(finished_set_fires(true, &set(&["a"]), &set(&["a", "c"])));
    }

    /// EXP-465 regression: the auto-sync pass may only carry a prune policy
    /// once BOTH the issue and board shapes are ready — a still-syncing
    /// (empty) issues collection derives empty keep sets, and git truth
    /// alone would force-remove the fresh 0-ahead worktree of an OPEN issue
    /// as "landed" debris.
    #[test]
    fn prune_policy_requires_ready_collections() {
        assert!(carry_prune_policy(true, true));
        assert!(!carry_prune_policy(false, true));
        assert!(!carry_prune_policy(true, false));
        assert!(!carry_prune_policy(false, false));
    }

    #[test]
    fn attention_is_silent_while_the_autopull_can_do_its_job() {
        // Clean on a branch — `ff_eligible`'s happy path, no badge.
        assert_eq!(attention_reason(&trunk("master", 0, false)), None);
        // Ahead without an upstream: nothing to be "not on origin" about.
        assert_eq!(
            attention_reason(&TrunkState { has_upstream: false, ..trunk("master", 3, false) }),
            None
        );
    }

    #[test]
    fn attention_names_every_state_ff_eligible_refuses() {
        assert_eq!(
            attention_reason(&trunk("master", 1, false)).as_deref(),
            Some("1 local commit not on origin. Auto-pull is paused.")
        );
        assert_eq!(
            attention_reason(&trunk("master", 0, true)).as_deref(),
            Some("local changes in the working tree. Auto-pull is paused.")
        );
        // A detached HEAD parks the autopull forever and used to show NOTHING.
        assert_eq!(
            attention_reason(&trunk("(detached)", 0, false)).as_deref(),
            Some("not on a branch. Auto-pull is paused.")
        );
        // A paused rebase/merge outranks the rest (it is the actionable one).
        let conflict = ConflictState { kind: ConflictKind::Rebase, files: Vec::new() };
        assert_eq!(
            attention_reason(&TrunkState {
                conflict: Some(conflict),
                ..trunk("(detached)", 2, true)
            })
            .as_deref(),
            Some("a rebase/merge is paused with conflicts")
        );
    }

    /// EXP-366: the clone retry fires ONLY on the git missing→present edge
    /// with something to redo — never on first sight, steady states, mid-op,
    /// or a healthy clone.
    #[test]
    fn git_recovery_retry_truth_table() {
        // The recovery edge: was-broken → now ok, clone missing.
        assert!(git_recovery_retry(Some(false), true, false, false, false));
        // …or clone present but a sticky error to clear (failed fetch).
        assert!(git_recovery_retry(Some(false), true, false, true, true));
        // First report ever — even a green one — is not an edge.
        assert!(!git_recovery_retry(None, true, false, false, false));
        // Steady green / steady red: no edge.
        assert!(!git_recovery_retry(Some(true), true, false, false, false));
        assert!(!git_recovery_retry(Some(false), false, false, false, false));
        // An op in flight is never preempted.
        assert!(!git_recovery_retry(Some(false), true, true, false, false));
        // Healthy clone, no error → nothing to redo.
        assert!(!git_recovery_retry(Some(false), true, false, true, false));
    }

    #[test]
    fn merge_stamp_compare_is_instant_based_not_lexicographic() {
        // The forms Electric and tRPC each hand us for the SAME instant must
        // compare equal — a raw string compare would call them different and
        // fire a spurious pull.
        assert!(!stamp_is_newer(
            Some("2026-07-03T10:11:12.000+00:00"),
            Some("2026-07-03 10:11:12+00"),
        ));
        // Trailing-zero trimming is what made the string compare work by
        // accident; the parse is right either way.
        assert!(stamp_is_newer(
            Some("2026-07-03 10:11:12.5+00"),
            Some("2026-07-03 10:11:12.25+00"),
        ));
        assert!(!stamp_is_newer(
            Some("2026-07-03 10:11:12.25+00"),
            Some("2026-07-03 10:11:12.5+00"),
        ));
        // Offsets are instants, not text.
        assert!(stamp_is_newer(
            Some("2026-07-03 11:00:00+00"),
            Some("2026-07-03 12:00:00+02"),
        ));
        // Unparseable text keeps the old string ordering.
        assert!(stamp_is_newer(Some("zzz"), Some("aaa")));
        // A first-ever merge triggers; a vanished stamp never does.
        assert!(stamp_is_newer(Some("2026-07-03 10:11:12+00"), None));
        assert!(!stamp_is_newer(None, Some("2026-07-03 10:11:12+00")));
        assert!(!stamp_is_newer(None, None));
    }
}
