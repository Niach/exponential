//! Source Control screen (masterplan v4 §4.4) — trunk-only: staged/unstaged
//! changes list with stage checkboxes, an exp-switch stash-restore strip, commit message + Commit /
//! Commit & Push, history pane, and (in conflict mode) the rebase/merge
//! banner with "Fix conflicts with Claude" / "Open terminal" / "Abort". The
//! working/commit diff renders through the shared `diff.rs` renderer.
//!
//! Trunk resolution (§4.2 rule 1: trunk-only, no project/issue scope): the
//! active workspace's clone. The workspace's first project (sidebar order)
//! resolves the backing repo via `repositories.list` (the v4 model —
//! `projects.repositoryId`); the clone lives at `<repos_root>/<owner>/<name>`.
//! All git state is derived from disk through [`coding::scm`] (§4.2 rule 3),
//! so it survives restarts and out-of-band fixes; every read/mutation runs on
//! the background executor (scm calls block on `git`).
//!
//! Commit & Push is the ONE transport path shared with the git bar: token
//! via [`coding::token_cache`], then [`coding::clone_manager::push`] against
//! the CHECKED-OUT branch read at spawn time.
//!
//! Conflict mode (§4.4): entry/exit is purely `scm::detect_conflict` off disk
//! (`.git/rebase-merge` / `MERGE_HEAD`), so the banner clears no matter who
//! finishes the rebase — Claude, a terminal, or another tool. **Fix conflicts
//! with Claude** opens a [`coding::claude_task`] in a `ClaudeTask` terminal tab
//! (§4.9); it is a *separate* primitive from `coding::launch` (no session row,
//! no worktree). All git invocations are argv-only through [`coding::scm`] —
//! never `gh`, never a library (DNR L5).

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, Entity, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    ActiveTheme as _, Disableable as _, Sizable as _,
};
use sync::Store;

use coding::clone_manager;
use coding::scm::{self, CommitInfo, ConflictKind, ConflictState, FileStatus, StatusSummary};
use terminal::TabKind;

use crate::coding_flow::{self, CodingHub};
use crate::diff::{build_scm_diff, DiffView};
use crate::navigation::{self, Navigation};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// History page size (§4.4: "200 at a time, Load more").
const HISTORY_PAGE: usize = 200;
/// Fixed width of the left changes/commit/history column.
const LEFT_COL_W: f32 = 360.;

/// Which commit path the identity prompt is gating (§4.4: the one-time inline
/// prompt runs *before* the pending commit, then continues it).
#[derive(Clone, Copy)]
enum CommitKind {
    Plain,
    Push,
}

/// What the right diff pane is showing.
#[derive(Clone)]
enum Selection {
    None,
    /// A working-tree file (`git diff [--cached]`); `staged` picks the side.
    Working { path: String, staged: bool },
    /// A history commit (`git show`).
    Commit { hash: String },
}

/// Scope-resolution / git-read lifecycle (mirror of the settings/run-bar load
/// gate — render-time kicks exactly one background job while `Idle`).
enum Load {
    Idle,
    Loading,
    Ready,
}

/// The resolved trunk clone (§4.2): the active workspace's backing repo on
/// disk, plus the ids the push path needs to mint a JIT installation token.
#[derive(Clone)]
struct TrunkScope {
    repository_id: String,
    /// The server-reported default branch (L30: server-healed, never fabricated
    /// as `main`). `None` when the API omitted it; used only as the labelling
    /// fallback for the conflict-fix task when no branch is checked out.
    default_branch: Option<String>,
    clone_dir: PathBuf,
}

/// The trunk Source Control center screen. Wired into
/// [`crate::navigation::Screen::SourceControl`].
pub struct SourceControlView {
    nav: Entity<Navigation>,
    /// The shared per-window rail state — carries the sidebar's "view this
    /// branch's history" selection (no checkout).
    rail: Entity<crate::sidebar::RailShared>,
    /// The branch whose history is shown (`None` = the checked-out branch,
    /// including the working-tree changes + commit box).
    viewing: Option<String>,
    /// The shared per-window repo resolver (§4.2) — the trunk repo comes from
    /// here instead of a per-screen `repositories.list` call.
    repo_resolver: Entity<RepoResolver>,
    /// Right pane — the shared side-by-side renderer (`set_prepared`, §4.4).
    diff: Entity<DiffView>,
    commit_input: Entity<InputState>,
    name_input: Entity<InputState>,
    email_input: Entity<InputState>,

