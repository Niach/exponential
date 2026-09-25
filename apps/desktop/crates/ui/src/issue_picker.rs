//! EXP-825 — the Agent page composer's issue picker: the team's OPEN issues
//! as a searchable checklist in a popover (the `#` tool), one checked issue =
//! Start coding, two or more = a batch. Ported out of the deleted
//! start-coding dialog: the pool rules (EXP-119: `done`/`cancelled`/
//! `duplicate`/PR-merged rows hidden, pre-seeded ids exempt and force-checked),
//! the per-run cap and the row anatomy (EXP-768: priority · identifier ·
//! status · title) are unchanged.
//!
//! EXP-892: searching runs through the ONE engine (`domain::issue_search`).
//!
//! EXP-1030: the POPOVER is gone from here — the `#` tool mounts
//! [`crate::picker::Picker`] like every other IDE pick, which owns the
//! surface, the filter field, the keyboard cursor and the selection mark
//! (a multi row's own highlight, never a circle). What stays is the DATA
//! half the primitive cannot know: the pool ([`snapshot_rows`]), the ranking
//! ([`visible_rows`], handed over as the primitive's `rank`), its memo, the
//! row anatomy ([`issue_row_body`], its `render_item`) and the overflow
//! notes ([`list_note`], its `footer`).

use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{div, px, AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _};
use gpui_component::{h_flex, ActiveTheme as _, Sizable as _};
use sync::Store;

use domain::options::get_issue_priority_config;
use domain::{IssuePriority, IssueStatus};

use crate::icons::{option_icon, resolved_status_icon};

/// Hard cap per run: every checked issue adds a prompt section, and a batch
/// beyond this size stops being one coherent session anyway.
pub(crate) const MAX_ISSUES_PER_RUN: usize = 30;

/// Unchecked search matches rendered at once — a team can hold hundreds
/// of issues, and the checklist is a plain (non-virtual) list.
pub(crate) const MAX_UNCHECKED_ROWS: usize = 50;

/// EXP-946 — the gap a composer popover keeps to the window edge (web's
/// `collisionPadding={12}`).
const POPOVER_GUTTER: f32 = 12.;

/// EXP-946 — the tallest a composer popover wants to be, room permitting.
pub(crate) const POPOVER_WANTED_HEIGHT: f32 = 400.;

/// EXP-946 — a popover never shrinks below this, however tight the window:
/// under it the list stops being a list, and the window edge clips it
/// anyway.
const POPOVER_MIN_HEIGHT: f32 = 140.;

/// EXP-946 — which side a composer popover opens on, and how tall it may be.
///
/// gpui-component's `Popover` anchors a CORNER and only CLAMPS — it never
/// flips and never caps — so a list taller than the room on its side ran
/// straight off the window. The composer sits low, so a picker prefers to
/// open ABOVE its trigger; with no room there it opens BELOW, and either way
/// it is capped to the room that side actually has. The ×4 rule (web's
/// `collisionPadding` + `--radix-popover-content-available-height`).
///
/// `Anchor::BottomLeft` hangs the popup a trigger-height above the trigger's
/// top and `Anchor::TopLeft` drops it from that same top
/// (`Popup::resolved_corner`), which is what the two rooms are measured
/// against.
pub(crate) fn popover_fit(
    trigger: gpui::Bounds<gpui::Pixels>,
    viewport: gpui::Size<gpui::Pixels>,
    wanted: f32,
) -> (gpui::Anchor, gpui::Pixels) {
    let gutter = px(POPOVER_GUTTER);
    let above = (trigger.top() - trigger.size.height - gutter).max(px(0.));
    let below = (viewport.height - trigger.top() - gutter).max(px(0.));
    // A trigger that has never painted reports an empty rect at the origin;
    // opening DOWN from there is the only guess that cannot be wrong.
    let unmeasured = trigger.size.height <= px(0.);
    let open_above = !unmeasured && (px(wanted) <= above || above >= below);
    let room = if open_above { above } else { below };
    let anchor = if open_above {
        gpui::Anchor::BottomLeft
    } else {
        gpui::Anchor::TopLeft
    };
    (anchor, px(wanted).min(room).max(px(POPOVER_MIN_HEIGHT)))
}

