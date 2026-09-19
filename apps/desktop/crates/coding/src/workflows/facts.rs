//! EXP-983 — the git FACTS a pass needs, shared by both hosts (the desktop
//! GUI's `ui::workflow_host` and the CLI daemon's worker).
//!
//! The engine is pure: it is told which branches exist at which sha
//! ([`super::Snapshot::tips`]) and which node pairs collide
//! ([`super::Snapshot::conflicts`]). Working that out is IO, and it happens
//! here — once per repository per beat.
//!
//! Collision testing is deliberately CHEAP. Only pairs that can matter are
//! tested at all: both nodes still in play, both with a branch, sharing a
//! dependent (work that never meets may differ forever), and with `touches`
//! globs that could overlap — an empty glob list means "unknown", which
//! tests. Each verdict is cached against the two tips it was decided at, so
//! a quiet beat runs no git at all.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::git_worktree::TokenUrl;

use super::base::merge_conflicts;
use super::state::conflict_key;

/// What the collision pre-filter reads off one node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeGit {
    pub id: String,
    /// The node's own head branch; `None` = nothing to test yet.
    pub branch: Option<String>,
    /// `workflow_nodes.touches` — the path globs the node expects to change.
    pub touches: Vec<String>,
    /// The node's state: a landed or skipped node collides with nobody.
    pub state: String,
}

/// The node pairs worth a `git merge-tree`, as sorted `(a, b)` ids.
pub fn conflict_candidates(nodes: &[NodeGit], edges: &[(String, String)]) -> Vec<(String, String)> {
    let mut dependents: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (from, to) in edges {
        dependents
            .entry(from.as_str())
            .or_default()
            .insert(to.as_str());
    }
    let live: Vec<&NodeGit> = nodes
        .iter()
        .filter(|node| node.branch.is_some())
        .filter(|node| !matches!(node.state.as_str(), "landed" | "skipped"))
        .collect();
    let mut pairs = Vec::new();
    for (index, left) in live.iter().enumerate() {
        for right in live.iter().skip(index + 1) {
            let shared = dependents
                .get(left.id.as_str())
                .zip(dependents.get(right.id.as_str()))
                .is_some_and(|(a, b)| a.intersection(b).next().is_some());
            if !shared || !globs_could_overlap(&left.touches, &right.touches) {
                continue;
            }
            let pair = if left.id <= right.id {
                (left.id.clone(), right.id.clone())
            } else {
                (right.id.clone(), left.id.clone())
            };
            pairs.push(pair);
        }
    }
    pairs.sort();
    pairs.dedup();
    pairs
}

/// Whether two `touches` lists could name the same file. An empty list means
/// the node never said, which is "maybe"; otherwise two globs overlap when
/// one's literal prefix contains the other's.
pub fn globs_could_overlap(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return true;
    }
    left.iter().any(|a| {
        let a = literal_prefix(a);
        right.iter().any(|b| {
            let b = literal_prefix(b);
            a.starts_with(b) || b.starts_with(a)
        })
    })
}

/// The part of a glob before its first wildcard — everything a match has to
/// start with.
fn literal_prefix(glob: &str) -> &str {
    match glob.find(['*', '?', '[', '{']) {
        Some(at) => &glob[..at],
        None => glob,
    }
}

