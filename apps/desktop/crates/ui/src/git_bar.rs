//! The git cluster (masterplan v4 §4.3) — trunk git chrome on the board
//! screen's top-right toolbar: branch chip (click → Source Control), sync
//! spinner / clone progress %, behind/ahead counts, ghost Pull/Push buttons,
//! and (in conflict mode) an amber `⚠ N conflicts` chip. Always trunk-only
//! (v4 §4.2 rule 1): everything here derives from the active project's trunk
//! clone on disk ([`coding::TrunkState`], read after each
//! clone/fetch/pull/push), so the chrome survives restarts and out-of-band
//! fixes (v4 §4.2 rule 3).
//!
//! Scope follows the window's navigation, exactly like the run widget: a
//! project board or an issue detail resolves to that project's primary repo;
//! other screens render nothing. On first resolve the bar kicks the §4.1
//! lifecycle — auto-clone when `<clone>/.git` is missing (progress streams into
//! the chip), else a freshness fetch — then reads the trunk state. Pull/Push
//! re-mint a JIT installation token and drive [`coding::clone_manager`]; a
//! rebase/merge conflict is left in place (never auto-aborted) and re-derived
//! from disk into the amber chip.

use std::path::{Path, PathBuf};

use gpui::{
    div, App, ClickEvent, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    spinner::Spinner, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::icons::ExpIcon;

use coding::scm::{self, BranchInfo};
use coding::{clone_manager, clone_path, trunk_state, CloneEvent, Settings, TokenUrl, TrunkState};

use crate::navigation::{self, Navigation};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::session::AuthContext;

/// The trunk repo a resolved project points at (v4 §4.2). All owned/`Send` so
/// the whole struct can ride onto the background executor for a git op.
#[derive(Clone)]
struct RepoInfo {
    /// `repositories.id` — the input to `repositories.installationToken`.
    repository_id: String,
    /// `owner/name` — the clone-root key + the remote's redaction anchor.
    full_name: String,
    /// The trunk's server-reported default branch — used ONLY as the branch-chip
    /// display fallback until the on-disk status is read. The actual pull/push
    /// target is the freshly-minted token's `default_branch` (L30), never
    /// this cached scope value. `None` when the server omitted it (never `main`).
    default_branch: Option<String>,
    /// `<repos_root>` — the clone-root prefix (`clone_manager::ensure` joins
    /// `full_name` onto it).
    repos_root: PathBuf,
    /// `<repos_root>/<owner>/<name>` — the trunk clone root.
    clone: PathBuf,
    /// Whether `<clone>/.git` exists (gates the auto-clone vs. fetch path and
    /// the Pull/Push enablement).
    clone_exists: bool,
}

/// Which git op a [`GitBar::start_sync`] runs on the background executor. All
/// re-mint the token + re-read the trunk on completion (v4 §4.1).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncMode {
    /// Auto-clone the missing trunk (streams `git clone --progress` %).
    Clone,
    /// Freshness `git fetch origin` (v4 §4.1 project-open path).
    Fetch,
    /// `git pull --rebase --autostash` (respecting `pull.rebase=false`).
    Pull,
    /// fetch → auto-rebase if behind → push.
    Push,
}

/// Foreground-marshaled progress of a background git op (v4 §4.1). The worker
/// streams these through a [`flume`] channel; the drain applies them with `cx`
/// (`recv_async` off the gpui foreground, then `this.update`).
enum SyncMsg {
    /// A `git clone` lifecycle event (spinner + percentage).
    Clone(CloneEvent),
    /// A fetch/pull/push failure detail (token already scrubbed).
    Failed(String),
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

/// The trunk git bar (v4 §4.3).
pub struct GitBar {
    nav: Entity<Navigation>,
    /// The shared per-window repo resolver (§4.2) — one `repositories.list`
    /// fetch for the whole window instead of a per-bar call.
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
    /// The on-disk trunk state (branch + ahead/behind + conflict).
    trunk: TrunkState,
    /// Local branches (current first) — refreshed with every trunk read.
    branches: Vec<BranchInfo>,
    /// A clone/fetch/pull/push is in flight (spinner; disables Pull/Push).
    syncing: bool,
    /// `git clone --progress` percentage while cloning (`None` otherwise).
    clone_progress: Option<u8>,
    /// The last op failure — the chip's error state (with a Retry affordance).
    op_error: Option<SharedString>,
    /// Scope generation — bumped on every scope change so a stale background
    /// job's marshaled messages are ignored.
    generation: u64,
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
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// The §4.2 scope: the window's active project (screen scope with the
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

    /// Whether a branch read has happened yet (`false` = show a skeleton, not
    /// a false "no branches").
    pub(crate) fn branches_ready(&self) -> bool {
        !self.branches.is_empty() || matches!(self.load, Load::Ready if !self.syncing)
    }

