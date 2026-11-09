//! The switcher's entries (#EDE-56): the Local desktops of this machine
//! (elocal's `desktops.json`, the machine's one index shared with the IDE, #EDE-65) and, for
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
    /// #EDE-65: the picked team (`switcher.json` beside `desktops.json`);
    /// a Local desktop is the desktops index's.
    current_path: PathBuf,
    demo: bool,
}

/// The shapes the shell syncs per account: the teams the switcher lists;
/// #VAPP-273: the members (the viewer's role), the apps (the team's
/// published packages) and their records, for a connected desktop;
/// #EDE-65: what the tray's inbox and Needs you read (`inbox::SHAPES`),
/// and the viewer's read marks (the rail's unread dot, the IDE's rule).
pub(crate) const SHAPES: &[&str] = &[
    "teams",
    "team_members",
    "apps",
    "app_records",
    "notifications",
    "issues",
    "boards",
    "sessions",
    // #EXP-1730: no `conversations`: messages name their session and
    // sessions carry their digest.
    "session_issues",
    "agents",
    "pull_requests",
    // #EDE-65 lane I: the agent chat's conversations (the IDE syncs the
    // same shape) and the people its transcript names.
    "messages",
    "users",
    "session_reads",
];

impl Workspaces {
    /// Opens the machine's Local desktops index (the IDE's, #EDE-65: eDE's
    /// own under `config_dir` imported once; the goldens keep theirs under
    /// `config_dir`) and the shared accounts.
    pub fn open(config_dir: &Path, demo: bool) -> Result<Self, String> {
        let desktops = if demo {
            elocal::Desktops::open(config_dir).map_err(|e| format!("desktops.json: {e}"))?
        } else {
            let (desktops, imported) = elocal::Desktops::open_importing(elocal::machine_root(), Some(config_dir)).map_err(|e| format!("desktops.json: {e}"))?;
            if !imported.ids.is_empty() || !imported.kept.is_empty() {
                tracing::info!(imported = ?imported.ids, kept = ?imported.kept, replaced = ?imported.replaced, root = %desktops.root().display(), "the Local desktops joined the machine's index");
            }
            desktops
        };
        let auth = api::AuthStore::load(api::default_data_dir());
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local").join("share"))
            .join("exponential-desktop");
        let sync = Arc::new(SyncManager::new().on_unauthorized(auth.unauthorized_handler_fn()));
        Ok(Self { desktops, auth, sync, data_dir, current_path: config_dir.join("switcher.json"), demo })
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

    /// The account's attachment uploads (#EDE-65 lane L: the chat composer's files).
    pub fn attachments(&self, account: &api::accounts::Account) -> api::attachment_upload::HttpAttachmentTransport {
        api::attachment_upload::HttpAttachmentTransport::new(&account.instance_url, self.auth.token_provider(&account.id))
    }

    /// The entry the desktops index says is current (a Local desktop).
    pub fn current_local(&self) -> Option<SwitcherEntry> {
        if self.demo {
            // The goldens: `EDE_SHELL_DEMO_TEAM=<id>` starts on that demo team.
            let team = std::env::var("EDE_SHELL_DEMO_TEAM").ok().filter(|t| !t.is_empty());
            let entries = demo_entries();
            return team.and_then(|t| entries.iter().find(|e| e.id == t).cloned()).or_else(|| entries.into_iter().next());
        }
        let id = self.desktops.current()?;
        let entry = self.desktops.get(id)?;
        Some(SwitcherEntry::local(&entry.id, &entry.name))
    }

    /// #EDE-65: the team picked last, to pick again once its account's
    /// teams are listed (None = the Local desktop the index names).
    pub fn saved_team(&self) -> Option<String> {
        if self.demo {
            return None;
        }
        read_saved_team(&self.current_path)
    }

    /// #EDE-65: remembers the pick across logins (a Local pick forgets the team).
    pub fn save_current(&self, entry: &SwitcherEntry) {
        if self.demo {
            return;
        }
        if let Err(e) = write_current(&self.current_path, entry) {
            tracing::warn!("switcher.json: {e}");
        }
    }

    /// The demo feed (the goldens): no accounts, no hand-off.
    pub fn demo(&self) -> bool {
        self.demo
    }

    /// #EDE-65: the signed-in accounts as (id, email).
    pub fn signed_in(&self) -> Vec<(String, String)> {
        if self.demo {
            return vec![("demo".into(), "danny@example.com".into())];
        }
        self.auth.signed_in_accounts().into_iter().map(|a| (a.id, a.email)).collect()
    }

    /// #EDE-65: every signed-in account's rows the tray reads (the inbox, Needs you).
    pub fn tray_rows(&self) -> Vec<crate::inbox::AccountRows> {
        if self.demo {
            return Vec::new();
        }
        self.auth
            .signed_in_accounts()
            .into_iter()
            .filter_map(|a| Some(crate::inbox::AccountRows::read(&a.id, &a.user_id, self.sync.store(&a.id)?.as_ref())))
            .collect()
    }

    /// #EDE-65: `notifications.markRead` on the account's instance, one call
    /// per row like the IDE's group press; the shape's echo clears them.
    pub fn mark_read(&self, account_id: &str, ids: Vec<String>) {
        let Some(account) = (!self.demo).then(|| self.account(account_id)).flatten() else { return };
        let trpc = self.trpc(&account);
        std::thread::spawn(move || {
            for id in ids {
                if let Err(e) = api::notifications::notifications_mark_read(&trpc, &id) {
                    tracing::warn!("notifications.markRead({id}): {e}");
                }
            }
        });
    }

    /// #EDE-65: what the start menu's issue search reads: every signed-in
    /// account's store and every active Local desktop's (None = the demo's).
    pub fn issue_sources(&self) -> Option<Vec<crate::issues::Source>> {
        if self.demo {
            return None;
        }
        let mut sources: Vec<crate::issues::Source> = self.auth.signed_in_accounts().iter().filter_map(|a| self.sync.store(&a.id)).map(crate::issues::Source::Account).collect();
        for d in self.desktops.active() {
            sources.push(crate::issues::Source::Local { id: d.id.clone(), name: d.name.clone(), path: self.desktops.desktop_dir(&d.id).join("local.sqlite") });
        }
        Some(sources)
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

fn read_saved_team(path: &Path) -> Option<String> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    (value["kind"] == "team").then(|| value["id"].as_str().map(str::to_string)).flatten().filter(|id| !id.is_empty())
}

fn write_current(path: &Path, entry: &SwitcherEntry) -> std::io::Result<()> {
    // The account too (#EDE-65): the Exponential app starts on the team's own account.
    std::fs::write(path, serde_json::json!({"kind": entry.kind, "id": entry.id, "account": entry.account}).to_string())
}

/// #EDE-65: what the switcher and the chat say about the connection: is an
/// account signed in (Connect Exponential only without one), is the current
/// entry a Local desktop (Share as a team only there), and the account the
/// current team belongs to (else the first signed-in one).
pub fn connection_json(current: &SwitcherEntry, accounts: &[(String, String)]) -> Value {
    let account = current.account.as_ref().and_then(|id| accounts.iter().find(|(a, _)| a == id)).or_else(|| accounts.first()).map(|(_, email)| email.clone()).unwrap_or_default();
    serde_json::json!({
        "connected": !accounts.is_empty(),
        "onLocal": current.kind == uicore_model::SwitcherKind::Local,
        "account": account,
    })
}

/// #EDE-65: the deep link that brings the Exponential app onto a team.
pub fn team_deep_link(team_id: &str) -> String {
    format!("{}://team/{team_id}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// #EDE-65: the rail's chat on a team: the Exponential app opens the session.
pub fn session_deep_link(session_id: &str) -> String {
    format!("{}://session/{session_id}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// #EDE-65: the new-issue button: the Exponential app's new issue, on the team when one is current.
pub fn new_issue_deep_link(team_id: Option<&str>) -> String {
    match team_id {
        Some(team) => format!("{}://new-issue/{team}", api::login::OAUTH_CALLBACK_SCHEME),
        None => format!("{}://new-issue", api::login::OAUTH_CALLBACK_SCHEME),
    }
}

/// #EDE-65: the deep link that brings the Exponential app onto a Local
/// desktop (the machine's one index, so the app knows the id).
pub fn local_deep_link(desktop_id: &str) -> String {
    format!("{}://local/{desktop_id}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// #EDE-65: the Exponential app follows the switcher: the team's or the
/// Local desktop's link goes to the running app's single-instance socket
/// (`exp-desktop.sock`); nothing runs = nothing to tell (it opens on its own
/// last pick).
pub fn ide_follow(kind: uicore_model::SwitcherKind, id: &str) {
    let link = match kind {
        uicore_model::SwitcherKind::Local => local_deep_link(id),
        _ => team_deep_link(id),
    };
    if ide_send(&link) {
        tracing::info!(%link, "the Exponential app follows the switcher");
    }
}

/// A deep link to the running Exponential app's single-instance socket
/// (false: nothing runs or the send failed).
pub fn ide_send(link: &str) -> bool {
    crate::inbox::hand_off(link)
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

#[cfg(test)]
mod tests {
    use super::*;
    use uicore_model::SwitcherEntry;

    fn accounts() -> Vec<(String, String)> {
        vec![("a1".into(), "danny@example.com".into()), ("a2".into(), "work@example.com".into())]
    }

    #[test]
    fn a_connected_account_hides_connect_and_names_the_teams_account() {
        let team = SwitcherEntry::team("t1", "Acme", "a2", "work@example.com");
        assert_eq!(connection_json(&team, &accounts()), serde_json::json!({"connected": true, "onLocal": false, "account": "work@example.com"}));
        let local = SwitcherEntry::local("local", "Local");
        assert_eq!(connection_json(&local, &accounts()), serde_json::json!({"connected": true, "onLocal": true, "account": "danny@example.com"}));
        assert_eq!(connection_json(&local, &[]), serde_json::json!({"connected": false, "onLocal": true, "account": ""}));
    }

    #[test]
    fn the_picked_team_survives_a_restart_and_a_local_pick_forgets_it() {
        let dir = std::env::temp_dir().join(format!("ede-switcher-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("switcher.json");
        assert_eq!(read_saved_team(&path), None);
        write_current(&path, &SwitcherEntry::team("t1", "Acme", "a2", "work@example.com")).unwrap();
        assert_eq!(read_saved_team(&path).as_deref(), Some("t1"));
        // The Exponential app reads the team's account from it (ui `session::ede_pick`).
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved, serde_json::json!({"kind": "team", "id": "t1", "account": "a2"}));
        write_current(&path, &SwitcherEntry::local("local", "Local")).unwrap();
        assert_eq!(read_saved_team(&path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_team_link_is_the_one_the_app_parses() {
        assert_eq!(team_deep_link("e891ab20"), "exponential://team/e891ab20");
        assert_eq!(local_deep_link("87858284"), "exponential://local/87858284");
    }

    #[test]
    fn the_hand_off_links_are_the_ones_the_app_parses() {
        assert_eq!(session_deep_link("s1"), "exponential://session/s1");
        assert_eq!(new_issue_deep_link(Some("t1")), "exponential://new-issue/t1");
        assert_eq!(new_issue_deep_link(None), "exponential://new-issue");
    }

    #[test]
    fn the_chats_sync_with_the_teams() {
        for shape in ["sessions", "session_reads"] {
            assert!(SHAPES.contains(&shape), "{shape}");
            assert!(sync::shapes::shape_by_name(shape).is_some(), "{shape} is a synced shape");
        }
        assert!(!SHAPES.contains(&"conversations"), "#EXP-1730: never read");
    }
}
