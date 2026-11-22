//! Sealed chats (#EDE-65 lane I, EXP-1458): the team chat opens sealed
//! bodies, cards and titles with the tree secrets the Exponential app on
//! this machine already holds: its per-account keyring file
//! (`api::seal_keyring_path`, written by the app's key holder; read here,
//! never written) through `sync::sealed::Opener`, the code the app's own
//! ingest opens rows with. A key the app fetches later reaches the chat on
//! the next read (the file is read again when it changed).
//!
//! #EDE-194: the agent spaces' titles open with every keyring the account's
//! installs keep on this machine ([`space_opener`]): the app's and the CLI
//! daemon's (`cli-seal-keys`). A space's session runs on this machine, so
//! its host, the app or the daemon, holds the key it sealed the title with.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use domain::rows::{MessageRow, SessionRow};
use serde_json::Value;
use sync::sealed::{OpenedBody, OpenedCard, Opener, Opens};

type Secrets = BTreeMap<String, BTreeMap<u32, [u8; 32]>>;

/// The app's keyring file of one account, read again when it changes.
pub struct KeyringFile {
    path: PathBuf,
    state: RwLock<(Option<SystemTime>, Arc<Secrets>)>,
}

impl KeyringFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path, state: RwLock::new((None, Arc::new(Secrets::new()))) }
    }

    /// The secrets as the file holds them now (unchanged file = no read).
    fn secrets(&self) -> Arc<Secrets> {
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        {
            let state = self.state.read().unwrap_or_else(|p| p.into_inner());
            if state.0.is_some() && state.0 == modified {
                return state.1.clone();
            }
        }
        let secrets = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| enet_core::sealed::keyring_secrets(&text).map_err(|e| tracing::warn!("{}: {e}", self.path.display())).ok())
            .unwrap_or_default();
        let secrets = Arc::new(secrets);
        *self.state.write().unwrap_or_else(|p| p.into_inner()) = (modified, secrets.clone());
        secrets
    }

    fn cached(&self, root_id: &str) -> impl Fn(u32) -> Option<[u8; 32]> {
        let secrets = self.secrets();
        let root = root_id.to_string();
        move |epoch| secrets.get(&root)?.get(&epoch).copied()
    }
}

impl Opens for KeyringFile {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
        enet_core::sealed::open_body(body, &self.cached(root_id))
    }
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
        enet_core::sealed::open_card(card, &self.cached(root_id))
    }
    fn open_title(&self, row: &SessionRow) -> Option<String> {
        let sealed = sealed_title(row)?;
        opened_title(enet_core::sealed::open_body(sealed, &self.cached(row.root_id_or_self())))
    }
}

/// A row's sealed title (an envelope), if it has one.
fn sealed_title(row: &SessionRow) -> Option<&str> {
    row.sealed_title.as_deref().filter(|title| !title.is_empty())
}

/// An opened title's text (`None` = it did not open: keep `title`).
fn opened_title(opened: OpenedBody) -> Option<String> {
    opened.text.filter(|_| matches!(opened.status, sync::sealed::OpenStatus::Opened | sync::sealed::OpenStatus::Plain))
}

/// #EDE-194: several keyring files of one account as one: a tree's secret
/// from whichever holds it.
pub struct Keyrings(Vec<KeyringFile>);

impl Keyrings {
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self(paths.into_iter().map(KeyringFile::new).collect())
    }

    fn cached(&self, root_id: &str) -> impl Fn(u32) -> Option<[u8; 32]> {
        let secrets: Vec<Arc<Secrets>> = self.0.iter().map(KeyringFile::secrets).collect();
        let root = root_id.to_string();
        move |epoch| secrets.iter().find_map(|s| s.get(&root)?.get(&epoch).copied())
    }
}

impl Opens for Keyrings {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
        enet_core::sealed::open_body(body, &self.cached(root_id))
    }
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
        enet_core::sealed::open_card(card, &self.cached(root_id))
    }
    fn open_title(&self, row: &SessionRow) -> Option<String> {
        let sealed = sealed_title(row)?;
        opened_title(enet_core::sealed::open_body(sealed, &self.cached(row.root_id_or_self())))
    }
}

/// An account's opener over the app's keyring.
pub fn opener(account_id: &str) -> Opener {
    Opener::new(Arc::new(KeyringFile::new(keyring_path(account_id))))
}

/// #EDE-194: the opener of the agent spaces' titles: every keyring of the
/// account on this machine ([`space_keyring_paths`]).
pub fn space_opener(account_id: &str) -> Opener {
    Opener::new(Arc::new(Keyrings::new(space_keyring_paths(&api::default_data_dir(), account_id))))
}

/// The app's keyring file of an account.
pub fn keyring_path(account_id: &str) -> PathBuf {
    api::seal_keyring_path(&api::default_data_dir(), account_id, false)
}

/// #EDE-194: every keyring the account's installs keep in `data_dir`: the
/// app's (`seal-keys`) and the CLI daemon's (`cli-seal-keys`).
pub fn space_keyring_paths(data_dir: &std::path::Path, account_id: &str) -> [PathBuf; 2] {
    [api::seal_keyring_path(data_dir, account_id, false), api::seal_keyring_path(data_dir, account_id, true)]
}

