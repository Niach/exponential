//! git operations — a port of `repo-manager.ts` + `worktree.ts`, shelling out to
//! the system `git` (the companion used simple-git / child_process). Clone/fetch
//! with a token-embedded HTTPS remote, per-issue worktrees on `agent/…` branches,
//! and a prefix-guarded cleanup.

use std::process::Command;

fn run_git(args: &[&str], cwd: Option<&str>) -> Result<String, String> {
    let mut c = Command::new("git");
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().map_err(|e| format!("git spawn failed: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Validate + split `owner/repo`.
pub fn parse_owner_repo(s: &str) -> Result<(String, String), String> {
    let ok = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let mut it = s.splitn(2, '/');
    match (it.next(), it.next()) {
        (Some(o), Some(r)) if ok(o) && ok(r) && !r.contains('/') => Ok((o.to_string(), r.to_string())),
        _ => Err(format!("invalid GitHub repo \"{s}\". Expected \"owner/name\".")),
    }
}

/// `x-access-token` is GitHub's recommended username for token HTTPS access.
fn authed_remote_url(owner: &str, repo: &str, token: &str) -> String {
    format!("https://x-access-token:{token}@github.com/{owner}/{repo}.git")
}

/// Branch-name slug: lowercase, non-alnum runs → '-', trimmed, ≤40 chars.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').chars().take(40).collect()
}

pub struct RepoHandle {
    pub repo_path: String,
    pub owner: String,
    pub repo: String,
    pub default_branch: String,
}

/// Ensure a local clone exists at `{repos_root}/{owner}/{repo}` with the remote
/// pointed at the token URL and the default branch fetched.
pub fn ensure_repo(repos_root: &str, owner_repo: &str, default_branch: &str, token: &str) -> Result<RepoHandle, String> {
    let (owner, repo) = parse_owner_repo(owner_repo)?;
    let repo_path = format!("{repos_root}/{owner}/{repo}");
    let remote = authed_remote_url(&owner, &repo, token);
    std::fs::create_dir_all(format!("{repos_root}/{owner}")).map_err(|e| format!("mkdir: {e}"))?;

    if std::path::Path::new(&format!("{repo_path}/.git")).exists() {
        // Refresh the (rotating) token in the remote + fetch.
        let _ = run_git(&["remote", "set-url", "origin", &remote], Some(&repo_path));
        run_git(&["fetch", "origin", default_branch], Some(&repo_path))?;
    } else {
        run_git(&["clone", &remote, &repo_path], None)?;
    }
    Ok(RepoHandle { repo_path, owner, repo, default_branch: default_branch.to_string() })
}

/// Push a branch using a freshly-rewritten token remote.
pub fn push_branch(repo_path: &str, owner: &str, repo: &str, branch: &str, token: &str) -> Result<(), String> {
    let _ = run_git(&["remote", "set-url", "origin", &authed_remote_url(owner, repo, token)], Some(repo_path));
    run_git(&["push", "-u", "origin", branch], Some(repo_path)).map(|_| ())
}

pub struct WorktreeClaim {
    pub worktree_path: String,
    pub branch: String,
    pub repo_path: String,
    pub default_branch: String,
}

/// Create (or reset) a worktree at `{worktrees_root}/{identifier}` on branch
/// `{prefix}/{identifier-lower}-{slug}`, based on `origin/{default_branch}`.
pub fn worktree_claim(
    worktrees_root: &str,
    branch_prefix: &str,
    repo_path: &str,
    default_branch: &str,
    identifier: &str,
    slug: &str,
) -> Result<WorktreeClaim, String> {
    std::fs::create_dir_all(worktrees_root).map_err(|e| format!("mkdir worktrees: {e}"))?;
    let worktree_path = format!("{worktrees_root}/{identifier}");
    let branch = format!("{branch_prefix}/{}-{}", identifier.to_lowercase(), slugify(slug));

    if std::path::Path::new(&worktree_path).exists() {
        // A dirty tree means uncommitted work (possibly the user's manual edits
        // in an interactive session). Never destroy it — rename it aside so the
        // claim still proceeds with a clean tree.
        let dirty = run_git(&["status", "--porcelain"], Some(&worktree_path))
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if dirty {
            let epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let stale = format!("{worktree_path}-stale-{epoch}");
            let _ = std::fs::rename(&worktree_path, &stale);
            let _ = run_git(&["worktree", "prune"], Some(repo_path));
        } else {
            let _ = run_git(&["worktree", "remove", "--force", &worktree_path], Some(repo_path));
            let _ = std::fs::remove_dir_all(&worktree_path);
        }
    }
    run_git(&["fetch", "origin", default_branch], Some(repo_path))?;
    // -B (force-create/reset) so a lingering branch from a prior run is fine.
    run_git(
        &["worktree", "add", "-B", &branch, &worktree_path, &format!("origin/{default_branch}")],
        Some(repo_path),
    )?;
    Ok(WorktreeClaim { worktree_path, branch, repo_path: repo_path.to_string(), default_branch: default_branch.to_string() })
}

/// Reuse an EXISTING worktree WITHOUT the `-B` force-reset, so an interactive
/// approve-and-continue resumes against the plan session's working tree intact.
/// Falls back to a fresh `worktree_claim` if the worktree is gone.
pub fn worktree_reuse(
    worktrees_root: &str,
    branch_prefix: &str,
    repo_path: &str,
    default_branch: &str,
    identifier: &str,
    slug: &str,
) -> Result<WorktreeClaim, String> {
    let worktree_path = format!("{worktrees_root}/{identifier}");
    if std::path::Path::new(&worktree_path).exists() {
        let branch = format!("{branch_prefix}/{}-{}", identifier.to_lowercase(), slugify(slug));
        return Ok(WorktreeClaim {
            worktree_path,
            branch,
            repo_path: repo_path.to_string(),
            default_branch: default_branch.to_string(),
        });
    }
    worktree_claim(worktrees_root, branch_prefix, repo_path, default_branch, identifier, slug)
}

/// Whether the worktree's branch has any commits beyond `origin/{default}` —
/// i.e. an agent code session actually produced something to push.
pub fn branch_has_commits(worktree_path: &str, default_branch: &str) -> bool {
    run_git(
        &["rev-list", "--count", &format!("origin/{default_branch}..HEAD")],
        Some(worktree_path),
    )
    .ok()
    .and_then(|s| s.trim().parse::<u64>().ok())
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Remove a worktree + its branch — only when the branch carries the agent
/// prefix (belt-and-suspenders). Best-effort.
pub fn worktree_cleanup(branch_prefix: &str, claim: &WorktreeClaim) {
    if !claim.branch.starts_with(&format!("{branch_prefix}/")) {
        return;
    }
    let _ = run_git(&["worktree", "remove", "--force", &claim.worktree_path], Some(&claim.repo_path));
    let _ = std::fs::remove_dir_all(&claim.worktree_path);
    let _ = run_git(&["branch", "-D", &claim.branch], Some(&claim.repo_path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_owner_repo_ok_and_bad() {
        assert_eq!(parse_owner_repo("Niach/exponential").unwrap(), ("Niach".into(), "exponential".into()));
        assert_eq!(parse_owner_repo("o.r-g_1/repo.name-2").unwrap().1, "repo.name-2");
        assert!(parse_owner_repo("noslash").is_err());
        assert!(parse_owner_repo("a/b/c").is_err());
        assert!(parse_owner_repo("/x").is_err());
        assert!(parse_owner_repo("a/").is_err());
        assert!(parse_owner_repo("bad space/repo").is_err());
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify("Fix the Login Bug!"), "fix-the-login-bug");
        assert_eq!(slugify("  --Hello-- "), "hello");
        assert_eq!(slugify(&"a".repeat(60)).len(), 40);
        assert_eq!(slugify("Caf\u{e9} \u{2014} test"), "caf-test");
    }

    #[test]
    fn authed_url_uses_x_access_token() {
        assert_eq!(authed_remote_url("o", "r", "T"), "https://x-access-token:T@github.com/o/r.git");
    }
}
