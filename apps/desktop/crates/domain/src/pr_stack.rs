//! PR STACKS, client side only: the pure chain the related-work badge
//! derives its stack band from (SLOP-3 keeps the badge, not the stack system),
//! and the stack merge choice a Merge control asks first ([`stack_merge_choice`]).
//!
//! A stack edge is one synced fact: `child.pr_base_branch == lower.branch`
//! (both non-empty), within ONE repository. There is no `pr_stacks` table and
//! no server-computed tree: the clients chain the rows they already sync.
//!
//! 1. `numbers a member from the bottom of the chain`: position is 1-BASED
//!    counted from the bottom (the PR nothing else is based on), `size` is
//!    the whole chain, `below`/`above` name the direct neighbours.
//! 2. `stops at a base nobody in the list owns`: a base branch no listed PR
//!    owns (the repo's default branch, or a merged PR the caller filtered
//!    out) ENDS the walk; it is never rendered as a missing member.
//! 3. `breaks a cycle where it first appears`: defensive, a base chain that
//!    loops stops at the first repeat rather than spinning.
//!
//! Callers scope the input list themselves (one team, and in practice one
//! repository): the rule matches branch NAMES.

use std::collections::{HashMap, HashSet};

use crate::rows::Issue;

// ---------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------

/// Where one PR sits in its stack. `None` from [`stack_position`] when the
/// chain is the PR alone: a lone pull request is not a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackPosition {
    /// 1-based, counted from the BOTTOM of the chain.
    pub position: usize,
    /// How many pull requests the whole chain holds.
    pub size: usize,
    /// The identifier of the PR directly below (the one this is based on).
    pub below: Option<String>,
    /// The identifier of the PR directly above (the one based on this).
    pub above: Option<String>,
}

/// A non-empty trimmed field, or `None`: the edge only ever matches real
/// branch names (`""` on both sides must never chain two unrelated rows).
fn branch(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// One issue per BRANCH: a batch PR lands N issues on ONE branch, and a
/// stack member is a PULL REQUEST, not an issue. First listed wins (callers
/// hand us their own order, newest first).
fn owners_by_branch<'a>(issues: &'a [Issue]) -> HashMap<&'a str, &'a Issue> {
    let mut owners: HashMap<&str, &Issue> = HashMap::new();
    for issue in issues {
        if let Some(head) = branch(issue.branch.as_deref()) {
            owners.entry(head).or_insert(issue);
        }
    }
    owners
}

/// The whole chain `issue` belongs to, BOTTOM first, one entry per pull
/// request (the representative issue of each). Walks down through
/// `pr_base_branch` until a base nobody in `issues` owns, then up through the
/// PRs based on each branch. Cycle-safe both ways.
pub fn stack_chain<'a>(issue: &'a Issue, issues: &'a [Issue]) -> Vec<&'a Issue> {
    let owners = owners_by_branch(issues);
    // Down: follow the base branch while a listed PR owns it.
    let mut below: Vec<&Issue> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    if let Some(head) = branch(issue.branch.as_deref()) {
        seen.insert(head);
    }
    let mut cursor = issue;
    while let Some(base) = branch(cursor.pr_base_branch.as_deref()) {
        let Some(lower) = owners.get(base).copied() else {
            break;
        };
        if !seen.insert(base) {
            // A cycle: it breaks where it first repeats.
            break;
        }
        below.push(lower);
        cursor = lower;
    }
    below.reverse();

    // Up: the PR whose base is this branch, recursively.
    let mut above: Vec<&Issue> = Vec::new();
    let mut cursor = issue;
    loop {
        let Some(head) = branch(cursor.branch.as_deref()) else {
            break;
        };
        let Some(upper) = owners
            .values()
            .copied()
            .filter(|candidate| branch(candidate.pr_base_branch.as_deref()) == Some(head))
            // Deterministic when a branch was (wrongly) based on twice.
            .min_by(|a, b| a.identifier.cmp(&b.identifier))
        else {
            break;
        };
        let Some(upper_head) = branch(upper.branch.as_deref()) else {
            break;
        };
        if !seen.insert(upper_head) {
            break;
        }
        above.push(upper);
        cursor = upper;
    }

    let mut chain = below;
    chain.push(issue);
    chain.extend(above);
    chain
}

