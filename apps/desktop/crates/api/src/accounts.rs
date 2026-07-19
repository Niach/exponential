//! Signed-in account set + session-token lifecycle (masterplan-v3 §5.7,
//! §5.10) — the desktop analogue of iOS `AccountStore`/`AuthRepository`.
//!
//! Multi-account is first-class: an account is one (instance URL, user)
//! pair. Non-secret metadata persists in `{data_dir}/accounts.json`; the
//! session token lives in the [`TokenStore`] (0600 file store) and is
//! mirrored in memory so [`TokenProvider`] reads are lock-cheap and
//! **call-time** (§5.7: a re-login updates every in-flight loop's next
//! request).
//!
//! This module is also the **401→reauth signal surface** the sync engine
//! consumes: when any shape thread or tRPC call gets a hard
//! 401, the pipeline owner calls [`AuthStore::handle_unauthorized`], which
//! deletes the stored session token and emits
//! [`AuthEvent::Unauthorized`] — the app shell drains [`AuthStore::events`]
//! and routes that account to the login screen. Never an empty board, never
//! an anonymous retry.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::error::ApiError;
use crate::login::{normalize_instance_url, AuthUser};
use crate::token_store::{SecretKind, TokenStore};
use crate::TokenProvider;

/// One signed-in (or signed-out-but-remembered) server account. Non-secret —
/// this struct is what `accounts.json` persists; the token never rides here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Stable id derived from (instance URL, user id) —
    /// [`account_id_for`]. Filesystem-safe: it names the per-account dirs
    /// (`accounts/{id}/sync-v2.sqlite`) and token files.
    pub id: String,
    /// Normalized instance base URL (`https://app.exponential.at`).
    pub instance_url: String,
    /// Better Auth user id (text, not UUID).
    pub user_id: String,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_admin: bool,
    /// ISO timestamp when onboarding finished; `None` gates the wizard.
    #[serde(default)]
    pub onboarding_completed_at: Option<String>,
}

/// Derive the stable, filesystem-safe account id for (instance URL, user id).
/// E.g. `app.exponential.at-8Jk3…` — readable in `accounts/` and unique per
/// server+user (the same user on two instances is two accounts, like iOS).
pub fn account_id_for(instance_url: &str, user_id: &str) -> String {
    let base = normalize_instance_url(instance_url);
    let host = base
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let sanitize = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    format!("{}--{}", sanitize(host), sanitize(user_id))
}

/// Auth lifecycle events, drained by ONE app-shell foreground task
/// (flume is MPMC — each event goes to a single consumer; don't fan out).
#[derive(Clone, Debug, PartialEq)]
pub enum AuthEvent {
    /// A (re-)login completed; the SyncManager reconcile spawns this
    /// account's 15 shape threads (§5.10).
    SignedIn { account_id: String },
    /// User-initiated sign-out; threads stop, SQLite stays on disk.
    SignedOut { account_id: String },
    /// The session token was rejected (hard 401). The token is
    /// already deleted; the UI must route this account to the login screen.
    Unauthorized { account_id: String },
}

#[derive(Default, Serialize, Deserialize)]
struct AccountsFile {
    #[serde(default)]
    accounts: Vec<Account>,
}

struct State {
    accounts: Vec<Account>,
    /// In-memory token mirror — the call-time source for [`TokenProvider`].
    tokens: HashMap<String, String>,
}

/// The account/session store. gpui-free; `Arc`-shared across the app shell,
/// the tRPC clients, and the sync manager's token providers.
pub struct AuthStore {
    state: RwLock<State>,
    token_store: TokenStore,
    accounts_path: PathBuf,
    events_tx: flume::Sender<AuthEvent>,
    events_rx: flume::Receiver<AuthEvent>,
}

impl AuthStore {
    /// Production store: file-based secrets under `data_dir`
    /// ([`crate::default_data_dir`]).
    pub fn load(data_dir: PathBuf) -> Arc<Self> {
        let token_store = TokenStore::new(data_dir.clone());
        Self::load_with_token_store(data_dir, token_store)
    }

