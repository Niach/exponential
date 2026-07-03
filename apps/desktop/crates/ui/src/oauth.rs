//! OAuth browser round-trip (masterplan-v3 §5.7, wired by the §4.2 login
//! view — EXP-5).
//!
//! Flow: [`start`] records the pending instance URL and opens the system
//! browser (through the `api::opener` chain — never a raw `xdg-open`) at
//! `/api/mobile-oauth-start?...`. The server runs the OAuth dance and
//! redirects to `/api/mobile-oauth-return`, which deep-links back as
//! `exp://oauth-return#token=<session-token>`. The app shell's
//! `on_open_urls` channel delivers that URL to [`handle_open_urls`], which
//! parses the token app-locally (it lives in the URL *fragment*) and adopts
//! it exactly like a password sign-in.
//!
//! RESIDUAL (§5.7 fallback): the `127.0.0.1` loopback capture needs a NEW
//! server-side `redirect=` param on `/api/mobile-oauth-return`
//! (127.0.0.1-bound allowlist, `?token=` query) that has not landed in
//! `apps/web` — until it does, unpackaged dev builds without the `exp://`
//! scheme registration rely on the copyable-URL degradation, and
//! `api::login::LoopbackListener` stays the ready client half.

use std::sync::Arc;

use gpui::{App, Global};
use sync::{SessionPhase, Store};

use crate::session::{connect_account, AuthContext};

/// The in-flight OAuth attempt (one at a time — starting a new one replaces
/// the old, whose late callback would then be adopted against the newer
/// instance URL; both point at the URL the user last chose).
#[derive(Default)]
struct PendingOAuth {
    instance_url: Option<String>,
}

impl Global for PendingOAuth {}

/// Open the browser for an OAuth start URL. `Err(url)` = the ENTIRE opener
/// chain failed (EXP-5): the caller must surface the URL copyably — a broken
/// opener degrades to copy-paste, never a dead end.
pub(crate) fn start(instance_url: String, start_url: String, cx: &mut App) -> Result<(), String> {
    cx.default_global::<PendingOAuth>().instance_url = Some(instance_url);
    match api::opener::open_in_browser(&start_url) {
        Ok(()) => Ok(()),
        Err(err) => {
            log::warn!("[ui] oauth: browser open failed: {err}");
            Err(start_url)
        }
    }
}

/// The `on_open_urls` sink (call from the app shell's foreground drain).
/// Routes OAuth callbacks and the §4.2 `exp://invite/<token>` deep link;
/// anything else is ignored.
pub fn handle_open_urls(urls: Vec<String>, cx: &mut App) {
    for url in urls {
        if let Some(token) = api::login::parse_oauth_callback(&url) {
            complete(token, cx);
            continue;
        }
        if let Some(token) = crate::join_workspace::parse_invite_deep_link(&url) {
            // Open the accept card directly (§4.2 path 1). Requires a signed
            // in session — the dialog itself renders the sign-in nudge.
            if let Some(window) = crate::navigation::active_or_primary_window(cx) {
                let _ = window.update(cx, |_, window, cx| {
                    crate::join_workspace::open(window, cx, Some(token));
                });
            }
            continue;
        }
        log::info!("[ui] open-urls: unhandled URL {url}");
    }
}

/// Adopt an OAuth callback token: validate it via `get-session`, persist the
/// account, connect sync — the same path as a password sign-in (§5.7 step 4).
fn complete(token: String, cx: &mut App) {
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

    let auth = AuthContext::global(cx).clone();
    store.begin_sign_in(cx);

    cx.spawn(async move |cx| {
        let client = Arc::clone(&auth.client);
        let (server_bg, token_bg) = (instance_url.clone(), token.clone());
        let result = cx
            .background_executor()
            .spawn(async move { client.fetch_session(&server_bg, &token_bg) })
            .await;

        cx.update(|cx| {
            if Store::global(cx).session(cx) != SessionPhase::SigningIn {
                return; // superseded (e.g. password sign-in raced the callback)
            }
            let store = Store::global(cx).clone();
            match result {
                Ok(Some(user)) => match auth.auth.sign_in(&instance_url, &token, &user) {
                    Ok(account) => {
                        cx.default_global::<PendingOAuth>().instance_url = None;
                        connect_account(&account, cx);
                    }
                    Err(err) => {
                        log::warn!("[ui] oauth: storing the session failed: {err}");
                        store.abort_sign_in(cx);
                    }
                },
                Ok(None) => {
                    log::warn!("[ui] oauth: callback token does not resolve — login stays");
                    store.abort_sign_in(cx);
                }
                Err(err) => {
                    log::warn!("[ui] oauth: session validation failed: {err}");
                    store.abort_sign_in(cx);
                }
            }
        });
    })
    .detach();
}
