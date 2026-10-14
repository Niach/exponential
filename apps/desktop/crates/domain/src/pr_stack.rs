//! PR STACKS, client side only: the pure chain the related-work badge and
//! the stack rail derive from, and the ONE confirm a stack merge asks first
//! ([`stack_merge_confirm`]).
//!
//! EXP-1294: a stack member is a synced [`PullRequestRow`] (all PR state
//! left `issues`). The edge is EXPLICIT: `upper.base_pull_request_id ==
//! lower.id`; a row without it (or naming a PR outside the list) falls back
//! to `upper.base == lower.head` within ONE repository (both non-empty). The
//! issues a PR closes come from `pull_request_issues` ([`PrSet`]): a PR that
//! closes several reads as ONE member named by its lowest identifier
//! (`EXP-874 +2` in the confirm).
//!
//! 1. `numbers a member from the bottom of the chain`: position is 1-BASED
//!    counted from the bottom, `size` is the whole chain, `below`/`above`
//!    name the direct neighbours.
//! 2. `stops at a base nobody in the list owns`: a base no listed PR owns
//!    (the default branch, or a merged PR the caller filtered out) ENDS the
//!    walk; it is never rendered as a missing member.
//! 3. `breaks a cycle where it first appears`: defensive.
//!
//! Callers scope the input themselves (one team).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::rows::{Issue, PullRequestIssueRow, PullRequestRow};

// ---------------------------------------------------------------------------
// The synced PR world
// ---------------------------------------------------------------------------

/// The pull requests, their issue links and the issues they name. All three
/// are the caller's synced rows (one team).
#[derive(Debug, Clone, Copy)]
pub struct PrSet<'a> {
    pub pulls: &'a [&'a PullRequestRow],
    pub links: &'a [&'a PullRequestIssueRow],
    pub issues: &'a [&'a Issue],
}

impl<'a> PrSet<'a> {
    /// The issues `pr` closes, identifier order (unsynced issues drop out).
    pub fn issues_of(&self, pr: &PullRequestRow) -> Vec<&'a Issue> {
        let ids: HashSet<&str> = self
            .links
            .iter()
            .filter(|link| link.pull_request_id == pr.id)
            .map(|link| link.issue_id.as_str())
            .collect();
        let mut issues: Vec<&'a Issue> = self
            .issues
            .iter()
            .copied()
            .filter(|issue| ids.contains(issue.id.as_str()))
            .collect();
        issues.sort_by(|a, b| a.identifier.cmp(&b.identifier));
        issues
    }

    /// The issue that names `pr` (its lowest identifier); `None` for an
    /// issue-less PR.
    pub fn representative(&self, pr: &PullRequestRow) -> Option<&'a Issue> {
        self.issues_of(pr).into_iter().next()
    }

    /// The PR's short name: its representative's identifier, else `#<n>`.
    pub fn name(&self, pr: &PullRequestRow) -> String {
        match self.representative(pr) {
            Some(issue) => issue.identifier.clone(),
            None => pr.number.map(|number| format!("#{number}")).unwrap_or_default(),
        }
    }

    /// [`Self::name`] plus ` +N` for the other issues it closes.
    pub fn label(&self, pr: &PullRequestRow) -> String {
        let others = self.issues_of(pr).len().saturating_sub(1);
        if others > 0 {
            format!("{} +{others}", self.name(pr))
        } else {
            self.name(pr)
        }
    }

    /// Every PR linked to `issue_id`, input order.
    pub fn pull_requests_of_issue(&self, issue_id: &str) -> Vec<&'a PullRequestRow> {
        let ids: HashSet<&str> = self
            .links
            .iter()
            .filter(|link| link.issue_id == issue_id)
            .map(|link| link.pull_request_id.as_str())
            .collect();
        self.pulls
            .iter()
            .copied()
            .filter(|pr| ids.contains(pr.id.as_str()))
            .collect()
    }

    /// THE pull request of an issue: the newest OPEN one, else the newest
    /// (`created_at`, ISO strings compare chronologically).
    pub fn issue_pull_request(&self, issue_id: &str) -> Option<&'a PullRequestRow> {
        let mut pulls = self.pull_requests_of_issue(issue_id);
        pulls.sort_by(|a, b| {
            b.is_open()
                .cmp(&a.is_open())
                .then_with(|| b.created_at.cmp(&a.created_at))
                .then_with(|| b.number.cmp(&a.number))
        });
        pulls.into_iter().next()
    }

    /// The PR with this id.
    pub fn pull(&self, id: &str) -> Option<&'a PullRequestRow> {
        self.pulls.iter().copied().find(|pr| pr.id == id)
    }
}