    /// The active workspace this state belongs to (scope-change reset key).
    scope_workspace: Option<String>,
    scope_load: Load,
    scope: Option<TrunkScope>,

    status: Option<StatusSummary>,
    conflict: Option<ConflictState>,
    /// exp-switch stashes of the CURRENT branch (the D-dialog's escape hatch,
    /// surfaced as the Restore · Discard strip).
    stashes: Vec<scm::StashEntry>,
    history: Vec<CommitInfo>,
    history_skip: usize,
    history_has_more: bool,
    history_loading: bool,

    selection: Selection,
    /// Clone-local `user.name`/`user.email` present (any git config scope).
    identity_ok: bool,
    /// The one-time inline identity prompt is open, gating `pending_commit`.
    show_identity: bool,
    pending_commit: Option<CommitKind>,

    /// A commit/push/abort/stage op is in flight (buttons show it, disable).
    busy: Option<SharedString>,
    error: Option<SharedString>,
    /// Stale-read guards (a superseded refresh / diff load is dropped).
    generation: u64,
    diff_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl SourceControlView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let rail = crate::sidebar::rail_shared_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let diff = cx.new(|cx| DiffView::new(window, cx));
        let commit_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(3)
                .placeholder("Commit message…")
        });
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Your name"));
        let email_input = cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.workspaces, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            // Re-render when the shared repo resolution lands / changes.
            cx.observe(&repo_resolver, |_, _, cx| cx.notify()),
            // The sidebar's view-branch selection lives on the rail state.
            cx.observe(&rail, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            rail,
            viewing: None,
            repo_resolver,
            diff,
            commit_input,
            name_input,
            email_input,
            scope_workspace: None,
            scope_load: Load::Idle,
            scope: None,
            status: None,
            conflict: None,
            stashes: Vec::new(),
            history: Vec::new(),
            history_skip: 0,
            history_has_more: false,
            history_loading: false,
            selection: Selection::None,
            identity_ok: false,
            show_identity: false,
            pending_commit: None,
            busy: None,
            error: None,
            generation: 0,
            diff_generation: 0,
            _subscriptions: subscriptions,
        }
    }

    // -- scope resolution ---------------------------------------------------

    /// Render-time gate: reset on workspace change, then (once) resolve the
    /// active workspace's trunk clone off the foreground and kick the first
    /// git read. `Idle` while nothing is loading — the collection observers
    /// re-notify us when workspaces/projects sync in.
    fn ensure_scope(&mut self, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // workspace, shared by all five trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        let workspace_id = navigation::active_workspace_id(&self.nav, cx);
        if workspace_id.as_deref() != self.scope_workspace.as_deref() {
            self.scope_workspace = workspace_id.clone();
            self.scope = None;
            self.status = None;
            self.conflict = None;
            self.stashes.clear();
            self.history.clear();
            self.selection = Selection::None;
            self.error = None;
            self.scope_load = Load::Idle;
        }
        // Re-run while resolving (Idle/Loading) so the resolver's completion is
        // picked up; only `Ready` (scope set or confirmed absent) short-circuits.
        if matches!(self.scope_load, Load::Ready) {
            return;
        }
        let Some(workspace_id) = workspace_id else {
            return;
        };
        let collections = Store::global(cx).collections();
        if !collections.projects.read(cx).is_ready() {
            return; // wait for the projects shape (the observer re-fires)
        }
        let first_project = collections
            .projects_in_workspace(&workspace_id, cx)
            .first()
            .map(|project| project.id.clone());

        // Read the shared resolution rather than firing our own network call:
        // the first project's repo, else the sole workspace repo (v4 §4.2).
        match self
            .repo_resolver
            .read(cx)
            .lookup_workspace_trunk(first_project.as_deref())
        {
            RepoLookup::Loading => {
                // Still resolving — show the "Resolving repository…" state and
                // wait for the resolver observer to re-render us.
                self.scope_load = Load::Loading;
            }
            RepoLookup::Found(repo) => {
                let repos_root = CodingHub::global(cx).read(cx).settings.repos_root_path();
                let clone_dir = coding::clone_path(&repos_root, &repo.full_name);
                self.scope = Some(TrunkScope {
                    repository_id: repo.repository_id,
                    default_branch: repo.default_branch,
                    clone_dir,
                });
                self.scope_load = Load::Ready;
                cx.notify();
                self.refresh(cx);
            }
            RepoLookup::NotFound | RepoLookup::Error(_) => {
                // No repo connected to the workspace (or resolution failed) —
                // the screen shows the "connect one in settings" notice.
                self.scope = None;
                self.scope_load = Load::Ready;
                cx.notify();
            }
        }
    }

    /// Whether the resolved clone exists on disk yet (the auto-clone is the
    /// git bar's `CloneManager` job — until it lands, the reads would fail).
    fn clone_ready(&self) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|scope| scope.clone_dir.join(".git").exists())
    }

    // -- git reads ----------------------------------------------------------

    /// Re-read the whole trunk git state off disk (status + conflict +
    /// exp-switch stashes + first history page + identity), superseding any
    /// in-flight read. All git work runs on the background executor. (The
    /// branch-flow graph lives in the sidebar's [`crate::flow_view`] now.)
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        if !scope.clone_dir.join(".git").exists() {
            return; // not cloned yet — render the not-cloned notice
        }
        self.generation += 1;
        let generation = self.generation;
        let clone = scope.clone_dir.clone();
        // `Some(branch)` = the sidebar's view-without-checkout selection.
        let branch = self.viewing.clone();

        cx.spawn(async move |this, cx| {
            let (status, conflict, stashes, history, identity_ok) = cx
                .background_executor()
                .spawn(async move {
                    let status = scm::status(&clone);
                    let conflict = scm::detect_conflict(&clone);
                    // exp-switch stashes of the CURRENT branch only — user
                    // stashes and other branches' stashes stay invisible.
                    let current_branch = status
                        .as_ref()
                        .map(|summary| summary.branch.clone())
                        .unwrap_or_default();
                    let stashes: Vec<scm::StashEntry> = scm::stash_list(&clone)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|entry| {
                            scm::stash_switch_branch(&entry.message)
                                == Some(current_branch.as_str())
                        })
                        .collect();
                    let history =
                        scm::log_branch(&clone, branch.as_deref(), 0, HISTORY_PAGE)
                            .unwrap_or_default();
                    let identity_ok = identity_configured(&clone);
                    (status, conflict, stashes, history, identity_ok)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match status {
                    Ok(summary) => {
                        this.status = Some(summary);
                        this.error = None;
                    }
                    Err(err) => {
                        this.status = None;
                        this.error = Some(format!("git status failed: {err}").into());
                    }
                }
                this.conflict = conflict;
                this.stashes = stashes;
                this.history_skip = history.len();
                this.history_has_more = history.len() == HISTORY_PAGE;
                this.history = history;
                this.identity_ok = identity_ok;
                cx.notify();
            });
        })
        .detach();
    }

    /// History "Load more" (§4.4): append the next page.
    fn load_more_history(&mut self, cx: &mut gpui::Context<Self>) {
        if self.history_loading || !self.history_has_more {
            return;
        }
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        let skip = self.history_skip;
        let branch = self.viewing.clone();
        self.history_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let page = cx
                .background_executor()
                .spawn(async move {
                    scm::log_branch(&clone, branch.as_deref(), skip, HISTORY_PAGE)
                        .unwrap_or_default()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.history_loading = false;
                this.history_skip += page.len();
                this.history_has_more = page.len() == HISTORY_PAGE;
                this.history.extend(page);
                cx.notify();
            });
        })
        .detach();
    }

    // -- diff pane ----------------------------------------------------------

    fn select_working(&mut self, path: String, staged: bool, cx: &mut gpui::Context<Self>) {
        self.selection = Selection::Working {
            path: path.clone(),
            staged,
        };
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        self.diff_generation += 1;
        let generation = self.diff_generation;
        // Clone the highlight theme up front so the Tree-sitter row build runs
        // on the background executor alongside the git call — only the cheap
        // `set_prepared` swap touches the foreground (mirrors `DiffView::fetch`).
        let theme = cx.theme().highlight_theme.clone();
        self.diff.update(cx, |diff, cx| diff.set_loading(cx));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    scm::working_diff(&clone, &path, staged)
                        .map(|file| build_scm_diff(&[file], &theme))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.diff_generation != generation {
                    return;
                }
                this.diff.update(cx, |diff, cx| match result {
                    Ok(prepared) => diff.set_prepared(prepared, cx),
                    Err(err) => diff.set_error(err.to_string(), cx),
                });
            });
        })
        .detach();
    }

    fn select_commit(&mut self, hash: String, cx: &mut gpui::Context<Self>) {
        self.selection = Selection::Commit { hash: hash.clone() };
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        self.diff_generation += 1;
        let generation = self.diff_generation;
        // Build the diff rows on the background executor (see `select_working`).
        let theme = cx.theme().highlight_theme.clone();
        self.diff.update(cx, |diff, cx| diff.set_loading(cx));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    scm::commit_diff(&clone, &hash).map(|files| build_scm_diff(&files, &theme))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.diff_generation != generation {
                    return;
                }
                this.diff.update(cx, |diff, cx| match result {
                    Ok(prepared) => diff.set_prepared(prepared, cx),
                    Err(err) => diff.set_error(err.to_string(), cx),
                });
            });
        })
        .detach();
    }

    // -- stage / unstage ----------------------------------------------------

    fn toggle_stage(&mut self, path: String, want_staged: bool, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if want_staged {
                        scm::stage(&clone, &path)
                    } else {
                        scm::unstage(&clone, &path)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.error = Some(format!("{err}").into());
                }
                this.refresh(cx);
            });
        })
        .detach();
    }

    // -- exp-switch stashes ---------------------------------------------------

    /// Restore (`git stash pop`) an exp-switch stash. A pop that conflicts
    /// leaves the stash in place — git's own behavior; the error surfaces.
    fn restore_stash(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.run_stash_op(index, true, cx);
    }

    /// Discard (`git stash drop`) an exp-switch stash.
    fn drop_stash(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.run_stash_op(index, false, cx);
    }

    fn run_stash_op(&mut self, index: usize, restore: bool, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let clone = scope.clone_dir.clone();
        self.busy = Some(if restore { "Restoring stash…".into() } else { "Discarding stash…".into() });
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if restore {
                        scm::stash_pop(&clone, index)
                    } else {
                        scm::stash_drop(&clone, index)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(err) = result {
                    this.error = Some(format!("{err}").into());
                }
                this.refresh(cx);
            });
        })
        .detach();
    }

    // -- commit / push ------------------------------------------------------

    fn has_staged(&self) -> bool {
        self.status
            .as_ref()
            .map(|status| status.changes.iter().any(|change| change.staged))
            .unwrap_or(false)
    }

    /// Commit entry point (both buttons). Empty-staged / empty-message are
    /// no-ops; a missing identity opens the one-time inline prompt first
    /// (§4.4) with the pending commit stashed.
    fn on_commit(&mut self, push: bool, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy.is_some() || !self.has_staged() {
            return;
        }
        let message = self.commit_input.read(cx).value().trim().to_string();
        if message.is_empty() {
            self.error = Some("Enter a commit message.".into());
            cx.notify();
            return;
        }
        if !self.identity_ok {
            self.pending_commit = Some(if push {
                CommitKind::Push
            } else {
                CommitKind::Plain
            });
            self.show_identity = true;
            self.error = None;
            cx.notify();
            return;
        }
        self.do_commit(push, message, window, cx);
    }

    /// Run the commit (and push) off the foreground. Push is the shared
    /// transport path: token via [`coding::token_cache`], then
    /// [`clone_manager::push`] against the CHECKED-OUT branch read fresh
    /// from disk at op time — `self.status` can be stale after a branch
    /// switch outside this view, and pushing a snapshot branch would
    /// silently ship nothing (auto-rebase if behind; a conflict leaves
    /// markers and surfaces as conflict mode on the follow-up refresh).
    fn do_commit(
        &mut self,
        push: bool,
        message: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let trpc = if push { queries::trpc_client(cx) } else { None };
        let clone = scope.clone_dir.clone();
        let repository_id = scope.repository_id.clone();
        self.busy = Some(if push {
            "Committing and pushing…".into()
        } else {
            "Committing…".into()
        });
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    scm::commit(&clone, &message).map_err(|err| err.to_string())?;
                    if push {
                        // Fresh read — every transport op targets the branch
                        // the disk reports (git_bar's run_sync_worker rule).
                        let branch = coding::trunk_state::read(&clone)
                            .map(|state| state.branch)
                            .unwrap_or_default();
                        if branch.is_empty() || branch.starts_with('(') {
                            return Err("No branch checked out.".to_string());
                        }
                        let trpc = trpc.ok_or_else(|| "Not signed in.".to_string())?;
                        let minted = coding::token_cache()
                            .get_or_mint(&trpc, &repository_id)
                            .map_err(|err| err.to_string())?;
                        clone_manager::push(&clone, &branch, &minted.url)
                            .map_err(|err| err.to_string())?;
                    }
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = None;
                match result {
                    Ok(()) => {
                        this.commit_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        this.error = None;
                    }
                    Err(err) => this.error = Some(err.into()),
                }
                this.refresh(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Identity prompt confirm (§4.4): write `user.name`/`user.email` to the
    /// **clone-local** config, then continue the stashed commit.
    fn save_identity(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name_input.read(cx).value().trim().to_string();
        let email = self.email_input.read(cx).value().trim().to_string();
        if name.is_empty() || email.is_empty() {
            self.error = Some("Enter a name and email.".into());
            cx.notify();
            return;
        }
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        let kind = self.pending_commit.take();
        let message = self.commit_input.read(cx).value().trim().to_string();
        self.show_identity = false;
        self.busy = Some("Saving identity…".into());
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let write = cx
                .background_executor()
                .spawn(async move {
                    scm::config_set_local(&clone, "user.name", &name)
                        .map_err(|err| err.to_string())?;
                    scm::config_set_local(&clone, "user.email", &email)
                        .map_err(|err| err.to_string())?;
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = None;
                match write {
                    Ok(()) => {
                        this.identity_ok = true;
                        let push = matches!(kind, Some(CommitKind::Push));
                        this.do_commit(push, message, window, cx);
                    }
                    Err(err) => {
                        this.error = Some(err.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    // -- conflict mode ------------------------------------------------------

    /// **Fix conflicts with Claude** (§4.4/§4.9): a Claude task in the trunk,
    /// in a `ClaudeTask` terminal tab (no session row, no worktree).
    fn fix_with_claude(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let Some(conflict) = self.conflict.clone() else {
            return;
        };
        let branch = self
            .status
            .as_ref()
            .map(|status| status.branch.clone())
            .filter(|branch| !branch.is_empty())
            .or_else(|| scope.default_branch.clone())
            .unwrap_or_default();
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let prompt = coding::fix_conflicts_prompt(&branch, &conflict.files);
        let label = format!("Fix conflicts · {branch}");
        let task = coding::claude_task(&settings, &scope.clone_dir, &prompt, &label);
        let Some(manager) = coding_flow::window_terminal_manager(window, cx) else {
            self.error = Some("No terminal dock in this window.".into());
            cx.notify();
            return;
        };
        let result = manager.update(cx, |manager, cx| {
            manager.open_tab(TabKind::ClaudeTask, task.tab_title.clone(), &task.spawn, None, cx)
        });
        if let Err(err) = result {
            self.error = Some(format!("Could not start Claude: {err}").into());
            cx.notify();
        }
    }

    /// **Open terminal** (§4.4): a plain shell tab at the trunk clone.
    fn open_terminal(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let Some(manager) = coding_flow::window_terminal_manager(window, cx) else {
            self.error = Some("No terminal dock in this window.".into());
            cx.notify();
            return;
        };
        let result =
            manager.update(cx, |manager, cx| manager.open_shell(Some(scope.clone_dir.clone()), cx));
        if let Err(err) = result {
            self.error = Some(format!("Could not open terminal: {err}").into());
            cx.notify();
        }
    }

    /// **Abort** (§4.4): `git rebase --abort` / `git merge --abort`, then
    /// refresh (the banner clears off the disk state, not this call).
    fn abort_conflict(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let Some(conflict) = self.conflict.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        let kind = conflict.kind;
        self.busy = Some("Aborting…".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { scm::abort_conflict(&clone, kind) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(err) = result {
                    this.error = Some(format!("Abort failed: {err}").into());
                }
                this.refresh(cx);
            });
        })
        .detach();
    }

    // -- render -------------------------------------------------------------

    /// The exp-switch stash strip (above the changes list): makes the
    /// dirty-switch dialog's stash visible and undoable — Restore pops it,
    /// Discard drops it. Only stashes tagged for the CURRENT branch show.
    fn render_stash_strip(&self, cx: &mut gpui::Context<Self>) -> Option<impl IntoElement> {
        if self.stashes.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let mut strip = gpui_component::v_flex().flex_shrink_0().gap_1().p_2().border_b_1()
            .border_color(theme.border)
            .bg(theme.accent.opacity(0.2));
        for entry in &self.stashes {
            let index = entry.index;
            strip = strip.child(
                gpui_component::h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(theme.foreground)
                            .child("Stashed changes from a branch switch"),
                    )
                    .child(
                        Button::new(SharedString::from(format!("stash-restore-{index}")))
                            .ghost()
                            .xsmall()
                            .label("Restore")
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.restore_stash(index, cx);
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("stash-discard-{index}")))
                            .ghost()
                            .xsmall()
                            .label("Discard")
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.drop_stash(index, cx);
                            })),
                    ),
            );
        }
        Some(strip)
    }

    fn render_changes(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let changes = self.status.as_ref().map(|s| s.changes.as_slice()).unwrap_or(&[]);
        let staged: Vec<_> = changes.iter().filter(|c| c.staged).collect();
        let unstaged: Vec<_> = changes.iter().filter(|c| !c.staged).collect();

        div()
            .id("scm-changes")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                gpui_component::v_flex()
                    .p_2()
                    .gap_1()
                    .when(!staged.is_empty(), |this| {
                        this.child(self.group_header(format!("Staged ({})", staged.len()), cx))
                            .children(
                                staged
                                    .iter()
                                    .map(|change| self.change_row(change, true, cx)),
                            )
                    })
                    .when(!unstaged.is_empty(), |this| {
                        this.child(self.group_header(format!("Changes ({})", unstaged.len()), cx))
                            .children(
                                unstaged
                                    .iter()
                                    .map(|change| self.change_row(change, false, cx)),
                            )
                    })
                    .when(staged.is_empty() && unstaged.is_empty(), |this| {
                        this.child(
                            div()
                                .py_2()
                                .text_xs()
                                .text_color(muted)
                                .child("No changes — the working tree is clean."),
                        )
                    }),
            )
    }

    fn group_header(
        &self,
        label: impl Into<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .pt_1()
            .pb_0p5()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label.into())
    }

    fn change_row(
        &self,
        change: &scm::FileChange,
        staged: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (glyph, color) = status_glyph(change.status, cx);
        let path = change.path.clone();
        let selected = matches!(
            &self.selection,
            Selection::Working { path: p, staged: s } if *p == change.path && *s == staged
        );
        let this = cx.entity().downgrade();
        let checkbox_path = change.path.clone();
        let row_path = change.path.clone();

        gpui_component::h_flex()
            .id(SharedString::from(format!(
                "scm-row-{}-{}",
                if staged { "s" } else { "u" },
                change.path
            )))
            .w_full()
            .items_center()
            .gap_2()
            .px_1()
            .py_0p5()
            .rounded(theme.radius)
            .when(selected, |this| this.bg(theme.accent.opacity(0.6)))
            .hover(|this| this.bg(theme.accent.opacity(0.3)))
            .cursor_pointer()
            .child(
                Checkbox::new(SharedString::from(format!(
                    "scm-stage-{}-{}",
                    if staged { "s" } else { "u" },
                    change.path
                )))
                .checked(staged)
                .on_click(move |checked, _window, cx| {
                    let path = checkbox_path.clone();
                    let want_staged = *checked;
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| this.toggle_stage(path, want_staged, cx));
                    }
                }),
            )
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(color)
                    .child(glyph),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(path)),
            )
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.select_working(row_path.clone(), staged, cx);
            }))
    }

    fn render_commit_box(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let can_commit = self.has_staged() && self.busy.is_none();
        gpui_component::v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_2()
            .border_t_1()
            .border_b_1()
            .border_color(theme.border)
            .child(Input::new(&self.commit_input).small())
            .when(self.show_identity, |this| {
                this.child(
                    gpui_component::v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Set the author for commits in this repository:"),
                        )
                        .child(Input::new(&self.name_input).small())
                        .child(Input::new(&self.email_input).small())
                        .child(
                            Button::new("scm-identity-save")
                                .primary()
                                .small()
                                .label("Save & commit")
                                .disabled(self.busy.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_identity(window, cx);
                                })),
                        ),
                )
            })
            .child(
                gpui_component::h_flex()
                    .gap_2()
                    .child(
                        Button::new("scm-commit")
                            .small()
                            .label("Commit")
                            .disabled(!can_commit)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_commit(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("scm-commit-push")
                            .primary()
                            .small()
                            .label("Commit & Push")
                            .disabled(!can_commit)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_commit(true, window, cx);
                            })),
                    )
                    .when_some(self.busy.clone(), |this, busy| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(busy),
                        )
                    }),
            )
    }

    fn render_history(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        div()
            .id("scm-history")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                gpui_component::v_flex()
                    .p_2()
                    .gap_0p5()
                    .child(self.group_header("History", cx))
                    .children(
                        self.history
                            .iter()
                            .map(|commit| self.commit_row(commit, cx)),
                    )
                    .when(self.history.is_empty(), |this| {
                        this.child(
                            div()
                                .py_2()
                                .text_xs()
                                .text_color(muted)
                                .child("No commits yet."),
                        )
                    })
                    .when(self.history_has_more, |this| {
                        this.child(
                            Button::new("scm-history-more")
                                .ghost()
                                .xsmall()
                                .label(if self.history_loading {
                                    "Loading…"
                                } else {
                                    "Load more"
                                })
                                .disabled(self.history_loading)
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.load_more_history(cx);
                                })),
                        )
                    }),
            )
    }

    fn commit_row(&self, commit: &CommitInfo, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let selected = matches!(&self.selection, Selection::Commit { hash } if *hash == commit.hash);
        let hash = commit.hash.clone();
        let meta = format!("{} · {}", commit.author, commit.relative_time);
        gpui_component::v_flex()
            .id(SharedString::from(format!("scm-commit-{}", commit.hash)))
            .w_full()
            .gap_0p5()
            .px_1()
            .py_1()
            .rounded(theme.radius)
            .when(selected, |this| this.bg(theme.accent.opacity(0.6)))
            .hover(|this| this.bg(theme.accent.opacity(0.3)))
            .cursor_pointer()
            .child(
                div()
                    .text_xs()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(commit.subject.clone())),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(meta)),
            )
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.select_commit(hash.clone(), cx);
            }))
    }

    /// The §4.4 conflict banner (leads the screen while a rebase/merge is
    /// paused). Conflicted-file chips open their marker diff; the three
    /// actions are Fix-with-Claude / Open-terminal / Abort.
    fn render_conflict_banner(
        &self,
        conflict: &ConflictState,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (verb, abort_label) = match conflict.kind {
            ConflictKind::Rebase => ("Rebase paused", "Abort rebase"),
            ConflictKind::Merge => ("Merge paused", "Abort merge"),
        };
        let title = format!("{verb} — {} conflicted files", conflict.files.len());
        let files = conflict.files.clone();
        gpui_component::v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.warning.opacity(0.12))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.warning_foreground)
                    .child(SharedString::from(title)),
            )
            .child(
                gpui_component::h_flex().gap_2().flex_wrap().children(
                    files.into_iter().map(|file| {
                        let file_for_click = file.clone();
                        gpui_component::h_flex()
                            .id(SharedString::from(format!("scm-conflict-{file}")))
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_0p5()
                            .rounded(theme.radius)
                            .bg(theme.warning.opacity(0.2))
                            .cursor_pointer()
                            .hover(|this| this.bg(theme.warning.opacity(0.35)))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.warning_foreground)
                                    .child(SharedString::from(format!("⚠ {file}"))),
                            )
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.select_working(file_for_click.clone(), false, cx);
                            }))
                    }),
                ),
            )
            .child(
                gpui_component::h_flex()
                    .gap_2()
                    .child(
                        Button::new("scm-fix-claude")
                            .primary()
                            .small()
                            .label("Fix conflicts with Claude")
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.fix_with_claude(window, cx);
                            })),
                    )
                    .child(
                        Button::new("scm-open-terminal")
                            .small()
                            .label("Open terminal")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_terminal(window, cx);
                            })),
                    )
                    .child(
                        Button::new("scm-abort")
                            .danger()
                            .small()
                            .label(abort_label)
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.abort_conflict(cx);
                            })),
                    ),
            )
    }

    fn render_diff_pane(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        match &self.selection {
            Selection::None => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Select a file or commit to view its diff.")
                .into_any_element(),
            // `.h_full()` is load-bearing: without a definite height the
            // DiffView's virtual list resolves to zero height and renders
            // nothing (the issue Changes tab embeds it the same way).
            _ => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.diff.clone())
                .into_any_element(),
        }
    }

    fn render_body(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();

        // Scope not yet resolvable (workspaces/projects still syncing).
        if matches!(self.scope_load, Load::Loading) && self.scope.is_none() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Resolving repository…")
                .into_any_element();
        }
        if self.scope.is_none() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(
                    "No repository connected to this workspace — connect one in workspace settings.",
                )
                .into_any_element();
        }
        if !self.clone_ready() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Repository not cloned yet — open a project to clone it.")
                .into_any_element();
        }

        // Viewing another branch's history (sidebar selection, no checkout):
        // the working-tree changes + commit box are the CURRENT branch's and
        // would mislead — show the banner + that branch's history only.
        // The flow graph lives in the SIDEBAR tool window ([`crate::flow_view`])
        // — the left column here stays changes/commit/history at full height.
        let left = if let Some(branch) = self.viewing.clone() {
            gpui_component::v_flex()
                .w(px(LEFT_COL_W))
                .flex_shrink_0()
                .h_full()
                .border_r_1()
                .border_color(theme.border)
                .child(self.render_viewing_banner(&branch, cx))
                .child(self.render_history(cx))
        } else {
            gpui_component::v_flex()
                .w(px(LEFT_COL_W))
                .flex_shrink_0()
                .h_full()
                .border_r_1()
                .border_color(theme.border)
                .when_some(self.render_stash_strip(cx), |this, strip| this.child(strip))
                .child(self.render_changes(cx))
                .child(self.render_commit_box(cx))
                .child(self.render_history(cx))
        };

        gpui_component::h_flex()
            .flex_1()
            .min_h_0()
            .child(left)
            .child(self.render_diff_pane(cx))
            .into_any_element()
    }

    /// The "history of another branch" strip: what is shown + the way back.
    fn render_viewing_banner(
        &self,
        branch: &str,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        gpui_component::h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(format!("\u{2387} {branch}"))),
            )
            .child(
                Button::new("scm-view-current")
                    .ghost()
                    .xsmall()
                    .label("Back to current")
                    .on_click(|_, window, cx| {
                        crate::sidebar::set_view_branch(window, cx, None);
                    }),
            )
    }
}

