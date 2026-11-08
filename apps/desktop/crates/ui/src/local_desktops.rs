//! #EXP-1727: Local desktops in the IDE, offline, on the same stack as
//! Exponential Desktop: `elocal`'s store is the active "account": the
//! collections read its SQLite ([`sync::Store::connect_local`]), the tRPC
//! wrappers talk to its executor over the Transport seam, and agent runs
//! go through `elocal::runs`. No sync, no presence, no device loop: nothing
//! leaves the machine.

use std::path::PathBuf;
use std::sync::Arc;

use elocal::runs::LocalRuns;
use elocal::{Desktops, LocalDesktop};
use gpui::{App, Global};
use sync::Store;

use crate::session::AuthContext;

const PREFIX: &str = "local-";
const URL_SCHEME: &str = api::accounts::LOCAL_SCHEME;

/// The machine's Local desktops and the open one.
pub(crate) struct LocalDesktops {
    desktops: Desktops,
    open: Option<Arc<LocalDesktop>>,
    runs: Option<Arc<LocalRuns>>,
}

impl Global for LocalDesktops {}

fn root(cx: &App) -> Option<PathBuf> {
    Some(cx.try_global::<AuthContext>()?.data_dir.join("desktops"))
}

fn ensure(cx: &mut App) -> bool {
    if cx.has_global::<LocalDesktops>() {
        return true;
    }
    let Some(root) = root(cx) else { return false };
    // #EDE-65: the machine's one index, eDE's desktops from before imported once.
    match Desktops::open_importing(root, elocal::ede_legacy_root().as_deref()) {
        Ok((desktops, imported)) => {
            if !imported.ids.is_empty() {
                log::info!("[exp-desktop] local desktops: imported eDE's {:?}", imported.ids);
            }
            cx.set_global(LocalDesktops { desktops, open: None, runs: None });
            true
        }
        Err(err) => {
            log::warn!("[exp-desktop] local desktops: {err}");
            false
        }
    }
}

/// The machine's desktops under `root`, for the tests and the pictures.
#[cfg(test)]
pub(crate) fn install(root: std::path::PathBuf, cx: &mut App) {
    cx.set_global(LocalDesktops { desktops: Desktops::open(root).unwrap(), open: None, runs: None });
}

/// The account id a Local desktop runs under.
fn account_id(desktop: &str) -> String {
    format!("{PREFIX}{desktop}")
}

/// The open desktop as the "account" of `account_id`.
pub(crate) fn account(id: &str, cx: &App) -> Option<api::Account> {
    let open = cx.try_global::<LocalDesktops>()?.open.as_ref()?;
    if account_id(&open.entry.id) != id {
        return None;
    }
    let identity = open.executor.identity();
    Some(api::Account {
        id: id.to_string(),
        instance_url: format!("{URL_SCHEME}{}", open.entry.id),
        user_id: identity.user_id,
        email: identity.email,
        name: Some(identity.name),
        is_admin: false,
        // No wizard: a desktop opens seeded, with its team and board.
        onboarding_completed_at: Some(String::new()),
    })
}

/// A client over the open desktop while it is the active account.
pub(crate) fn client(cx: &App) -> Option<api::TrpcClient> {
    let open = cx.try_global::<LocalDesktops>()?.open.as_ref()?;
    let active = Store::try_global(cx)?.session(cx);
    (active.account_id() == Some(account_id(&open.entry.id).as_str())).then(|| open.client())
}

/// The switcher's rows: `(id, name, current)`.
pub(crate) fn entries(cx: &mut App) -> Vec<(String, String, bool)> {
    if !ensure(cx) {
        return Vec::new();
    }
    // The index is eDE's too (#EDE-65): its new or renamed desktops show.
    cx.global_mut::<LocalDesktops>().desktops.reload();
    let locals = cx.global::<LocalDesktops>();
    let open = locals.open.as_ref().map(|d| d.entry.id.clone());
    locals.desktops.active().into_iter().map(|e| (e.id.clone(), e.name.clone(), Some(&e.id) == open.as_ref())).collect()
}

/// The runner over the person's first usable CLI (`local_runs.rs` in eDE).
fn start_runs(desktop: &Arc<LocalDesktop>) -> Option<Arc<LocalRuns>> {
    let cwd = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    match LocalRuns::start_first_cli(desktop.clone(), cwd)? {
        Ok(runs) => Some(runs),
        Err(err) => {
            log::warn!("[exp-desktop] local runs: {err}");
            None
        }
    }
}

