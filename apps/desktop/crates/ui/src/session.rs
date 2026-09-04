//! Session wiring between the `api` auth layer and the `sync` store
//! (masterplan-v3 §5.7/§5.10) — the glue the login surface, the app-shell
//! bootstrap, and the sign-out action all share.
//!
//! The [`AuthContext`] global carries the process-wide auth handles (the
//! `AuthStore`, one blocking `AuthClient`, the data dir). It is installed by
//! the app shell at bootstrap, before any window opens.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Global};
use sync::{SessionPhase, Store};

use crate::coding_flow::CodingHub;

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

/// Build the EXP-104 upgrade-required hook the sync manager invokes on an HTTP
/// 426 (a stale build). The sync layer calls it from a shape thread, so — like
/// the auth layer's `unauthorized_handler_fn` — it must be `Send + Sync`;
/// unlike that handler, it has to reach a gpui global (`UpdateState`), which is
/// foreground-only. It bridges the gap with a flume channel drained on ONE
/// foreground task (the same pattern as the sync `ShapeDelta` drain), where it
/// flips the app into the blocking "Update required" state.
///
/// Call once at bootstrap (before `Store::open`), with the returned hook passed
/// into it.
pub fn upgrade_required_handler(cx: &mut App) -> sync::UpgradeRequiredFn {
    let (tx, rx) = flume::unbounded::<()>();
    cx.spawn(async move |cx| {
        while rx.recv_async().await.is_ok() {
            let _ = cx.update(|cx| crate::update::UpdateState::set_blocked(cx));
        }
    })
    .detach();
    Arc::new(move || {
        let _ = tx.send(());
    })
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
        // The desktop syncs all 20 shapes; only the CLI daemon subsets.
        shapes: None,
    };
    match store.connect(config, cx) {
        Ok(_) => {
            // §08 device presence: dial the steer control socket for this
            // account (no-op when steer is disabled/unconfigured).
            crate::steer_wiring::start_control_channel(account, cx);
            // EXP-481: the device-state sync loop (heartbeat/work pull/
            // inventory reports) rides beside the control socket.
            crate::device_sync::start_device_sync(account, cx);
            // EXP-530: the action-automation host (schedules + event
            // triggers bound to THIS device) rides the same lifecycle.
            crate::automation_host::start_automation_host(account, cx);
            // EXP-229: end the coding_sessions rows a crash / forced logout
            // stranded `running` — this is the single choke point every
            // sign-in path (warm start, dev inject, OAuth, login form) runs
            // through, so orphans heal on the next connect instead of
            // blocking "coding now" for the server sweep's 2h window.
            crate::session_registry::reconcile_stale_sessions(account, cx);
            // EXP-369: give the account a clock for its daily digest.
            claim_timezone(account, cx);
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

/// EXP-369 best-effort timezone claim: stamp the OS zone on the account the
/// first time one is seen (`onlyIfUnset` — an explicit pick in settings, or
/// on the web, always wins). The daily digest's send hour is read in it, so
/// an account that never sets one would fall back to UTC. Fire-and-forget on
/// the background executor: failures (offline, an older server without the
/// route) only mean the next sign-in tries again.
fn claim_timezone(account: &api::Account, cx: &mut App) {
    let timezone = match iana_time_zone::get_timezone() {
        Ok(timezone) => timezone,
        Err(err) => {
            log::warn!("[exp-desktop] session: no system timezone: {err}");
            return;
        }
    };
    let Some(auth) = cx.try_global::<AuthContext>() else {
        return;
    };
    let provider = auth.auth.token_provider(&account.id);
    let trpc = api::TrpcClient::new(&account.instance_url, provider);
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::users::users_set_timezone(&trpc, &timezone, true) {
                log::warn!("[exp-desktop] session: timezone claim failed: {err}");
            }
        })
        .detach();
}

