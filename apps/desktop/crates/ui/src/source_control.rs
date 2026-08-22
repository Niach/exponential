//! Source Control screen (masterplan v4 §4.4, EXP-253 master-only rework) —
//! trunk-only and READ-ONLY: the shared diff renderer full-width (EXP-258:
//! the per-file changes column is gone — the trunk is expected always clean,
//! so the pane shows commit diffs from the sidebar History, and a dirty tree
//! is an ANOMALY that only surfaces a slim count/discard strip), plus (in
//! conflict mode) the rebase/merge banner with "Open terminal" / "Abort" /
//! "Discard & reset" (EXP-259 deleted the claude-task "Fix conflicts with
//! Claude" button — PR merge conflicts are fixed by the builtin "Fix merge
//! conflicts" ACTION run from the Reviews list / actions surfaces; a trunk
//! rebase/merge conflict is a manual-recovery anomaly). The editor is
//! view-only — changes arrive via PRs, and the trunk is kept fresh by the
//! headless [`crate::trunk_sync`] engine. TWO write affordances, both behind
//! explicit confirms (EXP-509): discard local changes via
//! [`crate::trunk_sync::TrunkSync::hard_reset`] (reset to origin/<default>),
//! and commit-and-push them upstream via
//! [`crate::trunk_sync::TrunkSync::commit_push`] — the history list's
//! uncommitted-row icons carry both.
//!
//! Commit HISTORY lives in the sidebar tool column ([`HistoryList`], EXP-253
//! — it replaced the branch list): clicking a commit selects it on the
//! shared rail state and opens this screen, which shows the commit's diff.
//! EXP-509 gave the list a lane GRAPH (a [`crate::commit_graph`] gutter —
//! colored lanes, curved merge connectors, hollow dots for unpushed
//! commits) and a synthetic muted top row while the tree is dirty or local
//! commits sit unpushed: clicking it shows the working-tree diff, its icon
//! buttons push upstream / discard. Since EXP-518 the window is the
//! multi-ref [`coding::scm::log_graph`] walk (HEAD + remote-tracking refs),
//! so kept squash-merged PR branches render as side lanes off the trunk —
//! HEAD alone is single-parent all the way down and drew a straight line.
//!
//! Trunk resolution (§4.2 rule 1: trunk-only, no board/issue scope): the
//! active team's clone. The team's first board (sidebar order)
//! resolves the backing repo via `repositories.list` (the v4 model —
//! `boards.repositoryId`); the clone lives at `<repos_root>/<owner>/<name>`.
//! All git state is derived from disk through [`coding::scm`] (§4.2 rule 3),
//! so it survives restarts and out-of-band fixes; every read runs on the
//! background executor (scm calls block on `git`).
//!
//! Conflict mode (§4.4): entry/exit is purely `scm::detect_conflict` off disk
//! (`.git/rebase-merge` / `MERGE_HEAD`), so the banner clears no matter who
//! finishes the rebase — a terminal, another tool, or the reset hatch. All
//! git invocations are argv-only through [`coding::scm`] — never `gh`, never
//! a library (DNR L5).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, point, px, size, AnyElement, App, AppContext as _, Bounds, Entity, FocusHandle,
    Focusable, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    scroll::{ScrollableElement as _, ScrollbarAxis},
    v_virtual_list, ActiveTheme as _, Disableable as _, VirtualListScrollHandle,
};
use sync::Store;

use coding::scm::{self, CommitInfo, ConflictKind, ConflictState};

use crate::coding_flow::{self, CodingHub};
use crate::commit_graph::{self, EdgeKind, Graph, GraphRow, SquashLink, MAX_LANES};
use crate::controls::WebControl as _;
use crate::diff::{build_scm_diff, DiffView};
use crate::icons::registry;
use crate::navigation::{self, Navigation};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::sidebar::ScSelection;

/// History page size (§4.4: "200 at a time, Load more").
const HISTORY_PAGE: usize = 200;
/// Fixed sidebar history row height (EXP-344: the list is virtualized, so
/// every row pins this height — two `text_xs` lines + the row inset).
const HISTORY_ROW_HEIGHT: f32 = 48.;
/// The trailing "Load more" row's height.
const HISTORY_MORE_ROW_HEIGHT: f32 = 30.;

// EXP-509 graph gutter geometry — the row height doubles as the lane pitch's
// vertical rhythm, so only the horizontals live here.
/// Horizontal distance between lane centers.
const GRAPH_LANE_PITCH: f32 = 10.;
/// Inset from the row's left edge to the first lane center.
const GRAPH_LEFT_PAD: f32 = 14.;
/// Gap between the last lane center and the text block.
const GRAPH_RIGHT_PAD: f32 = 9.;
/// Commit dot radius.
const GRAPH_DOT_RADIUS: f32 = 3.5;
/// Lane line width.
const GRAPH_LINE_WIDTH: f32 = 2.;

/// What the diff pane is showing.
#[derive(Clone)]
enum Selection {
    /// No commit picked yet (placeholder).
    None,
    /// One conflicted file's marker diff (conflict-banner chip).
    ConflictFile,
    /// A history commit (`git show`) — the sidebar history list carries
    /// WHICH commit (rail [`ScSelection`]); this only picks the pane.
    Commit,
    /// The uncommitted working tree (`git diff HEAD` + untracked files) —
    /// the history list's synthetic top row (EXP-509).
    WorkingTree,
}

/// Scope-resolution / git-read lifecycle (render-time kicks exactly one
/// background job while `Idle`).
enum Load {
    Idle,
    Loading,
    Ready,
}

/// The resolved trunk clone (§4.2): the active team's backing repo on disk.
#[derive(Clone)]
struct TrunkScope {
    /// The server-reported default branch (L30: server-healed, never fabricated
    /// as `main`). `None` when the API omitted it; the hard-reset confirm's
    /// labelling fallback when no branch is checked out.
    default_branch: Option<String>,
    clone_dir: PathBuf,
}

/// The trunk Source Control center screen. Wired into
/// [`crate::navigation::Screen::SourceControl`].
pub struct SourceControlView {
    nav: Entity<Navigation>,
    /// The shared per-window rail state — carries the sidebar history list's
    /// "show this commit" selection + the trunk-sync engine.
    rail: Entity<crate::sidebar::RailShared>,
    /// The last trunk-sync `sync_seq` this view re-read for — the shared
    /// engine's counter is the freshness signal (EXP-67: an external commit
    /// pulled by auto-sync must show up without closing/reopening the
    /// screen).
    seen_sync_seq: u64,
    /// The sidebar history selection this view last applied.
    seen_selection: ScSelection,
    /// The shared per-window repo resolver (§4.2) — the trunk repo comes from
    /// here instead of a per-screen `repositories.list` call.
    repo_resolver: Entity<RepoResolver>,
    /// Right pane — the shared side-by-side renderer (`set_prepared`, §4.4).
    diff: Entity<DiffView>,

    /// The active board this state belongs to (scope-change reset key) —
    /// the SAME scope rule as [`crate::trunk_sync::TrunkSync`] and the
    /// sidebar [`HistoryList`], so the diff pane, the history pane, and
    /// the hard-reset target can never point at different repos in a
    /// multi-repo team.
    scope_board: Option<String>,
    scope_load: Load,
    scope: Option<TrunkScope>,

    status: Option<scm::StatusSummary>,
    conflict: Option<ConflictState>,

    selection: Selection,

    /// An abort/reset op is in flight (buttons show it, disable).
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

