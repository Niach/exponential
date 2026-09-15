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
//! The connect/install itself is still a browser hand-off: the pane/dialog
//! open the URL in the system browser (`settings::open_url`). Since EXP-368
//! both fetchers send `platform: "mobile"`, so the server mints m-flagged
//! setup states (EXP-365): the web callbacks need no browser session and
//! every terminal branch hands back via the
//! `exponential://github-connected[?error=<code>]` deep link, which
//! [`crate::oauth::handle_open_urls`] routes into [`GithubConnectSignal`] —
//! live connect surfaces auto-refresh on success and show
//! [`connect_error_message`] copy on failure. The manual Refresh affordances
//! stay as the fallback (unpackaged dev builds may lack the scheme
//! registration). Inherited limitation, parity with mobile: the
//! multi-installation OAuth claim still 302s to the web claim page, which is
//! cookie-session-only.
//!
//! **Grant model:** the server lists only repos the signed-in user proved
//! access to at OAuth-connect time (a per-user grant snapshot).
//! `installations[].needs_reauth` flags a linked account whose grants were
//! never captured (pre-grant link) — its repos stay hidden until the user
//! re-runs the OAuth connect. Reconnect/refresh CTAs must open `connect_url`
//! (the OAuth authorize re-captures grants); the App **install** page does
//! NOT. `installations[].stale` (EXP-557) flags a linked account with zero
//! grants from ANYONE — no reconnect can refresh it; the fix is severing the
//! link ([`github_unlink`]), so stale accounts get a Disconnect affordance,
//! never the reconnect nag.

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

// The wire value of the `platform` input on both github queries. "mobile" =
// any native deep-link-capable client (the server's term predates the desktop
// IDE). Deliberately NOT "desktop": desktop talks to arbitrary self-hosted
// servers, and an old server's z.enum(["web","mobile"]) on `repos` would
// reject an unknown value and fail the whole query, while "mobile" is
// accepted everywhere (pre-EXP-365 servers just strip the key).
const PLATFORM: &str = "mobile";

/// One linked GitHub-App installation — `installations[]` on both the status
/// and repos results. `needs_reauth` is the grant-model signal (see the
/// module doc); `suspended` is a GitHub-side App suspension (REV2-29): the
/// installation lists no repos and mints no tokens until it's UNSUSPENDED on
/// GitHub — a reconnect cannot fix it, so the UI must never nudge one.
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
    #[serde(default)]
    pub suspended: bool,
    /// EXP-557: a linked account with ZERO repo grants from anyone — a
    /// reconnect can never refresh it; the only fix is disconnecting the
    /// link ([`github_unlink`]). The server includes foreign stale links
    /// only for team owners; members can still see their OWN link flagged.
    /// Absent on older servers (defaults false).
    #[serde(default)]
    pub stale: bool,
    /// Only present on the `repos` endpoint's installations (whether that
    /// installation's repo listing was truncated).
    #[serde(default)]
    pub has_more: Option<bool>,
}

impl GithubInstallation {
    /// The account's display label ("@login" without the @) for banners.
    pub(crate) fn label(&self) -> String {
        self.account_login
            .clone()
            .unwrap_or_else(|| copy::installation_fallback(self.installation_id))
    }

    /// The reconnect-nag arm: an OAuth reconnect can actually refresh this
    /// account's grants. Suspended installs need an unsuspend instead
    /// (REV2-29), and stale ones (EXP-557) can never be refreshed — both are
    /// reported separately, never as a reconnect.
    pub(crate) fn needs_reconnect(&self) -> bool {
        self.needs_reauth && !self.suspended && !self.stale
    }

    /// EXP-557: report as stale (Disconnect affordance). Suspension outranks
    /// it — unsuspend first, then judge staleness.
    pub(crate) fn is_stale(&self) -> bool {
        self.stale && !self.suspended
    }
}

/// The logins of the reconnectable (needs-reauth, non-suspended, non-stale)
/// accounts, in installation order — accounts without a login are skipped
/// (the banners name only what they can name, web parity).
pub(crate) fn reauth_logins(installations: &[GithubInstallation]) -> Vec<String> {
    installations
        .iter()
        .filter(|inst| inst.needs_reconnect())
        .filter_map(|inst| inst.account_login.clone())
        .collect()
}

