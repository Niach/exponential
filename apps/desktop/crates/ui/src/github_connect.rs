//! Shared GitHub-App connect state + repo fetches (masterplan §7 IDE track).
//!
//! Both the workspace-settings Repositories pane and the create-project dialog
//! read the **live server truth** for GitHub connect — never a local guess:
//!
//! - [`fetch_github_status`] → `integrations.github.status`
//!   (`{configured, installed, installUrl, connectUrl, accounts[]}`) drives the
//!   install banner ("Installed as @acme" vs. the install nudge).
//! - [`fetch_github_repos`] → `integrations.github.repos`
//!   (`{configured, installed, installUrl, connectUrl, repos[], hasMore}`)
//!   lists the repos the signed-in user can connect inline, mirroring the web
//!   `GithubRepoPicker` (`components/github-repo-picker.tsx`).
//!
//! Both take the current `workspaceId`: GitHub-App installs are claimed PER
//! workspace, so the status/repo lists are scoped to the workspace's claimed
//! installations. `connectUrl` is a single-consent OAuth authorize URL that
//! claims the account for the workspace; `installUrl` is the broader App
//! install page (also grants more repos). The **connect** hand-off prefers
//! `connect_url` and falls back to `install_url`.
//!
//! The connect/install itself is still a browser hand-off: the pane/dialog open
//! the URL in the system browser (`settings::open_url`), then re-fetch on an
//! explicit Refresh — gpui has no reliable cross-app focus signal to hang an
//! auto-refresh on.

use serde::{Deserialize, Serialize};

/// `integrations.github.status` (per-workspace App install state). Unknown
/// fields (e.g. the server's `installations[]`) are ignored — this mirror
/// carries only what the pane renders.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubStatus {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub installed: bool,
    /// The App install page (grants more repos / manage).
    #[serde(default)]
    pub install_url: Option<String>,
    /// Single-consent OAuth authorize URL that claims the account for the
    /// workspace — the preferred **connect** target (`connect_url.or(install_url)`).
    #[serde(default)]
    pub connect_url: Option<String>,
    #[serde(default)]
    pub accounts: Vec<String>,
}

/// One repo the signed-in user can connect inline — a row of
/// `integrations.github.repos`'s `repos[]` (server `InstallationRepo`). The
/// fields feed `projects.create`'s inline-repo union arm.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubRepo {
    pub full_name: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub installation_id: i64,
}

/// `integrations.github.repos` result (mirrors the web `ReposResult`).
/// Unknown fields (e.g. the server's `installations[]`) are ignored.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubReposResult {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub installed: bool,
    /// The App install page (grants more repos / manage).
    #[serde(default)]
    pub install_url: Option<String>,
    /// Single-consent OAuth authorize URL that claims the account for the
    /// workspace — the preferred **connect** target (`connect_url.or(install_url)`).
    #[serde(default)]
    pub connect_url: Option<String>,
    #[serde(default)]
    pub repos: Vec<GithubRepo>,
    #[serde(default)]
    pub has_more: bool,
}

/// `integrations.github.status` — per-workspace install state (best-effort
/// banner). `workspace_id` scopes the status to that workspace's claimed
/// installations.
pub(crate) fn fetch_github_status(
    trpc: &api::TrpcClient,
    workspace_id: &str,
) -> Result<GithubStatus, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    trpc.query_with_input("integrations.github.status", &Input { workspace_id })
}

/// `integrations.github.repos` — the installable-repo list for the workspace.
/// `refresh` bypasses the server's per-workspace cache so a just-completed
/// install reflects at once.
pub(crate) fn fetch_github_repos(
    trpc: &api::TrpcClient,
    workspace_id: &str,
    refresh: bool,
) -> Result<GithubReposResult, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh: Option<bool>,
    }
    trpc.query_with_input(
        "integrations.github.repos",
        &Input {
            workspace_id,
            refresh: refresh.then_some(true),
        },
    )
}
