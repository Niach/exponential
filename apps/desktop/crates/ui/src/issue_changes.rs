//! Issue **Changes** tab (masterplan v4 §4.8 / §4.9) — the single diff surface
//! per issue, folding in the old standalone PR section.
//!
//! A segmented `Details · Changes` header (owned by [`crate::issue_detail`])
//! flips the issue-detail body to this view. Changes is the *only* place a
//! worktree is visible; it renders (v4 §4.8 sketch):
//!
//! ```text
//! ⎇ exp/EXP-42   ● Claude running   5 files  +120 −34
//! [Open terminal in worktree]                       [⋯]
//! ┌─ files ────────┬─ side-by-side diff (shared renderer) ─┐
//! │ M src/app.rs   │  …                                    │
//! └────────────────┴───────────────────────────────────────┘
//! ```
//!
//! **Capability-tiered source (v4 §4.8, same tab meaning on every client):**
//! 1. local worktree exists (`worktree_path` on disk) → **live local diff**
//!    (`git diff <merge-base(origin/<default>, HEAD)>`, so committed *and*
//!    uncommitted tracked changes; header label "Local — includes uncommitted");
//! 2. a linked PR → PR diff (`issues.prFiles`, as the old PR section did);
//! 3. no worktree, no PR → the branchDiff tier is web/mobile only (desktop
//!    always has tier 1 when the worktree exists), so this collapses to a
//!    PR-absent empty state pointing at the web app for now.
//!
//! **Freshness (v4 §4.8):** refresh on tab focus ([`IssueChanges::set_visible`]),
//! plus a slow poll while the tab is visible and a local worktree is the source
//! (the terminal-quiet debounce is folded into the slow poll for v1 — the PTY
//! tee lives in the §08 steer track, not here). No FS watcher in v4.
//!
//! **Actions (v4 §4.8/§4.9):** *Open terminal in worktree* (a shell tab with
//! `cwd = worktree`), and an overflow menu with *Update from main* (a Claude
//! task — [`coding::claude_task`] — in the worktree, "rebase onto
//! origin/<default>…") and *Clean up worktree* (visible once the PR is
//! merged/closed: `git worktree remove` + local branch delete, blocked while a
//! session runs or the tree is dirty, with the reason).
//!
//! All git here routes through [`coding::scm`] (`branch_diff` for the tier-1
//! local diff, `remove_worktree` for clean-up) — argv `git`, never `gh`,
//! never a git library (masterplan L5) — so tier 1 renders through the exact
//! same `diff.rs` renderer as the PR path.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    div, px, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    highlighter::HighlightTheme,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;
use terminal::TabKind;

use api::issues::PullFile;
use coding::scm::DiffFile;
use domain::rows::Issue;

use crate::coding_flow::{window_terminal_manager, CodingHub};
use crate::diff::{build_pr_diff, build_scm_diff, DiffView, FileSummary, PreparedDiff};
use crate::icons::ExpIcon;
use crate::queries;

/// Slow-poll cadence while the Changes tab is visible on a local worktree
/// (v4 §4.8 "a slow poll while visible"). Also absorbs the terminal-quiet
/// refresh for v1 (see the module doc).
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Everything the actions need about the resolved repo/worktree for an issue.
#[derive(Clone)]
struct RepoContext {
    default_branch: String,
    /// The clone dir (`<repos_root>/<owner>/<name>`).
    clone: PathBuf,
    /// The git branch (`<prefix><IDENTIFIER>`, slashes kept).
    branch: String,
    /// The worktree dir (`<clone>.worktrees/<branch-sanitized>`).
    worktree: PathBuf,
}

/// Which §4.8 source tier is live for the current issue.
enum Tier {
    /// Resolving the repo + probing the worktree on disk.
    Resolving,
    /// Tier 1: a local worktree — live local diff.
    Local,
    /// Tier 2: a linked PR — PR diff.
    Pr,
    /// Tier 3/4: nothing local, no PR.
    Absent,
}

