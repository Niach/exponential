//! Landed-work pruning of session worktrees and stale coding branches
//! (EXP-465, superseding the EXP-76 pr_state-only rule).
//!
//! The old prune asked one question — "does a synced issue with
//! `pr_state == merged` map to this branch?" — which missed everything the
//! tracker no longer tells us about: deleted issues, issues closed without a
//! PR, abandoned batch branches. This engine asks GIT whether a branch's work
//! has **landed** in the repo's effective default branch, and lets the caller
//! layer the issue-state facts on top via [`PrunePolicy`]:
//!
//! * `keep` — branches that must never be touched (unfinished issues, live
//!   sessions, busy terminal tabs). Callers err toward keeping.
//! * `merged` — branches whose issue's PR is merged: landed by definition,
//!   pruned even when the local `origin/<default>` ref is stale.
//! * `finished` — branches whose issue is done/cancelled/duplicate: pruned
//!   only when git agrees the work landed.
//! * everything else prefix-matched (`exp/…`, `exp/batch-…`): the tracker has
//!   no opinion (issue deleted, branch never linked) — git truth alone
//!   decides.
//!
//! **Landed** = the branch is an ancestor of `origin/<default>`, OR every
//! commit is patch-equivalent to an upstream one (`git cherry`, catches
//! rebases), OR the branch's WHOLE diff squashed into one probe commit is
//! patch-equivalent (`commit-tree` + `cherry` — the load-bearing check for
//! squash-merge workflows, where a multi-commit branch lands as one commit
//! and neither ancestry nor per-commit cherry can see it).
//!
//! Dirty handling (EXP-465 decision): untracked-only leftovers do NOT protect
//! a landed worktree — they are exactly the debris (compose overrides, stray
//! agent files) that used to park worktrees forever — so those removals pass
//! `--force`. Modified or staged TRACKED files always protect the worktree
//! and are reported. A landed branch is deleted together with its worktree
//! (nothing unpushed can be lost once the work is in the default branch), and
//! the optional branch-only pass sweeps prefix branches whose worktree is
//! already gone.
//!
//! All git here is local and tokenless (`run_git` with `url: None`) — pruning
//! never touches the network; callers fetch on their own cadence.

use std::collections::HashSet;
use std::path::Path;

use crate::git_worktree::{list_worktrees, run_git, validate_branch_arg};

/// What the caller knows (issue state, live sessions) and wants (branch
/// sweep), fed into [`prune_landed`]. All sets hold full branch names
/// (`exp/EXP-42`).
#[derive(Clone, Debug, Default)]
pub struct PrunePolicy {
    /// Branch namespaces this engine may touch at all (typically the user's
    /// branch prefix plus the hardcoded `exp/batch-` — batch branches ignore
    /// the settings prefix). Branches outside every prefix are only ever
    /// candidates via `merged`/`finished` (an explicit issue mapping).
    pub prefixes: Vec<String>,
    /// The server-resolved effective default branch, when known. Never a
    /// fabricated `main` — [`effective_default_branch`] falls back to the
    /// clone's own `origin/HEAD`, then its checked-out branch.
    pub default_branch: Option<String>,
    /// Never touch these (unfinished issues, live sessions).
    pub keep: HashSet<String>,
    /// Paths currently in use (running terminal tab cwds): any worktree that
    /// is — or contains — one of these is kept, whatever its branch says.
    pub busy_paths: Vec<std::path::PathBuf>,
    /// Issue `pr_state == merged` — landed by definition, no git check.
    pub merged: HashSet<String>,
    /// Issue finished (done/cancelled/duplicate) — prune iff git agrees.
    pub finished: HashSet<String>,
    /// Also delete landed prefix branches that have no worktree anymore.
    pub delete_stale_branches: bool,
}

/// Why a nominated worktree survived the pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Modified or staged tracked files — real work, never force-removed.
    TrackedChanges,
    /// The branch has commits the default branch does not contain.
    NotLanded,
    /// `git worktree remove` failed (locked, permissions, …).
    RemoveFailed(String),
    /// No default branch could be resolved, so landed checks were impossible
    /// (`merged` candidates still pruned).
    NoDefaultBranch,
}