/// Test the candidate pairs at the CURRENT tips, reusing `cache` and filling
/// it with whatever had to be decided. Returns the colliding pairs.
pub fn detect_conflicts(
    clone: &Path,
    candidates: &[(String, String)],
    branch_of: &HashMap<String, String>,
    tips: &HashMap<String, String>,
    cache: &mut HashMap<String, bool>,
    url: Option<&TokenUrl>,
) -> HashSet<(String, String)> {
    let mut found = HashSet::new();
    for (left, right) in candidates {
        let (Some(left_branch), Some(right_branch)) = (branch_of.get(left), branch_of.get(right))
        else {
            continue;
        };
        // A branch origin has not seen yet cannot be tested.
        let (Some(left_sha), Some(right_sha)) = (tips.get(left_branch), tips.get(right_branch))
        else {
            continue;
        };
        let key = conflict_key((left, left_sha), (right, right_sha));
        let verdict = match cache.get(&key) {
            Some(cached) => *cached,
            None => {
                let ours = format!("refs/remotes/origin/{left_branch}");
                let theirs = format!("refs/remotes/origin/{right_branch}");
                match merge_conflicts(clone, &ours, &theirs, url) {
                    Ok(verdict) => {
                        cache.insert(key, verdict);
                        verdict
                    }
                    Err(err) => {
                        log::warn!("[workflows] merge-tree {left_branch}/{right_branch} — {err}");
                        continue;
                    }
                }
            }
        };
        if verdict {
            found.insert((left.clone(), right.clone()));
        }
    }
    found
}

/// The branch a node's run pushes to, by CONVENTION: the launcher cuts
/// `exp/<IDENTIFIER>` for an issue run. A node with no run yet has none.
pub fn conventional_branch(identifier: &str) -> String {
    crate::git_worktree::branch_name(crate::batch_launcher::RUN_BRANCH_PREFIX, identifier)
}

/// Keep only the branches origin actually HAS. Nothing may base on a branch
/// that is not up: a run's first push is what makes its branch real, and a
/// beat whose `ls-remote` failed carries no branches at all, which leaves
/// every speculative node blocked rather than cut from thin air.
///
/// EXP-984: the same pass fills [`super::Snapshot::pr_head`] — the tip of
/// each node's OWN branch, which is what an agent review runs against.
pub fn confine_branches_to_tips(snapshot: &mut super::Snapshot) {
    let mut heads = HashMap::new();
    for node in &mut snapshot.nodes {
        match node
            .branch
            .as_ref()
            .and_then(|branch| snapshot.tips.get(branch))
        {
            Some(sha) => {
                heads.insert(node.id.clone(), sha.clone());
            }
            None => node.branch = None,
        }
    }
    snapshot.pr_head = heads;
}

