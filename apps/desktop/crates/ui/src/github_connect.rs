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
            .unwrap_or_else(|| format!("installation {}", self.installation_id))
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

/// " from a, b" naming the reconnectable (needs-reauth, non-suspended,
/// non-stale) accounts — names make the reconnect actionable when several
/// accounts are linked (EXP-365). Empty when none are known. Stale accounts
/// (EXP-557) are excluded: they get a Disconnect affordance, never the
/// reconnect nag.
pub(crate) fn reauth_account_suffix(
    installations: &[GithubInstallation],
    preposition: &str,
) -> String {
    let names: Vec<String> = installations
        .iter()
        .filter(|inst| inst.needs_reconnect())
        .filter_map(|inst| inst.account_login.clone())
        .collect();
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
    /// NAMES come from (EXP-558 dropped the flat `accounts` mirror; serde
    /// ignores it while older servers still send it).
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

#[cfg(test)]
mod tests {
    use super::*;

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