/// The rows the chat reads, opened in place: the sessions first (the
/// opener learns each session's root), then the messages.
#[cfg(test)]
pub fn open(opener: &Opener, sessions: &mut [SessionRow], messages: &mut [MessageRow]) {
    for session in sessions.iter_mut() {
        opener.open_session(session);
    }
    for message in messages.iter_mut() {
        opener.open_message(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `sealed-content.json`: its epoch keys and its body cases.
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap()
    }

    fn keyring(dir: &std::path::Path, root: &str, fx: &Value) -> PathBuf {
        let keys: serde_json::Map<String, Value> = fx["keys"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(epoch, hex)| {
                let bytes = data_encoding::HEXLOWER.decode(hex.as_str().unwrap().as_bytes()).unwrap();
                (epoch.clone(), json!(enet_core::sealed::b64url_encode(&bytes)))
            })
            .collect();
        let path = dir.join("seal-keys");
        std::fs::write(&path, json!({"v": 1, "keys": {root: keys}}).to_string()).unwrap();
        path
    }

    fn case<'a>(fx: &'a Value, name: &str) -> &'a Value {
        fx["bodyCases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()
    }

    fn message(body: &str, root: &str) -> MessageRow {
        serde_json::from_value(json!({"id": "m1", "conversation_id": "c1", "root_id": root, "author_kind": "agent", "body": body})).unwrap()
    }

    #[test]
    fn a_sealed_body_opens_with_the_app_s_keyring_and_stays_locked_without() {
        let fx = fixture();
        let dir = tempfile::tempdir().unwrap();
        let path = keyring(dir.path(), "r1", &fx);
        let opener = Opener::new(Arc::new(KeyringFile::new(path)));
        let sealed = case(&fx, "sealed, epoch 1");
        let mut sessions: Vec<SessionRow> = vec![serde_json::from_value(json!({"id": "r1"})).unwrap()];
        let mut messages = vec![message(sealed["body"].as_str().unwrap(), "r1"), message(sealed["body"].as_str().unwrap(), "r-unknown")];
        open(&opener, &mut sessions, &mut messages);
        assert_eq!(messages[0].body.as_deref(), sealed["text"].as_str());
        assert!(messages[0].locked().is_none());
        assert!(messages[1].locked().is_some(), "no key for that tree: the locked pill");
    }

    #[test]
    fn a_key_written_later_opens_on_the_next_read() {
        let fx = fixture();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seal-keys");
        let file = Arc::new(KeyringFile::new(path.clone()));
        let opener = Opener::new(file.clone());
        let body = case(&fx, "sealed, epoch 1")["body"].as_str().unwrap().to_string();
        let mut first = vec![message(&body, "r1")];
        open(&opener, &mut [], &mut first);
        assert!(first[0].locked().is_some());
        std::thread::sleep(std::time::Duration::from_millis(20));
        keyring(dir.path(), "r1", &fx);
        let mut again = vec![message(&body, "r1")];
        open(&opener, &mut [], &mut again);
        assert!(again[0].locked().is_none(), "{:?}", again[0].body);
    }

    /// #EDE-194: the spaces' opener reads the app's and the CLI daemon's
    /// keyrings of the account; a tree's secret opens from either.
    #[test]
    fn the_spaces_open_with_the_app_s_and_the_daemon_s_keyrings() {
        let fx = fixture();
        let dir = tempfile::tempdir().unwrap();
        let paths = space_keyring_paths(dir.path(), "a1");
        assert!(paths[0].ends_with("accounts/a1/seal-keys") && paths[1].ends_with("accounts/a1/cli-seal-keys"), "{paths:?}");
        let daemon_dir = dir.path().join("daemon");
        std::fs::create_dir_all(&daemon_dir).unwrap();
        let held = keyring(&daemon_dir, "r1", &fx);
        let keys = Keyrings::new([dir.path().join("missing"), held]);
        let sealed = case(&fx, "sealed, epoch 1");
        let body = sealed["body"].as_str().unwrap();
        assert_eq!(keys.open_body("r1", body).text.as_deref(), sealed["text"].as_str(), "the second file holds the tree");
        let row: SessionRow = serde_json::from_value(json!({"id": "r1", "sealed_title": body})).unwrap();
        assert_eq!(keys.open_title(&row).as_deref(), sealed["text"].as_str());
        let other: SessionRow = serde_json::from_value(json!({"id": "r2", "sealed_title": body})).unwrap();
        assert_eq!(keys.open_title(&other), None, "no keyring holds that tree");
        assert_eq!(Keyrings::new(Vec::new()).open_title(&row), None);
    }

    #[test]
    fn plaintext_passes_and_the_path_is_the_app_s() {
        let opener = Opener::new(Arc::new(KeyringFile::new(PathBuf::from("/nonexistent/seal-keys"))));
        let mut messages = vec![message("Hello", "r1")];
        open(&opener, &mut [], &mut messages);
        assert_eq!((messages[0].body.as_deref(), messages[0].locked()), (Some("Hello"), None));
        assert!(api::seal_keyring_path(std::path::Path::new("/d"), "a1", false).ends_with("accounts/a1/seal-keys"));
    }
}
