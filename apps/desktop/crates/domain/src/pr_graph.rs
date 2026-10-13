//! EXP-897 §4: the ONE graph behind the work header's related-work badge
//! (SLOP-3 keeps the badge and its overlay).
//!
//! Two shapes of "this piece of work is not alone" are synced:
//!
//! * a **stack**: pull requests chained through their base
//!   ([`crate::pr_stack`]);
//! * a **batch**: ONE pull request closing several issues
//!   (`pull_request_issues`).
//!
//! EXP-1097 adds a third, the subject issue's OPEN BLOCKERS
//! ([`PrGraph::blocked_by`], filled by the caller, which owns the relations).
//!
//! EXP-1294: keyed on [`PullRequestRow`] (+ its issue links, [`PrSet`]); the
//! run tree and the batch RUN are gone. A stack ENTRY
//! is a pull request, never an issue, so a batch can itself be a stack
//! member. Mirrored ×4 by name (web `lib/pr-graph.ts`, iOS `PrGraph.swift`,
//! Android `PrGraph.kt`).

use crate::pr_stack::{self, PrSet, StackPosition};
use crate::rows::{Issue, PullRequestRow};

/// One pull request in a graph and the issue(s) it closes (identifier
/// order; `issues[0]` = the representative). `pull` is `None` only for the
/// placeholder entry of an issue with no pull request yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PrEntry {
    pub pull: Option<PullRequestRow>,
    pub issues: Vec<Issue>,
}

impl PrEntry {
    /// The issue that names the PR (`None` for an issue-less PR).
    pub fn representative(&self) -> Option<&Issue> {
        self.issues.first()
    }

    /// A batch pull request closes more than one issue.
    pub fn is_batch(&self) -> bool {
        self.issues.len() > 1
    }

    /// The PR's id (`None` for the placeholder entry).
    pub fn pull_request_id(&self) -> Option<&str> {
        self.pull.as_ref().map(|pull| pull.id.as_str())
    }

    /// `EXP-11` for a single PR, `EXP-11, EXP-12` for a batch, `#45` for an
    /// issue-less one.
    pub fn identifiers(&self) -> String {
        if self.issues.is_empty() {
            return self
                .pull
                .as_ref()
                .and_then(|pull| pull.number)
                .map(|number| format!("#{number}"))
                .unwrap_or_default();
        }
        self.issues
            .iter()
            .map(|issue| issue.identifier.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Whether `issue_id` is one of the issues on this pull request.
    pub fn holds(&self, issue_id: &str) -> bool {
        self.issues.iter().any(|issue| issue.id == issue_id)
    }
}

/// The entry of `pull`: the PR and the issues it closes.
pub fn pr_entry(pull: &PullRequestRow, set: &PrSet<'_>) -> PrEntry {
    PrEntry {
        pull: Some(pull.clone()),
        issues: set.issues_of(pull).into_iter().cloned().collect(),
    }
}

/// A stack member, bottom first. `depth` is its distance from the bottom (a
/// stack is linear, so it equals the 0-based position).
#[derive(Debug, Clone, PartialEq)]
pub struct StackEntry {
    pub entry: PrEntry,
    pub depth: usize,
}

/// The subject's own pull request when it closes several issues.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchEntry {
    pub issues: Vec<Issue>,
}

/// Everything around one subject. All parts are EMPTY/`None` for a lone
/// issue with no PR: [`badge_kind`] then reports nothing and the header
/// shows no badge.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrGraph {
    /// The subject's own pull request (or the placeholder entry of an issue
    /// without one).
    pub entry: Option<PrEntry>,
    /// The whole chain, BOTTOM first. Empty unless the subject's PR really is
    /// stacked (a lone PR is not a stack of one).
    pub stack: Vec<StackEntry>,
    /// The subject's own pull request, when it is a batch.
    pub batch: Option<BatchEntry>,
    /// The subject PR's id: which [`StackEntry`] the badge is ON.
    pub subject_pull_request_id: Option<String>,
    /// The subject ISSUE's id (`None` for a PR-only subject).
    pub subject_issue_id: Option<String>,
    /// EXP-1097: the subject issue's OPEN blockers (web `blockedBy`), in the
    /// ONE `openBlockers` order. [`pr_graph`] leaves it empty: the caller
    /// fills it; empty for an issue-less subject.
    pub blocked_by: Vec<Issue>,
}

