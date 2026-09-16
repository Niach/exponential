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
//! [`pr_graph`] derives all three for one subject (an issue, or a run) so the
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
        });

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

    // The run tree: the subject run's whole family, nested. A run with no
    // parent and no children is alone — no tree to show.
    let tree = session
        .map(|session| {
            let family = session_family(session, sessions);
            if family.len() < 2 {
                return Vec::new();
            }
            session_tree::nest_sessions(
                family,
                |row| row.id.as_str(),
                |row| row.parent_session_id.as_deref(),
                |row| row.started_at.as_deref(),
            )
        })
        .unwrap_or_default();

    PrGraph {
        subject_branch: subject_entry
            .as_ref()
            .and_then(|entry| entry.branch())
            .map(str::to_string),
        stack,
        batch,
        tree,
    }
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