// ---------------------------------------------------------------------------
// The edge
// ---------------------------------------------------------------------------

/// A non-empty trimmed field, or `None`: the fallback edge only ever matches
/// real branch names.
fn branch(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The PR `pr` is based on, among `pulls`: the explicit
/// `base_pull_request_id`, else the first PR (input order) of the same
/// repository whose head is `pr.base`.
pub fn lower_of<'a>(pr: &PullRequestRow, pulls: &[&'a PullRequestRow]) -> Option<&'a PullRequestRow> {
    if let Some(explicit) = branch(pr.base_pull_request_id.as_deref())
        .and_then(|id| pulls.iter().copied().find(|other| other.id == id && other.id != pr.id))
    {
        return Some(explicit);
    }
    let base = branch(pr.base.as_deref())?;
    pulls.iter().copied().find(|other| {
        other.id != pr.id
            && other.repository_id == pr.repository_id
            && branch(other.head.as_deref()) == Some(base)
    })
}

/// The PRs based directly on `pr`, input order.
pub fn uppers_of<'a>(pr: &PullRequestRow, pulls: &[&'a PullRequestRow]) -> Vec<&'a PullRequestRow> {
    pulls
        .iter()
        .copied()
        .filter(|candidate| lower_of(candidate, pulls).is_some_and(|lower| lower.id == pr.id))
        .collect()
}

// ---------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------

/// Where one PR sits in its stack. `None` from [`stack_position`] when the
/// chain is the PR alone: a lone pull request is not a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackPosition {
    /// 1-based, counted from the BOTTOM of the chain.
    pub position: usize,
    /// How many pull requests the whole chain holds.
    pub size: usize,
    /// The name of the PR directly below (the one this is based on).
    pub below: Option<String>,
    /// The name of the PR directly above (the one based on this).
    pub above: Option<String>,
}

