//! The switcher's entries (#EDE-56): the Local desktops of this machine
//! (elocal's `desktops.json`, the machine's one index shared with the IDE, #EDE-65) and, for
//! every signed-in Exponential account (the IDE's `accounts.json` and token
//! store, shared), its teams.
//!
//! #EDE-238: ONE STORE per login. The shell syncs nothing: the login's
//! holder (`exponential-app`) syncs the account into its store
//! (`<app data>/accounts/<id>/sync-v2.sqlite`) and the shell reads that file
//! READ-ONLY ([`sync::store::ShapeStore::open_read_only`]): the teams, the
//! tray's inbox and Needs you, the team chat's rows, a connected team's app
//! records. A watch on the file stands in for the sync's deltas
//! ([`Workspaces::deltas`]). The shell holds no account connection; what it
//! writes goes over tRPC when the person acts (a send, a mark read, a
//! queued app record).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use sync::store::ShapeStore;
use sync::ShapeDelta;
use uicore_model::SwitcherEntry;

pub struct Workspaces {
    pub desktops: elocal::Desktops,
    pub auth: Arc<api::AuthStore>,
    /// #EDE-238: the holder's data directory (its account stores).
    holder_dir: PathBuf,
    /// The holder's stores, opened read-only on first use.
    stores: Mutex<HashMap<String, Arc<ShapeStore>>>,
    /// The shell's own queue of a connected team's app record writes
    /// (`outbox.sqlite` per account in its data directory).
    outboxes: Mutex<HashMap<String, Arc<ShapeStore>>>,
    data_dir: PathBuf,
    /// #EDE-65: the picked team (`switcher.json` beside `desktops.json`);
    /// a Local desktop is the desktops index's.
    current_path: PathBuf,
    /// #EDE-223: what the watch on `accounts.json` saw change.
    changes: flume::Receiver<api::AuthEvent>,
    demo: bool,
}

/// #EDE-223: where the shell starts. One account per eDE login (#EDE-221):
/// with it signed in and no saved choice, its team; the team list arrives
/// with the first sync, so the start waits for it instead of settling on Local.
#[derive(Debug, Clone, PartialEq)]
pub enum StartOn {
    /// A Local desktop (the index's current): no account, or the saved pick.
    Local(SwitcherEntry),
    /// A team that is listed now (the saved one, else the account's first).
    Team(SwitcherEntry),
    /// Not listed yet: pick it once its account's teams arrive.
    Wait(Pending),
}

/// #EDE-223: a pick that waits for the teams: the saved team, else the
/// first team of the account (by name, the switcher's order).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pending {
    pub team: Option<String>,
    pub account: Option<String>,
}

impl Pending {
    /// The team to pick among `entries`, once it is listed.
    pub fn resolve(&self, entries: &[SwitcherEntry]) -> Option<String> {
        let teams = || entries.iter().filter(|e| e.kind == uicore_model::SwitcherKind::Team);
        match (&self.team, &self.account) {
            (Some(team), _) => teams().find(|e| &e.id == team).map(|e| e.id.clone()),
            (None, Some(account)) => teams().find(|e| e.account.as_ref() == Some(account)).map(|e| e.id.clone()),
            (None, None) => None,
        }
    }

    pub fn is_some(&self) -> bool {
        self.team.is_some() || self.account.is_some()
    }
}

/// #EDE-238: the holder's store of an account (`api::account_db_path`'s
/// file, without its clean-up of older files: they are the holder's).
pub fn holder_store_path(holder_dir: &Path, account_id: &str) -> PathBuf {
    holder_dir.join("accounts").join(account_id).join("sync-v2.sqlite")
}

/// The store file's and its WAL's size and time (a commit moves one).
fn store_stamp(path: &Path) -> Vec<Option<(u64, std::time::SystemTime)>> {
    let wal = path.with_file_name("sync-v2.sqlite-wal");
    [path.to_path_buf(), wal].iter().map(|p| std::fs::metadata(p).ok().and_then(|m| Some((m.len(), m.modified().ok()?)))).collect()
}

/// #EDE-65: the pick `switcher.json` remembers (None = never picked).
#[derive(Debug, Clone, PartialEq)]
enum Saved {
    Local,
    Team(String),
}