/// Sign the active (or auth-expired) account out: best-effort server-side
/// revocation on a background thread, local token deletion, pipeline stop,
/// collections cleared, session → `SignedOut` (§5.10 — the SQLite DB stays on
/// disk for offline resume).
///
/// EXP-229: the same background task first ENDS every coding_sessions row
/// this process launched — sign-out used to strand them `running` (ghost
/// "coding now" on every client) until the server's 2h sweep. Ordering is
/// load-bearing: the ends must precede the revocation, and both use a
/// SNAPSHOTTED token (`StaticToken`) — the provider-backed client would race
/// the foreground's `auth.auth.sign_out` token deletion below. Failed ends
/// just leave their registry entries for the next sign-in's reconcile.
/// (Local claude children survive sign-out unchanged — pre-existing
/// behavior; only the synced rows are closed out here.)
pub fn sign_out_active(cx: &mut App) {
    let store = Store::global(cx).clone();
    let Some(account_id) = store.session(cx).account_id().map(String::from) else {
        return;
    };
    // §08: stop this account's steer control socket before tearing sync down.
    crate::steer_wiring::stop_control_channel(&account_id, cx);
    crate::device_sync::stop_device_sync(&account_id, cx);
    crate::automation_host::stop_automation_host(&account_id, cx);
    let auth = AuthContext::global(cx).clone();

    // Best-effort server-side session ends + revocation — local sign-out
    // proceeds even when this fails (offline sign-out is legal, §5.7).
    if let (Some(token), Some(account)) = (
        auth.auth.token(&account_id),
        auth.auth.account(&account_id),
    ) {
        let session_ids = crate::coding_flow::LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).session_ids())
            .unwrap_or_default();
        let data_dir = auth.data_dir.clone();
        let client = Arc::clone(&auth.client);
        cx.background_executor()
            .spawn(async move {
                if !session_ids.is_empty() {
                    let trpc = api::TrpcClient::new(
                        &account.instance_url,
                        Arc::new(api::StaticToken(token.clone())),
                    );
                    for id in session_ids {
                        let result = api::coding_sessions::end(&trpc, &id);
                        if let Err(err) = &result {
                            eprintln!(
                                "[exp-desktop] session: sign-out end of coding session {id} failed: {err}"
                            );
                        }
                        if crate::session_registry::end_outcome_resolves(&result) {
                            crate::session_registry::remove(&data_dir, &id);
                        }
                    }
                }
                if let Err(err) = client.sign_out(&account.instance_url, &token) {
                    eprintln!("[exp-desktop] session: server-side sign-out failed: {err}");
                }
            })
            .detach();
    }

    auth.auth.sign_out(&account_id);
    store.sign_out(&account_id, cx);
}

