//! Source Control screen (masterplan v4 §4.4, EXP-253 master-only rework) —
//! trunk-only and READ-ONLY: a changes list + the shared diff renderer, plus
//! (in conflict mode) the rebase/merge banner with "Fix conflicts with
//! Claude" / "Open terminal" / "Abort". The IDE neither stages, commits nor
//! pushes anymore — the editor is view-only, changes arrive via PRs, and the
//! trunk is kept fresh by the headless [`crate::trunk_sync`] engine. The ONE
//! write affordance is the escape hatch: discard local changes via
//! [`crate::trunk_sync::TrunkSync::hard_reset`] (reset to origin/<default>),
//! behind an explicit confirm.
//!
//! Commit HISTORY lives in the sidebar tool column ([`HistoryList`], EXP-253
//! — it replaced the branch list): clicking a commit selects it on the
//! shared rail state and opens this screen, which shows the commit's diff.
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
//! finishes the rebase — Claude, a terminal, or another tool. **Fix conflicts
//! with Claude** opens a [`coding::claude_task`] in a `ClaudeTask` terminal tab
//! (§4.9); it is a *separate* primitive from `coding::launch` (no session row,
//! no worktree). All git invocations are argv-only through [`coding::scm`] —
//! never `gh`, never a library (DNR L5).

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, Entity, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use coding::scm::{self, CommitInfo, ConflictKind, ConflictState, FileStatus};
use terminal::TabKind;

use crate::coding_flow::{self, CodingHub};
use crate::diff::{build_scm_diff, DiffView};
use crate::navigation::{self, Navigation};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::scroll_pane::v_scroll_pane;

/// History page size (§4.4: "200 at a time, Load more").
const HISTORY_PAGE: usize = 200;
/// Fixed width of the left changes column.
const LEFT_COL_W: f32 = 360.;

/// What the right diff pane is showing.
#[derive(Clone)]
enum Selection {
    None,
    /// A working-tree file (`git diff [--cached]`); `staged` picks the side.
    Working { path: String, staged: bool },
    /// A history commit (`git show`) — the sidebar history list carries
    /// WHICH commit (rail `sc_selected_commit`); this only picks the pane.
    Commit,
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
    /// as `main`). `None` when the API omitted it; the labelling fallback for
    /// the conflict-fix task when no branch is checked out.
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
    /// The sidebar history selection this view last applied (`None` = none).
    seen_commit: Option<String>,
    changes_scroll: ScrollHandle,
    /// The shared per-window repo resolver (§4.2) — the trunk repo comes from
    /// here instead of a per-screen `repositories.list` call.
    repo_resolver: Entity<RepoResolver>,
    /// Right pane — the shared side-by-side renderer (`set_prepared`, §4.4).
    diff: Entity<DiffView>,

