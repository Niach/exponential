//! Typed `repositories.*` tRPC helpers the Start-coding launcher consumes
//! (masterplan-v3 §7.1 steps 1–2). Shapes verified against
//! `apps/web/src/lib/trpc/repositories.ts`:
//!
//! - `repositories.forIssue({issueId})` — **query** — resolves issue →
//!   project → the primary repo link (else the sole link, else `null`).
//!   `null` means "no repository linked" and the launcher must not proceed
//!   (the disabled Start-coding button with the "Link a repository…"
//!   helper — never a crash, never a false block).
//! - `repositories.installationToken({repositoryId})` — **mutation** — mints
//!   the session-gated JIT GitHub-App installation token (~55 min TTL,
//!   `INSTALLATION_TOKEN_TTL_MS` server-side). **NEVER persisted, never
//!   logged**; the raw value only ever flows into the transient token-embedded
//!   git remote URL (§7.1 step 3). [`InstallationToken`]'s `Debug` redacts it.
//!
//! Both procs throw `PRECONDITION_FAILED` (HTTP 412) when the Exponential
//! GitHub App is not installed on the repo — the launcher maps that to its
//! `GithubAppMissing` disabled state (the App-install flow is web-only, §7.9).

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `repositories.forIssue` output (non-null case): the clone target.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IssueRepository {
    pub repository_id: String,
    /// `owner/name`.
    pub full_name: String,
    pub default_branch: String,
}

/// `repositories.installationToken` output. `token` is a live GitHub-App
/// installation token — handle it like a password: it goes into the git
/// remote URL and NOWHERE else (no logs, no files, no error strings).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationToken {
    pub token: String,
    /// `owner/name`.
    pub full_name: String,
    pub default_branch: String,
    /// ISO timestamp (~55 min out) — the desktop re-mints per launch and
    /// re-sets the remote every time (§7.1 step 3), so this is advisory.
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// §7.1 step 2 redaction rule: the token must never reach logs — including
/// via a stray `{:?}`. No `Display` impl exists at all.
impl fmt::Debug for InstallationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallationToken")
            .field("token", &"***")
            .field("full_name", &self.full_name)
            .field("default_branch", &self.default_branch)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// One repo's open pull requests from `repositories.openPulls`. Every pull is
/// guaranteed issue-UNLINKED — the server excludes PRs a synced issue row
/// already carries, so the Reviews queue renders these below the issue rows
/// without dedup work.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenPullsRepo {
    pub repository_id: String,
    /// `owner/name`.
    pub full_name: String,
    pub pulls: Vec<OpenPull>,
}

/// One open pull request as GitHub lists it (no issues row backs these —
/// release PRs, manual branches, external contributors).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenPull {
    pub number: u64,
    pub url: String,
    pub title: String,
    /// Head branch name.
    pub branch: String,
    pub base_branch: String,
    pub draft: bool,
    #[serde(default)]
    pub author_login: Option<String>,
    #[serde(default)]
    pub author_avatar_url: Option<String>,
    /// ISO timestamp.
    pub created_at: String,
}

/// Output of `repositories.mergePull` — `{"merged": true}` on success.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct MergePullResult {
    pub merged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForIssueInput<'a> {
    issue_id: &'a str,
}

#[derive(Deserialize)]
struct OpenPullsOutput {
    repos: Vec<OpenPullsRepo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenPullsInput<'a> {
    workspace_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergePullInput<'a> {
    repository_id: &'a str,
    pr_number: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallationTokenInput<'a> {
    repository_id: &'a str,
}

/// `repositories.forIssue` — query. `Ok(None)` = no repo linked (the
/// disabled-button state, not an error).
pub fn for_issue(
    trpc: &TrpcClient,
    issue_id: &str,
) -> Result<Option<IssueRepository>, ApiError> {
    trpc.query_with_input("repositories.forIssue", &ForIssueInput { issue_id })
}

/// `repositories.installationToken` — mutation (JIT, session-gated).
pub fn installation_token(
    trpc: &TrpcClient,
    repository_id: &str,
) -> Result<InstallationToken, ApiError> {
    trpc.mutation(
        "repositories.installationToken",
        &InstallationTokenInput { repository_id },
    )
}

/// `repositories.openPulls` — query. Member-gated, server-cached (~60s), so
/// callers refetch on view-open/workspace-switch and never poll.
pub fn open_pulls(
    trpc: &TrpcClient,
    workspace_id: &str,
) -> Result<Vec<OpenPullsRepo>, ApiError> {
    let out: OpenPullsOutput =
        trpc.query_with_input("repositories.openPulls", &OpenPullsInput { workspace_id })?;
    Ok(out.repos)
}

