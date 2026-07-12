//! The git cluster — trunk git chrome on the board screen's top-right
//! toolbar, deliberately compact: branch chip (dropdown switches branches +
//! "Check for updates"; dirty dot; synced-ago in the tooltip), Commit entry,
//! sync status (spinner / amber conflict chip / op error + Retry / sticky ⚠
//! badge), and the `↓behind ↑ahead` counts AS the one context-sensitive
//! action (`↓N` fast-forwards, `↑N` pushes, `↓N ↑M` rebase+pushes, Publish
//! for an unpublished branch; clean+in-sync renders nothing). Always
//! trunk-only: everything derives from the
//! active project's trunk clone on disk ([`coding::TrunkState`], re-read
//! after every op), so the chrome survives restarts and out-of-band fixes.
//!
//! Scope follows the window's navigation, exactly like the run widget: a
//! project board or an issue detail resolves to that project's primary repo;
//! other screens render nothing. On first resolve the bar kicks the
//! lifecycle — auto-clone when `<clone>/.git` is missing (progress streams
//! into the chip), else a freshness sync (fetch + ff-only catch-up) — then
//! reads the trunk state.
//!
//! **Auto-sync engine** lives here: a [`clone_manager::AUTO_SYNC_INTERVAL`]
//! timer plus a window-focus observer call [`GitBar::maybe_auto_sync`],
//! debounced through [`clone_manager::should_fetch`] and skipped while a
//! sync is in flight or a Claude TASK tab is alive for this repo (never
//! fast-forward the tree under Claude's feet; Run tabs — dev servers — do
//! NOT hold it off, an ff under them equals the manual pull it replaces).
//! The background pass is
//! [`clone_manager::auto_sync`]: fetch → fast-forward ONLY when clean +
//! behind-only; anything else is a loud-but-quiet Skipped outcome. Background
//! failures collapse into one sticky amber badge (cleared on the next
//! success) — separate from `op_error`, which belongs to user-clicked ops.
//!
//! **Every transport op targets the CHECKED-OUT branch**: the worker re-reads
//! `trunk_state` on the background executor and pushes/publishes/ffs that
//! branch — never a cached default-branch value. Tokens come from
//! [`coding::token_cache`]. A rebase/merge conflict is left in place (never
//! auto-aborted) and re-derived from disk into the amber chip. A checkout
//! that git refuses because local changes would be clobbered opens the
//! stash-and-switch dialog ([`GitBar::prompt_stash_switch`]); the stash is
//! restorable from Source Control's stash strip.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{
    div, px, App, ClickEvent, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    menu::DropdownMenu as _,
    spinner::Spinner, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;
use terminal::TabKind;

use crate::icons::ExpIcon;

use coding::scm::{self, BranchInfo};
use coding::{
    clone_manager, clone_path, git_worktree, trunk_state, AutoSyncOutcome, CloneEvent, Settings,
    TrunkState,
};

use crate::navigation::{self, Navigation};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::session::AuthContext;

/// The trunk repo a resolved project points at. All owned/`Send` so the whole
/// struct can ride onto the background executor for a git op.
#[derive(Clone)]
struct RepoInfo {
    /// `repositories.id` — the input to the token cache's mint.
    repository_id: String,
    /// `owner/name` — the clone-root key + the remote's redaction anchor.
    full_name: String,
    /// The trunk's server-reported default branch — used ONLY as the branch-chip
    /// display fallback until the on-disk status is read. Transport never
    /// reads it: every op targets the branch `trunk_state::read` reports.
    /// `None` when the server omitted it (never `main`).
    default_branch: Option<String>,
    /// `<repos_root>` — the clone-root prefix (`clone_manager::ensure` joins
    /// `full_name` onto it).
    repos_root: PathBuf,
    /// `<repos_root>/<owner>/<name>` — the trunk clone root.
    clone: PathBuf,
    /// Whether `<clone>/.git` exists (gates the auto-clone vs. fetch path and
    /// the transport enablement).
    clone_exists: bool,
}

