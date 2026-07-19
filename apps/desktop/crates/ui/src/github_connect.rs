//! Shared GitHub-App connect state + repo fetches (masterplan §7 IDE track).
//!
//! Both the team-settings Repositories pane and the create-board dialog
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
//! Both take the current `teamId`: GitHub-App installs are claimed PER
//! team, so the status/repo lists are scoped to the team's claimed
//! installations. `connectUrl` is a single-consent OAuth authorize URL that
//! claims the account for the team; `installUrl` is the broader App
//! install page (also grants more repos). The **connect** hand-off prefers
//! `connect_url` and falls back to `install_url`.
//!
//! The connect/install itself is still a browser hand-off: the pane/dialog open
//! the URL in the system browser (`settings::open_url`), then re-fetch on an
//! explicit Refresh — gpui has no reliable cross-app focus signal to hang an
//! auto-refresh on.
//!
//! **Grant model:** the server lists only repos the signed-in user proved
//! access to at OAuth-connect time (a per-user grant snapshot).
//! `installations[].needs_reauth` flags a linked account whose grants were
//! never captured (pre-grant link) — its repos stay hidden until the user
//! re-runs the OAuth connect. Reconnect/refresh CTAs must open `connect_url`
//! (the OAuth authorize re-captures grants); the App **install** page does
//! NOT.

use serde::{Deserialize, Serialize};

/// One linked GitHub-App installation — `installations[]` on both the status
/// and repos results. `needs_reauth` is the grant-model signal (see the
/// module doc); the other fields are modeled for contract completeness but
/// not consumed yet.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct GithubInstallation {
    #[serde(default)]
    pub installation_id: i64,
    #[serde(default)]
    pub account_login: Option<String>,
    #[serde(default)]
    pub account_type: Option<String>,
    #[serde(default)]
    pub manage_url: String,
    #[serde(default)]
    pub needs_reauth: bool,
    /// Only present on the `repos` endpoint's installations (whether that
    /// installation's repo listing was truncated).
    #[serde(default)]
    pub has_more: Option<bool>,
}

/// `integrations.github.status` (per-team App install state). Unknown
/// fields are ignored — this mirror carries only what the panes render.
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
    /// team — the preferred **connect** target (`connect_url.or(install_url)`).
    #[serde(default)]
    pub connect_url: Option<String>,
    #[serde(default)]
    pub accounts: Vec<String>,
    /// The team's linked installations — `needs_reauth` drives the
    /// grant-model reconnect notice.
    #[serde(default)]
    pub installations: Vec<GithubInstallation>,
}

/// One repo the signed-in user can connect inline — a row of
/// `integrations.github.repos`'s `repos[]` (server `InstallationRepo`). The
/// fields feed `boards.create`'s inline-repo union arm.
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
/// Unknown fields are ignored.
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
    /// team — the preferred **connect** target (`connect_url.or(install_url)`).
    #[serde(default)]
    pub connect_url: Option<String>,
    #[serde(default)]
    pub repos: Vec<GithubRepo>,
    #[serde(default)]
    pub has_more: bool,
    /// The team's linked installations — `needs_reauth` drives the
    /// grant-model reconnect notice.
    #[serde(default)]
    pub installations: Vec<GithubInstallation>,
}

/// `integrations.github.status` — per-team install state (best-effort
/// banner). `team_id` scopes the status to that team's claimed
/// installations.
pub(crate) fn fetch_github_status(
    trpc: &api::TrpcClient,
    team_id: &str,
) -> Result<GithubStatus, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("integrations.github.status", &Input { team_id })
}

/// `integrations.github.repos` — the installable-repo list for the team.
/// `refresh` bypasses the server's per-team cache so a just-completed
/// install reflects at once.
pub(crate) fn fetch_github_repos(
    trpc: &api::TrpcClient,
    team_id: &str,
    refresh: bool,
) -> Result<GithubReposResult, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh: Option<bool>,
    }
    trpc.query_with_input(
        "integrations.github.repos",
        &Input {
            team_id,
            refresh: refresh.then_some(true),
        },
    )
}
