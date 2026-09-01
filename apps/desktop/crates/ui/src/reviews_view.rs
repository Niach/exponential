//! The Reviews center screen (EXP-706): open pull requests across the team,
//! each mergeable row with a two-click inline merge confirm.
//!
//! It used to be a rail TOOL window — a narrow list docked left of the diff.
//! EXP-706 promoted it to a tab-less FULL-PAGE screen like Devices / Actions:
//! one centered column (capped narrower than the settings-shaped pages, the
//! web route's `max-w-3xl`), no page title (the first board group header is
//! the page's first line, exactly as on web), and the PR diff its rows open is
//! the center view that replaces it.
//!
//! Issue-linked PRs come from the synced issues shape, grouped by board; below
//! them, PRs NOT linked to anything (manual branches, external contributors)
//! come from a background `repositories.openPulls` fetch, grouped by repo — the
//! synced lists never wait on GitHub. Merging goes through the server
//! (`issues.mergePr` / `repositories.mergePull`, GitHub App squash) — never
//! local git; synced rows leave the list via the Electric echo, unlinked pulls
//! are removed locally.

use std::collections::HashSet;

use gpui::{
    div, prelude::FluentBuilder as _, ClickEvent, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::actions_view::page_scaffold_with;
use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, resolved_screen, Navigation, Screen};
use crate::pr_merge::{pull_merge_key, MergeOp, MergeState};
use crate::queries;

/// The page column's cap — the web reviews route's `max-w-3xl`. Narrower than
/// [`crate::actions_view::page_scaffold`]'s default: these rows are short, and
/// a 1024px line of PR titles reads as a table, not a queue.
const REVIEWS_COLUMN_W: f32 = 768.;

pub struct ReviewsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// Fetched `repositories.openPulls` result: `(team_id, repos)` — open PRs
    /// with NO issue link (release PRs, manual branches, external
    /// contributors), listed straight from GitHub. Rendered below the board
    /// groups; a merged pull is removed locally (no Electric echo).
    open_pulls: Option<(String, Vec<api::repositories::OpenPullsRepo>)>,
    /// The team the current openPulls fetch belongs to. Cleared by
    /// [`Self::mark_pulls_stale`] whenever the screen is (re-)entered, so a
    /// return refetches (the server caches ~60s; there is deliberately no
    /// polling).
    open_pulls_key: Option<String>,
    /// Bumped per fetch — a stale response checks it before landing.
    open_pulls_seq: u64,
    _subscriptions: Vec<Subscription>,
}