/// One checklist row, snapshotted from the sync store (titles and
/// descriptions ride into the launch request verbatim — the launcher never
/// re-reads the collections).
#[derive(Clone)]
pub(crate) struct IssueRow {
    pub(crate) issue_id: String,
    /// The issue's board — EXP-712: its `default_branch` is the base every
    /// launch for this issue cuts from, and a batch may only mix boards that
    /// agree on it.
    pub(crate) board_id: String,
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    /// Status snapshot — the launcher's step 6.5 flips backlog to
    /// `in_progress` at launch (EXP-194).
    pub(crate) status: IssueStatus,
    pub(crate) priority: IssuePriority,
    pub(crate) resolved: domain::statuses::ResolvedStatus,
    /// The closed-state note (`done`/`cancelled`/`duplicate`/PR merged),
    /// shown muted next to the title. Only pre-seeded rows can carry one
    /// (the pool hides closed rows) — it flags a re-run. `None` = plain row.
    pub(crate) state_hint: Option<&'static str>,
    /// EXP-892: the search engine's recency keys (`domain::issue_search`
    /// ranks and tie-breaks on them, and lists the newest work for an empty
    /// query).
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

/// Whether an issue is closed for the pool's purposes (EXP-119). EXP-314:
/// ANCHOR-based on purpose — every closed-ish CATEGORY writes one of these
/// three anchors, so a custom completed / cancelled status is classified
/// correctly with no status-row join (this picker spans boards).
pub(crate) fn is_closed(status: IssueStatus, pr_state: Option<&str>) -> bool {
    matches!(
        status,
        IssueStatus::Done | IssueStatus::Cancelled | IssueStatus::Duplicate
    ) || pr_state == Some("merged")
}

/// The closed-state hint a pre-seeded row shows.
pub(crate) fn state_hint(status: IssueStatus, pr_state: Option<&str>) -> Option<&'static str> {
    if pr_state == Some("merged") {
        return Some("PR merged");
    }
    match status {
        IssueStatus::Done => Some("done"),
        IssueStatus::Cancelled => Some("cancelled"),
        IssueStatus::Duplicate => Some("duplicate"),
        _ => None,
    }
}

/// Snapshot the picker's pool for `team_id`: every OPEN team issue, board→
/// number ordered, plus the `preselected` ids whatever their state (the
/// explicit pick wins — the Play-button re-run of a done issue — and a
/// checked id MUST keep its row, or the run would silently drop it).
/// `team_issues` joins through the boards collection, but a play button
/// resolves its seed from the raw issues collection — a seed the join
/// dropped (a board row that hasn't synced yet) is re-read from there.
pub(crate) fn snapshot_rows(cx: &App, team_id: &str, preselected: &HashSet<String>) -> Vec<IssueRow> {
    let collections = Store::global(cx).collections();
    // EXP-868: filter on BORROWED rows and resolve against the team's status
    // rows read once — cloning every team issue (closed ones and their
    // descriptions included) and re-scanning the statuses per row made this
    // the costliest thing on screen.
    let mut issues = collections.issue_refs_in_team(team_id, cx);
    // A seed the team join dropped resolves on its own (its board is unknown
    // here, so the team's rows are not a safe assumption).
    let joined = issues.len();
    for seed in preselected {
        if !issues.iter().any(|issue| &issue.id == seed) {
            if let Some(issue) = collections.issues.read(cx).get(seed) {
                issues.push(issue);
            }
        }
    }
    let statuses = crate::queries::team_statuses(cx, team_id);
    let mut rows: Vec<(&domain::rows::Issue, bool)> = issues
        .into_iter()
        .enumerate()
        .map(|(ix, issue)| (issue, ix < joined))
        .filter(|(issue, _)| {
            preselected.contains(&issue.id) || !is_closed(issue.status, issue.pr_state.as_deref())
        })
        .collect();
    rows.sort_by(|(a, _), (b, _)| {
        a.board_id
            .cmp(&b.board_id)
            .then_with(|| a.number.cmp(&b.number))
    });
    rows.into_iter()
        .map(|(issue, in_team)| IssueRow {
            state_hint: state_hint(issue.status, issue.pr_state.as_deref()),
            issue_id: issue.id.clone(),
            board_id: issue.board_id.clone(),
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            created_at: issue.created_at.clone(),
            updated_at: issue.updated_at.clone(),
            status: issue.status,
            priority: issue.priority,
            resolved: if in_team {
                domain::statuses::resolve_status_sorted(issue, &statuses)
            } else {
                crate::queries::resolve_issue_status(cx, issue)
            },
        })
        .collect()
}

