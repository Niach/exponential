//! Shared GitHub-App connect state + repo fetches (masterplan §7 IDE track).
//!
//! Both the workspace-settings Repositories pane and the create-project dialog
//! read the **live server truth** for GitHub connect — never a local guess:
//!
//! - [`fetch_github_status`] → `integrations.github.status`
//!   (`{configured, installed, installUrl, accounts[]}`) drives the install
//!   banner ("Installed as @acme" vs. the install nudge).
//! - [`fetch_github_repos`] → `integrations.github.repos`
//!   (`{configured, installed, installUrl, repos[], hasMore}`) lists the repos
//!   the signed-in user can connect inline, mirroring the web
//!   `GithubRepoPicker` (`components/github-repo-picker.tsx`).
//!
//! The App **install** itself is still a browser hand-off: the pane/dialog open
//! the install URL in the system browser (`settings::open_url`), then re-fetch
//! on an explicit Refresh — gpui has no reliable cross-app focus signal to hang
//! an auto-refresh on.

use serde::{Deserialize, Serialize};

/// `integrations.github.status` (per-user App install state).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubStatus {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub install_url: Option<String>,
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
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubReposResult {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub install_url: Option<String>,
    #[serde(default)]
    pub repos: Vec<GithubRepo>,
    #[serde(default)]
    pub has_more: bool,
}

/// `integrations.github.status` — per-user install state (best-effort banner).
pub(crate) fn fetch_github_status(
    trpc: &api::TrpcClient,
) -> Result<GithubStatus, api::ApiError> {
    trpc.query("integrations.github.status")
}

/// `integrations.github.repos` — the installable-repo list. `refresh` bypasses
/// the server's per-user cache so a just-completed install reflects at once.
pub(crate) fn fetch_github_repos(
    trpc: &api::TrpcClient,
    refresh: bool,
) -> Result<GithubReposResult, api::ApiError> {
    if refresh {
        #[derive(Serialize)]
        struct Input {
            refresh: bool,
        }
        trpc.query_with_input("integrations.github.repos", &Input { refresh: true })
    } else {
        trpc.query("integrations.github.repos")
    }
}
