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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForIssueInput<'a> {
    issue_id: &'a str,
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