/// The list order for `query`: checked rows pinned first (in pool order),
/// then the unchecked matches RANKED by the ONE issue-search engine
/// (EXP-892, `domain::issue_search` — identifier over title over description,
/// ties by recency, an empty query listing the newest work) and capped at
/// [`MAX_UNCHECKED_ROWS`]. Returns `(row indices, hidden count, no-matches)`.
pub(crate) fn visible_rows(
    rows: &[IssueRow],
    checked: &HashSet<String>,
    query: &str,
) -> (Vec<usize>, usize, bool) {
    let mut checked_ixs: Vec<usize> = Vec::new();
    for (ix, row) in rows.iter().enumerate() {
        if checked.contains(&row.issue_id) {
            checked_ixs.push(ix);
        }
    }
    // The checked rows are pinned, so they leave the ranked pool entirely.
    let views: Vec<domain::issue_search::SearchRow<'_>> = rows.iter().map(engine_row).collect();
    let mut match_ixs =
        domain::issue_search::rank(&views, query, usize::MAX, checked);
    let hidden = match_ixs.len().saturating_sub(MAX_UNCHECKED_ROWS);
    let no_matches =
        !domain::issue_search::tokens(query).is_empty() && match_ixs.is_empty() && !rows.is_empty();
    match_ixs.truncate(MAX_UNCHECKED_ROWS);
    checked_ixs.extend(match_ixs);
    (checked_ixs, hidden, no_matches)
}

/// Release review R5: [`visible_rows`] memoised by the HOST. The popover's
/// content runs on every render of a composer that repaints at 60 fps while
/// a hosted run is busy (the live dot), and ranking the whole open-issue
/// pool each frame was measurable. Keyed on the pool's identity (the `Rc`
/// pointer — a rebuilt pool is a new key), the query and the checked set;
/// nothing else feeds the ranking.
#[derive(Default)]
pub(crate) struct VisibleRowsMemo {
    key: Option<(usize, String, HashSet<String>)>,
    value: (Vec<usize>, usize, bool),
    /// How many times the ranking actually ran (the tests read it).
    computed: usize,
}

impl VisibleRowsMemo {
    pub(crate) fn get(
        &mut self,
        rows: &Rc<Vec<IssueRow>>,
        checked: &HashSet<String>,
        query: &str,
    ) -> (Vec<usize>, usize, bool) {
        let pool = Rc::as_ptr(rows) as usize;
        let hit = self
            .key
            .as_ref()
            .is_some_and(|(seen_pool, seen_query, seen_checked)| {
                *seen_pool == pool && seen_query == query && seen_checked == checked
            });
        if !hit {
            self.value = visible_rows(rows, checked, query);
            self.key = Some((pool, query.to_string(), checked.clone()));
            self.computed += 1;
        }
        self.value.clone()
    }

    #[cfg(test)]
    fn computed(&self) -> usize {
        self.computed
    }
}

