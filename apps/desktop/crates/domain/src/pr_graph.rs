//! EXP-897 §4 — the ONE graph behind the work header's stack/batch badge.
//!
//! Three shapes of "this piece of work is not alone" are already synced, and
//! before this module every client rendered each of them somewhere else:
//!
//! * a **stack** — pull requests chained through `pr_base_branch`
//!   ([`crate::pr_stack`]);
//! * a **batch** — several issues sharing ONE `pr_url` (a batch run's combined
//!   pull request);
//! * a **session tree** — runs a run started ([`crate::session_tree`]).
//!
//! EXP-1097 adds a fourth, the subject issue's OPEN BLOCKERS
//! ([`PrGraph::blocked_by`], filled by the caller, which owns the relations).
//!
//! [`pr_graph`] derives the first three for one subject (an issue, or a run) so the
//! badge, its overlay and the Reviews rows read from ONE model. A stack ENTRY
//! is a pull request, never an issue, so a batch can itself be a stack member.
//!
//! Mirrored ×4 by name (web `lib/pr-graph.ts`, iOS `PrGraph.swift`, Android
//! `PrGraph.kt`) with the same four `badge_kind` tests.

use std::collections::HashMap;

use crate::pr_stack::{self, StackPosition};
use crate::rows::{CodingSession, Issue};
use crate::session_tree::{self, TreeRow};

/// One pull request in a graph: the issue(s) it closes. A batch PR carries
/// more than one; `issues[0]` is the representative (newest first, the
/// caller's order), the row whose id every PR mutation takes.
#[derive(Debug, Clone, PartialEq)]
pub struct PrEntry {
    pub issues: Vec<Issue>,
}

impl PrEntry {
    pub fn representative(&self) -> &Issue {
        &self.issues[0]
    }

    /// A batch pull request closes more than one issue.
    pub fn is_batch(&self) -> bool {
        self.issues.len() > 1
    }

    /// The head branch the PR is cut from (shared by every issue on it).
    pub fn branch(&self) -> Option<&str> {
        non_empty(self.representative().branch.as_deref())
    }

    /// The branch the PR TARGETS — the stack edge.
    pub fn base_branch(&self) -> Option<&str> {
        non_empty(self.representative().pr_base_branch.as_deref())
    }