/// " from a, b" naming the reconnectable (needs-reauth, non-suspended,
/// non-stale) accounts — names make the reconnect actionable when several
/// accounts are linked (EXP-365). Empty when none are known. Stale accounts
/// (EXP-557) are excluded: they get a Disconnect affordance, never the
/// reconnect nag.
pub(crate) fn reauth_account_suffix(
    installations: &[GithubInstallation],
    preposition: &str,
) -> String {
    let names = reauth_logins(installations);
    if names.is_empty() {
        String::new()
    } else {
        format!(" {preposition} {}", names.join(", "))
    }
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
    /// The team's linked installations — `needs_reauth` drives the
    /// grant-model reconnect notice, and `account_login` is where the account
    /// NAMES come from (EXP-558 dropped the flat `accounts` mirror the server
    /// used to send alongside). `default` keeps an older self-hosted server,
    /// which may omit the field entirely, decoding.
    #[serde(default)]
    pub installations: Vec<GithubInstallation>,
}

/// One repo the signed-in user can connect inline — a row of
/// `integrations.github.repos`'s `repos[]` (server `InstallationRepo`). These
/// are exactly the fields `boards.create`'s inline-repo union arm takes
/// (`fullName`/`defaultBranch`/`private`); the row's `installationId` is
/// deliberately NOT mirrored — the server re-resolves the installation from
/// the full name, so carrying a client copy would only invite passing it.
/// Unknown fields on the wire are ignored.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubRepo {
    pub full_name: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub default_branch: String,
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
        platform: &'a str,
    }
    trpc.query_with_input(
        "integrations.github.status",
        &Input {
            team_id,
            platform: PLATFORM,
        },
    )
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
        platform: &'a str,
    }
    trpc.query_with_input(
        "integrations.github.repos",
        &Input {
            team_id,
            refresh: refresh.then_some(true),
            platform: PLATFORM,
        },
    )
}

/// FEED-30: the ONE "owner/name" shape every repo-by-name entry point accepts
/// — mirror of the web `REPO_FULL_NAME_RE` (`/^[^/\s]+\/[^/\s]+$/`): exactly
/// one slash, both halves non-empty, no whitespace anywhere. The picker's
/// "Add by name" field validates against it so a name the client lets through
/// is never one the server rejects on shape alone.
pub(crate) fn is_repo_full_name(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && !name.contains('/')
        && !value.chars().any(char::is_whitespace)
}

/// `integrations.github.lookupRepo` (FEED-30) — the Add-repository picker's
/// "Add by name" escape hatch. Resolves a full name through the connect path's
/// own checks (linked installation, not suspended, the actor's own grant on
/// OAuth instances), so its error names the real reason and is
/// user-presentable verbatim. Read-only. The result carries exactly the
/// picker-row fields, so a hit is handled like a row pick.
pub(crate) fn lookup_repo(
    trpc: &api::TrpcClient,
    team_id: &str,
    full_name: &str,
) -> Result<GithubRepo, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        full_name: &'a str,
    }
    trpc.query_with_input(
        "integrations.github.lookupRepo",
        &Input { team_id, full_name },
    )
}

/// `integrations.github.unlink` — mutation (EXP-557): sever ONE linked
/// installation from the team. Server-gated link-creator-or-owner, and
/// refused (CONFLICT) while a connected repo still rides the installation —
/// that message is user-presentable verbatim. The Repositories pane offers
/// it on STALE links only ([`GithubInstallation::is_stale`]).
pub(crate) fn github_unlink(
    trpc: &api::TrpcClient,
    team_id: &str,
    installation_id: i64,
) -> Result<(), api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        installation_id: i64,
    }
    // `{ ok: true }` on success — nothing the caller consumes.
    #[derive(Deserialize)]
    struct Output {
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Output = trpc.mutation(
        "integrations.github.unlink",
        &Input {
            team_id,
            installation_id,
        },
    )?;
    Ok(())
}

/// Outcome of the browser GitHub-connect hand-off, delivered by the
/// `exponential://github-connected[?error=<code>]` deep link (EXP-365 wire
/// contract, consumed on desktop since EXP-368).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubConnectOutcome {
    Connected,
    /// The server's short error slug
    /// (`session|exchange|none|notowner|orgperm|forbidden`) — map it through
    /// [`connect_error_message`], never render it raw.
    Failed(String),
}

