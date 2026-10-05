//! OAuth browser round-trip (masterplan-v3 §5.7, wired by the §4.2 login
//! view).
//!
//! Flow: [`start`] records the pending instance URL + PKCE verifier and opens
//! the system browser (through the `api::opener` chain — never a raw
//! `xdg-open`) at `/api/mobile-oauth-start?...&code_challenge=…`. The server
//! runs the OAuth dance and redirects to `/api/mobile-oauth-return`, which
//! deep-links back as `exponential://oauth-return?code=…#code=…` — a
//! single-use short-TTL code, NOT the session token (REV-13: xdg/HKCU scheme
//! dispatch is hijackable by any other registered handler, so the deep link
//! must carry nothing redeemable without in-app state). The app shell's
//! `on_open_urls` channel delivers that URL to [`handle_open_urls`], which
//! exchanges the code + held verifier for the session token over TLS
//! (`POST /api/mobile-oauth-exchange`) and adopts it exactly like a password
//! sign-in. Legacy pre-PKCE servers (self-hosted lag) still deep-link
//! `#token=<session-token>`; that path is kept for compatibility. A FAILING
//! hop deep-links `?error=<reason>#error=<reason>` (REV2-53) — routed to
//! [`crate::login::report_oauth_failure`] with the same copy iOS and Android
//! show, so a failure ends the attempt instead of stranding the login button
//! on "Waiting for your browser…".
//!
//! The v3-era `127.0.0.1` loopback fallback was dropped (its server half — a
//! `redirect=` param on `/api/mobile-oauth-return` — is not scheduled by any
//! live plan): unpackaged dev builds without the `exponential://` scheme
//! registration rely on the copyable-URL degradation instead.

use std::sync::Arc;

use api::login::OAuthCallback;
use gpui::{App, Global, SharedString};
use sync::{SessionPhase, Store};

use crate::session::{connect_account, AuthContext};

/// The in-flight OAuth attempt (one at a time — starting a new one replaces
/// the old, whose late callback would then be adopted against the newer
/// instance URL; both point at the URL the user last chose). The PKCE
/// verifier lives here in memory only (never persisted) and pairs with the
/// code_challenge the start URL carried (REV-13).
#[derive(Default)]
struct PendingOAuth {
    instance_url: Option<String>,
    verifier: Option<String>,
    /// EXP-1126: a LINK-mode attempt (Settings → Account → Sign-in methods)
    /// is in flight. Its callbacks (`?linked=` / `?error=`) route to
    /// [`SignInLinkOutcome`] instead of the login surface.
    link: Option<PendingLink>,
}

impl Global for PendingOAuth {}

/// The account + provider a link-mode attempt attaches.
#[derive(Clone, Debug)]
struct PendingLink {
    account_id: String,
    provider_id: String,
}

/// EXP-1126: the last link-mode outcome. The completion runs on the `App`
/// (the settings window may be gone by the time the browser hands back), so
/// the Sign-in methods section adopts it through `observe_global`, keyed on
/// `seq` so the same outcome is never shown twice. `result`: `Ok(providerId)`
/// on success, `Err(copy)` with the web's `oauthLinkErrorMessage` line.
#[derive(Default)]
pub(crate) struct SignInLinkOutcome {
    pub seq: u64,
    pub account_id: Option<String>,
    /// The provider the attempt was for (SLOP-25: a failure names it too, so
    /// the GitHub connection surfaces can tell their hop from a sign-in
    /// method's). `None` only for a hand-back no attempt in this process
    /// started.
    pub provider_id: Option<String>,
    pub result: Option<Result<String, SharedString>>,
}

impl Global for SignInLinkOutcome {}

/// Open the browser for an OAuth start URL. `verifier` is the PKCE verifier
/// whose challenge is baked into `start_url`. `Err(url)` = the ENTIRE opener
/// chain failed: the caller must surface the URL copyably — a broken
/// opener degrades to copy-paste, never a dead end.
pub(crate) fn start(
    instance_url: String,
    start_url: String,
    verifier: String,
    cx: &mut App,
) -> Result<(), String> {
    let pending = cx.default_global::<PendingOAuth>();
    pending.instance_url = Some(instance_url);
    pending.verifier = Some(verifier);
    // A sign-in attempt replaces any link attempt still pending.
    pending.link = None;
    open_start_url(start_url)
}