impl SkipReason {
    /// Short human-readable reason for result lines and logs.
    pub fn describe(&self, default_branch: Option<&str>) -> String {
        match self {
            Self::TrackedChanges => "uncommitted changes".to_string(),
            Self::NotLanded => match default_branch {
                Some(branch) => format!("not merged into {branch}"),
                None => "not merged".to_string(),
            },
            Self::RemoveFailed(detail) => detail.clone(),
            Self::NoDefaultBranch => "no default branch to compare against".to_string(),
        }
    }
}

/// Outcome of one [`prune_landed`] pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Branches whose worktrees were removed (branch deleted too).
    pub removed_worktrees: Vec<String>,
    /// Every branch deleted — worktree-attached and branch-only sweeps.
    pub deleted_branches: Vec<String>,
    /// Nominated worktrees that survived, with the reason.
    pub skipped: Vec<(String, SkipReason)>,
    /// The default branch the landed checks compared against, for messages.
    pub default_branch: Option<String>,
}

impl PruneReport {
    /// Nothing removed, nothing to report on.
    pub fn is_empty(&self) -> bool {
        self.removed_worktrees.is_empty()
            && self.deleted_branches.is_empty()
            && self.skipped.is_empty()
    }
}

/// A worktree's `git status --porcelain`, split the way the prune cares:
/// untracked-only debris is removable, tracked changes are sacred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    UntrackedOnly,
    TrackedChanges,
}

/// Classify a worktree's status. A failing `git status` counts as
/// [`DirtyState::TrackedChanges`] — never prune what we couldn't inspect.
pub fn worktree_dirty_state(worktree: &Path) -> DirtyState {
    let Ok(output) = run_git(Some(worktree), &["status", "--porcelain"], None, "git status")
    else {
        return DirtyState::TrackedChanges;
    };
    let mut untracked_only = true;
    let mut any = false;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        any = true;
        if !line.starts_with("?? ") {
            untracked_only = false;
        }
    }
    match (any, untracked_only) {
        (false, _) => DirtyState::Clean,
        (true, true) => DirtyState::UntrackedOnly,
        (true, false) => DirtyState::TrackedChanges,
    }
}

/// Resolve the branch the landed checks compare against: the server-supplied
/// effective default when its `origin/<b>` ref exists locally, else the
/// clone's `origin/HEAD` symref, else the trunk's checked-out branch. `None`
/// when nothing verifies — never a fabricated `main`/`master`.
pub fn effective_default_branch(clone: &Path, server_default: Option<&str>) -> Option<String> {
    let verified = |branch: &str| -> bool {
        validate_branch_arg(branch, "prune default branch").is_ok()
            && run_git(
                Some(clone),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/origin/{branch}"),
                ],
                None,
                "git rev-parse origin default",
            )
            .is_ok()
    };
    if let Some(branch) = server_default.filter(|branch| !branch.is_empty()) {
        if verified(branch) {
            return Some(branch.to_string());
        }
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["symbolic-ref", "refs/remotes/origin/HEAD"],
        None,
        "git symbolic-ref origin/HEAD",
    ) {
        if let Some(branch) = output.trim().strip_prefix("refs/remotes/origin/") {
            if verified(branch) {
                return Some(branch.to_string());
            }
        }
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["symbolic-ref", "--short", "HEAD"],
        None,
        "git symbolic-ref HEAD",
    ) {
        let branch = output.trim();
        if !branch.is_empty() && verified(branch) {
            return Some(branch.to_string());
        }
    }
    None
}

/// Branches already reachable from `origin_ref`, in ONE git call — the fast
/// path of the landed check across an arbitrary branch count.
fn ancestor_branches(clone: &Path, origin_ref: &str) -> HashSet<String> {
    run_git(
        Some(clone),
        &[
            "for-each-ref",
            "refs/heads",
            "--format=%(refname:short)",
            "--merged",
            origin_ref,
        ],
        None,
        "git for-each-ref --merged",
    )
    .map(|output| output.lines().map(|line| line.trim().to_string()).collect())
    .unwrap_or_default()
}