/// `exponential://github-connected[?error=<code>]` → `Some(outcome)`; anything
/// else `None`. Mirror of [`crate::oauth::parse_issue_deep_link`]'s shape. The
/// slug is not percent-decoded: the server clamps error codes to plain ASCII
/// slugs, which are decode-inert.
pub(crate) fn parse_github_connected_deep_link(url: &str) -> Option<GithubConnectOutcome> {
    let prefix = format!("{}://github-connected", api::login::OAUTH_CALLBACK_SCHEME);
    let rest = url.strip_prefix(prefix.as_str())?;
    // Only the bare host, `/`, `?query` or `#fragment` may follow — anything
    // else is a different host sharing the prefix.
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    if !(rest.is_empty() || rest.starts_with('?') || rest.starts_with('#')) {
        return None;
    }
    let query = rest
        .split('#')
        .next()
        .unwrap_or_default()
        .strip_prefix('?')
        .unwrap_or_default();
    let error = query.split('&').find_map(|pair| {
        pair.split_once('=')
            .filter(|(k, v)| *k == "error" && !v.is_empty())
            .map(|(_, v)| v.to_string())
    });
    Some(match error {
        Some(code) => GithubConnectOutcome::Failed(code),
        None => GithubConnectOutcome::Connected,
    })
}

/// The last github-connected deep-link outcome. [`crate::oauth::handle_open_urls`]
/// bumps it; every live GitHub-connect surface (create-board dialog incl. the
/// onboarding wizard's embedded copy, settings Repositories pane, add-repository
/// dialog) adopts it via `observe_global` — same shape as
/// [`crate::login::OAuthFailure`], because the deep link arrives on the `App`
/// with no view in hand and several independent views must all react. Not
/// consumed: observers fire only on the bump, so a surface opened later never
/// re-applies a stale outcome; if NO surface is alive the outcome is
/// intentionally dropped — the browser return page already showed the same
/// copy.
#[derive(Default)]
pub(crate) struct GithubConnectSignal {
    pub outcome: Option<GithubConnectOutcome>,
}

impl Global for GithubConnectSignal {}

/// Publish a github-connected outcome to every live connect surface.
pub(crate) fn report_github_connected(outcome: GithubConnectOutcome, cx: &mut App) {
    cx.default_global::<GithubConnectSignal>().outcome = Some(outcome);
}

/// Human copy for a github-connected `?error=` slug — the in-app mirror of the
/// web return page's `ERROR_COPY` (`lib/integrations/github-return-page.ts`),
/// reworded for a surface that IS the app (no "return to the app"). Unknown
/// slugs (a newer server) fall back to the generic line, like
/// [`crate::oauth::complete`]'s `oauth_error_message`.
pub(crate) fn connect_error_message(code: &str) -> &'static str {
    match code {
        "session" => "The connect link expired or was already used. Try connecting again.",
        "exchange" => "GitHub sign-in didn't complete. Try connecting again.",
        "none" => {
            "The Exponential GitHub App isn't installed for any account you can access yet. Use Connect GitHub to install it."
        }
        "notowner" => {
            "The authorized GitHub account only has collaborator access to existing installations. Install the App on your own account or organization."
        }
        "orgperm" => {
            "Your organization hasn't approved the App's members-read permission yet. An org admin must accept it on GitHub, then reconnect."
        }
        // EXP-557 per-user sharing: connecting is member-level now — the slug
        // means "not a team member" (web claim-page wording).
        "forbidden" => "Only team members can connect GitHub accounts to a team.",
        _ => "Something went wrong while connecting GitHub. Please try again.",
    }
}

/// FEED-42: the ONE copy of the GitHub connection block (Settings ›
/// Repositories) and the Add-repository picker. Byte-identical ×4 — web
/// `github-connect-copy.ts`, iOS `GithubCopy.swift`, Android `GithubCopy.kt`
/// mirror these literals; the `copy_is_locked` test pins the key ones.
/// Typographic ’ everywhere.
pub(crate) mod copy {
    // --- A. Connection block -------------------------------------------------
    pub const SECTION_TITLE: &str = "Repositories";
    pub const ADD_REPOSITORY: &str = "Add repository";
    pub const INTRO: &str = "Connect a GitHub account or organization first, then add its \
        repositories to share them with the team \u{2014} everyone can code on a shared repo. \
        Point a board at one to make it the clone target for \u{201c}Start coding\u{201d}.";
    pub const STATUS_FAILED: &str = "Couldn\u{2019}t reach GitHub connect state.";
    pub const RETRY: &str = "Retry";
    pub const NOT_CONFIGURED: &str = "GitHub isn\u{2019}t configured on this server.";
    pub const NOT_INSTALLED: &str = "No GitHub account connected";
    pub const CONNECT_GITHUB: &str = "Connect GitHub";
    pub const INSTALL_ON_AN_ACCOUNT: &str = "Install on an account";
    pub const MANAGE: &str = "Manage";
    pub const ACCOUNTS_HEADER: &str = "GitHub accounts connected to this team";
    pub const CONFIGURE: &str = "Configure";
    pub const INSTALLATION_CAPTION: &str = "An installation is per GitHub account or \
        organization. Repositories come from the accounts listed here.";
    pub const CONNECT_ANOTHER_ACCOUNT: &str = "Connect another account";
    pub const REFRESH_ACCESS: &str = "Refresh access";
    pub const RECONNECT: &str = "Reconnect";
    pub const DISCONNECT_ACCOUNT: &str = "Disconnect account";
    pub const DISCONNECT_TITLE: &str = "Disconnect GitHub account";
    pub const DISCONNECT: &str = "Disconnect";
    pub const NO_REPOSITORIES: &str = "No repositories connected yet.";