/// The shapes the shell reads per account (#EDE-238: off the holder's
/// store; a change of the file counts as a change of each): the teams the switcher lists;
/// #VAPP-273: the members (the viewer's role), the apps (the team's
/// published packages) and their records, for a connected desktop;
/// #EDE-65: what the tray's inbox and Needs you read (`inbox::SHAPES`),
/// and the viewer's read marks (the rail's unread count, the IDE's rule).
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
    // #EDE-134: the team's prompts (the chat's `/` and + menus).
    "prompts",
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
        Ok(Self::with(desktops, auth, api::default_data_dir(), data_dir, config_dir.join("switcher.json"), demo))
    }

    /// [`Self::open`] over given parts (the tests: a temporary data directory).
    fn with(desktops: elocal::Desktops, auth: Arc<api::AuthStore>, holder_dir: PathBuf, data_dir: PathBuf, current_path: PathBuf, demo: bool) -> Self {
        // #EDE-223: a sign-in or sign-out in another process (the app, the
        // CLI) shows up while the shell runs; the demo has no accounts.
        let changes = if demo { flume::unbounded().1 } else { auth.watch(std::time::Duration::from_secs(2)) };
        Self { desktops, auth, holder_dir, stores: Mutex::default(), outboxes: Mutex::default(), data_dir, current_path, changes, demo }
    }

    /// #EDE-223: the accounts that signed in or out in another process.
    pub fn account_changes(&self) -> flume::Receiver<api::AuthEvent> {
        self.changes.clone()
    }

    /// #EDE-226: the login's one account ([`crate::session_account`]), as a
    /// list: everything the shell shows and syncs goes through it.
    pub fn session_accounts(&self) -> Vec<api::accounts::Account> {
        let picked = saved_account(&self.current_path);
        crate::session_account::session_account(self.auth.signed_in_accounts(), picked.as_deref()).into_iter().collect()
    }

    pub fn session_account(&self) -> Option<api::accounts::Account> {
        self.session_accounts().pop()
    }

    /// #EDE-226: a signed-out account's own store goes with its entry.
    pub fn wipe_when_forgotten(&self, account_id: &str) {
        crate::session_account::wipe_when_forgotten(self.auth.clone(), self.data_dir.clone(), account_id.to_string());
    }

    /// #EDE-223: a signed-out account's store closes (the holder's file
    /// is the holder's).
    pub fn stop_account(&self, account_id: &str) {
        self.stores.lock().unwrap_or_else(|p| p.into_inner()).remove(account_id);
        self.outboxes.lock().unwrap_or_else(|p| p.into_inner()).remove(account_id);
    }

    /// #EDE-223: where the shell starts ([`StartOn`]). The demo: the index's
    /// current (its `EDE_SHELL_DEMO_TEAM`).
    pub fn start_on(&self) -> Option<StartOn> {
        let local = self.current_local();
        if self.demo {
            return local.map(|e| if e.kind == uicore_model::SwitcherKind::Team { StartOn::Team(e) } else { StartOn::Local(e) });
        }
        let pending = match read_saved(&self.current_path) {
            Some(Saved::Local) => None,
            Some(Saved::Team(team)) => Some(Pending { team: Some(team), account: None }),
            // One account per eDE login: the first signed-in one.
            None => self.session_accounts().first().map(|a| Pending { team: None, account: Some(a.id.clone()) }),
        };
        let Some(pending) = pending else { return local.map(StartOn::Local) };
        let entries = self.entries();
        Some(match pending.resolve(&entries).and_then(|id| entries.into_iter().find(|e| e.kind == uicore_model::SwitcherKind::Team && e.id == id)) {
            Some(team) => StartOn::Team(team),
            None => StartOn::Wait(pending),
        })
    }

    /// #EDE-223: nothing saved in `switcher.json` (a sign-in while on Local
    /// then moves the shell onto the account's team).
    pub fn nothing_saved(&self) -> bool {
        !self.demo && read_saved(&self.current_path).is_none()
    }

    /// #EDE-238: the change stream of the accounts' stores: the holder's
    /// file watched once a second (its size and time, the WAL's too); a
    /// change is a full change of every shape the shell reads.
    pub fn deltas(&self) -> flume::Receiver<ShapeDelta> {
        let (tx, rx) = flume::unbounded();
        if self.demo {
            return rx;
        }
        let (auth, current_path, holder_dir) = (self.auth.clone(), self.current_path.clone(), self.holder_dir.clone());
        let spawned = std::thread::Builder::new().name("ede-shell-store-watch".into()).spawn(move || {
            let mut seen: HashMap<String, Vec<Option<(u64, std::time::SystemTime)>>> = HashMap::new();
            loop {
                let picked = saved_account(&current_path);
                let accounts = crate::session_account::session_account(auth.signed_in_accounts(), picked.as_deref());
                for account in accounts {
                    let path = holder_store_path(&holder_dir, &account.id);
                    let now = store_stamp(&path);
                    if seen.get(&account.id) == Some(&now) {
                        continue;
                    }
                    seen.insert(account.id.clone(), now);
                    for shape in SHAPES {
                        let delta = ShapeDelta::Applied { account_id: account.id.clone(), shape, keys: Vec::new(), full_replace: true, up_to_date: true };
                        if tx.send(delta).is_err() {
                            return;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("store watch: {e}");
        }
        rx
    }

    /// #EDE-238: a (re)signed-in account's store is opened afresh.
    pub fn start_account(&self, account: &api::accounts::Account) {
        self.stores.lock().unwrap_or_else(|p| p.into_inner()).remove(&account.id);
    }

    /// The switcher's entries: the active Local desktops, then every
    /// signed-in account's teams (by name).
    pub fn entries(&self) -> Vec<SwitcherEntry> {
        if self.demo {
            return demo_entries();
        }
        let mut entries: Vec<SwitcherEntry> = self.desktops.active().into_iter().map(|d| SwitcherEntry::local(&d.id, &d.name)).collect();
        for account in self.session_accounts() {
            let Some(store) = self.store(&account.id) else { continue };
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

    /// #EDE-238: an account's store: the holder's, read-only (none until
    /// the holder created it).
    pub fn store(&self, account_id: &str) -> Option<Arc<ShapeStore>> {
        let mut stores = self.stores.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(store) = stores.get(account_id) {
            return Some(store.clone());
        }
        let path = holder_store_path(&self.holder_dir, account_id);
        let store = Arc::new(ShapeStore::open_read_only(&path).map_err(|e| tracing::debug!(path = %path.display(), "the holder's store: {e}")).ok()?);
        stores.insert(account_id.to_string(), store.clone());
        Some(store)
    }

    /// #EDE-238: the shell's own queue of an account's app record writes.
    pub fn outbox(&self, account_id: &str) -> Option<Arc<ShapeStore>> {
        let mut outboxes = self.outboxes.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(store) = outboxes.get(account_id) {
            return Some(store.clone());
        }
        let path = self.data_dir.join("accounts").join(account_id).join("outbox.sqlite");
        let store = Arc::new(ShapeStore::open(&path).map_err(|e| tracing::warn!(path = %path.display(), "outbox: {e}")).ok()?);
        outboxes.insert(account_id.to_string(), store.clone());
        Some(store)
    }



    /// A tRPC client on the account's instance with its token.
    pub fn trpc(&self, account: &api::accounts::Account) -> api::TrpcClient {
        api::TrpcClient::new(&account.instance_url, self.auth.token_provider(&account.id))
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

    /// #EDE-65: remembers the pick across logins (a Local pick forgets the team).
    pub fn save_current(&self, entry: &SwitcherEntry) {
        if self.demo {
            return;
        }
        if let Err(e) = write_current(&self.current_path, entry) {
            tracing::warn!("switcher.json: {e}");
        }
    }

    /// #EDE-223: forgets the pick (a sign-out's move to Local is no choice:
    /// the next sign-in starts on its team again).
    pub fn forget_current(&self) {
        if !self.demo {
            let _ = std::fs::remove_file(&self.current_path);
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
        self.session_accounts().into_iter().map(|a| (a.id, a.email)).collect()
    }

    /// #EDE-65: every signed-in account's store the tray reads (the inbox,
    /// Needs you), with the account's id and user. #EDE-141: the stores,
    /// not their rows: the tray reads them off the main loop.
    pub fn tray_sources(&self) -> Vec<(String, String, Arc<sync::store::ShapeStore>)> {
        if self.demo {
            return Vec::new();
        }
        self.session_accounts().into_iter().filter_map(|a| Some((a.id.clone(), a.user_id.clone(), self.store(&a.id)?))).collect()
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
        // #EDE-258: the teams' issues are the holder's search (one store).
        let mut sources: Vec<crate::issues::Source> = Vec::new();
        for d in self.desktops.active() {
            sources.push(crate::issues::Source::Local { id: d.id.clone(), name: d.name.clone(), path: self.desktops.desktop_dir(&d.id).join("local.sqlite") });
        }
        Some(sources)
    }

    /// #EDE-258: the active Local desktops (id, name, store path).
    pub fn local_desktops(&self) -> Vec<(String, String, std::path::PathBuf)> {
        if self.demo {
            return Vec::new();
        }
        self.desktops.active().iter().map(|d| (d.id.clone(), d.name.clone(), self.desktops.desktop_dir(&d.id).join("local.sqlite"))).collect()
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

fn read_saved(path: &Path) -> Option<Saved> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    match value["kind"].as_str()? {
        "team" => value["id"].as_str().filter(|id| !id.is_empty()).map(|id| Saved::Team(id.to_string())),
        "local" => Some(Saved::Local),
        _ => None,
    }
}

#[cfg(test)]
fn read_saved_team(path: &Path) -> Option<String> {
    match read_saved(path)? {
        Saved::Team(id) => Some(id),
        Saved::Local => None,
    }
}

/// #EDE-226: the account the saved team belongs to.
fn saved_account(path: &Path) -> Option<String> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    value["account"].as_str().filter(|id| !id.is_empty()).map(str::to_string)
}

fn write_current(path: &Path, entry: &SwitcherEntry) -> std::io::Result<()> {
    // The account too (#EDE-65): the Exponential app starts on the team's own account.
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
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

/// #EDE-227: one request to the login's holder (eDE's component, or a
/// standalone app) over the session bus; `None` = no holder serves one (an
/// older app: the link socket is its way in).
pub fn holder_call(request: &api::bus::Request) -> Option<api::bus::Reply> {
    match api::bus::call(&api::bus::socket_path(&api::default_data_dir()), request) {
        Ok(reply) => Some(reply),
        Err(e) => {
            tracing::info!("exp-desktop.bus: {e}");
            None
        }
    }
}

/// #EDE-226/#EDE-227: what the shell asks of the login's account and
/// install, over the bus, else on the link socket (false: nobody took it).
pub fn holder_command(command: api::holder::Command) -> bool {
    use api::bus::{Reply, Request};
    match holder_call(&Request::Holder { command: command.clone() }) {
        Some(Reply::Done) => true,
        Some(Reply::Refused { message }) => {
            tracing::warn!(message, "the holder refused a command");
            false
        }
        Some(_) | None => ide_send(&command.to_datagram()),
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

    /// #EDE-223: a temporary data directory with one signed-in account,
    /// its token, its synced `teams` and one Local desktop.
    fn signed_in(dir: &Path, teams: &[(&str, &str)]) -> (Workspaces, api::accounts::Account) {
        let data = dir.join("exponential");
        let auth = api::AuthStore::load_with_token_store(data.clone(), api::TokenStore::file_only(data.clone()));
        let user = api::login::AuthUser { id: "user-1".into(), email: "danny@example.com".into(), name: None, is_admin: None, onboarding_completed_at: None };
        let account = auth.sign_in("http://localhost:3000", "tok", &user).unwrap();
        let desktops = elocal::Desktops::open(data.join("desktops")).unwrap();
        let workspaces = Workspaces::with(desktops, auth, data.clone(), dir.join("shell"), dir.join("switcher.json"), false);
        if !teams.is_empty() {
            // #EDE-238: the holder's store; the shell opens it read-only.
            let db = holder_store_path(&data, &account.id);
            let store = Arc::new(sync::store::ShapeStore::open(&db).unwrap());
            let spec = sync::shapes::shape_by_name("teams").unwrap();
            let rows: Vec<sync::protocol::ShapeMessage> = teams
                .iter()
                .map(|(id, name)| sync::protocol::ShapeMessage::Insert { key: sync::protocol::RowKey::Single(id.to_string()), value: serde_json::json!({"id": id, "name": name}).as_object().unwrap().clone() })
                .collect();
            store.apply_batch(spec, &rows, None).unwrap();
        }
        (workspaces, account)
    }

    #[test]
    fn a_signed_in_account_starts_on_its_team_not_on_local() {
        let dir = tempfile::tempdir().unwrap();
        let (workspaces, account) = signed_in(dir.path(), &[("t2", "Side project"), ("t1", "Acme")]);
        assert_eq!(workspaces.desktops.active().len(), 1, "one Local desktop");
        assert_eq!(workspaces.start_on(), Some(StartOn::Team(SwitcherEntry::team("t1", "Acme", &account.id, "danny@example.com"))));
        // A saved Local pick stays the person's choice.
        let local = workspaces.current_local().unwrap();
        workspaces.save_current(&local);
        assert_eq!(workspaces.start_on(), Some(StartOn::Local(local)));
        // A saved team wins over the account's first.
        workspaces.save_current(&SwitcherEntry::team("t2", "Side project", &account.id, "danny@example.com"));
        assert!(matches!(workspaces.start_on(), Some(StartOn::Team(e)) if e.id == "t2"));
    }

    #[test]
    fn before_the_first_sync_the_start_waits_for_the_account_s_team() {
        let dir = tempfile::tempdir().unwrap();
        let (workspaces, account) = signed_in(dir.path(), &[]);
        let pending = Pending { team: None, account: Some(account.id.clone()) };
        assert_eq!(workspaces.start_on(), Some(StartOn::Wait(pending.clone())));
        let local = workspaces.current_local().unwrap();
        assert_eq!(pending.resolve(&[local.clone()]), None, "never settles on Local");
        assert_eq!(pending.resolve(&[local, SwitcherEntry::team("t9", "Other", "a2", "w@example.com"), SwitcherEntry::team("t1", "Acme", &account.id, "danny@example.com")]).as_deref(), Some("t1"));
    }

    #[test]
    fn accounts_json_written_after_open_shows_up_in_the_entries() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("exponential");
        let auth = api::AuthStore::load_with_token_store(data.clone(), api::TokenStore::file_only(data.clone()));
        let workspaces = Workspaces::with(elocal::Desktops::open(data.join("desktops")).unwrap(), auth, data.clone(), dir.path().join("shell"), dir.path().join("switcher.json"), false);
        assert!(matches!(workspaces.start_on(), Some(StartOn::Local(_))), "no account: Local");
        // Another process signs in on the same data directory.
        let other = api::AuthStore::load_with_token_store(data.clone(), api::TokenStore::file_only(data.clone()));
        let user = api::login::AuthUser { id: "user-1".into(), email: "danny@example.com".into(), name: None, is_admin: None, onboarding_completed_at: None };
        let account = other.sign_in("http://localhost:3000", "tok", &user).unwrap();
        let change = workspaces.account_changes().recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(change, api::AuthEvent::SignedIn { account_id: account.id.clone() });
        assert_eq!(workspaces.signed_in(), vec![(account.id.clone(), "danny@example.com".to_string())]);
        let spec = sync::shapes::shape_by_name("teams").unwrap();
        let store = Arc::new(sync::store::ShapeStore::open(&holder_store_path(&data, &account.id)).unwrap());
        store.apply_batch(spec, &[sync::protocol::ShapeMessage::Insert { key: sync::protocol::RowKey::Single("t1".into()), value: serde_json::json!({"id": "t1", "name": "Acme"}).as_object().unwrap().clone() }], None).unwrap();
        assert!(workspaces.entries().iter().any(|e| e.id == "t1" && e.account.as_deref() == Some(account.id.as_str())));
        // And a sign-out there.
        other.sign_out(&account.id);
        let change = workspaces.account_changes().recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(change, api::AuthEvent::SignedOut { account_id: account.id });
        assert!(workspaces.entries().iter().all(|e| e.kind == uicore_model::SwitcherKind::Local));
    }

    #[test]
    fn the_team_link_is_the_one_the_app_parses() {
        assert_eq!(team_deep_link("e891ab20"), "exponential://team/e891ab20");
        assert_eq!(local_deep_link("87858284"), "exponential://local/87858284");
    }

    #[test]
    fn the_hand_off_links_are_the_ones_the_app_parses() {
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

    /// #EDE-134: every shape the tray reads is synced.
    #[test]
    fn what_the_tray_reads_is_synced() {
        for shape in crate::inbox::SHAPES.iter().chain(&["team_members"]) {
            assert!(SHAPES.contains(shape), "{shape}");
            assert!(sync::shapes::shape_by_name(shape).is_some(), "{shape} is a synced shape");
        }
    }
}
