//! The My Issues screen (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/routes/w/$workspaceSlug/my-issues/index.tsx`).
//!
//! Cross-project "My Issues": every issue assigned to the signed-in user
//! across all projects in the active workspace, grouped by status like the
//! project board. Like the web route (which composes `IssueFilterBar` +
//! `IssueList` itself rather than reusing the project page), this view owns
//! its own [`IssueFilterBar`] + [`IssueListView`] pair pinned to
//! [`IssueQuery::MyIssues`]:
//!
//! - bar: `title="My Issues"`, `canCreate=false` (web parity — no New Issue
//!   button here),
//! - body: the virtualized grouped list, or — when the *unfiltered* scope is
//!   ready and empty — the web's whole-list `CircleUser` empty state ("No
//!   issues assigned to you"),
//! - rows span projects, so the identifier column carries the project
//!   context; clicking a row opens the full-page detail (list behavior).
//!
//! Filters are per-visit local state (the web keeps them in the URL; desktop
//! has no shareable URLs by design, §4.2) and reset when the scope changes —
//! same lifecycle as the board. `is_ready` gating (§4.1) rides on the list's
//! skeleton plus the unfiltered `BoardData::is_ready` for the empty state, so
//! an in-flight snapshot never renders as "no issues" (EXP-1 #13).

use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render, Styled,
    Subscription, Window,
};
use gpui_component::{input::InputState, v_flex, ActiveTheme as _, Icon, IconName};
use sync::Store;

use domain::IssueFilters;

use crate::filter_bar::IssueFilterBar;
use crate::filter_popover::{FilterView, OnFiltersChange, OnViewChange};
use crate::issue_list::{IssueListView, IssueQuery};
use crate::navigation::{active_workspace_id, nav_for_window, Navigation};
use crate::queries;

pub struct MyIssuesView {
    nav: Entity<Navigation>,
    /// The list scope — `MyIssues { workspace, me }` once both resolve,
    /// `None` while the session/workspace are still coming up.
    query: IssueQuery,
    filters: IssueFilters,
    popover_view: FilterView,
    label_query: Entity<InputState>,
    issue_list: Entity<IssueListView>,
    _subscriptions: Vec<Subscription>,
}

impl MyIssuesView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let label_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter labels..."));
        let issue_list = cx.new(IssueListView::new);

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Scope inputs: the active workspace (nav + workspaces) and the
            // signed-in account (session state machine).
            cx.observe(&nav, |this, _, cx| this.sync_query(cx)),
            cx.observe(&collections.workspaces, |this, _, cx| this.sync_query(cx)),
            cx.observe(&Store::global(cx).state(), |this, _, cx| this.sync_query(cx)),
            // Bar + empty-state inputs: labels feed the popover/pills; issues
            // and projects drive the unfiltered "any assigned?" gate.
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // Live label search re-filters the popover's label rows.
            cx.observe(&label_query, |_, _, cx| cx.notify()),
        ];

        let mut this = Self {
            nav,
            query: IssueQuery::None,
            filters: IssueFilters::empty(),
            popover_view: FilterView::Categories,
            label_query,
            issue_list,
            _subscriptions: subscriptions,
        };
        this.sync_query(cx);
        this
    }

    /// Keep the list pinned at "assigned to me in the active workspace".
    /// Scope changes (workspace switch, re-login) reset the filters — the web
    /// route's filters live in the URL, so a new scope starts clean.
    fn sync_query(&mut self, cx: &mut gpui::Context<Self>) {
        let next = match (
            active_workspace_id(&self.nav, cx),
            queries::active_account(cx),
        ) {
            (Some(workspace_id), Some(account)) => IssueQuery::MyIssues {
                workspace_id,
                user_id: account.user_id,
            },
            _ => IssueQuery::None,
        };
        if self.query == next {
            return;
        }
        self.query = next.clone();
        self.filters = IssueFilters::empty();
        self.popover_view = FilterView::Categories;
        self.issue_list.update(cx, |list, cx| {
            list.set_query(next, cx);
            list.set_filters(IssueFilters::empty(), cx);
        });
        cx.notify();
    }

    /// The single `onFiltersChange` sink (tabs, popover, pills — web parity).
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
}

impl Render for MyIssuesView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web: `if (!workspace) return <div class="text-muted-foreground
        // text-sm p-6">Loading…</div>`.
        let IssueQuery::MyIssues {
            workspace_id,
            user_id,
        } = self.query.clone()
        else {
            return div()
                .size_full()
                .bg(cx.theme().colors.list)
                .p_6()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Loading…")
                .into_any_element();
        };

        let labels = queries::workspace_labels(cx, &workspace_id);

        // Web `issuesReady && totalIssueCount === 0` — the UNFILTERED scope
        // decides between the whole-list empty state and the list.
        let unfiltered = queries::my_issues(cx, &workspace_id, &user_id, &IssueFilters::empty());
        let show_empty = unfiltered.is_ready && !unfiltered.has_any_issues;

        let entity = cx.entity().downgrade();
        let on_filters_change: OnFiltersChange = Rc::new(move |next, _window, cx| {
            if let Some(view) = entity.upgrade() {
                view.update(cx, |view, cx| view.apply_filters(next, cx));
            }
        });
        let entity = cx.entity().downgrade();
        let on_view_change: OnViewChange = Rc::new(move |popover_view, _window, cx| {
            if let Some(view) = entity.upgrade() {
                view.update(cx, |view, cx| view.set_popover_view(popover_view, cx));
            }
        });

        let body: gpui::AnyElement = if show_empty {
            empty_state(cx)
        } else {
            self.issue_list.clone().into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .child(IssueFilterBar::new(
                "My Issues",
                self.filters.clone(),
                labels,
                self.popover_view,
                self.label_query.clone(),
                on_filters_change,
                on_view_change,
                false, // web `canCreate={false}` — no New Issue on My Issues
            ))
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

/// The web route's whole-list `EmptyState` (CircleUser icon).
fn empty_state(cx: &App) -> gpui::AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .child(
            Icon::new(IconName::CircleUser)
                .size_6()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child("No issues assigned to you"),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Issues assigned to you across all projects in this workspace will show up here."),
        )
        .into_any_element()
}
