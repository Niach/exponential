//! The board-view screen — web parity target
//! `apps/web/src/routes/t/$teamSlug/boards/$boardSlug/index.tsx`
//! (masterplan-v3 §4.2 "Board view").
//!
//! Composition mirrors the web route: [`IssueFilterBar`] (title row +
//! filter popover + active pills) on top, the virtualized [`IssueListView`]
//! filling the rest. The same view also backs **My Issues** (web
//! `my-issues/index.tsx` renders the identical bar+list pair with
//! `title="My Issues"` and `canCreate=false`).
//!
//! EXP-289: this view also hosts the list's **floating** bulk-action bar —
//! an absolutely-positioned overlay over the title row (see `render`). It
//! used to be an in-flow row inside the list, which shoved every issue down
//! the moment multiselect started. One fix covers both surfaces: the Inbox
//! tool window's *My Issues* tab is this same view.
//!
//! State ownership (§4.1): this entity owns the `IssueFilters` (the web route
//! keeps them in the URL; the desktop keeps them per-board and resets on
//! navigation — same lifecycle, no shareable URLs on desktop by design), the
//! filter-popover drill-down [`FilterView`] and the label-search
//! `InputState`. Children get snapshots + callbacks, exactly like the web
//! component props. The issue list itself re-renders off the synced
//! collections; this view re-renders on label/board changes for the bar.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{input::InputState, v_flex};
use sync::Store;

use domain::rows::Label;
use domain::IssueFilters;

use crate::filter_bar::IssueFilterBar;
use crate::filter_popover::{FilterView, OnFiltersChange, OnViewChange};
use crate::issue_list::{IssueListView, IssueQuery};

pub struct BoardView {
    query: IssueQuery,
    filters: IssueFilters,
    popover_view: FilterView,
    label_query: Entity<InputState>,
    issue_list: Entity<IssueListView>,
    _subscriptions: Vec<Subscription>,
}

impl BoardView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let label_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter labels..."));
        let issue_list = cx.new(IssueListView::new);

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // The bar reads labels (popover list + pills) and boards
            // (team resolution); the list observes its own collections.
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // EXP-314: the filter vocabulary is synced data now.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            // Live label search re-filters the popover's label rows.
            cx.observe(&label_query, |_, _, cx| cx.notify()),
            // EXP-289: the floating bulk bar lives in THIS view's tree, so a
            // selection change inside the list (checkbox, Cmd/Shift-click,
            // Cmd-A, Escape) has to re-render the board too.
            cx.observe(&issue_list, |_, _, cx| cx.notify()),
        ];

        Self {
            query: IssueQuery::None,
            filters: IssueFilters::empty(),
            popover_view: FilterView::Categories,
            label_query,
            issue_list,
            _subscriptions: subscriptions,
        }
    }

    /// Point the board at a new scope (called by the screens panel on
    /// navigation). Filters reset — the web route's filters live in the URL,
    /// so navigating to another board starts clean.
    pub fn set_query(&mut self, query: IssueQuery, cx: &mut gpui::Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        self.filters = IssueFilters::empty();
        self.popover_view = FilterView::Categories;
        self.issue_list.update(cx, |list, cx| {
            list.set_query(query, cx);
            list.set_filters(IssueFilters::empty(), cx);
        });
        cx.notify();
    }

    /// The board's current scope — read by the issue-detail prev/next
    /// switcher (EXP-48) so it follows exactly the list this board shows.
    pub fn query(&self) -> &IssueQuery {
        &self.query
    }

    /// The board's active filters (same EXP-48 read as [`Self::query`]).
    pub fn filters(&self) -> &IssueFilters {
        &self.filters
    }

    /// The single `onFiltersChange` sink (popover toggles, pills and
    /// clear-all all funnel here — web prop parity).
    fn apply_filters(&mut self, next: IssueFilters, cx: &mut gpui::Context<Self>) {
        if self.filters == next {
            return;
        }
        self.filters = next.clone();
        self.issue_list
            .update(cx, |list, cx| list.set_filters(next, cx));
        cx.notify();
    }

    fn set_popover_view(&mut self, view: FilterView, cx: &mut gpui::Context<Self>) {
        if self.popover_view == view {
            return;
        }
        self.popover_view = view;
        cx.notify();
    }

    /// Bar title: the board scope is the sidebar's "All Issues" tool
    /// window; My Issues names itself.
    fn title(&self) -> SharedString {
        match &self.query {
            IssueQuery::MyIssues { .. } => "My Issues".into(),
            _ => "All Issues".into(),
        }
    }

    /// The New Issue button shows on board scopes AND My Issues (EXP-181 —
    /// the `NewIssue` action targets the window's active board either way;
    /// web's `canCreate={false}` on My Issues is a web-only choice).
    fn can_create(&self) -> bool {
        matches!(
            self.query,
            IssueQuery::Board { .. } | IssueQuery::MyIssues { .. }
        )
    }

    /// The team whose labels feed the popover + pills (web
    /// `useBoardBoardData` scopes labels by `team.id`).
    fn team_id(&self, cx: &App) -> Option<String> {
        match &self.query {
            IssueQuery::None => None,
            IssueQuery::Board { board_id } => Store::global(cx)
                .collections()
                .boards
                .read(cx)
                .get(board_id)
                .map(|board| board.team_id.clone()),
            IssueQuery::MyIssues { team_id, .. } => Some(team_id.clone()),
        }
    }
}

