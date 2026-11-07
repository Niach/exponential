//! A connected desktop (#VAPP-273, spec §16: identity `member`): the current
//! switcher entry is a team of a signed-in account, so the apps are the
//! TEAM's published packages (the account's `apps` shape), their records the
//! team's (`app_records`, through the Engine's `SyncedRecordStore`: the
//! synced cache, the account's `app_record_outbox` as the queue, tRPC out,
//! the same providers the IDE uses, `sync::app_outbox`), and the viewer the
//! account's user with their team role (`team_members`). The shell feeds
//! the store's rows on every shape delta and tells each app's pump when the
//! queue moves, so the live sources follow.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use eengine::providers::{EngineIdentity, Identity, Pushed, SystemClock};
use eengine::outbox::OutboxEntry;
use eengine::{SyncedListener, SyncedRecordStore};
use eengine_runner::paint::Inbound;
use eui_core::host::RecordRow;
use serde_json::{Map, Value};
use sync::app_outbox::{PublishedVersions, ReplayErrors, SqliteLog, TrpcTransport};

use super::EngineApp;

/// A team's published app: the Engine app (its package) and the server's
/// app id, the key of its records.
#[derive(Debug, Clone)]
pub struct TeamApp {
    pub app: EngineApp,
    pub store_key: String,
    pub published: Option<i64>,
}

/// The team's apps from the account's `apps` rows: published, not deleted,
/// the published package; by name.
pub fn team_apps(rows: &[Map<String, Value>], team_id: &str) -> Vec<TeamApp> {
    let mut out: Vec<TeamApp> = rows
        .iter()
        .filter_map(|row| serde_json::from_value::<domain::rows::AppRow>(Value::Object(row.clone())).ok())
        .filter(|a| a.team_id.as_deref() == Some(team_id) && a.deleted_at.is_none() && a.published_version.unwrap_or(0) > 0)
        .filter_map(|a| {
            let package = a.package.clone()?;
            let pkg = eengine::package::package_of(&package);
            let id = pkg.get("id").and_then(Value::as_str).filter(|s| !s.is_empty())?.to_string();
            let text = |k: &str| pkg.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let app = EngineApp {
                id,
                name: if text("name").is_empty() { a.name.clone() } else { text("name") },
                description: text("description"),
                version: if text("version").is_empty() { "0.0.0".into() } else { text("version") },
                path: std::path::PathBuf::from(format!("team:{}/{}", team_id, a.id)),
                file: package,
                store_key: Some(a.id.clone()),
            };
            Some(TeamApp { app, store_key: a.id, published: a.published_version })
        })
        .collect();
    out.sort_by(|a, b| a.app.name.to_lowercase().cmp(&b.app.name.to_lowercase()));
    out
}

/// ISO 8601 UTC with milliseconds from the shape's timestamp text (the IDE's `iso_millis`).
fn iso_millis(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    let t_form = trimmed.replacen(' ', "T", 1);
    [trimmed.to_string(), t_form.clone(), format!("{t_form}:00"), format!("{t_form}Z")]
        .iter()
        .find_map(|c| chrono::DateTime::parse_from_rfc3339(c).ok())
        .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| trimmed.to_string())
}

/// The team's records from the account's `app_records` rows, by the server's app id.
pub fn record_rows(rows: &[Map<String, Value>], team_id: &str) -> HashMap<String, Vec<RecordRow>> {
    let mut out: HashMap<String, Vec<RecordRow>> = HashMap::new();
    for r in rows.iter().filter_map(|row| serde_json::from_value::<domain::rows::AppRecordRow>(Value::Object(row.clone())).ok()) {
        if r.team_id.as_deref() != Some(team_id) {
            continue;
        }
        out.entry(r.app_id.clone()).or_default().push(RecordRow {
            id: r.id,
            kind: r.kind,
            data: r.data,
            created_by: r.created_by,
            author_id: r.author_id,
            created_at: iso_millis(r.created_at.as_deref().unwrap_or("")),
            updated_at: iso_millis(r.updated_at.as_deref().unwrap_or("")),
        });
    }
    out
}

/// The viewer: the account's user with their role on the team (`team_members`).
pub fn member_identity(rows: &[Map<String, Value>], user_id: &str, team_id: &str) -> EngineIdentity {
    let role = rows
        .iter()
        .find(|r| r.get("user_id").and_then(Value::as_str) == Some(user_id) && r.get("team_id").and_then(Value::as_str) == Some(team_id))
        .and_then(|r| r.get("role").and_then(Value::as_str))
        .map(str::to_string);
    EngineIdentity::Member { user_id: user_id.to_string(), role, access: None }
}

pub struct MemberIdentity(pub EngineIdentity);

impl Identity for MemberIdentity {
    fn identity(&self) -> EngineIdentity {
        self.0.clone()
    }
}

/// The queue moved: every app's pump re-reads its sources.
struct Pumps(Arc<Mutex<Vec<flume::Sender<Inbound>>>>);

impl SyncedListener for Pumps {
    fn changed(&self) {
        for tx in self.0.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            let _ = tx.send(Inbound::StoreChanged);
        }
    }

    fn refused(&self, entry: &OutboxEntry, verb: &str, pushed: &Pushed) {
        tracing::warn!(write = %entry.write_id, verb, message = ?pushed.message, "the server refused a queued write");
    }
}