    /// The account label when GitHub gave no login (lower-case, ×4).
    pub fn installation_fallback(installation_id: i64) -> String {
        format!("installation {installation_id}")
    }

    pub fn suspended_status(names: &str) -> String {
        format!("GitHub suspended the Exponential app for {names}. Unsuspend it on GitHub.")
    }

    pub fn reauth_status(names: &str) -> String {
        format!("Reconnect GitHub to refresh which repositories you can access from {names}.")
    }

    pub fn stale_line(label: &str) -> String {
        format!(
            "No one\u{2019}s GitHub connection covers {label} anymore \u{2014} reconnecting \
             can\u{2019}t refresh it."
        )
    }

    /// Unlink confirm for a LIVE link (the server refuses while a connected
    /// repo still rides it).
    pub fn disconnect_live_confirm(label: &str) -> String {
        format!(
            "This disconnects {label} from the team. Repositories connected through it must \
             be removed first."
        )
    }

    /// Unlink confirm for a STALE link (EXP-557).
    pub fn disconnect_stale_confirm(label: &str) -> String {
        format!(
            "This removes {label} from the team. Nobody\u{2019}s GitHub connection covers it, \
             so no repositories are lost."
        )
    }

    // --- B. Add-repository picker --------------------------------------------
    pub const LOADING_REPOS: &str = "Loading your GitHub repositories\u{2026}";
    pub const PICKER_NOT_CONFIGURED: &str = "GitHub isn\u{2019}t configured on this server, so \
        repositories can\u{2019}t be connected.";
    pub const PICKER_NOT_INSTALLED: &str = "Connect the Exponential GitHub App to pick a \
        repository. You\u{2019}ll come right back here.";
    pub const I_HAVE_CONNECTED: &str = "I\u{2019}ve connected";
    /// A suspended installation with no login in the picker banner.
    pub const SUSPENDED_FALLBACK: &str = "a connected account";
    pub const RECONNECT_GITHUB: &str = "Reconnect GitHub";
    pub const SEARCH_PLACEHOLDER: &str = "Search repositories\u{2026}";
    pub const NO_REPOS_FOUND: &str = "No repositories found.";
    pub const NO_GRANTS: &str = "None of your connected GitHub accounts grants a repository yet.";
    pub const FOOTER_SENTENCE: &str = "Only repositories your GitHub installation grants appear \
        here. Missing one? Grant it on GitHub, then refresh.";
    pub const HAS_MORE: &str = "Showing the first 500 repositories per account \u{2014} use the \
        field below for the rest.";
    pub const REFRESH: &str = "Refresh";
    pub const INSTALL_ON_ANOTHER_ACCOUNT: &str = "Install on another account";
    pub const LOOKUP_PLACEHOLDER: &str = "owner/name";
    /// The by-name field's accessibility label on web/iOS/Android; gpui's
    /// `Input` exposes no label slot yet, so desktop only mirrors it.
    #[allow(dead_code)]
    pub const LOOKUP_A11Y: &str = "Add repository by name";
    pub const LOOK_UP: &str = "Look up";
    pub const ADD_FORBIDDEN: &str = "GitHub says you don\u{2019}t have access to this repository, \
        or your connection is stale. Reconnect GitHub and try again.";
    pub const UPGRADE_ON_THE_WEB: &str = "Upgrade on the web";

