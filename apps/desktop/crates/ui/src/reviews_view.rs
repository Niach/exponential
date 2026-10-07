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
            // EXP-734: the "Agent runs" block is a live read over the synced
            // `coding_sessions` rows (a run's own chore PR lives there).
            subscriptions.push(cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()));
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
        // Re-entering the screen is a refetch: a refusal from the previous
        // visit describes a snapshot that is no longer the one on screen.
        MergeState::clear_error(cx);
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

    // -- rows ----------------------------------------------------------------

    /// One Reviews row for a PR entry: PR icon + identifier + title with a
    /// trailing Merge button, the branch as a sub-line, optional error
    /// caption. A single-issue entry shows the issue identifier + title; a
    /// BATCH entry (EXP-131: N issues on ONE PR) shows `#<pr_number>` (the
    /// only identifier it has), the `pr-batch` glyph with its issues in a
    /// popover, and the linked identifiers in place of the title. Merge acts on the representative issue's id —
    /// the server merges the ONE PR and completes every linked issue. Clicking
    /// the row opens the PR diff screen (EXP-181), which owns the close-PR
    /// affordance (EXP-706 took the ghost `×` off this row: the list is a
    /// queue of things to merge, rejection is a decision made in the diff).
    ///
    /// EXP-1233: the trailing slot is ALWAYS the Merge button. A merge refused
    /// by a REAL content conflict opens the composer on the Fix merge
    /// conflicts builtin at once (`work_header::conflict_recovery`, the Merge
    /// pill's own hook) and leaves no caption; any other refusal captions the
    /// row.
    fn review_row(
        &self,
        entry: &queries::ReviewEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let issue = entry.representative();
        let is_batch = entry.is_batch();
        let (identifier_text, title_text) = pr_row_texts(&entry.issues);
        // EXP-897: a batch PR wears the `pr-batch` CONCEPT with its issues in
        // a popover, never a bare count.
        let batch_glyph = is_batch.then(|| {
            batch_glyph(
                SharedString::from(format!("review-batch-{}", issue.id)),
                entry.issues.clone(),
                cx,
            )
        });

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::IssueDetail { issue_id }) if issue_id == issue.id
        );
        // EXP-325: the two-click arm/spinner/error live in the shared
        // app-global merge state — a merge driven from the PR diff header or a
        // terminal tab renders here identically.
        let (merging, armed, error) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (
                state.merging(&issue.id),
                state.armed(&issue.id),
                state.error(&issue.id),
            )
        };

        // EXP-706: PR numbers are leaving the row — the branch IS the sub-line.
        let sub = issue.branch.clone().filter(|branch| !branch.is_empty());

        let trailing = {
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
                button = button.icon(Icon::new(registry::PR_MERGED)).label(MERGE_LABEL);
            }
            let click_id = issue.id.clone();
            button
                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    // EXP-1145: a member of an open PR stack asks first
                    // (the list itself stays FLAT).
                    if crate::pr_merge::ask_stack_merge(&click_id, window, cx) {
                        return;
                    }
                    // EXP-1233: a real conflict opens the fix-conflicts
                    // composer — the same hook as the Merge pill's.
                    crate::pr_merge::two_click(
                        MergeOp::MergeIssuePr {
                            issue_id: click_id.clone(),
                            stack_through: None,
                        },
                        Some(crate::work_header::conflict_recovery(
                            click_id.clone(),
                            window.window_handle(),
                        )),
                        None,
                        cx,
                    );
                }))
                .into_any_element()
        };

        let nav_id = issue.id.clone();
        let on_click: crate::run_rows::RunRowAction = Box::new(cx.listener(move |_, _, window, cx| {
            // Any click outside the armed button disarms the confirm.
            MergeState::disarm(cx);
            // EXP-1154: a review click is about the CODE — the issue opens
            // on its Changes face (the review of a PR IS the issue's Work
            // screen). EXP-870: explicitly beside the Reviews queue it came
            // from.
            crate::screens::open_issue_changes(
                &nav_id,
                crate::navigation::Screen::Reviews.list_origin(),
                window,
                cx,
            );
        }));
        pr_row(
            PrRowSpec {
                id: SharedString::from(format!("review-{}", issue.id)),
                identifier: identifier_text,
                title: title_text,
                pr_state: None,
                selected,
                batch_glyph,
                sub,
                error,
                trailing,
                on_click,
            },
            cx,
        )
    }

    /// EXP-734 — one "Agent runs" row: an action or chat run holding a pull
    /// request of its OWN (nothing links it to an issue, so neither the board
    /// groups above nor the `repositories.openPulls` fetch below can carry
    /// it). Shaped like [`Self::pull_row`]: `#N` + the run's name with a
    /// trailing Merge, the branch as the sub-line, an error caption. Merging
    /// goes through `codingSessions.mergePr` and is ECHO-settled on the
    /// session row's `pr_state` — no local removal, unlike an unlinked pull.
    /// EXP-1194: clicking the row opens the RUN on its Changes face beside
    /// this queue (the issue rows' EXP-1154 rule) — the PR's files come from
    /// `codingSessions.prFiles` when the run published no live diff, so a
    /// teammate's or an ended run still lands on the PR diff. The work header
    /// above that face carries the GitHub link.
    fn run_row(
        &self,
        run: &domain::rows::CodingSession,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let row_hover = theme.list_hover;
        let pr_green = theme::tokens::GREEN.to_hsla();

        let key = crate::pr_merge::session_merge_key(&run.id);
        let (merging, armed, error) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (state.merging(&key), state.armed(&key), state.error(&key))
        };

        let number = match run.pr_number {
            Some(number) => format!("#{number}"),
            None => String::new(),
        };
        // A chat run has no action behind it — web/mobile label it with the
        // agent's auto-named title, else "Chat" (EXP-908).
        let title = domain::batch_run::action_run_subject(run)
            .unwrap_or_else(|| domain::batch_run::CHAT_RUN_NAME.to_string());
        let sub = run
            .branch
            .clone()
            .filter(|branch| !branch.is_empty());

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("run-merge-{}", run.id)))
                .web_sm()
                .outline()
                .cursor_pointer();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger().cursor_pointer();
            } else {
                button = button.icon(Icon::new(registry::PR_MERGED)).label("Merge");
            }
            let session_id = run.id.clone();
            button.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // Echo-settled: the row leaves this list when its synced
                // `pr_state` flips off `open` — nothing is removed locally.
                crate::pr_merge::two_click(
                    MergeOp::MergeSessionPr {
                        session_id: session_id.clone(),
                    },
                    None,
                    None,
                    cx,
                );
            }))
        };

        let run_id = run.id.clone();
        crate::surface::flat_row()
            .id(SharedString::from(format!("run-{}", run.id)))
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .min_w_0()
            .px_3()
            .py_2p5()
            .gap_2()
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, window, cx| {
                MergeState::disarm(cx);
                // EXP-1194: the review of a run's PR is the run's Changes
                // face, opened beside the Reviews queue it came from. The
                // face request waits for the view `navigate_from` builds.
                crate::session_screen::open_session_with_origin(
                    &run_id,
                    crate::navigation::Screen::Reviews.list_origin(),
                    window,
                    cx,
                );
                crate::screens::set_run_face(
                    &run_id,
                    crate::screens::RunFace::Diff,
                    window,
                    cx,
                );
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
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
                            .when(!number.is_empty(), |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(muted)
                                        .font_family(theme::terminal::FONT_FAMILY)
                                        .child(SharedString::from(number.clone())),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .truncate()
                                    .text_color(fg)
                                    .child(SharedString::from(title)),
                            ),
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
                        this.child(
                            div()
                                .pl_5()
                                .text_xs()
                                .truncate()
                                .text_color(danger)
                                .child(message),
                        )
                    }),
            )
            .child(merge_button)
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
        let row_hover = theme.list_hover;
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
        crate::surface::flat_row()
            .id(SharedString::from(format!("pull-{key}")))
            .flex()
            .flex_row()
            // EXP-698: same centred trailing slot as the linked-PR row above.
            .items_center()
            .w_full()
            .min_w_0()
            .px_3()
            .py_2p5()
            .gap_2()
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, _, cx| {
                // Any click outside the armed button disarms the confirm.
                MergeState::disarm(cx);
                crate::settings::open_url(cx, url.clone());
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
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
                            }),
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
                                .child(message),
                        )
                    }),
            )
            .child(merge_button)
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
        // EXP-734: runs holding a PR of their own — no issue links them, and
        // the server excludes them from `repositories.openPulls`, so this
        // synced read is the only place they surface.
        let runs = team_id
            .as_deref()
            .map(|id| queries::review_runs(cx, id))
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
                .child(crate::controls::skeleton().h_3p5().w_40())
                .child(crate::controls::skeleton().h_3p5().w_48())
                .child(crate::controls::skeleton().h_3p5().w_32())
        } else if groups.is_empty()
            && runs.is_empty()
            && pull_repos.is_empty()
        {
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
                let mut block = v_flex().min_w_0().pb_2().child(
                    // EXP-818: the group BAND (`surface::glass_section_band`).
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .px_3()
                        .py_1p5()
                        .mb_1()
                        .rounded(gpui::px(theme::tokens::radius::MD))
                        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
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
                // One FLAT row per pull request (deduped by `pr_url`).
                for entry in &group.entries {
                    block = block.child(self.review_row(entry, cx));
                }
                children.push(block.into_any_element());
            }
            // EXP-734: between the board groups and the external pulls — a
            // run's own PR is the team's work (unlike an outside contributor's
            // branch), but it completes no issue, so it gets its own block.
            if !runs.is_empty() {
                let mut block = v_flex().min_w_0().pb_2().child(
                    // EXP-818: the group BAND (`surface::glass_section_band`).
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .px_3()
                        .py_1p5()
                        .mb_1()
                        .rounded(gpui::px(theme::tokens::radius::MD))
                        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
                        .gap_1p5()
                        .items_center()
                        .child(
                            Icon::new(registry::UI_AGENT_SOURCE)
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
                                .child("Agent runs"),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(heading_fg.opacity(0.5))
                                .child(SharedString::from(format!("{}", runs.len()))),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted.opacity(0.8))
                                .child("not linked to an issue"),
                        ),
                );
                for run in &runs {
                    block = block.child(self.run_row(run, cx));
                }
                children.push(block.into_any_element());
            }
            for repo in &pull_repos {
                let mut block = v_flex().min_w_0().pb_2().child(
                    // EXP-818: the group BAND (`surface::glass_section_band`).
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .px_3()
                        .py_1p5()
                        .mb_1()
                        .rounded(gpui::px(theme::tokens::radius::MD))
                        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
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

// ---------------------------------------------------------------------------
// The PR row (SLOP-16 round 3: shared with the work header's "Related work")
// ---------------------------------------------------------------------------

/// A PR entry's identifier + title (EXP-131): a single-issue entry shows the
/// issue identifier + title; a BATCH (N issues on ONE PR) shows `#<number>`
/// (the only identifier it has — its identifier without one) and the linked
/// identifiers in place of the title. `issues[0]` = the representative.
pub(crate) fn pr_row_texts(issues: &[domain::rows::Issue]) -> (String, String) {
    let Some(issue) = issues.first() else {
        return (String::new(), String::new());
    };
    if issues.len() > 1 {
        let identifier = match issue.pr_number {
            Some(number) => format!("#{number}"),
            None => issue.identifier.clone(),
        };
        let title = issues
            .iter()
            .map(|i| i.identifier.clone())
            .collect::<Vec<_>>()
            .join(", ");
        (identifier, title)
    } else {
        (issue.identifier.clone(), issue.title.clone())
    }
}

/// Everything one [`pr_row`] draws.
pub(crate) struct PrRowSpec {
    pub(crate) id: SharedString,
    /// The mono identifier ([`pr_row_texts`]).
    pub(crate) identifier: String,
    pub(crate) title: String,
    /// The PR's state; `None` = open (the Reviews queue only lists open PRs).
    /// A merged or closed PR wears its own glyph, muted.
    pub(crate) pr_state: Option<String>,
    /// The active fill (the PR on screen).
    pub(crate) selected: bool,
    /// The batch glyph after the title.
    pub(crate) batch_glyph: Option<gpui::AnyElement>,
    /// The branch sub-line.
    pub(crate) sub: Option<String>,
    /// A failed merge's caption.
    pub(crate) error: Option<SharedString>,
    /// The trailing action slot, centred against the whole row.
    pub(crate) trailing: gpui::AnyElement,
    pub(crate) on_click: crate::run_rows::RunRowAction,
}

/// THE pull request row — the Reviews queue's: PR glyph · mono identifier ·
/// title · batch glyph, the sub-lines under it, the trailing slot centred.
pub(crate) fn pr_row(spec: PrRowSpec, cx: &gpui::App) -> gpui::AnyElement {
    let theme = cx.theme();
    let fg = theme.foreground;
    let muted = theme.muted_foreground;
    let danger = theme.danger;
    // EXP-277/642: rows use the glass list fills (EXP-269 list_* tokens);
    // hover is the web `GlassRow`'s `hover:bg-glass-active/50`.
    let row_active = theme.list_active;
    let row_hover = row_active.opacity(0.5);
    // Open-PR green (the token the status/priority accents use).
    let (glyph, tint) = match spec.pr_state.as_deref() {
        Some("merged") => (registry::PR_MERGED, muted),
        Some("closed") => (registry::PR_CLOSED, muted),
        _ => (ExpIcon::GitPullRequest, theme::tokens::GREEN.to_hsla()),
    };
    let on_click = spec.on_click;
    // EXP-642: one glass row CARD per PR (web parity) — selected wears the
    // active fill, hover half of it.
    crate::surface::flat_row()
        .id(spec.id)
        .flex()
        .flex_row()
        // EXP-698: the trailing Merge cluster is CENTRED against the whole
        // card, not pinned to its first line — a two-line row (branch
        // sub-line, error caption) otherwise reads with the pill floating
        // at the top edge. The text column stacks; the action sits beside
        // it in its own vertically centered slot.
        .items_center()
        .w_full()
        .min_w_0()
        .px_3()
        .py_2p5()
        .gap_2()
        .when(spec.selected, |this| this.bg(row_active))
        .hover(move |this| this.bg(row_hover))
        .cursor_pointer()
        .on_click(move |event, window, cx| on_click(event, window, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_1p5()
                        .child(Icon::from(glyph).xsmall().flex_shrink_0().text_color(tint))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .font_family(theme::terminal::FONT_FAMILY)
                                .child(SharedString::from(spec.identifier)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .truncate()
                                .text_color(fg)
                                .child(SharedString::from(spec.title)),
                        )
                        .children(spec.batch_glyph),
                )
                .when_some(spec.sub, |this, branch| {
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
                .when_some(spec.error, |this, message| {
                    // EXP-706: message only — the recovery button moved
                    // into the Merge slot beside the column.
                    this.child(
                        div()
                            .pl_5()
                            .text_xs()
                            .truncate()
                            .text_color(danger)
                            .child(message),
                    )
                }),
        )
        .child(spec.trailing)
        .into_any_element()
}

/// The Reviews row's resting merge label (web `ReviewRow`, iOS, Android).
pub(crate) const MERGE_LABEL: &str = "Merge";

/// The covered-issues popover's title on a multi-issue run's header (iOS and
/// Android "Issues in this run").
pub(crate) const COVERED_ISSUES_TITLE: &str = "Issues in this run";

/// A popover's width — issue rows, not a graph.
const ISSUES_POPOVER_W: f32 = 360.;

/// THE issue-list popover: `trigger` opens `issues` as the relations card's
/// issue rows (a click opens the issue), under an optional muted `title`.
/// The Reviews batch glyph and a multi-issue run's header title share it.
pub(crate) fn issues_popover<T>(
    id: SharedString,
    trigger: T,
    title: Option<&'static str>,
    issues: Vec<domain::rows::Issue>,
) -> gpui::AnyElement
where
    T: gpui_component::Selectable + IntoElement + 'static,
{
    gpui_component::popover::Popover::new(id)
        .p_1()
        .trigger(trigger)
        .content(move |_, _window, cx| {
            let muted = cx.theme().muted_foreground;
            let rows: Vec<gpui::AnyElement> = issues
                .iter()
                .map(|issue| {
                    crate::issue_relations::issue_row(
                        &format!("issues-popover-{}", issue.id),
                        issue,
                        crate::issue_relations::IssueRowOpts {
                            title: None,
                            open: true,
                            remove: None,
                            guides: None,
                            in_dialog: false,
                        },
                        cx,
                    )
                })
                .collect();
            v_flex()
                .w(gpui::px(ISSUES_POPOVER_W))
                .min_w_0()
                .children(title.map(|title| {
                    div()
                        .px_3()
                        .pt_1p5()
                        .pb_1()
                        .text_xs()
                        .text_color(muted)
                        .child(title)
                }))
                .children(rows)
        })
        .into_any_element()
}

/// The Reviews list's batch glyph: the `pr-batch` concept with the batch's
/// issues in [`issues_popover`].
pub(crate) fn batch_glyph(
    id: SharedString,
    issues: Vec<domain::rows::Issue>,
    cx: &gpui::App,
) -> gpui::AnyElement {
    use crate::surface::{glass_pill_button, PillSize};
    let count = issues.len();
    let muted = cx.theme().muted_foreground;
    let trigger = glass_pill_button(id.clone(), PillSize::Sm, cx)
        .icon(
            Icon::new(registry::PR_BATCH)
                .with_size(gpui::px(PillSize::Sm.glyph()))
                .text_color(muted),
        )
        .label(SharedString::from(format!("{count} issues")))
        .tooltip(SharedString::from(format!(
            "This pull request closes {count} issues"
        )));
    issues_popover(SharedString::from(format!("{id}-batch")), trigger, None, issues)
}
