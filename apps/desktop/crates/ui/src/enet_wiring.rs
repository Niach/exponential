//! EXP-1457 step 1: the desktop app's eNet install (`steer::enet`). One node
//! per signed-in account, on the steer tokio runtime: started with the
//! account's control channel, its allowlist re-read whenever the synced
//! `teams` collection changes, stopped on sign-out (and revoked when the
//! sign-out is explicit).

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
        });
    }
    cx.global_mut::<EnetInstalls>()
}

/// Starts `account`'s node (idempotent: a running one only re-reads its
/// allowlist). From `steer_wiring::start_control_channel`.
pub fn start(account: &api::Account, cx: &mut App) {
    let Some(runtime) = crate::steer_wiring::runtime(cx) else {
        return;
    };
    let auth = AuthContext::global(cx).clone();
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
    state.by_account.insert(account.id.clone(), node);
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