/// Which git op a [`GitBar::start_sync`] runs on the background executor. All
/// token ops route through `coding::token_cache` and re-read the trunk on
/// completion.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncMode {
    /// Auto-clone the missing trunk (streams `git clone --progress` %).
    Clone,
    /// Freshness `git fetch origin` (project-open + "Up to date" click).
    Fetch,
    /// Background pass: fetch → ff-only when clean & behind-only, else skip.
    AutoSync,
    /// User-clicked fetch + `merge --ff-only` (git refuses loudly when the
    /// tree would be clobbered or has diverged).
    GetLatest,
    /// fetch → auto-rebase if behind → push — the checked-out branch.
    Push,
    /// `git push -u origin <branch>` for an unpublished branch.
    Publish,
}

/// Foreground-marshaled progress of a background git op. The worker streams
/// these through a [`flume`] channel; the drain applies them with `cx`
/// (`recv_async` off the gpui foreground, then `this.update`).
enum SyncMsg {
    /// A `git clone` lifecycle event (spinner + percentage).
    Clone(CloneEvent),
    /// A user-op failure detail (token already scrubbed) → `op_error`.
    Failed(String),
    /// The background auto-sync pass finished → `auto_sync_error` bookkeeping
    /// (NEVER `op_error` — one sticky badge, no error-strip flooding).
    AutoSyncDone(Result<AutoSyncOutcome, String>),
    /// The terminal on-disk trunk read (always sent last; `Err` keeps the
    /// prior state). A conflict engaged by a failed rebase surfaces HERE.
    Trunk(Result<TrunkState, String>),
    /// The local branch list (read alongside every trunk read — feeds the
    /// branch-chip menu and the Source Control tool window).
    Branches(Vec<BranchInfo>),
}

/// Render/load gate — mirrors the run bar's scope-follows-navigation state
/// machine.
enum Load {
    Idle,
    Loading,
    Ready,
}

/// The trunk git bar.
pub struct GitBar {
    nav: Entity<Navigation>,
    /// The shared per-window repo resolver — one `repositories.list` fetch
    /// for the whole window instead of a per-bar call.
    repo_resolver: Entity<RepoResolver>,
    /// The project scope the loaded state below belongs to (`None` off a
    /// project screen → the bar renders nothing).
    project_id: Option<String>,
    load: Load,
    /// The resolved trunk repo (`None` = no repo linked to the project).
    repo: Option<RepoInfo>,
    /// Repo-resolution problem (no repo linked / `repositories.list` failed) —
    /// a muted note in place of the chips.
    repo_error: Option<SharedString>,
    /// The on-disk trunk state (branch + dirty + upstream + ahead/behind +
    /// conflict).
    trunk: TrunkState,
    /// Local branches (current first) — refreshed with every trunk read.
    branches: Vec<BranchInfo>,
    /// A clone/fetch/push/… is in flight (spinner; blocks a second op).
    syncing: bool,
    /// `git clone --progress` percentage while cloning (`None` otherwise).
    clone_progress: Option<u8>,
    /// The last USER-op failure — the chip's error state (with Retry).
    op_error: Option<SharedString>,
    /// The last BACKGROUND auto-sync failure — the sticky amber `⚠ sync
    /// failed` badge, cleared on the next successful sync.
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
    /// surfaces (the Source Control screen's history pane, EXP-67) compare
    /// instead of diffing trunk snapshots.
    sync_seq: u64,
    _subscriptions: Vec<Subscription>,
}

impl GitBar {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Scope follows navigation (board / issue-detail → project).
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The issue→project join reads synced rows.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
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
            project_id: None,
            load: Load::Idle,
            repo: None,
            repo_error: None,
            trunk: TrunkState::empty(),
            branches: Vec::new(),
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

    /// The scope: the window's active project (screen scope with the
    /// last-board fallback) — populated on EVERY screen so the sidebar's
    /// Source Control tool window and the rail's conflict badge stay live.
    fn scope_project_id(&self, cx: &App) -> Option<String> {
        navigation::active_project_id(&self.nav, cx)
    }

