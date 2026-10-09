//! The Reviews center screen (EXP-706): open pull requests across the team.
//!
//! A tab-less FULL-PAGE screen like Devices / Actions: one centered column
//! (the web route's `max-w-3xl`), no page title (the first band is the
//! page's first line, exactly as on web).
//!
//! EXP-1248: every row is ONE line, [`crate::pr_rows::pr_row`] (web
//! `PrRow`): ring lead · mono identifier · title. Per board band (no count):
//! a PR TREE nests with tree guides, a linear STACK hangs off its rail down
//! to the base-branch row (`stack` on its top row), singles stay flat — the
//! `domain::reviews_queue` rule-9 items. NO inline Merge, no two-click
//! confirm: merging lives on the issue's Guide, which a row opens. An "Agent
//! runs" row opens the run's Guide; a PR nothing links opens on GitHub.
//! EXP-1246: the PR opens FULL WIDTH (no Reviews second sidebar); Back
//! returns here through history.
//!
//! Issue-linked PRs come from the synced issues shape; the unlinked ones from
//! the app-wide `repositories.openPulls` store, so the synced lists never
//! wait on GitHub.

use gpui::{
    div, App, ClickEvent, Entity, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::list_item::PrNodeState;

use crate::actions_view::page_scaffold_with;
use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::pr_rows::{pr_list, pr_row, stack_rail, PrListRow, PrRowSpec, StackRailMember, StackRailSpec};
use crate::queries;

/// The page column's cap — the web reviews route's `max-w-3xl`. Narrower than
/// [`crate::actions_view::page_scaffold`]'s default: these rows are short, and
/// a 1024px line of PR titles reads as a table, not a queue.
const REVIEWS_COLUMN_W: f32 = 768.;

/// The quiet word on a stack's top row ×4 (web `word="stack"`).
pub(crate) const STACK_WORD: &str = "stack";

pub struct ReviewsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// The team the Reviews page last force-refreshed the app-wide openPulls
    /// store ([`crate::open_pulls::OpenPulls`], EXP-1244) for. Cleared by
    /// [`Self::mark_pulls_stale`] whenever the screen is (re-)entered, so a
    /// return refetches (the server caches ~60s; there is deliberately no
    /// polling).
    open_pulls_key: Option<String>,
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
        // EXP-1244: the unlinked pulls live in the app-wide store the rail's
        // Reviews dot reads too.
        let open_pulls = crate::open_pulls::OpenPulls::global(cx);
        subscriptions.push(cx.observe(&open_pulls, |_, _, cx| cx.notify()));

        Self {
            nav,
            scroll: ScrollHandle::new(),
            open_pulls_key: None,
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

    /// Force-refresh the team's entry in the app-wide openPulls store when
    /// the screen is entered or the team changes — never on a timer (the
    /// server caches ~60s). The store keeps rendering the previous result
    /// while the refresh is in flight.
    fn ensure_open_pulls(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.open_pulls_key.as_deref() == Some(team_id) {
            return;
        }
        self.open_pulls_key = Some(team_id.to_string());
        crate::open_pulls::OpenPulls::refresh(team_id, true, cx);
    }

    // -- rows ----------------------------------------------------------------

    /// One board band's rows (`domain::reviews_queue` rule 9): consecutive
    /// tree/single items share ONE [`pr_list`] (the guides span them), each
    /// stack is a [`stack_rail`].
    fn board_rows(&self, group: &queries::ReviewGroup, cx: &mut gpui::Context<Self>) -> Vec<gpui::AnyElement> {
        let mut out: Vec<gpui::AnyElement> = Vec::new();
        let mut run: Vec<PrListRow> = Vec::new();
        let prefix = format!("review-{}", group.board.id);
        let flush = |run: &mut Vec<PrListRow>, out: &mut Vec<gpui::AnyElement>, cx: &App| {
            if !run.is_empty() {
                out.push(pr_list(&prefix, std::mem::take(run), cx));
            }
        };
        for (index, item) in group.items.iter().enumerate() {
            match item {
                queries::ReviewItem::Pr { entry, depth } => {
                    let (identifier, title) = entry_texts(entry);
                    run.push(PrListRow {
                        key: entry.representative().id.clone(),
                        identifier: Some(identifier.into()),
                        title: title.into(),
                        depth: *depth,
                        node: PrNodeState::Open,
                        word: None,
                        active: false,
                        on_open: Some(open_issue_guide(&entry.representative().id)),
                        trailing: None,
                    });
                }
                queries::ReviewItem::Stack {
                    entries,
                    base_branch,
                } => {
                    flush(&mut run, &mut out, cx);
                    let members = entries
                        .iter()
                        .map(|entry| {
                            let (identifier, title) = entry_texts(entry);
                            StackRailMember {
                                key: entry.representative().id.clone(),
                                identifier: identifier.into(),
                                title: title.into(),
                                current: false,
                                on_open: Some(open_issue_guide(&entry.representative().id)),
                                trailing: None,
                            }
                        })
                        .collect();
                    out.push(stack_rail(
                        StackRailSpec {
                            id_prefix: SharedString::from(format!("{prefix}-stack-{index}")),
                            members,
                            base_branch: SharedString::from(
                                base_branch
                                    .clone()
                                    .or_else(|| group.board.default_branch.clone())
                                    .unwrap_or_else(|| {
                                        crate::session_results::STACK_BASE_FALLBACK.to_string()
                                    }),
                            ),
                            word: Some(SharedString::from(STACK_WORD)),
                            hovered: None,
                            on_hover: None,
                            on_merge_through: None,
                        },
                        cx,
                    ));
                }
            }
        }
        flush(&mut run, &mut out, cx);
        out
    }

    /// EXP-734 — one "Agent runs" row: an action or chat run holding a pull
    /// request of its OWN. EXP-1194: it opens the RUN on its Guide (the PR's
    /// files come from `codingSessions.prFiles` when the run published no
    /// live diff).
    fn run_row(&self, run: &domain::rows::CodingSession, cx: &App) -> gpui::AnyElement {
        let number = run.pr_number.map(|number| format!("#{number}"));
        // A chat run has no action behind it — web/mobile label it with the
        // agent's auto-named title, else "Chat" (EXP-908).
        let title = domain::batch_run::action_run_subject(run)
            .unwrap_or_else(|| domain::batch_run::CHAT_RUN_NAME.to_string());
        let run_id = run.id.clone();
        pr_row(
            PrRowSpec {
                identifier: number.map(SharedString::from),
                on_click: Some(Box::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    crate::session_screen::open_session(&run_id, window, cx);
                    crate::screens::set_run_face(&run_id, crate::screens::RunFace::Guide, window, cx);
                })),
                ..PrRowSpec::open(format!("review-run-{}", run.id), title)
            },
            cx,
        )
    }

    /// One unlinked-PR row: `#N` + title (`draft` as its quiet word), a muted
    /// external-link glyph trailing. It opens the PR on GitHub — no local
    /// detail exists behind these.
    fn pull_row(&self, pull: &api::repositories::OpenPull, cx: &App) -> gpui::AnyElement {
        let url = pull.url.clone();
        let muted = cx.theme().muted_foreground;
        pr_row(
            PrRowSpec {
                identifier: Some(SharedString::from(format!("#{}", pull.number))),
                word: pull.draft.then(|| SharedString::from("draft")),
                trailing: Some(
                    Icon::new(registry::UI_EXTERNAL_LINK)
                        .xsmall()
                        .text_color(muted)
                        .into_any_element(),
                ),
                on_click: Some(Box::new(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    crate::settings::open_url(cx, url.clone());
                })),
                ..PrRowSpec::open(format!("review-pull-{}", pull.url), pull.title.clone())
            },
            cx,
        )
    }
}

