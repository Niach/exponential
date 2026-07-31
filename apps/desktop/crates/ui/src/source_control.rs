//! Source Control screen (masterplan v4 §4.4, EXP-253 master-only rework) —
//! trunk-only and READ-ONLY: the shared diff renderer full-width (EXP-258:
//! the per-file changes column is gone — the trunk is expected always clean,
//! so the pane shows commit diffs from the sidebar History, and a dirty tree
//! is an ANOMALY that only surfaces a slim count/discard strip), plus (in
//! conflict mode) the rebase/merge banner with "Open terminal" / "Abort" /
//! "Discard & reset" (EXP-259 deleted the claude-task "Fix conflicts with
//! Claude" button — PR merge conflicts are fixed by the builtin "Fix merge
//! conflicts" ACTION run from the Reviews list / actions surfaces; a trunk
//! rebase/merge conflict is a manual-recovery anomaly). The IDE neither
//! stages, commits nor pushes anymore — the editor is view-only, changes
//! arrive via PRs, and the trunk is kept fresh by the headless
//! [`crate::trunk_sync`] engine. The ONE write affordance is the escape
//! hatch: discard local changes via
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
//! finishes the rebase — a terminal, another tool, or the reset hatch. All
//! git invocations are argv-only through [`coding::scm`] — never `gh`, never
//! a library (DNR L5).

use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, AnyElement, App, AppContext as _, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    scroll::{ScrollableElement as _, ScrollbarAxis},
    v_virtual_list, ActiveTheme as _, Disableable as _, Sizable as _, VirtualListScrollHandle,
};
use sync::Store;

use coding::scm::{self, CommitInfo, ConflictKind, ConflictState};

use crate::coding_flow::{self, CodingHub};
use crate::diff::{build_scm_diff, DiffView};
use crate::icons::registry;
use crate::navigation::{self, Navigation};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// History page size (§4.4: "200 at a time, Load more").
const HISTORY_PAGE: usize = 200;
/// Fixed sidebar history row height (EXP-344: the list is virtualized, so
/// every row pins this height — two `text_xs` lines + the row inset).
const HISTORY_ROW_HEIGHT: f32 = 48.;
/// The trailing "Load more" row's height.
const HISTORY_MORE_ROW_HEIGHT: f32 = 30.;

/// What the diff pane is showing.
#[derive(Clone)]
enum Selection {
    /// No commit picked yet (placeholder).
    None,
    /// One conflicted file's marker diff (conflict-banner chip).
    ConflictFile,
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
    /// The sidebar history selection this view last applied (`None` = none).
    seen_commit: Option<String>,
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
            seen_commit: None,
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
                // A resolved conflict invalidates a chip's marker diff.
                if this.conflict.is_none()
                    && matches!(this.selection, Selection::ConflictFile)
                {
                    this.selection = Selection::None;
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
            })
            .unwrap_or_else(|| "the remote branch".to_string());
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        // The hatch stays available while an Action / agent-shell tab is
        // working on this clone (it's the escape hatch), but the confirm
        // must say the tree is about to move under a live session.
        let session_live = trunk_sync.read(cx).repo_agents_alive(window, cx);
        let this = cx.entity().downgrade();
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
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.selection = Selection::None;
                    this.error = None;
                    cx.notify();
                });
            }
            true
        });
        crate::native_dialog::open_alert(window, cx, spec);
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
                        .xsmall()
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
                    .xsmall()
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
                            Button::new("scm-hard-reset")
                                .xsmall()
                                .label("Discard changes & reset…")
                                .disabled(self.busy.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_hard_reset(window, cx);
                                })),
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
        // Follow the sidebar history list's commit selection.
        let want = self
            .rail
            .read(cx)
            .sc_selected_commit()
            .map(str::to_string);
        if want != self.seen_commit {
            self.seen_commit = want.clone();
            match want {
                Some(hash) => self.select_commit(hash, cx),
                // Deselected out from under us (scope change) — back to the
                // placeholder.
                None => {
                    if matches!(self.selection, Selection::Commit) {
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
            scroll: VirtualListScrollHandle::new(),
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

    /// One virtual row: a commit for `ix < history.len()`, else the trailing
    /// "Load more" row (present only while `history_has_more`).
    fn render_row(&self, ix: usize, cx: &mut gpui::Context<Self>) -> AnyElement {
        match self.history.get(ix) {
            Some(commit) => self.commit_row(commit, cx).into_any_element(),
            None => self.load_more_row(cx).into_any_element(),
        }
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
        // EXP-282: flat edge-to-edge rows like the issue list — the pill
        // inset (rounded + horizontal container padding) and the pre-glass
        // `accent` fills are gone; hover/selected use the glass list tokens.
        // EXP-344: the row pins the virtual list's fixed height.
        gpui_component::v_flex()
            .id(SharedString::from(format!("hist-commit-{}", commit.hash)))
            .w_full()
            .h(px(HISTORY_ROW_HEIGHT))
            .overflow_hidden()
            .justify_center()
            .gap_0p5()
            .px_3()
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
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
                    .truncate()
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

    /// History "Load more" (§4.4) as the list's trailing virtual row.
    fn load_more_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .h(px(HISTORY_MORE_ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .child(
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
    }
}

impl Render for HistoryList {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let no_clone = self.seen_clone.is_none();

        // Empty states need no scroller at all.
        if no_clone || self.history.is_empty() {
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
        // rows. The optional "Load more" row rides along as the last index.
        let mut sizes = vec![size(px(0.), px(HISTORY_ROW_HEIGHT)); self.history.len()];
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