impl PrGraph {
    /// Where the subject sits in [`Self::stack`] (`2 of 3`), `None` when it is
    /// not stacked.
    pub fn position(&self) -> Option<StackPosition> {
        let id = self.subject_pull_request_id.as_deref()?;
        let index = self
            .stack
            .iter()
            .position(|member| member.entry.pull_request_id() == Some(id))?;
        Some(StackPosition {
            position: index + 1,
            size: self.stack.len(),
            below: index
                .checked_sub(1)
                .map(|lower| self.stack[lower].entry.identifiers()),
            above: self
                .stack
                .get(index + 1)
                .map(|upper| upper.entry.identifiers()),
        })
    }
}

/// Which glyph(s) the badge wears. `None` = nothing to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Stack,
    Batch,
    StackAndBatch,
}

/// The badge rule: a stacked PR reports a stack, a combined PR a batch, a
/// batch inside a stack both, a lone pull request nothing.
pub fn badge_kind(graph: &PrGraph) -> Option<BadgeKind> {
    match (!graph.stack.is_empty(), graph.batch.is_some()) {
        (true, true) => Some(BadgeKind::StackAndBatch),
        (true, false) => Some(BadgeKind::Stack),
        (false, true) => Some(BadgeKind::Batch),
        (false, false) => None,
    }
}

/// EXP-1097 / SLOP-16 round 5: what the header's icon button draws (web
/// `badgeShape`): a PR relation ([`badge_kind`]) first, then the subject's
/// open blockers. `None` = no badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeShape {
    Stack,
    Batch,
    StackAndBatch,
    Blocked,
}

/// The badge's shape rule.
pub fn badge_shape(graph: &PrGraph) -> Option<BadgeShape> {
    match badge_kind(graph) {
        Some(BadgeKind::Stack) => Some(BadgeShape::Stack),
        Some(BadgeKind::Batch) => Some(BadgeShape::Batch),
        Some(BadgeKind::StackAndBatch) => Some(BadgeShape::StackAndBatch),
        None if !graph.blocked_by.is_empty() => Some(BadgeShape::Blocked),
        None => None,
    }
}

/// EXP-1058: the badge's front issue and how many ride behind it (`+N`).
/// `issue` = the subject's pull request's representative row (EXP-1097: the
/// FIRST open blocker for the `blocked` shape). `count` = every OTHER member
/// on the stack (each entry's issues, an issue-less PR counting one) or
/// batch, every other blocker for `blocked`. Byte-identical ×4 (web
/// `badgeChip`).
#[derive(Debug, Clone, PartialEq)]
pub struct BadgeChip {
    pub issue: Option<Issue>,
    pub count: usize,
}

/// The chip rule. `None` = no chip, exactly when [`badge_shape`] is.
pub fn badge_chip(graph: &PrGraph) -> Option<BadgeChip> {
    let shape = badge_shape(graph)?;
    if shape == BadgeShape::Blocked {
        return Some(BadgeChip {
            issue: graph.blocked_by.first().cloned(),
            count: graph.blocked_by.len().saturating_sub(1),
        });
    }
    let issue = graph
        .entry
        .as_ref()
        .and_then(|entry| entry.representative().cloned());
    if graph.stack.len() >= 2 {
        let total: usize = graph
            .stack
            .iter()
            .map(|member| member.entry.issues.len().max(1))
            .sum();
        return Some(BadgeChip {
            issue,
            count: total.saturating_sub(1),
        });
    }
    let batch = graph.batch.as_ref().map_or(1, |batch| batch.issues.len());
    Some(BadgeChip {
        issue,
        count: batch.saturating_sub(1),
    })
}

/// One band of the "Related work" dialog (web `OverlaySection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlaySection {
    Blocked,
    Batch,
    Stack,
}

