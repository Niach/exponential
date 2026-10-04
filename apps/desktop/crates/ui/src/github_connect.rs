//! The ONE GitHub flow on the IDE (SLOP-7 / SLOP-25).
//!
//! A person CONNECTS their GitHub account (a Better Auth `github` social
//! link on their Exponential account), INSTALLS the Exponential app on the
//! GitHub accounts whose repositories they want, and ADDS repositories they
//! can push to. Nothing is claimed per team any more: the connection is the
//! person's, the repositories below it are the team's. Every surface reads
//! the **live server truth**:
//!
//! - [`fetch_github_status`] → `integrations.github.status`
//!   (`{configured, linked, needsReconnect, login, installed, installUrl,
//!   connectUrl, installations[]}`) drives the connection block of Settings ›
//!   Repositories and the readiness checklist's GitHub step;
//! - [`fetch_github_repos`] → `integrations.github.repos` (the same fields
//!   plus `repos[]`/`hasMore`) lists the push-able repositories inline,
//!   mirroring the web `GithubRepoPicker` (`components/github-repo-picker.tsx`).
//!
//! The three prerequisites and their ONE fix each ([`Prerequisite`]):
//!
//! - **not linked** → Connect GitHub = the EXP-1126 link-ticket path
//!   ([`connect_github`]: `users.mintSignInLinkTicket({provider: "github"})`
//!   → `/api/mobile-oauth-start?provider=github&link=<ticket>` in the system
//!   browser → `exponential://oauth-return?linked=github`, which
//!   [`crate::oauth`] turns into a [`crate::oauth::SignInLinkOutcome`] every
//!   live surface observes);
//! - **expired** (`needsReconnect`: the stored token was refused) → Reconnect
//!   GitHub = the same hop;
//! - **not installed** → Install the app = `installUrl` (GitHub's install
//!   page) in the browser. The App's setup URL lands on the web page, which
//!   offers "Return to the app" → `exponential://github-connected`
//!   ([`parse_github_connected_deep_link`] → [`GithubConnectSignal`]).
//!
//! Window activation stays the fallback refetch trigger on every surface
//! (unpackaged dev builds may lack the scheme registration).

use gpui::{App, Global, SharedString};
use serde::{Deserialize, Serialize};

/// The Better Auth provider id of the GitHub connection (SLOP-7).
pub(crate) const GITHUB_PROVIDER_ID: &str = "github";

// The wire value of the `platform` input on both github queries. "mobile" =
// any native deep-link-capable client (the server's term predates the desktop
// IDE): the server marks `connectUrl` with `return=app` so the guided web page
// hands back through the deep link. Deliberately NOT "desktop": desktop talks
// to arbitrary self-hosted servers whose z.enum(["web","mobile"]) would reject
// an unknown value and fail the whole query.
const PLATFORM: &str = "mobile";

/// One GitHub-App installation the viewer's token can see — `installations[]`
/// on both the status and repos results. `suspended` is a GitHub-side App
/// suspension (REV2-29): the installation lists no repos and mints no tokens
/// until it's UNSUSPENDED on GitHub — a reconnect cannot fix it, so the UI
/// must never nudge one. The pre-SLOP-7 `needsReauth`/`stale` flags the
/// server still sends (always false) are ignored on the wire.
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
    /// The installation's GitHub settings page (Configure / Manage).
    #[serde(default)]
    pub manage_url: String,
    #[serde(default)]
    pub suspended: bool,
    /// Only present on the `repos` endpoint's installations (whether that
    /// installation's repo listing was truncated).
    #[serde(default)]
    pub has_more: Option<bool>,
}

impl GithubInstallation {
    /// The account's label: the login, else lower-case `installation {id}`
    /// (web `githubInstallationLabel`, ×4).
    pub(crate) fn label(&self) -> String {
        self.account_login
            .clone()
            .filter(|login| !login.is_empty())
            .unwrap_or_else(|| copy::installation_fallback(self.installation_id))
    }

