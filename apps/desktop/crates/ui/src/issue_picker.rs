//! EXP-825 — the Agent page composer's issue picker: the team's OPEN issues
//! as a searchable checklist in a popover (the `#` tool), one checked issue =
//! Start coding, two or more = a batch. Ported out of the deleted
//! start-coding dialog: the pool rules (EXP-119: `done`/`cancelled`/
//! `duplicate`/PR-merged rows hidden, pre-seeded ids exempt and force-checked),
//! the per-run cap and the row anatomy (EXP-768: selection glyph · priority ·
//! identifier · status · title) are unchanged.

use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window,
};
use gpui_component::button::Button;
use gpui_component::input::InputState;
use gpui_component::popover::Popover;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::options::get_issue_priority_config;
use domain::{IssuePriority, IssueStatus};

use crate::controls::glass_input;
use crate::icons::{option_icon, registry, resolved_status_icon};

/// Hard cap per run: every checked issue adds a prompt section, and a batch
/// beyond this size stops being one coherent session anyway.
pub(crate) const MAX_ISSUES_PER_RUN: usize = 30;

/// Unchecked search matches rendered at once — a team can hold hundreds
/// of issues, and the checklist is a plain (non-virtual) list.
pub(crate) const MAX_UNCHECKED_ROWS: usize = 50;

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
    let mut issues = crate::queries::team_issues(cx, team_id);
    for seed in preselected {
        if !issues.iter().any(|issue| &issue.id == seed) {
            if let Some(issue) = Store::global(cx).collections().issues.read(cx).get(seed) {
                issues.push(issue.clone());
            }
        }
    }
    issues.retain(|issue| {
        preselected.contains(&issue.id) || !is_closed(issue.status, issue.pr_state.as_deref())
    });
    issues.sort_by(|a, b| {
        a.board_id
            .cmp(&b.board_id)
            .then_with(|| a.number.cmp(&b.number))
    });
    issues
        .into_iter()
        .map(|issue| {
            let resolved = crate::queries::resolve_issue_status(cx, &issue);
            IssueRow {
                state_hint: state_hint(issue.status, issue.pr_state.as_deref()),
                issue_id: issue.id,
                board_id: issue.board_id,
                identifier: issue.identifier,
                title: issue.title,
                description: issue.description,
                status: issue.status,
                priority: issue.priority,
                resolved,
            }
        })
        .collect()
}

/// The list order for `query`: checked rows pinned first, then the
/// unchecked matches capped at [`MAX_UNCHECKED_ROWS`]. Returns
/// `(row indices, hidden count, no-matches)`.
pub(crate) fn visible_rows(
    rows: &[IssueRow],
    checked: &HashSet<String>,
    query: &str,
) -> (Vec<usize>, usize, bool) {
    let query = query.trim().to_lowercase();
    let mut checked_ixs: Vec<usize> = Vec::new();
    let mut match_ixs: Vec<usize> = Vec::new();
    for (ix, row) in rows.iter().enumerate() {
        if checked.contains(&row.issue_id) {
            checked_ixs.push(ix);
        } else if query.is_empty()
            || row.identifier.to_lowercase().contains(&query)
            || row.title.to_lowercase().contains(&query)
        {
            match_ixs.push(ix);
        }
    }
    let hidden = match_ixs.len().saturating_sub(MAX_UNCHECKED_ROWS);
    let no_matches = !query.is_empty() && match_ixs.is_empty() && !rows.is_empty();
    match_ixs.truncate(MAX_UNCHECKED_ROWS);
    checked_ixs.extend(match_ixs);
    (checked_ixs, hidden, no_matches)
}

/// The search row heading the picker (EXP-768): the search glyph leading, a
/// chrome-less field filling the row.
fn search_row(state: &Entity<InputState>, window: &Window, cx: &App) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    crate::surface::glass_row_shell()
        .child(Icon::new(registry::NAV_SEARCH).small().text_color(muted))
        .child(
            div().flex_1().min_w_0().child(
                glass_input(state, window, cx)
                    .appearance(false)
                    .h_auto()
                    .px_0()
                    .py_0()
                    .cleanable(true),
            ),
        )
}

/// One muted, hairline-divided NOTE row (empty / no-match / overflow copy).
pub(crate) fn list_note(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    crate::surface::glass_row_divider(
        div()
            .px_4()
            .py_3()
            .text_sm()
            .text_color(cx.theme().foreground.opacity(0.7))
            .child(text.into()),
    )
}