/// One connected team's synced store and what feeds it.
pub struct TeamStores {
    pub account_id: String,
    pub team_id: String,
    pub store: Arc<SyncedRecordStore>,
    pub published: PublishedVersions,
    pub errors: ReplayErrors,
    pub online: Arc<AtomicBool>,
    /// The running apps' pumps (filled as they boot).
    pub pumps: Arc<Mutex<Vec<flume::Sender<Inbound>>>>,
}

impl TeamStores {
    /// Opens the team's store over the account's sync sqlite and tRPC.
    pub fn open(workspaces: &crate::accounts::Workspaces, account: &api::accounts::Account, team_id: &str) -> Result<Self, String> {
        let published: PublishedVersions = Arc::default();
        let errors: ReplayErrors = Arc::default();
        let transport = Arc::new(TrpcTransport::new(workspaces.trpc(account), published.clone(), errors.clone()));
        let log = Arc::new(SqliteLog::new(workspaces.manager(), account.id.clone()));
        let pumps: Arc<Mutex<Vec<flume::Sender<Inbound>>>> = Arc::default();
        let store = SyncedRecordStore::new(log, transport, Arc::new(SystemClock), Some(Arc::new(Pumps(pumps.clone())))).map_err(|e| e.message)?;
        Ok(TeamStores { account_id: account.id.clone(), team_id: team_id.to_string(), store: Arc::new(store), published, errors, online: Arc::new(AtomicBool::new(true)), pumps })
    }

    /// The account's shape rows into the store: the records, the published versions.
    pub fn feed(&self, shape_store: &sync::store::ShapeStore) {
        if let Some(spec) = sync::shapes::shape_by_name("app_records") {
            let rows = shape_store.read_all(spec).unwrap_or_default();
            self.store.set_all_rows(record_rows(&rows, &self.team_id));
        }
        if let Some(spec) = sync::shapes::shape_by_name("apps") {
            let rows = shape_store.read_all(spec).unwrap_or_default();
            let next: HashMap<String, Option<i64>> = team_apps(&rows, &self.team_id).into_iter().map(|a| (a.store_key, a.published)).collect();
            if let Ok(mut published) = self.published.write() {
                *published = next;
            }
        }
    }

    /// Replay what queued (off the main thread; a second call only asks for another pass).
    pub fn flush(&self) {
        let store = self.store.clone();
        std::thread::spawn(move || store.flush());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    const SAMPLE: &str = include_str!("../../../../../ede/apps/notes.app.json");

    #[test]
    fn the_teams_published_apps_become_engine_apps() {
        let package: Value = serde_json::from_str(SAMPLE).unwrap();
        let rows = vec![
            row(json!({"id": "app-1", "team_id": "t-1", "slug": "notes", "name": "Notes (team)", "package": package.to_string(), "published_version": 3, "deleted_at": null})),
            row(json!({"id": "app-2", "team_id": "t-1", "slug": "draft", "name": "Draft", "package": package.to_string(), "published_version": 0, "deleted_at": null})),
            row(json!({"id": "app-3", "team_id": "t-2", "slug": "other", "name": "Other", "package": package.to_string(), "published_version": 1, "deleted_at": null})),
            row(json!({"id": "app-4", "team_id": "t-1", "slug": "gone", "name": "Gone", "package": package.to_string(), "published_version": 1, "deleted_at": "2026-11-01 10:00:00"})),
        ];
        let apps = team_apps(&rows, "t-1");
        assert_eq!(apps.len(), 1);
        assert_eq!((apps[0].app.id.as_str(), apps[0].store_key.as_str(), apps[0].published), ("at.exponential.notes", "app-1", Some(3)));
        assert_eq!(apps[0].app.store_key.as_deref(), Some("app-1"));
        assert_eq!(apps[0].app.name, "Notes");
    }

    #[test]
    fn the_teams_records_group_by_app_with_iso_times() {
        let rows = vec![
            row(json!({"id": "r1", "app_id": "app-1", "team_id": "t-1", "kind": "note", "data": "{\"text\":\"hi\"}", "created_by": "u1", "author_id": "u1", "created_at": "2026-11-07 10:00:00", "updated_at": "2026-11-07T10:00:00Z"})),
            row(json!({"id": "r2", "app_id": "app-1", "team_id": "t-2", "kind": "note", "data": {}, "created_at": "2026-11-07T10:00:00Z", "updated_at": "2026-11-07T10:00:00Z"})),
        ];
        let grouped = record_rows(&rows, "t-1");
        assert_eq!(grouped.len(), 1);
        let r = &grouped["app-1"][0];
        assert_eq!((r.id.as_str(), r.data["text"].as_str()), ("r1", Some("hi")));
        assert_eq!(r.created_at, "2026-11-07T10:00:00.000Z");
    }

    #[test]
    fn the_viewer_is_the_member_with_their_role() {
        let rows = vec![row(json!({"team_id": "t-1", "user_id": "u1", "role": "owner"})), row(json!({"team_id": "t-2", "user_id": "u1", "role": "member"}))];
        assert_eq!(member_identity(&rows, "u1", "t-1"), EngineIdentity::Member { user_id: "u1".into(), role: Some("owner".into()), access: None });
        assert_eq!(member_identity(&rows, "u9", "t-1"), EngineIdentity::Member { user_id: "u9".into(), role: None, access: None });
    }
}