impl ReviewsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.teams, |_, _, cx| cx.notify()));
        }
        // EXP-325: the rows' merge arm/spinner/error live in the shared
        // app-global merge state (any surface can drive them).
        let merge_state = MergeState::global(cx);
        subscriptions.push(cx.observe(&merge_state, |_, _, cx| cx.notify()));
        // "Fixing…" parks a row's button while a local fix run holds its
        // branch — that registry is process-global, not synced.
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        subscriptions.push(cx.observe(&local_sessions, |_, _, cx| cx.notify()));

        Self {
            nav,
            scroll: ScrollHandle::new(),
            open_pulls: None,
            open_pulls_key: None,
            open_pulls_seq: 0,
            _subscriptions: subscriptions,
        }
    }

    /// Drop the openPulls fetch key so the next render refetches. The screens
    /// panel calls this on every transition INTO the screen — the view is
    /// long-lived, so re-entering it is the only "opened" signal the GitHub
    /// half of the list gets (the synced half is always live).
    pub fn mark_pulls_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.open_pulls_key = None;
        cx.notify();
    }

    /// Kick the `repositories.openPulls` fetch when the screen is entered or
    /// the team changes — never on a timer (the server caches ~60s). Data from
    /// another team is dropped immediately; a re-entry in the same team keeps
    /// rendering the previous result while the refresh is in flight.
    fn ensure_open_pulls(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.open_pulls_key.as_deref() == Some(team_id) {
            return;
        }
        self.open_pulls_key = Some(team_id.to_string());
        if self
            .open_pulls
            .as_ref()
            .is_some_and(|(ws, _)| ws != team_id)
        {
            self.open_pulls = None;
        }
        self.open_pulls_seq += 1;
        let seq = self.open_pulls_seq;
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let ws = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let call_ws = ws.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::open_pulls(&trpc, &call_ws) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.open_pulls_seq != seq {
                    return;
                }
                match result {
                    Ok(repos) => {
                        this.open_pulls = Some((ws, repos));
                        cx.notify();
                    }
                    Err(err) => {
                        // The synced rows still render; the unlinked section
                        // just stays absent (same degradation as the web).
                        log::warn!("[ui] repositories.openPulls failed: {err}");
                    }
                }
            });
        })
        .detach();
    }

    /// The row's "Fix conflicts" button (EXP-259): open the Start coding
    /// dialog with the builtin "Fix merge conflicts" action and this PR
    /// preselected (EXP-313 — agent/model/effort stay choosable; the run only
    /// starts when the dialog confirms). The error caption stays — the PR
    /// really does still have conflicts until a fix lands.
    fn on_fix_conflicts_click(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        crate::start_coding_dialog::open_for_fix_conflicts(window, cx, team_id, issue_id);
    }

    // -- rows ----------------------------------------------------------------

    /// One Reviews row for a PR entry: PR icon + identifier + title with a
    /// trailing Merge button, the branch as a sub-line, optional error
    /// caption. A single-issue entry shows the issue identifier + title; a
    /// BATCH entry (EXP-131: N issues on ONE PR) shows `#<pr_number>` (the
    /// only identifier it has), a "N issues" count, and the linked identifiers
    /// in place of the title. Merge acts on the representative issue's id —
    /// the server merges the ONE PR and completes every linked issue. Clicking
    /// the row opens the PR diff screen (EXP-181), which owns the close-PR
    /// affordance (EXP-706 took the ghost `×` off this row: the list is a
    /// queue of things to merge, rejection is a decision made in the diff).
    ///
    /// EXP-706 "Fix conflicts replaces Merge": when a merge failed on a REAL
    /// content conflict the recovery button takes the Merge button's SLOT
    /// instead of trailing the caption — merging is exactly what is blocked,
    /// so offering it there is a dead end.
    fn review_row(
        &self,
        entry: &queries::ReviewEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let issue = entry.representative();
        let is_batch = entry.is_batch();
        let identifier_text = if is_batch {
            match issue.pr_number {
                Some(number) => format!("#{number}"),
                None => issue.identifier.clone(),
            }
        } else {
            issue.identifier.clone()
        };
        let title_text = if is_batch {
            entry
                .issues
                .iter()
                .map(|i| i.identifier.clone())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            issue.title.clone()
        };
        let batch_count = is_batch.then(|| format!("{} issues", entry.issues.len()));

        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        // EXP-277/642: rows use the glass list fills (EXP-269 list_* tokens);
        // hover is the web `GlassRow`'s `hover:bg-glass-active/50`.
        let row_active = theme.list_active;
        let row_hover = row_active.opacity(0.5);
        // Open-PR green (the token the status/priority accents use).
        let pr_green = theme::tokens::GREEN.to_hsla();

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::PrDiff { issue_id }) if issue_id == issue.id
        );
        // EXP-325: the two-click arm/spinner/error live in the shared
        // app-global merge state — a merge driven from the PR diff header or a
        // terminal tab renders here identically.
        let (merging, armed, error, failed_op, is_conflict) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (
                state.merging(&issue.id),
                state.armed(&issue.id),
                state.error(&issue.id),
                state.failed_op(&issue.id),
                state.is_conflict(&issue.id),
            )
        };
        // EXP-259: a failed merge (typically "not mergeable" — conflicts)
        // offers the builtin "Fix merge conflicts" action run right on the
        // row. MERGE failures only — the run ends in a merge, the opposite of
        // what a failed close was asked to do. Needs the PR's recorded branch
        // (the run rebases it); "Fixing…" parks the button only while an
        // ACTUAL fix run works the branch — any other session still holding it
        // is ended by the fix-run launch itself.
        let fixing = issue.branch.as_deref().is_some_and(|branch| {
            crate::coding_flow::LocalSessions::global_ref(cx)
                .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
        });
        let fix_button = error
            .as_ref()
            .filter(|_| failed_op == Some(crate::pr_merge::FailedOp::Merge))
            // EXP-533: only a REAL content conflict (409). A merge that failed
            // because the machine is offline, the base is stale or no GitHub
            // App is installed offers nothing an agent could rebase.
            .filter(|_| is_conflict)
            .filter(|_| issue.branch.is_some())
            .map(|_| {
                let mut button =
                    Button::new(SharedString::from(format!("review-fix-{}", issue.id)))
                        .web_sm()
                        .outline()
                        .cursor_pointer();
                if fixing {
                    button = button.label("Fixing…").disabled(true);
                } else if let Some(reason) = crate::coding_flow::no_agent_reason(cx) {
                    // EXP-367: no agent CLI → disabled with the reason.
                    button = button.label("Fix conflicts").tooltip(reason).disabled(true);
                } else {
                    button = button.label("Fix conflicts");
                }
                let click_id = issue.id.clone();
                button
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.on_fix_conflicts_click(click_id.clone(), window, cx);
                    }))
                    .into_any_element()
            });

        // EXP-706: PR numbers are leaving the row — the branch IS the sub-line.
        let sub = issue.branch.clone().filter(|branch| !branch.is_empty());

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("review-merge-{}", issue.id)))
                .web_sm()
                .outline()
                .cursor_pointer();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger().cursor_pointer();
            } else {
                // EXP-642 (web parity): the merge glyph rides the label.
                button = button.icon(Icon::new(registry::PR_MERGED)).label("Merge");
            }
            let click_id = issue.id.clone();
            button
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    crate::pr_merge::two_click(
                        MergeOp::MergeIssuePr {
                            issue_id: click_id.clone(),
                        },
                        None,
                        None,
                        cx,
                    );
                }))
                .into_any_element()
        };
        // The swap: a conflict-classified merge failure REPLACES Merge.
        let trailing = fix_button.unwrap_or(merge_button);

        let nav_id = issue.id.clone();
        // EXP-642: one glass row CARD per PR (web parity) — selected wears the
        // active fill, hover half of it.
        crate::surface::glass_row_card()
            .id(SharedString::from(format!("review-{}", issue.id)))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .px_3()
            .py_2p5()
            .gap_0p5()
            .when(selected, |this| this.bg(row_active))
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, window, cx| {
                // Any click outside the armed button disarms the confirm.
                MergeState::disarm(cx);
                // The PR diff (EXP-181): a review click is about the CODE —
                // the diff screen renders it, and its header links back to the
                // issue detail for the body.
                crate::navigation::navigate(
                    window,
                    cx,
                    crate::navigation::Screen::PrDiff {
                        issue_id: nav_id.clone(),
                    },
                );
            }))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Icon::from(ExpIcon::GitPullRequest)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(pr_green),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(identifier_text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(fg)
                            .child(SharedString::from(title_text)),
                    )
                    .when_some(batch_count, |this, count| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(count)),
                        )
                    })
                    .child(trailing),
            )
            .when_some(sub, |this, branch| {
                this.child(
                    div()
                        .pl_5()
                        .text_xs()
                        .truncate()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(muted)
                        .child(SharedString::from(branch)),
                )
            })
            .when_some(error, |this, message| {
                // EXP-706: message only — the recovery button moved into the
                // Merge slot above.
                this.child(
                    div()
                        .pl_5()
                        .text_xs()
                        .truncate()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            })
            .into_any_element()
    }

    /// One unlinked-PR row: `#N` + title with a trailing Merge button
    /// (disabled for drafts — GitHub refuses those), sub-line `branch → base`,
    /// optional Draft pill and error caption. Clicking the row opens the PR on
    /// GitHub — no local detail exists behind these.
    fn pull_row(
        &self,
        repository_id: &str,
        pull: &api::repositories::OpenPull,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let radius = theme.radius;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let row_hover = theme.list_active.opacity(0.5);
        let pr_green = theme::tokens::GREEN.to_hsla();

        let key = pull_merge_key(repository_id, pull.number);
        let (merging, armed, error) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (state.merging(&key), state.armed(&key), state.error(&key))
        };

        let sub = format!("{} \u{2192} {}", pull.branch, pull.base_branch);

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("pull-merge-{key}")))
                .web_sm()
                .outline()
                .cursor_pointer();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if pull.draft {
                button = button
                    .icon(Icon::new(registry::PR_MERGED))
                    .label("Merge")
                    .disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger().cursor_pointer();
            } else {
                button = button.icon(Icon::new(registry::PR_MERGED)).label("Merge");
            }
            let click_repo = repository_id.to_string();
            let number = pull.number;
            button.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // There is no Electric echo for unlinked pulls — success drops
                // the row from this view's fetched state.
                let view = cx.entity().downgrade();
                let success_repo = click_repo.clone();
                crate::pr_merge::two_click(
                    MergeOp::MergePull {
                        repository_id: click_repo.clone(),
                        number,
                    },
                    None,
                    Some(Box::new(move |cx: &mut gpui::App| {
                        let _ = view.update(cx, |this: &mut Self, cx| {
                            if let Some((_, repos)) = this.open_pulls.as_mut() {
                                queries::remove_merged_pull(repos, &success_repo, number);
                            }
                            cx.notify();
                        });
                    })),
                    cx,
                );
            }))
        };

        let url = pull.url.clone();
        crate::surface::glass_row_card()
            .id(SharedString::from(format!("pull-{key}")))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .px_3()
            .py_2p5()
            .gap_0p5()
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, _, cx| {
                // Any click outside the armed button disarms the confirm.
                MergeState::disarm(cx);
                crate::settings::open_url(cx, url.clone());
            }))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Icon::from(ExpIcon::GitPullRequest)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(pr_green),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(format!("#{}", pull.number))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(fg)
                            .child(SharedString::from(pull.title.clone())),
                    )
                    .when(pull.draft, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .px_1()
                                .rounded(radius)
                                .bg(muted.opacity(0.15))
                                .text_xs()
                                .text_color(muted)
                                .child("Draft"),
                        )
                    })
                    .child(merge_button),
            )
            .child(
                div()
                    .pl_5()
                    .text_xs()
                    .truncate()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_color(muted)
                    .child(SharedString::from(sub)),
            )
            .when_some(error, |this, message| {
                this.child(
                    div()
                        .pl_5()
                        .text_xs()
                        .truncate()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            })
            .into_any_element()
    }
}