/// A row opening `issue_id` on its Guide (the review of a PR IS the issue's
/// work screen, EXP-1154), full width (EXP-1246: no list beside it).
fn open_issue_guide(issue_id: &str) -> crate::run_rows::RunRowAction {
    let issue_id = issue_id.to_string();
    Box::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
        crate::screens::open_issue_changes(&issue_id, None, window, cx);
    })
}

/// A Reviews entry's mono identifier + title: the issue's, a batch PR's
/// `EXP-874 +2` (×4, the stack labels' rule) over its representative's title.
pub(crate) fn entry_texts(entry: &queries::ReviewEntry) -> (String, String) {
    let issue = entry.representative();
    let identifier = match entry.issues.len() {
        0 | 1 => issue.identifier.clone(),
        n => format!("{} +{}", issue.identifier, n - 1),
    };
    (identifier, issue.title.clone())
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
        // EXP-1244: ONE shared queue ×4 (`domain::reviews_queue`): the board
        // groups, the runs holding a PR of their own (EXP-734) and the
        // fetched pulls no synced issue or run links.
        let open_pulls = crate::open_pulls::OpenPulls::global(cx);
        let queries::ReviewsQueue {
            groups,
            runs,
            pull_repos,
            count,
        } = team_id
            .as_deref()
            .map(|id| queries::reviews_queue(cx, id, open_pulls.read(cx).repos(id)))
            .unwrap_or(queries::ReviewsQueue {
                groups: Vec::new(),
                runs: Vec::new(),
                pull_repos: Vec::new(),
                count: 0,
            });

        let muted = cx.theme().muted_foreground;
        let heading_fg = cx.theme().foreground;
        let caption = |text: &'static str| {
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted.opacity(0.8))
                .child(text)
                .into_any_element()
        };

        let column = if !is_ready {
            v_flex()
                .min_w_0()
                .gap_2()
                .child(crate::controls::skeleton().h_3p5().w_40())
                .child(crate::controls::skeleton().h_3p5().w_48())
                .child(crate::controls::skeleton().h_3p5().w_32())
        } else if count == 0 {
            // EXP-525: the web `EmptyState` (icon disc + title + description).
            v_flex().min_w_0().child(crate::controls::empty_state(
                Icon::from(ExpIcon::GitPullRequest),
                "No open pull requests",
                "Open pull requests in this team's repositories land here for review.",
                cx,
            ))
        } else {
            // EXP-818: one BAND per board (its glyph, NO count: EXP-1248),
            // the rows flush under it.
            let mut children: Vec<gpui::AnyElement> = Vec::new();
            for group in &groups {
                let band = crate::surface::glass_section_band(
                    Some(
                        crate::icons::board_icon(&group.board)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(heading_fg.opacity(0.7))
                            .into_any_element(),
                    ),
                    SharedString::from(group.board.name.clone()),
                    None,
                    cx,
                );
                let rows = self.board_rows(group, cx);
                children.push(v_flex().min_w_0().pb_2().child(band).children(rows).into_any_element());
            }
            // EXP-734: a run's own PR is the team's work, but it completes no
            // issue, so it gets its own band.
            if !runs.is_empty() {
                let band = crate::surface::glass_section_band(
                    Some(
                        Icon::new(registry::UI_AGENT_SOURCE)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(heading_fg.opacity(0.7))
                            .into_any_element(),
                    ),
                    "Agent runs",
                    Some(caption(domain::reviews_queue::RUN_BAND_CAPTION)),
                    cx,
                );
                let rows: Vec<gpui::AnyElement> = runs.iter().map(|run| self.run_row(run, cx)).collect();
                children.push(v_flex().min_w_0().pb_2().child(band).children(rows).into_any_element());
            }
            for repo in &pull_repos {
                let band = crate::surface::glass_section_band(
                    Some(
                        Icon::from(ExpIcon::GitPullRequest)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(heading_fg.opacity(0.7))
                            .into_any_element(),
                    ),
                    SharedString::from(repo.full_name.clone()),
                    Some(caption(domain::reviews_queue::REPO_BAND_CAPTION)),
                    cx,
                );
                let rows: Vec<gpui::AnyElement> =
                    repo.pulls.iter().map(|pull| self.pull_row(pull, cx)).collect();
                children.push(v_flex().min_w_0().pb_2().child(band).children(rows).into_any_element());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: &str, identifier: &str, title: &str) -> domain::rows::Issue {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "board_id": "b1",
            "number": 1,
            "identifier": identifier,
            "title": title,
            "status": "in_review",
        }))
        .unwrap()
    }

    /// EXP-1248: a row names its issue; a batch PR is ONE row named
    /// `EXP-874 +2` over its representative's title (no PR number).
    #[test]
    fn a_review_row_names_its_issue_or_batch() {
        let single = queries::ReviewEntry {
            issues: vec![issue("i1", "EXP-1", "Fix the sync loop")],
        };
        assert_eq!(
            entry_texts(&single),
            ("EXP-1".to_string(), "Fix the sync loop".to_string())
        );
        let batch = queries::ReviewEntry {
            issues: vec![
                issue("i2", "EXP-874", "Session list fixes"),
                issue("i3", "EXP-875", "B"),
                issue("i4", "EXP-876", "C"),
            ],
        };
        assert_eq!(
            entry_texts(&batch),
            ("EXP-874 +2".to_string(), "Session list fixes".to_string())
        );
        assert_eq!(STACK_WORD, "stack");
    }
}
