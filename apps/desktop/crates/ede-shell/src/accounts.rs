//! The switcher's entries (#EDE-56): the Local desktops of this machine
//! (elocal's `desktops.json` under the shell's config directory) and, for
//! every signed-in Exponential account (the IDE's `accounts.json` and token
//! store, shared), its teams. The teams come from the account's `teams`
//! shape, synced by the sync crate into the shell's OWN per-account store
//! (never the IDE's file: two sync cursors on one file would fight); a
//! delta on that store rebuilds the entries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use sync::{AccountSyncConfig, ShapeDelta, SyncManager};
use uicore_model::SwitcherEntry;

pub struct Workspaces {
    pub desktops: elocal::Desktops,
    pub auth: Arc<api::AuthStore>,
    sync: Arc<SyncManager>,
    data_dir: PathBuf,
    demo: bool,
}

/// The shapes the shell syncs per account: the teams the switcher lists;
/// #VAPP-273: the members (the viewer's role), the apps (the team's
/// published packages) and their records, for a connected desktop.
const SHAPES: &[&str] = &["teams", "team_members", "apps", "app_records"];

impl Workspaces {
    /// Opens the desktops index under `config_dir` and the shared accounts.
    pub fn open(config_dir: &Path, demo: bool) -> Result<Self, String> {
        let desktops = elocal::Desktops::open(config_dir).map_err(|e| format!("desktops.json: {e}"))?;
        let auth = api::AuthStore::load(api::default_data_dir());
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local").join("share"))
            .join("exponential-desktop");
        let sync = Arc::new(SyncManager::new().on_unauthorized(auth.unauthorized_handler_fn()));
        Ok(Self { desktops, auth, sync, data_dir, demo })
    }

    /// The change stream of the accounts' team stores.
    pub fn deltas(&self) -> flume::Receiver<ShapeDelta> {
        self.sync.deltas()
    }

    /// Syncs every signed-in account's teams (idempotent per account).
    pub fn start_sync(&self) {
        if self.demo {
            return;
        }
        for account in self.auth.signed_in_accounts() {
            self.start_account(&account);
        }
    }

    pub fn start_account(&self, account: &api::accounts::Account) {
        let config = AccountSyncConfig {
            account_id: account.id.clone(),
            base_url: account.instance_url.clone(),
            db_path: api::account_db_path(&self.data_dir, &account.id),
            token: self.auth.token_provider_fn(&account.id),
            shapes: Some(SHAPES),
        };
        match self.sync.start_account(config) {
            Ok(started) => tracing::info!(account = %account.email, started, "account sync"),
            Err(e) => tracing::warn!(account = %account.email, "account sync: {e}"),
        }
    }

    /// The switcher's entries: the active Local desktops, then every
    /// signed-in account's teams (by name).
    pub fn entries(&self) -> Vec<SwitcherEntry> {
        if self.demo {
            return demo_entries();
        }
        let mut entries: Vec<SwitcherEntry> = self.desktops.active().into_iter().map(|d| SwitcherEntry::local(&d.id, &d.name)).collect();
        for account in self.auth.signed_in_accounts() {
            let Some(store) = self.sync.store(&account.id) else { continue };
            let Some(spec) = sync::shapes::shape_by_name("teams") else { continue };
            let mut teams: Vec<(String, String)> = store
                .read_all(spec)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|row| Some((row.get("id")?.as_str()?.to_string(), row.get("name")?.as_str()?.to_string())))
                .collect();
            teams.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            for (id, name) in teams {
                entries.push(SwitcherEntry::team(id, name, &account.id, &account.email));
            }
        }
        entries
    }

    /// A signed-in account by id.
    pub fn account(&self, account_id: &str) -> Option<api::accounts::Account> {
        self.auth.account(account_id)
    }

    /// An account's synced store (once its sync started).
    pub fn store(&self, account_id: &str) -> Option<Arc<sync::store::ShapeStore>> {
        self.sync.store(account_id)
    }

    pub fn manager(&self) -> Arc<SyncManager> {
        self.sync.clone()
    }

    /// A tRPC client on the account's instance with its token.
    pub fn trpc(&self, account: &api::accounts::Account) -> api::TrpcClient {
        api::TrpcClient::new(&account.instance_url, self.auth.token_provider(&account.id))
    }

    /// The entry the desktops index says is current (a Local desktop).
    pub fn current_local(&self) -> Option<SwitcherEntry> {
        if self.demo {
            return demo_entries().into_iter().next();
        }
        let id = self.desktops.current()?;
        let entry = self.desktops.get(id)?;
        Some(SwitcherEntry::local(&entry.id, &entry.name))
    }

    /// `/accounts`: what Settings' Account page lists.
    pub fn accounts_json(&self) -> Value {
        if self.demo {
            return serde_json::json!([{"id": "demo", "email": "danny@example.com", "instance": "https://app.exponential.at", "name": "Danny"}]);
        }
        Value::Array(
            self.auth
                .signed_in_accounts()
                .iter()
                .map(|a| serde_json::json!({"id": a.id, "email": a.email, "instance": a.instance_url, "name": a.name.clone().unwrap_or_else(|| a.email.clone())}))
                .collect(),
        )
    }
}

/// The goldens' switcher: two Local desktops and one account with two teams.
fn demo_entries() -> Vec<SwitcherEntry> {
    vec![
        SwitcherEntry::local("local", "Local"),
        SwitcherEntry::local("work", "Work"),
        SwitcherEntry::team("t-feedback", "Feedback", "demo", "danny@example.com"),
        SwitcherEntry::team("t-side", "Side project", "demo", "danny@example.com"),
    ]
}