/// SLOP-16 round 5 (web `overlaySections`): the dialog's bands, in ONE fixed
/// order ×4: Blocked by, Same pull request, Pull request stack: each only
/// when it has rows ([`batch_partners`], [`stack_others`]).
pub fn overlay_sections(graph: &PrGraph) -> Vec<OverlaySection> {
    use OverlaySection::{Batch, Blocked, Stack};
    [Blocked, Batch, Stack]
        .into_iter()
        .filter(|section| match section {
            Blocked => !graph.blocked_by.is_empty(),
            Batch => !batch_partners(graph).is_empty(),
            Stack => !stack_others(graph).is_empty(),
        })
        .collect()
}

/// The "Same pull request" band: every issue sharing the subject's pull
/// request but the subject issue itself (all of them for a PR subject).
pub fn batch_partners(graph: &PrGraph) -> Vec<&Issue> {
    let Some(batch) = graph.batch.as_ref() else {
        return Vec::new();
    };
    batch
        .issues
        .iter()
        .filter(|issue| graph.subject_issue_id.as_deref() != Some(issue.id.as_str()))
        .collect()
}

/// The "Pull request stack" band: the OTHER pull requests of the subject's
/// stack, BOTTOM-UP, the subject's own pull request left out.
pub fn stack_others(graph: &PrGraph) -> Vec<&PrEntry> {
    graph
        .stack
        .iter()
        .map(|member| &member.entry)
        .filter(|entry| match graph.subject_pull_request_id.as_deref() {
            Some(id) => entry.pull_request_id() != Some(id),
            None => true,
        })
        .collect()
}

/// SLOP-16: the "Related work" dialog's copy, byte-identical ×4 (web
/// `lib/pr-graph.ts`, iOS, Android): the title, one band label per
/// [`OverlaySection`], the empty state.
pub mod overlay_copy {
    use super::OverlaySection;

    /// The dialog's title (constant, whatever the badge's shape).
    pub const RELATED_WORK_TITLE: &str = "Related work";
    /// The "Blocked by" band: the relations card's copy.
    pub const BLOCKED_BY: &str = "Blocked by";
    /// The batch partners' band.
    pub const SAME_PULL_REQUEST: &str = "Same pull request";
    /// The PR stack's band.
    pub const PULL_REQUEST_STACK: &str = "Pull request stack";
    /// Nothing to list.
    pub const EMPTY: &str = "Nothing else is linked to this issue.";

    /// The band over one section.
    pub fn band_label(section: OverlaySection) -> &'static str {
        match section {
            OverlaySection::Blocked => BLOCKED_BY,
            OverlaySection::Batch => SAME_PULL_REQUEST,
            OverlaySection::Stack => PULL_REQUEST_STACK,
        }
    }
}

