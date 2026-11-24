//! EXP-1457 step 1: the desktop app's eNet install (`steer::enet`). One node
//! per signed-in account, on the steer tokio runtime: started with the
//! account's control channel, its allowlist re-read whenever the synced
//! `teams` collection changes, stopped on sign-out (and revoked when the
//! sign-out is explicit). VAPP-43: it serves this machine's stored journals
//! (STORED REPLAY) under the synced `sessions` rows' access rule.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui::{App, Global, Subscription};
use steer::enet::{InstallKind, InstallNode, InstallSpec};
use sync::Store;

use crate::session::AuthContext;

struct EnetInstalls {
    by_account: HashMap<String, InstallNode>,
    /// The synced team ids, shared with the node's allowlist loop.
    team_ids: Arc<Mutex<Vec<String>>>,
    _teams: Option<Subscription>,
    /// #EXP-1941: the key holder watcher runs ([`watch_key_holder`]).
    watching: bool,
}
impl Global for EnetInstalls {}

fn synced_team_ids(cx: &App) -> Vec<String> {
    let mut ids: Vec<String> = Store::global(cx)
        .collections()
        .teams
        .read(cx)
        .iter()
        .map(|team| team.id.clone())
        .collect();
    ids.sort();
    ids
}

fn installs(cx: &mut App) -> &mut EnetInstalls {
    if !cx.has_global::<EnetInstalls>() {
        let teams = Store::global(cx).collections().teams.clone();
        let subscription = cx.observe(&teams, |_, cx| {
            let ids = synced_team_ids(cx);
            let Some(installs) = cx.try_global::<EnetInstalls>() else {
                return;
            };
            let mut current = installs.team_ids.lock().unwrap_or_else(|p| p.into_inner());
            if *current != ids {
                *current = ids;
                drop(current);
                for node in installs.by_account.values() {
                    node.refresh();
                }
            }
        });
        let team_ids = Arc::new(Mutex::new(synced_team_ids(cx)));
        cx.set_global(EnetInstalls {
            by_account: HashMap::new(),
            team_ids,
            _teams: Some(subscription),
            watching: false,
        });
    }
    cx.global_mut::<EnetInstalls>()
}

/// Starts `account`'s node (idempotent: a running one only re-reads its
/// allowlist). From `steer_wiring::start_control_channel`.
pub fn start(account: &api::Account, cx: &mut App) {
    if account.is_local() {
        return;
    }
    let Some(runtime) = crate::steer_wiring::runtime(cx) else {
        return;
    };
    let auth = AuthContext::global(cx).clone();
    let rows = Store::global(cx).manager();
    let state = installs(cx);
    if let Some(node) = state.by_account.get(&account.id) {
        node.refresh();
        return;
    }
    // The store syncs one account at a time and the allowlist reads ITS
    // teams: another account's node stops (it restarts on switching back).
    for (_, other) in state.by_account.drain() {
        other.stop();
    }
    let team_ids = Arc::clone(&state.team_ids);
    let trpc = Arc::new(api::TrpcClient::new(
        &account.instance_url,
        auth.auth.token_provider(&account.id),
    ));
    let spec = InstallSpec {
        kind: InstallKind::Desktop,
        user_id: account.user_id.clone(),
        device_id: Some(steer::persistent_device_id(&auth.data_dir)),
        label: Some(api::users::hostname()),
        identity_path: steer::enet::identity_path(&auth.data_dir, &account.id, InstallKind::Desktop),
        net: steer::enet::EnetConfig::default(),
    };
    let node = InstallNode::start(
        runtime.handle(),
        spec,
        trpc,
        Arc::new(move || team_ids.lock().unwrap_or_else(|p| p.into_inner()).clone()),
    );
    // VAPP-43 STORED REPLAY: this machine's journals, read under the access
    // rule of the account's synced `sessions` rows.
    node.serve_stored(steer::StoredJournals {
        data_dir: auth.data_dir.clone(),
        session: stored_session_lookup(rows, account.id.clone()),
        // #EDE-108: catch-up pages for this machine's runs come off its
        // journals, for any other device's off the account's chat store.
        device_id: Some(steer::persistent_device_id(&auth.data_dir)),
        chat_store: Some(crate::chat_store::serving_slot()),
    });
    state.by_account.insert(account.id.clone(), node);
    watch_key_holder(cx);
}

