//! #EDE-226: on Exponential Desktop this app is the HOLDER of the login's
//! account. The shell's Connect is the one sign-in; the app follows
//! `accounts.json` ([`follow_accounts`]), does what the shell asks of the
//! account and the install ([`command`]) and tells it what its pin store
//! yields ([`publish`]). On every other desktop none of this runs.

use std::cell::RefCell;
use std::time::Duration;

use api::holder::{Command, FailedPress, InstallRow, InstallState};
use gpui::App;
use steer::install_trust::{TrustRow, TrustSnapshot};
use sync::{SessionPhase, Store};

use crate::session::AuthContext;

/// This app holds the login's account (the session is Exponential Desktop).
pub(crate) fn holds_login() -> bool {
    crate::local_desktops::available()
}

/// Follows the sign-ins and sign-outs of the shell and the CLI.
pub fn follow_accounts(cx: &mut App) {
    if !holds_login() {
        return;
    }
    if let Some(path) = runtime_path(api::holder::HOLDER_PID) {
        if let Err(err) = std::fs::write(&path, std::process::id().to_string()) {
            log::warn!("[holder] {}: {err}", path.display());
        }
    }
    let changes = AuthContext::global(cx).auth.watch(Duration::from_secs(1));
    cx.spawn(async move |cx| {
        while let Ok(event) = changes.recv_async().await {
            cx.update(|cx| account_changed(event, cx));
        }
    })
    .detach();
}

fn account_changed(event: api::AuthEvent, cx: &mut App) {
    log::info!("[holder] accounts.json changed: {event:?}");
    match event {
        api::AuthEvent::SignedIn { account_id } => {
            // A new token of the synced account is read at call time.
            if matches!(Store::global(cx).session(cx), SessionPhase::Synced { .. }) && crate::local_desktops::open_id(cx).is_none() {
                return;
            }
            if crate::local_desktops::open_id(cx).is_some() {
                // A Local desktop the person picked stays; the one the app
                // started on because nobody had signed in does not.
                if !crate::session::ede_picked(elocal::ede_legacy_root().as_deref()) {
                    crate::local_desktops::leave(cx);
                }
                return;
            }
            let account = AuthContext::global(cx).auth.account(&account_id);
            if let Some(account) = account {
                crate::session::connect_account(&account, cx);
            }
        }
        api::AuthEvent::SignedOut { account_id } | api::AuthEvent::Unauthorized { account_id } => crate::session::signed_out_elsewhere(&account_id, cx),
    }
}

/// A holder command from the shell; `false`: `datagram` is none.
pub(crate) fn command(datagram: &str, cx: &mut App) -> bool {
    let Some(command) = Command::parse(datagram) else { return false };
    if !holds_login() {
        return true;
    }
    match command {
        Command::SignOut { account } => {
            if Store::global(cx).session(cx).account_id() == Some(account.as_str()) && crate::local_desktops::account(&account, cx).is_none() {
                log::info!("[holder] signing out of Exponential (asked by the shell)");
                crate::session::sign_out_with(true, cx);
            } else {
                // Not the account this app is on: the shell signs it out itself.
                log::info!("[holder] a sign-out of another account is the shell's");
            }
        }
        Command::Approve { account, install, code } => press(account, install, code, true, cx),
        Command::Remove { account, install, code } => press(account, install, code, false, cx),
    }
    true
}

/// The row the person pressed, as the pin store yields it: the install AND
/// the code they compared.
fn pressed(rows: &[TrustRow], install: &str, code: &str) -> Option<TrustRow> {
    rows.iter().find(|row| row.install_id == install && row.code == code).cloned()
}

fn press(account: String, install: String, code: String, approve: bool, cx: &mut App) {
    let keeper = crate::enet_wiring::node(&account, cx).and_then(|node| node.trust());
    let row = keeper.as_ref().and_then(|keeper| pressed(&keeper.snapshot().rows, &install, &code));
    let (Some(keeper), Some(row)) = (keeper, row) else {
        failed(approve, "This install is not waiting for approval any more".into(), cx);
        return;
    };
    log::info!("[holder] {} install {install} (asked by the shell)", if approve { "approving" } else { "removing" });
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { if approve { keeper.approve(&row) } else { keeper.remove(&row) } }).await;
        cx.update(|cx| match result {
            Ok(()) => publish(cx),
            Err(err) => failed(approve, err, cx),
        });
    })
    .detach();
}

thread_local! {
    /// The last failed press, and the state file as last written (its account).
    static FAILED: RefCell<Option<FailedPress>> = const { RefCell::new(None) };
    static WRITTEN: RefCell<Option<(String, InstallState)>> = const { RefCell::new(None) };
}

fn failed(approve: bool, message: String, cx: &mut App) {
    FAILED.with(|last| {
        let mut last = last.borrow_mut();
        let seq = last.as_ref().map_or(1, |f| f.seq + 1);
        *last = Some(FailedPress { seq, verb: if approve { "approve" } else { "remove" }.into(), message });
    });
    publish(cx);
}