/// EXP-367 "Reset IDE data" (Settings → Tools → Danger zone; built for
/// testing fresh-install flows): sign everything out, delete ALL local IDE
/// state, and relaunch onto the login screen.
///
/// Deletes ALL THREE local roots — the data dir (`accounts.json`, tokens,
/// per-account sync DBs, `settings.json`, session registry, hook/action
/// scratch), the window-state dir (`window-size.json`, `layouts/`,
/// `updates/`; a DIFFERENT directory on macOS), and — since EXP-369 — the
/// repos root with every clone and worktree under it (the confirm dialog
/// names the path and warns about uncommitted work). Deletion is
/// best-effort: a straggling handle (Windows file locks, a detached sqlite
/// reader) logs and moves on — the restart lands on whatever is left, which
/// a fresh boot recreates or re-deletes.
pub fn reset_ide_data(cx: &mut App) {
    let auth = AuthContext::global(cx).clone();
    let store = Store::global(cx).clone();

    // The active account goes through the full sign-out path (steer stop,
    // best-effort session ends + server revocation, pipeline stop with
    // grace); other signed-in accounts get their sockets stopped and their
    // sessions revoked best-effort.
    let active = store.session(cx).account_id().map(String::from);
    sign_out_active(cx);
    for account in auth.auth.signed_in_accounts() {
        if Some(&account.id) == active.as_ref() {
            continue;
        }
        crate::steer_wiring::stop_control_channel(&account.id, cx);
        crate::device_sync::stop_device_sync(&account.id, cx);
        crate::automation_host::stop_automation_host(&account.id, cx);
        if let Some(token) = auth.auth.token(&account.id) {
            let client = Arc::clone(&auth.client);
            let instance = account.instance_url.clone();
            cx.background_executor()
                .spawn(async move {
                    if let Err(err) = client.sign_out(&instance, &token) {
                        log::warn!("[exp-desktop] reset: server-side sign-out failed: {err}");
                    }
                })
                .detach();
        }
    }

    let mut roots = vec![auth.data_dir.clone()];
    if let Some(local) = crate::window_size::app_data_dir() {
        if !roots.contains(&local) {
            roots.push(local);
        }
    }
    // EXP-369: the clones go too. The path is user-editable (Settings →
    // Tools), so it only qualifies when it is strictly INSIDE the user's home
    // directory (see [`repos_root_is_deletable`]) — never `/`, `/Users`,
    // `/home`, `/tmp` or `~` itself.
    let repos_root = CodingHub::global(cx)
        .read(cx)
        .settings
        .repos_root_path();
    let sane = repos_root_is_deletable(&repos_root, dirs::home_dir().as_deref());
    if sane && !roots.contains(&repos_root) {
        roots.push(repos_root);
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .spawn(async move {
                for root in roots {
                    match std::fs::remove_dir_all(&root) {
                        Ok(()) => {}
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                        Err(err) => {
                            log::warn!(
                                "[ui] reset: could not fully delete {}: {err}",
                                root.display()
                            );
                        }
                    }
                }
            })
            .await;
        let _ = cx.update(|cx| cx.restart());
    })
    .detach();
}

/// Whether [`reset_ide_data`] may recursively delete `repos_root`. The path is
/// user-editable (Settings → Tools), so the bar is deliberately high: the
/// user's home directory must be a PROPER ancestor of it. That refuses the
/// filesystem root, `~` itself, and every shared parent a stray setting could
/// name (`/Users`, `/home`, `/tmp`, `/`), while allowing the default
/// `~/.exponential/repos` and any other in-home location. An unknown home dir
/// (`home == None`) refuses outright — nothing to prove containment against.
fn repos_root_is_deletable(repos_root: &Path, home: Option<&Path>) -> bool {
    // `..` would satisfy the prefix test while resolving anywhere (`~/../..`).
    if repos_root
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return false;
    }
    repos_root.parent().is_some()
        && home.is_some_and(|home| repos_root != home && repos_root.starts_with(home))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-367: "Reset IDE data" only recurses into a repos root the home
    /// directory PROPERLY contains — every shared parent is refused.
    #[test]
    fn repos_root_must_live_under_the_home_directory() {
        let home = PathBuf::from("/Users/dev");
        let deletable = |path: &str| repos_root_is_deletable(Path::new(path), Some(&home));

        assert!(deletable("/Users/dev/.exponential/repos"));
        assert!(deletable("/Users/dev/code"));

        assert!(!deletable("/Users/dev")); // home itself
        assert!(!deletable("/Users")); // the shared parent of every home
        assert!(!deletable("/home"));
        assert!(!deletable("/tmp"));
        assert!(!deletable("/"));
        assert!(!deletable("/Users/other/code")); // a sibling user's tree
        assert!(!deletable("/Users/dev/../other")); // escapes via `..`
        assert!(!deletable("/Users/dev/..")); // ditto, straight to /Users
    }

    /// No resolvable home directory ⇒ nothing to prove containment against.
    #[test]
    fn repos_root_without_a_home_directory_is_refused() {
        assert!(!repos_root_is_deletable(
            Path::new("/Users/dev/.exponential/repos"),
            None
        ));
    }
}
