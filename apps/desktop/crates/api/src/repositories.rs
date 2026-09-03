//! Typed `repositories.*` tRPC helpers the Start-coding launcher consumes
//! (masterplan-v3 §7.1 steps 1–2). Shapes verified against
//! `apps/web/src/lib/trpc/repositories.ts`:
//!
//! - `repositories.forIssue({issueId})` — **query** — resolves issue →
//!   board → the primary repo link (else the sole link, else `null`).
//!   `null` means "no repository linked" and the launcher must not proceed
//!   (the disabled Start-coding button with the "Link a repository…"
//!   helper — never a crash, never a false block).
//! - `repositories.installationToken({repositoryId, boardId?})` —
//!   **mutation** — mints
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

/// `repositories.add` output: `{repository: <full row>}`. Only the fields the
/// desktop consumes are mirrored — the rest of the row is ignored.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AddedRepository {
    pub repository: AddedRepositoryRow,
}

/// The connected repo row `repositories.add` hands back.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AddedRepositoryRow {
    pub id: String,
    /// `owner/name`.
    pub full_name: String,
}

/// Output of `repositories.remove` — `{"ok": true}` on success.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct RemoveResult {
    pub ok: bool,
}

/// Output of `repositories.listBranches` — the repo's branch names, listed
/// live from GitHub for the default-branch picker (EXP-462).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct RepoBranches {
    pub branches: Vec<String>,
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
    team_id: &'a str,
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
    /// EXP-712: the board the launch is for. When set, the returned
    /// `defaultBranch` is THAT board's branch (board pin → team pin →
    /// GitHub); omitted for board-less runs (actions, chat, shells), which
    /// keep the repo-level resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    board_id: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddInput<'a> {
    team_id: &'a str,
    full_name: &'a str,
    /// Omitted when unknown — the server then asks GitHub for the
    /// authoritative default instead of blind-seeding `main`.
    #[serde(skip_serializing_if = "Option::is_none")]
    default_branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveInput<'a> {
    repository_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListBranchesInput<'a> {
    repository_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetDefaultBranchInput<'a> {
    repository_id: &'a str,
    /// `None` serializes as JSON `null` — the explicit "follow GitHub again"
    /// signal, deliberately NOT skipped like AddInput's optionals.
    branch: Option<&'a str>,
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
/// `board_id` (EXP-712) makes the returned `default_branch` the BOARD's
/// branch instead of the repo's — every worktree base and PR target of a
/// board-scoped launch resolves through it.
pub fn installation_token(
    trpc: &TrpcClient,
    repository_id: &str,
    board_id: Option<&str>,
) -> Result<InstallationToken, ApiError> {
    trpc.mutation(
        "repositories.installationToken",
        &InstallationTokenInput {
            repository_id,
            board_id,
        },
    )
}