    /// Whether the trunk sits in conflict mode — the sidebar rail paints an
    /// amber badge on the Source Control icon off this.
    pub(crate) fn has_conflict(&self) -> bool {
        self.trunk.conflict.is_some()
    }

    /// The checked-out branch as of the last on-disk read (empty until read).
    pub(crate) fn branch(&self) -> &str {
        &self.trunk.branch
    }

    /// Local branches (current first) — the sidebar's Source Control tool
    /// window renders these.
    pub(crate) fn branches(&self) -> &[BranchInfo] {
        &self.branches
    }

    /// The resolved trunk clone root on disk (`None` while unresolved or on
    /// a repo-less project) — the sidebar flow view's git-op target.
    pub(crate) fn clone_dir(&self) -> Option<PathBuf> {
        self.repo
            .as_ref()
            .filter(|repo| repo.clone_exists)
            .map(|repo| repo.clone.clone())
    }

    /// The server-reported default branch (`None` when the server omitted
    /// it — never fabricated as `main`).
    pub(crate) fn default_branch(&self) -> Option<String> {
        self.repo
            .as_ref()
            .and_then(|repo| repo.default_branch.clone())
            .filter(|name| !name.is_empty())
    }

    /// Whether a branch read has happened yet (`false` = show a skeleton, not
    /// a false "no branches").
    pub(crate) fn branches_ready(&self) -> bool {
        !self.branches.is_empty() || matches!(self.load, Load::Ready if !self.syncing)
    }

