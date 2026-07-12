//! Session wiring between the `api` auth layer and the `sync` store
//! (masterplan-v3 §5.7/§5.10) — the glue the login surface, the app-shell
//! bootstrap, and the sign-out action all share.
//!
//! The [`AuthContext`] global carries the process-wide auth handles (the
//! `AuthStore`, one blocking `AuthClient`, the data dir). It is installed by
//! the app shell at bootstrap, before any window opens.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Global};
use sync::{SessionPhase, Store};

/// Process-wide auth handles. Cheap to clone (Arcs + a path).
#[derive(Clone)]
pub struct AuthContext {
    /// The signed-in account set + session tokens (0600 file store).
    pub auth: Arc<api::AuthStore>,
    /// Blocking Better Auth client — drive it from a background task, never
    /// on the foreground.
    pub client: Arc<api::AuthClient>,
    /// `{data_dir}` of §5.4 — `accounts.json`, per-account sync DBs, token
    /// file fallback.
    pub data_dir: PathBuf,
}

impl Global for AuthContext {}

impl AuthContext {
    pub fn global(cx: &App) -> &AuthContext {
        cx.global::<AuthContext>()
    }
}

/// Start (or resume) syncing `account`: builds the [`sync::AccountSyncConfig`]
/// (per-account DB path + call-time token provider, §5.7) and flips the
/// session machine to `Synced` via [`Store::connect`]. Returns `false` (and
/// rolls the session back to `SignedOut`) when the local store cannot open.
pub fn connect_account(account: &api::Account, cx: &mut App) -> bool {
    let auth = AuthContext::global(cx).clone();
    let store = Store::global(cx).clone();
    let config = sync::AccountSyncConfig {
        account_id: account.id.clone(),
        base_url: account.instance_url.clone(),
        db_path: api::account_db_path(&auth.data_dir, &account.id),
        token: auth.auth.token_provider_fn(&account.id),
    };
    match store.connect(config, cx) {
        Ok(_) => {
            // §08 device presence: dial the steer control socket for this
            // account (no-op when steer is disabled/unconfigured).
            crate::steer_wiring::start_control_channel(account, cx);
            true
        }
        Err(err) => {
            eprintln!(
                "[exp-desktop] session: opening sync store for {} failed: {err}",
                account.id
            );
            store.abort_sign_in(cx);
            false
        }
    }
}

/// Sign the active (or auth-expired) account out: best-effort server-side
/// revocation on a background thread, local token deletion, pipeline stop,
/// collections cleared, session → `SignedOut` (§5.10 — the SQLite DB stays on
/// disk for offline resume).
pub fn sign_out_active(cx: &mut App) {
    let store = Store::global(cx).clone();
    let Some(account_id) = store.session(cx).account_id().map(String::from) else {
        return;
    };
    // §08: stop this account's steer control socket before tearing sync down.
    crate::steer_wiring::stop_control_channel(&account_id, cx);
    let auth = AuthContext::global(cx).clone();

    // Best-effort server-side revocation — local sign-out proceeds even when
    // this fails (offline sign-out is legal, §5.7).
    if let (Some(token), Some(account)) = (
        auth.auth.token(&account_id),
        auth.auth.account(&account_id),
    ) {
        let client = Arc::clone(&auth.client);
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = client.sign_out(&account.instance_url, &token) {
                    eprintln!("[exp-desktop] session: server-side sign-out failed: {err}");
                }
            })
            .detach();
    }

    auth.auth.sign_out(&account_id);
    store.sign_out(&account_id, cx);
}

/// The startup session bootstrap (app-shell wiring):
///
/// 1. **Dev override** — `EXP_DEV_SERVER` + `EXP_DEV_TOKEN` inject a session
///    for headless verification: the token is validated against
///    `/api/auth/get-session` on a background thread, then signed in and
///    connected exactly like a real login. **Dev-only** — never document for
///    users; a dead dev token lands on the login screen (the same
///    dead-token routing the runtime gate checks).
/// 2. Otherwise resume the first persisted signed-in account (warm start —
///    the pipeline resumes from the persisted cursor, §5.11 gate 3).
pub fn bootstrap(cx: &mut App) {
    // EXP-43 zero-workspace self-heal: observes the workspaces collection for
    // the "synced but no workspace" dead end (last workspace deleted; personal
    // bootstrap failed). Anchored here because the `Store` global exists by
    // now (ui::init runs before it is set).
    crate::workspace_heal::install(cx);

    let dev_server = std::env::var("EXP_DEV_SERVER").ok();
    let dev_token = std::env::var("EXP_DEV_TOKEN").ok();
    if let (Some(server), Some(token)) = (dev_server, dev_token) {
        dev_inject_session(server, token, cx);
        return;
    }

    let auth = AuthContext::global(cx).clone();
    if let Some(account) = auth.auth.signed_in_accounts().into_iter().next() {
        connect_account(&account, cx);
    }
}

/// DEV-ONLY (§11.4 headless verification). Validates the injected token via
/// `get-session`, then runs the normal sign-in + connect path.
fn dev_inject_session(server: String, token: String, cx: &mut App) {
    let auth = AuthContext::global(cx).clone();
    let store = Store::global(cx).clone();
    store.begin_sign_in(cx);
    eprintln!("[exp-desktop] session: DEV bootstrap via EXP_DEV_SERVER={server}");

    cx.spawn(async move |cx| {
        let client = Arc::clone(&auth.client);
        let (server_bg, token_bg) = (server.clone(), token.clone());
        let result = cx
            .background_executor()
            .spawn(async move { client.fetch_session(&server_bg, &token_bg) })
            .await;

        cx.update(|cx| {
            // Guard against a state change while the validation was in
            // flight (e.g. the user already signed in via the form).
            if Store::global(cx).session(cx) != SessionPhase::SigningIn {
                return;
            }
            let store = Store::global(cx).clone();
            match result {
                Ok(Some(user)) => match auth.auth.sign_in(&server, &token, &user) {
                    Ok(account) => {
                        connect_account(&account, cx);
                    }
                    Err(err) => {
                        eprintln!("[exp-desktop] session: dev sign-in failed: {err}");
                        store.abort_sign_in(cx);
                    }
                },
                Ok(None) => {
                    // Dead dev token → login screen, never an empty board.
                    eprintln!("[exp-desktop] session: EXP_DEV_TOKEN does not resolve — login");
                    store.abort_sign_in(cx);
                }
                Err(err) => {
                    eprintln!("[exp-desktop] session: dev session validation failed: {err}");
                    store.abort_sign_in(cx);
                }
            }
        });
    })
    .detach();
}