        let trunk_sync = rail.read(cx).trunk_sync().clone();
        let seen_sync_seq = trunk_sync.read(cx).sync_seq();
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // Re-render when the shared repo resolution lands / changes.
            cx.observe(&repo_resolver, |_, _, cx| cx.notify()),
            // The sidebar's commit selection lives on the rail state.
            cx.observe(&rail, |_, _, cx| cx.notify()),
            // Freshness (EXP-67): re-read status when a trunk-sync pass lands
            // fresh on-disk state (external commits pulled by auto-sync used
            // to stay invisible until the screen reopened).
            cx.observe(&trunk_sync, |this: &mut Self, engine, cx| {
                let seq = engine.read(cx).sync_seq();
                if seq != this.seen_sync_seq {
                    this.seen_sync_seq = seq;
                    this.refresh(cx);
                }
                cx.notify();
            }),
        ];

        Self {
            nav,
            rail,
            seen_sync_seq,
            seen_selection: ScSelection::None,
            repo_resolver,
            diff,
            scope_board: None,
            scope_load: Load::Idle,
            scope: None,
            status: None,
            conflict: None,
            selection: Selection::None,
            busy: None,
            error: None,
            generation: 0,
            diff_generation: 0,
            _subscriptions: subscriptions,
        }
    }

    // -- scope resolution ---------------------------------------------------

    /// Render-time gate: reset on team change, then (once) resolve the
    /// active team's trunk clone off the foreground and kick the first
    /// git read. `Idle` while nothing is loading — the collection observers
    /// re-notify us when teams/boards sync in.
    fn ensure_scope(&mut self, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // team, shared by all trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        let board_id = navigation::active_board_id(&self.nav, cx);
        if board_id.as_deref() != self.scope_board.as_deref() {
            self.scope_board = board_id.clone();
            self.scope = None;
            self.status = None;
            self.conflict = None;
            self.selection = Selection::None;
            self.error = None;
            self.scope_load = Load::Idle;
            // A scope change invalidates the sidebar's selection — clearing
            // it also lets the SAME hash re-fire later (the equality guards
            // would otherwise swallow the re-select).
            self.seen_selection = ScSelection::None;
            let rail = self.rail.clone();
            rail.update(cx, |rail, cx| rail.clear_sc_selection(cx));
        }
        // Re-run while resolving (Idle/Loading) so the resolver's completion is
        // picked up; only `Ready` (scope set or confirmed absent) short-circuits.
        if matches!(self.scope_load, Load::Ready) {
            return;
        }
        let Some(board_id) = board_id else {
            return;
        };

        // Read the shared resolution rather than firing our own network call:
        // the ACTIVE board's repo — the trunk-sync engine's exact scope.
        match self.repo_resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Loading => {
                // Still resolving — show the "Resolving repository…" state and
                // wait for the resolver observer to re-render us.
                self.scope_load = Load::Loading;
            }
            RepoLookup::Found(repo) => {
                let repos_root = CodingHub::global(cx).read(cx).settings.repos_root_path();
                let clone_dir = coding::clone_path(&repos_root, &repo.full_name);
                self.scope = Some(TrunkScope {
                    default_branch: repo.default_branch,
                    clone_dir,
                });
                self.scope_load = Load::Ready;
                cx.notify();
                self.refresh(cx);
            }
            RepoLookup::NotFound | RepoLookup::Error(_) => {
                // No repo connected to the team (or resolution failed) —
                // the screen shows the "connect one in settings" notice.
                self.scope = None;
                self.scope_load = Load::Ready;
                cx.notify();
            }
        }
    }

    /// Whether the resolved clone exists on disk yet (the auto-clone is the
    /// trunk-sync engine's job — until it lands, the reads would fail).
    fn clone_ready(&self) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|scope| scope.clone_dir.join(".git").exists())
    }

    // -- git reads ----------------------------------------------------------

    /// Re-read status + conflict off disk, superseding any in-flight read.
    /// All git work runs on the background executor. (History lives in the
    /// sidebar's [`HistoryList`] now.)
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

        cx.spawn(async move |this, cx| {
            let (status, conflict) = cx
                .background_executor()
                .spawn(async move {
                    let status = scm::status(&clone);
                    let conflict = scm::detect_conflict(&clone);
                    (status, conflict)
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
                // A resolved conflict invalidates a chip's marker diff.
                if this.conflict.is_none()
                    && matches!(this.selection, Selection::ConflictFile)
                {
                    this.selection = Selection::None;
                }
                // EXP-509: a fresh status re-cuts the working-tree pane —
                // and a now-clean tree falls back to the placeholder (the
                // rail clear flows to the history row highlight too).
                if matches!(this.selection, Selection::WorkingTree) {
                    let clean = this
                        .status
                        .as_ref()
                        .is_none_or(|status| status.changes.is_empty());
                    if clean {
                        this.selection = Selection::None;
                        this.seen_selection = ScSelection::None;
                        let rail = this.rail.clone();
                        rail.update(cx, |rail, cx| rail.clear_sc_selection(cx));
                    } else {
                        this.select_working_tree(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- diff pane ----------------------------------------------------------

    /// A conflict-banner chip: one conflicted file's marker diff.
    fn select_conflict_file(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        self.selection = Selection::ConflictFile;
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
                    scm::working_diff(&clone, &path, false)
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
        self.selection = Selection::Commit;
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        self.diff_generation += 1;
        let generation = self.diff_generation;
        // Build the diff rows on the background executor (see `select_conflict_file`).
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

    /// EXP-509: the uncommitted working tree — the whole `git diff HEAD`
    /// patch plus synthesized adds for untracked files, through the same
    /// shared renderer (per-file headers come free).
    fn select_working_tree(&mut self, cx: &mut gpui::Context<Self>) {
        self.selection = Selection::WorkingTree;
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let clone = scope.clone_dir.clone();
        self.diff_generation += 1;
        let generation = self.diff_generation;
        // Build the diff rows on the background executor (see `select_conflict_file`).
        let theme = cx.theme().highlight_theme.clone();
        self.diff.update(cx, |diff, cx| diff.set_loading(cx));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    scm::working_tree_diff(&clone).map(|files| build_scm_diff(&files, &theme))
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

    // -- conflict mode ------------------------------------------------------

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
        let shell_override = crate::coding_flow::terminal_shell_override(cx);
        let result = manager.update(cx, |manager, cx| {
            manager.open_shell(Some(scope.clone_dir.clone()), shell_override, cx)
        });
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

    /// The EXP-253 escape hatch, behind an explicit confirm: abort any
    /// rebase/merge, fetch, and `reset --hard origin/<default>` via the
    /// shared trunk-sync engine. Discards local TRACKED changes; untracked
    /// files survive.
    fn prompt_hard_reset(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let branch = self
            .scope
            .as_ref()
            .and_then(|scope| scope.default_branch.clone())
            .or_else(|| {
                self.status
                    .as_ref()
                    .map(|status| status.branch.clone())
                    .filter(|branch| !branch.is_empty())
            });
        let this = cx.entity().downgrade();
        prompt_hard_reset_confirm(window, cx, &self.rail, branch, move |cx| {
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.selection = Selection::None;
                    this.error = None;
                    cx.notify();
                });
            }
        });
    }

    // -- render -------------------------------------------------------------

    /// The §4.4 conflict banner (leads the screen while a rebase/merge is
    /// paused). Conflicted-file chips open their marker diff; the actions
    /// are Open-terminal / Abort / the reset hatch (EXP-259 removed the
    /// claude-task fix button).
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
        let title = format!("{verb}: {} conflicted files", conflict.files.len());
        let files = conflict.files.clone();
        gpui_component::v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .border_b_1()
            // EXP-282: faint glass row stroke instead of the heavy chrome
            // border (the warning tint already separates the banner).
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
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
                                this.select_conflict_file(file_for_click.clone(), cx);
                            }))
                    }),
                ),
            )
            .child(
                gpui_component::h_flex()
                    .gap_2()
                    .child(
                        Button::new("scm-open-terminal")
                            .web_sm()
                            .label("Open terminal")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_terminal(window, cx);
                            })),
                    )
                    .child(
                        Button::new("scm-abort")
                            .danger()
                            .web_sm()
                            .label(abort_label)
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.abort_conflict(cx);
                            })),
                    )
                    .child(
                        Button::new("scm-conflict-reset")
                            .web_sm()
                            .label("Discard & reset…")
                            .disabled(self.busy.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prompt_hard_reset(window, cx);
                            })),
                    ),
            )
    }

    fn render_diff_pane(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        match &self.selection {
            Selection::None => div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Select a commit from History to view its diff.")
                .into_any_element(),
            // The definite height (`flex_1` + `min_h_0` in the column) is
            // load-bearing: without it the DiffView's virtual list resolves
            // to zero height and renders nothing (the issue Changes tab
            // embeds it the same way).
            _ => div()
                .flex_1()
                .min_h_0()
                .child(self.diff.clone())
                .into_any_element(),
        }
    }

    /// EXP-366: the not-cloned state is where a FAILED auto-clone lands — it
    /// must say why (the sticky sync error; "git not found on PATH" used to
    /// color the rail dot red and say nothing anywhere), guide a missing-git
    /// install, and offer the retry that used to hide behind the sidebar
    /// refresh button.
    fn render_not_cloned(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let (syncing, progress, error) = {
            let engine = trunk_sync.read(cx);
            (
                engine.is_syncing(),
                engine.clone_progress(),
                engine.sync_error(),
            )
        };
        let git_missing = CodingHub::global_ref(cx)
            .and_then(|hub| hub.read(cx).doctor.report.clone())
            .is_some_and(|report| !report.git.ok);
        // Copied out (Hsla is Copy) so the theme borrow doesn't overlap the
        // listener borrows below.
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        let column = gpui_component::v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .p_4()
            .text_xs()
            .text_color(muted);
        if syncing {
            let label = match progress {
                Some(percent) => format!("Cloning repository… {percent}%"),
                None => "Syncing repository…".to_string(),
            };
            return column.child(SharedString::from(label)).into_any_element();
        }
        let failed = error.is_some();
        column
            .child(match error {
                Some(error) => div()
                    .max_w_full()
                    .text_color(danger)
                    .child(SharedString::from(format!("Repository clone failed: {error}")))
                    .into_any_element(),
                None => div()
                    .child("Repository not cloned yet.")
                    .into_any_element(),
            })
            .when(git_missing, |this| {
                this.child(
                    "Git is required to clone repositories. Install it, then run \
                     \u{201c}Check tools\u{201d} in Settings → Tools.",
                )
                .child(
                    Button::new("scm-install-git")
                        .web_xs()
                        .label("Install git")
                        .icon(registry::UI_EXTERNAL_LINK)
                        .on_click(cx.listener(|_, _, _, cx| {
                            crate::settings::open_url(
                                cx,
                                "https://git-scm.com/downloads".to_string(),
                            );
                        })),
                )
            })
            .child(
                Button::new("scm-retry-clone")
                    .web_xs()
                    .label(if failed { "Retry" } else { "Clone now" })
                    .on_click(cx.listener(|this, _, window, cx| {
                        let trunk_sync = this.rail.read(cx).trunk_sync().clone();
                        trunk_sync.update(cx, |engine, cx| engine.refresh(window, cx));
                    })),
            )
            .into_any_element()
    }

    fn render_body(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();

        // Scope not yet resolvable (teams/boards still syncing).
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
                    "No repository linked to this board. Link one in team settings.",
                )
                .into_any_element();
        }
        if !self.clone_ready() {
            return self.render_not_cloned(cx);
        }

        // A dirty tree OR local commits are ANOMALIES (view-only editor, PRs
        // merge cleanly, autopull only ever fast-forwards) — no changes UI,
        // just a slim strip naming the anomaly plus the discard escape hatch,
        // the ONE write affordance (a conflicted tree gets it in the banner
        // instead). EXP-346: a diverged-but-clean trunk used to render
        // NOTHING here while auto-sync silently skipped it forever.
        let anomaly = self
            .conflict
            .is_none()
            .then(|| self.status.as_ref().and_then(anomaly_strip_message))
            .flatten();
        // EXP-509: "View changes" only makes sense with an actual dirty tree
        // (an ahead-only anomaly has nothing uncommitted to show).
        let dirty = self
            .status
            .as_ref()
            .is_some_and(|status| !status.changes.is_empty());
        // Copied out (Hsla is Copy) so the theme borrow doesn't overlap the
        // mutable cx borrows of the render calls below.
        // EXP-277: faint glass row stroke for the content header line.
        let border = theme::tokens::glass::STROKE_ROW.to_hsla();
        let muted = theme.muted_foreground;

        gpui_component::v_flex()
            .flex_1()
            .min_h_0()
            .when_some(anomaly, |this, message| {
                this.child(
                    gpui_component::h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(message)),
                        )
                        .child(
                            gpui_component::h_flex()
                                .gap_1()
                                .when(dirty, |this| {
                                    this.child(
                                        // EXP-509: jump to the working-tree diff.
                                        Button::new("scm-view-changes")
                                            .web_xs()
                                            .label("View changes")
                                            .on_click(cx.listener(|_, _, window, cx| {
                                                crate::sidebar::set_sc_selection(
                                                    window,
                                                    cx,
                                                    ScSelection::WorkingTree,
                                                );
                                            })),
                                    )
                                })
                                .child(
                                    Button::new("scm-hard-reset")
                                        .web_xs()
                                        .label("Discard changes & reset…")
                                        .disabled(self.busy.is_some())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.prompt_hard_reset(window, cx);
                                        })),
                                ),
                        ),
                )
            })
            .child(self.render_diff_pane(cx))
            .into_any_element()
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
        // Follow the sidebar history list's selection.
        let want = self.rail.read(cx).sc_selection().clone();
        if want != self.seen_selection {
            self.seen_selection = want.clone();
            match want {
                ScSelection::Commit(hash) => self.select_commit(hash, cx),
                ScSelection::WorkingTree => self.select_working_tree(cx),
                // Deselected out from under us (scope change) — back to the
                // placeholder.
                ScSelection::None => {
                    if matches!(self.selection, Selection::Commit | Selection::WorkingTree) {
                        self.selection = Selection::None;
                    }
                }
            }
        }
        let conflict = self.conflict.clone();
        let error = self.error.clone();

        // EXP-282: no opaque fill — the screen floats on the page gradient
        // like every other center screen (the old `theme.background` paint
        // blocked the gradient behind the whole diff detail view).
        gpui_component::v_flex()
            .size_full()
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

