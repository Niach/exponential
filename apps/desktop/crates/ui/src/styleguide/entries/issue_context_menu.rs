//! EXP-1074 — `issue-context-menu` (3 Special components): THE menu a
//! right-click opens on any issue.
//!
//! One layout on every client that has one: the web's
//! `packages/ui/src/issue-menu.ts` `ISSUE_MENU_LAYOUT` (drawn live by the
//! layout's single menu host, any row or chip opts in) and this crate's
//! `issue_list::build_row_context_menu`, which mirrors it item for item
//! (the Estimate submenu since EXP-1077). iOS and Android have no row menu
//! by decision: status and priority sit on the row, a long-press selects.
//!
//! EXP-1092: the demo IS `issue_list::build_row_context_menu` — the very
//! builder every issue row's `.context_menu(…)` calls — over a static issue
//! and the six builtin statuses, kept alive by `window.use_keyed_state` and
//! drawn open in place. With no synced store behind it, the store-fed parts
//! read empty (no assignees, no labels, no Estimate, no Move to board), which
//! is exactly how a row menu reads before its team has synced.

use std::rc::Rc;

use gpui::{div, App, Context, Div, Entity, IntoElement, ParentElement as _, Render, Window};
use gpui_component::menu::PopupMenu;

use domain::rows::Issue;
use domain::statuses::{constructed_default, ResolvedStatus};
use domain::IssueStatus;

pub(crate) const ID: &str = "issue-context-menu";
pub(crate) const OWNER: &str = "EXP-1074";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-issue-context-menu", cx, IssueMenuDemo::new);
    div().child(demo)
}

/// A static issue: in progress, high priority, due — synced-shape.
fn demo_issue() -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": "sg-issue",
        "board_id": "sg-board",
        "number": 42,
        "identifier": "EXP-42",
        "title": "Tidy the release notes",
        "status": "in_progress",
        "priority": "high",
        "due_date": "2026-10-02",
    }))
    .expect("a styleguide issue row deserializes")
}

/// The team's statuses as a fresh team has them: the builtin rows.
fn demo_statuses() -> Rc<Vec<ResolvedStatus>> {
    Rc::new(
        [
            IssueStatus::Backlog,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            IssueStatus::Done,
            IssueStatus::Cancelled,
            IssueStatus::Duplicate,
        ]
        .into_iter()
        .map(constructed_default)
        .collect(),
    )
}

struct IssueMenuDemo {
    menu: Entity<PopupMenu>,
}

impl IssueMenuDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let issue = demo_issue();
        let statuses = demo_statuses();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::issue_list::build_row_context_menu(menu, &issue, &statuses, None, window, cx)
        });
        Self { menu }
    }
}

impl Render for IssueMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.menu.clone()
    }
}
