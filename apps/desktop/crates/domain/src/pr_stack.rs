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

// ---------------------------------------------------------------------------
// EXP-1248: tree vs stack, the stack rail, the ONE confirm
// ---------------------------------------------------------------------------
//
// ×4 off `fixtures/pr-stack-view.json` (`open_pr_shape` + `stack_view`) and
// `fixtures/stack-merge-choice.json` `confirm` (`stack_merge_confirm`): web
// `lib/pr-stack.ts` `openPrShape` / `stackView` / `stackMergeConfirm`.

/// A base-chained component's shape: `Tree` = a fork anywhere (follow-up
/// runs; nests with tree guides), `Stack` = linear (GitHub stacks it; the
/// stack rail), `Single` = one pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrGraphShape {
    Tree,
    Stack,
    Single,
}

impl PrGraphShape {
    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Stack => "stack",
            Self::Single => "single",
        }
    }
}

/// The ghost action on a hovered stack-rail member (contract
/// `diffUi.mergeThrough`; the merge control's own label is
/// [`MERGE_STACK_LABEL`]).
pub const MERGE_THROUGH_LABEL: &str = crate::contract::DIFF_UI_MERGE_THROUGH;
/// The confirm's dismiss button.
pub const STACK_CONFIRM_CANCEL_LABEL: &str = "Cancel";

/// Branch → its FIRST owner in input order (web `ownerMap`).
fn first_owners<'a>(nodes: &[&'a Issue]) -> HashMap<&'a str, &'a Issue> {
    let mut owners: HashMap<&str, &Issue> = HashMap::new();
    for node in nodes {
        if let Some(head) = branch(node.branch.as_deref()) {
            owners.entry(head).or_insert(node);
        }
    }
    owners
}