/// `repositories.mergePull` — mutation (GitHub-App squash merge of an
/// issue-unlinked PR; the issue-linked path is `issues.mergePr`). There is no
/// Electric echo — the caller drops the row from its local state on success.
pub fn merge_pull(
    trpc: &TrpcClient,
    repository_id: &str,
    pr_number: u64,
) -> Result<MergePullResult, ApiError> {
    trpc.mutation(
        "repositories.mergePull",
        &MergePullInput {
            repository_id,
            pr_number,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn for_issue_decodes_repo() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repositoryId":"repo-1","fullName":"acme/web","defaultBranch":"main"}}}"#,
        );
        let repo = for_issue(&client(&base), "11111111-2222-3333-4444-555555555555")
            .unwrap()
            .unwrap();
        assert_eq!(
            repo,
            IssueRepository {
                repository_id: "repo-1".to_string(),
                full_name: "acme/web".to_string(),
                default_branch: "main".to_string(),
            }
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // Query → GET with percent-encoded raw-JSON input ({"issueId":…}).
        assert!(request.starts_with("GET /api/trpc/repositories.forIssue?input=%7B%22issueId%22%3A%2211111111-2222-3333-4444-555555555555%22%7D HTTP/1.1"));
    }

    #[test]
    fn for_issue_null_is_no_repo_linked() {
        // The no-repo-linked gate: null ⇒ Ok(None), NOT an error.
        let (base, _captured) = one_shot_server(200, r#"{"result":{"data":null}}"#);
        let repo = for_issue(&client(&base), "issue-1").unwrap();
        assert_eq!(repo, None);
    }

    #[test]
    fn installation_token_decodes_and_posts() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"main","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#,
        );
        let token = installation_token(&client(&base), "repo-1").unwrap();
        assert_eq!(token.token, "ghs_secret123");
        assert_eq!(token.full_name, "acme/web");
        assert_eq!(token.default_branch, "main");
        assert_eq!(
            token.expires_at.as_deref(),
            Some("2026-07-03T12:55:00.000Z")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/repositories.installationToken HTTP/1.1"));
        assert!(request.ends_with(r#"{"repositoryId":"repo-1"}"#));
    }

    #[test]
    fn installation_token_debug_redacts_the_secret() {
        let token = InstallationToken {
            token: "ghs_secret123".to_string(),
            full_name: "acme/web".to_string(),
            default_branch: "main".to_string(),
            expires_at: None,
        };
        let debug = format!("{token:?}");
        assert!(!debug.contains("ghs_secret123"), "token leaked: {debug}");
        assert!(debug.contains("***"));
        assert!(debug.contains("acme/web"));
    }

    #[test]
    fn open_pulls_decodes_repo_groups() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repos":[{"repositoryId":"repo-1","fullName":"acme/web","pulls":[{"number":42,"url":"https://github.com/acme/web/pull/42","title":"Fix login","branch":"fix/login","baseBranch":"main","draft":true,"authorLogin":"octocat","authorAvatarUrl":null,"createdAt":"2026-07-10T08:00:00Z"}]},{"repositoryId":"repo-2","fullName":"acme/api","pulls":[]}]}}}"#,
        );
        let repos = open_pulls(&client(&base), "11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(
            repos[0],
            OpenPullsRepo {
                repository_id: "repo-1".to_string(),
                full_name: "acme/web".to_string(),
                pulls: vec![OpenPull {
                    number: 42,
                    url: "https://github.com/acme/web/pull/42".to_string(),
                    title: "Fix login".to_string(),
                    branch: "fix/login".to_string(),
                    base_branch: "main".to_string(),
                    draft: true,
                    author_login: Some("octocat".to_string()),
                    author_avatar_url: None,
                    created_at: "2026-07-10T08:00:00Z".to_string(),
                }],
            }
        );
        assert!(repos[1].pulls.is_empty());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // Query → GET with percent-encoded raw-JSON input ({"workspaceId":…}).
        assert!(request.starts_with("GET /api/trpc/repositories.openPulls?input=%7B%22workspaceId%22%3A%2211111111-2222-3333-4444-555555555555%22%7D HTTP/1.1"));
    }

    #[test]
    fn merge_pull_posts_camel_case_input_and_decodes_result() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"merged":true}}}"#);
        let out = merge_pull(&client(&base), "repo-1", 42).unwrap();
        assert!(out.merged);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/repositories.mergePull HTTP/1.1"));
        assert!(request.ends_with(r#"{"repositoryId":"repo-1","prNumber":42}"#));
    }

    #[test]
    fn merge_pull_surfaces_the_server_message() {
        // e.g. a 405 "not mergeable" mapped to PRECONDITION_FAILED server-side.
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"Pull request is not mergeable","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        match merge_pull(&client(&base), "repo-1", 42) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("not mergeable"));
            }
            other => panic!("expected 412 Http error, got {other:?}"),
        }
    }

    #[test]
    fn app_missing_surfaces_as_412_with_message() {
        // PRECONDITION_FAILED → the launcher's GithubAppMissing mapping.
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in workspace settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        match installation_token(&client(&base), "repo-1") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("GitHub App is not installed"));
            }
            other => panic!("expected 412 Http error, got {other:?}"),
        }
    }
}