/// Whether `branch`'s work is contained in `origin_ref`. Three probes, cheap
/// to expensive: ancestry (pre-batched), per-commit patch equivalence
/// (`git cherry`, catches rebases), then the squash probe — synthesize ONE
/// commit carrying the branch's whole diff over the merge-base and cherry
/// THAT, which is the only shape a squash-merged multi-commit branch matches.
/// The probe writes a dangling commit object; gc reclaims it.
fn branch_landed(
    clone: &Path,
    branch: &str,
    origin_ref: &str,
    ancestors: &HashSet<String>,
) -> bool {
    if ancestors.contains(branch) {
        return true;
    }
    if let Ok(output) = run_git(
        Some(clone),
        &["cherry", origin_ref, branch],
        None,
        "git cherry",
    ) {
        if output.lines().all(|line| !line.starts_with('+')) {
            return true;
        }
    }
    let Ok(base) = run_git(
        Some(clone),
        &["merge-base", origin_ref, branch],
        None,
        "git merge-base",
    ) else {
        return false;
    };
    let base = base.trim().to_string();
    let tree = |rev: &str| {
        run_git(
            Some(clone),
            &["rev-parse", &format!("{rev}^{{tree}}")],
            None,
            "git rev-parse tree",
        )
        .map(|output| output.trim().to_string())
    };
    let (Ok(branch_tree), Ok(base_tree)) = (tree(branch), tree(&base)) else {
        return false;
    };
    if branch_tree == base_tree {
        // The branch nets to zero diff over its base — nothing to land.
        return true;
    }
    // Identity via -c: the probe must work without global git config, and
    // commit-tree refuses to run identity-less.
    let Ok(probe) = run_git(
        Some(clone),
        &[
            "-c",
            "user.name=exp-prune",
            "-c",
            "user.email=prune@exponential.local",
            "commit-tree",
            &branch_tree,
            "-p",
            &base,
            "-m",
            "exp-prune landed probe",
        ],
        None,
        "git commit-tree",
    ) else {
        return false;
    };
    run_git(
        Some(clone),
        &["cherry", origin_ref, probe.trim(), &base],
        None,
        "git cherry (squash probe)",
    )
    .map(|output| {
        let mut lines = output.lines().filter(|line| !line.trim().is_empty());
        matches!(lines.next(), Some(line) if line.starts_with('-')) && lines.next().is_none()
    })
    .unwrap_or(false)
}

