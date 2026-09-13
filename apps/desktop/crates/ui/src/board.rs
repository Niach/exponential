//! The board-view screen — web parity target
//! `apps/web/src/routes/t/$teamSlug/boards/$boardSlug/index.tsx`
//! (masterplan-v3 §4.2 "Board view").
//!
//! Composition mirrors the web route: the virtualized [`IssueListView`] fills
//! the view. The same view also backs **My Issues** (web
//! `my-issues/index.tsx` renders the identical list with `canCreate=false`).
//!
//! EXP-862 removed issue filtering from every client, so the board is the
//! list and nothing above it — the list starts at the top of its panel.
//!
//! EXP-289/EXP-426: the list's bulk-action bar is still rendered by THIS
//! view, never by the list itself. Without the filter bar's control row to
//! swap into, it floats over the bottom of the list, so starting a selection
//! never moves a row (the EXP-289 no-jump invariant) and the list keeps the
//! whole panel. One fix covers both surfaces: the Inbox tool window's
//! *My Issues* tab is this same view.
//!
//! State ownership (§4.1): this entity owns the scope [`IssueQuery`] and
//! hands it to the list, which re-renders off the synced collections.

use gpui::{
    div, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex};

use crate::issue_list::{IssueListView, IssueQuery};

pub struct BoardView {
    query: IssueQuery,
    issue_list: Entity<IssueListView>,
    _subscriptions: Vec<Subscription>,
}

impl BoardView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let issue_list = cx.new(|cx| IssueListView::new(window, cx));

        let subscriptions = vec![
            // EXP-289: the floating bulk bar lives in THIS view's tree, so a
            // selection change inside the list (checkbox, Cmd/Shift-click,
            // Cmd-A, Escape) has to re-render the board too.
            cx.observe(&issue_list, |_, _, cx| cx.notify()),
        ];

        Self {
            query: IssueQuery::None,
            issue_list,
            _subscriptions: subscriptions,
        }
    }

    /// Point the board at a new scope (called by the screens panel on
    /// navigation).
    pub fn set_query(&mut self, query: IssueQuery, cx: &mut gpui::Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        self.issue_list.update(cx, |list, cx| {
            list.set_query(query, cx);
        });
        cx.notify();
    }
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let bulk_bar = self.issue_list.update(cx, |list, cx| list.bulk_bar(cx));

        v_flex()
            .size_full()
            .relative()
            .child(div().flex_1().min_h_0().child(self.issue_list.clone()))
            .children(bulk_bar.map(|bulk| {
                h_flex()
                    .absolute()
                    .bottom_4()
                    .left_0()
                    .right_0()
                    .justify_center()
                    .px_4()
                    .child(bulk)
            }))
    }
}
