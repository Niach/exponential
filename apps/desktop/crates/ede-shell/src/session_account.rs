//! #EDE-226: the eDE login's ONE Exponential account. The shell signs in
//! (Connect) and out (Settings → Account); the holder, the app started by
//! `exponential-device.service`, keeps the install and its keys
//! (`api::holder`: what they say to each other).
//!
//! - Sign out goes to the holder: it ends the account's runs, revokes the
//!   install and removes token, keys and stores. With no holder (or one that
//!   does not take it) the shell does it itself. The shell's own sync store
//!   goes with the account's entry ([`wipe_when_forgotten`]).
//! - Install approval: the holder's state file drawn as `/installs`
//!   (install-trust.json's parts in Needs you, Connect and Settings); a press
//!   goes back to the holder with the code the person compared.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use api::accounts::Account;
use api::holder::InstallState;
use api::AuthStore;
use serde_json::{json, Value};

/// The login's account among the signed-in ones: the one the picked team
/// belongs to, else the first (the app's rule, `ui::session::pick_account`).
pub fn session_account(signed_in: Vec<Account>, picked: Option<&str>) -> Option<Account> {
    let at = picked.and_then(|id| signed_in.iter().position(|a| a.id == id)).unwrap_or(0);
    signed_in.into_iter().nth(at)
}

/// How long the holder has to take a sign-out (its token goes at once).
const HOLDER_TAKES: Duration = Duration::from_secs(6);