/// Where `issue`'s pull request sits in its stack: `None` for a lone PR
/// (nothing below it, nothing on top).
pub fn stack_position(issue: &Issue, issues: &[Issue]) -> Option<StackPosition> {
    let chain = stack_chain(issue, issues);
    if chain.len() < 2 {
        return None;
    }
    let index = chain
        .iter()
        .position(|member| member.id == issue.id)
        // A batch member is represented by another of its issues: fall back
        // to the branch it shares.
        .or_else(|| {
            let head = branch(issue.branch.as_deref())?;
            chain
                .iter()
                .position(|member| branch(member.branch.as_deref()) == Some(head))
        })?;
    Some(StackPosition {
        position: index + 1,
        size: chain.len(),
        below: index
            .checked_sub(1)
            .map(|lower| chain[lower].identifier.clone()),
        above: chain.get(index + 1).map(|up| up.identifier.clone()),
    })
}

// ---------------------------------------------------------------------------
// EXP-1145: the stack merge choice
// ---------------------------------------------------------------------------
//
// A Merge control on a stack member with another OPEN member asks first:
// merge the whole stack, or through this pull request. Mirrored x4 by name
// (web `lib/pr-stack.ts` `stackMergeChoice`, iOS `PrStack.stackMergeChoice`,
// Android `PrStack.stackMergeChoice`), locked by
// `packages/domain-contract/fixtures/stack-merge-choice.json`.

/// The dialog's title.
pub const STACK_MERGE_CHOICE_TITLE: &str = "This pull request is part of a stack";
/// The dialog's primary button: `issues.mergePr({issueId: top, mergeStack})`.
pub const MERGE_STACK_LABEL: &str = "Merge stack";
/// The dialog's secondary button: this pull request (and what lies below it).
pub const MERGE_THIS_PR_LABEL: &str = "Merge this pull request";
/// The dialog's dismiss button.
pub const STACK_MERGE_CANCEL_LABEL: &str = "Cancel";

const THIS_ONE: &str = "this one";

/// Everything the stack merge dialog says (EXP-1145).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMergeChoice {
    /// The chain's OPEN members, bottom to top, one label per pull request
    /// (`EXP-874 +2` for a batch PR).
    pub members: Vec<String>,
    /// 1-based, from the bottom: where the pull request being merged sits.
    pub position: usize,
    /// The bottom member's issue id.
    pub bottom_issue_id: String,
    /// The top member's issue id: what `mergePr({ mergeStack: true })` takes.
    pub top_issue_id: String,
    /// `EXP-1105 → EXP-1144 (this one) → EXP-1150`.
    pub listing: String,
    /// What Merge stack does.
    pub stack_sentence: String,
    /// What Merge this pull request does.
    pub this_sentence: String,
    /// The dialog's body: the listing, a blank line, the two sentences.
    pub body: String,
}

impl StackMergeChoice {
    /// Whether the merged pull request is the chain's bottom member: its
    /// "Merge this pull request" is then the plain single-PR merge, any
    /// other member merges the chain bottom-up THROUGH itself.
    pub fn is_bottom(&self) -> bool {
        self.position == 1
    }
}