/// EXP-1126: open the browser handoff in LINK mode
/// (`api::login::link_start_url`) to attach `provider_id` to the signed-in
/// `account_id`. The verifier is held like a sign-in's (the route requires
/// PKCE), but `instance_url` stays `None`, so a stray `code=` callback can
/// never sign anybody in off this attempt. Same `Err(url)` degradation as
/// [`start`].
pub(crate) fn start_link(
    account_id: String,
    provider_id: String,
    start_url: String,
    verifier: String,
    cx: &mut App,
) -> Result<(), String> {
    let pending = cx.default_global::<PendingOAuth>();
    pending.instance_url = None;
    pending.verifier = Some(verifier);
    pending.link = Some(PendingLink {
        account_id,
        provider_id,
    });
    open_start_url(start_url)
}

fn open_start_url(start_url: String) -> Result<(), String> {
    match api::opener::open_in_browser(&start_url) {
        Ok(()) => Ok(()),
        Err(err) => {
            log::warn!("[ui] oauth: browser open failed: {err}");
            Err(start_url)
        }
    }
}

/// EXP-1126: end a link-mode attempt — clear it, publish the outcome.
fn finish_link(result: Result<String, SharedString>, cx: &mut App) {
    let pending = cx.default_global::<PendingOAuth>();
    let link = pending.link.take();
    pending.verifier = None;
    if let Some(link) = &link {
        log::info!("[ui] oauth: link attempt for {} ended", link.provider_id);
    }
    let outcome = cx.default_global::<SignInLinkOutcome>();
    outcome.seq += 1;
    outcome.provider_id = link
        .as_ref()
        .map(|link| link.provider_id.clone())
        .or_else(|| result.as_ref().ok().cloned());
    outcome.account_id = link.map(|link| link.account_id);
    outcome.result = Some(result);
}

/// The `on_open_urls` sink (call from the app shell's foreground drain).
/// Routes OAuth callbacks, the EXP-368 `exponential://github-connected`
/// hand-back, the §4.2 `exponential://invite/<token>` deep link and the
/// EXP-4 `exponential://issue/<IDENTIFIER>` deep link; anything else is
/// ignored.
pub fn handle_open_urls(urls: Vec<String>, cx: &mut App) {
    for url in urls {
        // EXP-368: checked before parse_oauth_callback — the host-agnostic
        // param scan would otherwise adopt `github-connected?error=…` as an
        // OAuth Error callback and silently eat it (complete() ignores
        // callbacks while signed in). Belt to the api-crate scheme-host
        // guard's suspenders.
        if let Some(outcome) = crate::github_connect::parse_github_connected_deep_link(&url) {
            crate::github_connect::report_github_connected(outcome, cx);
            continue;
        }
        if let Some(callback) = api::login::parse_oauth_callback(&url) {
            complete(callback, cx);
            continue;
        }
        if let Some(token) = crate::join_team::parse_invite_deep_link(&url) {
            // Open the accept card directly (§4.2 path 1). Requires a signed
            // in session — the dialog itself renders the sign-in nudge.
            if let Some(window) = crate::navigation::active_or_primary_window(cx) {
                let _ = window.update(cx, |_, window, cx| {
                    crate::join_team::open(window, cx, Some(token));
                });
            }
            continue;
        }
        if let Some(identifier) = parse_issue_deep_link(&url) {
            open_issue_deep_link(&identifier, cx);
            continue;
        }
        log::info!("[ui] open-urls: unhandled URL {url}");
    }
}

/// `exponential://issue/<IDENTIFIER>` → `Some(identifier)` (e.g. `EXP-42` —
/// the EXP-4 deep-link form; mirror of
/// [`crate::join_team::parse_invite_deep_link`]).
pub(crate) fn parse_issue_deep_link(url: &str) -> Option<String> {
    let prefix = format!("{}://issue/", api::login::OAUTH_CALLBACK_SCHEME);
    let rest = url.strip_prefix(prefix.as_str())?;
    let identifier = rest
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    (!identifier.is_empty()).then(|| identifier.to_string())
}