    /// Freshness fetch + trunk/branch re-read (the sidebar's refresh button).
    pub(crate) fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.start_sync(SyncMode::Fetch, cx);
    }

    /// Check out `branch` on the trunk clone, then re-read trunk state +
    /// branches from disk. Purely local (no token, no network); a dirty tree
    /// surfaces git's own error in the status segment.
    pub(crate) fn checkout(&mut self, branch: String, cx: &mut gpui::Context<Self>) {
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
        cx.spawn(async move |this, cx| {
            let clone = repo.clone.clone();
            let (result, trunk, branches) = cx
                .background_executor()
                .spawn(async move {
                    let result = scm::checkout(&clone, &branch).map_err(|err| err.to_string());
                    // Always re-derive from disk, even after a failed checkout.
                    let trunk = trunk_state::read(&clone).ok();
                    let branches = scm::branches(&clone).unwrap_or_default();
                    (result, trunk, branches)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a scope change
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
    /// background resolve of the project's trunk repo + its on-disk state, then
    /// the §4.1 lifecycle (auto-clone / freshness fetch). `pub(crate)` so the
    /// sidebar rail can keep the auto-clone lifecycle + conflict badge live
    /// even while the Source Control tool window is closed.
    pub(crate) fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
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
                let clone_exists = repo.clone_exists;
                this.repo = Some(repo);
                this.repo_error = None;
                // §4.1: auto-clone a missing trunk, else a freshness fetch on
                // project open.
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

    /// Spawn a background git op (v4 §4.1): re-mint the JIT token, run it, and
    /// re-read the trunk. Progress marshals to the foreground through a
    /// [`flume`] channel drained here. No-op while another op is in flight
    /// (one trunk op at a time) or off a resolved repo.
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
        self.op_error = None;
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

        // Background worker — mint + git op + trunk read (argv-only git, §L5).
        cx.background_executor()
            .spawn(async move {
                run_sync_worker(mode, &trpc, &repo, &tx);
            })
            .detach();
    }

    /// Apply one marshaled [`SyncMsg`] on the foreground (v4 §4.1). Stale
    /// messages (a superseded scope) are dropped by the generation guard.
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
                self.op_error = Some(detail.into());
            }
            SyncMsg::Failed(detail) => {
                self.op_error = Some(detail.into());
            }
            SyncMsg::Trunk(Ok(trunk)) => {
                self.trunk = trunk;
                self.syncing = false;
                self.clone_progress = None;
                if let Some(repo) = &mut self.repo {
                    repo.clone_exists = true;
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

    /// The branch chip (v4 §4.3): `⎇ <branch>` — a dropdown that SWITCHES
    /// branches (checkout on the trunk clone) plus the entry to the changes
    /// view. Falls back to the default branch label until the on-disk status
    /// is read.
    fn render_branch_chip(&self, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let branch = if !self.trunk.branch.is_empty() {
            self.trunk.branch.clone()
        } else {
            self.repo
                .as_ref()
                .and_then(|repo| repo.default_branch.clone())
                .unwrap_or_default()
        };
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
            .label(SharedString::from(format!("\u{2387} {branch}")))
            .tooltip("Switch branch")
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu.label("Branches");
                for (name, current) in &picker {
                    menu = menu.menu_with_check(
                        SharedString::from(name.clone()),
                        *current,
                        Box::new(crate::actions::SwitchBranch {
                            branch: name.clone(),
                        }),
                    );
                }
                menu.separator().menu_with_icon(
                    "Open changes view",
                    Icon::from(ExpIcon::GitMerge),
                    Box::new(crate::actions::OpenSourceControl),
                )
            })
    }

    /// The middle segment: sync spinner + clone %, OR the amber `⚠ N conflicts`
    /// chip, OR the `↓behind ↑ahead` counts (v4 §4.3).
    fn render_status(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut row = h_flex().gap_2().items_center();

        // Repo-resolution problem takes over the whole segment.
        if let Some(error) = &self.repo_error {
            return row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(error.clone()),
            );
        }

        // Sync spinner + clone progress %.
        if self.syncing {
            let label: SharedString = match self.clone_progress {
                Some(percent) => {
                    let name = self
                        .repo
                        .as_ref()
                        .map(|repo| short_name(&repo.full_name))
                        .unwrap_or_default();
                    format!("Cloning {name}… {percent}%").into()
                }
                None => "Syncing…".into(),
            };
            return row
                .child(Spinner::new().xsmall().color(cx.theme().muted_foreground))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                );
        }

        // A clone/fetch/pull/push/checkout error (chip → error + Retry).
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

        // Behind / ahead counts — only the non-zero side(s) (v4 §4.3 mock
        // `↓2 ↑1`); a clean, in-sync trunk shows neither.
        if self.trunk.behind > 0 {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!("\u{2193}{}", self.trunk.behind))),
            );
        }
        if self.trunk.ahead > 0 {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!("\u{2191}{}", self.trunk.ahead))),
            );
        }
        row
    }

    /// The ghost Pull/Push buttons (v4 §4.3) — JetBrains-style ↙/↗ arrow
    /// icons: disabled with a tooltip while a clone/sync is in flight
    /// or the trunk clone does not exist yet.
    fn render_transport(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let clone_exists = self.repo.as_ref().is_some_and(|repo| repo.clone_exists);
        let disabled = self.syncing || !clone_exists;
        let blocked: Option<SharedString> = if self.syncing {
            Some("Syncing…".into())
        } else if !clone_exists {
            Some("Trunk not cloned yet".into())
        } else {
            None
        };
        let pull_tooltip: SharedString = blocked.clone().unwrap_or_else(|| "Pull".into());
        let push_tooltip: SharedString = blocked.unwrap_or_else(|| "Push".into());

        h_flex()
            .gap_1()
            .items_center()
            // JetBrains-style Commit entry: opens the Source Control screen
            // (commit box + changes) with branches in the sidebar.
            .child(
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
                    })),
            )
            .child(
                Button::new("git-pull")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::ArrowDownLeft))
                    .disabled(disabled)
                    .tooltip(pull_tooltip)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_sync(SyncMode::Pull, cx);
                    })),
            )
            .child(
                Button::new("git-push")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::ArrowUpRight))
                    .disabled(disabled)
                    .tooltip(push_tooltip)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_sync(SyncMode::Push, cx);
                    })),
            )
    }
}

