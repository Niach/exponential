//! The project-board screen — web parity target
//! `apps/web/src/routes/w/$workspaceSlug/projects/$projectSlug/index.tsx`
//! (masterplan-v3 §4.2 "Project board", EXP-1 #4/#12).
//!
//! Composition mirrors the web route: [`IssueFilterBar`] (title row + tabs +
//! filter popover + active pills) on top, the virtualized [`IssueListView`]
//! filling the rest. The same view also backs **My Issues** (web
//! `my-issues/index.tsx` renders the identical bar+list pair with
//! `title="My Issues"` and `canCreate=false`).
//!
//! State ownership (§4.1): this entity owns the `IssueFilters` (the web route
//! keeps them in the URL; the desktop keeps them per-board and resets on
//! navigation — same lifecycle, no shareable URLs on desktop by design), the
//! filter-popover drill-down [`FilterView`] and the label-search
//! `InputState`. Children get snapshots + callbacks, exactly like the web
//! component props. The issue list itself re-renders off the synced
//! collections; this view re-renders on label/project changes for the bar.

use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
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
            // The bar reads labels (popover list + pills) and projects
            // (workspace resolution); the list observes its own collections.
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            // Live label search re-filters the popover's label rows.
            cx.observe(&label_query, |_, _, cx| cx.notify()),
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

    /// The single `onFiltersChange` sink (bar tabs, popover toggles, pills,
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

    /// Bar title (web: the project route renders the default `"Issues"`;
    /// my-issues passes `"My Issues"`).
    fn title(&self) -> SharedString {
        match &self.query {
            IssueQuery::MyIssues { .. } => "My Issues".into(),
            _ => "Issues".into(),
        }
    }

    /// Web `canCreate`: the New Issue button shows on project boards, never
    /// on My Issues (web passes `canCreate={false}` there).
    fn can_create(&self) -> bool {
        matches!(self.query, IssueQuery::Project { .. })
    }

    /// The workspace whose labels feed the popover + pills (web
    /// `useProjectBoardData` scopes labels by `workspace.id`).
    fn workspace_id(&self, cx: &App) -> Option<String> {
        match &self.query {
            IssueQuery::None => None,
            IssueQuery::Project { project_id } => Store::global(cx)
                .collections()
                .projects
                .read(cx)
                .get(project_id)
                .map(|project| project.workspace_id.clone()),
            IssueQuery::MyIssues { workspace_id, .. } => Some(workspace_id.clone()),
        }
    }
}

/// A workspace's labels, sort-order-then-name sorted (settings order — the
/// web live query has no explicit order; deterministic here).
fn labels_in_workspace(workspace_id: &str, cx: &App) -> Vec<Label> {
    let mut out: Vec<Label> = Store::global(cx)
        .collections()
        .labels
        .read(cx)
        .iter()
        .filter(|label| label.workspace_id == workspace_id)
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
        let labels = self
            .workspace_id(cx)
            .map(|workspace_id| labels_in_workspace(&workspace_id, cx))
            .unwrap_or_default();

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

        v_flex()
            .size_full()
            .child(IssueFilterBar::new(
                self.title(),
                self.filters.clone(),
                labels,
                self.popover_view,
                self.label_query.clone(),
                on_filters_change,
                on_view_change,
                self.can_create(),
            ))
            .child(div().flex_1().min_h_0().child(self.issue_list.clone()))
    }
}
