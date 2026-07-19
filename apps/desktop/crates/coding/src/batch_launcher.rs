//! BATCH-run launch types — "start coding on several issues at once": ONE
//! Claude session per (issue set, repo), prepared by the unified
//! [`crate::launcher::prepare`] (`PrepareRequest::Batch`) and spawned through
//! the same `spawn_prepared_with`. Deliberately looser than the old release
//! orchestrator: no per-issue subagent definitions, no per-issue worktrees,
//! no per-issue PRs — Claude owns the full set on ONE branch and opens ONE
//! combined PR (`exponential_pr_open` with `issueIds`).
//!
//! What lives here: the dialog-facing request types and the batch-branch
//! naming — `exp/batch-<id8>` with a LOWERCASE hex id, so the webhook's
//! issue-identifier parse can never mis-link a batch branch (locked by the
//! test below against the ported server regex).

use crate::argv::LaunchOptions;
use crate::launcher::LaunchOrigin;
use domain::IssueStatus;

/// The repo this session works (the dialog enforces ONE repo per run).
#[derive(Clone, Debug)]
pub struct RepoGroup {
    pub repository_id: String,
    /// `owner/name`.
    pub full_name: String,
    pub default_branch: String,
}

/// One selected issue, snapshotted by the dialog from the sync store.
#[derive(Clone, Debug)]
pub struct BatchIssueSpec {
    /// UUID — rides into the prompt so Claude can pass `issueIds` to
    /// `exponential_pr_open`.
    pub issue_id: String,
    /// `EXP-42`.
    pub issue_identifier: String,
    pub title: String,
    pub description: Option<String>,
    /// Status snapshot at dialog time — the launcher flips backlog/todo
    /// issues to `in_progress` at launch (EXP-194).
    pub status: IssueStatus,
}

/// The dialog's launch input for ONE batch run.
#[derive(Clone, Debug)]
pub struct BatchLaunchRequest {
    /// Client-generated [`new_batch_id`] — names the branch and keys the
    /// local session registry.
    pub batch_id: String,
    /// The team the issues live in — `codingSessions.start`'s batch
    /// subject (`{ teamId }`).
    pub team_id: String,
    pub repo: RepoGroup,
    /// Only issues resolving to `repo`, already filtered to launchable ones
    /// (2+ — a single selection takes the plain issue path).
    pub issues: Vec<BatchIssueSpec>,
    pub device_label: String,
    pub origin: LaunchOrigin,
    pub options: LaunchOptions,
}

/// A fresh batch id: the first 8 hex chars of a v4 UUID — `[0-9a-f]{8}` by
/// construction, so [`batch_branch_name`] stays lowercase/webhook-safe.
pub fn new_batch_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// `exp/batch-<id8>` — the batch run's working branch. Deliberately NOT the
/// user's branch prefix: the lowercase `batch-` marker + hex id are the
/// webhook safety guarantee (the server's issue parse requires an UPPERCASE
/// `[A-Z0-9]+-\d+` tail), independent of prefix configuration.
pub fn batch_branch_name(batch_id: &str) -> String {
    format!("exp/batch-{batch_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's `parseIssueIdentifierFromBranch` regex, ported: the last
    /// `/`-segment must be ENTIRELY `[A-Z0-9]+-\d+` to link an issue (see
    /// apps/web/src/lib/integrations/pr-sync.ts — its test suite locks the
    /// same batch-branch cases). The batch branch must NEVER satisfy it.
    fn parses_as_issue_branch(branch: &str) -> bool {
        let tail = branch.rsplit('/').next().unwrap_or(branch);
        let Some(dash) = tail.rfind('-') else { return false };
        let (head, digits) = (&tail[..dash], &tail[dash + 1..]);
        !head.is_empty()
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && head
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }

    #[test]
    fn batch_branches_can_never_match_the_issue_webhook_parse() {
        // The critical case is the ALL-DIGITS id (`batch-12345678` has a
        // digits-only tail after the dash) — only the lowercase `batch` head
        // keeps the parse from matching, which is exactly the contract.
        for id in ["a1b2c3d4", "12345678", "deadbeef", "00000000", "abcdef01"] {
            let branch = batch_branch_name(id);
            assert!(
                branch.starts_with("exp/batch-"),
                "branch {branch:?} lost its batch- marker"
            );
            let tail = branch.rsplit('/').next().unwrap();
            assert!(
                tail.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "branch tail must be lowercase-safe: {branch:?}"
            );
            assert!(
                !parses_as_issue_branch(&branch),
                "batch branch {branch:?} would link as an issue!"
            );
        }
        // Sanity: the checker itself recognizes real issue branches.
        assert!(parses_as_issue_branch("exp/EXP-42"));
        assert!(!parses_as_issue_branch("exp/batch-12345678"));
    }

    #[test]
    fn new_batch_id_is_8_lowercase_hex_chars() {
        for _ in 0..32 {
            let id = new_batch_id();
            assert_eq!(id.len(), 8, "id: {id:?}");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "id must be lowercase hex: {id:?}"
            );
        }
        // Two draws are (overwhelmingly) distinct — the uniqueness the branch
        // name rides on.
        assert_ne!(new_batch_id(), new_batch_id());
    }

    #[test]
    fn batch_branch_name_is_exact() {
        assert_eq!(batch_branch_name("a1b2c3d4"), "exp/batch-a1b2c3d4");
    }
}