    /// Organization installs wear the building glyph, personal ones the
    /// person glyph (web `Building2` / `User`).
    pub(crate) fn is_organization(&self) -> bool {
        self.account_type.as_deref() == Some("Organization")
    }
}

/// Which prerequisite of the flow is missing, in the order the surfaces
/// check them. Derived from the connection fields both queries share
/// ([`GithubStatus::prerequisite`], [`GithubReposResult::prerequisite`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prerequisite {
    /// The GitHub App isn't configured on this server: nothing can be
    /// connected here.
    NotConfigured,
    /// No GitHub account linked → Connect GitHub.
    NotLinked,
    /// Linked, but the stored token was refused → Reconnect GitHub.
    Expired,
    /// Linked, but the app is installed nowhere the viewer can see →
    /// Install the app.
    NotInstalled,
    /// Everything in place: the live repository list applies.
    Ready,
}

fn prerequisite_of(
    configured: bool,
    linked: bool,
    needs_reconnect: bool,
    installed: bool,
) -> Prerequisite {
    if !configured {
        Prerequisite::NotConfigured
    } else if !linked {
        Prerequisite::NotLinked
    } else if needs_reconnect {
        Prerequisite::Expired
    } else if !installed {
        Prerequisite::NotInstalled
    } else {
        Prerequisite::Ready
    }
}

/// `integrations.github.status` — the viewer's connection in a team context.
/// Unknown fields are ignored; every field defaults so an older self-hosted
/// server (no `linked`/`login`/`needsReconnect`) still decodes — it then reads
/// as not linked, whose fix is the connect hop anyway.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubStatus {
    #[serde(default)]
    pub configured: bool,
    /// The App's OAuth client is configured too, so connecting is possible.
    #[serde(default)]
    #[allow(dead_code)]
    pub connect_configured: bool,
    /// The viewer has a GitHub account linked.
    #[serde(default)]
    pub linked: bool,
    /// Linked, but GitHub refused the stored token: reconnect.
    #[serde(default)]
    pub needs_reconnect: bool,
    /// The linked account's GitHub login.
    #[serde(default)]
    pub login: Option<String>,
    /// Linked AND the app is installed on at least one account the token sees.
    #[serde(default)]
    pub installed: bool,
    /// GitHub's install page (installations/new) — Install the app / Install
    /// on another account.
    #[serde(default)]
    pub install_url: Option<String>,
    /// The guided web page (`/integrations/github?return=app`). Natives
    /// prefer the in-app link hop ([`connect_github`]); kept decoded for
    /// older servers and as the documented hand-back origin.
    #[serde(default)]
    #[allow(dead_code)]
    pub connect_url: Option<String>,
    /// The accounts where the app is installed, as the viewer's token sees
    /// them. Empty while not linked.
    #[serde(default)]
    pub installations: Vec<GithubInstallation>,
}

impl GithubStatus {
    pub(crate) fn prerequisite(&self) -> Prerequisite {
        prerequisite_of(self.configured, self.linked, self.needs_reconnect, self.installed)
    }

    /// The suspended installations' labels (REV2-29).
    pub(crate) fn suspended_labels(&self) -> Vec<String> {
        suspended_labels(&self.installations)
    }
}

/// One repo the signed-in user can add — a row of
/// `integrations.github.repos`'s `repos[]` (server `InstallationRepo`). These
/// are exactly the fields `repositories.add` and `boards.create`'s inline-repo
/// arm take (`fullName`/`defaultBranch`/`private`); the row's `installationId`
/// is deliberately NOT mirrored — the server re-resolves the installation from
/// the full name. Unknown fields on the wire are ignored.
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
    #[allow(dead_code)]
    pub connect_configured: bool,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub needs_reconnect: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub login: Option<String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub install_url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub connect_url: Option<String>,
    /// Every push-able repo of every installation, deduped and sorted.
    #[serde(default)]
    pub repos: Vec<GithubRepo>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub installations: Vec<GithubInstallation>,
}