    /// Seam for tests / headless deployments (pass
    /// [`TokenStore::file_only`]).
    pub fn load_with_token_store(data_dir: PathBuf, token_store: TokenStore) -> Arc<Self> {
        let accounts_path = data_dir.join("accounts.json");
        let accounts = fs::read_to_string(&accounts_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<AccountsFile>(&raw).ok())
            .map(|f| f.accounts)
            .unwrap_or_default();

        // Hydrate tokens from the secret store. Unbounded on purpose: at app
        // start, waiting out a Keychain prompt beats booting to a spurious
        // login screen (the ~2s bound of §7.2 is for the launcher hot path).
        let mut tokens = HashMap::new();
        for account in &accounts {
            if let Some(token) = token_store.get(&account.id, SecretKind::SessionToken) {
                tokens.insert(account.id.clone(), token);
            }
        }

        let (events_tx, events_rx) = flume::unbounded();
        Arc::new(Self {
            state: RwLock::new(State { accounts, tokens }),
            token_store,
            accounts_path,
            events_tx,
            events_rx,
        })
    }

    /// Snapshot of the remembered accounts.
    pub fn accounts(&self) -> Vec<Account> {
        self.state.read().unwrap().accounts.clone()
    }

    /// One account by id.
    pub fn account(&self, account_id: &str) -> Option<Account> {
        self.state
            .read()
            .unwrap()
            .accounts
            .iter()
            .find(|a| a.id == account_id)
            .cloned()
    }

    /// Accounts that currently hold a session token — the signed-in set the
    /// SyncManager reconciles against (§5.10).
    pub fn signed_in_accounts(&self) -> Vec<Account> {
        let state = self.state.read().unwrap();
        state
            .accounts
            .iter()
            .filter(|a| state.tokens.contains_key(&a.id))
            .cloned()
            .collect()
    }

    /// The current session token for an account (call-time read).
    pub fn token(&self, account_id: &str) -> Option<String> {
        self.state.read().unwrap().tokens.get(account_id).cloned()
    }

    /// The shared secret store (the coding launcher reads the personal
    /// key through this — see [`crate::users`]).
    pub fn token_store(&self) -> &TokenStore {
        &self.token_store
    }

    /// A [`TokenProvider`] bound to one account, reading the token **at call
    /// time** (§5.7).
    pub fn token_provider(self: &Arc<Self>, account_id: &str) -> Arc<dyn TokenProvider> {
        let store = Arc::clone(self);
        let id = account_id.to_string();
        Arc::new(move || store.token(&id))
    }

    /// The §5.7 plain-closure form the sync crate consumes
    /// (`token_provider: Arc<dyn Fn() -> Option<String> + Send + Sync>` —
    /// sync must not depend on `api`, §3.1 dependency direction).
    pub fn token_provider_fn(
        self: &Arc<Self>,
        account_id: &str,
    ) -> Arc<dyn Fn() -> Option<String> + Send + Sync> {
        let store = Arc::clone(self);
        let id = account_id.to_string();
        Arc::new(move || store.token(&id))
    }

    /// Plain-closure 401 reporter for the sync pipeline: the shape thread
    /// that hits a hard 401 calls this with its account id (§5.6b).
    pub fn unauthorized_handler_fn(self: &Arc<Self>) -> Arc<dyn Fn(&str) + Send + Sync> {
        let store = Arc::clone(self);
        Arc::new(move |account_id: &str| store.handle_unauthorized(account_id))
    }