    /// `EXP-11` for a single PR, `EXP-11, EXP-12` for a batch.
    pub fn identifiers(&self) -> String {
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

/// Everything around one subject. All three parts are EMPTY/`None` for a lone
/// issue with no PR and no sibling run — [`badge_kind`] then reports nothing
/// and the header shows no badge.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrGraph {
    /// The subject's own pull request (web `entry`): the subject issue's, or
    /// the one the subject run resolves to. `None` for a run with neither an
    /// issue nor a pull request of its own.
    pub entry: Option<PrEntry>,
    /// The whole chain, BOTTOM first. Empty unless the subject's PR really is
    /// stacked (a lone PR is not a stack of one).
    pub stack: Vec<StackEntry>,
    /// The subject's own pull request, when it is a batch.
    pub batch: Option<BatchEntry>,
    /// The subject run's tree — its root and every descendant, nested. Empty
    /// when the subject has no run, or the run is alone.
    pub tree: Vec<TreeRow<CodingSession>>,
    /// The subject PR's head branch — which [`StackEntry`] the badge is ON.
    pub subject_branch: Option<String>,
    /// EXP-1097: the subject issue's OPEN blockers (web `blockedBy`), in the
    /// ONE `openBlockers` order. [`pr_graph`] leaves it empty — it takes no
    /// relations — and the caller fills it; empty for an issue-less subject.
    pub blocked_by: Vec<Issue>,
}

impl PrGraph {
    /// Where the subject sits in [`Self::stack`] (`2 of 3`), `None` when it is
    /// not stacked.
    pub fn position(&self) -> Option<StackPosition> {
        let branch = self.subject_branch.as_deref()?;
        let index = self
            .stack
            .iter()
            .position(|member| member.entry.branch() == Some(branch))?;
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

    /// The BOTTOM member — the row that offers "Merge stack".
    pub fn bottom(&self) -> Option<&PrEntry> {
        self.stack.first().map(|member| &member.entry)
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

/// Which face of a top tab the badge is drawn on. EXP-1097: it decides ONLY
/// which overlay section comes first — the chip itself is face-independent
/// ([`badge_shape`]). Web `PrGraphFace`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrGraphFace {
    Issue,
    Run,
    Changes,
}

/// EXP-1097: what the header chip draws, on EVERY face alike (web
/// `badgeShape`): a PR relation ([`badge_kind`]) first, then the run tree
/// when the subject has a family, then its open blockers. `None` = no chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeShape {
    Stack,
    Batch,
    StackAndBatch,
    Runs,
    Blocked,
}

/// The chip's shape rule — no face in it. (Here a lone run has an EMPTY tree
/// and a family's tree carries the subject, so "a family" = more than one
/// row on both sides.)
pub fn badge_shape(graph: &PrGraph) -> Option<BadgeShape> {
    match badge_kind(graph) {
        Some(BadgeKind::Stack) => Some(BadgeShape::Stack),
        Some(BadgeKind::Batch) => Some(BadgeShape::Batch),
        Some(BadgeKind::StackAndBatch) => Some(BadgeShape::StackAndBatch),
        None if graph.tree.len() > 1 => Some(BadgeShape::Runs),
        None if !graph.blocked_by.is_empty() => Some(BadgeShape::Blocked),
        None => None,
    }
}

/// EXP-1058: what the header's STACKED issue chip draws in place of the old
/// pill — the front chip's issue and how many ride behind it (`+N`).
/// `issue` = the subject's pull request's representative row (EXP-1097: the
/// FIRST open blocker for the `blocked` shape); `None` only for a run family
/// with no issue, where the front chip names the run instead. `count` =
/// every OTHER issue on the stack (all its entries' issues) or batch, every
/// other run of the tree for `runs`, every other blocker for `blocked`.
/// Byte-identical ×4 (web `badgeChip`).
#[derive(Debug, Clone, PartialEq)]
pub struct BadgeChip {
    pub issue: Option<Issue>,
    pub count: usize,
}

/// The chip rule. `None` = no chip, exactly when [`badge_shape`] is.
pub fn badge_chip(graph: &PrGraph) -> Option<BadgeChip> {
    let shape = badge_shape(graph)?;
    let issue = graph
        .entry
        .as_ref()
        .map(|entry| entry.representative().clone());
    match shape {
        BadgeShape::Runs => {
            return Some(BadgeChip {
                issue,
                count: graph.tree.len() - 1,
            })
        }
        BadgeShape::Blocked => {
            return Some(BadgeChip {
                issue: graph.blocked_by.first().cloned(),
                count: graph.blocked_by.len().saturating_sub(1),
            })
        }
        BadgeShape::Stack | BadgeShape::Batch | BadgeShape::StackAndBatch => {}
    }
    if graph.stack.len() >= 2 {
        let total: usize = graph
            .stack
            .iter()
            .map(|member| member.entry.issues.len())
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

/// One section of the chip's overlay (web `OverlaySection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlaySection {
    Blocked,
    Batch,
    Runs,
    Stack,
}

/// EXP-1097 (web `overlaySections`): every relation the subject HAS, the
/// face's own section first (Issue: Blocked by; Run: the run's issues and its
/// tree; Changes: the pull requests). A section with nothing to list is left
/// out, save the Changes face's own lead (its pull request, even a lone one).
/// The web's other exception — the Run face's tree of ONE run — cannot occur
/// here: a lone run's tree is empty (see [`PrGraph::tree`]).
pub fn overlay_sections(graph: &PrGraph, face: PrGraphFace) -> Vec<OverlaySection> {
    use OverlaySection::{Batch, Blocked, Runs, Stack};
    let order: [OverlaySection; 4] = match face {
        PrGraphFace::Issue => [Blocked, Batch, Stack, Runs],
        PrGraphFace::Run => [Batch, Runs, Stack, Blocked],
        PrGraphFace::Changes => [Stack, Batch, Runs, Blocked],
    };
    order
        .into_iter()
        .filter(|section| match section {
            Blocked => !graph.blocked_by.is_empty(),
            Batch => graph.batch.is_some(),
            Runs => !graph.tree.is_empty(),
            Stack => {
                graph.stack.len() >= 2 || (face == PrGraphFace::Changes && graph.entry.is_some())
            }
        })
        .collect()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Group `issues` into pull requests: issues sharing a `pr_url` collapse into
/// ONE entry (a batch), an issue without one keys on itself. First-seen order
/// is preserved.
pub fn pr_entries(issues: &[Issue]) -> Vec<PrEntry> {
    let mut order: Vec<String> = Vec::new();
    let mut by_pr: HashMap<String, Vec<Issue>> = HashMap::new();
    for issue in issues {
        let key = non_empty(issue.pr_url.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| issue.id.clone());
        let bucket = by_pr.entry(key.clone()).or_default();
        if bucket.is_empty() {
            order.push(key);
        }
        bucket.push(issue.clone());
    }
    order
        .into_iter()
        .filter_map(|key| by_pr.remove(&key))
        .map(|issues| PrEntry { issues })
        .collect()
}

/// EXP-876: a BATCH run's own entry. A batch links no issue and stamps no
/// `pr_url` of its own, so before this it resolved nothing at all — the pill
/// and its sheet, the one surface built to name work that spans several
/// issues, never appeared on the very run that spans them. Its covered set
/// (`batch_issue_ids`, the one source since EXP-972) IS the entry.
///
/// The PR-grouped entry wins whenever there is one: it carries the branch and
/// the base the stack chains on, so a batch PR stacked on another still reads
/// `stack+batch` and still offers Merge stack. The synthesized entry is what a
/// batch wears BEFORE its PR exists.
fn batch_session_entry(
    session: &CodingSession,
    issues: &[Issue],
    entries: &[PrEntry],
) -> Option<PrEntry> {
    let covered = crate::batch_run::batch_run_issues(session, issues);
    let first = covered.first()?;
    if let Some(grouped) = entries
        .iter()
        .find(|entry| entry.holds(&first.id) && entry.is_batch())
    {
        return Some(grouped.clone());
    }
    Some(PrEntry {
        issues: covered.into_iter().cloned().collect(),
    })
}

/// The graph for one subject. `issue` names the issue face's subject; a
/// `session` (issue-bound, batch or issue-less) contributes the run tree and,
/// when no issue was given, resolves the subject issue itself.
///
/// `issues` should be the team's (or board's) rows — the stack rule matches
/// branch names, so the caller owns the scoping. `sessions` are the run rows
/// the tree is nested from.
pub fn pr_graph(
    issue: Option<&Issue>,
    session: Option<&CodingSession>,
    issues: &[Issue],
    sessions: &[CodingSession],
) -> PrGraph {
    // The subject ISSUE: the given one, else the one the subject run works
    // (web `subjectIssue = input.issue ?? issues.find(session.issueId)`).
    let issue = issue.or_else(|| {
        let id = session?.issue_id.as_deref()?;
        issues.iter().find(|row| row.id == id)
    });
    // The subject PULL REQUEST: the subject issue's, else the issue(s) the
    // subject run's own `pr_url` closes (a batch run links none directly).
    let entries = pr_entries(issues);
    let subject_entry = issue
        .and_then(|issue| {
            entries
                .iter()
                .find(|entry| entry.holds(&issue.id))
                .cloned()
                .or_else(|| {
                    Some(PrEntry {
                        issues: vec![issue.clone()],
                    })
                })
        })
        .or_else(|| {
            let url = non_empty(session?.pr_url.as_deref())?;
            entries
                .iter()
                .find(|entry| {
                    non_empty(entry.representative().pr_url.as_deref()) == Some(url)
                })
                .cloned()
        })
        // EXP-876: a BATCH run's own entry — see [`batch_session_entry`].
        .or_else(|| batch_session_entry(session?, issues, &entries));

    let batch = subject_entry
        .as_ref()
        .filter(|entry| entry.is_batch())
        .map(|entry| BatchEntry {
            issues: entry.issues.clone(),
        });

    // The stack: chain the subject PR bottom-up. Only a REAL chain counts —
    // a lone pull request is not a stack of one.
    let stack = subject_entry
        .as_ref()
        .map(|subject| {
            let chain = pr_stack::stack_chain(subject.representative(), issues);
            if chain.len() < 2 {
                return Vec::new();
            }
            chain
                .into_iter()
                .enumerate()
                .map(|(depth, member)| StackEntry {
                    entry: entries
                        .iter()
                        .find(|entry| entry.holds(&member.id))
                        .cloned()
                        .unwrap_or_else(|| PrEntry {
                            issues: vec![member.clone()],
                        }),
                    depth,
                })
                .collect()
        })
        .unwrap_or_default();

    // The run tree: the subject run's whole family, nested — or, for an
    // issue subject with no run given (EXP-1097, web parity), every run on
    // the issue with its subtree. One run alone is no tree to show.
    let family = match (session, issue) {
        (Some(session), _) => session_family(session, sessions),
        (None, Some(issue)) => issue_runs(&issue.id, sessions),
        (None, None) => Vec::new(),
    };
    let tree = if family.len() < 2 {
        Vec::new()
    } else {
        session_tree::nest_sessions(
            family,
            |row| row.id.as_str(),
            |row| row.parent_session_id.as_deref(),
            |row| row.started_at.as_deref(),
        )
    };

    PrGraph {
        subject_branch: subject_entry
            .as_ref()
            .and_then(|entry| entry.branch())
            .map(str::to_string),
        entry: subject_entry,
        stack,
        batch,
        tree,
        blocked_by: Vec::new(),
    }
}

/// EXP-1097 (web `prGraph`'s issue branch): every run on `issue_id` and each
/// one's whole subtree, in the caller's order.
fn issue_runs(issue_id: &str, sessions: &[CodingSession]) -> Vec<CodingSession> {
    let mut ids: Vec<&str> = Vec::new();
    let mut frontier: Vec<&str> = sessions
        .iter()
        .filter(|row| row.issue_id.as_deref() == Some(issue_id))
        .map(|row| row.id.as_str())
        .collect();
    while let Some(id) = frontier.pop() {
        if ids.contains(&id) {
            continue;
        }
        ids.push(id);
        frontier.extend(
            sessions
                .iter()
                .filter(|row| row.parent_session_id.as_deref() == Some(id))
                .map(|row| row.id.as_str()),
        );
    }
    sessions
        .iter()
        .filter(|row| ids.contains(&row.id.as_str()))
        .cloned()
        .collect()
}

/// Every run in the subject's tree: walk up to the root, then collect the
/// whole subtree under it. Cycle-safe (a parent chain that repeats stops).
fn session_family(session: &CodingSession, sessions: &[CodingSession]) -> Vec<CodingSession> {
    let by_id: HashMap<&str, &CodingSession> =
        sessions.iter().map(|row| (row.id.as_str(), row)).collect();
    let mut root = session;
    let mut seen: Vec<&str> = vec![root.id.as_str()];
    while let Some(parent) = non_empty(root.parent_session_id.as_deref())
        .and_then(|parent| by_id.get(parent).copied())
    {
        if seen.contains(&parent.id.as_str()) {
            break;
        }
        seen.push(parent.id.as_str());
        root = parent;
    }
    // Everything reachable downward from the root.
    let mut family: Vec<CodingSession> = vec![root.clone()];
    let mut frontier: Vec<String> = vec![root.id.clone()];
    while let Some(id) = frontier.pop() {
        for row in sessions {
            if row.parent_session_id.as_deref() == Some(id.as_str())
                && !family.iter().any(|kept| kept.id == row.id)
            {
                family.push(row.clone());
                frontier.push(row.id.clone());
            }
        }
    }
    // The root's own row first, then the rest in the caller's order — the
    // nesting rule re-sorts children by start anyway.
    family
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_stack::tests::issue;

    fn batch_issue(identifier: &str, head: &str, base: Option<&str>, pr: &str) -> Issue {
        let mut row = issue(identifier, Some(head), base);
        row.pr_url = Some(pr.to_string());
        row
    }

    fn run(id: &str, parent: Option<&str>) -> CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "parent_session_id": parent,
            "status": "running",
            "started_at": "2026-09-10T10:00:00Z",
        }))
        .unwrap()
    }