impl GithubReposResult {
    pub(crate) fn prerequisite(&self) -> Prerequisite {
        prerequisite_of(self.configured, self.linked, self.needs_reconnect, self.installed)
    }

    /// The suspended installations' labels, "a connected account" standing in
    /// for one without a login (web `GH_SUSPENDED_ACCOUNT_FALLBACK`).
    pub(crate) fn suspended_picker_labels(&self) -> Vec<String> {
        self.installations
            .iter()
            .filter(|inst| inst.suspended)
            .map(|inst| {
                inst.account_login
                    .clone()
                    .filter(|login| !login.is_empty())
                    .unwrap_or_else(|| copy::SUSPENDED_FALLBACK.to_string())
            })
            .collect()
    }
}

fn suspended_labels(installations: &[GithubInstallation]) -> Vec<String> {
    installations
        .iter()
        .filter(|inst| inst.suspended)
        .map(GithubInstallation::label)
        .collect()
}

/// `integrations.github.status` — the viewer's connection for `team_id`
/// (member-gated; best-effort banner).
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

/// `integrations.github.repos` — the viewer's push-able repositories.
/// `refresh` bypasses the server's per-user discovery cache so a hop just
/// finished on GitHub reflects at once.
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
/// "Add by name" escape hatch. Resolves a full name through the connect
/// gate's own checks (linked, token alive, push access, app installed), so
/// its error names the real reason and is user-presentable verbatim.
/// Read-only. The result carries exactly the picker-row fields, so a hit is
/// handled like a row pick.
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