    /// Freshness fetch + trunk/branch re-read (the sidebar's refresh button).
    pub(crate) fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.start_sync(SyncMode::Fetch, cx);
    }

    /// Local-only re-read of trunk state + branch list (no fetch, no token)
    /// — the cheap nudge after some OTHER surface mutates the repo on disk
    /// (a session launch created a worktree, the Changes tab cleaned one
    /// up…). The sidebar flow graph observes the bar, so its lanes update
    /// immediately instead of waiting for the next auto-sync tick. No-op
    /// mid-sync: that op's trailing read supersedes it anyway.
    pub(crate) fn reread_local(&mut self, cx: &mut gpui::Context<Self>) {
        if self.syncing {
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if !repo.clone_exists {
            return;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let clone = repo.clone.clone();
            let (trunk, branches) = cx
                .background_executor()
                .spawn(async move {
                    (
                        trunk_state::read(&clone).ok(),
                        scm::branches(&clone).unwrap_or_default(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if let Some(trunk) = trunk {
                    this.trunk = trunk;
                }
                this.branches = branches;
                this.sync_seq += 1;
                cx.notify();
            });
        })
        .detach();
    }

    /// Debounced background sync trigger (timer tick + window focus): no-op
    /// while an op is in flight, before the clone exists, inside the
    /// [`clone_manager::FETCH_DEBOUNCE`] window, or while a Claude task tab
    /// is alive for this repo (never move the tree under Claude's feet).
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
        self.start_sync(SyncMode::AutoSync, cx);
    }

    /// Whether a live Claude TASK tab (conflict fix, run-config authoring…)
    /// is working inside this repo's clone (or one of its worktrees) — the
    /// auto-sync hold-off. Run tabs deliberately do NOT hold auto-sync off:
    /// a dev server is alive for entire work sessions, and an ff under it is
    /// the same event as the manual pull it replaces — blocking on Run tabs
    /// would disable auto-sync exactly when the IDE is in use.
    fn repo_tasks_alive(&self, window: &Window, cx: &App) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        let Some(manager) = crate::coding_flow::window_terminal_manager(window, cx) else {
            return false;
        };
        let worktrees = git_worktree::worktrees_dir(&repo.clone);
        manager.read(cx).tabs().iter().any(|tab| {
            matches!(tab.kind, TabKind::ClaudeTask)
                && tab.is_running()
                && tab
                    .cwd
                    .as_deref()
                    .is_some_and(|cwd| cwd.starts_with(&repo.clone) || cwd.starts_with(&worktrees))
        })
    }

    /// Check out `branch` on the trunk clone, then re-read trunk state +
    /// branches from disk. Purely local (no token, no network). When git
    /// refuses because local changes would be clobbered, the stash-and-switch
    /// dialog opens; other errors surface in the status segment.
    pub(crate) fn checkout(
        &mut self,
        branch: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.syncing {
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if !repo.clone_exists || self.trunk.branch == branch {
            return;
        }
        self.syncing = true;
        self.op_error = None;
        cx.notify();
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let clone = repo.clone.clone();
            let target = branch.clone();
            let (result, trunk, branches) = cx
                .background_executor()
                .spawn(async move {
                    let result = scm::checkout(&clone, &target).map_err(|err| err.detail);
                    // Always re-derive from disk, even after a failed checkout.
                    let trunk = trunk_state::read(&clone).ok();
                    let branches = scm::branches(&clone).unwrap_or_default();
                    (result, trunk, branches)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation {
                    return; // superseded by a scope change
                }
                this.syncing = false;
                if let Err(detail) = result {
                    if scm::checkout_blocked_by_local_changes(&detail) {
                        this.prompt_stash_switch(branch, window, cx);
                    } else {
                        this.op_error = Some(detail.into());
                    }
                }
                if let Some(trunk) = trunk {
                    this.trunk = trunk;
                }
                this.branches = branches;
                cx.notify();
            });
        })
        .detach();
    }

    /// The dirty-switch dialog: git refused the checkout because local
    /// changes would be clobbered. "Stash changes & switch" stashes with the
    /// `exp-switch: <branch>` tag (restorable from Source Control's stash
    /// strip) and re-runs the checkout. Deliberately NO "bring my changes"
    /// option — git's native non-conflicting carry-over already happened for
    /// the safe cases, and a forced carry has documented data-loss failure
    /// modes.
    fn prompt_stash_switch(
        &mut self,
        target: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let bar = cx.entity().downgrade();
        // Alert dialog, not a plain dialog: only AlertDialog renders the
        // button_props footer — a plain Dialog shows title/body and NO
        // ok/cancel buttons.
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let bar = bar.clone();
            let target_for_ok = target.clone();
            alert
                .confirm()
                // Dismissable like any dialog: overlay click, Esc, and the ✕
                // all cancel (AlertDialog's default locks all three off).
                .overlay_closable(true)
                .close_button(true)
                .width(px(416.))
                .title(SharedString::from(format!("Switch to {target}?")))
                .description(
                    "Your local changes would be overwritten by switching. \
                     Stash them and switch? The stash can be restored from \
                     Source Control.",
                )
                .button_props(DialogButtonProps::default().ok_text("Stash changes & switch"))
                .on_ok(move |_, _, cx| {
                    if let Some(bar) = bar.upgrade() {
                        let target = target_for_ok.clone();
                        bar.update(cx, |bar, cx| bar.stash_and_switch(target, cx));
                    }
                    true
                })
        });
    }

    /// The dialog's confirm path: `git stash push` tagged `exp-switch:
    /// <current>`, then the checkout, then a fresh disk read.
    fn stash_and_switch(&mut self, target: String, cx: &mut gpui::Context<Self>) {
        if self.syncing {
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let stash_tag = scm::stash_switch_message(&self.trunk.branch);
        self.syncing = true;
        self.op_error = None;
        cx.notify();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let clone = repo.clone.clone();
            let (result, trunk, branches) = cx
                .background_executor()
                .spawn(async move {
                    let result = scm::stash_push(&clone, &stash_tag)
                        .and_then(|()| scm::checkout(&clone, &target))
                        .map_err(|err| err.to_string());
                    let trunk = trunk_state::read(&clone).ok();
                    let branches = scm::branches(&clone).unwrap_or_default();
                    (result, trunk, branches)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.syncing = false;
                if let Err(err) = result {
                    this.op_error = Some(err.into());
                }
                if let Some(trunk) = trunk {
                    this.trunk = trunk;
                }
                this.branches = branches;
                cx.notify();
            });
        })
        .detach();
    }

    /// Render-time load gate: a scope change resets, `Idle` kicks one
    /// background resolve of the project's trunk repo + its on-disk state,
    /// then the lifecycle (auto-clone / freshness fetch) and the per-scope
    /// auto-sync timer loop. `pub(crate)` so the sidebar rail can keep the
    /// auto-clone lifecycle + conflict badge live even while the Source
    /// Control tool window is closed.
    pub(crate) fn ensure_loaded(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // workspace, shared by all five trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        let scope = self.scope_project_id(cx);
        if scope != self.project_id {
            self.project_id = scope;
            self.load = Load::Idle;
            self.repo = None;
            self.repo_error = None;
            self.trunk = TrunkState::empty();
            self.branches = Vec::new();
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
        let Some(project_id) = self.project_id.clone() else {
            return;
        };
        // Read the shared resolution rather than firing our own network call.
        let meta = match self.repo_resolver.read(cx).lookup_project(&project_id) {
            RepoLookup::Loading => return, // the resolver observer re-renders us
            RepoLookup::Found(repo) => repo,
            RepoLookup::NotFound => {
                self.load = Load::Ready;
                self.repo = None;
                self.repo_error = Some("No repository linked to this project.".into());
                return;
            }
            RepoLookup::Error(message) => {
                log::warn!("[ui] git bar: repo resolution failed: {message}");
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
            let project = project_id.clone();
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let clone = clone_path(&repos_root, &meta.full_name);
                    let clone_exists = clone.join(".git").exists();
                    // Read the on-disk trunk + branch list up front so an
                    // existing clone paints before the fetch.
                    let (trunk, branches) = if clone_exists {
                        (
                            trunk_state::read(&clone).ok(),
                            scm::branches(&clone).unwrap_or_default(),
                        )
                    } else {
                        (None, Vec::new())
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
                        branches,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation
                    || this.project_id.as_deref() != Some(project.as_str())
                {
                    return; // superseded by a scope change
                }
                this.load = Load::Ready;
                let (repo, trunk, branches) = resolved;
                if let Some(trunk) = trunk {
                    this.trunk = trunk;
                }
                this.branches = branches;
                this.sync_seq += 1;
                let clone_exists = repo.clone_exists;
                this.repo = Some(repo);
                this.repo_error = None;
                // Auto-clone a missing trunk, else a freshness sync on
                // project open (fetch + ff when cleanly behind-only — the
                // bar must never open onto a stale ↓N it could resolve).
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
        if self.syncing {
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
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
                    break; // the bar was dropped (window closed)
                }
            }
        })
        .detach();

        // Background worker — token + git op + trunk read (argv-only git).
        cx.background_executor()
            .spawn(async move {
                run_sync_worker(mode, &trpc, &repo, &tx);
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
                // ONE sticky amber badge — never op_error, never a strip.
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
            SyncMsg::Branches(branches) => {
                self.branches = branches;
                self.sync_seq += 1;
            }
        }
        cx.notify();
    }

    /// Open Source Control (branch chip / Commit / conflict chip target):
    /// selects the rail tool (branches in the sidebar) AND navigates to the
    /// changes screen.
    fn open_source_control(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::SourceControl);
    }

    // ------------------------------ render --------------------------------

    /// The branch chip: `⎇ <branch>` (+ `●` while the tree is dirty) — a
    /// dropdown that SWITCHES branches (checkout on the trunk clone) plus the
    /// entry to the changes view and a manual "Check for updates". Falls back
    /// to the default branch label until the on-disk status is read. The
    /// "synced Xm ago" stamp lives in the tooltip (compact bar — it earns no
    /// standing width of its own).
    fn render_branch_chip(&self, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let branch = if !self.trunk.branch.is_empty() {
            self.trunk.branch.clone()
        } else {
            self.repo
                .as_ref()
                .and_then(|repo| repo.default_branch.clone())
                .unwrap_or_default()
        };
        let label = if self.trunk.dirty {
            format!("\u{2387} {branch} \u{25CF}")
        } else {
            format!("\u{2387} {branch}")
        };
        let mut tooltip = if self.trunk.dirty {
            "Switch branch (uncommitted changes)".to_string()
        } else {
            "Switch branch".to_string()
        };
        if let Some(last) = self.last_synced {
            let (_, synced) = synced_ago_labels(last.elapsed());
            tooltip = format!("{tooltip} — {synced}");
        }
        // Snapshot for the lazy menu builder (menus must not read `self`).
        // Branches living in session worktrees are excluded — git refuses a
        // second checkout, so offering them would only ever error.
        let picker: Vec<(String, bool)> = self
            .branches
            .iter()
            .filter(|b| !b.worktree)
            .map(|b| (b.name.clone(), b.current))
            .collect();
        Button::new("git-branch-chip")
            .ghost()
            .xsmall()
            .label(SharedString::from(label))
            .tooltip(SharedString::from(tooltip))
            .dropdown_menu(move |menu, _window, _cx| {
                // Branch lists grow with the repo — cap + scroll (EXP-46a).
                // Flat items only (no submenus).
                let mut menu = menu.scrollable(true).max_h(px(320.)).label("Branches");
                for (name, current) in &picker {
                    menu = menu.menu_with_check(
                        SharedString::from(name.clone()),
                        *current,
                        Box::new(crate::actions::SwitchBranch {
                            branch: name.clone(),
                        }),
                    );
                }
                menu.separator()
                    .menu_with_icon(
                        "Open changes view",
                        Icon::from(ExpIcon::GitMerge),
                        Box::new(crate::actions::OpenSourceControl),
                    )
                    .menu("Check for updates", Box::new(crate::actions::SyncNow))
            })
    }

    /// The Commit entry (JetBrains-style): opens the Source Control screen
    /// (commit box + changes) with branches in the sidebar.
    fn render_commit_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let clone_exists = self.repo.as_ref().is_some_and(|repo| repo.clone_exists);
        Button::new("git-commit")
            .ghost()
            .xsmall()
            .icon(Icon::from(ExpIcon::Check))
            .disabled(!clone_exists)
            .tooltip(if clone_exists {
                "Commit…"
            } else {
                "Trunk not cloned yet"
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.open_source_control(window, cx);
            }))
    }

    /// The middle segment: sync spinner (+ clone % while cloning), OR the op
    /// error + Retry, OR the amber `⚠ N conflicts` chip, OR the sticky
    /// background-sync-failed badge. The behind/ahead counts are NOT here —
    /// they render as the context ACTION chip (click ↓N to pull it).
    fn render_status(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut row = h_flex().gap_1().items_center();

        // Repo-resolution problem takes over the whole segment.
        if let Some(error) = &self.repo_error {
            return row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(error.clone()),
            );
        }

        // Sync spinner — bare while syncing (the op is obvious from the chip
        // it replaces); cloning keeps its % label, that one is long-running.
        if self.syncing {
            row = row.child(Spinner::new().xsmall().color(cx.theme().muted_foreground));
            if let Some(percent) = self.clone_progress {
                let name = self
                    .repo
                    .as_ref()
                    .map(|repo| short_name(&repo.full_name))
                    .unwrap_or_default();
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!("Cloning {name}… {percent}%"))),
                );
            }
            return row;
        }

        // A user-op error (clone/fetch/push/checkout — error + Retry).
        // Truncated hard — git errors can be full sentences and must not
        // flood the top bar.
        if let Some(error) = &self.op_error {
            return row
                .child(
                    div()
                        .max_w(gpui::px(320.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                )
                .child(
                    Button::new("git-retry")
                        .ghost()
                        .xsmall()
                        .label("Retry")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let mode = if this
                                .repo
                                .as_ref()
                                .is_some_and(|repo| repo.clone_exists)
                            {
                                SyncMode::Fetch
                            } else {
                                SyncMode::Clone
                            };
                            this.start_sync(mode, cx);
                        })),
                );
        }

        // Conflict mode: the counts are replaced by the amber chip.
        if let Some(conflict) = &self.trunk.conflict {
            let count = conflict.files.len();
            let noun = if count == 1 { "conflict" } else { "conflicts" };
            return row.child(
                Button::new("git-conflicts")
                    .ghost()
                    .xsmall()
                    .label(SharedString::from(format!("\u{26A0} {count} {noun}")))
                    .text_color(cx.theme().warning)
                    .tooltip("Resolve in Source Control")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_source_control(window, cx);
                    })),
            );
        }

        // Sticky background-sync failure: ONE amber ⚠, cleared on the next
        // success; the detail lives in the tooltip, click retries.
        if let Some(detail) = &self.auto_sync_error {
            row = row.child(
                Button::new("git-sync-failed")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::TriangleAlert))
                    .text_color(cx.theme().warning)
                    .tooltip(SharedString::from(format!("Background sync failed: {detail}")))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_sync(SyncMode::AutoSync, cx);
                    })),
            );
        }
        row
    }

    /// ONE context-sensitive action for the checked-out branch, worn by the
    /// counts themselves — the number IS the button: `↓N` fast-forwards,
    /// `↑N` pushes, `↓N ↑M` syncs (rebase + push), and an unpublished branch
    /// gets the labeled Publish. A clean, in-sync trunk renders NOTHING
    /// (auto-sync owns freshness; manual re-check lives in the branch menu).
    fn render_context_action(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let clone_exists = self.repo.as_ref().is_some_and(|repo| repo.clone_exists);
        if !clone_exists
            || self.syncing
            || self.trunk.conflict.is_some()
            || self.trunk.branch.is_empty()
            || self.trunk.branch.starts_with('(')
        {
            return None;
        }
        if !self.trunk.has_upstream {
            return Some(
                Button::new("git-publish")
                    .primary()
                    .xsmall()
                    .label("Publish")
                    .tooltip(SharedString::from(format!(
                        "Publish {} to origin",
                        self.trunk.branch
                    )))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_sync(SyncMode::Publish, cx);
                    }))
                    .into_any_element(),
            );
        }
        let (behind, ahead) = (self.trunk.behind, self.trunk.ahead);
        let (id, label, tooltip, mode): (&'static str, String, String, SyncMode) =
            match (behind > 0, ahead > 0) {
                // Rare: auto-ff usually beat it; shows when the tree is dirty.
                (true, false) => (
                    "git-get-latest",
                    format!("\u{2193}{behind}"),
                    format!("Fast-forward to origin/{}", self.trunk.branch),
                    SyncMode::GetLatest,
                ),
                (false, true) => (
                    "git-push",
                    format!("\u{2191}{ahead}"),
                    format!("Push {} to origin", self.trunk.branch),
                    SyncMode::Push,
                ),
                (true, true) => (
                    "git-sync",
                    format!("\u{2193}{behind} \u{2191}{ahead}"),
                    format!("Rebase onto origin/{}, then push", self.trunk.branch),
                    SyncMode::Push,
                ),
                (false, false) => return None,
            };
        Some(
            Button::new(id)
                .ghost()
                .xsmall()
                .label(SharedString::from(label))
                .text_color(cx.theme().muted_foreground)
                .tooltip(SharedString::from(tooltip))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.start_sync(mode, cx);
                }))
                .into_any_element(),
        )
    }
}