impl Render for ReviewsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let collections = Store::global(cx).collections().clone();
        let is_ready =
            collections.issues.read(cx).is_ready() && collections.boards.read(cx).is_ready();
        let team_id = active_team_id(&self.nav, cx);
        if let Some(id) = team_id.as_deref() {
            self.ensure_open_pulls(id, cx);
        }
        let groups = team_id
            .as_deref()
            .map(|id| queries::review_groups(cx, id))
            .unwrap_or_default();
        let pull_repos: Vec<api::repositories::OpenPullsRepo> = self
            .open_pulls
            .as_ref()
            .filter(|(ws, _)| Some(ws.as_str()) == team_id.as_deref())
            .map(|(_, repos)| queries::visible_pull_repos(repos))
            .unwrap_or_default();

        // Unlinked pulls have no Electric echo — a pull merged elsewhere drops
        // its transient merge state here against the fetched list. (Issue rows
        // are echo-settled by the shared state's own issues observer, EXP-325.)
        {
            let live_keys: HashSet<String> = pull_repos
                .iter()
                .flat_map(|repo| {
                    repo.pulls
                        .iter()
                        .map(|pull| pull_merge_key(&repo.repository_id, pull.number))
                })
                .collect();
            MergeState::global(cx).update(cx, |state, cx| state.retain_pull_keys(&live_keys, cx));
        }

        let muted = cx.theme().muted_foreground;
        let heading_fg = cx.theme().foreground;

        let column = if !is_ready {
            v_flex()
                .min_w_0()
                .gap_2()
                .child(Skeleton::new().h_3p5().w_40())
                .child(Skeleton::new().h_3p5().w_48())
                .child(Skeleton::new().h_3p5().w_32())
        } else if groups.is_empty() && pull_repos.is_empty() {
            // EXP-525: the web `EmptyState` (icon disc + title + description).
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::from(ExpIcon::GitPullRequest),
                "No open pull requests",
                "Open pull requests in this team's repositories land here for review.",
                cx,
            ))
        } else {
            // EXP-642: the web `GlassSectionHeader` over a GAPPED list of glass
            // row CARDS — one card per PR, one headed group per board (and one
            // for each repo's unlinked pulls). EXP-706: the board glyph
            // replaces the old color dot (the web `BoardGlyph`), and the page
            // deliberately has NO title — the first group header IS the top.
            let mut children: Vec<gpui::AnyElement> = Vec::new();
            for group in &groups {
                let mut block = v_flex().min_w_0().gap_2().pb_2().child(
                    h_flex()
                        .min_w_0()
                        .px_1()
                        .pt_1()
                        .gap_1p5()
                        .items_center()
                        .child(
                            crate::icons::board_icon(&group.board)
                                .xsmall()
                                .flex_shrink_0()
                                .text_color(heading_fg.opacity(0.7)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(heading_fg.opacity(0.7))
                                .child(SharedString::from(group.board.name.clone())),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(heading_fg.opacity(0.5))
                                .child(SharedString::from(format!("{}", group.entries.len()))),
                        ),
                );
                for entry in &group.entries {
                    block = block.child(self.review_row(entry, cx));
                }
                children.push(block.into_any_element());
            }
            for repo in &pull_repos {
                let mut block = v_flex().min_w_0().gap_2().pb_2().child(
                    h_flex()
                        .min_w_0()
                        .px_1()
                        .pt_1()
                        .gap_1p5()
                        .items_center()
                        .child(
                            Icon::from(ExpIcon::GitPullRequest)
                                .xsmall()
                                .flex_shrink_0()
                                .text_color(heading_fg.opacity(0.7)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(heading_fg.opacity(0.7))
                                .child(SharedString::from(repo.full_name.clone())),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(heading_fg.opacity(0.5))
                                .child(SharedString::from(format!("{}", repo.pulls.len()))),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted.opacity(0.8))
                                .child("not linked to an issue"),
                        ),
                );
                for pull in &repo.pulls {
                    block = block.child(self.pull_row(&repo.repository_id, pull, cx));
                }
                children.push(block.into_any_element());
            }
            v_flex().min_w_0().gap_2().children(children)
        };

        page_scaffold_with(
            "reviews-screen-scroll",
            &self.scroll,
            column,
            REVIEWS_COLUMN_W,
        )
    }
}
