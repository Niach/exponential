//! EXP-897 — PR STACKS: the pure rule every client derives nesting from.
//!
//! A stack edge is one synced fact: `child.pr_base_branch == lower.branch`
//! (both non-empty), within ONE repository. There is no `pr_stacks` table and
//! no server-computed tree — the clients chain the rows they already sync, so
//! a stack renders the instant the base branch lands.
//!
//! Mirrored ×4 by name (web `lib/pr-stack.ts`, iOS `PrStack.swift`, Android
//! `PrStack.kt`) with the same three tests:
//!
//! 1. `numbers a member from the bottom of the chain` — position is 1-BASED
//!    counted from the bottom (the PR nothing else is based on), `size` is
//!    the whole chain, `below`/`above` name the direct neighbours.
//! 2. `stops at a base nobody in the list owns` — a base branch no listed PR
//!    owns (the repo's default branch, or a merged PR the caller filtered
//!    out) ENDS the walk; it is never rendered as a missing member.
//! 3. `breaks a cycle where it first appears` — defensive: a base chain that
//!    loops stops at the first repeat rather than spinning.
//!
//! Callers scope the input list themselves (one team, and in practice one
//! repository): the rule matches branch NAMES, exactly like the web twin.

use std::collections::{HashMap, HashSet};

use crate::rows::Issue;

// ---------------------------------------------------------------------------
// Copy — byte-locked ×4
// ---------------------------------------------------------------------------

/// The Reviews row's primary control on the BOTTOM member of a stack, and the
/// same entry in the stack overlay.
pub const MERGE_STACK_LABEL: &str = "Merge stack";
/// The two-click armed label of [`MERGE_STACK_LABEL`] (the desktop's Reviews
/// row confirms in place, like every other merge there).
pub const MERGE_STACK_CONFIRM_LABEL: &str = "Confirm merge stack";
/// The confirm dialog's title (web + the desktop overlay, where a popover
/// cannot host a two-click arm).
pub const MERGE_STACK_DIALOG_TITLE: &str = "Merge the whole stack?";
/// The fold chevron's accessible label on a collapsed / expanded parent row.
pub const EXPAND_CHILD_RUNS: &str = "Expand child runs";
pub const COLLAPSE_CHILD_RUNS: &str = "Collapse child runs";

/// The confirm dialog's body: `3 pull requests, bottom-up.`
pub fn merge_stack_dialog_body(pull_requests: usize) -> String {
    format!("{pull_requests} pull requests, bottom-up.")
}

/// The caption an upper stack member carries: `on top of #EXP-11`.
pub fn on_top_of(identifier: &str) -> String {
    format!("on top of #{identifier}")
}

/// The run screen's position line: `2 of 3 · on top of #EXP-11`.
pub fn stack_position_line(position: usize, size: usize, below_identifier: &str) -> String {
    format!("{position} of {size} · {}", on_top_of(below_identifier))
}

/// The sentence under [`stack_position_line`] naming what the PR is based on.
pub fn stack_base_note(below_identifier: &str, default_branch: &str) -> String {
    format!(
        "Your pull request is based on #{below_identifier}'s branch, not on {default_branch}."
    )
}

// ---------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------

/// Where one PR sits in its stack. `None` from [`stack_position`] when the
/// chain is the PR alone — a lone pull request is not a stack.
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

/// A non-empty trimmed field, or `None` — the edge only ever matches real
/// branch names (`""` on both sides must never chain two unrelated rows).
fn branch(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// One issue per BRANCH — a batch PR lands N issues on ONE branch, and a
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

/// Where `issue`'s pull request sits in its stack — `None` for a lone PR
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

/// One row of [`nest_pr_stacks`] — the caller's entry with its nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedEntry<T> {
    pub entry: T,
    /// 0 = the bottom of a stack (or a lone PR), +1 per member above.
    pub depth: usize,
    /// At least one member is nested directly below this row.
    pub has_children: bool,
}