/// The result of one background load (repo probe + tier-appropriate diff).
/// The stat line and the (Tree-sitter-heavy) diff rows are both built on the
/// background executor, so `apply` only swaps them onto the foreground.
enum Loaded {
    Local {
        repo: RepoContext,
        result: Result<(Stats, PreparedDiff), String>,
    },
    Pr {
        stats: Stats,
        prepared: PreparedDiff,
    },
    Absent,
}

/// Additions/deletions/file counts for the header stat line.
#[derive(Clone, Copy, Default)]
struct Stats {
    files: usize,
    additions: u32,
    deletions: u32,
}

pub struct IssueChanges {
    issue_id: Option<String>,
    /// The Changes tab is the active segment (the detail view drives this via
    /// [`set_visible`] — a hidden tab never fetches or polls).
    ///
    /// [`set_visible`]: IssueChanges::set_visible
    visible: bool,
    diff: Entity<DiffView>,
    tier: Tier,
    /// Resolved repo/worktree for the actions (Some once a repo resolves).
    repo: Option<RepoContext>,
    stats: Option<Stats>,
    /// A transient action error (e.g. clean-up blocked on a dirty tree).
    action_error: Option<SharedString>,
    /// Bumped per load; a stale background result is dropped on apply.
    load_gen: u64,
    /// Bumped when the issue or visibility changes; invalidates a poll loop.
    poll_token: u64,
    _subscriptions: Vec<Subscription>,
}

impl IssueChanges {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let diff = cx.new(|cx| DiffView::new(window, cx));
        let collections = Store::global(cx).collections().clone();
        // The header (branch chip, session badge, stat line) and the tier
        // decision read the live issue + coding_sessions shapes.
        let subscriptions = vec![
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
        ];
        Self {
            issue_id: None,
            visible: false,
            diff,
            tier: Tier::Resolving,
            repo: None,
            stats: None,
            action_error: None,
            load_gen: 0,
            poll_token: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Point the tab at another issue (navigation edge). Resets everything and,
    /// if the tab is currently visible, kicks a fresh load + poll.
    pub fn set_issue(&mut self, issue_id: Option<String>, cx: &mut gpui::Context<Self>) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.tier = Tier::Resolving;
        self.repo = None;
        self.stats = None;
        self.action_error = None;
        // Invalidate any in-flight poll for the previous issue.
        self.poll_token += 1;
        if self.visible {
            self.refresh(cx);
            self.start_poll(cx);
        }
        cx.notify();
    }