/// `integrations.github.disconnect` — mutation (SLOP-7): unlink the viewer's
/// GitHub account. Repositories already added keep working (their tokens mint
/// off the App installation); adding more needs a reconnect. The server's
/// last-way-in refusal (GitHub login on, no other way in) is user-presentable
/// verbatim.
pub(crate) fn github_disconnect(trpc: &api::TrpcClient) -> Result<(), api::ApiError> {
    // `{ ok: true }` on success — nothing the caller consumes.
    #[derive(Deserialize)]
    struct Output {
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Output = trpc.mutation_no_input("integrations.github.disconnect")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Connect GitHub — the link-ticket hop
// ---------------------------------------------------------------------------

/// Connect (or reconnect) GitHub: mint the EXP-1126 link ticket for the
/// `github` provider and open the browser handoff in LINK mode. The outcome
/// lands on the App as a [`crate::oauth::SignInLinkOutcome`] whose
/// `provider_id` is [`GITHUB_PROVIDER_ID`] — every live GitHub surface
/// observes it ([`github_link_outcome`]) and re-reads the connect state.
///
/// Failures before the browser opens (no session, the server refusing the
/// ticket, a broken opener) surface as a toast on the active window; a broken
/// opener also copies the start URL so the hop still completes by paste
/// (never a dead end).
pub(crate) fn connect_github(cx: &mut App) {
    let (Some(trpc), Some(account)) = (
        crate::queries::trpc_client(cx),
        crate::queries::active_account(cx),
    ) else {
        crate::toast::show_in_active_window(
            crate::toast::Toast::error(copy::CONNECT_NOT_SIGNED_IN),
            cx,
        );
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::users::mint_sign_in_link_ticket(&trpc, GITHUB_PROVIDER_ID)
            })
            .await;
        let _ = cx.update(|cx| {
            let ticket = match result {
                Ok(ticket) => ticket,
                Err(err) => {
                    log::warn!("[ui] users.mintSignInLinkTicket(github) failed: {err}");
                    crate::toast::show_in_active_window(
                        crate::toast::Toast::error(crate::settings::form_error(
                            &err,
                            copy::CONNECT_START_FAILED,
                        )),
                        cx,
                    );
                    return;
                }
            };
            let pkce = api::login::generate_pkce();
            let url = api::login::link_start_url(
                &account.instance_url,
                &ticket.ticket,
                GITHUB_PROVIDER_ID,
                &pkce.challenge,
            );
            if let Err(url) = crate::oauth::start_link(
                account.id.clone(),
                GITHUB_PROVIDER_ID.to_string(),
                url,
                pkce.verifier,
                cx,
            ) {
                // The whole opener chain failed — the pending link is
                // recorded, so the copied URL still completes in this process.
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
                crate::toast::show_in_active_window(
                    crate::toast::Toast::error(copy::CONNECT_OPENER_FAILED),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// The last link-mode outcome IF it was the GitHub connection's: `Ok(())`
/// (re-read the connect state) or `Err(copy)` (the web's link-error line).
/// `None` = no outcome, or another provider's. Read inside an
/// `observe_global::<SignInLinkOutcome>` callback.
pub(crate) fn github_link_outcome(cx: &App) -> Option<Result<(), SharedString>> {
    let outcome = cx.try_global::<crate::oauth::SignInLinkOutcome>()?;
    let result = outcome.result.as_ref()?;
    let provider = outcome
        .provider_id
        .as_deref()
        .or_else(|| result.as_ref().ok().map(String::as_str))?;
    if provider != GITHUB_PROVIDER_ID {
        return None;
    }
    Some(match result {
        Ok(_) => Ok(()),
        Err(message) => Err(message.clone()),
    })
}

// ---------------------------------------------------------------------------
// The github-connected hand-back (Install the app → web page → Return)
// ---------------------------------------------------------------------------

/// Outcome of the browser GitHub hand-off, delivered by the
/// `exponential://github-connected[?error=<code>]` deep link the guided web
/// page fires from its "Return to the app" button (EXP-365 wire contract,
/// consumed on desktop since EXP-368).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GithubConnectOutcome {
    Connected,
    /// The server's short error slug — map it through
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
/// bumps it; every live GitHub surface (Repositories pane, Add-repository
/// dialog, create-board dialog incl. the onboarding wizard's embedded copy,
/// the readiness control) adopts it via `observe_global` — same shape as
/// [`crate::login::OAuthFailure`], because the deep link arrives on the `App`
/// with no view in hand and several independent views must all react. Not
/// consumed: observers fire only on the bump, so a surface opened later never
/// re-applies a stale outcome; if NO surface is alive the outcome is
/// intentionally dropped — the browser page already showed the same copy.
#[derive(Default)]
pub(crate) struct GithubConnectSignal {
    pub outcome: Option<GithubConnectOutcome>,
}

impl Global for GithubConnectSignal {}

/// Publish a github-connected outcome to every live connect surface.
pub(crate) fn report_github_connected(outcome: GithubConnectOutcome, cx: &mut App) {
    cx.default_global::<GithubConnectSignal>().outcome = Some(outcome);
}

/// Human copy for a github-connected `?error=` slug. SLOP-7's guided page
/// hands back without an error (every failure is shown on the page itself),
/// so any slug — an older server's `session|exchange|none|…` included — reads
/// as the one generic line, never raw.
pub(crate) fn connect_error_message(_code: &str) -> &'static str {
    copy::CONNECT_FAILED
}

/// SLOP-7: the ONE copy of the GitHub connection block (Settings ›
/// Repositories) and the Add-repository picker. Byte-identical ×4 — web
/// `github-connect-copy.ts` (sections B + C), iOS `GithubCopy.swift`,
/// Android `GithubCopy.kt` mirror these literals; the `copy_is_locked` test
/// pins the key ones. Typographic ’ everywhere.
pub(crate) mod copy {
    // --- B. Connection block (Settings › Repositories) -----------------------
    pub const SECTION_TITLE: &str = "Repositories";
    pub const ADD_REPOSITORY: &str = "Add repository";
    pub const STATUS_FAILED: &str = "Couldn\u{2019}t reach GitHub connect state.";
    pub const RETRY: &str = "Retry";
    pub const NOT_CONFIGURED: &str = "GitHub isn\u{2019}t configured on this server.";
    pub const NOT_LINKED: &str = "No GitHub account connected";
    pub const CONNECT_GITHUB: &str = "Connect GitHub";
    pub const CONNECTED: &str = "GitHub connected";
    pub const RECONNECT_NEEDED: &str = "Your GitHub connection expired.";
    pub const RECONNECT: &str = "Reconnect";
    pub const RECONNECT_GITHUB: &str = "Reconnect GitHub";
    pub const DISCONNECT: &str = "Disconnect";
    pub const DISCONNECT_CONFIRM_TITLE: &str = "Disconnect GitHub";
    pub const DISCONNECT_BODY: &str = "This unlinks GitHub from your account. Repositories \
        already added keep working; adding more needs a reconnect.";
    pub const NOT_INSTALLED: &str = "The Exponential app isn\u{2019}t installed on any of your \
        GitHub accounts yet.";
    pub const INSTALL_APP: &str = "Install the app";
    pub const INSTALL_ANOTHER: &str = "Install on another account";
    pub const ACCOUNTS_HEADER: &str = "GitHub accounts with the app installed";
    pub const CONFIGURE: &str = "Configure";
    pub const INSTALLATION_CAPTION: &str = "Repositories come from these accounts. Configure one \
        to grant more.";
    pub const MANAGE: &str = "Manage";
    pub const NO_REPOSITORIES: &str = "No repositories added yet.";

    /// `Connected as octocat`.
    pub fn connected_as(login: &str) -> String {
        format!("Connected as {login}")
    }

    /// The account label when GitHub gave no login (lower-case, ×4).
    pub fn installation_fallback(installation_id: i64) -> String {
        format!("installation {installation_id}")
    }

    /// The Configure link's tooltip.
    pub fn configure_title(label: &str) -> String {
        format!("Configure which repositories {label} grants on GitHub")
    }

    pub fn suspended_line(labels: &[String]) -> String {
        format!(
            "GitHub suspended the Exponential app for {}. Unsuspend it on GitHub.",
            labels.join(", ")
        )
    }

    // --- C. Add-repository picker --------------------------------------------
    pub const PICKER_TITLE: &str = "Add repository";
    pub const PICKER_LOADING: &str = "Loading your GitHub repositories\u{2026}";
    pub const PICKER_NOT_CONFIGURED: &str = "GitHub isn\u{2019}t configured on this server, so \
        repositories can\u{2019}t be added.";
    pub const PICKER_NOT_LINKED: &str = "Connect your GitHub account to pick a repository. \
        You\u{2019}ll come right back here.";
    pub const PICKER_NOT_INSTALLED: &str = "Install the Exponential app on the GitHub account \
        that owns the repository. You\u{2019}ll come right back here.";
    pub const PICKER_CONNECTED_CHECK: &str = "I\u{2019}ve done that";
    /// A suspended installation with no login in the picker banner.
    pub const SUSPENDED_FALLBACK: &str = "a connected account";
    pub const PICKER_RECONNECT_BANNER: &str = "Your GitHub connection expired. Reconnect to list \
        your repositories.";
    pub const SEARCH_PLACEHOLDER: &str = "Search repositories\u{2026}";
    pub const NO_MATCH: &str = "No repositories found.";
    pub const NONE_PUSHABLE: &str = "The app is installed, but none of its repositories lets you \
        push. Grant one on GitHub, then refresh.";
    pub const FOOTER_EXPLAIN: &str = "Only repositories you can push to, on accounts where the \
        app is installed, appear here. Missing one? Grant it on GitHub, then refresh.";
    pub const CAP_NOTE: &str = "Showing the first 500 repositories per account \u{2014} use the \
        field below for the rest.";
    pub const REFRESH: &str = "Refresh";
    pub const LOOKUP_PLACEHOLDER: &str = "owner/name";
    /// The by-name field's accessibility label on web/iOS/Android; gpui's
    /// `Input` exposes no label slot yet, so desktop only mirrors it.
    #[allow(dead_code)]
    pub const LOOKUP_A11Y: &str = "Add repository by name";
    pub const LOOK_UP: &str = "Look up";
    pub const ADD_FORBIDDEN: &str = "GitHub says you can\u{2019}t push to this repository, or \
        your connection expired. Reconnect GitHub and try again.";
    /// Billing is web-only (§4.9): the desktop's pointer where the web links
    /// to its billing page.
    pub const UPGRADE_ON_THE_WEB: &str = "Upgrade on the web";

    pub fn picker_suspended_banner(labels: &[String]) -> String {
        format!(
            "GitHub suspended the Exponential app for {}. Its repositories can\u{2019}t be added \
             until you unsuspend it on GitHub.",
            labels.join(", ")
        )
    }

    // --- Desktop-only lines around the in-app link hop ------------------------
    /// The hop needs a signed-in account.
    pub const CONNECT_NOT_SIGNED_IN: &str = "Sign in to connect GitHub.";
    /// The server refused the link ticket and said nothing usable.
    pub const CONNECT_START_FAILED: &str = "Couldn\u{2019}t start connecting GitHub. Try again.";
    /// Every browser opener failed — the start URL is on the clipboard.
    pub const CONNECT_OPENER_FAILED: &str = "Couldn\u{2019}t open your browser. The connect link \
        was copied \u{2014} paste it into a browser to continue.";
    /// The `exponential://github-connected?error=…` hand-back (older
    /// servers only; the guided page itself shows SLOP-7's failures).
    pub const CONNECT_FAILED: &str = "Something went wrong while connecting GitHub. Please try \
        again.";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SLOP-7: the copy is byte-identical ×4 — these literals mirror web
    /// `github-connect-copy.ts` (its test locks the same keys); change all
    /// four clients together.
    #[test]
    fn copy_is_locked() {
        assert_eq!(copy::installation_fallback(42), "installation 42");
        assert_eq!(copy::STATUS_FAILED, "Couldn’t reach GitHub connect state.");
        assert_eq!(copy::NOT_CONFIGURED, "GitHub isn’t configured on this server.");
        assert_eq!(copy::NOT_LINKED, "No GitHub account connected");
        assert_eq!(copy::connected_as("octocat"), "Connected as octocat");
        assert_eq!(copy::RECONNECT_NEEDED, "Your GitHub connection expired.");
        assert_eq!(
            copy::DISCONNECT_BODY,
            "This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect."
        );
        assert_eq!(
            copy::NOT_INSTALLED,
            "The Exponential app isn’t installed on any of your GitHub accounts yet."
        );
        assert_eq!(copy::ACCOUNTS_HEADER, "GitHub accounts with the app installed");
        assert_eq!(
            copy::suspended_line(&["acme".to_string(), "octocat".to_string()]),
            "GitHub suspended the Exponential app for acme, octocat. Unsuspend it on GitHub."
        );
        assert_eq!(copy::NO_REPOSITORIES, "No repositories added yet.");
        assert_eq!(
            copy::configure_title("acme"),
            "Configure which repositories acme grants on GitHub"
        );
        assert_eq!(
            copy::INSTALLATION_CAPTION,
            "Repositories come from these accounts. Configure one to grant more."
        );

        assert_eq!(copy::PICKER_LOADING, "Loading your GitHub repositories…");
        assert_eq!(
            copy::PICKER_NOT_CONFIGURED,
            "GitHub isn’t configured on this server, so repositories can’t be added."
        );
        assert_eq!(
            copy::PICKER_NOT_LINKED,
            "Connect your GitHub account to pick a repository. You’ll come right back here."
        );
        assert_eq!(
            copy::PICKER_NOT_INSTALLED,
            "Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here."
        );
        assert_eq!(copy::PICKER_CONNECTED_CHECK, "I’ve done that");
        assert_eq!(
            copy::picker_suspended_banner(&[copy::SUSPENDED_FALLBACK.to_string()]),
            "GitHub suspended the Exponential app for a connected account. Its repositories can’t be added until you unsuspend it on GitHub."
        );
        assert_eq!(
            copy::PICKER_RECONNECT_BANNER,
            "Your GitHub connection expired. Reconnect to list your repositories."
        );
        assert_eq!(
            copy::NONE_PUSHABLE,
            "The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh."
        );
        assert_eq!(
            copy::FOOTER_EXPLAIN,
            "Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh."
        );
        assert_eq!(
            copy::CAP_NOTE,
            "Showing the first 500 repositories per account — use the field below for the rest."
        );
        assert_eq!(copy::LOOKUP_A11Y, "Add repository by name");
        assert_eq!(
            copy::ADD_FORBIDDEN,
            "GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again."
        );
        assert_eq!(copy::RECONNECT_GITHUB, "Reconnect GitHub");
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
            "exponential://oauth-return?linked=github",
            "exponential://invite/abc",
            "exponential://github-connectedish?error=x",
            "https://app.exponential.at/github-connected",
        ] {
            assert_eq!(parse_github_connected_deep_link(url), None, "{url}");
        }
    }

    /// SLOP-7: the three prerequisites in their fixed order, then ready; a
    /// pre-SLOP-7 server (no `linked`) reads as not linked, whose fix is the
    /// connect hop either way.
    #[test]
    fn prerequisites_follow_the_one_flow_order() {
        let status = |json: &str| serde_json::from_str::<GithubStatus>(json).unwrap();
        assert_eq!(
            status(r#"{"configured":false}"#).prerequisite(),
            Prerequisite::NotConfigured
        );
        assert_eq!(
            status(r#"{"configured":true,"installed":true,"installations":[{"installationId":7}]}"#)
                .prerequisite(),
            Prerequisite::NotLinked
        );
        assert_eq!(
            status(r#"{"configured":true,"linked":true,"needsReconnect":true,"installed":true}"#)
                .prerequisite(),
            Prerequisite::Expired
        );
        assert_eq!(
            status(r#"{"configured":true,"linked":true,"login":"octocat","installed":false}"#)
                .prerequisite(),
            Prerequisite::NotInstalled
        );
        let ready = status(
            r#"{"configured":true,"linked":true,"login":"octocat","installed":true,"installations":[{"installationId":7,"accountLogin":"acme","accountType":"Organization","manageUrl":"https://github.com/organizations/acme/settings/installations/7","needsReauth":false,"stale":false,"suspended":true}]}"#,
        );
        assert_eq!(ready.prerequisite(), Prerequisite::Ready);
        assert_eq!(ready.login.as_deref(), Some("octocat"));
        assert!(ready.installations[0].is_organization());
        assert_eq!(ready.suspended_labels(), vec!["acme".to_string()]);
    }

    /// The repos result shares the connection fields and the picker's
    /// suspended labels fall back to "a connected account".
    #[test]
    fn repos_result_decodes_the_connection_and_the_rows() {
        let result: GithubReposResult = serde_json::from_str(
            r#"{"configured":true,"connectConfigured":true,"linked":true,"needsReconnect":false,"login":"octocat","installed":true,"installUrl":"https://github.com/apps/exp/installations/new","connectUrl":null,"repos":[{"fullName":"acme/web","private":true,"defaultBranch":"main","installationId":7}],"hasMore":false,"installations":[{"installationId":7,"accountLogin":null,"suspended":true}]}"#,
        )
        .unwrap();
        assert_eq!(result.prerequisite(), Prerequisite::Ready);
        assert_eq!(result.repos[0].full_name, "acme/web");
        assert!(result.repos[0].private);
        assert_eq!(
            result.suspended_picker_labels(),
            vec!["a connected account".to_string()]
        );
        assert_eq!(result.installations[0].label(), "installation 7");
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
}