/// Resolve an issue deep link and navigate to its detail. Unlike the
/// team-scoped `#IDENT` pill path
/// (`description_editor::open_issue_by_identifier`), a deep link carries only
/// the identifier — so it resolves case-insensitively across ALL synced
/// issues. Unknown / not-yet-synced identifiers log and no-op, consistent
/// with unhandled deep links.
fn open_issue_deep_link(identifier: &str, cx: &mut App) {
    let issue = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .iter()
        .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
        .map(|issue| (issue.id.clone(), issue.board_id.clone()));
    let Some((issue_id, board_id)) = issue else {
        log::info!("[ui] open-urls: issue deep link {identifier} matches no synced issue — ignored");
        return;
    };
    if let Some(window) = crate::navigation::active_or_primary_window(cx) {
        let _ = window.update(cx, |_, window, cx| {
            // EXP-510: land scoped on the issue's board; EXP-1192: with no
            // list beside it (a deep link was opened from nowhere on screen).
            crate::navigation::open_issue_scoped(window, cx, issue_id, board_id, None);
        });
    }
}

/// The login-surface message when the callback lands but the exchange /
/// session resolve fails (expired one-time code, offline, 401) — same line as
/// iOS `LoginViewModel` and Android's `reportLoginError`; the cause stays in
/// the log.
const COMPLETION_FAILED: &str = "Couldn't verify your sign-in. Please try again.";

/// Human copy for the `error` reason slug the server deep-links back on a
/// failed OAuth hop (REV2-53 — `exponential://oauth-return?error=…`). Mirrors
/// the web `oauthErrorMessage` (apps/web/src/lib/deep-link.ts), iOS
/// `LoginViewModel.oauthErrorMessage` and Android's `oauthErrorMessage` — the
/// slugs are already clamped server-side by `normalizeOauthErrorReason`, and an
/// unknown one gets the generic line so a new server slug is never shown raw.
fn oauth_error_message(reason: &str) -> &'static str {
    match reason {
        "access_denied" => "Sign-in was cancelled.",
        "state_missing" | "state_invalid" | "state_mismatch" | "state_not_found"
        | "please_restart_the_process" => "That sign-in link expired. Please try again.",
        "no_session" | "session_cookie_missing" => {
            "Sign-in didn't complete on the server. Please try again."
        }
        _ => "Couldn't complete sign-in. Please try again.",
    }
}