/// Nest a list of PR entries into stack order: every entry follows the one it
/// is based on, indented one level deeper. Roots keep the CALLER's order; a
/// base nobody in the list owns leaves its entry a root; a cycle breaks where
/// it first repeats.
///
/// `branch_of` / `base_of` read the two columns off whatever entry the caller
/// holds — a single issue on the natives, a `ReviewEntry` (a PR, so possibly
/// a whole batch) on the Reviews page.
pub fn nest_pr_stacks<T, B, A>(entries: Vec<T>, branch_of: B, base_of: A) -> Vec<NestedEntry<T>>
where
    B: Fn(&T) -> Option<&str>,
    A: Fn(&T) -> Option<&str>,
{
    // branch → the index of the entry that owns it (first listed wins).
    let mut owner_of: HashMap<String, usize> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(head) = branch(branch_of(entry)) {
            owner_of.entry(head.to_string()).or_insert(index);
        }
    }
    let parent_of: Vec<Option<usize>> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            branch(base_of(entry))
                .and_then(|base| owner_of.get(base).copied())
                .filter(|parent| *parent != index)
        })
        .collect();
    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, parent) in parent_of.iter().enumerate() {
        if let Some(parent) = parent {
            children_of.entry(*parent).or_default().push(index);
        }
    }

    let mut order: Vec<(usize, usize, bool)> = Vec::with_capacity(entries.len());
    let mut placed = vec![false; entries.len()];
    fn visit(
        index: usize,
        depth: usize,
        children_of: &HashMap<usize, Vec<usize>>,
        placed: &mut [bool],
        order: &mut Vec<(usize, usize, bool)>,
    ) {
        if placed[index] {
            return;
        }
        placed[index] = true;
        let children: Vec<usize> = children_of
            .get(&index)
            .map(|kids| kids.iter().copied().filter(|kid| !placed[*kid]).collect())
            .unwrap_or_default();
        order.push((index, depth, !children.is_empty()));
        for child in children {
            visit(child, depth + 1, children_of, placed, order);
        }
    }
    for index in 0..entries.len() {
        if parent_of[index].is_none() {
            visit(index, 0, &children_of, &mut placed, &mut order);
        }
    }
    // A chain that cycled without ever reaching a root — keep every row, at
    // depth 0 from where the cycle first repeats.
    for index in 0..entries.len() {
        visit(index, 0, &children_of, &mut placed, &mut order);
    }

    let mut slots: Vec<Option<T>> = entries.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|(index, depth, has_children)| NestedEntry {
            entry: slots[index].take().expect("each index placed once"),
            depth,
            has_children,
        })
        .collect()
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
        // And the copy off it is byte-locked.
        assert_eq!(
            stack_position_line(middle.position, middle.size, middle.below.as_deref().unwrap()),
            "2 of 3 · on top of #EXP-11"
        );
        assert_eq!(on_top_of("ABC-12"), "on top of #ABC-12");
        assert_eq!(
            stack_base_note("EXP-11", "master"),
            "Your pull request is based on #EXP-11's branch, not on master."
        );
        assert_eq!(merge_stack_dialog_body(3), "3 pull requests, bottom-up.");
    }

    /// Byte-locked ×4 (web `pr-stack.ts`, iOS `PrStack.swift`, Android
    /// `PrStack.kt`): the same words in every client's Reviews list, confirm
    /// dialog and fold chevron.
    #[test]
    fn the_stack_copy_is_byte_locked() {
        assert_eq!(MERGE_STACK_LABEL, "Merge stack");
        assert_eq!(MERGE_STACK_CONFIRM_LABEL, "Confirm merge stack");
        assert_eq!(MERGE_STACK_DIALOG_TITLE, "Merge the whole stack?");
        assert_eq!(merge_stack_dialog_body(2), "2 pull requests, bottom-up.");
        assert_eq!(EXPAND_CHILD_RUNS, "Expand child runs");
        assert_eq!(COLLAPSE_CHILD_RUNS, "Collapse child runs");
        assert_eq!(on_top_of("ABC-12"), "on top of #ABC-12");
        assert_eq!(stack_position_line(2, 3, "ABC-12"), "2 of 3 · on top of #ABC-12");
    }

    #[test]
    fn stops_at_a_base_nobody_in_the_list_owns() {
        // `master` is nobody's head branch — the walk stops there, and the
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
        // Nesting the same pair keeps BOTH rows, never loops.
        let nested = nest_pr_stacks(
            issues.clone(),
            |issue: &Issue| issue.branch.as_deref(),
            |issue: &Issue| issue.pr_base_branch.as_deref(),
        );
        assert_eq!(nested.len(), 2);
        assert_eq!(nested.iter().map(|row| row.depth).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn nest_pr_stacks_indents_every_member_under_its_base() {
        // Caller order is deliberately scrambled; roots keep it.
        let issues = vec![
            issue("EXP-13", Some("exp/EXP-13"), Some("exp/EXP-12")),
            issue("EXP-99", Some("exp/EXP-99"), Some("master")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
        ];
        let rows = nest_pr_stacks(
            issues,
            |issue: &Issue| issue.branch.as_deref(),
            |issue: &Issue| issue.pr_base_branch.as_deref(),
        );
        let shape: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    "{}@{}{}",
                    row.entry.identifier,
                    row.depth,
                    if row.has_children { "+" } else { "" }
                )
            })
            .collect();
        assert_eq!(shape, ["EXP-99@0", "EXP-11@0+", "EXP-12@1+", "EXP-13@2"]);
    }
}