/// `repositories.openPulls` — query. Member-gated, server-cached (~60s), so
/// callers refetch on view-open/team-switch and never poll.
pub fn open_pulls(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<Vec<OpenPullsRepo>, ApiError> {
    let out: OpenPullsOutput =
        trpc.query_with_input("repositories.openPulls", &OpenPullsInput { team_id })?;
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

/// `repositories.add` — mutation (owner-gated). Registers a repo reachable
/// through one of the caller's GitHub-App installations; the installation id
/// is resolved server-side, never supplied here. An already-registered repo
/// is un-archived and returned, so this is idempotent.
pub fn add(
    trpc: &TrpcClient,
    team_id: &str,
    full_name: &str,
    default_branch: Option<&str>,
    private: Option<bool>,
) -> Result<AddedRepository, ApiError> {
    trpc.mutation(
        "repositories.add",
        &AddInput {
            team_id,
            full_name,
            default_branch,
            private,
        },
    )
}

/// `repositories.remove` — mutation (owner-gated hard delete). A repo still
/// backing a board is refused with `CONFLICT` (HTTP 409) whose message names
/// the blocking boards — it is user-presentable and must be surfaced verbatim.
pub fn remove(trpc: &TrpcClient, repository_id: &str) -> Result<RemoveResult, ApiError> {
    trpc.mutation("repositories.remove", &RemoveInput { repository_id })
}

/// `repositories.listBranches` — query (member-gated, live from GitHub).
/// Feeds the settings pane's default-branch dropdown (EXP-462) and the board
/// form's branch dropdown (EXP-712).
pub fn list_branches(trpc: &TrpcClient, repository_id: &str) -> Result<RepoBranches, ApiError> {
    trpc.query_with_input(
        "repositories.listBranches",
        &ListBranchesInput { repository_id },
    )
}

/// `repositories.setDefaultBranch` — mutation (owner-gated). Pins the branch
/// the product treats as the repo's default; `None` clears the pin (follow
/// GitHub). The server validates the branch exists and normalizes picking
/// GitHub's own default back to `None` — a `BAD_REQUEST`/`BAD_GATEWAY`
/// message is user-presentable verbatim.
pub fn set_default_branch(
    trpc: &TrpcClient,
    repository_id: &str,
    branch: Option<&str>,
) -> Result<(), ApiError> {
    let _: serde_json::Value = trpc.mutation(
        "repositories.setDefaultBranch",
        &SetDefaultBranchInput {
            repository_id,
            branch,
        },
    )?;
    Ok(())
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
        let token = installation_token(&client(&base), "repo-1", None).unwrap();
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

    /// EXP-712: a BOARD launch names its board so the mint answers with the
    /// board's branch. A board-less run (action/chat/shell) must keep sending
    /// the bare input — the server would otherwise resolve someone else's pin.
    #[test]
    fn installation_token_carries_the_board_for_board_launches() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"token":"ghs_secret123","fullName":"acme/web","defaultBranch":"develop","expiresAt":"2026-07-03T12:55:00.000Z"}}}"#,
        );
        let token = installation_token(&client(&base), "repo-1", Some("board-1")).unwrap();
        assert_eq!(token.default_branch, "develop");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(r#"{"repositoryId":"repo-1","boardId":"board-1"}"#),
            "{request}"
        );
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
        // Query → GET with percent-encoded raw-JSON input ({"teamId":…}).
        assert!(request.starts_with("GET /api/trpc/repositories.openPulls?input=%7B%22teamId%22%3A%2211111111-2222-3333-4444-555555555555%22%7D HTTP/1.1"));
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
    fn add_posts_camel_case_input_and_decodes_the_row() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repository":{"id":"repo-1","teamId":"team-1","fullName":"acme/web","defaultBranch":"main","private":true}}}}"#,
        );
        let out = add(&client(&base), "team-1", "acme/web", Some("trunk"), Some(true)).unwrap();
        assert_eq!(
            out,
            AddedRepository {
                repository: AddedRepositoryRow {
                    id: "repo-1".to_string(),
                    full_name: "acme/web".to_string(),
                },
            }
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/repositories.add HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"teamId":"team-1","fullName":"acme/web","defaultBranch":"trunk","private":true}"#
        ));
    }

    #[test]
    fn add_omits_the_optional_fields_when_unknown() {
        // An absent defaultBranch is what makes the server resolve the live
        // one from GitHub — sending null/"" instead would seed a wrong row.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"repository":{"id":"repo-1","fullName":"acme/web"}}}}"#,
        );
        add(&client(&base), "team-1", "acme/web", None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"team-1","fullName":"acme/web"}"#));
    }

    #[test]
    fn remove_posts_camel_case_input_and_decodes_result() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        let out = remove(&client(&base), "repo-1").unwrap();
        assert!(out.ok);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/repositories.remove HTTP/1.1"));
        assert!(request.ends_with(r#"{"repositoryId":"repo-1"}"#));
    }

    #[test]
    fn list_branches_gets_with_encoded_input_and_decodes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"branches":["master","develop"]}}}"#,
        );
        let out = list_branches(&client(&base), "repo-1").unwrap();
        assert_eq!(
            out,
            RepoBranches {
                branches: vec!["master".to_string(), "develop".to_string()],
            }
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with(
            "GET /api/trpc/repositories.listBranches?input=%7B%22repositoryId%22%3A%22repo-1%22%7D HTTP/1.1"
        ));
    }

    #[test]
    fn set_default_branch_posts_the_pin() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"repository":{"id":"repo-1"}}}}"#);
        set_default_branch(&client(&base), "repo-1", Some("develop")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/repositories.setDefaultBranch HTTP/1.1"));
        assert!(request.ends_with(r#"{"repositoryId":"repo-1","branch":"develop"}"#));
    }

    #[test]
    fn set_default_branch_none_sends_an_explicit_null() {
        // `null` is the "follow GitHub again" signal — omitting the field
        // would be a schema error, not a clear.
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"repository":{"id":"repo-1"}}}}"#);
        set_default_branch(&client(&base), "repo-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"repositoryId":"repo-1","branch":null}"#));
    }

    #[test]
    fn remove_conflict_message_passes_through_verbatim() {
        // `repoInUseMessage` — the FK-restrict refusal names the blocking
        // boards, so the pane renders it as-is instead of a generic failure.
        let (base, _captured) = one_shot_server(
            409,
            r#"{"error":{"message":"Can't remove this repository. It backs 2 boards. Retarget or delete those boards first (a board in the trash may still use it).","code":-32009,"data":{"code":"CONFLICT","httpStatus":409}}}"#,
        );
        match remove(&client(&base), "repo-1") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 409);
                assert_eq!(
                    message,
                    "Can't remove this repository. It backs 2 boards. Retarget or delete those \
                     boards first (a board in the trash may still use it)."
                );
            }
            other => panic!("expected 409 Http error, got {other:?}"),
        }
    }

    #[test]
    fn app_missing_surfaces_as_412_with_message() {
        // PRECONDITION_FAILED → the launcher's GithubAppMissing mapping.
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in team settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        match installation_token(&client(&base), "repo-1", None) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("GitHub App is not installed"));
            }
            other => panic!("expected 412 Http error, got {other:?}"),
        }
    }
}