/// Every node base-chained to `node` (either direction), in input order.
pub fn pr_component<'a>(node: &'a Issue, nodes: &[&'a Issue]) -> Vec<&'a Issue> {
    let owners = first_owners(nodes);
    let mut seen: HashSet<&str> = HashSet::from([node.id.as_str()]);
    let mut queue: std::collections::VecDeque<&Issue> = std::collections::VecDeque::from([node]);
    while let Some(current) = queue.pop_front() {
        let mut linked: Vec<&Issue> = Vec::new();
        if let Some(lower) = branch(current.pr_base_branch.as_deref()).and_then(|b| owners.get(b)) {
            linked.push(lower);
        }
        if let Some(head) = branch(current.branch.as_deref()) {
            linked.extend(
                nodes
                    .iter()
                    .copied()
                    .filter(|candidate| branch(candidate.pr_base_branch.as_deref()) == Some(head)),
            );
        }
        for next in linked {
            if seen.insert(next.id.as_str()) {
                queue.push_back(next);
            }
        }
    }
    nodes
        .iter()
        .copied()
        .filter(|candidate| seen.contains(candidate.id.as_str()))
        .collect()
}

/// Tree (any node with two children), stack (linear) or single (< 2).
pub fn pr_graph_shape(component: &[&Issue]) -> PrGraphShape {
    if component.len() < 2 {
        return PrGraphShape::Single;
    }
    let owners = first_owners(component);
    let mut children: HashMap<&str, usize> = HashMap::new();
    for node in component {
        let Some(parent) = branch(node.pr_base_branch.as_deref()).and_then(|b| owners.get(b))
        else {
            continue;
        };
        if parent.id == node.id {
            continue;
        }
        let count = children.entry(parent.id.as_str()).or_insert(0);
        *count += 1;
        if *count > 1 {
            return PrGraphShape::Tree;
        }
    }
    PrGraphShape::Stack
}

/// Web `stackChain` over already-picked nodes: bottom first, ids as the seen
/// key, a fork above takes the first by identifier.
fn chain_by_id<'a>(issue: &'a Issue, issues: &[&'a Issue]) -> Vec<&'a Issue> {
    let owners = first_owners(issues);
    let mut seen: HashSet<&str> = HashSet::from([issue.id.as_str()]);
    let mut below: Vec<&Issue> = Vec::new();
    let mut current = issue;
    while let Some(base) = branch(current.pr_base_branch.as_deref()) {
        let Some(lower) = owners.get(base).copied() else {
            break;
        };
        if lower.id == current.id || !seen.insert(lower.id.as_str()) {
            break;
        }
        below.insert(0, lower);
        current = lower;
    }
    let mut above: Vec<&Issue> = Vec::new();
    current = issue;
    while let Some(head) = branch(current.branch.as_deref()) {
        let upper = issues
            .iter()
            .copied()
            .filter(|candidate| {
                candidate.id != current.id
                    && branch(candidate.pr_base_branch.as_deref()) == Some(head)
                    && !seen.contains(candidate.id.as_str())
            })
            .min_by(|a, b| a.identifier.cmp(&b.identifier));
        let Some(upper) = upper else {
            break;
        };
        seen.insert(upper.id.as_str());
        above.push(upper);
        current = upper;
    }
    let mut chain = below;
    chain.push(issue);
    chain.extend(above);
    chain
}

/// The OPEN rows (identifier order) with ONE representative per pull request
/// and the subject's representative; `None` without an open PR.
struct OpenPick<'a> {
    open: Vec<&'a Issue>,
    reps: Vec<&'a Issue>,
    subject: &'a Issue,
}

fn open_representatives<'a>(issue: &'a Issue, issues: &'a [Issue]) -> Option<OpenPick<'a>> {
    if issue.pr_state.as_deref() != Some("open") {
        return None;
    }
    let mut open: Vec<&Issue> = issues
        .iter()
        .filter(|row| row.pr_state.as_deref() == Some("open"))
        .collect();
    if !open.iter().any(|row| row.id == issue.id) {
        open.push(issue);
    }
    open.sort_by(|a, b| a.identifier.cmp(&b.identifier));
    let mut reps: Vec<&Issue> = Vec::new();
    let mut subject: Option<&Issue> = None;
    for row in &open {
        let url = row.pr_url.as_deref();
        let rep = url.and_then(|url| reps.iter().copied().find(|other| other.pr_url.as_deref() == Some(url)));
        let rep = match rep {
            Some(rep) => rep,
            None => {
                reps.push(row);
                row
            }
        };
        if row.id == issue.id {
            subject = Some(rep);
        }
    }
    Some(OpenPick {
        open,
        reps,
        subject: subject?,
    })
}

/// The shape of the open component `issue`'s pull request sits in
/// (`Single` without an open pull request).
pub fn open_pr_shape(issue: &Issue, issues: &[Issue]) -> PrGraphShape {
    match open_representatives(issue, issues) {
        Some(pick) => pr_graph_shape(&pr_component(pick.subject, &pick.reps)),
        None => PrGraphShape::Single,
    }
}

/// The linear open stack `issue` sits in: the chain bottom → top, the
/// subject's representative and every open row (for a batch's partners).
struct OpenStack<'a> {
    chain: Vec<&'a Issue>,
    subject: &'a Issue,
    open: Vec<&'a Issue>,
}

impl OpenStack<'_> {
    /// A member's label: `EXP-874 +2` for a batch PR.
    fn label(&self, row: &Issue) -> String {
        let siblings = match row.pr_url.as_deref() {
            Some(url) => self
                .open
                .iter()
                .filter(|other| other.id != row.id && other.pr_url.as_deref() == Some(url))
                .count(),
            None => 0,
        };
        if siblings > 0 {
            format!("{} +{siblings}", row.identifier)
        } else {
            row.identifier.clone()
        }
    }
}

fn open_stack<'a>(issue: &'a Issue, issues: &'a [Issue]) -> Option<OpenStack<'a>> {
    let pick = open_representatives(issue, issues)?;
    if pr_graph_shape(&pr_component(pick.subject, &pick.reps)) != PrGraphShape::Stack {
        return None;
    }
    let chain = chain_by_id(pick.subject, &pick.reps);
    Some(OpenStack {
        chain,
        subject: pick.subject,
        open: pick.open,
    })
}

/// One stack-rail row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackViewRow {
    pub issue_id: String,
    pub identifier: String,
    pub title: String,
    pub pr_number: Option<i64>,
    /// The subject's pull request: the row wears the active wash.
    pub is_current: bool,
}

/// The stack rail: rows TOP first + the trailing base-branch row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackView {
    pub rows: Vec<StackViewRow>,
    /// The bottom member's base; `None` = unknown.
    pub base_branch: Option<String>,
}