    pub fn picker_suspended(names: &str) -> String {
        format!(
            "GitHub suspended the Exponential app for {names}. Its repositories can\u{2019}t be \
             connected until you unsuspend it on GitHub."
        )
    }

    /// Re-auth banner over a list that HAS rows.
    pub fn picker_reauth(logins: &[String]) -> String {
        let names = if logins.is_empty() {
            String::new()
        } else {
            format!(" ({})", logins.join(", "))
        };
        format!(
            "Reconnect GitHub{names} to refresh. Repos created or shared with you since your \
             last connect won\u{2019}t appear until you do."
        )
    }

    /// Re-auth banner over an EMPTY list.
    pub fn picker_reauth_empty(logins: &[String]) -> String {
        let names = if logins.is_empty() {
            String::new()
        } else {
            format!(" from {}", logins.join(", "))
        };
        format!("Reconnect GitHub to load the repositories you can access{names}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FEED-42: the copy is byte-identical ×4 — these literals mirror web
    /// `github-connect-copy.ts`; change all four clients together.
    #[test]
    fn copy_is_locked() {
        assert_eq!(
            copy::INTRO,
            "Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”."
        );
        assert_eq!(copy::STATUS_FAILED, "Couldn’t reach GitHub connect state.");
        assert_eq!(copy::RETRY, "Retry");
        assert_eq!(copy::NOT_CONFIGURED, "GitHub isn’t configured on this server.");
        assert_eq!(copy::NOT_INSTALLED, "No GitHub account connected");
        assert_eq!(
            copy::INSTALLATION_CAPTION,
            "An installation is per GitHub account or organization. Repositories come from the accounts listed here."
        );
        assert_eq!(copy::installation_fallback(42), "installation 42");
        assert_eq!(
            copy::suspended_status("acme"),
            "GitHub suspended the Exponential app for acme. Unsuspend it on GitHub."
        );
        assert_eq!(
            copy::reauth_status("a, b"),
            "Reconnect GitHub to refresh which repositories you can access from a, b."
        );
        assert_eq!(
            copy::stale_line("acme"),
            "No one’s GitHub connection covers acme anymore — reconnecting can’t refresh it."
        );
        assert_eq!(
            copy::disconnect_live_confirm("acme"),
            "This disconnects acme from the team. Repositories connected through it must be removed first."
        );
        assert_eq!(
            copy::disconnect_stale_confirm("acme"),
            "This removes acme from the team. Nobody’s GitHub connection covers it, so no repositories are lost."
        );
        assert_eq!(copy::LOADING_REPOS, "Loading your GitHub repositories…");
        assert_eq!(
            copy::PICKER_NOT_CONFIGURED,
            "GitHub isn’t configured on this server, so repositories can’t be connected."
        );
        assert_eq!(
            copy::PICKER_NOT_INSTALLED,
            "Connect the Exponential GitHub App to pick a repository. You’ll come right back here."
        );
        assert_eq!(copy::I_HAVE_CONNECTED, "I’ve connected");
        assert_eq!(
            copy::picker_suspended("a connected account"),
            "GitHub suspended the Exponential app for a connected account. Its repositories can’t be connected until you unsuspend it on GitHub."
        );
        let logins = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            copy::picker_reauth(&logins),
            "Reconnect GitHub (a, b) to refresh. Repos created or shared with you since your last connect won’t appear until you do."
        );
        assert_eq!(
            copy::picker_reauth(&[]),
            "Reconnect GitHub to refresh. Repos created or shared with you since your last connect won’t appear until you do."
        );
        assert_eq!(
            copy::picker_reauth_empty(&logins),
            "Reconnect GitHub to load the repositories you can access from a, b."
        );
        assert_eq!(
            copy::NO_GRANTS,
            "None of your connected GitHub accounts grants a repository yet."
        );
        assert_eq!(
            copy::FOOTER_SENTENCE,
            "Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh."
        );
        assert_eq!(
            copy::HAS_MORE,
            "Showing the first 500 repositories per account — use the field below for the rest."
        );
        assert_eq!(
            copy::ADD_FORBIDDEN,
            "GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again."
        );
        assert_eq!(copy::UPGRADE_ON_THE_WEB, "Upgrade on the web");
    }

    #[test]
    fn parses_success_forms() {
        for url in [
            "exponential://github-connected",
            "exponential://github-connected/",
            "exponential://github-connected?",
            "exponential://github-connected?error=",
            "exponential://github-connected#frag",
        ] {
            assert_eq!(
                parse_github_connected_deep_link(url),
                Some(GithubConnectOutcome::Connected),
                "{url}"
            );
        }
    }

    #[test]
    fn parses_error_slug() {
        assert_eq!(
            parse_github_connected_deep_link("exponential://github-connected?error=session"),
            Some(GithubConnectOutcome::Failed("session".to_string()))
        );
        assert_eq!(
            parse_github_connected_deep_link(
                "exponential://github-connected?error=orgperm#error=ignored"
            ),
            Some(GithubConnectOutcome::Failed("orgperm".to_string()))
        );
    }

    #[test]
    fn rejects_other_urls() {
        for url in [
            "exponential://oauth-return?error=session",
            "exponential://invite/abc",
            "exponential://github-connectedish?error=x",
            "https://app.exponential.at/github-connected",
        ] {
            assert_eq!(parse_github_connected_deep_link(url), None, "{url}");
        }
    }

    fn installation(
        login: &str,
        needs_reauth: bool,
        suspended: bool,
        stale: bool,
    ) -> GithubInstallation {
        GithubInstallation {
            installation_id: 1,
            account_login: Some(login.to_string()),
            account_type: None,
            manage_url: String::new(),
            needs_reauth,
            suspended,
            stale,
            has_more: None,
        }
    }

    /// EXP-557: `stale` is additive — an older server's installation row
    /// (no `stale` key) must deserialize with it false.
    #[test]
    fn stale_defaults_false_on_old_servers() {
        let inst: GithubInstallation =
            serde_json::from_str(r#"{"installationId":7,"accountLogin":"acme"}"#).unwrap();
        assert!(!inst.stale);
        assert!(!inst.is_stale());
    }

    /// A stale account is reported separately from the needs-reauth ones: it
    /// gets the Disconnect affordance, never the reconnect nag — and
    /// suspension outranks both (REV2-29).
    #[test]
    fn stale_and_reconnect_derivations_are_disjoint() {
        let reconnect = installation("a", true, false, false);
        assert!(reconnect.needs_reconnect());
        assert!(!reconnect.is_stale());

        // Stale wins over needs_reauth — reconnecting can't fix it.
        let stale = installation("b", true, false, true);
        assert!(!stale.needs_reconnect());
        assert!(stale.is_stale());

        let suspended_stale = installation("c", true, true, true);
        assert!(!suspended_stale.needs_reconnect());
        assert!(!suspended_stale.is_stale());
    }

    #[test]
    fn reauth_suffix_skips_stale_and_suspended_accounts() {
        let installations = vec![
            installation("fresh", true, false, false),
            installation("gone-stale", true, false, true),
            installation("suspended", true, true, false),
        ];
        assert_eq!(reauth_account_suffix(&installations, "from"), " from fresh");

        let all_stale = vec![installation("gone-stale", true, false, true)];
        assert_eq!(reauth_account_suffix(&all_stale, "from"), "");
    }

    /// FEED-30: the by-name field's shape check mirrors the web
    /// `REPO_FULL_NAME_RE` — one slash, both halves present, no whitespace.
    #[test]
    fn repo_full_name_shape_matches_the_web_pattern() {
        for ok in ["acme/web", "a/b", "org-name/repo.name", "Niach/exponential"] {
            assert!(is_repo_full_name(ok), "{ok}");
        }
        for bad in [
            "",
            "acme",
            "acme/",
            "/web",
            "acme/web/extra",
            "acme /web",
            "acme/we b",
            " acme/web",
            "acme/web\n",
        ] {
            assert!(!is_repo_full_name(bad), "{bad:?}");
        }
    }

    /// The lookup result decodes into the picker-row type — `installationId`
    /// on the wire is ignored on purpose (the server re-resolves it).
    #[test]
    fn lookup_result_decodes_as_a_picker_row() {
        let repo: GithubRepo = serde_json::from_str(
            r#"{"fullName":"acme/web","private":true,"defaultBranch":"trunk","installationId":7}"#,
        )
        .unwrap();
        assert_eq!(repo.full_name, "acme/web");
        assert!(repo.private);
        assert_eq!(repo.default_branch, "trunk");
    }

    #[test]
    fn error_copy_covers_every_server_slug() {
        let generic = connect_error_message("some-future-slug");
        for slug in [
            "session",
            "exchange",
            "none",
            "notowner",
            "orgperm",
            "forbidden",
        ] {
            assert_ne!(connect_error_message(slug), generic, "{slug}");
        }
    }
}