/// The whole chain `pr` belongs to, BOTTOM first: down through the base
/// until a base nobody in `pulls` owns, then up through the PRs based on
/// each (a fork above takes the lowest number). Cycle-safe both ways.
pub fn stack_chain<'a>(pr: &'a PullRequestRow, pulls: &[&'a PullRequestRow]) -> Vec<&'a PullRequestRow> {
    let mut seen: HashSet<&str> = HashSet::from([pr.id.as_str()]);
    let mut below: Vec<&PullRequestRow> = Vec::new();
    let mut cursor = pr;
    while let Some(lower) = lower_of(cursor, pulls) {
        if !seen.insert(lower.id.as_str()) {
            break;
        }
        below.insert(0, lower);
        cursor = lower;
    }
    let mut above: Vec<&PullRequestRow> = Vec::new();
    cursor = pr;
    loop {
        let upper = uppers_of(cursor, pulls)
            .into_iter()
            .filter(|candidate| !seen.contains(candidate.id.as_str()))
            .min_by(|a, b| a.number.cmp(&b.number).then_with(|| a.id.cmp(&b.id)));
        let Some(upper) = upper else {
            break;
        };
        seen.insert(upper.id.as_str());
        above.push(upper);
        cursor = upper;
    }
    let mut chain = below;
    chain.push(pr);
    chain.extend(above);
    chain
}

/// Where `pr` sits in its stack: `None` for a lone PR.
pub fn stack_position(pr: &PullRequestRow, set: &PrSet<'_>) -> Option<StackPosition> {
    let subject = set.pull(&pr.id).unwrap_or(pr);
    let chain = stack_chain(subject, set.pulls);
    if chain.len() < 2 {
        return None;
    }
    let index = chain.iter().position(|member| member.id == pr.id)?;
    Some(StackPosition {
        position: index + 1,
        size: chain.len(),
        below: index.checked_sub(1).map(|lower| set.name(chain[lower])),
        above: chain.get(index + 1).map(|upper| set.name(upper)),
    })
}

/// The merge control's label on an open-stack member.
pub const MERGE_STACK_LABEL: &str = crate::contract::DIFF_UI_MERGE_STACK;

// ---------------------------------------------------------------------------
// EXP-1248: tree vs stack, the stack rail, the ONE confirm
// ---------------------------------------------------------------------------
//
// ×4 off `fixtures/pr-stack-view.json` (`open_pr_shape` + `stack_view`) and
// `fixtures/stack-merge-choice.json` `confirm` (`stack_merge_confirm`).

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

/// Every PR base-chained to `pr` (either direction), in input order.
pub fn pr_component<'a>(pr: &'a PullRequestRow, pulls: &[&'a PullRequestRow]) -> Vec<&'a PullRequestRow> {
    let mut seen: HashSet<&str> = HashSet::from([pr.id.as_str()]);
    let mut queue: VecDeque<&PullRequestRow> = VecDeque::from([pr]);
    while let Some(current) = queue.pop_front() {
        let mut linked: Vec<&PullRequestRow> = lower_of(current, pulls).into_iter().collect();
        linked.extend(uppers_of(current, pulls));
        for next in linked {
            if seen.insert(next.id.as_str()) {
                queue.push_back(next);
            }
        }
    }
    pulls
        .iter()
        .copied()
        .filter(|candidate| seen.contains(candidate.id.as_str()))
        .collect()
}

/// Tree (any PR with two PRs on it), stack (linear) or single (< 2).
pub fn pr_graph_shape(component: &[&PullRequestRow]) -> PrGraphShape {
    if component.len() < 2 {
        return PrGraphShape::Single;
    }
    let mut children: HashMap<&str, usize> = HashMap::new();
    for pr in component {
        let Some(parent) = lower_of(pr, component) else {
            continue;
        };
        let count = children.entry(parent.id.as_str()).or_insert(0);
        *count += 1;
        if *count > 1 {
            return PrGraphShape::Tree;
        }
    }
    PrGraphShape::Stack
}

/// The OPEN pull requests (input order) with `pr` itself; `None` unless `pr`
/// is open.
fn open_pulls<'a>(pr: &'a PullRequestRow, set: &PrSet<'a>) -> Option<(Vec<&'a PullRequestRow>, &'a PullRequestRow)> {
    if !pr.is_open() {
        return None;
    }
    let mut open: Vec<&PullRequestRow> = set.pulls.iter().copied().filter(|row| row.is_open()).collect();
    let subject = match open.iter().copied().find(|row| row.id == pr.id) {
        Some(subject) => subject,
        None => {
            open.push(pr);
            pr
        }
    };
    Some((open, subject))
}

/// The shape of the open component `pr` sits in (`Single` when `pr` is not
/// open).
pub fn open_pr_shape(pr: &PullRequestRow, set: &PrSet<'_>) -> PrGraphShape {
    match open_pulls(pr, set) {
        Some((open, subject)) => pr_graph_shape(&pr_component(subject, &open)),
        None => PrGraphShape::Single,
    }
}

/// The linear open stack `pr` sits in, bottom → top, and its subject row.
fn open_stack<'a>(pr: &'a PullRequestRow, set: &PrSet<'a>) -> Option<(Vec<&'a PullRequestRow>, &'a PullRequestRow)> {
    let (open, subject) = open_pulls(pr, set)?;
    if pr_graph_shape(&pr_component(subject, &open)) != PrGraphShape::Stack {
        return None;
    }
    Some((stack_chain(subject, &open), subject))
}

/// One stack-rail row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackViewRow {
    pub pull_request_id: String,
    /// The representative issue (`None` = an issue-less PR).
    pub issue_id: Option<String>,
    /// The representative's identifier, else `#<n>`.
    pub identifier: String,
    /// The representative's title, else the PR's own title.
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

