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
//! flight or a Claude task / Action tab is alive for this repo (never
//! fast-forward the tree under Claude's feet). The background pass is
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
/// completion. (EXP-253 deleted the Push/Publish/GetLatest transport modes —
/// the IDE neither commits nor switches branches anymore.)
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncMode {
    /// Auto-clone the missing trunk (streams `git clone --progress` %).
    Clone,
    /// User freshness pass (refresh button / board open): fetch + the same
    /// ff-only catch-up AutoSync runs.
    Fetch,
    /// Background pass: fetch → ff-only when clean & behind-only, else skip
    /// (+ the EXP-76 worktree-prune nominations).
    AutoSync,
    /// The escape hatch: abort any in-progress rebase/merge, fetch,
    /// force-checkout the default branch, `reset --hard origin/<default>`.
    HardReset,
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
    _subscriptions: Vec<Subscription>,
}

impl TrunkSync {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
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

    /// Whether the trunk sits in conflict mode — the rail paints an amber
    /// badge on the Source Control icon off this.
    pub(crate) fn has_conflict(&self) -> bool {
        self.trunk.conflict.is_some()
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
    pub(crate) fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.start_sync(SyncMode::Fetch, cx);
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
                Some("A sync is already running — try the reset again in a moment.".into());
            cx.notify();
            return;
        }
        self.start_sync(SyncMode::HardReset, cx);
    }

    /// Debounced background sync trigger (timer tick + window focus): no-op
    /// while an op is in flight, before the clone exists, inside the
    /// [`clone_manager::FETCH_DEBOUNCE`] window, or while a Claude task /
    /// Action tab is alive for this repo (never move the tree under Claude's
    /// feet).
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
        if self
            .last_synced
            .is_some_and(|last| !clone_manager::should_fetch(last))
        {
            return;
        }
        if self.repo_tasks_alive(window, cx) {
            return;
        }
        let prune = self.prunable_worktree_branches(window, cx);
        self.start_sync_with_prune(SyncMode::AutoSync, prune, cx);
    }

    /// Branches whose session worktrees are safe to prune (EXP-76 disk
    /// hygiene, piggybacked on the auto-sync pass): synced issues of THIS
    /// repo's boards whose PR has merged, minus any issue with a running
    /// coding session (synced — covers every device) and minus worktrees
    /// hosting a live terminal tab in this window. The prune itself
    /// ([`git_worktree::prune_merged_worktrees`]) additionally refuses any
    /// worktree with modified or untracked files, so a tab in another window
    /// can at worst block on a clean tree it was merely cd'ed into.
    fn prunable_worktree_branches(&self, window: &Window, cx: &App) -> Vec<String> {
        let Some(repo) = &self.repo else {
            return Vec::new();
        };
        if !repo.clone_exists {
            return Vec::new();
        }
        let collections = Store::global(cx).collections().clone();
        let boards = collections.boards.read(cx);
        let repo_boards: Vec<&str> = boards
            .iter()
            .filter(|board| {
                board.repository_id.as_deref() == Some(repo.repository_id.as_str())
            })
            .map(|board| board.id.as_str())
            .collect();
        let sessions = collections.coding_sessions.read(cx);
        // Heartbeat-stale rows count as absent (EXP-153) — a phantom row must
        // not block cleaning up a merged branch forever.
        let now = chrono::Utc::now().timestamp();
        let running_issues: Vec<&str> = sessions
            .iter()
            .filter(|session| crate::queries::coding_session_is_live(session, now))
            .filter_map(|session| session.issue_id.as_deref())
            .collect();
        let issues = collections.issues.read(cx);
        let mut branches: Vec<String> = issues
            .iter()
            .filter(|issue| repo_boards.contains(&issue.board_id.as_str()))
            .filter(|issue| issue.pr_state.as_deref() == Some(domain::contract::PR_STATE_MERGED))
            .filter(|issue| !running_issues.contains(&issue.id.as_str()))
            .filter_map(|issue| issue.branch.clone())
            .collect();
        if branches.is_empty() {
            return branches;
        }
        if let Some(manager) = crate::coding_flow::window_terminal_manager(window, cx) {
            let busy: Vec<PathBuf> = manager
                .read(cx)
                .tabs()
                .iter()
                .filter(|tab| tab.is_running())
                .filter_map(|tab| tab.cwd.clone())
                .collect();
            branches.retain(|branch| {
                let worktree = git_worktree::worktree_path(&repo.clone, branch);
                !busy.iter().any(|cwd| cwd.starts_with(&worktree))
            });
        }
        branches
    }

    /// Whether a live Claude task or Action tab is working inside this
    /// repo's clone (or one of its worktrees) — the auto-sync hold-off
    /// (EXP-253 extended it to Action tabs: an action runs ON the trunk
    /// clone, so an ff under it would move the tree under Claude's feet).
    /// Shell tabs deliberately do NOT hold auto-sync off: a shell is alive
    /// for entire work sessions, and an ff under it is the same event as the
    /// manual pull it replaces.
    fn repo_tasks_alive(&self, window: &Window, cx: &App) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        let Some(manager) = crate::coding_flow::window_terminal_manager(window, cx) else {
            return false;
        };
        let worktrees = git_worktree::worktrees_dir(&repo.clone);
        manager.read(cx).tabs().iter().any(|tab| {
            matches!(tab.kind, TabKind::ClaudeTask | TabKind::Action(_))
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
            // Kill the previous scope's timer loop + in-flight job tail.
            self.generation += 1;
        }
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

        cx.spawn(async move |this, cx| {
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
            let _ = this.update(cx, |this, cx| {
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
                // trunk must never open stale when it could be current).
                this.start_sync(
                    if clone_exists {
                        SyncMode::Fetch
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
        self.start_sync_with_prune(mode, Vec::new(), cx);
    }

    /// [`Self::start_sync`] with the auto-sync pass's worktree-prune
    /// nominations (empty for every user-triggered op — pruning rides ONLY
    /// the background pass, whose trigger has the Window needed for the
    /// live-tab check).
    fn start_sync_with_prune(
        &mut self,
        mode: SyncMode,
        prune_branches: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.syncing {
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        // A user freshness pass on a missing clone RE-ATTEMPTS the clone —
        // the old error+Retry chrome is gone, so refresh is the retry path
        // after a failed auto-clone.
        let mode = if mode == SyncMode::Fetch && !repo.clone_exists {
            SyncMode::Clone
        } else {
            mode
        };

        self.syncing = true;
        self.job_failed = false;
        if mode != SyncMode::AutoSync {
            // A background pass must not clear a user-op error the user has
            // not acted on yet.
            self.op_error = None;
        }
        if mode == SyncMode::Clone {
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
                run_sync_worker(mode, &trpc, &repo, &prune_branches, &tx);
            })
            .detach();
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
                self.syncing = false;
                self.clone_progress = None;
                self.job_failed = true;
                self.op_error = Some(detail.into());
            }
            SyncMsg::Failed(detail) => {
                self.job_failed = true;
                self.op_error = Some(detail.into());
            }
            SyncMsg::AutoSyncDone(Ok(_outcome)) => {
                self.auto_sync_error = None;
            }
            SyncMsg::AutoSyncDone(Err(detail)) => {
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
    prune_branches: &[String],
    tx: &flume::Sender<SyncMsg>,
) {
    // A pre-transport failure (mint or ambient-auth install), surfaced
    // through whichever channel the badge reads.
    let send_failure = |detail: String| {
        let _ = tx.send(match mode {
            SyncMode::Clone => SyncMsg::Clone(CloneEvent::Failed(detail.clone())),
            SyncMode::AutoSync => SyncMsg::AutoSyncDone(Err(detail.clone())),
            _ => SyncMsg::Failed(detail.clone()),
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
    if mode != SyncMode::Clone {
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
        SyncMode::AutoSync => {
            let outcome = clone_manager::auto_sync(clone, &url).map_err(|err| err.to_string());
            let _ = tx.send(SyncMsg::AutoSyncDone(outcome));
            // EXP-76: reclaim merged sessions' worktrees (their ignored build
            // caches are the disk cost). Nominations were computed on the
            // foreground; the removal is quiet and best-effort — git itself
            // refuses any worktree with modified or untracked files.
            if !prune_branches.is_empty() {
                let _ = git_worktree::prune_merged_worktrees(clone, prune_branches);
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
    }

    // Always re-derive the trunk from disk: a paused rebase engages conflict
    // mode even though the op returned an error.
    let trunk = trunk_state::read(clone).map_err(|err| err.to_string());
    let _ = tx.send(SyncMsg::Trunk(trunk));
}
