//! EXP-982 — the integration branch, the one piece of git IO the workflow
//! engine owns. BOTH hosts (the desktop GUI's `ui::workflow_host` and the
//! CLI daemon's worker) call these; they are the host half of
//! [`super::Decision::EnsureIntegrationBranch`] and
//! [`super::Decision::DeleteIntegrationBranch`].
//!
//! It happens in the engine's OWN clone of the repository — the launcher's
//! trunk clone, never a session's worktree — and touches no working tree at
//! all: the branch is pushed straight off `origin/<default>`. The JIT
//! installation token rides the usual cache (the mint's `default_branch` IS
//! the resolution board default → repo override → GitHub), and every error
//! comes back already token-scrubbed.

use std::path::Path;

use api::TrpcClient;

use crate::git_worktree::{
    clone_path, ensure_clone, fetch_base, run_git, validate_branch_arg, TokenUrl,
};

/// The clone + ambient auth the two operations below share.
fn engine_clone(
    trpc: &TrpcClient,
    repos_root: &Path,
    repository_id: &str,
    board_id: Option<&str>,
    branch: &str,
) -> Result<(std::path::PathBuf, TokenUrl, String), String> {
    // A branch name off the wire reaches git argv here: the same gate every
    // other branch argument takes (`-…` is an option, not a ref).
    validate_branch_arg(branch, "integration branch").map_err(|err| err.to_string())?;
    let minted = crate::token_cache::token_cache()
        .get_or_mint_with_margin(
            trpc,
            repository_id,
            board_id,
            crate::token_refresh::REFRESH_LEAD,
        )
        .map_err(|err| err.to_string())?;
    let url = minted.url.clone();
    let clone = ensure_clone(repos_root, url.full_name(), &url).map_err(|err| err.to_string())?;
    crate::git_credentials::ensure(&clone, &url, minted.expires_at.as_deref())
        .map_err(|err| err.to_string())?;
    Ok((clone, url, minted.default_branch))
}

/// Whether `origin/<branch>` resolves in this clone right now. A fetch that
/// fails (the branch does not exist yet) is the ANSWER, not an error.
fn origin_has(clone: &Path, branch: &str, url: &TokenUrl) -> bool {
    if fetch_base(clone, branch, url).is_err() {
        return false;
    }
    run_git(
        Some(clone),
        &["rev-parse", "--verify", &format!("refs/remotes/origin/{branch}")],
        Some(url),
        &format!("git rev-parse origin/{branch}"),
    )
    .is_ok()
}

/// Create `<branch>` from the repository's default branch and push it, or do
/// nothing when it is already up. Idempotent: the engine calls this every
/// pass until the branch exists.
pub fn ensure_integration_branch(
    trpc: &TrpcClient,
    repos_root: &Path,
    repository_id: &str,
    board_id: Option<&str>,
    branch: &str,
) -> Result<(), String> {
    let (clone, url, default_branch) =
        engine_clone(trpc, repos_root, repository_id, board_id, branch)?;
    if origin_has(&clone, branch, &url) {
        return Ok(());
    }
    // The base must be local before it can be pushed under a new name; no
    // checkout, no worktree — the ref travels on its own.
    fetch_base(&clone, &default_branch, &url).map_err(|err| err.to_string())?;
    run_git(
        Some(&clone),
        &[
            "push",
            "origin",
            &format!("refs/remotes/origin/{default_branch}:refs/heads/{branch}"),
        ],
        Some(&url),
        &format!("git push origin {branch}"),
    )
    .map_err(|err| err.to_string())?;
    log::info!("[workflows] pushed integration branch {branch} from {default_branch}");
    Ok(())
}

/// Drop `<branch>` on origin — a cancelled workflow's last act. A branch
/// that is already gone is a success.
pub fn delete_integration_branch(
    trpc: &TrpcClient,
    repos_root: &Path,
    repository_id: &str,
    board_id: Option<&str>,
    branch: &str,
) -> Result<(), String> {
    let (clone, url, _) = engine_clone(trpc, repos_root, repository_id, board_id, branch)?;
    match run_git(
        Some(&clone),
        &["push", "origin", "--delete", branch],
        Some(&url),
        &format!("git push origin --delete {branch}"),
    ) {
        Ok(_) => Ok(()),
        // "remote ref does not exist" — someone (or a previous pass) got
        // there first, which is exactly the state we wanted.
        Err(err) if err.to_string().contains("remote ref does not exist") => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

/// The engine's clone path for a repository — the launcher's trunk clone,
/// which is what makes this a reuse rather than a second checkout.
pub fn engine_clone_path(repos_root: &Path, full_name: &str) -> std::path::PathBuf {
    clone_path(repos_root, full_name)
}