/// The graph for one subject: an `issue` (its pull request = the newest open
/// one linked to it, [`PrSet::issue_pull_request`]) and/or a `pull` request
/// directly (a thread Guide's PR; wins over the issue's).
pub fn pr_graph(issue: Option<&Issue>, pull: Option<&PullRequestRow>, set: &PrSet<'_>) -> PrGraph {
    let pull = pull
        .map(|pull| set.pull(&pull.id).unwrap_or(pull))
        .or_else(|| set.issue_pull_request(&issue?.id));
    let entry = match (pull, issue) {
        (Some(pull), _) => Some(pr_entry(pull, set)),
        (None, Some(issue)) => Some(PrEntry {
            pull: None,
            issues: vec![issue.clone()],
        }),
        (None, None) => None,
    };
    let batch = entry
        .as_ref()
        .filter(|entry| entry.is_batch())
        .map(|entry| BatchEntry {
            issues: entry.issues.clone(),
        });
    let stack = pull
        .map(|pull| {
            let chain = pr_stack::stack_chain(pull, set.pulls);
            if chain.len() < 2 {
                return Vec::new();
            }
            chain
                .into_iter()
                .enumerate()
                .map(|(depth, member)| StackEntry {
                    entry: pr_entry(member, set),
                    depth,
                })
                .collect()
        })
        .unwrap_or_default();
    PrGraph {
        subject_issue_id: issue.map(|issue| issue.id.clone()),
        subject_pull_request_id: pull.map(|pull| pull.id.clone()),
        entry,
        stack,
        batch,
        blocked_by: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_stack::tests::{spec, Spec, World};

    fn batch_spec<'a>(identifier: &'a str, head: &'a str, base: Option<&'a str>, pr: &'a str) -> Spec<'a> {
        Spec {
            url: Some(pr),
            ..spec(identifier, Some(head), base)
        }
    }

    fn graph_for(world: &World, issue_id: &str) -> PrGraph {
        let (pulls, links, issues) = world.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        let issue = world.issues.iter().find(|issue| issue.id == issue_id).unwrap();
        pr_graph(Some(issue), None, &set)
    }

    #[test]
    fn reports_a_stack_badge_for_a_stacked_pr() {
        let world = World::build(&[
            spec("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]);
        let graph = graph_for(&world, "EXP-12");
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Stack));
        assert_eq!(graph.stack.len(), 2);
        assert_eq!(graph.stack[0].entry.identifiers(), "EXP-11");
        assert_eq!(graph.stack[1].depth, 1);
        let position = graph.position().unwrap();
        assert_eq!((position.position, position.size), (2, 2));
        assert_eq!(position.below.as_deref(), Some("EXP-11"));
        assert!(graph.batch.is_none());
    }

    #[test]
    fn reports_a_batch_badge_for_a_batch_pr() {
        let world = World::build(&[
            batch_spec("EXP-20", "exp/batch-a1b2c3d4", Some("master"), "u/1"),
            batch_spec("EXP-21", "exp/batch-a1b2c3d4", Some("master"), "u/1"),
        ]);
        let graph = graph_for(&world, "EXP-20");
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Batch));
        assert_eq!(graph.batch.as_ref().unwrap().issues.len(), 2);
        assert!(graph.stack.is_empty());
        assert_eq!(world.pulls.len(), 1, "the two issues are ONE pull request");
        assert_eq!(graph.entry.as_ref().unwrap().identifiers(), "EXP-20, EXP-21");
    }

    #[test]
    fn reports_both_for_a_batch_inside_a_stack() {
        let world = World::build(&[
            batch_spec("EXP-20", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            batch_spec("EXP-21", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]);
        let graph = graph_for(&world, "EXP-20");
        assert_eq!(badge_kind(&graph), Some(BadgeKind::StackAndBatch));
        assert_eq!(graph.stack.len(), 2);
        assert!(graph.stack[1].entry.is_batch());
        assert_eq!(graph.stack[1].entry.identifiers(), "EXP-20, EXP-21");
        let position = graph.position().unwrap();
        assert_eq!((position.position, position.size), (2, 2));
    }

    #[test]
    fn a_pull_request_subject_needs_no_issue() {
        // A thread Guide's PR: issue-less, stacked on an issue's PR.
        let mut world = World::build(&[
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
            Spec { id: "none", identifier: "none", url: Some("chore"), number: Some(7), ..spec("none", Some("chore"), Some("exp/EXP-11")) },
        ]);
        world.links.retain(|link| link.issue_id != "none");
        let (pulls, links, issues) = world.refs();
        let set = PrSet { pulls: &pulls, links: &links, issues: &issues };
        let chore = world.pulls.iter().find(|pr| pr.id == "chore").unwrap();
        let graph = pr_graph(None, Some(chore), &set);
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Stack));
        assert_eq!(graph.subject_issue_id, None);
        assert_eq!(graph.stack[1].entry.identifiers(), "#7");
        let chip = badge_chip(&graph).unwrap();
        assert_eq!(chip.issue, None);
        assert_eq!(chip.count, 1);
    }

    #[test]
    fn reports_nothing_for_a_lone_pr() {
        let world = World::build(&[spec("EXP-30", Some("exp/EXP-30"), Some("master"))]);
        let graph = graph_for(&world, "EXP-30");
        assert_eq!(badge_kind(&graph), None);
        assert!(graph.stack.is_empty());
        assert!(graph.batch.is_none());
        let bare = World::build(&[spec("EXP-31", None, None)]);
        let graph = graph_for(&bare, "EXP-31");
        assert_eq!(badge_kind(&graph), None);
        assert_eq!(graph.entry.as_ref().unwrap().pull, None, "the placeholder entry");
    }

    /// EXP-1058: names the representative issue and the count.
    #[test]
    fn badge_chip_names_the_representative_issue_and_the_count() {
        let world = World::build(&[
            spec("LOWER", Some("exp/LOWER"), Some("master")),
            batch_spec("ONE", "exp/batch-abcd1234", Some("exp/LOWER"), "u/9"),
            batch_spec("TWO", "exp/batch-abcd1234", Some("exp/LOWER"), "u/9"),
        ]);
        let chip = badge_chip(&graph_for(&world, "TWO")).unwrap();
        assert_eq!(chip.issue.as_ref().map(|issue| issue.id.as_str()), Some("ONE"));
        assert_eq!(chip.count, 2);
        let loose = World::build(&[
            batch_spec("ONE", "exp/batch-abcd1234", None, "u/9"),
            batch_spec("TWO", "exp/batch-abcd1234", None, "u/9"),
        ]);
        assert_eq!(badge_chip(&graph_for(&loose, "ONE")).unwrap().count, 1);
    }

    /// EXP-1097: stack/batch first, then the open blockers.
    #[test]
    fn badge_shape_ranks_stack_batch_then_blockers() {
        let world = World::build(&[spec("EXP-40", None, None), spec("EXP-41", None, None), spec("EXP-42", None, None)]);
        let blockers = vec![world.issues[1].clone(), world.issues[2].clone()];
        let mut graph = graph_for(&world, "EXP-40");
        assert_eq!(badge_shape(&graph), None);
        graph.blocked_by = blockers.clone();
        assert_eq!(badge_shape(&graph), Some(BadgeShape::Blocked));
        let chip = badge_chip(&graph).unwrap();
        assert_eq!(chip.issue.map(|issue| issue.identifier), Some("EXP-41".to_string()));
        assert_eq!(chip.count, 1);
        graph.blocked_by.truncate(1);
        assert_eq!(badge_chip(&graph).unwrap().count, 0);
        let stacked = World::build(&[
            spec("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]);
        let mut stack = graph_for(&stacked, "EXP-12");
        stack.blocked_by = blockers;
        assert_eq!(badge_shape(&stack), Some(BadgeShape::Stack));
        assert_eq!(badge_chip(&stack).unwrap().issue.unwrap().identifier, "EXP-12");
    }

    /// SLOP-16 r5: ONE band order ×4, each band only with rows.
    #[test]
    fn overlay_sections_are_blocked_batch_stack() {
        use OverlaySection::*;
        let stacked = World::build(&[
            spec("EXP-13", Some("exp/EXP-13"), Some("exp/EXP-12")),
            spec("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]);
        let mut graph = graph_for(&stacked, "EXP-12");
        assert_eq!(overlay_sections(&graph), vec![Stack]);
        let others: Vec<String> = stack_others(&graph).iter().map(|entry| entry.identifiers()).collect();
        assert_eq!(others, vec!["EXP-11".to_string(), "EXP-13".to_string()]);
        graph.blocked_by = vec![stacked.issues[0].clone()];
        assert_eq!(overlay_sections(&graph), vec![Blocked, Stack]);
        let lone = World::build(&[spec("EXP-30", Some("exp/EXP-30"), Some("master"))]);
        assert!(overlay_sections(&graph_for(&lone, "EXP-30")).is_empty());
        let world = World::build(&[
            batch_spec("EXP-20", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            batch_spec("EXP-21", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            spec("EXP-11", Some("exp/EXP-11"), Some("master")),
        ]);
        let graph = graph_for(&world, "EXP-21");
        assert_eq!(overlay_sections(&graph), vec![Batch, Stack]);
        let partners: Vec<&str> = batch_partners(&graph).iter().map(|issue| issue.identifier.as_str()).collect();
        assert_eq!(partners, vec!["EXP-20"]);
        assert_eq!(stack_others(&graph).len(), 1);
    }

    /// SLOP-16: the "Related work" copy is byte-identical ×4.
    #[test]
    fn the_related_work_copy_is_pinned() {
        use overlay_copy::*;
        assert_eq!(RELATED_WORK_TITLE, "Related work");
        assert_eq!(EMPTY, "Nothing else is linked to this issue.");
        assert_eq!(band_label(OverlaySection::Blocked), "Blocked by");
        assert_eq!(band_label(OverlaySection::Batch), "Same pull request");
        assert_eq!(band_label(OverlaySection::Stack), "Pull request stack");
    }
}