    /// The Changes segment became active/inactive. Focus refresh (v4 §4.8):
    /// showing the tab always re-loads; hiding it stops the poll.
    pub fn set_visible(&mut self, visible: bool, cx: &mut gpui::Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.refresh(cx);
            self.start_poll(cx);
        } else {
            // Invalidate the running poll loop.
            self.poll_token += 1;
        }
        cx.notify();
    }

    fn issue(&self, cx: &App) -> Option<Issue> {
        let issue_id = self.issue_id.as_deref()?;
        Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .cloned()
    }

    // -- load pipeline ---------------------------------------------------------

    /// Re-resolve the source tier and (re)load its diff on the background
    /// executor. Foreground reads (issue, settings, trpc) are snapshotted here;
    /// the blocking git/network work runs off-thread.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let Some(issue) = self.issue(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let has_pr = issue
            .pr_url
            .as_deref()
            .map(|url| !url.is_empty())
            .unwrap_or(false);
        let identifier = issue.identifier.clone();
        // Cloned up front so the diff-row build runs on the background executor.
        let theme = cx.theme().highlight_theme.clone();

        self.load_gen += 1;
        let gen = self.load_gen;
        self.action_error = None;
        self.diff.update(cx, |diff, cx| diff.set_loading(cx));

        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    load_source(&trpc, &issue_id, has_pr, &identifier, &settings, &theme)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.load_gen != gen {
                    return; // superseded by a newer refresh
                }
                this.apply(loaded, cx);
            });
        })
        .detach();
    }

    fn apply(&mut self, loaded: Loaded, cx: &mut gpui::Context<Self>) {
        match loaded {
            Loaded::Local { repo, result } => {
                self.repo = Some(repo);
                self.tier = Tier::Local;
                match result {
                    Ok((stats, prepared)) => {
                        self.stats = Some(stats);
                        self.diff.update(cx, |diff, cx| diff.set_prepared(prepared, cx));
                    }
                    Err(message) => {
                        self.stats = None;
                        self.diff.update(cx, |diff, cx| diff.set_error(message, cx));
                    }
                }
            }
            Loaded::Pr { stats, prepared } => {
                self.tier = Tier::Pr;
                self.stats = Some(stats);
                self.diff.update(cx, |diff, cx| diff.set_prepared(prepared, cx));
            }
            Loaded::Absent => {
                self.tier = Tier::Absent;
                self.stats = None;
            }
        }
        cx.notify();
    }

    /// Poll-tick refresh (v4 §4.8): the issue→repo mapping is immutable, so a
    /// visible-poll tick re-runs ONLY the local `git diff` against the cached
    /// worktree — never the `for_issue` / `pr_files` network resolve that the
    /// full [`refresh`] does. Falls back to a full [`refresh`] if no repo is
    /// cached yet (nothing local to diff). Both the diff and the stat line are
    /// rebuilt on the background executor; a tick never flashes the loading
    /// state over the currently-rendered diff.
    ///
    /// [`refresh`]: IssueChanges::refresh
    fn refresh_local_diff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            self.refresh(cx);
            return;
        };
        let theme = cx.theme().highlight_theme.clone();
        let worktree = repo.worktree.clone();
        let default_branch = repo.default_branch.clone();
        self.load_gen += 1;
        let gen = self.load_gen;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { local_diff_prepared(&worktree, &default_branch, &theme) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.load_gen != gen {
                    return; // superseded by a newer (full or poll) refresh
                }
                this.apply(Loaded::Local { repo, result }, cx);
            });
        })
        .detach();
    }

    /// Start (or restart) the slow poll. Poll ticks only re-load while the tab
    /// stays visible on the same issue and a local worktree is the source
    /// (PR/absent tiers are static — no point re-fetching).
    fn start_poll(&mut self, cx: &mut gpui::Context<Self>) {
        self.poll_token += 1;
        let token = self.poll_token;
        let issue_id = self.issue_id.clone();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let alive = this
                .update(cx, |this, cx| {
                    if this.poll_token != token || !this.visible || this.issue_id != issue_id {
                        return false;
                    }
                    if matches!(this.tier, Tier::Local) {
                        this.refresh_local_diff(cx);
                    }
                    true
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();
    }

    // -- actions ---------------------------------------------------------------

    /// Open a shell tab whose cwd is the worktree (v4 §4.8). No-op off a local
    /// worktree.
    fn open_terminal_in_worktree(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(worktree) = self.repo.as_ref().map(|repo| repo.worktree.clone()) else {
            return;
        };
        let Some(manager) = window_terminal_manager(window, cx) else {
            self.action_error = Some("No terminal dock in this window.".into());
            cx.notify();
            return;
        };
        if let Err(err) = manager.update(cx, |manager, cx| manager.open_shell(Some(worktree), cx)) {
            self.action_error = Some(format!("Could not open a terminal: {err}").into());
            cx.notify();
        }
    }

    // -- header pieces ---------------------------------------------------------

    /// The stat line ("N files  +A −D"), rendered from whatever tier loaded.
    fn render_stats(&self, cx: &App) -> Option<impl IntoElement> {
        let stats = self.stats?;
        Some(
            h_flex()
                .gap_2()
                .items_center()
                .text_xs()
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "{} {}",
                            stats.files,
                            if stats.files == 1 { "file" } else { "files" }
                        ))),
                )
                .child(
                    div()
                        .text_color(cx.theme().green)
                        .child(SharedString::from(format!("+{}", stats.additions))),
                )
                .child(
                    div()
                        .text_color(cx.theme().red)
                        .child(SharedString::from(format!("−{}", stats.deletions))),
                ),
        )
    }

    fn render_header(&mut self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Branch chip: the resolved worktree branch, else the issue's PR branch.
        let branch = self
            .repo
            .as_ref()
            .map(|repo| repo.branch.clone())
            .or_else(|| issue.branch.clone().filter(|branch| !branch.is_empty()));

        let mut left = h_flex().gap_3().items_center().min_w_0();
        if let Some(branch) = branch {
            left = left.child(
                div()
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_color(cx.theme().foreground)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(format!("⎇ {branch}"))),
            );
        }
        // Session badge — a running coding_sessions row (any client, v4 §4.8).
        if session_running(&issue.id, cx) {
            left = left.child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .size_1p5()
                            .rounded_full()
                            .bg(theme::tokens::GREEN.to_hsla()),
                    )
                    .child("Claude running"),
            );
        }
        // Source label (§4.8 freshness copy).
        let source_label = match self.tier {
            Tier::Resolving => Some("Loading…".to_string()),
            Tier::Local => Some("Local — includes uncommitted".to_string()),
            Tier::Pr => issue
                .pr_number
                .map(|number| format!("PR #{number}"))
                .or(Some("Pull request".to_string())),
            Tier::Absent => None,
        };
        if let Some(label) = source_label {
            left = left.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(label)),
            );
        }
        if let Some(stats) = self.render_stats(cx) {
            left = left.child(stats);
        }

        let mut row = h_flex()
            .w_full()
            .px_4()
            .py_2()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(left)
            .child(div().flex_1());

        // Open-terminal-in-worktree (tier 1 only — needs a worktree on disk).
        if matches!(self.tier, Tier::Local) {
            row = row.child(
                Button::new("changes-open-terminal")
                    .ghost()
                    .xsmall()
                    .icon(
                        Icon::new(IconName::SquareTerminal)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .label("Open terminal in worktree")
                    .tooltip("Open a shell in this issue's worktree")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_terminal_in_worktree(window, cx)
                    })),
            );
        }
        row = row.children(self.render_overflow_menu(issue, cx));
        row
    }

    /// The `⋯` overflow menu: Update from main / Clean up worktree (v4 §4.8).
    /// `None` while no repo is resolved — every item needs one, and an empty
    /// menu button is just confusing chrome (EXP-67).
    fn render_overflow_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        self.repo.as_ref()?;
        let repo = self.repo.clone();
        // Clean up appears once the PR is merged/closed (v4 §4.8).
        let pr_state = issue.pr_state.clone().unwrap_or_default();
        let show_cleanup = pr_state == "merged"
            || pr_state == "closed"
            || issue.pr_merged_at.is_some();
        // Blocked while a session runs (dirtiness is checked at click time).
        let running = session_running(&issue.id, cx);
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        let identifier = issue.identifier.clone();
        let weak = cx.entity().downgrade();

        Some(
            Button::new("changes-overflow")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::Ellipsis).text_color(cx.theme().muted_foreground))
                .dropdown_menu(move |mut menu, _, _| {
                    if let Some(repo) = repo.clone() {
                        // Update from main → a Claude task in the worktree (§4.9).
                        // Blocked while a session runs: a second `claude` in the
                        // same worktree would supersede the session transcript
                        // the public-activity emitter tails.
                        let settings = settings.clone();
                        let identifier = identifier.clone();
                        let update_repo = repo.clone();
                        let item = if running {
                            PopupMenuItem::new(
                                "Update from main — stop the running session first",
                            )
                            .icon(Icon::from(ExpIcon::Repeat))
                            .disabled(true)
                        } else {
                            PopupMenuItem::new("Update from main")
                                .icon(Icon::from(ExpIcon::Repeat))
                                .on_click(move |_, window, cx| {
                                    update_from_main(
                                        &settings,
                                        &update_repo,
                                        &identifier,
                                        window,
                                        cx,
                                    );
                                })
                        };
                        menu = menu.item(item);

                        if show_cleanup {
                            let weak = weak.clone();
                            let cleanup_repo = repo.clone();
                            let item = if running {
                                PopupMenuItem::new(
                                    "Clean up worktree — stop the running session first",
                                )
                                .icon(Icon::new(IconName::Delete))
                                .disabled(true)
                            } else {
                                PopupMenuItem::new("Clean up worktree")
                                    .icon(Icon::new(IconName::Delete))
                                    .on_click(move |_, _, cx| {
                                        cleanup_worktree(weak.clone(), cleanup_repo.clone(), cx);
                                    })
                            };
                            menu = menu.item(item);
                        }
                    }
                    menu
                }),
        )
    }

    // -- body ------------------------------------------------------------------

    fn render_body(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        if matches!(self.tier, Tier::Absent) {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No local worktree or pull request yet."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground.opacity(0.7))
                        .child("Start coding on this issue, or open it in the web app."),
                )
                .into_any_element();
        }

        // File list (left) + shared side-by-side diff renderer (right).
        let files: Vec<FileSummary> = self.diff.read(cx).files().to_vec();
        h_flex()
            .flex_1()
            .min_h_0()
            .items_start()
            .child(self.render_file_list(&files, cx))
            .child(div().flex_1().min_w_0().h_full().child(self.diff.clone()))
            .into_any_element()
    }

    fn render_file_list(
        &self,
        files: &[FileSummary],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut list = v_flex().w_full().p_1().gap_0p5();
        if files.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No changed files."),
            );
        }
        for (ix, file) in files.iter().enumerate() {
            let filename = file.filename.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("changes-file-{ix}")))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1p5()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().colors.list_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.diff.update(cx, |diff, cx| diff.scroll_to_file(ix, cx));
                    }))
                    .child(
                        div()
                            .w(px(12.))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(status_color(&file.status, cx))
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(status_glyph(&file.status))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(filename),
                    ),
            );
        }
        div()
            .id("changes-file-list")
            .w(px(240.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(list)
    }
}