impl Focusable for SourceControlView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.diff.focus_handle(cx)
    }
}

impl Render for SourceControlView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_scope(cx);
        // Follow the sidebar's view-branch selection: a change resets the
        // selection + reloads history for that branch (no checkout).
        let view = self.rail.read(cx).view_branch().map(str::to_string);
        if view != self.viewing {
            self.viewing = view;
            self.selection = Selection::None;
            self.history.clear();
            self.history_skip = 0;
            self.history_has_more = false;
            self.refresh(cx);
        }
        let theme = cx.theme();
        let conflict = self.conflict.clone();
        let error = self.error.clone();

        gpui_component::v_flex()
            .size_full()
            .bg(theme.background)
            .when_some(conflict, |this, conflict| {
                this.child(self.render_conflict_banner(&conflict, cx))
            })
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(self.render_body(cx))
    }
}

// ---------------------------------------------------------------------------
// Git identity (§4.4 one-time prompt) — reads/writes through the scm config
// wrappers; the decision of WHEN to prompt stays a UI concern.
// ---------------------------------------------------------------------------

/// True when both `user.name` and `user.email` resolve in ANY git config
/// scope for the clone (§4.4: identity comes from the user's global config;
/// the prompt only appears when unset).
fn identity_configured(clone: &std::path::Path) -> bool {
    !scm::config_get(clone, "user.name").is_empty()
        && !scm::config_get(clone, "user.email").is_empty()
}

/// Status glyph + color for a change row (M/A/D/R/? — §4.4 changes list).
fn status_glyph(status: FileStatus, cx: &App) -> (&'static str, gpui::Hsla) {
    let theme = cx.theme();
    match status {
        FileStatus::Modified => ("M", theme.yellow),
        FileStatus::Added => ("A", theme.green),
        FileStatus::Deleted => ("D", theme.red),
        FileStatus::Renamed => ("R", theme.blue),
        FileStatus::Untracked => ("?", theme.muted_foreground),
    }
}