/// What the shell draws of `snapshot`.
fn install_state(snapshot: &TrustSnapshot, title: &dyn Fn(&TrustRow) -> String, failed: Option<FailedPress>) -> InstallState {
    let rows = |row: &TrustRow| InstallRow { id: row.install_id.clone(), title: title(row), code: row.code.clone(), badge: row.kind.badge().into() };
    InstallState {
        waiting: snapshot.code.clone().filter(|_| snapshot.waiting()),
        rows: if snapshot.waiting() { Vec::new() } else { snapshot.rows.iter().map(rows).collect() },
        failed,
    }
}

/// The running install's state into its file for the shell; with no install
/// running the file goes (a stopped app's rows cannot be pressed).
pub(crate) fn publish(cx: &App) {
    if !holds_login() {
        return;
    }
    let Some(data_dir) = cx.try_global::<AuthContext>().map(|auth| auth.data_dir.clone()) else { return };
    let current = crate::enet_wiring::trusted(cx).map(|(account, keeper)| {
        let names: Vec<(String, String)> = Store::try_global(cx)
            .map(|store| store.collections().users.read(cx).iter().filter_map(|user| Some((user.id.clone(), user.name.clone()?))).collect())
            .unwrap_or_default();
        let name_of = |user: &str| names.iter().find(|(id, _)| id == user).map(|(_, name)| name.clone());
        let failed = FAILED.with(|f| f.borrow().clone());
        (account, install_state(&keeper.snapshot(), &|row| crate::install_approval::row_title(row, &name_of), failed))
    });
    WRITTEN.with(|written| {
        let mut written = written.borrow_mut();
        if *written == current {
            return;
        }
        if let Some((account, _)) = written.as_ref().filter(|(account, _)| current.as_ref().map(|(a, _)| a) != Some(account)) {
            let _ = std::fs::remove_file(api::holder::install_state_path(&data_dir, account));
        }
        if let Some((account, state)) = &current {
            if let Err(err) = state.write(&api::holder::install_state_path(&data_dir, account)) {
                log::warn!("[holder] the install's state for the shell: {err}");
                return;
            }
        }
        *written = current;
    });
}

fn runtime_path(name: &str) -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    Some(std::path::Path::new(&dir).join(name))
}

/// The shell's Connect socket (it exists while a shell that knows it runs).
fn connect_socket() -> Option<std::path::PathBuf> {
    runtime_path(api::holder::CONNECT_SOCKET)
}

/// A sign-in's return the app got as the scheme's handler is the shell's
/// Connect's (`own_attempt`: one of the app's own is pending, it stays here).
#[cfg(unix)]
pub(crate) fn forward_return(url: &str, callback: &api::login::OAuthCallback, own_attempt: bool) -> bool {
    use api::login::OAuthCallback;
    if own_attempt || matches!(callback, OAuthCallback::Linked(_)) || !shell_signs_in() {
        return false;
    }
    let Some(path) = runtime_path(api::holder::URL_SOCKET) else { return false };
    std::os::unix::net::UnixDatagram::unbound().and_then(|sock| sock.send_to(url.as_bytes(), path)).is_ok()
}

#[cfg(not(unix))]
pub(crate) fn forward_return(_: &str, _: &api::login::OAuthCallback, _: bool) -> bool {
    false
}

/// The shell signs in (its Connect): the app's own login is not shown.
pub(crate) fn shell_signs_in() -> bool {
    holds_login() && connect_socket().is_some_and(|path| path.exists())
}

/// Opens the shell's Connect.
#[cfg(unix)]
pub(crate) fn open_connect() -> std::io::Result<()> {
    let path = connect_socket().ok_or_else(|| std::io::Error::other("no session"))?;
    std::os::unix::net::UnixDatagram::unbound()?.send_to(api::holder::CONNECT_OPEN.as_bytes(), path).map(|_| ())
}

#[cfg(not(unix))]
pub(crate) fn open_connect() -> std::io::Result<()> {
    Err(std::io::Error::other("Exponential Desktop only"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_is_told_the_rows_or_the_waiting_code() {
        let failed = Some(FailedPress { seq: 2, verb: "approve".into(), message: "offline".into() });
        let rows = install_state(&crate::install_approval::specimen_snapshot("install-approval"), &|row| row.label.clone().unwrap_or_default(), failed.clone());
        assert_eq!((rows.waiting.clone(), rows.failed), (None, failed));
        let row = &rows.rows[0];
        assert_eq!((row.id.as_str(), row.title.as_str(), row.code.as_str(), row.badge.as_str()), ("install-N", "Pixel 9", "HNMBN A0319", "New install"));
        assert_eq!(rows.rows[1].badge, "Reset");
        let waiting = install_state(&crate::install_approval::specimen_snapshot("install-waiting"), &|_| String::new(), None);
        assert_eq!(waiting, InstallState { waiting: Some("HNMBN A0319".into()), ..Default::default() });
    }

    /// The press names the install and the code the person compared: one
    /// without the other approves nothing.
    #[test]
    fn a_press_needs_the_install_and_its_code() {
        let rows = crate::install_approval::specimen_snapshot("install-approval").rows;
        assert_eq!(pressed(&rows, "install-N", "HNMBN A0319"), Some(rows[0].clone()));
        assert_eq!(pressed(&rows, "install-N", &rows[1].code), None);
        assert_eq!(pressed(&rows, "other", "HNMBN A0319"), None);
    }
}