/// Prune a conflict cache to the keys this pass could still ask for, so
/// settings.json does not grow one entry per tip pair for ever.
pub fn prune_conflict_cache(
    cache: &mut HashMap<String, bool>,
    candidates: &[(String, String)],
    branch_of: &HashMap<String, String>,
    tips: &HashMap<String, String>,
) {
    let keep: HashSet<String> = candidates
        .iter()
        .filter_map(|(left, right)| {
            let left_sha = tips.get(branch_of.get(left)?)?;
            let right_sha = tips.get(branch_of.get(right)?)?;
            Some(conflict_key((left, left_sha), (right, right_sha)))
        })
        .collect();
    cache.retain(|key, _| keep.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, branch: Option<&str>, touches: &[&str], state: &str) -> NodeGit {
        NodeGit {
            id: id.to_string(),
            branch: branch.map(str::to_string),
            touches: touches.iter().map(|glob| (*glob).to_string()).collect(),
            state: state.to_string(),
        }
    }

    /// Only pairs whose work has to MEET are tested, and only while both are
    /// still in play with a branch to test.
    #[test]
    fn only_siblings_that_share_a_dependent_are_candidates() {
        let nodes = vec![
            node("a", Some("exp/EXP-1"), &[], "running"),
            node("b", Some("exp/EXP-2"), &[], "running"),
            node("c", None, &[], "blocked"),
            // Landed, branchless and unrelated nodes all drop out.
            node("d", Some("exp/EXP-4"), &[], "landed"),
            node("e", Some("exp/EXP-5"), &[], "running"),
        ];
        let edges = vec![
            ("a".to_string(), "c".to_string()),
            ("b".to_string(), "c".to_string()),
            ("d".to_string(), "c".to_string()),
        ];
        assert_eq!(
            conflict_candidates(&nodes, &edges),
            vec![("a".to_string(), "b".to_string())]
        );
        // No shared dependent at all: nothing to test.
        assert!(conflict_candidates(&nodes, &[]).is_empty());
    }

    /// The glob pre-filter: disjoint trees are never tested, an unknown one
    /// always is.
    #[test]
    fn the_glob_filter_skips_work_that_cannot_meet() {
        let web = ["apps/web/**".to_string()];
        let ios = ["apps/ios/**".to_string()];
        let deep = ["apps/web/src/lib/**".to_string()];
        assert!(!globs_could_overlap(&web, &ios));
        assert!(globs_could_overlap(&web, &deep));
        assert!(globs_could_overlap(&web, &[]));
        assert!(globs_could_overlap(&[], &ios));
        assert!(globs_could_overlap(&web, &web));
    }

    /// A branch origin does not have is not a branch anything may base on:
    /// the run has not pushed yet, or this beat could not read the remote.
    #[test]
    fn only_branches_origin_has_survive() {
        assert_eq!(conventional_branch("EXP-42"), "exp/EXP-42");
        let mut snapshot = super::super::Snapshot {
            nodes: vec![
                super::super::NodeFacts {
                    id: "a".to_string(),
                    branch: Some("exp/EXP-1".to_string()),
                    ..Default::default()
                },
                super::super::NodeFacts {
                    id: "b".to_string(),
                    branch: Some("exp/EXP-2".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        snapshot
            .tips
            .insert("exp/EXP-1".to_string(), "sha-a1".to_string());
        confine_branches_to_tips(&mut snapshot);
        assert_eq!(snapshot.nodes[0].branch.as_deref(), Some("exp/EXP-1"));
        assert_eq!(snapshot.nodes[1].branch, None);
        // EXP-984: the same pass names the head a review would run against.
        assert_eq!(snapshot.pr_head.get("a").map(String::as_str), Some("sha-a1"));
        assert_eq!(snapshot.pr_head.get("b"), None);

        // A beat with no remote view at all leaves nothing to base on.
        snapshot.tips.clear();
        confine_branches_to_tips(&mut snapshot);
        assert!(snapshot.nodes.iter().all(|node| node.branch.is_none()));
        assert!(snapshot.pr_head.is_empty());
    }

    /// A cached verdict costs no git at all, and the cache keeps only the
    /// keys the current tips can still ask for.
    #[test]
    fn a_cached_verdict_is_reused_and_stale_keys_are_pruned() {
        let branch_of: HashMap<String, String> = [
            ("a".to_string(), "exp/EXP-1".to_string()),
            ("b".to_string(), "exp/EXP-2".to_string()),
        ]
        .into();
        let tips: HashMap<String, String> = [
            ("exp/EXP-1".to_string(), "sha-a1".to_string()),
            ("exp/EXP-2".to_string(), "sha-b1".to_string()),
        ]
        .into();
        let candidates = vec![("a".to_string(), "b".to_string())];
        let mut cache: HashMap<String, bool> = [
            (conflict_key(("a", "sha-a1"), ("b", "sha-b1")), true),
            (conflict_key(("a", "sha-a0"), ("b", "sha-b1")), false),
        ]
        .into();
        // The path is bogus on purpose: a cache hit must not reach git.
        let found = detect_conflicts(
            Path::new("/nonexistent"),
            &candidates,
            &branch_of,
            &tips,
            &mut cache,
            None,
        );
        assert_eq!(found, [("a".to_string(), "b".to_string())].into());

        prune_conflict_cache(&mut cache, &candidates, &branch_of, &tips);
        assert_eq!(
            cache.keys().collect::<Vec<_>>(),
            [&conflict_key(("a", "sha-a1"), ("b", "sha-b1"))],
            "the tip pair that moved on is dropped"
        );
    }
}
