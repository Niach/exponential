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
//! `#token=<session-token>`; that path is kept for compatibility.
//!
//! RESIDUAL (§5.7 fallback): the `127.0.0.1` loopback capture needs a NEW
//! server-side `redirect=` param on `/api/mobile-oauth-return`
//! (127.0.0.1-bound allowlist, `?token=` query) that has not landed in
//! `apps/web` — until it does, unpackaged dev builds without the `exponential://`
//! scheme registration rely on the copyable-URL degradation, and
//! `api::login::LoopbackListener` stays the ready client half.

use std::sync::Arc;

use api::login::OAuthCallback;
use gpui::{App, Global};
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
}

impl Global for PendingOAuth {}

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
    match api::opener::open_in_browser(&start_url) {
        Ok(()) => Ok(()),
        Err(err) => {
            log::warn!("[ui] oauth: browser open failed: {err}");
            Err(start_url)
        }
    }
}

/// The `on_open_urls` sink (call from the app shell's foreground drain).
/// Routes OAuth callbacks, the §4.2 `exponential://invite/<token>` deep link and the
/// EXP-4 `exponential://issue/<IDENTIFIER>` deep link; anything else is ignored.
pub fn handle_open_urls(urls: Vec<String>, cx: &mut App) {
    for url in urls {
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
    let issue_id = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .iter()
        .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
        .map(|issue| issue.id.clone());
    let Some(issue_id) = issue_id else {
        log::info!("[ui] open-urls: issue deep link {identifier} matches no synced issue — ignored");
        return;
    };
    if let Some(window) = crate::navigation::active_or_primary_window(cx) {
        let _ = window.update(cx, |_, window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                crate::navigation::Screen::IssueDetail { issue_id },
            );
        });
    }
}

/// Adopt an OAuth callback: for a PKCE code, first redeem it via
/// `POST /api/mobile-oauth-exchange` with the held verifier (REV-13); then
/// validate the token via `get-session`, persist the account, connect sync —
/// the same path as a password sign-in (§5.7 step 4).
fn complete(callback: OAuthCallback, cx: &mut App) {
    let store = Store::global(cx).clone();
    if matches!(store.session(cx), SessionPhase::Synced { .. }) {
        log::info!("[ui] oauth: callback while already signed in — ignored");
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
                        store.abort_sign_in(cx);
                    }
                },
                Ok((_, None)) => {
                    log::warn!("[ui] oauth: callback token does not resolve — login stays");
                    store.abort_sign_in(cx);
                }
                Err(err) => {
                    log::warn!("[ui] oauth: sign-in completion failed: {err}");
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
}