impl Render for GitBar {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        // Trunk chrome only on a project scope (mirrors the Run section's
        // self-hide) — the sidebar's project panel collapses to nothing on
        // other screens.
        if self.project_id.is_none() {
            return div().into_any_element();
        }

        // Compact horizontal cluster for the board's top-right toolbar:
        // branch chip (click → Source Control), status, Pull/Push.
        h_flex()
            .gap_2()
            .items_center()
            .child(self.render_branch_chip(cx))
            .child(self.render_status(cx))
            .child(self.render_transport(cx))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// `owner/name` → `name` (the clone progress chip's short label).
fn short_name(full_name: &str) -> &str {
    full_name.rsplit('/').next().unwrap_or(full_name)
}

/// The background side of [`GitBar::start_sync`] (v4 §4.1): mint a JIT
/// installation token, run the git op (argv-only), and re-read the trunk from
/// disk — streaming [`SyncMsg`]s to the foreground drain. A conflict left by a
/// failed rebase is picked up by the trailing trunk read, never auto-aborted.
fn run_sync_worker(
    mode: SyncMode,
    trpc: &api::TrpcClient,
    repo: &RepoInfo,
    tx: &flume::Sender<SyncMsg>,
) {
    // Mint the ~55-min token (never persisted/logged; only ever rides in the
    // token remote URL, redacted in every error).
    let token = match api::repositories::installation_token(trpc, &repo.repository_id) {
        Ok(token) => token,
        Err(err) => {
            let detail = err.to_string();
            // Surface through whichever channel the segment reads.
            let _ = tx.send(if mode == SyncMode::Clone {
                SyncMsg::Clone(CloneEvent::Failed(detail.clone()))
            } else {
                SyncMsg::Failed(detail.clone())
            });
            let _ = tx.send(SyncMsg::Trunk(Err(detail)));
            return;
        }
    };
    let url = TokenUrl::new(token.full_name.clone(), token.token.clone());
    let clone: &Path = &repo.clone;

    let result = match mode {
        SyncMode::Clone => {
            let progress_tx = tx.clone();
            let mut on_event = move |event: CloneEvent| {
                let _ = progress_tx.send(SyncMsg::Clone(event));
            };
            clone_manager::ensure(&repo.repos_root, &repo.full_name, &url, &mut on_event)
                .map(|_| ())
        }
        SyncMode::Fetch => clone_manager::fetch(clone, &url),
        // L30: pull/push target the branch the token minting resolved
        // *live* from GitHub, not the cached scope value (which could be a stale
        // or omitted default). The token's `default_branch` is authoritative.
        SyncMode::Pull => clone_manager::pull(clone, &token.default_branch, &url),
        SyncMode::Push => clone_manager::push(clone, &token.default_branch, &url),
    };

    // A clone failure already streamed `CloneEvent::Failed` through the
    // callback; fetch/pull/push surface their detail here.
    if let Err(err) = &result {
        if mode != SyncMode::Clone {
            let _ = tx.send(SyncMsg::Failed(err.to_string()));
        }
    }

    // Always re-derive the trunk from disk (v4 §4.2 rule 3): a paused rebase
    // engages conflict mode even though the op returned an error.
    let trunk = trunk_state::read(clone).map_err(|err| err.to_string());
    let _ = tx.send(SyncMsg::Trunk(trunk));
    let _ = tx.send(SyncMsg::Branches(scm::branches(clone).unwrap_or_default()));
}
