//! EXP-1145: `stack-merge-choice-dialog` (3 Special components): what a
//! plain Merge asks when its pull request is a member of an OPEN stack,
//! Cancel · Merge this pull request · Merge stack.
//!
//! The IDE's dialog is a native alert window (`pr_merge::ask_stack_merge`).
//! A window cannot be embedded, so the demo draws the alert's REAL view
//! (`native_dialog::alert_specimen`) over the SAME spec the product opens
//! (`pr_merge::stack_merge_alert`): the title in the titlebar band, the body
//! (the chain bottom to top, then one sentence per answer) and the three
//! buttons.
//!
//! Nothing is retyped: static synced-shape rows (never live ones) go
//! through `domain::pr_stack::stack_merge_choice`, the header's own call.
//! The state is the web specimen's, the fixture's "middle of a three-stack"
//! case (`stack-merge-choice.json`): the one that shows both sentences at
//! work.

use gpui::{div, App, Div, ParentElement as _, Window};

use domain::pr_stack::{stack_merge_choice, StackMergeChoice};
use domain::rows::Issue;

pub(crate) const ID: &str = "stack-merge-choice-dialog";
pub(crate) const OWNER: &str = "EXP-1145";

/// The pull request the Merge was pressed on: the middle of the stack.
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

/// What the dialog says for a Merge pressed mid-stack.
fn choice() -> Option<StackMergeChoice> {
    let issues = stack();
    let pressed = issues.iter().find(|issue| issue.id == PRESSED)?;
    stack_merge_choice(pressed, &issues)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    // A stack member always earns the dialog (the test below locks it).
    let Some(choice) = choice() else {
        return div();
    };
    let spec = crate::pr_merge::stack_merge_alert(PRESSED, &choice);
    div().child(crate::native_dialog::alert_specimen(
        "sg-stack-merge-choice",
        spec,
        window,
        cx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(serde::Deserialize)]
    struct Case {
        choice: Option<CaseChoice>,
    }

    #[derive(serde::Deserialize)]
    struct CaseChoice {
        members: Vec<String>,
        position: usize,
        body: String,
    }

    /// The demo's state is the fixture's middle-of-three case, word for
    /// word.
    #[test]
    fn the_demo_is_the_fixtures_middle_of_a_three_stack() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../../../../packages/domain-contract/fixtures/stack-merge-choice.json"
        ))
        .expect("stack-merge-choice.json parses");
        let case = fixture
            .cases
            .iter()
            .filter_map(|case| case.choice.as_ref())
            .find(|choice| choice.position == 2 && choice.members.len() == 3)
            .expect("a middle-of-three case");
        let choice = choice().expect("a stack member asks");
        assert_eq!(choice.members, case.members);
        assert_eq!(choice.position, case.position);
        assert_eq!(choice.body, case.body);
    }
}
