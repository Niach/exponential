//! EXP-481: the worktree-inventory scan behind `devices.reportWorktrees` —
//! the server-persisted mirror that powers remote resume offers, the
//! device-settings worktree list, and queued prune/remove commands even
//! while this machine is offline.
//!
//! Blocking (one `git worktree list` + one `git status` per worktree) —
//! callers run it on a background executor/thread, never the gpui
//! foreground or the daemon's 1s loop. Per-clone errors are logged and
//! skipped: one broken repo must not blank the whole report.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::agent::CodingAgent;
use crate::git_worktree::list_worktrees;
use crate::prune::{worktree_dirty_state, DirtyState};
use crate::worktree_agents::worktree_agents;

/// The server-side `reportWorktrees` bound — the scan truncates
/// (deterministically, after sorting) rather than failing the whole report.
pub const MAX_REPORTED_WORKTREES: usize = 256;

/// One trunk clone under the repos root (`<root>/<owner>/<name>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloneRef {
    /// `owner/name`, reassembled from the directory layout.
    pub full_name: String,
    pub path: PathBuf,
}

/// The two-level `<root>/<owner>/<name>` walk — a directory is a clone iff
/// it carries a `.git`. `.worktrees`/`.cargo-target` siblings never match
/// (they have no `.git` dir of their own at that level). Sorted by full
/// name for deterministic reports.
pub fn scan_clones(repos_root: &Path) -> Vec<CloneRef> {
    let mut out = Vec::new();
    let Ok(owners) = std::fs::read_dir(repos_root) else {
        return out;
    };
    for owner in owners.flatten() {
        if !owner.path().is_dir() {
            continue;
        }
        let owner_name = owner.file_name().to_string_lossy().into_owned();
        let Ok(repos) = std::fs::read_dir(owner.path()) else {
            continue;
        };
        for repo in repos.flatten() {
            let path = repo.path();
            if !path.is_dir() || !path.join(".git").exists() {
                continue;
            }
            let repo_name = repo.file_name().to_string_lossy().into_owned();
            out.push(CloneRef {
                full_name: format!("{owner_name}/{repo_name}"),
                path,
            });
        }
    }
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    out
}

/// One session worktree in the report (the wire row minus `busy`, which the
/// caller stamps from its live-session registry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInventoryEntry {
    /// The clone's `owner/name`.
    pub repo: String,
    /// The checked-out branch (`exp/EXP-42`, `exp/batch-<id8>`, …).
    pub branch: String,
    pub path: PathBuf,
    pub dirty: DirtyState,
    /// `.exp-agents` marker contents; `None` = pre-marker worktree whose
    /// agent history is unknown (readers treat it as "any agent may resume").
    pub agents: Option<Vec<CodingAgent>>,
}

impl WorktreeInventoryEntry {
    /// The wire `dirty` vocabulary (`clean`/`untracked`/`tracked` — the
    /// server degrades anything newer to `unknown`).
    pub fn dirty_wire(&self) -> &'static str {
        match self.dirty {
            DirtyState::Clean => "clean",
            DirtyState::UntrackedOnly => "untracked",
            DirtyState::TrackedChanges => "tracked",
        }
    }

    /// The issue identifier this worktree's branch links (the server's
    /// `parseIssueIdentifierFromBranch` port): the last `/`-segment must be
    /// entirely `[A-Z0-9]+-\d+`. Batch branches (`exp/batch-<id8>`) never
    /// match by construction.
    pub fn issue_identifier(&self) -> Option<&str> {
        let tail = self.branch.rsplit('/').next().unwrap_or(&self.branch);
        let dash = tail.rfind('-')?;
        let (head, digits) = (&tail[..dash], &tail[dash + 1..]);
        let valid = !head.is_empty()
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        valid.then_some(tail)
    }
}

/// Every session worktree under every clone of `repos_root`, sorted by
/// (repo, branch) and truncated to [`MAX_REPORTED_WORKTREES`]. The clone's
/// own primary worktree and detached checkouts are skipped — nothing
/// actionable remotely.
pub fn scan_inventory(repos_root: &Path) -> Vec<WorktreeInventoryEntry> {
    let mut out = Vec::new();
    for clone in scan_clones(repos_root) {
        let entries = match list_worktrees(&clone.path) {
            Ok(entries) => entries,
            Err(err) => {
                log::debug!("inventory: {} unreadable — skipped ({err})", clone.full_name);
                continue;
            }
        };
        // The first entry is always the main working tree.
        for entry in entries.into_iter().skip(1) {
            if entry.path == clone.path {
                continue;
            }
            let Some(branch) = entry.branch else {
                continue;
            };
            out.push(WorktreeInventoryEntry {
                repo: clone.full_name.clone(),
                branch,
                dirty: worktree_dirty_state(&entry.path),
                agents: worktree_agents(&entry.path),
                path: entry.path,
            });
        }
    }
    out.sort_by(|a, b| (a.repo.as_str(), a.branch.as_str()).cmp(&(b.repo.as_str(), b.branch.as_str())));
    out.truncate(MAX_REPORTED_WORKTREES);
    out
}