/// The engine's borrowed view of a checklist row.
fn engine_row(row: &IssueRow) -> domain::issue_search::SearchRow<'_> {
    domain::issue_search::SearchRow {
        id: &row.issue_id,
        identifier: &row.identifier,
        title: &row.title,
        description: row.description.as_deref(),
        created_at: row.created_at.as_deref(),
        updated_at: row.updated_at.as_deref(),
        // EXP-922: undone rows rank above done ones. The pool already hides
        // closed issues, so this only orders the PRE-SEEDED re-run rows.
        status: row.status.as_wire(),
    }
}

/// One muted NOTE row under a picker's list — the overflow / no-match copy
/// the rows themselves cannot say, handed to the primitive as its `footer`
/// (EXP-1030). The compact picker rung, because it sits between
/// `pickers::picker_row`s inside the picker surface.
pub(crate) fn list_note(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    div()
        .w_full()
        .px_2()
        .py_1p5()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// The `#` picker's rows for the primitive: ONE item per pool row, in POOL
/// order. [`visible_rows`] does the ranking (the primitive's `rank` hook), so
/// the indices it hands back index straight into this list.
///
/// The label is only the SEARCH keyword set — what a row draws is
/// [`issue_row_body`], which the composer hands over as `render_item`.
pub(crate) fn picker_items(rows: &[IssueRow]) -> Vec<crate::picker::PickerItem<String>> {
    rows.iter()
        .map(|row| {
            crate::picker::PickerItem::new(
                row.issue_id.clone(),
                format!("{} {}", row.identifier, row.title),
            )
            .keywords(vec![row.identifier.clone().into(), row.title.clone().into()])
        })
        .collect()
}

/// One checklist row's BODY (EXP-768, the mobile anatomy on every client):
/// priority · identifier · status · title (+ state hint or `note`).
///
/// The selection mark is NOT here and never comes back: EXP-1021 made a multi
/// row's own highlight its mark, and the row shell, the hover and the toggle
/// all belong to [`crate::picker::Picker`].
pub(crate) fn issue_row_body(
    row: &IssueRow,
    note: Option<SharedString>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let priority = get_issue_priority_config(row.priority);
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2p5()
        .child(option_icon(priority, cx).xsmall())
        .child(
            div()
                .flex_shrink_0()
                .min_w(px(60.))
                .text_xs()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(row.identifier.clone())),
        )
        .child(resolved_status_icon(&row.resolved, cx).xsmall())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .text_color(theme.foreground)
                .child(SharedString::from(row.title.clone())),
        )
        .when_some(
            note.or_else(|| row.state_hint.map(SharedString::from)),
            |this, note| {
                this.child(div().flex_shrink_0().text_xs().text_color(muted).child(note))
            },
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-946 — the picker opens ABOVE its trigger when that side has room,
    /// BELOW when it does not, and is capped to whichever side it took, so it
    /// can never run off the window.
    #[test]
    fn a_composer_popover_takes_the_side_with_room_and_caps_itself_to_it() {
        let trigger = |top: f32| gpui::Bounds {
            origin: gpui::Point::new(px(40.), px(top)),
            size: gpui::Size::new(px(32.), px(32.)),
        };
        let viewport = gpui::Size::new(px(1200.), px(800.));
        // A composer low in the window: plenty above, so the picker opens
        // there at its full height.
        let (anchor, height) = popover_fit(trigger(700.), viewport, POPOVER_WANTED_HEIGHT);
        assert_eq!(anchor, gpui::Anchor::BottomLeft);
        assert_eq!(height, px(POPOVER_WANTED_HEIGHT));
        // A trigger near the TOP has no room above and flips BELOW — this is
        // the case that used to run off the top of the window.
        let (anchor, height) = popover_fit(trigger(80.), viewport, POPOVER_WANTED_HEIGHT);
        assert_eq!(anchor, gpui::Anchor::TopLeft);
        assert_eq!(height, px(POPOVER_WANTED_HEIGHT));
        // Too little room above for the full height, but MORE than below: it
        // stays above and shrinks to what that side has (300 - 32 - 12).
        let (anchor, height) = popover_fit(
            trigger(300.),
            gpui::Size::new(px(1200.), px(500.)),
            POPOVER_WANTED_HEIGHT,
        );
        assert_eq!(anchor, gpui::Anchor::BottomLeft);
        assert_eq!(height, px(256.));
        // A short window: neither side fits, and the floor keeps the list a
        // list.
        let (_, height) = popover_fit(
            trigger(120.),
            gpui::Size::new(px(1200.), px(200.)),
            POPOVER_WANTED_HEIGHT,
        );
        assert_eq!(height, px(140.));
        // A trigger that has never painted opens DOWN — the only guess that
        // cannot put the panel off-screen.
        let (anchor, _) = popover_fit(gpui::Bounds::default(), viewport, POPOVER_WANTED_HEIGHT);
        assert_eq!(anchor, gpui::Anchor::TopLeft);
    }

    fn row(id: &str, identifier: &str, title: &str) -> IssueRow {
        dated(id, identifier, title, None)
    }

    fn dated(id: &str, identifier: &str, title: &str, created_at: Option<&str>) -> IssueRow {
        IssueRow {
            issue_id: id.to_string(),
            board_id: "b".to_string(),
            identifier: identifier.to_string(),
            title: title.to_string(),
            description: None,
            created_at: created_at.map(str::to_string),
            updated_at: None,
            status: IssueStatus::Backlog,
            priority: IssuePriority::None,
            resolved: domain::statuses::constructed_default(IssueStatus::Backlog),
            state_hint: None,
        }
    }

    /// EXP-119: closed rows leave the pool by anchor OR merged PR; the hint
    /// names which.
    #[test]
    fn closed_rows_and_their_hints() {
        assert!(is_closed(IssueStatus::Done, None));
        assert!(is_closed(IssueStatus::Cancelled, None));
        assert!(is_closed(IssueStatus::Duplicate, None));
        assert!(is_closed(IssueStatus::Backlog, Some("merged")));
        assert!(!is_closed(IssueStatus::InProgress, Some("open")));
        assert_eq!(state_hint(IssueStatus::Backlog, Some("merged")), Some("PR merged"));
        assert_eq!(state_hint(IssueStatus::Done, None), Some("done"));
        assert_eq!(state_hint(IssueStatus::InReview, Some("open")), None);
    }

    /// Release review R5: the memo ranks once per (pool, query, checked) and
    /// hands back the same answer `visible_rows` would; a new pool, a new
    /// query or a changed checked set each rank again.
    #[test]
    fn the_visible_rows_memo_ranks_once_per_key() {
        let rows: Rc<Vec<IssueRow>> = Rc::new(
            (0..5)
                .map(|n| row(&format!("i{n}"), &format!("EXP-{n}"), &format!("Title {n}")))
                .collect(),
        );
        let mut checked: HashSet<String> = HashSet::new();
        let mut memo = VisibleRowsMemo::default();
        assert_eq!(memo.get(&rows, &checked, ""), visible_rows(&rows, &checked, ""));
        memo.get(&rows, &checked, "");
        memo.get(&rows, &checked, "");
        assert_eq!(memo.computed(), 1, "the same key never ranks twice");
        assert_eq!(memo.get(&rows, &checked, "Title 3"), visible_rows(&rows, &checked, "Title 3"));
        assert_eq!(memo.computed(), 2, "a new query ranks");
        checked.insert("i2".to_string());
        assert_eq!(memo.get(&rows, &checked, "Title 3"), visible_rows(&rows, &checked, "Title 3"));
        assert_eq!(memo.computed(), 3, "a changed checked set ranks");
        let rebuilt: Rc<Vec<IssueRow>> = Rc::new(rows.as_ref().clone());
        memo.get(&rebuilt, &checked, "Title 3");
        assert_eq!(memo.computed(), 4, "a rebuilt pool is a new key");
        memo.get(&rebuilt, &checked, "Title 3");
        assert_eq!(memo.computed(), 4);
    }

    /// Checked rows pin first whatever the query; the rest is the shared
    /// engine's ranking, capped at [`MAX_UNCHECKED_ROWS`].
    #[test]
    fn visible_rows_pin_checked_first_and_cap_matches() {
        let mut rows: Vec<IssueRow> = (0..60)
            .map(|n| row(&format!("i{n}"), &format!("EXP-{n}"), &format!("Title {n}")))
            .collect();
        rows.push(row("special", "EXP-999", "Login flicker"));
        let checked: HashSet<String> = ["i5".to_string()].into_iter().collect();
        let (visible, hidden, no_matches) = visible_rows(&rows, &checked, "");
        assert_eq!(visible[0], 5, "the checked row leads");
        assert_eq!(visible.len(), 1 + MAX_UNCHECKED_ROWS);
        assert_eq!(hidden, 60 - MAX_UNCHECKED_ROWS);
        assert!(!no_matches);
        // Undated rows tie on recency, so the identifier number orders them:
        // EXP-999 first, then EXP-59 down.
        assert_eq!(visible[1], 60, "the highest identifier number leads");

        let (visible, hidden, no_matches) = visible_rows(&rows, &checked, "FLICKER");
        assert_eq!(visible, vec![5, 60]);
        assert_eq!(hidden, 0);
        assert!(!no_matches);

        let (visible, _, no_matches) = visible_rows(&rows, &checked, "nothing here");
        assert_eq!(visible, vec![5]);
        assert!(no_matches);
    }

    /// EXP-1030: the primitive's rows are 1:1 with the POOL, in pool order —
    /// that is what lets [`visible_rows`]' indices be handed straight to
    /// `Picker::rank`. A row's label is only its search keyword set (the body
    /// is [`issue_row_body`]), and both the identifier and the title match.
    #[test]
    fn the_picker_items_mirror_the_pool_one_for_one() {
        let rows = vec![
            row("a", "EXP-870", "Login flicker"),
            row("b", "EXP-12", "Login timeout"),
        ];
        let items = picker_items(&rows);
        assert_eq!(items.len(), rows.len());
        let values: Vec<&str> = items.iter().map(|item| item.value.as_str()).collect();
        assert_eq!(values, vec!["a", "b"], "pool order, so an index means the same row");
        assert_eq!(items[0].label, SharedString::from("EXP-870 Login flicker"));
        assert_eq!(
            items[0].keywords,
            vec![
                SharedString::from("EXP-870"),
                SharedString::from("Login flicker")
            ]
        );
        // A row carries no glyph and no description of its own: the composer
        // draws the whole body itself (`render_item`).
        assert!(items[0].icon.is_none());
        assert!(items[0].description.is_none());
        assert!(!items[0].disabled);

        // …and the ranking indexes straight into them.
        let picked: HashSet<String> = HashSet::new();
        let (visible, _, _) = visible_rows(&rows, &picked, "flicker");
        assert_eq!(visible, vec![0]);
        assert_eq!(items[visible[0]].value, "a");
    }

    /// EXP-892: the picker ranks through `domain::issue_search` — an exact
    /// identifier beats a title hit, and an empty query lists the newest
    /// created work first.
    #[test]
    fn visible_rows_rank_through_the_shared_engine() {
        let rows = vec![
            dated("a", "EXP-870", "Login flicker", Some("2026-03-01T00:00:00Z")),
            dated("b", "EXP-87", "Unrelated", Some("2026-01-01T00:00:00Z")),
            dated("c", "EXP-12", "Login timeout", Some("2026-02-01T00:00:00Z")),
        ];
        let none = HashSet::new();
        // `#87` — exact number, then the number prefix; the title rows drop.
        assert_eq!(visible_rows(&rows, &none, "#87").0, vec![1, 0]);
        // Newest created first for an empty query.
        assert_eq!(visible_rows(&rows, &none, "").0, vec![0, 2, 1]);
        // Descriptions and titles are AND-matched across tokens.
        assert_eq!(visible_rows(&rows, &none, "login time").0, vec![2]);
    }
}