    /// The active board this state belongs to (scope-change reset key) —
    /// the SAME scope rule as [`crate::trunk_sync::TrunkSync`] and the
    /// sidebar [`HistoryList`], so the changes list, the history pane, and
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
            seen_commit: None,
            changes_scroll: ScrollHandle::new(),
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
            // A scope change invalidates the sidebar's commit selection —
            // clearing it also lets the SAME hash re-fire later (the
            // equality guards would otherwise swallow the re-select).
            self.seen_commit = None;
            let rail = self.rail.clone();
            rail.update(cx, |rail, cx| rail.clear_sc_selected_commit(cx));
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
        self.selection = Selection::Commit;
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
        // A pre-EXP-98 "Create with Claude" crash can leave a stale
        // `.mcp.json` in the clone root, re-raising claude's project-approval
        // dialog.
        coding::remove_stale_legacy_mcp_json(&scope.clone_dir);
        let task = coding::claude_task(&settings, &scope.clone_dir, &prompt, &label);
        let Some(manager) = coding_flow::window_terminal_manager(window, cx) else {
            self.error = Some("No terminal dock in this window.".into());
            cx.notify();
            return;
        };
        let result = manager.update(cx, |manager, cx| {
            manager.open_tab(TabKind::ClaudeTask, task.tab_title.clone(), None, &task.spawn, None, cx)
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
            })
            .unwrap_or_else(|| "the remote branch".to_string());
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        let this = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let trunk_sync = trunk_sync.clone();
            let this = this.clone();
            alert
                .confirm()
                .overlay_closable(true)
                .close_button(true)
                .width(px(416.))
                .title("Discard local changes?")
                .description(SharedString::from(format!(
                    "This resets the trunk to origin/{branch}, discarding all \
                     local tracked changes and aborting any paused rebase or \
                     merge. Untracked files are kept. This cannot be undone."
                )))
                .button_props(
                    DialogButtonProps::default().ok_text("Discard changes & reset"),
                )
                .on_ok(move |_, _, cx| {
                    trunk_sync.update(cx, |engine, cx| engine.hard_reset(cx));
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.selection = Selection::None;
                            this.error = None;
                            cx.notify();
                        });
                    }
                    true
                })
        });
    }

    // -- render -------------------------------------------------------------

    fn render_changes(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let changes = self.status.as_ref().map(|s| s.changes.as_slice()).unwrap_or(&[]);

        v_scroll_pane(
            "scm-changes",
            &self.changes_scroll,
            gpui_component::v_flex()
                .p_2()
                .gap_1()
                .when(!changes.is_empty(), |this| {
                    this.child(self.group_header(format!("Changes ({})", changes.len()), cx))
                        .children(
                            changes
                                .iter()
                                .map(|change| self.change_row(change, cx)),
                        )
                })
                .when(changes.is_empty(), |this| {
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

    /// One read-only changed-file row (EXP-253: no stage checkbox — the
    /// staged flag only picks which diff side to show).
    fn change_row(
        &self,
        change: &scm::FileChange,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (glyph, color) = status_glyph(change.status, cx);
        let staged = change.staged;
        let path = change.path.clone();
        let selected = matches!(
            &self.selection,
            Selection::Working { path: p, staged: s } if *p == change.path && *s == staged
        );
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
            .on_click(cx.listener(move |this, _, window, cx| {
                // A manual file pick supersedes the sidebar's commit
                // selection (and clears it so re-picking the same commit
                // re-fires).
                crate::sidebar::select_sc_commit(window, cx, None);
                this.seen_commit = None;
                this.select_working(row_path.clone(), staged, cx);
            }))
    }

    /// The §4.4 conflict banner (leads the screen while a rebase/merge is
    /// paused). Conflicted-file chips open their marker diff; the actions
    /// are Fix-with-Claude / Open-terminal / Abort / the reset hatch.
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
                    )
                    .child(
                        Button::new("scm-conflict-reset")
                            .small()
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
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Select a file or a commit from History to view its diff.")
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
                    "No repository linked to this board — link one in team settings.",
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
                .child("Repository not cloned yet — open a board to clone it.")
                .into_any_element();
        }

        let dirty = self
            .status
            .as_ref()
            .map(|status| !status.changes.is_empty())
            .unwrap_or(false);
        // Copied out (Hsla is Copy) so the theme borrow doesn't overlap the
        // mutable cx borrows of the render calls below.
        let border = theme.border;
        let left = gpui_component::v_flex()
            .w(px(LEFT_COL_W))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(border)
            .child(self.render_changes(cx))
            // The escape hatch for a dirty tree (a conflicted tree gets it
            // in the banner instead): the ONE write affordance left.
            .when(dirty && self.conflict.is_none(), |this| {
                this.child(
                    gpui_component::h_flex()
                        .flex_shrink_0()
                        .p_2()
                        .border_t_1()
                        .border_color(border)
                        .child(
                            Button::new("scm-hard-reset")
                                .small()
                                .label("Discard changes & reset…")
                                .disabled(self.busy.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_hard_reset(window, cx);
                                })),
                        ),
                )
            });

        gpui_component::h_flex()
            .flex_1()
            .min_h_0()
            .child(left)
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
        // Follow the sidebar history list's commit selection.
        let want = self
            .rail
            .read(cx)
            .sc_selected_commit()
            .map(str::to_string);
        if want != self.seen_commit {
            self.seen_commit = want.clone();
            if let Some(hash) = want {
                self.select_commit(hash, cx);
            }
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
    scroll: ScrollHandle,
    /// The clone the loaded history belongs to (scope-change reset key).
    seen_clone: Option<PathBuf>,
    /// The last trunk-sync `sync_seq` this list re-read for.
    seen_sync_seq: u64,
    history: Vec<CommitInfo>,
    history_skip: usize,
    history_has_more: bool,
    history_loading: bool,
    /// Stale-read guard (a superseded refresh is dropped).
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl HistoryList {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let rail = crate::sidebar::rail_shared_for_window(window, cx);
        let trunk_sync = rail.read(cx).trunk_sync().clone();
        let subscriptions = vec![
            // Selection highlight + scope both live on the rail state.
            cx.observe(&rail, |_, _, cx| cx.notify()),
            cx.observe(&trunk_sync, |_, _, cx| cx.notify()),
        ];
        Self {
            rail,
            scroll: ScrollHandle::new(),
            seen_clone: None,
            seen_sync_seq: 0,
            history: Vec::new(),
            history_skip: 0,
            history_has_more: false,
            history_loading: false,
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

    /// (Re)load the first history page for the current clone.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(clone) = self.seen_clone.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let page = cx
                .background_executor()
                .spawn(async move {
                    scm::log_branch(&clone, None, 0, HISTORY_PAGE).unwrap_or_default()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.history_skip = page.len();
                this.history_has_more = page.len() == HISTORY_PAGE;
                this.history = page;
                cx.notify();
            });
        })
        .detach();
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
                    scm::log_branch(&clone, None, skip, HISTORY_PAGE).unwrap_or_default()
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
                cx.notify();
            });
        })
        .detach();
    }

    fn commit_row(&self, commit: &CommitInfo, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let selected = self
            .rail
            .read(cx)
            .sc_selected_commit()
            .is_some_and(|hash| hash == commit.hash);
        let hash = commit.hash.clone();
        let meta = format!("{} · {}", commit.author, commit.relative_time);
        gpui_component::v_flex()
            .id(SharedString::from(format!("hist-commit-{}", commit.hash)))
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
            .on_click(cx.listener(move |_, _, window, cx| {
                crate::sidebar::select_sc_commit(window, cx, Some(hash.clone()));
                // Opens/refocuses the Source Control screen, which follows
                // the selection.
                crate::sidebar::activate_tool(
                    window,
                    cx,
                    crate::sidebar::ToolWindow::SourceControl,
                );
            }))
    }
}

impl Render for HistoryList {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let muted = cx.theme().muted_foreground;
        let no_clone = self.seen_clone.is_none();
        v_scroll_pane(
            "hist-scroll",
            &self.scroll,
            gpui_component::v_flex()
                .p_2()
                .gap_0p5()
                .when(no_clone, |this| {
                    this.child(
                        div()
                            .py_2()
                            .text_xs()
                            .text_color(muted)
                            .child("No repository resolved yet."),
                    )
                })
                .children(
                    self.history
                        .iter()
                        .map(|commit| self.commit_row(commit, cx)),
                )
                .when(!no_clone && self.history.is_empty(), |this| {
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
                        Button::new("hist-more")
                            .ghost()
                            .xsmall()
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
                }),
        )
    }
}