/// A team's labels, sort-order-then-name sorted (settings order — the
/// web live query has no explicit order; deterministic here).
fn labels_in_team(team_id: &str, cx: &App) -> Vec<Label> {
    let mut out: Vec<Label> = Store::global(cx)
        .collections()
        .labels
        .read(cx)
        .iter()
        .filter(|label| label.team_id == team_id)
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(f64::MAX)
            .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = self.team_id(cx);
        let labels = team_id
            .as_deref()
            .map(|team_id| labels_in_team(team_id, cx))
            .unwrap_or_default();
        // EXP-314: the filter popover/pills render the TEAM's status rows.
        let statuses = team_id
            .as_deref()
            .map(|team_id| crate::queries::team_status_options(cx, team_id))
            .unwrap_or_else(domain::statuses::default_resolved_statuses);

        let entity = cx.entity().downgrade();
        let on_filters_change: OnFiltersChange = Rc::new(move |next, _window, cx| {
            if let Some(board) = entity.upgrade() {
                board.update(cx, |board, cx| board.apply_filters(next, cx));
            }
        });
        let entity = cx.entity().downgrade();
        let on_view_change: OnViewChange = Rc::new(move |view, _window, cx| {
            if let Some(board) = entity.upgrade() {
                board.update(cx, |board, cx| board.set_popover_view(view, cx));
            }
        });

        // EXP-289: the list's bulk-action bar FLOATS over the title row
        // instead of sitting in flow above the list — an in-flow row pushed
        // every issue down the moment a checkbox was ticked (the list
        // "jumped"). The overlay wrapper below pins to the root's top edge and
        // repeats the filter bar's `px_4` inset, so the 34px pill lands on the
        // 44px title row; the pill itself is opaque + occluding, so it masks
        // (and swallows the clicks over) what it covers.
        let bulk_bar = self.issue_list.update(cx, |list, cx| list.bulk_bar(cx));

        v_flex()
            .size_full()
            .relative()
            .child(IssueFilterBar::new(
                self.title(),
                self.filters.clone(),
                labels,
                statuses,
                self.popover_view,
                self.label_query.clone(),
                on_filters_change,
                on_view_change,
                self.can_create(),
            ))
            .child(div().flex_1().min_h_0().child(self.issue_list.clone()))
            .when_some(bulk_bar, |board, bar| {
                board.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .px_4()
                        .py_1()
                        .flex()
                        .child(bar),
                )
            })
    }
}