/// One checklist row (EXP-768, the mobile anatomy on every client):
/// selection glyph · priority · identifier · status · title (+ state hint or
/// `note`). The whole row toggles through `toggle`.
fn issue_row<V: Render>(
    row: &IssueRow,
    is_checked: bool,
    note: Option<SharedString>,
    view: &WeakEntity<V>,
    toggle: fn(&mut V, String, bool, &mut Window, &mut gpui::Context<V>),
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let toggle_id = row.issue_id.clone();
    let priority = get_issue_priority_config(row.priority);
    let view = view.clone();
    crate::surface::glass_row_divider(
        h_flex()
            .id(SharedString::from(format!("chat-issue-{}", row.issue_id)))
            .w_full()
            .items_center()
            .gap_2p5()
            .px_4()
            .py_2()
            .cursor_pointer()
            .when(is_checked, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
            // The popover's content closure runs on `&mut App`, so the row
            // reaches the host view through its weak handle (the MCP
            // picker's pattern).
            .on_click(move |_: &ClickEvent, window, cx| {
                if let Some(view) = view.upgrade() {
                    let toggle_id = toggle_id.clone();
                    view.update(cx, |this, cx| {
                        toggle(this, toggle_id, !is_checked, window, cx);
                    });
                }
            }),
    )
    .child(
        Icon::new(if is_checked {
            registry::UI_SELECTED
        } else {
            registry::UI_UNSELECTED
        })
        .small()
        .text_color(if is_checked { theme.foreground } else { muted }),
    )
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
            this.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(note),
            )
        },
    )
    .into_any_element()
}

/// The picker POPOVER hung off `trigger` (the composer's `#` tool): the
/// search row, then the checklist — checked rows first, matches capped.
/// `note_for` supplies a row's transient probe note (resolving / excluded).
#[allow(clippy::too_many_arguments)] // one popover, one place
pub(crate) fn issue_picker_popover<V: Render>(
    trigger: Button,
    rows: &[IssueRow],
    checked: &HashSet<String>,
    search: &Entity<InputState>,
    notes: Vec<(String, SharedString)>,
    toggle: fn(&mut V, String, bool, &mut Window, &mut gpui::Context<V>),
    cx: &mut gpui::Context<V>,
) -> Popover {
    let rows = rows.to_vec();
    let checked = checked.clone();
    let search = search.clone();
    let view = cx.entity().downgrade();
    Popover::new("chat-issue-picker")
        .p_1()
        .trigger(trigger)
        .content(move |_, window, cx| {
            let query = search.read(cx).value().to_string();
            let (visible, hidden, no_matches) = visible_rows(&rows, &checked, &query);
            let mut list = v_flex().w_full();
            if rows.is_empty() {
                list = list.child(list_note("No open issues in this team.", cx));
            }
            for ix in visible {
                let row = &rows[ix];
                let note = notes
                    .iter()
                    .find(|(id, _)| id == &row.issue_id)
                    .map(|(_, note)| note.clone());
                list = list.child(issue_row(
                    row,
                    checked.contains(&row.issue_id),
                    note,
                    &view,
                    toggle,
                    cx,
                ));
            }
            if no_matches {
                list = list.child(list_note("No matches. Only open issues are shown.", cx));
            }
            if hidden > 0 {
                list = list.child(list_note(
                    format!("+{hidden} more. Refine your search."),
                    cx,
                ));
            }
            v_flex()
                .w(px(480.))
                .max_w_full()
                .child(search_row(&search, window, cx))
                .child(
                    div()
                        .id("chat-issue-picker-scroll")
                        .max_h(px(360.))
                        .overflow_y_scroll()
                        .child(list),
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, identifier: &str, title: &str) -> IssueRow {
        IssueRow {
            issue_id: id.to_string(),
            board_id: "b".to_string(),
            identifier: identifier.to_string(),
            title: title.to_string(),
            description: None,
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

    /// Checked rows pin first whatever the query; matches search identifier
    /// and title case-insensitively and cap at [`MAX_UNCHECKED_ROWS`].
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

        let (visible, hidden, no_matches) = visible_rows(&rows, &checked, "FLICKER");
        assert_eq!(visible, vec![5, 60]);
        assert_eq!(hidden, 0);
        assert!(!no_matches);

        let (visible, _, no_matches) = visible_rows(&rows, &checked, "nothing here");
        assert_eq!(visible, vec![5]);
        assert!(no_matches);
    }
}