/// Remove every landed session worktree of `clone` (+ its branch), then
/// optionally sweep landed prefix branches that lost their worktree long ago.
/// Blocking (spawned gits) — callers run it on a background executor.
pub fn prune_landed(clone: &Path, policy: &PrunePolicy) -> PruneReport {
    let mut report = PruneReport::default();
    let Ok(entries) = list_worktrees(clone) else {
        return report;
    };
    let default_branch = effective_default_branch(clone, policy.default_branch.as_deref());
    report.default_branch = default_branch.clone();
    let origin_ref = default_branch.as_deref().map(|branch| format!("origin/{branch}"));
    let ancestors = origin_ref
        .as_deref()
        .map(|origin_ref| ancestor_branches(clone, origin_ref))
        .unwrap_or_default();

    let prefix_matched = |branch: &str| {
        policy
            .prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && branch.starts_with(prefix.as_str()))
    };
    // A busy tab anywhere under a worktree parks it. Both sides canonicalized
    // when possible — git reports resolved paths, tab cwds may be symlinked.
    let busy = |worktree: &Path| {
        let canonical = worktree.canonicalize().ok();
        policy.busy_paths.iter().any(|path| {
            let path = path.canonicalize().unwrap_or_else(|_| path.clone());
            path.starts_with(worktree)
                || canonical
                    .as_deref()
                    .is_some_and(|worktree| path.starts_with(worktree))
        })
    };
    // `merged` needs no git agreement; `finished` and bare prefix matches do.
    // Returns None when the branch is no candidate at all (silent skip),
    // Some(Err(reason)) when nominated but not removable.
    let landed_verdict = |branch: &str| -> Option<Result<(), SkipReason>> {
        if policy.merged.contains(branch) {
            return Some(Ok(()));
        }
        if !policy.finished.contains(branch) && !prefix_matched(branch) {
            return None;
        }
        let Some(origin_ref) = origin_ref.as_deref() else {
            return Some(Err(SkipReason::NoDefaultBranch));
        };
        if branch_landed(clone, branch, origin_ref, &ancestors) {
            Some(Ok(()))
        } else {
            Some(Err(SkipReason::NotLanded))
        }
    };

    // ---- worktree pass (the first entry is always the main working tree) ----
    for entry in entries.iter().skip(1) {
        if entry.path == *clone {
            continue;
        }
        let Some(branch) = &entry.branch else {
            continue; // detached — no branch to reason about
        };
        if validate_branch_arg(branch, "prune worktree").is_err()
            || policy.keep.contains(branch)
            || Some(branch.as_str()) == default_branch.as_deref()
            || busy(&entry.path)
        {
            continue;
        }
        match landed_verdict(branch) {
            None => continue,
            Some(Err(reason)) => report.skipped.push((branch.clone(), reason)),
            Some(Ok(())) => {
                let path = entry.path.to_string_lossy().into_owned();
                let args: &[&str] = match worktree_dirty_state(&entry.path) {
                    DirtyState::TrackedChanges => {
                        report
                            .skipped
                            .push((branch.clone(), SkipReason::TrackedChanges));
                        continue;
                    }
                    DirtyState::Clean => &["worktree", "remove", &path],
                    // Untracked-only debris goes with the worktree — git
                    // refuses untracked files without --force.
                    DirtyState::UntrackedOnly => &["worktree", "remove", "--force", &path],
                };
                match run_git(Some(clone), args, None, &format!("git worktree remove ({branch})"))
                {
                    Ok(_) => {
                        report.removed_worktrees.push(branch.clone());
                        // Landed (or PR-merged) — the branch carries nothing
                        // unpushed anymore, so it goes too.
                        if run_git(
                            Some(clone),
                            &["branch", "-D", branch],
                            None,
                            &format!("git branch -D {branch}"),
                        )
                        .is_ok()
                        {
                            report.deleted_branches.push(branch.clone());
                        }
                    }
                    Err(err) => report
                        .skipped
                        .push((branch.clone(), SkipReason::RemoveFailed(err.detail))),
                }
            }
        }
    }

    // ---- branch-only sweep: landed prefix branches without a worktree ----
    if policy.delete_stale_branches {
        let checked_out: HashSet<String> = entries
            .iter()
            .filter_map(|entry| entry.branch.clone())
            .collect();
        let branches = run_git(
            Some(clone),
            &["for-each-ref", "refs/heads", "--format=%(refname:short)"],
            None,
            "git for-each-ref",
        )
        .map(|output| output.lines().map(|line| line.trim().to_string()).collect())
        .unwrap_or_else(|_| Vec::<String>::new());
        for branch in branches {
            if branch.is_empty()
                || validate_branch_arg(&branch, "prune branch").is_err()
                || !prefix_matched(&branch)
                || checked_out.contains(&branch)
                || policy.keep.contains(&branch)
                || Some(branch.as_str()) == default_branch.as_deref()
                || report.deleted_branches.contains(&branch)
            {
                continue;
            }
            let landed = policy.merged.contains(&branch)
                || origin_ref
                    .as_deref()
                    .is_some_and(|origin_ref| {
                        branch_landed(clone, &branch, origin_ref, &ancestors)
                    });
            if !landed {
                continue; // unlanded stale branches are kept, silently
            }
            if run_git(
                Some(clone),
                &["branch", "-D", &branch],
                None,
                &format!("git branch -D {branch}"),
            )
            .is_ok()
            {
                report.deleted_branches.push(branch);
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_worktree::{
        create_worktree, sanitize_branch_for_path, worktrees_dir, TokenUrl,
    };
    use std::fs;
    use std::path::PathBuf;
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
        path.push(format!("exp-coding-prune-{tag}-{}-{nanos}", std::process::id()));
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

    /// A local `main`-defaulted origin plus its clone (file://, no network).
    fn seed(dir: &Path) -> (PathBuf, PathBuf) {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        let clone = dir.join("clone");
        git(dir, &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        (origin, clone)
    }

    fn worktree(clone: &Path, branch: &str) -> PathBuf {
        create_worktree(clone, branch, "origin/main", &TokenUrl::new("acme/web", "ghs_dead"))
            .unwrap()
    }

    fn commit_file(worktree: &Path, name: &str, content: &str, message: &str) {
        fs::write(worktree.join(name), content).unwrap();
        git(worktree, &["add", "."]);
        git(worktree, &["commit", "--quiet", "-m", message]);
    }

    fn policy(prefixes: &[&str]) -> PrunePolicy {
        PrunePolicy {
            prefixes: prefixes.iter().map(|prefix| prefix.to_string()).collect(),
            ..PrunePolicy::default()
        }
    }

    fn branch_exists(clone: &Path, branch: &str) -> bool {
        run_git(
            Some(clone),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
            None,
            "rev-parse",
        )
        .is_ok()
    }

    #[test]
    fn squash_merged_branch_is_landed_and_pruned() {
        let dir = temp_dir("squash");
        let (origin, clone) = seed(&dir.0);
        // Two commits on the branch …
        let wt = worktree(&clone, "exp/EXP-1");
        commit_file(&wt, "a.txt", "one\n", "part 1");
        commit_file(&wt, "b.txt", "two\n", "part 2");
        // … land on origin/main as ONE squashed commit (the repo's real
        // merge style): same combined content, unrelated history.
        fs::write(origin.join("a.txt"), "one\n").unwrap();
        fs::write(origin.join("b.txt"), "two\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "EXP-1: squashed"]);
        git(&clone, &["fetch", "--quiet", "origin"]);

        // Ancestry can NOT see a squash merge — the probe must.
        let out = Command::new("git")
            .args(["merge-base", "--is-ancestor", "exp/EXP-1", "origin/main"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert!(!out.status.success(), "fixture must not be a plain merge");

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-1".to_string()]);
        assert_eq!(report.deleted_branches, vec!["exp/EXP-1".to_string()]);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/EXP-1"));
    }

    #[test]
    fn unlanded_branch_survives_and_reports_not_landed() {
        let dir = temp_dir("unlanded");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-2");
        commit_file(&wt, "wip.txt", "unmerged\n", "real work");

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.removed_worktrees.is_empty());
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-2".to_string(), SkipReason::NotLanded)]
        );
        assert_eq!(
            report.skipped[0].1.describe(report.default_branch.as_deref()),
            "not merged into main"
        );
        assert!(wt.exists());
        assert!(branch_exists(&clone, "exp/EXP-2"));
    }

    #[test]
    fn keep_protects_a_trivially_landed_worktree() {
        // A fresh session worktree is 0 commits ahead — trivially "landed".
        // The keep set (live sessions, open issues) must shield it.
        let dir = temp_dir("keep");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-3");

        let mut kept = policy(&["exp/"]);
        kept.keep.insert("exp/EXP-3".to_string());
        let report = prune_landed(&clone, &kept);
        assert!(report.is_empty(), "{report:?}");
        assert!(wt.exists());

        // Without the keep it is debris and goes.
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-3".to_string()]);
        assert!(!wt.exists());
    }

    #[test]
    fn untracked_only_is_removed_tracked_changes_survive() {
        let dir = temp_dir("dirty");
        let (_origin, clone) = seed(&dir.0);
        // Both landed (0 commits ahead); one holds untracked debris, one a
        // tracked modification.
        let debris = worktree(&clone, "exp/EXP-4");
        fs::write(debris.join("docker-compose.override.yaml"), "ports: []\n").unwrap();
        let real = worktree(&clone, "exp/EXP-5");
        fs::write(real.join("README.md"), "edited\n").unwrap();

        assert_eq!(worktree_dirty_state(&debris), DirtyState::UntrackedOnly);
        assert_eq!(worktree_dirty_state(&real), DirtyState::TrackedChanges);

        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-4".to_string()]);
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-5".to_string(), SkipReason::TrackedChanges)]
        );
        assert!(!debris.exists(), "untracked debris must not protect");
        assert!(real.exists(), "tracked changes must protect");

        // Staged counts as tracked too.
        git(&real, &["add", "."]);
        assert_eq!(worktree_dirty_state(&real), DirtyState::TrackedChanges);
    }

    #[test]
    fn merged_set_prunes_without_git_agreement() {
        // PR merged per the tracker, but the local origin ref is stale (the
        // squashed commit was never fetched): the merged set is authoritative.
        let dir = temp_dir("merged");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-6");
        commit_file(&wt, "landed-elsewhere.txt", "x\n", "work");

        let mut merged = policy(&["exp/"]);
        merged.merged.insert("exp/EXP-6".to_string());
        let report = prune_landed(&clone, &merged);
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-6".to_string()]);
        assert!(!wt.exists());
        assert!(!branch_exists(&clone, "exp/EXP-6"));
    }

    #[test]
    fn branch_only_sweep_deletes_landed_prefix_branches() {
        let dir = temp_dir("sweep");
        let (_origin, clone) = seed(&dir.0);
        // Landed (at origin/main) without worktrees: prefix + batch → go;
        // non-prefix → stays. Unlanded prefix branch → stays, silently.
        git(&clone, &["branch", "exp/OLD-1", "origin/main"]);
        git(&clone, &["branch", "exp/batch-1b81d4c7", "origin/main"]);
        git(&clone, &["branch", "feature/keep-me", "origin/main"]);
        let wt = worktree(&clone, "exp/OLD-2");
        commit_file(&wt, "wip.txt", "unmerged\n", "work");
        git(&clone, &["worktree", "remove", "--force", wt.to_str().unwrap()]);

        let mut sweeping = policy(&["exp/"]);
        sweeping.delete_stale_branches = true;
        let report = prune_landed(&clone, &sweeping);
        let mut deleted = report.deleted_branches.clone();
        deleted.sort();
        assert_eq!(deleted, vec!["exp/OLD-1".to_string(), "exp/batch-1b81d4c7".to_string()]);
        assert!(!branch_exists(&clone, "exp/OLD-1"));
        assert!(!branch_exists(&clone, "exp/batch-1b81d4c7"));
        assert!(branch_exists(&clone, "exp/OLD-2"), "unlanded branch survives");
        assert!(branch_exists(&clone, "feature/keep-me"), "non-prefix survives");
        assert!(branch_exists(&clone, "main"), "default branch survives");

        // Without the flag nothing branch-only happens.
        let report = prune_landed(&clone, &policy(&["exp/"]));
        assert!(report.deleted_branches.is_empty());
        assert!(branch_exists(&clone, "exp/OLD-2"));
    }

    #[test]
    fn effective_default_branch_resolution_chain() {
        let dir = temp_dir("default");
        let (_origin, clone) = seed(&dir.0);
        // Server value wins when its origin ref exists.
        assert_eq!(
            effective_default_branch(&clone, Some("main")),
            Some("main".to_string())
        );
        // A bogus/hostile server value falls back to origin/HEAD; a
        // flag-shaped one must never reach argv.
        assert_eq!(
            effective_default_branch(&clone, Some("nope")),
            Some("main".to_string())
        );
        assert_eq!(
            effective_default_branch(&clone, Some("--force")),
            Some("main".to_string())
        );
        assert_eq!(effective_default_branch(&clone, None), Some("main".to_string()));
        // origin/HEAD gone → the checked-out branch still verifies.
        git(&clone, &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"]);
        assert_eq!(effective_default_branch(&clone, None), Some("main".to_string()));
    }

    #[test]
    fn no_default_branch_reports_instead_of_guessing() {
        // A repo with NO origin remote at all: nothing verifies, nothing is
        // fabricated — prefix candidates report, merged still prunes.
        let dir = temp_dir("nodefault");
        let root = dir.0.join("repo");
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "-b", "trunk"]);
        fs::write(root.join("README.md"), "seed\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "seed"]);
        assert_eq!(effective_default_branch(&root, Some("trunk")), None);

        git(&root, &["branch", "exp/EXP-7"]);
        let path = worktrees_dir(&root).join(sanitize_branch_for_path("exp/EXP-7"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        git(&root, &["worktree", "add", path.to_str().unwrap(), "exp/EXP-7"]);

        let report = prune_landed(&root, &policy(&["exp/"]));
        assert_eq!(
            report.skipped,
            vec![("exp/EXP-7".to_string(), SkipReason::NoDefaultBranch)]
        );

        let mut merged = policy(&["exp/"]);
        merged.merged.insert("exp/EXP-7".to_string());
        let report = prune_landed(&root, &merged);
        assert_eq!(report.removed_worktrees, vec!["exp/EXP-7".to_string()]);
    }

    #[test]
    fn busy_path_parks_its_worktree() {
        // A running terminal tab cd'ed anywhere under a worktree keeps it,
        // whatever the branch's landed state says.
        let dir = temp_dir("busy");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-9");
        let mut parked = policy(&["exp/"]);
        // A real dir (tab cwds exist) so canonicalization works on macOS,
        // where git reports /private/tmp for a /tmp-composed worktree path.
        fs::create_dir_all(wt.join("some/subdir")).unwrap();
        parked.busy_paths.push(wt.join("some/subdir"));
        let report = prune_landed(&clone, &parked);
        assert!(report.is_empty(), "{report:?}");
        assert!(wt.exists());
    }

    #[test]
    fn clean_worktree_states() {
        let dir = temp_dir("state");
        let (_origin, clone) = seed(&dir.0);
        let wt = worktree(&clone, "exp/EXP-8");
        assert_eq!(worktree_dirty_state(&wt), DirtyState::Clean);
        // A missing dir fails `git status` → fail-safe TrackedChanges.
        assert_eq!(
            worktree_dirty_state(&dir.0.join("missing")),
            DirtyState::TrackedChanges
        );
    }
}