/// Waits for the holder to sign `account` out, else does it here: revokes
/// the install(s) of this machine, the session, then everything of the
/// account's. BLOCKING. `true` = done here.
pub fn sign_out(auth: &Arc<AuthStore>, data_dir: &Path, account: &Account, asked: bool) -> bool {
    let deadline = std::time::Instant::now() + HOLDER_TAKES;
    while asked && std::time::Instant::now() < deadline {
        if auth.token(&account.id).is_none() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let Some(token) = auth.token(&account.id) else { return false };
    tracing::info!(account = %account.email, asked, "no holder took the sign-out: signed out here");
    let trpc = api::TrpcClient::new(&account.instance_url, Arc::new(api::StaticToken(token.clone())));
    let home = data_dir.join("accounts").join(&account.id);
    for identity in ["enet-identity", "cli-enet-identity"] {
        api::installs::revoke_stored(&trpc, &home.join(identity));
    }
    if let Err(e) = api::AuthClient::new().sign_out(&account.instance_url, &token) {
        tracing::warn!("the server-side sign-out failed (continuing here): {e}");
    }
    auth.sign_out(&account.id);
    auth.forget_account(&account.id, data_dir);
    true
}

/// The shell's own sync store of a signed-out account goes once its entry
/// left `accounts.json` (a deliberate sign-out, here or in the app); an
/// expired one keeps both for the next sign-in.
pub fn wipe_when_forgotten(auth: Arc<AuthStore>, shell_data: PathBuf, account_id: String) {
    let spawned = std::thread::Builder::new().name("ede-shell-sign-out".into()).spawn(move || {
        for _ in 0..60 {
            if auth.token(&account_id).is_some() {
                return;
            }
            if auth.account(&account_id).is_none() {
                let Some(dir) = api::account_db_path(&shell_data, &account_id).parent().map(Path::to_path_buf) else { return };
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => tracing::info!(dir = %dir.display(), "the signed-out account's store is removed"),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!(dir = %dir.display(), "the signed-out account's store: {e}"),
                }
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
            auth.reload();
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("sign-out: {e}");
    }
}

/// The holder's state file of the login's account, read when it changed.
#[derive(Default)]
pub struct Installs {
    path: Option<PathBuf>,
    /// Its time and its inode (each write is a new file; two can share a
    /// clock tick).
    modified: Option<(SystemTime, u64)>,
    state: InstallState,
    /// The last failed press already told.
    told: u64,
}

impl Installs {
    /// Looks at the file of `account`; `true` = what is drawn changed.
    pub fn follow(&mut self, data_dir: &Path, account: Option<&str>) -> bool {
        let path = account.map(|id| api::holder::install_state_path(data_dir, id));
        let modified = path.as_ref().and_then(|p| {
            let meta = std::fs::metadata(p).ok()?;
            Some((meta.modified().ok()?, std::os::unix::fs::MetadataExt::ino(&meta)))
        });
        if path == self.path && modified == self.modified {
            return false;
        }
        // A press that failed before the shell looked is not told again.
        let first = path != self.path;
        self.path = path;
        self.modified = modified;
        let state = self.path.as_ref().filter(|_| modified.is_some()).and_then(|p| InstallState::read(p)).unwrap_or_default();
        if first {
            self.told = state.failed.as_ref().map_or(0, |f| f.seq);
        }
        let changed = state != self.state;
        self.state = state;
        changed
    }

    /// A press that failed since the last look: (verb, message).
    pub fn failed(&mut self) -> Option<(String, String)> {
        let failed = self.state.failed.clone()?;
        if failed.seq == self.told {
            return None;
        }
        self.told = failed.seq;
        Some((failed.verb, failed.message))
    }

    pub fn count(&self) -> usize {
        self.state.rows.len()
    }

    /// `/installs`.
    pub fn feed(&self) -> Value {
        json!({
            "waiting": self.state.waiting.is_some(),
            "code": self.state.waiting.clone().unwrap_or_default(),
            "count": self.state.rows.len(),
            "rows": self.state.rows.iter().map(|r| json!({"id": r.id, "title": r.title, "code": r.code, "badge": r.badge})).collect::<Vec<_>>(),
        })
    }

    /// The goldens: `EDE_SHELL_DEMO_INSTALLS=rows|waiting`.
    pub fn demo() -> Self {
        let row = |id: &str, title: &str, code: &str, badge: &str| api::holder::InstallRow { id: id.into(), title: title.into(), code: code.into(), badge: badge.into() };
        let state = match std::env::var("EDE_SHELL_DEMO_INSTALLS").as_deref() {
            Ok("rows") => InstallState { rows: vec![row("n", "Pixel 9", "HNMBN A0319", "New install"), row("t3", "Mira · Mira's new iPhone", "776AF T3NNF", "Reset")], ..Default::default() },
            Ok("waiting") => InstallState { waiting: Some("HNMBN A0319".into()), ..Default::default() },
            _ => InstallState::default(),
        };
        Self { state, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::holder::{install_state_path, FailedPress, InstallRow};
    use api::login::AuthUser;

    fn auth(data: &Path) -> Arc<AuthStore> {
        AuthStore::load_with_token_store(data.to_path_buf(), api::TokenStore::file_only(data.to_path_buf()))
    }

    fn user(id: &str) -> AuthUser {
        AuthUser { id: id.into(), email: format!("{id}@example.com"), name: None, is_admin: None, onboarding_completed_at: None }
    }

    #[test]
    fn one_account_the_picked_teams_else_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth(dir.path());
        let first = auth.sign_in("http://localhost:1", "t1", &user("user-1")).unwrap();
        let second = auth.sign_in("http://localhost:1", "t2", &user("user-2")).unwrap();
        let pick = |id: Option<&str>| session_account(auth.signed_in_accounts(), id).map(|a| a.id);
        assert_eq!(pick(None), Some(first.id.clone()));
        assert_eq!(pick(Some(&second.id)), Some(second.id.clone()));
        assert_eq!(pick(Some("signed-out")), Some(first.id));
        assert_eq!(session_account(Vec::new(), None), None);
    }

    /// No holder: the shell signs out itself (the server is unreachable here,
    /// a sign-out offline is legal) and nothing of the account stays.
    #[test]
    fn without_a_holder_the_shell_signs_out_itself() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("exponential");
        let auth = auth(&data);
        let account = auth.sign_in("http://127.0.0.1:1", "tok", &user("user-1")).unwrap();
        let home = data.join("accounts").join(&account.id);
        std::fs::write(home.join("enet-identity"), "07".repeat(32)).unwrap();
        std::fs::write(home.join("seal-keys"), "k").unwrap();
        let shell_data = dir.path().join("shell");
        let own = api::account_db_path(&shell_data, &account.id);
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, "db").unwrap();
        assert!(sign_out(&auth, &data, &account, false));
        assert!(auth.accounts().is_empty() && auth.signed_in_accounts().is_empty());
        assert!(!home.exists());
        wipe_when_forgotten(auth.clone(), shell_data, account.id.clone());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while own.exists() {
            assert!(std::time::Instant::now() < deadline, "the shell's store stayed");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The holder took it (its sign-out empties the token): the shell does nothing.
    #[test]
    fn a_sign_out_the_holder_took_is_not_done_twice() {
        let dir = tempfile::tempdir().unwrap();
        let shell = auth(dir.path());
        let account = shell.sign_in("http://127.0.0.1:1", "tok", &user("user-1")).unwrap();
        let _watch = shell.watch(Duration::from_millis(20));
        let holder = auth(dir.path());
        std::thread::spawn({
            let id = account.id.clone();
            move || {
                std::thread::sleep(Duration::from_millis(300));
                holder.sign_out(&id);
            }
        });
        assert!(!sign_out(&shell, dir.path(), &account, true));
        assert!(dir.path().join("accounts").join(&account.id).exists(), "the holder's to remove");
    }

    #[test]
    fn the_installs_follow_the_holders_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = install_state_path(dir.path(), "a1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut installs = Installs::default();
        assert!(!installs.follow(dir.path(), None));
        assert!(!installs.follow(dir.path(), Some("a1")), "no file: nothing drawn");
        let mut state = InstallState { rows: vec![InstallRow { id: "i".into(), title: "Pixel 9".into(), code: "HNMBN A0319".into(), badge: "New install".into() }], ..Default::default() };
        state.write(&path).unwrap();
        assert!(installs.follow(dir.path(), Some("a1")));
        assert_eq!(installs.feed(), json!({"waiting": false, "code": "", "count": 1, "rows": [{"id": "i", "title": "Pixel 9", "code": "HNMBN A0319", "badge": "New install"}]}));
        assert_eq!(installs.failed(), None);
        state.failed = Some(FailedPress { seq: 1, verb: "approve".into(), message: "offline".into() });
        state.write(&path).unwrap();
        installs.follow(dir.path(), Some("a1"));
        assert_eq!(installs.failed(), Some(("approve".into(), "offline".into())));
        assert_eq!(installs.failed(), None, "told once");
        std::fs::remove_file(&path).unwrap();
        assert!(installs.follow(dir.path(), Some("a1")), "the holder stopped");
        assert_eq!(installs.count(), 0);
    }
}