/// The anomaly strip's message (EXP-346): local commits (a diverged or
/// ahead-only trunk) and/or working-tree changes — every one of these parks
/// the ff-only autopull, so the strip must SAY so instead of leaving the
/// trunk to go stale silently. `None` when the tree is the expected
/// clean/in-sync state (no strip). Conflicts are the banner's job — the
/// caller gates on `conflict.is_none()`.
fn anomaly_strip_message(status: &scm::StatusSummary) -> Option<String> {
    // A detached HEAD (`# branch.head (detached)`) has no branch to
    // fast-forward, so the autopull refuses it forever. It gets the strip to
    // itself — the counts below are meaningless off a branch, and the reset
    // hatch beside the message (force-checkout the default branch) is the way
    // back.
    if status.branch.starts_with('(') {
        return Some("Not on a branch. Auto-pull is paused.".to_string());
    }
    let mut parts: Vec<String> = Vec::new();
    if status.ahead > 0 && status.upstream.is_some() {
        let noun = if status.ahead == 1 { "local commit" } else { "local commits" };
        let lag = if status.behind > 0 {
            format!(" ({} behind origin)", status.behind)
        } else {
            String::new()
        };
        parts.push(format!("{} {noun} not on origin{lag}", status.ahead));
    }
    let changed = status.changes.len();
    if changed == 1 {
        parts.push("1 changed file in the working tree".to_string());
    } else if changed > 1 {
        parts.push(format!("{changed} changed files in the working tree"));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("{}. Auto-pull is paused.", parts.join(" · ")))
}