/// Change-damping key over everything the report carries (branch set, dirty
/// states, markers, busy flags) — senders re-report only when it moves.
/// Process-stable only (std `DefaultHasher`); a spurious first report after
/// a restart is a no-op server-side (the diff-upsert is change-guarded).
pub fn inventory_fingerprint(
    entries: &[WorktreeInventoryEntry],
    busy_branches: &HashSet<String>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry in entries {
        entry.repo.hash(&mut hasher);
        entry.branch.hash(&mut hasher);
        entry.dirty_wire().hash(&mut hasher);
        match &entry.agents {
            None => (-1i8).hash(&mut hasher),
            Some(agents) => {
                for agent in agents {
                    agent.id().hash(&mut hasher);
                }
            }
        }
        busy_branches.contains(&entry.branch).hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_worktree::{create_worktree, TokenUrl};
    use crate::worktree_agents::record_worktree_agent;
    use std::fs;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-coding-inventory-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// `<root>/acme/web` seeded as a real clone with a local origin.
    fn seed_repo(root: &Path, owner: &str, name: &str) -> PathBuf {
        let origin = root.join(format!(".origins-{owner}-{name}"));
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        let clone = root.join(owner).join(name);
        fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(
            root,
            &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );
        clone
    }

    #[test]
    fn scan_walks_owner_name_and_reports_worktrees() {
        let dir = temp_dir("scan");
        let clone = seed_repo(&dir.0, "acme", "web");
        let wt = create_worktree(&clone, "exp/EXP-7", "origin/main", &TokenUrl::new("acme/web", "ghs_dead")).unwrap();
        record_worktree_agent(&wt, CodingAgent::Codex).unwrap();
        // Untracked debris flips the dirty state.
        fs::write(wt.join("scratch.txt"), "debris\n").unwrap();

        let clones = scan_clones(&dir.0);
        assert_eq!(clones.len(), 1);
        assert_eq!(clones[0].full_name, "acme/web");

        let inventory = scan_inventory(&dir.0);
        assert_eq!(inventory.len(), 1);
        let entry = &inventory[0];
        assert_eq!(entry.repo, "acme/web");
        assert_eq!(entry.branch, "exp/EXP-7");
        assert_eq!(entry.dirty, DirtyState::UntrackedOnly);
        assert_eq!(entry.dirty_wire(), "untracked");
        assert_eq!(entry.agents, Some(vec![CodingAgent::Codex]));
        assert_eq!(entry.issue_identifier(), Some("EXP-7"));
    }

    #[test]
    fn batch_branches_carry_no_issue_identifier() {
        let entry = WorktreeInventoryEntry {
            repo: "acme/web".into(),
            branch: "exp/batch-a1b2c3d4".into(),
            path: PathBuf::new(),
            dirty: DirtyState::Clean,
            agents: None,
        };
        assert_eq!(entry.issue_identifier(), None);
        let issue = WorktreeInventoryEntry {
            branch: "feat/EXP-42".into(),
            ..entry.clone()
        };
        assert_eq!(issue.issue_identifier(), Some("EXP-42"));
    }

    #[test]
    fn fingerprint_moves_with_dirty_marker_and_busy_changes() {
        let base = WorktreeInventoryEntry {
            repo: "acme/web".into(),
            branch: "exp/EXP-1".into(),
            path: PathBuf::new(),
            dirty: DirtyState::Clean,
            agents: Some(vec![CodingAgent::Claude]),
        };
        let empty = HashSet::new();
        let busy: HashSet<String> = [base.branch.clone()].into();
        let fp = inventory_fingerprint(&[base.clone()], &empty);
        assert_eq!(fp, inventory_fingerprint(&[base.clone()], &empty), "stable");
        assert_ne!(fp, inventory_fingerprint(&[base.clone()], &busy), "busy");
        let dirty = WorktreeInventoryEntry {
            dirty: DirtyState::TrackedChanges,
            ..base.clone()
        };
        assert_ne!(fp, inventory_fingerprint(&[dirty], &empty), "dirty");
        let unmarked = WorktreeInventoryEntry {
            agents: None,
            ..base
        };
        assert_ne!(fp, inventory_fingerprint(&[unmarked], &empty), "marker");
    }

    #[test]
    fn missing_root_scans_empty() {
        let dir = temp_dir("missing");
        assert!(scan_clones(&dir.0.join("never")).is_empty());
        assert!(scan_inventory(&dir.0.join("never")).is_empty());
    }
}