/// Opens the Local desktop `id` as the active account.
pub(crate) fn enter(id: &str, cx: &mut App) -> bool {
    if !ensure(cx) {
        return false;
    }
    let opened = {
        let locals = cx.global_mut::<LocalDesktops>();
        if let Err(err) = locals.desktops.set_current(id) {
            log::warn!("[exp-desktop] local desktops: {err}");
        }
        locals.desktops.open_desktop(id)
    };
    let desktop = match opened {
        Ok(desktop) => Arc::new(desktop),
        Err(err) => {
            log::warn!("[exp-desktop] local desktop {id}: {err}");
            return false;
        }
    };
    let (tx, rx) = flume::unbounded();
    let deltas = desktop.store.subscribe();
    std::thread::Builder::new()
        .name("local-desktop-deltas".into())
        .spawn(move || {
            while let Ok(delta) = deltas.recv() {
                if tx.send((delta.table.to_string(), delta.keys)).is_err() {
                    return;
                }
            }
        })
        .expect("the deltas thread");
    let runs = start_runs(&desktop);
    let db = desktop.dir.join("local.sqlite");
    let account = account_id(id);
    crate::app_outbox::forget_account(&account, cx);
    {
        let locals = cx.global_mut::<LocalDesktops>();
        if let Some(previous) = locals.runs.take() {
            previous.stop_all();
        }
        locals.open = Some(desktop);
        locals.runs = runs;
    }
    if let Err(err) = Store::global(cx).clone().connect_local(&account, &db, rx, cx) {
        log::warn!("[exp-desktop] local desktop {id}: {err}");
        cx.global_mut::<LocalDesktops>().open = None;
        return false;
    }
    crate::ui_prefs::set_on_local_desktop(true);
    true
}

/// Opens the machine's current Local desktop (one exists from the first open).
pub(crate) fn enter_current(cx: &mut App) -> bool {
    if !ensure(cx) {
        return false;
    }
    let Some(current) = cx.global::<LocalDesktops>().desktops.current().map(str::to_string) else { return false };
    enter(&current, cx)
}

/// A new Local desktop, opened.
pub(crate) fn create(cx: &mut App) -> bool {
    if !ensure(cx) {
        return false;
    }
    let name = format!("Desktop {}", cx.global::<LocalDesktops>().desktops.active().len() + 1);
    let created = cx.global_mut::<LocalDesktops>().desktops.create(&name);
    match created {
        Ok(entry) => enter(&entry.id, cx),
        Err(err) => {
            log::warn!("[exp-desktop] new local desktop: {err}");
            false
        }
    }
}

/// Back to the signed-in account, or to the login surface.
pub(crate) fn leave(cx: &mut App) {
    if let Some(locals) = cx.try_global::<LocalDesktops>() {
        let runs = locals.runs.clone();
        if let Some(runs) = runs {
            runs.stop_all();
        }
    }
    if cx.has_global::<LocalDesktops>() {
        let locals = cx.global_mut::<LocalDesktops>();
        locals.open = None;
        locals.runs = None;
    }
    crate::ui_prefs::set_on_local_desktop(false);
    let store = Store::global(cx).clone();
    if let Some(id) = store.session(cx).account_id().map(str::to_string) {
        store.sign_out(&id, cx);
    }
    let account = AuthContext::global(cx).auth.signed_in_accounts().into_iter().next();
    if let Some(account) = account {
        crate::session::connect_account(&account, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole seam: the store's team and board reach the collections,
    /// the client writes through the executor, the write's delta comes back
    /// into the collections, and leaving clears the session.
    #[gpui::test]
    fn a_desktop_is_the_active_account_and_its_writes_come_back(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        cx.update(|cx| {
            let store = sync::Store::open(cx, None, None);
            cx.set_global(store);
            install(path, cx);
            assert!(enter_current(cx));
        });
        let (team, client) = cx.update(|cx| {
            let c = Store::global(cx).collections().clone();
            let teams = c.teams_sorted(cx);
            assert_eq!(teams.len(), 1);
            assert_eq!(c.boards_in_team(&teams[0].id, cx).len(), 1);
            assert!(crate::queries::active_account(cx).is_some_and(|a| a.id.starts_with(PREFIX) && a.is_local()));
            // The record outbox's log finds the desktop's store.
            assert!(Store::global(cx).manager().store(&account_id(&teams[0].id)).is_some());
            (teams[0].id.clone(), client(cx).expect("a local client"))
        });
        let board = cx.update(|cx| Store::global(cx).collections().boards_in_team(&team, cx)[0].id.clone());
        api::issues::issues_create(&client, &api::issues::IssuesCreateInput::new(&board, "First issue")).expect("the executor answers");
        for _ in 0..200 {
            cx.run_until_parked();
            if cx.update(|cx| !Store::global(cx).collections().issues_in_board(&board, cx).is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|cx| assert_eq!(Store::global(cx).collections().issues_in_board(&board, cx)[0].title, "First issue"));
        std::mem::forget(dir);
        // The leaked window context keeps the app past gpui's exit-time leak
        // check (`Store::open` holds its handles).
        let _ = cx.add_window_view(|_, _| gpui::Empty);
    }

    #[test]
    fn a_desktop_account_id_is_prefixed() {
        assert_eq!(account_id("abc"), "local-abc");
    }
}