/// The stack rail of `issue`'s pull request (the Guide's Stack card, a
/// Reviews stack group): `None` unless it sits in a linear open stack of 2+
/// pull requests.
pub fn stack_view(issue: &Issue, issues: &[Issue]) -> Option<StackView> {
    let stack = open_stack(issue, issues)?;
    if stack.chain.len() < 2 {
        return None;
    }
    Some(StackView {
        rows: stack
            .chain
            .iter()
            .rev()
            .map(|row| StackViewRow {
                issue_id: row.id.clone(),
                identifier: row.identifier.clone(),
                title: row.title.clone(),
                pr_number: row.pr_number,
                is_current: row.id == stack.subject.id,
            })
            .collect(),
        base_branch: branch(stack.chain[0].pr_base_branch.as_deref()).map(str::to_string),
    })
}

/// `Stack` = the merge control (the whole open chain, through its top);
/// `Through` = Merge through here on this member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackConfirmMode {
    Stack,
    Through,
}

/// The ONE confirm a stack merge asks (it replaces the 3-way dialog).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackMergeConfirm {
    /// The title AND the primary button.
    pub title: String,
    /// What lands, bottom first; a batch PR = `EXP-874 +2`.
    pub landing: Vec<String>,
    /// What stays open above it (GitHub retargets it).
    pub stays_open: Vec<String>,
    pub body: String,
    /// `issues.mergePr({issueId, mergeStack: true})`'s issue.
    pub issue_id: String,
}