/// The shared discard confirm (the EXP-253 escape hatch; EXP-509 reuses it
/// from the history list's uncommitted row): explicit dialog, then
/// [`crate::trunk_sync::TrunkSync::hard_reset`] + rail-selection clear.
/// `branch` labels the reset target (fallback: the engine's checked-out
/// branch, then "the remote branch"); `after_ok` is the caller's own
/// post-kick cleanup.
fn prompt_hard_reset_confirm(
    window: &mut Window,
    cx: &mut App,
    rail: &Entity<crate::sidebar::RailShared>,
    branch: Option<String>,
    after_ok: impl Fn(&mut App) + 'static,
) {
    let trunk_sync = rail.read(cx).trunk_sync().clone();
    let branch = branch
        .or_else(|| {
            let checked_out = trunk_sync.read(cx).branch().to_string();
            (!checked_out.is_empty() && !checked_out.starts_with('(')).then_some(checked_out)
        })
        .unwrap_or_else(|| "the remote branch".to_string());
    // The hatch stays available while an Action / agent-shell tab is
    // working on this clone (it's the escape hatch), but the confirm
    // must say the tree is about to move under a live session.
    let session_live = trunk_sync.read(cx).repo_agents_alive(window, cx);
    let rail = rail.clone();
    let mut description = format!(
        "This resets the trunk to origin/{branch}, discarding all \
         local tracked changes and aborting any paused rebase or \
         merge. Untracked files are kept. This cannot be undone."
    );
    if session_live {
        description.push_str(
            " A coding or action session is currently running in \
             this clone. The reset will move the working tree \
             under it and may disrupt the session.",
        );
    }
    let spec = crate::native_dialog::AlertSpec::new(
        "Discard local changes?",
        description,
        "Discard changes & reset",
    )
    .height(px(280.))
    .on_ok(move |_, cx| {
        trunk_sync.update(cx, |engine, cx| engine.hard_reset(cx));
        rail.update(cx, |rail, cx| rail.clear_sc_selection(cx));
        after_ok(cx);
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

// ---------------------------------------------------------------------------
// HistoryList — the sidebar Source Control tool window (EXP-253: it replaced
// the branch list / flow graph)
// ---------------------------------------------------------------------------

/// The trunk's commit history in the sidebar tool column. Scope comes from
/// the shared [`crate::trunk_sync::TrunkSync`] engine (the active board's
/// clone — the same scope the old branch list followed); a fresh sync
/// (`sync_seq`) re-reads the first page. Clicking a commit selects it on the
/// shared rail state and opens the Source Control screen, which shows its
/// diff.
pub struct HistoryList {
    rail: Entity<crate::sidebar::RailShared>,
    scroll: VirtualListScrollHandle,
    /// The clone the loaded history belongs to (scope-change reset key).
    seen_clone: Option<PathBuf>,
    /// The last trunk-sync `sync_seq` this list re-read for.
    seen_sync_seq: u64,
    history: Vec<CommitInfo>,
    /// The loaded window's lane layout (EXP-509) — recomputed whole on every
    /// refresh/append (deterministic over a prefix, so appends never
    /// re-lane or recolor loaded rows).
    graph: Graph,
    /// Commits on HEAD not on origin (`git rev-list origin/<b>..HEAD`) —
    /// rendered hollow/muted (EXP-509).
    unpushed: HashSet<String>,
    /// HEAD's hash as of the last `refresh` — the layout's trunk-lane seed
    /// (EXP-518). Pinned per window: `load_more` reuses it rather than
    /// re-resolving, so a HEAD moved mid-window can't re-lane the visible
    /// prefix (the sync-triggered refresh reloads cleanly instead).
    history_tip: Option<String>,
    /// PR number → the merged PR branch's kept remote tip (EXP-537): synced
    /// merged-PR rows joined with `scm::remote_branch_tips`. Feeds
    /// [`squash_links`], which closes squash-merged lanes back into the
    /// trunk. Pinned per window like `history_tip` (`load_more` reuses it),
    /// so appends never re-lane the visible prefix.
    merged_pr_tips: HashMap<i64, String>,
    history_skip: usize,
    history_has_more: bool,
    history_loading: bool,
    /// The commit-and-push dialog's message field (EXP-509).
    commit_msg_input: Entity<InputState>,
    /// Stale-read guard (a superseded refresh is dropped).
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl HistoryList {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let rail = crate::sidebar::rail_shared_for_window(window, cx);
        let trunk_sync = rail.read(cx).trunk_sync().clone();
        let commit_msg_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Commit message"));
        let subscriptions = vec![
            // Selection highlight + scope both live on the rail state.
            cx.observe(&rail, |_, _, cx| cx.notify()),
            cx.observe(&trunk_sync, |_, _, cx| cx.notify()),
        ];
        Self {
            rail,
            scroll: VirtualListScrollHandle::new(),
            seen_clone: None,
            seen_sync_seq: 0,
            history: Vec::new(),
            graph: Graph::default(),
            unpushed: HashSet::new(),
            history_tip: None,
            merged_pr_tips: HashMap::new(),
            history_skip: 0,
            history_has_more: false,
            history_loading: false,
            commit_msg_input,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Render-time freshness gate: reset + reload on a clone change, reload
    /// the first page on a fresh sync.
    fn ensure_fresh(&mut self, cx: &mut gpui::Context<Self>) {
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let engine = trunk_sync.read(cx);
        let clone = engine.clone_dir();
        let seq = engine.sync_seq();
        if clone != self.seen_clone {
            self.seen_clone = clone.clone();
            self.seen_sync_seq = seq;
            self.history.clear();
            self.graph = Graph::default();
            self.unpushed.clear();
            self.history_tip = None;
            self.merged_pr_tips.clear();
            self.history_skip = 0;
            self.history_has_more = false;
            self.generation += 1;
            if clone.is_some() {
                self.refresh(cx);
            }
            return;
        }
        if seq != self.seen_sync_seq {
            self.seen_sync_seq = seq;
            self.refresh(cx);
        }
    }

    /// (Re)load the first history page for the current clone, plus the
    /// unpushed set when the engine reports local commits (EXP-509).
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(clone) = self.seen_clone.clone() else {
            return;
        };
        // Which branch to diff against origin — only when there IS an
        // upstream and something ahead (the common clean case skips the
        // extra rev-list entirely).
        let unpushed_branch = {
            let trunk_sync = self.rail.read(cx).trunk_sync().clone();
            let trunk = trunk_sync.read(cx).trunk();
            (trunk.ahead > 0
                && trunk.has_upstream
                && !trunk.branch.is_empty()
                && !trunk.branch.starts_with('('))
            .then(|| trunk.branch.clone())
        };
        // The store side of the squash links (EXP-537): pr number → branch
        // for every synced merged PR on this repo; the git side (branch →
        // remote tip hash) joins in on the background executor.
        let merged_branches = {
            let full_name = self.rail.read(cx).trunk_sync().read(cx).repo_full_name();
            merged_pr_branches(cx, full_name.as_deref())
        };
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let (page, unpushed, tip, merged_tips) = cx
                .background_executor()
                .spawn(async move {
                    // Resolve HEAD before the log so the seed can only lag the
                    // walk (a benign open tail lane), never lead it.
                    let tip = scm::head_hash(&clone).ok();
                    let page = scm::log_graph(&clone, 0, HISTORY_PAGE).unwrap_or_default();
                    let unpushed: HashSet<String> = unpushed_branch
                        .and_then(|branch| scm::unpushed_hashes(&clone, &branch).ok())
                        .unwrap_or_default()
                        .into_iter()
                        .collect();
                    let tips: HashMap<String, String> = scm::remote_branch_tips(&clone)
                        .unwrap_or_default()
                        .into_iter()
                        .collect();
                    let merged_tips: HashMap<i64, String> = merged_branches
                        .into_iter()
                        .filter_map(|(number, branch)| {
                            tips.get(&branch).map(|hash| (number, hash.clone()))
                        })
                        .collect();
                    (page, unpushed, tip, merged_tips)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.history_skip = page.len();
                this.history_has_more = page.len() == HISTORY_PAGE;
                this.history = page;
                this.history_tip = tip;
                this.merged_pr_tips = merged_tips;
                this.relayout();
                this.unpushed = unpushed;
                cx.notify();
            });
        })
        .detach();
    }

    /// Recompute the lane layout over the loaded window: squash links first
    /// (derived from the window + the pinned merged-PR tips), then the
    /// EXP-509 lane pass.
    fn relayout(&mut self) {
        let links = squash_links(&self.history, &self.merged_pr_tips);
        self.graph = commit_graph::layout(&self.history, self.history_tip.as_deref(), &links);
    }

    /// History "Load more" (§4.4): append the next page.
    fn load_more(&mut self, cx: &mut gpui::Context<Self>) {
        if self.history_loading || !self.history_has_more {
            return;
        }
        let Some(clone) = self.seen_clone.clone() else {
            return;
        };
        let skip = self.history_skip;
        let generation = self.generation;
        self.history_loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let page = cx
                .background_executor()
                .spawn(async move {
                    scm::log_graph(&clone, skip, HISTORY_PAGE).unwrap_or_default()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.history_loading = false;
                if this.generation != generation {
                    return;
                }
                this.history_skip += page.len();
                this.history_has_more = page.len() == HISTORY_PAGE;
                this.history.extend(page);
                // Whole-window recompute — deterministic over a prefix, so
                // the already-visible rows keep their lanes and colors (the
                // seed and the merged-PR tips reuse the values pinned at
                // refresh time for the same reason).
                this.relayout();
                cx.notify();
            });
        })
        .detach();
    }

    /// The uncommitted synthetic top row's presence (EXP-509): a dirty tree
    /// or unpushed local commits.
    fn uncommitted_visible(&self, cx: &App) -> bool {
        if self.seen_clone.is_none() {
            return false;
        }
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let trunk = trunk_sync.read(cx).trunk();
        trunk.dirty || (trunk.ahead > 0 && trunk.has_upstream)
    }

    /// The gutter cell's width — global over the loaded window, so every
    /// row's lanes line up (including "Load more" and the uncommitted row).
    fn gutter_width(&self) -> f32 {
        let lanes = self.graph.max_lane.min(MAX_LANES - 1) as f32 + 1.;
        GRAPH_LEFT_PAD + (lanes - 0.5) * GRAPH_LANE_PITCH + GRAPH_RIGHT_PAD
    }

    /// One virtual row: the synthetic uncommitted row first (when present),
    /// commits, then the trailing "Load more" row (while `history_has_more`).
    fn render_row(&self, ix: usize, cx: &mut gpui::Context<Self>) -> AnyElement {
        let uncommitted = self.uncommitted_visible(cx);
        if uncommitted && ix == 0 {
            return self.uncommitted_row(cx).into_any_element();
        }
        let ix = ix - usize::from(uncommitted);
        match self.history.get(ix) {
            Some(commit) => self.commit_row(ix, commit, cx).into_any_element(),
            None => self.load_more_row(cx).into_any_element(),
        }
    }

    fn commit_row(
        &self,
        ix: usize,
        commit: &CommitInfo,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let selected = self
            .rail
            .read(cx)
            .sc_selected_commit()
            .is_some_and(|hash| hash == commit.hash);
        let hash = commit.hash.clone();
        let meta = format!("{} · {}", commit.author, commit.relative_time);
        // EXP-509: not-yet-pushed commits render hollow + muted.
        let unpushed = self.unpushed.contains(&commit.hash);
        let row = self.graph.rows.get(ix).cloned().unwrap_or_default();
        let gutter = gutter_cell(self.gutter_width(), move |bounds, window| {
            paint_graph_row(bounds, window, &row, unpushed);
        });
        // EXP-282: flat edge-to-edge rows like the issue list — the pill
        // inset (rounded + horizontal container padding) and the pre-glass
        // `accent` fills are gone; hover/selected use the glass list tokens.
        // EXP-344: the row pins the virtual list's fixed height.
        // EXP-509: an h_flex now — graph gutter cell + the two-line text.
        gpui_component::h_flex()
            .id(SharedString::from(format!("hist-commit-{}", commit.hash)))
            .w_full()
            .h(px(HISTORY_ROW_HEIGHT))
            .overflow_hidden()
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .child(gutter)
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .justify_center()
                    .gap_0p5()
                    .pr_3()
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(if unpushed {
                                theme.muted_foreground
                            } else {
                                theme.foreground
                            })
                            .child(SharedString::from(commit.subject.clone())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(meta)),
                    ),
            )
            .on_click(cx.listener(move |_, _, window, cx| {
                crate::sidebar::set_sc_selection(
                    window,
                    cx,
                    ScSelection::Commit(hash.clone()),
                );
                // Opens/refocuses the Source Control screen, which follows
                // the selection.
                crate::sidebar::activate_tool(
                    window,
                    cx,
                    crate::sidebar::ToolWindow::SourceControl,
                );
            }))
    }

    /// The EXP-509 uncommitted-changes row: a muted synthetic entry above
    /// HEAD with a dashed stub in the gutter and icon-only actions — push
    /// upstream (commit first when dirty) and the discard hatch.
    fn uncommitted_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let engine = trunk_sync.read(cx);
        let trunk = engine.trunk();
        let selected = matches!(self.rail.read(cx).sc_selection(), ScSelection::WorkingTree);
        let dirty = trunk.dirty;
        // Push needs a branch with an upstream and no engaged conflict; the
        // worker re-derives all of it from disk before writing.
        let push_blocked = engine.is_syncing()
            || trunk.conflict.is_some()
            || trunk.branch.is_empty()
            || trunk.branch.starts_with('(')
            || !trunk.has_upstream;
        let busy = engine.is_syncing();
        let mut parts: Vec<String> = Vec::new();
        if trunk.dirty_files > 0 {
            let noun = if trunk.dirty_files == 1 { "file" } else { "files" };
            parts.push(format!("{} {noun}", trunk.dirty_files));
        }
        if trunk.ahead > 0 && trunk.has_upstream {
            parts.push(format!("{} ahead", trunk.ahead));
        }
        // EXP-516: a diverged trunk (ahead AND behind) is exactly the state
        // the push path now rebases through — say so at the affordance.
        if trunk.behind > 0 && trunk.has_upstream {
            parts.push(format!("{} behind", trunk.behind));
        }
        let meta = parts.join(" · ");
        // The trunk (HEAD) lane is 0 by construction in every case — seeded
        // (lane 0 reserved for the tip), unseeded tip-first (the fresh tip
        // allocates lane 0), or empty history. Row 0 may be a newer BRANCH
        // tip since EXP-518, so `rows.first()` would point the stub at the
        // wrong lane.
        let head_lane = 0;
        let muted = theme.muted_foreground;
        let gutter = gutter_cell(self.gutter_width(), move |bounds, window| {
            paint_uncommitted_stub(bounds, window, head_lane, muted);
        });

        gpui_component::h_flex()
            .id("hist-uncommitted")
            .w_full()
            .h(px(HISTORY_ROW_HEIGHT))
            .overflow_hidden()
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .child(gutter)
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .justify_center()
                    .gap_0p5()
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(theme.muted_foreground)
                            .child(if dirty {
                                "Uncommitted changes"
                            } else {
                                "Unpushed commits"
                            }),
                    )
                    .when(!meta.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(theme.muted_foreground.opacity(0.7))
                                .child(SharedString::from(meta)),
                        )
                    }),
            )
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_0p5()
                    .pr_2()
                    .child(
                        Button::new("hist-push")
                            .ghost()
                            .web_icon_xs()
                            .icon(registry::SC_PUSH)
                            .tooltip(if dirty {
                                "Commit & push local changes"
                            } else {
                                "Push local commits"
                            })
                            .disabled(push_blocked)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prompt_push(window, cx);
                            })),
                    )
                    .child(
                        Button::new("hist-discard")
                            .ghost()
                            .web_icon_xs()
                            .icon(registry::UI_DELETE)
                            .tooltip("Discard changes & reset…")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                prompt_hard_reset_confirm(
                                    window,
                                    cx,
                                    &this.rail.clone(),
                                    None,
                                    |_| {},
                                );
                            })),
                    ),
            )
            .on_click(cx.listener(|_, _, window, cx| {
                crate::sidebar::set_sc_selection(window, cx, ScSelection::WorkingTree);
                crate::sidebar::activate_tool(
                    window,
                    cx,
                    crate::sidebar::ToolWindow::SourceControl,
                );
            }))
    }

    /// The push confirm (EXP-509): with a dirty tree, a commit-message input
    /// rides the dialog (machine-rename pattern); clean-but-ahead is a plain
    /// confirm. When the trunk is ALSO behind origin the confirm says the
    /// push rebases through the divergence first (EXP-516) — the worker
    /// re-checks everything from disk either way.
    fn prompt_push(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let trunk = trunk_sync.read(cx).trunk().clone();
        let branch = if trunk.branch.is_empty() {
            "the branch".to_string()
        } else {
            trunk.branch.clone()
        };
        let behind_note = if trunk.behind > 0 && trunk.has_upstream {
            let noun = if trunk.behind == 1 { "commit" } else { "commits" };
            format!(
                " The trunk is also {} {noun} behind origin — local work is \
                 rebased onto origin/{branch} first.",
                trunk.behind
            )
        } else {
            String::new()
        };
        if trunk.dirty {
            self.commit_msg_input.update(cx, |state, cx| {
                state.set_value("Local changes", window, cx);
            });
            let content_input = self.commit_msg_input.clone();
            let ok_input = self.commit_msg_input.clone();
            let spec = crate::native_dialog::AlertSpec::new(
                "Push local changes?",
                format!(
                    "Commits ALL local changes on the trunk and pushes them \
                     straight to origin/{branch}.{behind_note}"
                ),
                "Commit & push",
            )
            .height(px(if behind_note.is_empty() { 260. } else { 300. }))
            .content(move |_, _| {
                div()
                    .mt_2()
                    .child(Input::new(&content_input).web_input_sm())
                    .into_any_element()
            })
            .on_ok(move |_, cx| {
                let message = ok_input.read(cx).value().trim().to_string();
                if message.is_empty() {
                    return false; // no unnamed commits — keep the dialog open
                }
                trunk_sync.update(cx, |engine, cx| engine.commit_push(Some(message), cx));
                true
            });
            crate::native_dialog::open_alert(window, cx, spec);
        } else {
            let noun = if trunk.ahead == 1 { "commit" } else { "commits" };
            let height = px(if behind_note.is_empty() { 220. } else { 260. });
            let spec = crate::native_dialog::AlertSpec::new(
                "Push local commits?",
                format!("Pushes {} local {noun} to origin/{branch}.{behind_note}", trunk.ahead),
                "Push",
            )
            .height(height)
            .on_ok(move |_, cx| {
                trunk_sync.update(cx, |engine, cx| engine.commit_push(None, cx));
                true
            });
            crate::native_dialog::open_alert(window, cx, spec);
        }
    }

    /// History "Load more" (§4.4) as the list's trailing virtual row — its
    /// gutter keeps painting the graph's open lanes so lines visually run
    /// off the bottom toward the not-yet-loaded commits.
    fn load_more_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tail = self.graph.tail.clone();
        let gutter = gutter_cell(self.gutter_width(), move |bounds, window| {
            paint_lane_tail(bounds, window, &tail);
        });
        gpui_component::h_flex()
            .h(px(HISTORY_MORE_ROW_HEIGHT))
            .items_center()
            .child(gutter)
            .child(
                Button::new("hist-more")
                    .ghost()
                    .web_xs()
                    .label(if self.history_loading {
                        "Loading…"
                    } else {
                        "Load more"
                    })
                    .disabled(self.history_loading)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.load_more(cx);
                    })),
            )
    }
}