/// Adopt an OAuth callback: for a PKCE code, first redeem it via
/// `POST /api/mobile-oauth-exchange` with the held verifier (REV-13); then
/// validate the token via `get-session`, persist the account, connect sync —
/// the same path as a password sign-in (§5.7 step 4).
fn complete(callback: OAuthCallback, cx: &mut App) {
    // EXP-1126 link mode, BEFORE the signed-in guard below: linking happens
    // while signed in by definition. A `linked=` hand-back carries no
    // credential (it only says "refresh"), and a failure while a link is
    // pending belongs to the Sign-in methods section, not the login surface.
    let link_pending = cx
        .try_global::<PendingOAuth>()
        .is_some_and(|pending| pending.link.is_some());
    match &callback {
        OAuthCallback::Linked(provider_id) => {
            log::info!("[ui] oauth: provider {provider_id} linked");
            finish_link(Ok(provider_id.clone()), cx);
            return;
        }
        OAuthCallback::Error(reason) if link_pending => {
            log::warn!("[ui] oauth: server reported link failure ({reason})");
            finish_link(
                Err(api::login::link_error_message(reason).into()),
                cx,
            );
            return;
        }
        _ => {}
    }
    let store = Store::global(cx).clone();
    if matches!(store.session(cx), SessionPhase::Synced { .. }) {
        log::info!("[ui] oauth: callback while already signed in — ignored");
        return;
    }
    // The server declared the hop failed (REV2-53). Surface it and end the
    // attempt here — there is nothing to redeem, and falling through would
    // leave the login button stuck on "Waiting for your browser…" forever.
    // Handled BEFORE the pending-attempt checks: a failure that arrives
    // without in-process state must still un-stick the surface, exactly like
    // iOS clearing `pendingPkce` and Android consuming the verifier.
    if let OAuthCallback::Error(reason) = &callback {
        log::warn!("[ui] oauth: server reported sign-in failure ({reason})");
        let pending = cx.default_global::<PendingOAuth>();
        pending.instance_url = None;
        pending.verifier = None;
        crate::login::report_oauth_failure(oauth_error_message(reason), cx);
        // Only our own in-flight attempt may be aborted — a password sign-in
        // that raced the callback owns the phase now.
        if store.session(cx) == SessionPhase::SigningIn {
            store.abort_sign_in(cx);
        }
        return;
    }
    let Some(instance_url) = cx
        .try_global::<PendingOAuth>()
        .and_then(|pending| pending.instance_url.clone())
    else {
        log::warn!("[ui] oauth: callback with no pending attempt — ignored");
        return;
    };
    let verifier = cx
        .try_global::<PendingOAuth>()
        .and_then(|pending| pending.verifier.clone());
    if matches!(callback, OAuthCallback::Code(_)) && verifier.is_none() {
        // A code arrived without an attempt this process started (out-of-band
        // or replay) — nothing to redeem it with; mirror the
        // no-pending-attempt branch above.
        log::warn!("[ui] oauth: code callback with no held PKCE verifier — ignored");
        return;
    }

    let auth = AuthContext::global(cx).clone();
    store.begin_sign_in(cx);

    cx.spawn(async move |cx| {
        let client = Arc::clone(&auth.client);
        let (server_bg, callback_bg, verifier_bg) =
            (instance_url.clone(), callback, verifier.clone());
        // Result<(token, session user), ApiError> — the token comes straight
        // from a legacy callback, or from the code exchange (checked above:
        // a Code always has a verifier here).
        let result = cx
            .background_executor()
            .spawn(async move {
                let token = match callback_bg {
                    OAuthCallback::Token(token) => token,
                    OAuthCallback::Code(code) => client.exchange_oauth_code(
                        &server_bg,
                        &code,
                        verifier_bg.as_deref().unwrap_or_default(),
                    )?,
                    // Unreachable — `complete` returns on the error arm above;
                    // spelled out rather than panicked so a future caller can
                    // never turn a failure callback into a crash.
                    OAuthCallback::Error(reason) => {
                        return Err(api::ApiError::Decode(format!(
                            "oauth error callback: {reason}"
                        )))
                    }
                    // Unreachable too — link hand-backs return above.
                    OAuthCallback::Linked(provider) => {
                        return Err(api::ApiError::Decode(format!(
                            "oauth linked callback: {provider}"
                        )))
                    }
                };
                let user = client.fetch_session(&server_bg, &token)?;
                Ok::<_, api::ApiError>((token, user))
            })
            .await;

        cx.update(|cx| {
            if Store::global(cx).session(cx) != SessionPhase::SigningIn {
                return; // superseded (e.g. password sign-in raced the callback)
            }
            let store = Store::global(cx).clone();
            match result {
                Ok((token, Some(user))) => match auth.auth.sign_in(&instance_url, &token, &user)
                {
                    Ok(account) => {
                        let pending = cx.default_global::<PendingOAuth>();
                        pending.instance_url = None;
                        pending.verifier = None;
                        connect_account(&account, cx);
                    }
                    Err(err) => {
                        log::warn!("[ui] oauth: storing the session failed: {err}");
                        crate::login::report_oauth_failure(
                            format!("Could not store the session: {err}"),
                            cx,
                        );
                        store.abort_sign_in(cx);
                    }
                },
                Ok((_, None)) => {
                    log::warn!("[ui] oauth: callback token does not resolve — login stays");
                    crate::login::report_oauth_failure(COMPLETION_FAILED, cx);
                    store.abort_sign_in(cx);
                }
                Err(err) => {
                    log::warn!("[ui] oauth: sign-in completion failed: {err}");
                    crate::login::report_oauth_failure(COMPLETION_FAILED, cx);
                    store.abort_sign_in(cx);
                }
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_deep_link_parses_identifier() {
        assert_eq!(
            parse_issue_deep_link("exponential://issue/EXP-42"),
            Some("EXP-42".to_string())
        );
        assert_eq!(
            parse_issue_deep_link("exponential://issue/EXP-42/"),
            Some("EXP-42".to_string())
        );
        assert_eq!(
            parse_issue_deep_link("exponential://issue/EXP-42?utm=x#frag"),
            Some("EXP-42".to_string())
        );
        assert_eq!(parse_issue_deep_link("exponential://issue/"), None);
        assert_eq!(parse_issue_deep_link("exponential://invite/abc123"), None);
        assert_eq!(parse_issue_deep_link("exponential://oauth-return#token=t"), None);
        assert_eq!(parse_issue_deep_link("https://x/issue/EXP-42"), None);
    }

    #[test]
    fn oauth_error_copy_matches_the_other_clients() {
        // Byte-parity with iOS `LoginViewModel.oauthErrorMessage` and Android's
        // `oauthErrorMessage` — the same slug must read the same on every
        // client (the web's own copy differs only on the expired-link line).
        assert_eq!(oauth_error_message("access_denied"), "Sign-in was cancelled.");
        for reason in [
            "state_missing",
            "state_invalid",
            "state_mismatch",
            "state_not_found",
            "please_restart_the_process",
        ] {
            assert_eq!(
                oauth_error_message(reason),
                "That sign-in link expired. Please try again.",
                "{reason}"
            );
        }
        for reason in ["no_session", "session_cookie_missing"] {
            assert_eq!(
                oauth_error_message(reason),
                "Sign-in didn't complete on the server. Please try again.",
                "{reason}"
            );
        }
    }

    #[test]
    fn oauth_link_error_copy_matches_the_web() {
        // EXP-1126: byte-parity with the web `oauthLinkErrorMessage`
        // (apps/web/src/lib/deep-link.ts) — the link surface's copy, distinct
        // from the sign-in lines above.
        let Some(OAuthCallback::Error(reason)) = api::login::parse_oauth_callback(
            "exponential://oauth-return?error=account_already_linked_to_different_user",
        ) else {
            panic!("link failure deep link did not parse as an error callback");
        };
        assert_eq!(
            api::login::link_error_message(&reason),
            "That account is already linked to a different user."
        );
        assert_eq!(
            api::login::link_error_message("access_denied"),
            "Linking was cancelled."
        );
        assert_eq!(
            api::login::link_error_message("link_ticket_invalid"),
            "That link request expired. Please try again."
        );
        assert_eq!(
            api::login::link_error_message("unable_to_link_account"),
            "Couldn't link that account. Please try again."
        );
        assert_eq!(
            api::login::parse_oauth_callback("exponential://oauth-return?linked=google#linked=google"),
            Some(OAuthCallback::Linked("google".to_string()))
        );
    }

    #[test]
    fn unknown_oauth_error_slugs_get_the_generic_line() {
        // A slug a newer server invents must never reach the user raw.
        for reason in ["oauth_failed", "server_exploded", "", "ACCESS_DENIED"] {
            assert_eq!(
                oauth_error_message(reason),
                "Couldn't complete sign-in. Please try again.",
                "{reason}"
            );
        }
    }

    #[test]
    fn error_callbacks_route_to_the_failure_copy() {
        // The seam the desktop was missing: the server's failure deep link
        // must parse as `Error` and map to user-facing copy, not fall through
        // to the unhandled-URL log.
        let Some(OAuthCallback::Error(reason)) = api::login::parse_oauth_callback(
            "exponential://oauth-return?error=access_denied#error=access_denied",
        ) else {
            panic!("failure deep link did not parse as an OAuth error callback");
        };
        assert_eq!(oauth_error_message(&reason), "Sign-in was cancelled.");
    }
}
