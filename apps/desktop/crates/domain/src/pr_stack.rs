//! PR STACKS, client side only: the pure chain the related-work badge
//! derives its stack band from (SLOP-3 keeps the badge, not the stack system).
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
}
