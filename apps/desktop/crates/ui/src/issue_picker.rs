//! EXP-825 — the Agent page composer's issue picker: the team's OPEN issues
//! as a searchable checklist in a popover (the `#` tool), one checked issue =
//! Start coding, two or more = a batch. Ported out of the deleted
//! start-coding dialog: the pool rules (EXP-119: `done`/`cancelled`/
//! `duplicate`/PR-merged rows hidden, pre-seeded ids exempt and force-checked),
//! the per-run cap and the row anatomy (EXP-768: selection glyph · priority ·
//! identifier · status · title) are unchanged.
//!
//! EXP-892: searching runs through the ONE engine (`domain::issue_search`),
//! and the popover carries the uniform list keyboard contract — the top row
//! is selected while typing, ↑/↓ move the selection, hovering a row MOVES it
//! (one highlight, never a second hover tint) and Enter TOGGLES the selected
//! row with the popover staying open.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, WeakEntity, Window,
};
use gpui_component::button::Button;
use gpui_component::input::InputState;
use gpui_component::popover::Popover;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::options::get_issue_priority_config;
use domain::{IssuePriority, IssueStatus};

use crate::controls::{search_field, SearchFieldSize};
use crate::icons::{option_icon, registry, resolved_status_icon};

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

/// The search row heading the picker (EXP-768): the row shell around ONE
/// chrome-less [`search_field`] (EXP-963) — the glyph and the clear come from
/// the shared field, never hand-drawn here, or the row would carry two search
/// marks. The box tweaks stay: the SHELL owns the row's height (`py_3`), so
/// the field must not add its own 36px rung on top of it.
fn search_row(state: &Entity<InputState>, window: &Window, cx: &App) -> gpui::Div {
    crate::surface::glass_row_shell().child(
        div().flex_1().min_w_0().child(
            search_field(state, SearchFieldSize::Md, window, cx)
                .appearance(false)
                .h_auto()
                .px_0()
                .py_0(),
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
/// `note`). The whole row toggles; hovering it MOVES the keyboard selection
/// onto it (EXP-892 — one highlight, so what Enter would pick is always the
/// row under the pointer).
fn issue_row<V: IssuePickerHost>(
    row: &IssueRow,
    is_checked: bool,
    is_selected: bool,
    position: usize,
    note: Option<SharedString>,
    view: &WeakEntity<V>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let toggle_id = row.issue_id.clone();
    let priority = get_issue_priority_config(row.priority);
    let hover_view = view.clone();
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
            // ONE highlight: the selected row. Hover moves the selection
            // instead of painting a second tint.
            .when(is_selected, |this| this.bg(theme.list_active))
            .when(is_checked && !is_selected, |this| {
                this.bg(theme.list_active.opacity(0.4))
            })
            .on_hover(move |hovered, _window, cx| {
                if !*hovered {
                    return;
                }
                if let Some(view) = hover_view.upgrade() {
                    view.update(cx, |this, cx| {
                        if this.picker_selected() != position {
                            this.set_picker_selected(position);
                            cx.notify();
                        }
                    });
                }
            })
            // The popover's content closure runs on `&mut App`, so the row
            // reaches the host view through its weak handle (the MCP
            // picker's pattern).
            .on_click(move |_: &ClickEvent, window, cx| {
                if let Some(view) = view.upgrade() {
                    let toggle_id = toggle_id.clone();
                    view.update(cx, |this, cx| {
                        this.set_picker_selected(position);
                        this.toggle_picked_issue(toggle_id, !is_checked, window, cx);
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

/// The host view behind the picker popover (EXP-892): it owns the checked
/// set AND the keyboard selection, because the popover's content closure runs
/// on a bare `&mut App` and can only reach state through the host's handle.
pub(crate) trait IssuePickerHost: Render + Sized {
    /// The selected POSITION in the currently visible list (not a row index).
    fn picker_selected(&self) -> usize;
    fn set_picker_selected(&mut self, position: usize);
    /// The host's [`VisibleRowsMemo`] — interior-mutable, because the
    /// content closure only ever READS the host while it paints.
    fn picker_memo(&self) -> &std::cell::RefCell<VisibleRowsMemo>;
    /// Check/uncheck one row; the popover stays open.
    fn toggle_picked_issue(
        &mut self,
        issue_id: String,
        on: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    );
}

/// The picker POPOVER hung off `trigger` (the composer's `#` tool): the
/// search row, then the checklist — checked rows first, ranked matches
/// capped. `notes` supplies a row's transient probe note (resolving /
/// excluded). Keyboard (EXP-892): ↑/↓ move, Enter toggles, the top row starts
/// selected.
pub(crate) fn issue_picker_popover<V: IssuePickerHost>(
    trigger: Button,
    // EXP-868: shared, not copied — the composer renders on every window
    // redraw, and a per-render copy of the whole pool was measurable.
    rows: Rc<Vec<IssueRow>>,
    checked: &HashSet<String>,
    search: &Entity<InputState>,
    notes: Vec<(String, SharedString)>,
    // EXP-946: the side and the height cap [`popover_fit`] worked out from
    // where the trigger actually painted.
    fit: (gpui::Anchor, gpui::Pixels),
    cx: &mut gpui::Context<V>,
) -> Popover {
    let (anchor, max_height) = fit;
    let checked = checked.clone();
    let search = search.clone();
    let view = cx.entity().downgrade();
    let search_for_open = search.clone();
    let view_for_open = view.clone();
    Popover::new("chat-issue-picker")
        .p_1()
        .anchor(anchor)
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            // Fresh search + selection per open, and the field takes focus so
            // ↑/↓/Enter land on the list (the label picker's recipe).
            search_for_open.update(cx, |input, cx| input.set_value("", window, cx));
            if let Some(view) = view_for_open.upgrade() {
                view.update(cx, |this, cx| {
                    this.set_picker_selected(0);
                    cx.notify();
                });
            }
            if *open {
                search_for_open.read(cx).focus_handle(cx).focus(window, cx);
            }
        })
        .content(move |_, window, cx| {
            let query = search.read(cx).value().to_string();
            // Memoised on the host: the same pool, query and checked set
            // never rank twice (release review R5).
            let (visible, hidden, no_matches) = match view.upgrade() {
                Some(host) => host.read(cx).picker_memo().borrow_mut().get(&rows, &checked, &query),
                None => visible_rows(&rows, &checked, &query),
            };
            // The host's selection is clamped to what is on screen, so a
            // narrowing query always leaves a real row selected.
            let selected = view
                .upgrade()
                .map(|view| view.read(cx).picker_selected())
                .unwrap_or(0)
                .min(visible.len().saturating_sub(1));
            let mut list = v_flex().w_full();
            if rows.is_empty() {
                list = list.child(list_note("No open issues in this team.", cx));
            }
            for (position, ix) in visible.iter().copied().enumerate() {
                let row = &rows[ix];
                let note = notes
                    .iter()
                    .find(|(id, _)| id == &row.issue_id)
                    .map(|(_, note)| note.clone());
                list = list.child(issue_row(
                    row,
                    checked.contains(&row.issue_id),
                    position == selected,
                    position,
                    note,
                    &view,
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
            let count = visible.len();
            let picked: Option<(String, bool)> = visible
                .get(selected)
                .map(|ix| (rows[*ix].issue_id.clone(), checked.contains(&rows[*ix].issue_id)));
            v_flex()
                .w(px(480.))
                .max_w_full()
                // EXP-946: the list SHRINKS with the room its side has, so the
                // popover can never run off the window.
                .max_h(max_height)
                // ↑/↓/Enter arrive as the search field's own actions — a
                // single-line input no-ops them, so the popover CAPTURES them
                // for the list (the `MentionInput` pattern).
                .capture_action(move_selection_listener::<V, gpui_component::input::MoveUp>(
                    &view, -1, count,
                ))
                .capture_action(move_selection_listener::<V, gpui_component::input::MoveDown>(
                    &view, 1, count,
                ))
                .capture_action({
                    let view = view.clone();
                    move |_: &gpui_component::input::Enter, window, cx: &mut App| {
                        let Some((issue_id, was_checked)) = picked.clone() else {
                            return;
                        };
                        if let Some(view) = view.upgrade() {
                            view.update(cx, |this, cx| {
                                this.toggle_picked_issue(issue_id, !was_checked, window, cx);
                            });
                        }
                    }
                })
                .child(search_row(&search, window, cx))
                .child(
                    div()
                        .id("chat-issue-picker-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(list),
                )
        })
}

/// One ↑/↓ handler: move the host's selection by `delta`, clamped to the
/// `count` rows on screen.
fn move_selection_listener<V: IssuePickerHost, A: gpui::Action>(
    view: &WeakEntity<V>,
    delta: isize,
    count: usize,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, _window, cx| {
        if count == 0 {
            return;
        }
        if let Some(view) = view.upgrade() {
            view.update(cx, |this, cx| {
                let next = this
                    .picker_selected()
                    .min(count - 1)
                    .saturating_add_signed(delta)
                    .min(count - 1);
                this.set_picker_selected(next);
                cx.notify();
            });
        }
    }
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