    /// Record a completed sign-in (password or OAuth): upsert the account,
    /// persist metadata, store the token (file + memory), emit
    /// [`AuthEvent::SignedIn`]. Returns the account.
    pub fn sign_in(
        &self,
        instance_url: &str,
        token: &str,
        user: &AuthUser,
    ) -> Result<Account, ApiError> {
        let instance_url = normalize_instance_url(instance_url);
        let account = Account {
            id: account_id_for(&instance_url, &user.id),
            instance_url,
            user_id: user.id.clone(),
            email: user.email.clone(),
            name: user.name.clone(),
            is_admin: user.is_admin.unwrap_or(false),
            onboarding_completed_at: user.onboarding_completed_at.clone(),
        };

        self.token_store
            .set(&account.id, SecretKind::SessionToken, token)?;

        {
            let mut state = self.state.write().unwrap();
            state.tokens.insert(account.id.clone(), token.to_string());
            match state.accounts.iter_mut().find(|a| a.id == account.id) {
                Some(existing) => {
                    // Preserve a previously-known onboarding flag so a login
                    // response that omits it doesn't re-trigger the wizard
                    // (iOS AuthRepository parity).
                    let onboarding = account
                        .onboarding_completed_at
                        .clone()
                        .or_else(|| existing.onboarding_completed_at.clone());
                    *existing = Account {
                        onboarding_completed_at: onboarding,
                        ..account.clone()
                    };
                }
                None => state.accounts.push(account.clone()),
            }
            self.persist_locked(&state);
        }

        let _ = self.events_tx.send(AuthEvent::SignedIn {
            account_id: account.id.clone(),
        });
        Ok(account)
    }

    /// User-initiated sign-out (§5.10): drop the session token (memory +
    /// secret store), keep the account metadata AND its on-disk sync DB for
    /// offline resume, emit [`AuthEvent::SignedOut`]. Server-side revocation
    /// (`AuthClient::sign_out`) is the caller's best-effort extra.
    pub fn sign_out(&self, account_id: &str) {
        let had_token = {
            let mut state = self.state.write().unwrap();
            state.tokens.remove(account_id).is_some()
        };
        self.token_store.delete(account_id, SecretKind::SessionToken);
        if had_token {
            let _ = self.events_tx.send(AuthEvent::SignedOut {
                account_id: account_id.to_string(),
            });
        }
    }

    /// Full removal ("Delete local data" is separate and explicit, §5.10 —
    /// this removes the account entry + every secret, not the sync DB).
    pub fn remove_account(&self, account_id: &str) {
        let had_token = {
            let mut state = self.state.write().unwrap();
            let had = state.tokens.remove(account_id).is_some();
            state.accounts.retain(|a| a.id != account_id);
            self.persist_locked(&state);
            had
        };
        self.token_store.delete_all(account_id);
        if had_token {
            let _ = self.events_tx.send(AuthEvent::SignedOut {
                account_id: account_id.to_string(),
            });
        }
    }

    /// The hard-401 path (§5.6b, §5.7): delete the stored
    /// session token, clear the in-memory mirror, emit
    /// [`AuthEvent::Unauthorized`] exactly once (15 shape threads may all
    /// 401 at the same instant; only the first caller finds a token to
    /// clear). The account metadata stays so login can prefill.
    pub fn handle_unauthorized(&self, account_id: &str) {
        let had_token = {
            let mut state = self.state.write().unwrap();
            state.tokens.remove(account_id).is_some()
        };
        if had_token {
            self.token_store.delete(account_id, SecretKind::SessionToken);
            let _ = self.events_tx.send(AuthEvent::Unauthorized {
                account_id: account_id.to_string(),
            });
        }
    }

    /// The event stream. Drain from ONE foreground task (flume is MPMC:
    /// cloned receivers steal, they don't broadcast).
    pub fn events(&self) -> flume::Receiver<AuthEvent> {
        self.events_rx.clone()
    }