// ---------------------------------------------------------------------------
// EXP-537 squash links — closing merged PR lanes back into the trunk
// ---------------------------------------------------------------------------

/// PR number → branch for every synced MERGED PR on `full_name` — the store
/// side of the squash-link join (the git side is `scm::remote_branch_tips`).
/// Matching rides the `pr_url` (`…/{owner}/{name}/pull/{n}`), so only PRs of
/// THIS repo qualify no matter how many teams/boards are synced.
fn merged_pr_branches(cx: &App, full_name: Option<&str>) -> Vec<(i64, String)> {
    let Some(full_name) = full_name else {
        return Vec::new();
    };
    let marker = format!("/{}/pull/", full_name.to_ascii_lowercase());
    let collections = Store::global(cx).collections().clone();
    let issues = collections.issues.read(cx);
    issues
        .iter()
        .filter(|issue| issue.pr_state.as_deref() == Some("merged"))
        .filter(|issue| {
            issue
                .pr_url
                .as_deref()
                .is_some_and(|url| url.to_ascii_lowercase().contains(&marker))
        })
        .filter_map(|issue| Some((issue.pr_number?, issue.branch.clone()?)))
        .collect()
}

/// The trailing GitHub squash-merge marker of a commit subject — `(#N)`,
/// exactly at the end.
fn squash_pr_number(subject: &str) -> Option<i64> {
    let (_, digits) = subject.trim_end().strip_suffix(')')?.rsplit_once("(#")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Derive the loaded window's [`SquashLink`]s: a single-parent commit whose
/// subject ends in `(#N)` is the squash of merged PR N — link it to the PR
/// branch's kept remote tip. A tip already emitted ABOVE its squash commit
/// (a reused branch with newer work) is skipped: the synthetic lane could
/// never resolve and would dangle open forever. Deterministic over a window
/// prefix, so `Load more` appends never re-lane visible rows.
fn squash_links(
    history: &[CommitInfo],
    merged_pr_tips: &HashMap<i64, String>,
) -> Vec<SquashLink> {
    let mut links = Vec::new();
    let mut linked: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for commit in history {
        seen.insert(commit.hash.as_str());
        if commit.parents.len() != 1 {
            continue; // a true merge commit already draws its branch line
        }
        let Some(tip) = squash_pr_number(&commit.subject)
            .and_then(|number| merged_pr_tips.get(&number))
        else {
            continue;
        };
        // `seen` also covers tip == commit hash; `linked` dedupes a tip
        // claimed twice (batch PRs collapse to one map entry already, but a
        // cherry-picked squash subject could re-claim it).
        if seen.contains(tip.as_str()) || !linked.insert(tip.as_str()) {
            continue;
        }
        links.push(SquashLink { commit: commit.hash.clone(), tip: tip.clone() });
    }
    links
}

// ---------------------------------------------------------------------------
// EXP-509 graph gutter painting (canvas + PathBuilder — the shell.rs notch is
// the precedent; PathBuilder, never the raw scene Path)
// ---------------------------------------------------------------------------

/// The lane color palette — fixed semantic accents, cycled by the layout's
/// color index (a branch keeps its color for its whole run). Lane 0 is
/// neutral (EXP-594: the indigo brand accent is retired; trunk reads as the
/// calm gray lane, the colored lanes mark diverging branches).
fn lane_color(ix: usize) -> Hsla {
    const PALETTE: [theme::Srgb8; 6] = [
        theme::tokens::NEUTRAL,
        theme::tokens::GREEN,
        theme::tokens::ORANGE,
        theme::tokens::BLUE,
        theme::tokens::YELLOW,
        theme::tokens::RED,
    ];
    PALETTE[ix % PALETTE.len()].to_hsla()
}

/// A fixed-width full-height gutter cell wrapping a paint-only canvas.
fn gutter_cell(
    width: f32,
    paint: impl Fn(Bounds<Pixels>, &mut Window) + 'static,
) -> impl IntoElement {
    div()
        .w(px(width))
        .h_full()
        .flex_shrink_0()
        .child(
            canvas(|_, _, _| (), move |bounds, _, window, _| paint(bounds, window))
                .size_full(),
        )
}

/// A lane's x center inside `bounds` (clamped to the render cap).
fn lane_x(bounds: &Bounds<Pixels>, lane: usize) -> Pixels {
    bounds.origin.x
        + px(GRAPH_LEFT_PAD + lane.min(MAX_LANES - 1) as f32 * GRAPH_LANE_PITCH)
}

/// A stroked lane path builder — dashed for synthetic (squash link) edges,
/// with the uncommitted stub's dash pattern: squash links are merges by
/// patch, not ancestry, so they never read as solid history.
fn lane_stroke(dashed: bool) -> gpui::PathBuilder {
    let builder = gpui::PathBuilder::stroke(px(GRAPH_LINE_WIDTH));
    if dashed {
        builder.dash_array(&[px(2.), px(3.)])
    } else {
        builder
    }
}

/// A vertical lane segment — a cheap quad, or a dashed path for synthetic
/// edges.
fn paint_lane_segment(
    window: &mut Window,
    x: Pixels,
    top: Pixels,
    bottom: Pixels,
    color: Hsla,
    dashed: bool,
) {
    if bottom <= top {
        return;
    }
    if dashed {
        let mut path = lane_stroke(true);
        path.move_to(point(x, top));
        path.line_to(point(x, bottom));
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
        return;
    }
    window.paint_quad(gpui::fill(
        Bounds::new(
            point(x - px(GRAPH_LINE_WIDTH / 2.), top),
            size(px(GRAPH_LINE_WIDTH), bottom - top),
        ),
        color,
    ));
}

/// One commit row's gutter: pass-throughs, curved connectors, and the dot
/// (hollow when the commit is not on origin yet). Synthetic (squash link)
/// edges draw dashed.
fn paint_graph_row(
    bounds: Bounds<Pixels>,
    window: &mut Window,
    row: &GraphRow,
    unpushed: bool,
) {
    let top = bounds.origin.y;
    let bottom = top + bounds.size.height;
    let mid = top + bounds.size.height / 2.;
    let dot_x = lane_x(&bounds, row.lane);
    let dot_gap = px(GRAPH_DOT_RADIUS + 1.5);

    for edge in &row.edges {
        let color = lane_color(edge.color);
        match edge.kind {
            EdgeKind::Pass => {
                let x = lane_x(&bounds, edge.lane_top);
                paint_lane_segment(window, x, top, bottom, color, edge.synthetic);
            }
            EdgeKind::IntoDot => {
                let from_x = lane_x(&bounds, edge.lane_top);
                if edge.lane_top == row.lane {
                    // Straight from the row top into the dot.
                    paint_lane_segment(window, from_x, top, mid - dot_gap, color, edge.synthetic);
                } else {
                    // Curved: down from the top edge, bending into the dot.
                    let mut path = lane_stroke(edge.synthetic);
                    path.move_to(point(from_x, top));
                    path.curve_to(point(dot_x, mid), point(from_x, mid));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
            }
            EdgeKind::OutOfDot => {
                let to_x = lane_x(&bounds, edge.lane_bottom);
                if edge.lane_bottom == row.lane {
                    // Straight from the dot to the row bottom.
                    paint_lane_segment(window, to_x, mid + dot_gap, bottom, color, edge.synthetic);
                } else {
                    // Curved: out of the dot toward the target lane, then
                    // down to the row bottom.
                    let mut path = lane_stroke(edge.synthetic);
                    path.move_to(point(dot_x, mid));
                    path.curve_to(point(to_x, bottom), point(to_x, mid));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
            }
        }
    }

    paint_dot(window, dot_x, mid, lane_color(row.color), !unpushed);
}

/// The "Load more" row's gutter: every still-open lane passes through
/// (dashed while a squash link's tip is still beyond the window).
fn paint_lane_tail(bounds: Bounds<Pixels>, window: &mut Window, tail: &[(usize, usize, bool)]) {
    let top = bounds.origin.y;
    let bottom = top + bounds.size.height;
    for &(lane, color, synthetic) in tail {
        paint_lane_segment(window, lane_x(&bounds, lane), top, bottom, lane_color(color), synthetic);
    }
}

/// The uncommitted row's gutter (EXP-509): a dashed hollow circle above
/// HEAD's lane with a dashed stub running down toward it — visibly "not a
/// commit yet".
fn paint_uncommitted_stub(
    bounds: Bounds<Pixels>,
    window: &mut Window,
    head_lane: usize,
    color: Hsla,
) {
    let mid = bounds.origin.y + bounds.size.height / 2.;
    let bottom = bounds.origin.y + bounds.size.height;
    let x = lane_x(&bounds, head_lane);
    let r = px(GRAPH_DOT_RADIUS);

    let mut stub = gpui::PathBuilder::stroke(px(GRAPH_LINE_WIDTH)).dash_array(&[px(2.), px(3.)]);
    stub.move_to(point(x, mid + r + px(1.5)));
    stub.line_to(point(x, bottom));
    if let Ok(path) = stub.build() {
        window.paint_path(path, color);
    }

    let mut ring = gpui::PathBuilder::stroke(px(1.5)).dash_array(&[px(2.), px(2.)]);
    ring.move_to(point(x - r, mid));
    ring.arc_to(point(r, r), px(0.), false, true, point(x + r, mid));
    ring.arc_to(point(r, r), px(0.), false, true, point(x - r, mid));
    if let Ok(path) = ring.build() {
        window.paint_path(path, color);
    }
}

/// A commit dot — filled, or a hollow ring for unpushed commits. A fully
/// rounded quad, not a lyon path: the GPU quad's SDF corners stay crisp at
/// this radius where the tessellated arc reads square-ish.
fn paint_dot(window: &mut Window, x: Pixels, y: Pixels, color: Hsla, filled: bool) {
    let r = px(GRAPH_DOT_RADIUS);
    let bounds = Bounds::new(point(x - r, y - r), size(r * 2., r * 2.));
    let quad = if filled {
        gpui::fill(bounds, color).corner_radii(r)
    } else {
        gpui::fill(bounds, gpui::transparent_black())
            .corner_radii(r)
            .border_widths(px(1.5))
            .border_color(color)
    };
    window.paint_quad(quad);
}

impl Render for HistoryList {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let no_clone = self.seen_clone.is_none();
        // EXP-509: the synthetic uncommitted row leads the list — even over
        // an empty history (a fresh repo with only local edits).
        let uncommitted = self.uncommitted_visible(cx);

        // Empty states need no scroller at all.
        if no_clone || (self.history.is_empty() && !uncommitted) {
            let muted = cx.theme().muted_foreground;
            return div()
                .flex_1()
                .min_h_0()
                .py_1()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child(if no_clone {
                            "No repository resolved yet."
                        } else {
                            "No commits yet."
                        }),
                )
                .into_any_element();
        }

        // EXP-344: the history is a virtualized fixed-height-row list (like
        // the issue list) — the old plain scroll pane re-laid-out every
        // loaded commit row each frame, which got visibly laggy at 200+
        // rows. The optional "Load more" row rides along as the last index;
        // the optional uncommitted row as the first (EXP-509).
        let mut sizes = Vec::with_capacity(self.history.len() + 2);
        if uncommitted {
            sizes.push(size(px(0.), px(HISTORY_ROW_HEIGHT)));
        }
        sizes.extend(std::iter::repeat_n(
            size(px(0.), px(HISTORY_ROW_HEIGHT)),
            self.history.len(),
        ));
        if self.history_has_more {
            sizes.push(size(px(0.), px(HISTORY_MORE_ROW_HEIGHT)));
        }
        let sizes: Rc<Vec<_>> = Rc::new(sizes);

        div()
            .flex_1()
            .min_h_0()
            .child(
                gpui_component::v_flex()
                    .id("hist-scroll")
                    .relative()
                    .size_full()
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "hist-commits",
                            sizes,
                            |this, visible_range, _window, cx| {
                                visible_range
                                    .map(|ix| this.render_row(ix, cx))
                                    .collect()
                            },
                        )
                        // EXP-282: no horizontal padding — the commit rows
                        // run full-width (they carry their own `px_3`).
                        .py_1()
                        .track_scroll(&self.scroll),
                    )
                    .scrollbar(&self.scroll, ScrollbarAxis::Vertical),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::scm::{FileChange, FileStatus, StatusSummary};

    fn summary(ahead: u32, behind: u32, changed: usize, upstream: bool) -> StatusSummary {
        StatusSummary {
            branch: "master".to_string(),
            upstream: upstream.then(|| "origin/master".to_string()),
            ahead,
            behind,
            changes: (0..changed)
                .map(|ix| FileChange {
                    path: format!("src/file{ix}.rs"),
                    status: FileStatus::Modified,
                    staged: false,
                })
                .collect(),
        }
    }

    fn commit_info(hash: &str, parents: &[&str], subject: &str) -> CommitInfo {
        CommitInfo {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            subject: subject.to_string(),
            author: "t".to_string(),
            relative_time: "now".to_string(),
        }
    }

    #[test]
    fn squash_pr_number_matches_only_the_trailing_marker() {
        assert_eq!(squash_pr_number("EXP-534: android markdown refinements (#451)"), Some(451));
        assert_eq!(squash_pr_number("fix (#7)  "), Some(7)); // trailing whitespace
        assert_eq!(squash_pr_number("no marker"), None);
        assert_eq!(squash_pr_number("mid (#12) mention"), None); // not trailing
        assert_eq!(squash_pr_number("Revert \"fix (#12)\""), None); // quoted
        assert_eq!(squash_pr_number("weird (#)"), None);
        assert_eq!(squash_pr_number("weird (#1a)"), None);
    }

    #[test]
    fn squash_links_join_squash_commits_to_their_branch_tips() {
        // s squashed PR 9 (branch tip t); the (#N) of an unmerged/unknown PR
        // and a true merge commit yield nothing.
        let history = [
            commit_info("s", &["b"], "EXP-1: feature (#9)"),
            commit_info("m", &["b", "x"], "old merge (#3)"),
            commit_info("u", &["b"], "unknown (#4)"),
            commit_info("t", &["a"], "feature work"),
        ];
        let tips = HashMap::from([(9, "t".to_string()), (3, "x".to_string())]);
        assert_eq!(
            squash_links(&history, &tips),
            vec![SquashLink { commit: "s".to_string(), tip: "t".to_string() }]
        );
    }

    #[test]
    fn squash_links_skip_a_tip_that_sits_above_its_squash_commit() {
        // The branch grew new commits after the merge: its tip is NEWER than
        // the squash commit, so a synthetic lane could never resolve — skip.
        let history = [
            commit_info("t2", &["t"], "more work on the reused branch"),
            commit_info("s", &["b"], "EXP-1: feature (#9)"),
        ];
        let tips = HashMap::from([(9, "t2".to_string())]);
        assert_eq!(squash_links(&history, &tips), Vec::new());
    }

    #[test]
    fn anomaly_strip_is_silent_for_the_expected_states() {
        // Clean + in sync, clean + behind-only (autopull's normal food), and
        // ahead-without-upstream (no origin to diverge from) → no strip.
        assert_eq!(anomaly_strip_message(&summary(0, 0, 0, true)), None);
        assert_eq!(anomaly_strip_message(&summary(0, 48, 0, true)), None);
        assert_eq!(anomaly_strip_message(&summary(3, 0, 0, false)), None);
    }

    #[test]
    fn anomaly_strip_names_local_commits_and_dirt() {
        // The Linux EXP-346 screenshot: 1 local commit, 48 behind, clean.
        assert_eq!(
            anomaly_strip_message(&summary(1, 48, 0, true)).as_deref(),
            Some("1 local commit not on origin (48 behind origin). Auto-pull is paused.")
        );
        // Ahead-only (nothing behind yet) drops the lag suffix, plural noun.
        assert_eq!(
            anomaly_strip_message(&summary(2, 0, 0, true)).as_deref(),
            Some("2 local commits not on origin. Auto-pull is paused.")
        );
        // Dirty-only keeps the pre-EXP-346 count wording.
        assert_eq!(
            anomaly_strip_message(&summary(0, 0, 1, true)).as_deref(),
            Some("1 changed file in the working tree. Auto-pull is paused.")
        );
        // Both compose into one strip.
        assert_eq!(
            anomaly_strip_message(&summary(1, 2, 3, true)).as_deref(),
            Some(
                "1 local commit not on origin (2 behind origin) · \
                 3 changed files in the working tree. Auto-pull is paused."
            )
        );
    }

    #[test]
    fn anomaly_strip_names_a_detached_head() {
        // `ff_eligible` refuses a detached HEAD forever — the strip says so
        // instead of leaving a clean-looking trunk to park stale, and it wins
        // over the counts (which mean nothing off a branch).
        let detached = StatusSummary { branch: "(detached)".to_string(), ..summary(0, 7, 2, true) };
        assert_eq!(
            anomaly_strip_message(&detached).as_deref(),
            Some("Not on a branch. Auto-pull is paused.")
        );
    }
}