impl Render for IssueChanges {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let base = v_flex().size_full();
        let Some(issue) = self.issue(cx) else {
            return base.into_any_element();
        };

        // The Absent tier renders only its empty state — a header with no
        // branch, no stats, and no actions is just an empty bar (EXP-67).
        let mut view = base;
        if !matches!(self.tier, Tier::Absent) {
            view = view.child(self.render_header(&issue, cx));
        }
        if let Some(err) = self.action_error.clone() {
            view = view.child(
                div()
                    .w_full()
                    .px_4()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(err),
            );
        }
        view.child(self.render_body(cx)).into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Background load (repo probe + tier-appropriate diff) — gpui-free
// ---------------------------------------------------------------------------

/// Resolve the §4.8 source tier for `issue_id` and load its diff. Local worktree
/// wins, then a linked PR, then absent. Never fails hard — a repo-probe hiccup
/// falls through to the PR / absent tiers.
fn load_source(
    trpc: &api::TrpcClient,
    issue_id: &str,
    has_pr: bool,
    identifier: &str,
    settings: &coding::Settings,
    theme: &HighlightTheme,
) -> Loaded {
    if let Ok(Some(repo)) = api::repositories::for_issue(trpc, issue_id) {
        let clone = coding::clone_path(&settings.repos_root_path(), &repo.full_name);
        let branch = coding::branch_name(&settings.branch_prefix, identifier);
        let worktree = coding::worktree_path(&clone, &branch);
        if worktree.join(".git").exists() {
            let ctx = RepoContext {
                default_branch: repo.default_branch.clone(),
                clone,
                branch,
                worktree: worktree.clone(),
            };
            let result = local_diff_prepared(&worktree, &repo.default_branch, theme);
            return Loaded::Local { repo: ctx, result };
        }
    }
    if has_pr {
        // A PR that can't load still means we're in the PR tier — surface the
        // empty state rather than a hard error (freshness will retry).
        let files = api::issues::pr_files(trpc, issue_id)
            .map(|pr| pr.files)
            .unwrap_or_default();
        return Loaded::Pr {
            stats: stats_from_pr(&files),
            prepared: build_pr_diff(&files, theme),
        };
    }
    Loaded::Absent
}

/// [`coding::scm::branch_diff`] plus the background-side stat + Tree-sitter
/// row build (§4.8 tier 1). Shared by the full [`IssueChanges::refresh`] and
/// the poll-tick [`IssueChanges::refresh_local_diff`], so both do all the
/// heavy work off the foreground and hand `apply` a ready-to-swap
/// [`PreparedDiff`].
fn local_diff_prepared(
    worktree: &Path,
    default_branch: &str,
    theme: &HighlightTheme,
) -> Result<(Stats, PreparedDiff), String> {
    let files =
        coding::scm::branch_diff(worktree, default_branch).map_err(|err| err.to_string())?;
    let stats = stats_from_scm(&files);
    let prepared = build_scm_diff(&files, theme);
    Ok((stats, prepared))
}

/// Update from main (v4 §4.9): a Claude task in the worktree — "rebase onto
/// origin/<default>, resolve conflicts, verify the build, then push with
/// --force-with-lease" — opened as a `ClaudeTask` terminal tab.
fn update_from_main(
    settings: &coding::Settings,
    repo: &RepoContext,
    identifier: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(manager) = window_terminal_manager(window, cx) else {
        return;
    };
    let prompt = coding::resolve_pr_prompt(&repo.default_branch);
    let label = format!("Update from main · {identifier}");
    let task = coding::claude_task(settings, &repo.worktree, &prompt, &label);
    let _ = manager.update(cx, |manager, cx| {
        manager.open_tab(TabKind::ClaudeTask, task.tab_title.clone(), &task.spawn, None, cx)
    });
}

/// Clean up worktree (v4 §4.8): [`coding::scm::remove_worktree`] — dirty
/// refusal, then `git worktree remove` + prune + local branch delete. Blocked
/// (with the reason surfaced on the view) when the tree is dirty; the
/// running-session block is enforced at menu-build time.
fn cleanup_worktree(
    weak: gpui::WeakEntity<IssueChanges>,
    repo: RepoContext,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let outcome = cx
            .background_executor()
            .spawn(async move {
                coding::scm::remove_worktree(&repo.clone, &repo.worktree, &repo.branch)
                    .map_err(|err| err.detail)
            })
            .await;
        let _ = weak.update(cx, |this, cx| match outcome {
            Ok(()) => {
                // Worktree gone → the tier flips (to PR/absent) on reload.
                this.refresh(cx);
                // …and the sidebar flow graph drops the lane immediately
                // (local re-read of the window's git chrome, deferred).
                crate::navigation::on_active_window(cx, |window, cx| {
                    let git_bar = crate::sidebar::rail_shared_for_window(window, cx)
                        .read(cx)
                        .git_bar()
                        .clone();
                    git_bar.update(cx, |bar, cx| bar.reread_local(cx));
                });
            }
            Err(message) => {
                this.action_error = Some(message.into());
                cx.notify();
            }
        });
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn stats_from_scm(files: &[DiffFile]) -> Stats {
    Stats {
        files: files.len(),
        additions: files.iter().map(|f| f.additions).sum(),
        deletions: files.iter().map(|f| f.deletions).sum(),
    }
}

fn stats_from_pr(files: &[PullFile]) -> Stats {
    Stats {
        files: files.len(),
        additions: files.iter().map(|f| f.additions).sum(),
        deletions: files.iter().map(|f| f.deletions).sum(),
    }
}

/// A running `coding_sessions` row for the issue (any client — the synced
/// shape, matching the issue-detail "coding now" pill).
fn session_running(issue_id: &str, cx: &App) -> bool {
    Store::global(cx)
        .collections()
        .coding_sessions
        .read(cx)
        .iter()
        .any(|session| {
            session.issue_id.as_deref() == Some(issue_id)
                && session.status.as_deref() == Some("running")
        })
}

/// One-letter status glyph for a file-list row (matches the GitHub-style status
/// vocabulary the diff renderer speaks).
fn status_glyph(status: &str) -> &'static str {
    match status {
        "added" => "A",
        "removed" => "D",
        "renamed" => "R",
        _ => "M",
    }
}

fn status_color(status: &str, cx: &App) -> gpui::Hsla {
    match status {
        "added" => cx.theme().green,
        "removed" => cx.theme().red,
        _ => cx.theme().muted_foreground,
    }
}