/// #EXP-1941: the device caps follow the key holder (`call-seal`), and caps
/// reach the server only inside a register. The holder starts on the steer
/// runtime once the install registered, beside the control channel's own
/// register (which a first sign-in loses), and goes when the node stops:
/// each change runs the advertisement's refresh. One watcher per process.
fn watch_key_holder(cx: &mut App) {
    if std::mem::replace(&mut installs(cx).watching, true) {
        return;
    }
    cx.spawn(async move |cx| {
        let mut had = false;
        let mut trust_seen: Option<(usize, u64)> = None;
        loop {
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            let has = coding::sealing::service().is_some();
            if has != had {
                had = has;
                cx.update(crate::steer_wiring::refresh_device_advertisement);
            }
            // #EXP-1950: the pin store's rows / state moved: Needs you and
            // its counts re-read them.
            cx.update(|cx| {
                let now = install_trust(cx).map(|keeper| (Arc::as_ptr(&keeper) as usize, keeper.generation()));
                if now != trust_seen {
                    trust_seen = now;
                    let tick = cx.try_global::<crate::install_approval::InstallTrustTick>().map_or(0, |tick| tick.0) + 1;
                    cx.set_global(crate::install_approval::InstallTrustTick(tick));
                    cx.refresh_windows();
                }
            });
        }
    })
    .detach();
}

/// A synced `sessions` row → its stored-replay audience, read off the
/// account's sync store (any thread).
fn stored_session_lookup(
    manager: Arc<sync::SyncManager>,
    account_id: String,
) -> Arc<dyn Fn(&str) -> Option<steer::StoredSession> + Send + Sync> {
    Arc::new(move |session_id| {
        let store = manager.store(&account_id)?;
        let spec = sync::shapes::shape_by_name("sessions")?;
        let row = store
            .read_by_key(spec, &sync::protocol::RowKey::Single(session_id.to_string()))
            .ok()??;
        let row: domain::rows::SessionRow = serde_json::from_value(serde_json::Value::Object(row)).ok()?;
        steer::StoredSession::of(&row)
    })
}

/// #EXP-1950: the running account's install approval state (its pin store
/// and rules, `steer::install_trust`), once its key holder runs. The store
/// syncs one account at a time, so at most one node runs.
pub(crate) fn install_trust(cx: &App) -> Option<Arc<steer::install_trust::TrustKeeper>> {
    cx.try_global::<EnetInstalls>()?.by_account.values().find_map(|node| node.trust())
}

/// #EDE-108: `account_id`'s running node (the chat store's catch-up asks
/// through it).
pub(crate) fn node(account_id: &str, cx: &App) -> Option<InstallNode> {
    cx.try_global::<EnetInstalls>()?.by_account.get(account_id).cloned()
}

/// Stops `account_id`'s node; the install stays registered. Returns it so
/// an explicit sign-out can revoke it ([`InstallNode::revoke_blocking`]).
pub fn stop(account_id: &str, cx: &mut App) -> Option<InstallNode> {
    if !cx.has_global::<EnetInstalls>() {
        return None;
    }
    let node = cx.global_mut::<EnetInstalls>().by_account.remove(account_id)?;
    node.stop();
    Some(node)
}

/// EXP-1459: the running node of the account the store syncs (the writer
/// wraps a new root's key through it).
pub fn active_node(cx: &App) -> Option<InstallNode> {
    let account = crate::queries::active_account(cx)?;
    cx.try_global::<EnetInstalls>()?.by_account.get(&account.id).cloned()
}