    #[test]
    fn reports_a_stack_badge_for_a_stacked_pr() {
        let issues = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let graph = pr_graph(Some(&issues[0]), None, &issues, &[]);
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Stack));
        assert_eq!(graph.stack.len(), 2);
        // Bottom first, depth 0 upward.
        assert_eq!(graph.stack[0].entry.identifiers(), "EXP-11");
        assert_eq!(graph.stack[1].depth, 1);
        let position = graph.position().unwrap();
        assert_eq!((position.position, position.size), (2, 2));
        assert_eq!(position.below.as_deref(), Some("EXP-11"));
        assert_eq!(graph.bottom().unwrap().identifiers(), "EXP-11");
        assert!(graph.batch.is_none());
    }

    #[test]
    fn reports_a_batch_badge_for_a_batch_pr() {
        let issues = vec![
            batch_issue("EXP-20", "exp/batch-a1b2c3d4", Some("master"), "u/1"),
            batch_issue("EXP-21", "exp/batch-a1b2c3d4", Some("master"), "u/1"),
        ];
        let graph = pr_graph(Some(&issues[0]), None, &issues, &[]);
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Batch));
        assert_eq!(graph.batch.as_ref().unwrap().issues.len(), 2);
        assert!(graph.stack.is_empty());
        // The two issues are ONE pull request, never two rows.
        assert_eq!(pr_entries(&issues).len(), 1);
        assert_eq!(pr_entries(&issues)[0].identifiers(), "EXP-20, EXP-21");
    }

    /// EXP-876: the pill and its sheet are the surface built to name work
    /// that spans several issues — and a batch RUN, which spans them,
    /// resolved nothing at all before this (it links no issue and stamps no
    /// `pr_url`). Mirrored ×4.
    #[test]
    fn reports_a_batch_badge_for_a_batch_run_before_its_pr() {
        let issues = vec![
            issue("EXP-20", None, None),
            issue("EXP-21", None, None),
        ];
        let mut batch = run("run-1", None);
        batch.batch_issue_ids = Some(serde_json::json!(["id-EXP-20", "id-EXP-21"]));
        let graph = pr_graph(None, Some(&batch), &issues, std::slice::from_ref(&batch));
        assert_eq!(badge_kind(&graph), Some(BadgeKind::Batch));
        // The composer's order, so the sheet reads like the row that named it.
        assert_eq!(
            graph.batch.as_ref().unwrap().issues.len(),
            2,
            "the covered set is the entry"
        );
        assert_eq!(
            graph
                .batch
                .as_ref()
                .unwrap()
                .issues
                .iter()
                .map(|issue| issue.identifier.as_str())
                .collect::<Vec<_>>(),
            vec!["EXP-20", "EXP-21"]
        );
        // No pull request yet: a batch of two is not a stack of two.
        assert!(graph.stack.is_empty());
    }

    #[test]
    fn reports_a_batch_runs_pull_request_once_it_opens() {
        // The GROUPED entry wins the moment the PR exists: it carries the
        // branch and the base the stack chains on, so a stacked batch still
        // reads `stack+batch` from its run.
        let issues = vec![
            batch_issue("EXP-20", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            batch_issue("EXP-21", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let mut batch = run("run-1", None);
        batch.batch_issue_ids = Some(serde_json::json!(["id-EXP-20", "id-EXP-21"]));
        batch.branch = Some("exp/batch-a1b2c3d4".to_string());
        let graph = pr_graph(None, Some(&batch), &issues, std::slice::from_ref(&batch));
        assert_eq!(badge_kind(&graph), Some(BadgeKind::StackAndBatch));
        assert_eq!(graph.stack.len(), 2);
        assert_eq!(graph.batch.as_ref().unwrap().issues.len(), 2);
    }

    #[test]
    fn reports_nothing_for_a_batch_run_whose_issues_are_unknown() {
        // No stored ids and no PR: the row reads "Batch run" and wears no
        // pill, rather than a pill that could say nothing.
        let bare = run("run-1", None);
        let graph = pr_graph(None, Some(&bare), &[], std::slice::from_ref(&bare));
        assert_eq!(badge_kind(&graph), None);
        assert!(graph.batch.is_none());
        // An ACTION run is never a batch, whatever else it carries.
        let issues = vec![issue("EXP-20", None, None)];
        let mut action = run("run-2", None);
        action.action_name = Some("Chat".to_string());
        action.batch_issue_ids = Some(serde_json::json!(["id-EXP-20"]));
        let graph = pr_graph(None, Some(&action), &issues, std::slice::from_ref(&action));
        assert!(graph.batch.is_none());
    }

    #[test]
    fn reports_both_for_a_batch_inside_a_stack() {
        let issues = vec![
            batch_issue("EXP-20", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            batch_issue("EXP-21", "exp/batch-a1b2c3d4", Some("exp/EXP-11"), "u/2"),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let graph = pr_graph(Some(&issues[0]), None, &issues, &[]);
        assert_eq!(badge_kind(&graph), Some(BadgeKind::StackAndBatch));
        // A stack member is a PULL REQUEST: the batch is ONE member, not two.
        assert_eq!(graph.stack.len(), 2);
        assert!(graph.stack[1].entry.is_batch());
        assert_eq!(graph.stack[1].entry.identifiers(), "EXP-20, EXP-21");
        let position = graph.position().unwrap();
        assert_eq!((position.position, position.size), (2, 2));
    }

    #[test]
    fn reports_nothing_for_a_lone_pr() {
        let issues = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        let graph = pr_graph(Some(&issues[0]), None, &issues, &[]);
        assert_eq!(badge_kind(&graph), None);
        assert!(graph.stack.is_empty());
        assert!(graph.batch.is_none());
        assert!(graph.tree.is_empty());
        // An issue with no pull request at all is just as quiet.
        let bare = vec![issue("EXP-31", None, None)];
        assert_eq!(badge_kind(&pr_graph(Some(&bare[0]), None, &bare, &[])), None);
    }

    /// EXP-1058 — mirrors the web's "names the representative issue and the
    /// count on the stacked chip" (`pr-graph.test.ts`).
    #[test]
    fn badge_chip_names_the_representative_issue_and_the_count() {
        // A batch inside a stack: the subject PR's representative, every
        // other issue on the stack behind it.
        let lower = issue("LOWER", Some("exp/LOWER"), Some("master"));
        let one = batch_issue("ONE", "exp/batch-abcd1234", Some("exp/LOWER"), "u/9");
        let two = batch_issue("TWO", "exp/batch-abcd1234", Some("exp/LOWER"), "u/9");
        let issues = vec![lower.clone(), one.clone(), two.clone()];
        let both = pr_graph(Some(&two), None, &issues, &[]);
        let chip = badge_chip(&both).unwrap();
        assert_eq!(chip.issue.as_ref().map(|issue| issue.id.as_str()), Some("id-ONE"));
        assert_eq!(chip.count, 2);
        // A plain batch: the others of the batch.
        let mut loose = two.clone();
        loose.pr_base_branch = None;
        let issues = vec![one.clone(), loose];
        let batch = pr_graph(Some(&one), None, &issues, &[]);
        assert_eq!(badge_chip(&batch).unwrap().count, 1);
        // A run family with no issue: no front issue, the other runs behind.
        let sessions = vec![run("child", Some("root")), run("root", None)];
        let family = pr_graph(None, Some(&sessions[0]), &[], &sessions);
        assert_eq!(
            badge_chip(&family),
            Some(BadgeChip { issue: None, count: 1 })
        );
        // No relation, no family, no blocker = no chip.
        let lone = pr_graph(None, Some(&sessions[1]), &[], &sessions[1..]);
        assert_eq!(badge_chip(&lone), None);
    }

    /// EXP-1097 — the chip is FACE-INDEPENDENT: stack/batch first, then the
    /// run tree when it has a family, then the open blockers (front = the
    /// first blocker, `+N` = the rest). Mirrors web `pr-graph.test.ts`.
    #[test]
    fn badge_shape_ranks_stack_batch_then_runs_then_blockers() {
        let subject = issue("EXP-40", None, None);
        let blockers = vec![issue("EXP-41", None, None), issue("EXP-42", None, None)];
        let mut graph = pr_graph(Some(&subject), None, std::slice::from_ref(&subject), &[]);
        assert_eq!(badge_shape(&graph), None);
        graph.blocked_by = blockers.clone();
        assert_eq!(badge_shape(&graph), Some(BadgeShape::Blocked));
        let chip = badge_chip(&graph).unwrap();
        assert_eq!(chip.issue.map(|issue| issue.identifier), Some("EXP-41".to_string()));
        assert_eq!(chip.count, 1);
        // One blocker: the blocker alone, nothing behind it.
        graph.blocked_by.truncate(1);
        assert_eq!(badge_chip(&graph).unwrap().count, 0);

        // A family of runs on the issue outranks the blockers.
        let mut root = run("root", None);
        root.issue_id = Some(subject.id.clone());
        let sessions = vec![root, run("child", Some("root")), run("other", None)];
        let mut family = pr_graph(Some(&subject), None, std::slice::from_ref(&subject), &sessions);
        family.blocked_by = blockers.clone();
        assert_eq!(badge_shape(&family), Some(BadgeShape::Runs));
        assert_eq!(badge_chip(&family).unwrap().count, 1);
        assert_eq!(family.tree.len(), 2, "the issue's runs, never an unrelated one");

        // A stack outranks both.
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let mut stack = pr_graph(Some(&stacked[0]), None, &stacked, &[]);
        stack.blocked_by = blockers;
        assert_eq!(badge_shape(&stack), Some(BadgeShape::Stack));
        assert_eq!(badge_chip(&stack).unwrap().issue.unwrap().identifier, "EXP-12");
    }

    /// EXP-1097 — the face orders the overlay, it never hides a relation.
    #[test]
    fn overlay_sections_lead_with_the_face() {
        use OverlaySection::*;
        let stacked = vec![
            issue("EXP-12", Some("exp/EXP-12"), Some("exp/EXP-11")),
            issue("EXP-11", Some("exp/EXP-11"), Some("master")),
        ];
        let mut graph = pr_graph(Some(&stacked[0]), None, &stacked, &[]);
        graph.blocked_by = vec![issue("EXP-9", None, None)];
        assert_eq!(overlay_sections(&graph, PrGraphFace::Issue), vec![Blocked, Stack]);
        assert_eq!(overlay_sections(&graph, PrGraphFace::Run), vec![Stack, Blocked]);
        assert_eq!(overlay_sections(&graph, PrGraphFace::Changes), vec![Stack, Blocked]);
        // A lone pull request leads the Changes face only.
        let lone = vec![issue("EXP-30", Some("exp/EXP-30"), Some("master"))];
        let mut graph = pr_graph(Some(&lone[0]), None, &lone, &[]);
        graph.blocked_by = vec![issue("EXP-9", None, None)];
        assert_eq!(overlay_sections(&graph, PrGraphFace::Changes), vec![Stack, Blocked]);
        assert_eq!(overlay_sections(&graph, PrGraphFace::Issue), vec![Blocked]);
    }

    /// Web parity: a run with no explicit issue still fronts the issue its
    /// `issue_id` names, so the Run face's chip carries it.
    #[test]
    fn a_runs_subject_issue_comes_off_its_issue_id() {
        let one = issue("ONE", Some("exp/ONE"), None);
        let mut root = run("root", None);
        root.issue_id = Some(one.id.clone());
        let sessions = vec![root.clone(), run("child", Some("root"))];
        let graph = pr_graph(None, Some(&root), std::slice::from_ref(&one), &sessions);
        assert_eq!(
            badge_chip(&graph),
            Some(BadgeChip {
                issue: Some(one),
                count: 1
            })
        );
    }

    #[test]
    fn the_tree_carries_the_subject_runs_whole_family() {
        let sessions = vec![run("root", None), run("child", Some("root")), run("other", None)];
        let issues: Vec<Issue> = Vec::new();
        let graph = pr_graph(None, Some(&sessions[1]), &issues, &sessions);
        let ids: Vec<&str> = graph
            .tree
            .iter()
            .map(|row| row.session.id.as_str())
            .collect();
        assert_eq!(ids, ["root", "child"]);
        assert_eq!(graph.tree[1].depth, 1);
        // A run with neither a parent nor children has no tree to show.
        let alone = pr_graph(None, Some(&sessions[2]), &issues, &sessions);
        assert!(alone.tree.is_empty());
    }
}
