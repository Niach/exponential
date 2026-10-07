//! EXP-1233: `composer-fix-conflicts` (3 Special components): what a Merge
//! refused by a REAL conflict opens — the composer on the Fix merge
//! conflicts builtin with that pull request picked, at once (the Merge slot
//! never swaps to a "Fix conflicts" button any more).
//!
//! The demo draws the composer's REAL pieces over a static synced-shape row
//! (never a live one), resolved through `domain::fix_conflicts`, the
//! composer's own call: the headline verb
//! (`chat_launch::headline(FixConflicts)`) beside the PR's issue chip
//! (`chat_screen::fix_conflicts_issue_chip`), the card with the PR row and
//! the refusal line (`chat_screen::fix_conflicts_card` +
//! `fix_conflicts_pr_row`, what `render_fix_conflicts_card` hangs the `pr`
//! dropdown on), and a second card in the unpicked state — the placeholder
//! alone. The same three things the web island draws
//! (`apps/styleguide/src/entries/composer-fix-conflicts.tsx`).

use gpui::{div, px, App, Div, FontWeight, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

use crate::chat_launch::{self, SubjectKind};
use crate::chat_screen::{fix_conflicts_card, fix_conflicts_issue_chip, fix_conflicts_pr_row};
use domain::fix_conflicts::resolve_fix_conflicts_pr;
use domain::rows::Issue;

pub(crate) const ID: &str = "composer-fix-conflicts";
pub(crate) const OWNER: &str = "EXP-1233";

/// APP-14's synced row: PR #2117 open on `exp/APP-14`, based on master.
fn issue() -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": "sg-APP-14",
        "board_id": "sg-board",
        "number": 14,
        "identifier": "APP-14",
        "title": "Group board issues by assignee",
        "status": "in_review",
        "branch": "exp/APP-14",
        "pr_base_branch": "master",
        "pr_number": 2117,
        "pr_url": "https://github.com/o/r/pull/2117",
        "pr_state": "open",
    }))
    .expect("a styleguide issue row deserializes")
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let rows = [issue()];
    let pr = resolve_fix_conflicts_pr(Some("sg-APP-14"), &rows);
    let chips = pr
        .iter()
        .flat_map(|pr| pr.issues.iter())
        .map(|issue| {
            fix_conflicts_issue_chip(issue, cx)
                .on_remove(
                    gpui::SharedString::from(format!("chat-chip-issue-{}-remove", issue.identifier)),
                    |_, _, _| {},
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let picked = fix_conflicts_pr_row("sg-fix-conflicts-pr", pr.as_ref(), cx).into_any_element();
    let unpicked = fix_conflicts_pr_row("sg-fix-conflicts-pr-empty", None, cx).into_any_element();
    div()
        .flex()
        .flex_col()
        .w(px(520.))
        .gap_2()
        // The headline: the builtin's own verb and the PR's issue chip.
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .px_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child(chat_launch::headline(&SubjectKind::FixConflicts)),
                )
                .children(chips),
        )
        // The card a refused merge opens: the PR row + the refusal line.
        .child(fix_conflicts_card("sg-fix-conflicts-card", picked, true, cx))
        // Picked from the action picker instead: the placeholder alone.
        .child(fix_conflicts_card("sg-fix-conflicts-card-empty", unpicked, false, cx))
}
