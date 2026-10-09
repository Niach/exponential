//! EXP-1248: `stack-merge-choice-dialog` (3 Special components): the ONE
//! confirm a stack merge asks — `Merge stack` from the merge control (the
//! whole open chain) and `Merge through here` from a hovered stack-rail row
//! (that member and everything beneath it) — Cancel beside the primary.
//!
//! The IDE's confirm is a native alert window (`pr_merge::ask_stack_merge_mode`).
//! A window cannot be embedded, so the demo draws the alert's REAL view
//! (`native_dialog::alert_specimen`) over the SAME spec the product opens
//! (`pr_merge::stack_confirm_alert`).
//!
//! Nothing is retyped: static synced-shape rows (never live ones) go
//! through `domain::pr_stack::stack_merge_confirm`, the control's own call,
//! on the fixture's three-stack (`stack-merge-choice.json` `confirm`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use domain::pr_stack::{stack_merge_confirm, StackConfirmMode, StackMergeConfirm};
use domain::rows::Issue;

pub(crate) const ID: &str = "stack-merge-choice-dialog";
pub(crate) const OWNER: &str = "EXP-1145";

/// The pull request the control was pressed on: the middle of the stack.
const PRESSED: &str = "sg-EXP-1144";

/// A synced-shape issue row with an open pull request on `exp/<identifier>`,
/// based on `base`.
fn issue(identifier: &str, base: &str) -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": format!("sg-{identifier}"),
        "board_id": "sg-board",
        "number": 1,
        "identifier": identifier,
        "title": identifier,
        "status": "in_review",
        "branch": format!("exp/{identifier}"),
        "pr_base_branch": base,
        "pr_url": format!("https://github.com/o/r/pull/{identifier}"),
        "pr_state": "open",
    }))
    .expect("a styleguide issue row deserializes")
}

/// A stack of three: EXP-1105 on master, EXP-1144 on it, EXP-1150 on top.
fn stack() -> Vec<Issue> {
    vec![
        issue("EXP-1150", "exp/EXP-1144"),
        issue("EXP-1105", "master"),
        issue("EXP-1144", "exp/EXP-1105"),
    ]
}

/// What the confirm says for `mode` pressed mid-stack.
fn confirm(mode: StackConfirmMode) -> Option<StackMergeConfirm> {
    let issues = stack();
    let pressed = issues.iter().find(|issue| issue.id == PRESSED)?;
    stack_merge_confirm(pressed, &issues, mode)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let mut row = div().flex().flex_wrap().gap_4();
    for (id, mode) in [
        ("sg-stack-merge-confirm", StackConfirmMode::Stack),
        ("sg-stack-merge-through", StackConfirmMode::Through),
    ] {
        // A stack member always earns the confirm (the test below locks it).
        if let Some(confirm) = confirm(mode) {
            let spec = crate::pr_merge::stack_confirm_alert(PRESSED, &confirm);
            row = row.child(crate::native_dialog::alert_specimen(id, spec, window, cx));
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's words are the fixture's: the whole chain from the middle,
    /// and through the middle with the top left open.
    #[test]
    fn the_demo_says_the_fixtures_words() {
        let stack = confirm(StackConfirmMode::Stack).expect("a stack member asks");
        assert_eq!(stack.title, "Merge stack");
        assert_eq!(stack.body, "Lands 3 pull requests, bottom-up: EXP-1105, EXP-1144, EXP-1150.");
        let through = confirm(StackConfirmMode::Through).expect("a stack member asks");
        assert_eq!(through.title, "Merge through here");
        assert_eq!(through.body, "Lands 2 pull requests, bottom-up: EXP-1105, EXP-1144. EXP-1150 stays open.");
        assert_eq!(through.issue_id, PRESSED);
    }
}