impl Render for GitBar {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(window, cx);

        // Trunk chrome only on a project scope (mirrors the Run section's
        // self-hide) — the sidebar's project panel collapses to nothing on
        // other screens.
        if self.project_id.is_none() {
            return div().into_any_element();
        }

        // Compact horizontal cluster for the board's top-right toolbar:
        // branch chip · Commit · status (spinner/errors/conflict/badge) ·
        // count-chip action. In-sync + clean = just the chip and the ✓.
        let mut row = h_flex()
            .gap_1()
            .items_center()
            .child(self.render_branch_chip(cx))
            .child(self.render_commit_button(cx))
            .child(self.render_status(cx));
        if let Some(action) = self.render_context_action(cx) {
            row = row.child(action);
        }
        row.into_any_element()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// `owner/name` → `name` (the clone progress chip's short label).
fn short_name(full_name: &str) -> &str {
    full_name.rsplit('/').next().unwrap_or(full_name)
}

/// The "synced Xm ago" pair: the bar's short form + the tooltip sentence.
fn synced_ago_labels(elapsed: Duration) -> (SharedString, SharedString) {
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

/// The background side of [`GitBar::start_sync`]: token via the process-wide
/// cache, the git op (argv-only), then a trunk re-read — streaming
/// [`SyncMsg`]s to the foreground drain. **Every transport op targets the
/// branch a fresh `trunk_state::read` reports** (never a cached default), so
/// the bar can never push a branch other than the one checked out. A conflict
/// left by a failed rebase is picked up by the trailing trunk read, never
/// auto-aborted.
fn run_sync_worker(
    mode: SyncMode,
    trpc: &api::TrpcClient,
    repo: &RepoInfo,
    tx: &flume::Sender<SyncMsg>,
) {
    // Token via the cache (re-mints only near expiry; never persisted/logged;
    // only ever rides in the token remote URL, redacted in every error).
    let minted = match coding::token_cache().get_or_mint(trpc, &repo.repository_id) {
        Ok(minted) => minted,
        Err(err) => {
            let detail = err.to_string();
            // Surface through whichever channel the segment reads.
            let _ = tx.send(match mode {
                SyncMode::Clone => SyncMsg::Clone(CloneEvent::Failed(detail.clone())),
                SyncMode::AutoSync => SyncMsg::AutoSyncDone(Err(detail.clone())),
                _ => SyncMsg::Failed(detail.clone()),
            });
            let _ = tx.send(SyncMsg::Trunk(Err(detail)));
            return;
        }
    };
    let url = minted.url;
    let clone: &Path = &repo.clone;

    match mode {
        SyncMode::Clone => {
            let progress_tx = tx.clone();
            let mut on_event = move |event: CloneEvent| {
                let _ = progress_tx.send(SyncMsg::Clone(event));
            };
            // A clone failure already streamed `CloneEvent::Failed` through
            // the callback — nothing more to send here.
            let _ = clone_manager::ensure(&repo.repos_root, &repo.full_name, &url, &mut on_event);
        }
        SyncMode::Fetch => {
            // A freshness pass is fetch + the same ff-only catch-up AutoSync
            // runs: a refresh that KNOWS the tree is cleanly behind-only must
            // fast-forward it, not park the count on "Get latest" until the
            // next timer tick (dirty/diverged trees still surface manually).
            if let Err(err) = clone_manager::auto_sync(clone, &url) {
                let _ = tx.send(SyncMsg::Failed(err.to_string()));
            }
        }
        SyncMode::AutoSync => {
            let outcome = clone_manager::auto_sync(clone, &url).map_err(|err| err.to_string());
            let _ = tx.send(SyncMsg::AutoSyncDone(outcome));
        }
        SyncMode::GetLatest | SyncMode::Push | SyncMode::Publish => {
            // The C fix: read the CHECKED-OUT branch fresh from disk — the
            // one thing every transport op is allowed to target.
            let branch = trunk_state::read(clone)
                .map(|state| state.branch)
                .unwrap_or_default();
            if branch.is_empty() || branch.starts_with('(') {
                let _ = tx.send(SyncMsg::Failed("No branch checked out".to_string()));
            } else {
                let result = match mode {
                    SyncMode::GetLatest => clone_manager::fetch(clone, &url)
                        .and_then(|()| clone_manager::ff_update(clone, &branch)),
                    SyncMode::Push => clone_manager::push(clone, &branch, &url),
                    SyncMode::Publish => clone_manager::publish(clone, &branch, &url),
                    _ => unreachable!("outer match covers the transport modes"),
                };
                if let Err(err) = result {
                    let _ = tx.send(SyncMsg::Failed(err.to_string()));
                }
            }
        }
    }

    // Always re-derive the trunk from disk: a paused rebase engages conflict
    // mode even though the op returned an error.
    let trunk = trunk_state::read(clone).map_err(|err| err.to_string());
    let _ = tx.send(SyncMsg::Trunk(trunk));
    let _ = tx.send(SyncMsg::Branches(scm::branches(clone).unwrap_or_default()));
}
