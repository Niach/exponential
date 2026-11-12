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

/// EXP-757: the startup pass of [`coding::scratch::sweep`], off the
/// foreground. The keep set is what THIS process has live plus what the
/// shared session registry attributes to a live sibling (the CLI daemon on
/// the same machine — REV-20, one data dir).
///
/// EXP-1487: plus every session that is idle, not over: one the registry
/// still names (whatever process recorded it: a left-behind run is re-claimed
/// AFTER this pass, "idle, waiting for its next message") and every synced
/// row on this device that has not ended. An idle chat's scratch dir, run
/// record and journal ARE its transcript and its resume; purging them left
/// the view on "The agent is starting" forever.
fn sweep_scratch_dirs(cx: &mut App) {
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    let mut live: Vec<String> = crate::coding_flow::LocalSessions::global_ref(cx)
        .map(|sessions| sessions.read(cx).session_ids())
        .unwrap_or_default();
    live.extend(crate::session_registry::left_behind());
    let device_id = crate::queries::own_device_id(cx);
    if let Some(store) = Store::try_global(cx) {
        live.extend(
            store
                .collections()
                .sessions
                .read(cx)
                .iter()
                .filter(|row| row.on_device(&device_id) && !row.is_ended())
                .map(|row| row.id.clone()),
        );
    }
    cx.background_executor()
        .spawn(async move {
            // EXP-1794: every claude login onto the one conversation store
            // (whichever host starts first; spawns wait for its lock).
            coding::agent_profiles::migrate_claude_store(&data_dir);
            live.extend(crate::session_registry::live_ids(&data_dir));
            live.extend(crate::session_registry::recorded_ids(&data_dir));
            let report = coding::scratch::sweep(&data_dir, &live);
            // #EXP-1658: ended runs' team skills folders.
            let skills = coding::team_skills::sweep(&data_dir, &live);
            if skills > 0 {
                log::info!("skills sweep: removed {skills} folder(s)");
            }
            // EXP-764: the purged runs' steer journals go with their records.
            for session_id in &report.purged {
                steer::remove_journal(&data_dir, session_id);
            }
            if !report.is_noop() {
                log::info!(
                    "scratch sweep: removed {} run dir(s), purged {} run(s), dropped {} trust entr(y/ies), kept {} live + {} young",
                    report.removed.len(),
                    report.purged.len(),
                    report.trust_dropped,
                    report.kept_live,
                    report.kept_young
                );
            }
        })
        .detach();
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
        // The desktop syncs all 23 shapes; only the CLI daemon subsets.
        shapes: None,
    };
    // Sealed rows open from the first frame: the persisted tree keys are
    // in memory before the store hydrates its rows.
    crate::sealing::preload(account, &auth.data_dir);
    match store.connect(config, cx) {
        Ok(_) => {
            // §08 device presence: dial the steer control socket for this
            // account (no-op when steer is disabled/unconfigured).
            crate::steer_wiring::start_control_channel(account, cx);
            // EXP-481: the device-state sync loop (heartbeat/work pull/
            // inventory reports) rides beside the control socket.
            crate::device_sync::start_device_sync(account, cx);
            // EXP-1005: walled runs rotate to another account between turns.
            crate::account_rotation_host::start_account_rotation_host(cx);
            // EXP-229: end the sessions rows a crash / forced logout
            // stranded `running` — this is the single choke point every
            // sign-in path (warm start, dev inject, OAuth, login form) runs
            // through, so orphans heal on the next connect instead of
            // blocking "coding now" for the server sweep's 2h window.
            crate::session_registry::reconcile_stale_sessions(account, cx);
            // EXP-757/764: purge the repo-less runs nothing is running any
            // more — crashes, older builds: scratch dirs, claude trust
            // entries, run records, journals. Live runs, ours and a sibling
            // daemon's, are the keep set. Background: a dir walk plus a
            // `~/.claude.json` rewrite.
            sweep_scratch_dirs(cx);
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
/// the background executor: failures (offline, a transport error) only
/// mean the next sign-in tries again.
fn claim_timezone(account: &api::Account, cx: &mut App) {
    // #EXP-1727: a Local desktop has no digest to send.
    if account.is_local() {
        return;
    }
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
/// EXP-229: the same background task first ENDS every sessions row
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
    // #EXP-1727: leaving a Local desktop signs nobody out.
    if crate::local_desktops::account(&account_id, cx).is_some() {
        crate::local_desktops::leave(cx);
        return;
    }
    // EXP-1457: an explicit sign-out revokes this install's eNet endpoint
    // (taken before the control-channel stop, which only stops it).
    let enet = crate::enet_wiring::stop(&account_id, cx);
    // #EXP-1538: the Dock's last-known stream slots belong to this account.
    crate::steer_viewer::StreamSlotMemo::clear(cx);
    // §08: stop this account's steer control socket before tearing sync down.
    crate::steer_wiring::stop_control_channel(&account_id, cx);
    crate::device_sync::stop_device_sync(&account_id, cx);
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
        // EXP-1275: the BLOCKING wait covers every window until the server
        // round trips settle (offline included: they are best-effort, so
        // no error toast).
        crate::feedback::Blocking::begin(crate::feedback::BLOCKING_SIGN_OUT, cx);
        let ends = cx.background_executor().spawn(async move {
            if let Some(enet) = enet {
                let trpc = api::TrpcClient::new(
                    &account.instance_url,
                    Arc::new(api::StaticToken(token.clone())),
                );
                enet.revoke_blocking(&trpc);
            }
            if !session_ids.is_empty() {
                let trpc = api::TrpcClient::new(
                    &account.instance_url,
                    Arc::new(api::StaticToken(token.clone())),
                );
                for id in session_ids {
                    let result = api::sessions::end(&trpc, &id);
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
        });
        cx.spawn(async move |cx| {
            ends.await;
            let _ = cx.update(|cx| crate::feedback::Blocking::end(cx));
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
        let _ = cx.update(|cx| crate::update::relaunch(None, cx));
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
    crate::sealing::init(cx);
    let dev_server = std::env::var("EXP_DEV_SERVER").ok();
    let dev_token = std::env::var("EXP_DEV_TOKEN").ok();
    if let (Some(server), Some(token)) = (dev_server, dev_token) {
        dev_inject_session(server, token, cx);
        return;
    }

    // #EDE-65: on Exponential Desktop the app starts where eDE's switcher is
    // (it starts after the shell, so it never hears the shell's start link;
    // a link that arrives later still wins).
    match on_exponential_desktop().then(|| ede_pick(elocal::ede_legacy_root().as_deref())) {
        Some(EdePick::Local) => {
            if crate::local_desktops::enter_current(cx) {
                return;
            }
        }
        Some(EdePick::Team { team_id, account_id }) => {
            crate::ui_prefs::set_on_local_desktop(false);
            crate::navigation::remember_team(cx, team_id);
            // The account that owns the team, not just the first signed-in one.
            let auth = AuthContext::global(cx).clone();
            if let Some(account) = pick_account(auth.auth.signed_in_accounts(), account_id.as_deref()) {
                connect_account(&account, cx);
            }
            return;
        }
        // #EXP-1741: off eDE the IDE has no Local desktops: a window saved
        // on one (an earlier build) reopens on the signed-in account.
        None => crate::ui_prefs::set_on_local_desktop(false),
    }
    let auth = AuthContext::global(cx).clone();
    if let Some(account) = auth.auth.signed_in_accounts().into_iter().next() {
        connect_account(&account, cx);
    }
}

/// The session is Exponential Desktop (eDE sets `XDG_CURRENT_DESKTOP`).
fn on_exponential_desktop() -> bool {
    crate::local_desktops::available()
}

/// #EDE-65: where eDE's switcher is: the team its `switcher.json` names,
/// else the Local desktop the shared index calls current (eDE's
/// `set_current`; a Local pick forgets the team, and a shell with no pick
/// yet starts on Local).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EdePick {
    Local,
    /// The team and the account eDE's switcher listed it under (older
    /// files name no account).
    Team { team_id: String, account_id: Option<String> },
}

/// The signed-in account to connect: the one named, else the first.
pub(crate) fn pick_account(accounts: Vec<api::accounts::Account>, wanted: Option<&str>) -> Option<api::accounts::Account> {
    let named = wanted.and_then(|id| accounts.iter().position(|a| a.id == id));
    let mut accounts = accounts;
    match named {
        Some(i) => Some(accounts.swap_remove(i)),
        None => accounts.into_iter().next(),
    }
}

pub(crate) fn ede_pick(ede_config: Option<&std::path::Path>) -> EdePick {
    let saved = ede_config
        .and_then(|dir| std::fs::read_to_string(dir.join("switcher.json")).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|v| v["kind"] == "team");
    let Some(saved) = saved else { return EdePick::Local };
    let text = |key: &str| saved[key].as_str().filter(|v| !v.is_empty()).map(str::to_string);
    match text("id") {
        Some(team_id) => EdePick::Team { team_id, account_id: text("account") },
        None => EdePick::Local,
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

    /// #EDE-65: the app starts after eDE's shell, so it reads where the
    /// switcher is: a picked team from `switcher.json`, else Local.
    #[test]
    fn the_app_starts_where_edes_switcher_is() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ede_pick(None), EdePick::Local);
        assert_eq!(ede_pick(Some(dir.path())), EdePick::Local, "no pick yet: eDE starts on Local");
        std::fs::write(dir.path().join("switcher.json"), r#"{"kind":"team","id":"e891ab20"}"#).unwrap();
        assert_eq!(ede_pick(Some(dir.path())), EdePick::Team { team_id: "e891ab20".into(), account_id: None });
        // What eDE writes since: the account the switcher listed the team under.
        std::fs::write(dir.path().join("switcher.json"), r#"{"kind":"team","id":"e891ab20","account":"a2"}"#).unwrap();
        assert_eq!(ede_pick(Some(dir.path())), EdePick::Team { team_id: "e891ab20".into(), account_id: Some("a2".into()) });
        std::fs::write(dir.path().join("switcher.json"), r#"{"kind":"local","id":"87858284"}"#).unwrap();
        assert_eq!(ede_pick(Some(dir.path())), EdePick::Local);
        std::fs::write(dir.path().join("switcher.json"), "not json").unwrap();
        assert_eq!(ede_pick(Some(dir.path())), EdePick::Local);
    }

    /// #EDE-65: two signed-in accounts: the app connects the one that owns
    /// eDE's team, the first only when eDE named none (or one signed out).
    #[test]
    fn the_teams_own_account_connects() {
        let account = |id: &str| -> api::accounts::Account {
            serde_json::from_value(serde_json::json!({"id": id, "instanceUrl": "https://app.exponential.at", "userId": format!("u-{id}"), "email": format!("{id}@example.com")})).unwrap()
        };
        let two = || vec![account("a1"), account("a2")];
        assert_eq!(pick_account(two(), Some("a2")).unwrap().id, "a2");
        assert_eq!(pick_account(two(), Some("a1")).unwrap().id, "a1");
        assert_eq!(pick_account(two(), None).unwrap().id, "a1");
        assert_eq!(pick_account(two(), Some("gone")).unwrap().id, "a1");
        assert_eq!(pick_account(Vec::new(), Some("a2")), None);
    }

    /// #EDE-65: on Local the app opens the desktop eDE's switcher made
    /// current in the shared index, not the one the app had open last.
    #[gpui::test]
    fn a_local_start_opens_the_desktop_ede_made_current(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let mut ede = elocal::Desktops::open(&path).unwrap();
        let first = ede.current().unwrap().to_string();
        let work = ede.create("Work").unwrap().id;
        assert!(ede.set_current(&first).unwrap());
        cx.update(|cx| {
            let store = sync::Store::open(cx, None, None);
            cx.set_global(store);
            crate::local_desktops::install(path.clone(), cx);
        });
        // eDE picks Work after the app opened the index.
        assert!(ede.set_current(&work).unwrap());
        cx.update(|cx| {
            assert!(crate::local_desktops::enter_current(cx));
            assert_eq!(Store::global(cx).session(cx).account_id(), Some(format!("local-{work}").as_str()));
        });
        std::mem::forget(dir);
        let _ = cx.add_window_view(|_, _| gpui::Empty);
    }

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
