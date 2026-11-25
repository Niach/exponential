//! #EXP-2138 (`fixtures/held-gone.json`): the desktop app's host of
//! [`steer::held_gone`]: the active account's thread, from its connect to the
//! sign-out or the switch to another account.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chat_store::sqlite::SqliteStore;
use gpui::{App, Task};

use crate::AuthContext;

/// How often the foreground tells the thread whether the rows are synced and
/// which store is open.
const LOOK: Duration = Duration::from_secs(2);

struct HeldGoneGlobal {
    /// Dropping it stops the thread.
    _handle: steer::held_gone::HeldGoneHandle,
    _watch: Task<()>,
}

impl gpui::Global for HeldGoneGlobal {}

/// Starts `account`'s thread (connect; replaces another account's).
pub(crate) fn start(account: &api::Account, cx: &mut App) {
    stop(cx);
    // #EXP-1727: a Local desktop has no server to ask.
    if account.is_local() {
        return;
    }
    let Some(auth) = cx.try_global::<AuthContext>().cloned() else { return };
    let ready = Arc::new(AtomicBool::new(false));
    let store: Arc<Mutex<std::sync::Weak<SqliteStore>>> = Arc::default();
    let trpc = Arc::new(api::TrpcClient::new(&account.instance_url, auth.auth.token_provider(&account.id)));
    let handle = steer::held_gone::spawn(steer::held_gone::HeldGoneDeps {
        data_dir: auth.data_dir.clone(),
        account_id: account.id.clone(),
        api: Arc::new(trpc),
        ready: {
            let ready = Arc::clone(&ready);
            Arc::new(move || ready.load(Ordering::SeqCst))
        },
        store: {
            let store = Arc::clone(&store);
            Arc::new(move || store.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).upgrade())
        },
    });
    let account_id = account.id.clone();
    let watch = cx.spawn(async move |cx| loop {
        cx.update(|cx| {
            let active = crate::queries::active_account(cx).is_some_and(|active| active.id == account_id);
            let synced = sync::Store::try_global(cx).is_some_and(|sync| sync.collections().sessions.read(cx).is_ready());
            // Weak: a sign-out's wipe waits for the store's holders.
            let open = crate::chat_store::store(cx).map(|open| Arc::downgrade(&open)).unwrap_or_default();
            *store.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = open;
            ready.store(active && synced, Ordering::SeqCst);
        });
        cx.background_executor().timer(LOOK).await;
    });
    cx.set_global(HeldGoneGlobal { _handle: handle, _watch: watch });
}

/// Sign-out, or another account takes over.
pub(crate) fn stop(cx: &mut App) {
    if cx.has_global::<HeldGoneGlobal>() {
        cx.remove_global::<HeldGoneGlobal>();
    }
}