/// The stack rail of `pr` (the Guide's Stack card): `None` unless it sits in
/// a linear open stack of 2+ pull requests.
pub fn stack_view(pr: &PullRequestRow, set: &PrSet<'_>) -> Option<StackView> {
    let (chain, subject) = open_stack(pr, set)?;
    if chain.len() < 2 {
        return None;
    }
    Some(StackView {
        rows: chain
            .iter()
            .rev()
            .map(|row| {
                let issue = set.representative(row);
                StackViewRow {
                    pull_request_id: row.id.clone(),
                    issue_id: issue.map(|issue| issue.id.clone()),
                    identifier: set.name(row),
                    title: issue
                        .map(|issue| issue.title.clone())
                        .or_else(|| row.title.clone())
                        .unwrap_or_default(),
                    pr_number: row.number,
                    is_current: row.id == subject.id,
                }
            })
            .collect(),
        base_branch: branch(chain[0].base.as_deref()).map(str::to_string),
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
    /// What lands, bottom first; a PR closing several issues = `EXP-874 +2`.
    pub landing: Vec<String>,
    /// What stays open above it (GitHub retargets it).
    pub stays_open: Vec<String>,
    pub body: String,
    /// `pullRequests.merge({pullRequestId, stack: true})`'s pull request.
    pub pull_request_id: String,
}

/// EXP-1248: the confirm for `mode`; `None` = not in a linear open stack (the
/// plain merge confirm).
pub fn stack_merge_confirm(
    pr: &PullRequestRow,
    set: &PrSet<'_>,
    mode: StackConfirmMode,
) -> Option<StackMergeConfirm> {
    let (chain, subject) = open_stack(pr, set)?;
    if chain.len() < 2 {
        return None;
    }
    let index = chain.iter().position(|row| row.id == subject.id)?;
    let through = match mode {
        StackConfirmMode::Stack => chain.len() - 1,
        StackConfirmMode::Through => index,
    };
    let landing: Vec<String> = chain[..=through].iter().map(|row| set.label(row)).collect();
    let stays_open: Vec<String> = chain[through + 1..].iter().map(|row| set.label(row)).collect();
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
        pull_request_id: chain[through].id.clone(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A test PR world built from issue-shaped rows (the fixtures' form):
    /// rows sharing a PR url are ONE pull request closing all of them.
    pub(crate) struct World {
        pub pulls: Vec<PullRequestRow>,
        pub links: Vec<PullRequestIssueRow>,
        pub issues: Vec<Issue>,
    }

    /// One issue-shaped row: identifier, head, base, url, state, number,
    /// title.
    pub(crate) struct Spec<'a> {
        pub id: &'a str,
        pub identifier: &'a str,
        pub title: Option<&'a str>,
        pub head: Option<&'a str>,
        pub base: Option<&'a str>,
        pub url: Option<&'a str>,
        pub state: Option<&'a str>,
        pub number: Option<i64>,
    }

    impl World {
        pub(crate) fn build(specs: &[Spec<'_>]) -> World {
            let mut pulls: Vec<PullRequestRow> = Vec::new();
            let mut links = Vec::new();
            let mut issues = Vec::new();
            for spec in specs {
                issues.push(
                    serde_json::from_value::<Issue>(serde_json::json!({
                        "id": spec.id,
                        "board_id": "board-1",
                        "number": 1,
                        "identifier": spec.identifier,
                        "title": spec.title.unwrap_or(spec.identifier),
                        "status": "in_review",
                    }))
                    .unwrap(),
                );
                let Some(url) = spec.url else {
                    continue;
                };
                if !pulls.iter().any(|pr| pr.id == url) {
                    pulls.push(
                        serde_json::from_value(serde_json::json!({
                            "id": url,
                            "repository_id": "repo-1",
                            "number": spec.number,
                            "url": url,
                            "head": spec.head,
                            "base": spec.base,
                            "state": spec.state,
                        }))
                        .unwrap(),
                    );
                }
                links.push(
                    serde_json::from_value(serde_json::json!({
                        "id": format!("link-{}", spec.id),
                        "pull_request_id": url,
                        "issue_id": spec.id,
                    }))
                    .unwrap(),
                );
            }
            World { pulls, links, issues }
        }

        pub(crate) fn refs(&self) -> (Vec<&PullRequestRow>, Vec<&PullRequestIssueRow>, Vec<&Issue>) {
            (self.pulls.iter().collect(), self.links.iter().collect(), self.issues.iter().collect())
        }

        pub(crate) fn pr_of(&self, issue_id: &str) -> Option<&PullRequestRow> {
            let link = self.links.iter().find(|link| link.issue_id == issue_id)?;
            self.pulls.iter().find(|pr| pr.id == link.pull_request_id)
        }
    }

    /// An open PR row for issue `identifier`: head/base branch names.
    pub(crate) fn spec<'a>(identifier: &'a str, head: Option<&'a str>, base: Option<&'a str>) -> Spec<'a> {
        Spec {
            id: identifier,
            identifier,
            title: None,
            head,
            base,
            url: head,
            state: head.map(|_| "open"),
            number: Some(1),
        }
    }

    /// EXP-11 sits on main, EXP-12 on EXP-11's branch, EXP-13 on EXP-12's.
    fn chain_world() -> World {
        World::build(&[
            spec("EXP-13", Some("exp/EXP-13"), Some("exp/EXP-12")),
            spec("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ])
    }

    #[test]
    fn numbers_a_member_from_the_bottom_of_the_chain() {
        let world = chain_world();
        let (pulls, links, issues) = world.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        let middle = stack_position(world.pr_of("EXP-12").unwrap(), &set).unwrap();
        assert_eq!(middle.position, 2);
        assert_eq!(middle.size, 3);
        assert_eq!(middle.below.as_deref(), Some("EXP-11"));
        assert_eq!(middle.above.as_deref(), Some("EXP-13"));
        let bottom = stack_position(world.pr_of("EXP-11").unwrap(), &set).unwrap();
        assert_eq!((bottom.position, bottom.size), (1, 3));
        assert_eq!(bottom.below, None);
        let top = stack_position(world.pr_of("EXP-13").unwrap(), &set).unwrap();
        assert_eq!((top.position, top.size), (3, 3));
        assert_eq!(top.above, None);
        let chain: Vec<String> = stack_chain(world.pr_of("EXP-13").unwrap(), &pulls)
            .iter()
            .map(|pr| set.name(pr))
            .collect();
        assert_eq!(chain, ["EXP-11", "EXP-12", "EXP-13"]);
    }

    #[test]
    fn the_explicit_base_pull_request_wins() {
        // `base_pull_request_id` chains even when the branch names do not.
        let mut world = World::build(&[
            spec("EXP-2", Some("feature"), Some("renamed")),
            spec("EXP-1", Some("exp/EXP-1"), Some("master")),
        ]);
        world.pulls[0].base_pull_request_id = Some("exp/EXP-1".to_string());
        let (pulls, _, _) = world.refs();
        let chain: Vec<&str> = stack_chain(&world.pulls[0], &pulls).iter().map(|pr| pr.id.as_str()).collect();
        assert_eq!(chain, ["exp/EXP-1", "feature"]);
        // Another repository's head never chains by name.
        let mut other = World::build(&[
            spec("EXP-4", Some("exp/EXP-4"), Some("exp/EXP-3")),
            spec("EXP-3", Some("exp/EXP-3"), Some("master")),
        ]);
        other.pulls[1].repository_id = Some("repo-2".to_string());
        let (pulls, _, _) = other.refs();
        assert_eq!(stack_chain(&other.pulls[0], &pulls).len(), 1);
    }

    #[test]
    fn stops_at_a_base_nobody_in_the_list_owns() {
        let world = chain_world();
        let top = world.pr_of("EXP-13").unwrap();
        let only_top = vec![top];
        assert_eq!(stack_chain(top, &only_top).len(), 1);
        let lone = World::build(&[spec("EXP-20", Some("exp/EXP-20"), Some("master"))]);
        let (pulls, links, issues) = lone.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        assert_eq!(stack_position(&lone.pulls[0], &set), None);
        // An EMPTY base never chains two unrelated rows.
        let blanks = World::build(&[
            spec("EXP-30", Some("exp/EXP-30"), Some("")),
            Spec { url: Some("u-31"), ..spec("EXP-31", Some(""), Some("")) },
        ]);
        let (pulls, _, _) = blanks.refs();
        assert_eq!(stack_chain(&blanks.pulls[0], &pulls).len(), 1);
    }

    #[test]
    fn breaks_a_cycle_where_it_first_appears() {
        let world = World::build(&[spec("EXP-40", Some("a"), Some("b")), spec("EXP-41", Some("b"), Some("a"))]);
        let (pulls, links, issues) = world.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        let chain: Vec<String> = stack_chain(&world.pulls[0], &pulls).iter().map(|pr| set.name(pr)).collect();
        assert_eq!(chain, ["EXP-41", "EXP-40"]);
    }

    #[test]
    fn an_issue_resolves_its_open_pull_request_first() {
        let world = World::build(&[
            Spec { url: Some("old"), state: Some("merged"), ..spec("EXP-1", Some("a"), Some("master")) },
            Spec { id: "EXP-1", url: Some("new"), ..spec("EXP-1", Some("b"), Some("master")) },
        ]);
        let (pulls, links, issues) = world.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        assert_eq!(set.pull_requests_of_issue("EXP-1").len(), 2);
        assert_eq!(set.issue_pull_request("EXP-1").map(|pr| pr.id.as_str()), Some("new"));
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

    fn view_world(rows: &[ViewRow]) -> World {
        let specs: Vec<Spec<'_>> = rows
            .iter()
            .map(|row| Spec {
                id: &row.id,
                identifier: &row.identifier,
                title: row.title.as_deref(),
                head: row.branch.as_deref(),
                base: row.pr_base_branch.as_deref(),
                url: row.pr_url.as_deref(),
                state: row.pr_state.as_deref(),
                number: row.pr_number,
            })
            .collect();
        World::build(&specs)
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
            let world = view_world(&case.issues);
            let (pulls, links, issues) = world.refs();
            let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
            let subject = set.issue_pull_request(&case.issue);
            let shape = subject.map_or(PrGraphShape::Single, |pr| open_pr_shape(pr, &set));
            assert_eq!(shape.as_str(), case.shape, "{}", case.name);
            let expected = case.view.map(|view| StackView {
                rows: view
                    .rows
                    .into_iter()
                    .map(|row| StackViewRow {
                        pull_request_id: world.pr_of(&row.issue_id).unwrap().id.clone(),
                        issue_id: Some(row.issue_id),
                        identifier: row.identifier,
                        title: row.title,
                        pr_number: row.pr_number,
                        is_current: row.is_current,
                    })
                    .collect(),
                base_branch: view.base_branch,
            });
            assert_eq!(subject.and_then(|pr| stack_view(pr, &set)), expected, "{}", case.name);
        }
    }

    /// EXP-1248: a fork anywhere is a tree; a line is a stack.
    #[test]
    fn pr_graph_shape_tells_a_fork_from_a_line() {
        let line = chain_world();
        let (pulls, _, _) = line.refs();
        assert_eq!(pr_graph_shape(&pulls), PrGraphShape::Stack);
        assert_eq!(pr_graph_shape(&pulls[..1]), PrGraphShape::Single);
        let fork = World::build(&[
            spec("EXP-1", Some("a"), Some("master")),
            spec("EXP-2", Some("b"), Some("a")),
            spec("EXP-3", Some("c"), Some("a")),
        ]);
        let (pulls, _, _) = fork.refs();
        assert_eq!(pr_graph_shape(&pulls), PrGraphShape::Tree);
        assert_eq!(pr_component(pulls[2], &pulls).len(), 3);
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
            let world = view_world(&case.issues);
            let (pulls, links, issues) = world.refs();
            let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
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
                    pull_request_id: world.pr_of(&confirm.input.issue_id).unwrap().id.clone(),
                }
            });
            let subject = set.issue_pull_request(&case.issue);
            assert_eq!(
                subject.and_then(|pr| stack_merge_confirm(pr, &set, mode)),
                expected,
                "{}",
                case.name
            );
        }
    }
}