    fn persist_locked(&self, state: &State) {
        if let Some(dir) = self.accounts_path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let file = AccountsFile {
            accounts: state.accounts.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&file) {
            let _ = fs::write(&self.accounts_path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-auth-test-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_store(dir: &TempDir) -> Arc<AuthStore> {
        AuthStore::load_with_token_store(dir.0.clone(), TokenStore::file_only(dir.0.clone()))
    }

    fn user() -> AuthUser {
        AuthUser {
            id: "user-1".to_string(),
            email: "danny@example.com".to_string(),
            name: Some("Danny".to_string()),
            is_admin: Some(false),
            onboarding_completed_at: Some("2026-01-01T00:00:00.000Z".to_string()),
        }
    }

    #[test]
    fn account_id_is_stable_and_filesystem_safe() {
        let id = account_id_for("https://app.exponential.at/", "u_8Jk3");
        assert_eq!(id, "app.exponential.at--u-8Jk3");
        // Scheme/trailing-slash variants collapse to the same id.
        assert_eq!(id, account_id_for("app.exponential.at", "u_8Jk3"));
        // Separators can never smuggle a path segment into accounts/{id}/.
        let hostile = account_id_for("http://localhost:5173", "../../etc");
        assert!(!hostile.contains('/'));
        assert_eq!(hostile, "localhost-5173--..-..-etc");
    }

    #[test]
    fn sign_in_persists_and_token_provider_reads_call_time() {
        let dir = TempDir::new("signin");
        let store = test_store(&dir);
        let account = store
            .sign_in("app.exponential.at", "tok-A", &user())
            .unwrap();

        assert_eq!(store.token(&account.id).as_deref(), Some("tok-A"));
        assert_eq!(store.signed_in_accounts().len(), 1);

        let provider = store.token_provider_fn(&account.id);
        assert_eq!(provider().as_deref(), Some("tok-A"));

        // Re-login rotates the token; the SAME provider sees it (call-time).
        store
            .sign_in("app.exponential.at", "tok-B", &user())
            .unwrap();
        assert_eq!(provider().as_deref(), Some("tok-B"));

        // Events: two sign-ins.
        let events = store.events();
        assert_eq!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            AuthEvent::SignedIn {
                account_id: account.id.clone()
            }
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            AuthEvent::SignedIn {
                account_id: account.id.clone()
            }
        );
    }

    #[test]
    fn restart_rehydrates_accounts_and_tokens() {
        let dir = TempDir::new("restart");
        let account_id = {
            let store = test_store(&dir);
            store
                .sign_in("app.exponential.at", "tok-A", &user())
                .unwrap()
                .id
        };
        // Fresh store over the same dir = app restart.
        let store = test_store(&dir);
        assert_eq!(store.accounts().len(), 1);
        assert_eq!(store.token(&account_id).as_deref(), Some("tok-A"));
        assert_eq!(
            store.account(&account_id).unwrap().email,
            "danny@example.com"
        );
    }

    #[test]
    fn handle_unauthorized_clears_token_and_emits_once() {
        // Dead token → token gone + ONE Unauthorized event,
        // even when all 15 shape threads report simultaneously.
        let dir = TempDir::new("unauth");
        let store = test_store(&dir);
        let account = store
            .sign_in("app.exponential.at", "tok-A", &user())
            .unwrap();
        let events = store.events();
        let _ = events.recv_timeout(Duration::from_secs(1)).unwrap(); // SignedIn

        let handler = store.unauthorized_handler_fn();
        for _ in 0..15 {
            handler(&account.id);
        }

        assert_eq!(store.token(&account.id), None);
        assert_eq!(store.signed_in_accounts().len(), 0);
        // Token also gone from the secret store (not just memory).
        assert_eq!(
            store
                .token_store()
                .get(&account.id, SecretKind::SessionToken),
            None
        );
        // Exactly one event.
        assert_eq!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            AuthEvent::Unauthorized {
                account_id: account.id.clone()
            }
        );
        assert!(events.try_recv().is_err());
        // Account metadata survives for login prefill.
        assert!(store.account(&account.id).is_some());
    }

    #[test]
    fn sign_out_keeps_account_remove_deletes_it() {
        let dir = TempDir::new("signout");
        let store = test_store(&dir);
        let account = store
            .sign_in("app.exponential.at", "tok-A", &user())
            .unwrap();

        store.sign_out(&account.id);
        assert_eq!(store.token(&account.id), None);
        assert!(store.account(&account.id).is_some());

        store.remove_account(&account.id);
        assert!(store.account(&account.id).is_none());

        // Removal persisted.
        let reloaded = test_store(&dir);
        assert!(reloaded.accounts().is_empty());
    }
}