/// EXP-1248: the confirm for `mode`; `None` = not in a linear open stack (the
/// plain merge confirm).
pub fn stack_merge_confirm(
    issue: &Issue,
    issues: &[Issue],
    mode: StackConfirmMode,
) -> Option<StackMergeConfirm> {
    let stack = open_stack(issue, issues)?;
    if stack.chain.len() < 2 {
        return None;
    }
    let index = stack
        .chain
        .iter()
        .position(|row| row.id == stack.subject.id)?;
    let through = match mode {
        StackConfirmMode::Stack => stack.chain.len() - 1,
        StackConfirmMode::Through => index,
    };
    let landing: Vec<String> = stack.chain[..=through].iter().map(|row| stack.label(row)).collect();
    let stays_open: Vec<String> = stack.chain[through + 1..]
        .iter()
        .map(|row| stack.label(row))
        .collect();
    let lands = if landing.len() == 1 {
        format!("Lands 1 pull request: {}.", landing[0])
    } else {
        format!(
            "Lands {} pull requests, bottom-up: {}.",
            landing.len(),
            landing.join(", ")
        )
    };
    let open = match stays_open.len() {
        0 => String::new(),
        1 => format!(" {} stays open.", stays_open[0]),
        _ => format!(" {} stay open.", stays_open.join(", ")),
    };
    Some(StackMergeConfirm {
        title: match mode {
            StackConfirmMode::Stack => MERGE_STACK_LABEL.to_string(),
            StackConfirmMode::Through => MERGE_THROUGH_LABEL.to_string(),
        },
        landing,
        stays_open,
        body: format!("{lands}{open}"),
        issue_id: match mode {
            StackConfirmMode::Stack => stack.chain[stack.chain.len() - 1].id.clone(),
            StackConfirmMode::Through => issue.id.clone(),
        },
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

    /// The fixture row `pr-stack-view.json` / `stack-merge-choice.json` share.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ViewRow {
        id: String,
        identifier: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        pr_number: Option<i64>,
        branch: Option<String>,
        pr_base_branch: Option<String>,
        pr_state: Option<String>,
        pr_url: Option<String>,
    }

    fn view_issue(row: &ViewRow) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": row.id,
            "board_id": "board-1",
            "number": 1,
            "identifier": row.identifier,
            "title": row.title.clone().unwrap_or_else(|| row.identifier.clone()),
            "status": "in_review",
            "branch": row.branch,
            "pr_base_branch": row.pr_base_branch,
            "pr_url": row.pr_url,
            "pr_state": row.pr_state,
            "pr_number": row.pr_number,
        }))
        .unwrap()
    }

    /// EXP-1248: `pr-stack-view.json` locks `openPrShape` + `stackView` ×4.
    #[test]
    fn stack_view_matches_the_fixture() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            issue: String,
            issues: Vec<ViewRow>,
            shape: String,
            view: Option<View>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct View {
            rows: Vec<Row>,
            base_branch: Option<String>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Row {
            issue_id: String,
            identifier: String,
            title: String,
            pr_number: Option<i64>,
            is_current: bool,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/pr-stack-view.json"
        ))
        .expect("pr-stack-view.json parses");
        assert!(!fixture.cases.is_empty());
        for case in fixture.cases {
            let issues: Vec<Issue> = case.issues.iter().map(view_issue).collect();
            let issue = issues
                .iter()
                .find(|row| row.id == case.issue)
                .unwrap_or_else(|| panic!("{}: the subject is listed", case.name));
            assert_eq!(open_pr_shape(issue, &issues).as_str(), case.shape, "{}", case.name);
            let expected = case.view.map(|view| StackView {
                rows: view
                    .rows
                    .into_iter()
                    .map(|row| StackViewRow {
                        issue_id: row.issue_id,
                        identifier: row.identifier,
                        title: row.title,
                        pr_number: row.pr_number,
                        is_current: row.is_current,
                    })
                    .collect(),
                base_branch: view.base_branch,
            });
            assert_eq!(stack_view(issue, &issues), expected, "{}", case.name);
        }
    }

    /// EXP-1248: a fork anywhere is a tree; a line is a stack (web
    /// `prGraphShape` `tells a fork from a line`).
    #[test]
    fn pr_graph_shape_tells_a_fork_from_a_line() {
        let line = chain_fixture();
        let refs: Vec<&Issue> = line.iter().collect();
        assert_eq!(pr_graph_shape(&refs), PrGraphShape::Stack);
        assert_eq!(pr_graph_shape(&refs[..1]), PrGraphShape::Single);
        let fork = vec![
            issue("EXP-1", Some("a"), Some("master")),
            issue("EXP-2", Some("b"), Some("a")),
            issue("EXP-3", Some("c"), Some("a")),
        ];
        let refs: Vec<&Issue> = fork.iter().collect();
        assert_eq!(pr_graph_shape(&refs), PrGraphShape::Tree);
        // The whole component, from any member.
        let component = pr_component(&fork[2], &refs);
        assert_eq!(component.len(), 3);
    }

    /// EXP-1248: `stack-merge-choice.json` `confirm` locks the ONE confirm ×4.
    #[test]
    fn stack_merge_confirm_matches_the_fixture() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            confirm: Confirm,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Confirm {
            labels: Labels,
            cases: Vec<Case>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Labels {
            merge_stack: String,
            merge_through: String,
            cancel: String,
        }
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            issue: String,
            mode: String,
            issues: Vec<ViewRow>,
            confirm: Option<Expected>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Expected {
            title: String,
            landing: Vec<String>,
            stays_open: Vec<String>,
            body: String,
            input: Input,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            issue_id: String,
            merge_stack: bool,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/stack-merge-choice.json"
        ))
        .expect("stack-merge-choice.json parses");
        assert_eq!(MERGE_STACK_LABEL, fixture.confirm.labels.merge_stack);
        assert_eq!(MERGE_THROUGH_LABEL, fixture.confirm.labels.merge_through);
        assert_eq!(STACK_CONFIRM_CANCEL_LABEL, fixture.confirm.labels.cancel);
        assert!(!fixture.confirm.cases.is_empty());
        for case in fixture.confirm.cases {
            let issues: Vec<Issue> = case.issues.iter().map(view_issue).collect();
            let issue = issues
                .iter()
                .find(|row| row.id == case.issue)
                .unwrap_or_else(|| panic!("{}: the subject is listed", case.name));
            let mode = match case.mode.as_str() {
                "stack" => StackConfirmMode::Stack,
                "through" => StackConfirmMode::Through,
                other => panic!("{}: unknown mode {other}", case.name),
            };
            let expected = case.confirm.map(|confirm| {
                assert!(confirm.input.merge_stack, "{}", case.name);
                StackMergeConfirm {
                    title: confirm.title,
                    landing: confirm.landing,
                    stays_open: confirm.stays_open,
                    body: confirm.body,
                    issue_id: confirm.input.issue_id,
                }
            });
            assert_eq!(stack_merge_confirm(issue, &issues, mode), expected, "{}", case.name);
        }
    }
}
