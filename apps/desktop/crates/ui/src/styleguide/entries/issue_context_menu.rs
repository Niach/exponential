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
//! Describes rather than paints (`fn() -> Div`, no `&App`), like `sub_shell`.

use gpui::{div, Div, ParentElement as _};

pub(crate) const ID: &str = "issue-context-menu";
pub(crate) const OWNER: &str = "EXP-1074";

pub(crate) fn render() -> Div {
    div()
        .child("Issue context menu — header band (identifier, title), then:")
        .child("Open issue · Mark as done / Move to backlog · Copy issue ID · [Select on a phone] · [Unmark duplicate]")
        .child("— divider —")
        .child("Status › · Assignee › · Priority › · Labels › · [Estimate ›] · Set due date › · [Move to board ›] · Add relation ›")
        .child("Delete issue › (red, no divider above it)")
        .child("Each submenu row shows its current value before the chevron; the label never wraps.")
        .child("IDE: ui::issue_list::build_row_context_menu (the same rows, the same order)")
}