/// Whether merging `issue`'s pull request needs the stack dialog, and
/// everything that dialog says. `None` = a plain merge: the issue has no open
/// pull request, or its chain has no OTHER open member. Only OPEN pull
/// requests form the chain, read in identifier order so every client picks
/// the same representative for a fork or a batch.
pub fn stack_merge_choice(issue: &Issue, issues: &[Issue]) -> Option<StackMergeChoice> {
    if issue.pr_state.as_deref() != Some("open") {
        return None;
    }
    let mut open: Vec<Issue> = issues
        .iter()
        .filter(|row| row.pr_state.as_deref() == Some("open"))
        .cloned()
        .collect();
    open.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    if !open.iter().any(|row| row.id == issue.id) {
        open.insert(0, issue.clone());
    }
    let chain = stack_chain(issue, &open);
    if chain.len() < 2 {
        return None;
    }
    let index = chain.iter().position(|member| member.id == issue.id)?;

    // A batch PR's siblings share its url: one label per pull request.
    let label = |member: &Issue| -> String {
        let siblings = match member.pr_url.as_deref() {
            Some(url) => open
                .iter()
                .filter(|row| row.id != member.id && row.pr_url.as_deref() == Some(url))
                .count(),
            None => 0,
        };
        if siblings > 0 {
            format!("{} +{siblings}", member.identifier)
        } else {
            member.identifier.clone()
        }
    };
    let members: Vec<String> = chain.iter().map(|member| label(member)).collect();
    let own = &members[index];
    let below = &members[..index];
    let above = &members[index + 1..];

    let listing = members
        .iter()
        .enumerate()
        .map(|(at, name)| {
            if at == index {
                format!("{name} ({THIS_ONE})")
            } else {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" \u{2192} ");
    let stack_sentence = format!(
        "{MERGE_STACK_LABEL} lands all {} pull requests, bottom-up.",
        members.len()
    );
    let lands_below = match below.len() {
        0 => format!("{own} alone"),
        1 => format!("{own} and the one below it ({})", below.join(", ")),
        n => format!("{own} and the {n} below it ({})", below.join(", ")),
    };
    let left_open = match above.len() {
        0 => ", the whole stack.".to_string(),
        1 => format!(
            "; {} is retargeted onto the base branch and stays open.",
            above.join(", ")
        ),
        _ => format!(
            "; {} are retargeted onto the base branch and stay open.",
            above.join(", ")
        ),
    };
    let this_sentence = format!("{MERGE_THIS_PR_LABEL} lands {lands_below}{left_open}");
    let body = format!("{listing}\n\n{stack_sentence}\n{this_sentence}");

    Some(StackMergeChoice {
        members,
        position: index + 1,
        bottom_issue_id: chain[0].id.clone(),
        top_issue_id: chain[chain.len() - 1].id.clone(),
        listing,
        stack_sentence,
        this_sentence,
        body,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal issue row with the three columns the rule reads.
    pub(crate) fn issue(identifier: &str, head: Option<&str>, base: Option<&str>) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": format!("id-{identifier}"),
            "board_id": "board-1",
            "number": 1,
            "identifier": identifier,
            "title": identifier,
            "status": "in_progress",
            "branch": head,
            "pr_base_branch": base,
            "pr_url": head.map(|head| format!("https://github.com/o/r/pull/{head}")),
            "pr_state": head.map(|_| "open"),
        }))
        .unwrap()
    }

    /// EXP-11 sits on main, EXP-12 on EXP-11's branch, EXP-13 on EXP-12's.
    fn chain_fixture() -> Vec<Issue> {
        vec![
            issue("EXP-13", Some("exp/EXP-13"), Some("exp/EXP-12")),
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]
    }

    #[test]
    fn numbers_a_member_from_the_bottom_of_the_chain() {
        let issues = chain_fixture();
        let middle = stack_position(&issues[1], &issues).unwrap();
        assert_eq!(middle.position, 2);
        assert_eq!(middle.size, 3);
        assert_eq!(middle.below.as_deref(), Some("EXP-11"));
        assert_eq!(middle.above.as_deref(), Some("EXP-13"));
        let bottom = stack_position(&issues[2], &issues).unwrap();
        assert_eq!((bottom.position, bottom.size), (1, 3));
        assert_eq!(bottom.below, None);
        assert_eq!(bottom.above.as_deref(), Some("EXP-12"));
        let top = stack_position(&issues[0], &issues).unwrap();
        assert_eq!((top.position, top.size), (3, 3));
        assert_eq!(top.above, None);
        // The chain always reads bottom-first, whoever asked.
        let chain: Vec<&str> = stack_chain(&issues[0], &issues)
            .iter()
            .map(|issue| issue.identifier.as_str())
            .collect();
        assert_eq!(chain, ["EXP-11", "EXP-12", "EXP-13"]);
    }

    #[test]
    fn stops_at_a_base_nobody_in_the_list_owns() {
        // `master` is nobody's head branch: the walk stops there, and the
        // bottom member is not "missing its base".
        let issues = chain_fixture();
        let only_top = vec![issues[0].clone()];
        assert_eq!(stack_position(&issues[0], &only_top), None);
        assert_eq!(stack_chain(&issues[0], &only_top).len(), 1);
        // A lone PR straight on the default branch is no stack either.
        let lone = vec![issue("EXP-20", Some("exp/EXP-20"), Some("master"))];
        assert_eq!(stack_position(&lone[0], &lone), None);
        // An EMPTY base never chains two unrelated rows.
        let blanks = vec![
            issue("EXP-30", Some("exp/EXP-30"), Some("")),
            issue("EXP-31", Some(""), Some("")),
        ];
        assert_eq!(stack_position(&blanks[0], &blanks), None);
    }

    #[test]
    fn breaks_a_cycle_where_it_first_appears() {
        let issues = vec![
            issue("EXP-40", Some("a"), Some("b")),
            issue("EXP-41", Some("b"), Some("a")),
        ];
        let chain: Vec<&str> = stack_chain(&issues[0], &issues)
            .iter()
            .map(|issue| issue.identifier.as_str())
            .collect();
        assert_eq!(chain, ["EXP-41", "EXP-40"]);
        let position = stack_position(&issues[0], &issues).unwrap();
        assert_eq!((position.position, position.size), (2, 2));
    }

    /// A fixture row with every column [`stack_merge_choice`] reads.
    fn stack_row(
        id: &str,
        identifier: &str,
        head: Option<&str>,
        base: Option<&str>,
        pr_state: Option<&str>,
        pr_url: Option<&str>,
    ) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "board_id": "board-1",
            "number": 1,
            "identifier": identifier,
            "title": identifier,
            "status": "in_progress",
            "branch": head,
            "pr_base_branch": base,
            "pr_url": pr_url,
            "pr_state": pr_state,
        }))
        .unwrap()
    }

    /// EXP-1145: `stack-merge-choice.json` locks the dialog x4.
    #[test]
    fn stack_merge_choice_matches_the_fixture() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            labels: Labels,
            cases: Vec<Case>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Labels {
            title: String,
            merge_stack: String,
            merge_this: String,
            cancel: String,
        }
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            issue: String,
            issues: Vec<Row>,
            choice: Option<Choice>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Row {
            id: String,
            identifier: String,
            branch: Option<String>,
            pr_base_branch: Option<String>,
            pr_state: Option<String>,
            pr_url: Option<String>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Choice {
            members: Vec<String>,
            position: usize,
            bottom_issue_id: String,
            top_issue_id: String,
            listing: String,
            stack_sentence: String,
            this_sentence: String,
            body: String,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/stack-merge-choice.json"
        ))
        .expect("stack-merge-choice.json parses");
        assert_eq!(STACK_MERGE_CHOICE_TITLE, fixture.labels.title);
        assert_eq!(MERGE_STACK_LABEL, fixture.labels.merge_stack);
        assert_eq!(MERGE_THIS_PR_LABEL, fixture.labels.merge_this);
        assert_eq!(STACK_MERGE_CANCEL_LABEL, fixture.labels.cancel);
        assert!(!fixture.cases.is_empty());

        for case in fixture.cases {
            let issues: Vec<Issue> = case
                .issues
                .iter()
                .map(|row| {
                    stack_row(
                        &row.id,
                        &row.identifier,
                        row.branch.as_deref(),
                        row.pr_base_branch.as_deref(),
                        row.pr_state.as_deref(),
                        row.pr_url.as_deref(),
                    )
                })
                .collect();
            let issue = issues
                .iter()
                .find(|row| row.id == case.issue)
                .unwrap_or_else(|| panic!("{}: the merged issue is listed", case.name));
            let actual = stack_merge_choice(issue, &issues);
            let expected = case.choice.map(|choice| StackMergeChoice {
                members: choice.members,
                position: choice.position,
                bottom_issue_id: choice.bottom_issue_id,
                top_issue_id: choice.top_issue_id,
                listing: choice.listing,
                stack_sentence: choice.stack_sentence,
                this_sentence: choice.this_sentence,
                body: choice.body,
            });
            assert_eq!(actual, expected, "{}", case.name);
        }
    }
}
