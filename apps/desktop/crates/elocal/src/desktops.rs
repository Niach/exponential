//! The Local desktops of a machine: `desktops.json` in the index directory
//! ([`machine_root`]), one entry per desktop with
//! its name, when it was made, and after Share as a team which team it
//! became (its file is then archived for `ARCHIVE_DAYS`, #EDE-57). Each
//! desktop owns `desktops/<id>/`: `local.sqlite` (the store), `blobs/` and
//! `desktop.json` (the sidecar: the identity, the device id, what the shapes
//! leave out). A machine always has one desktop: the first open makes
//! `Local`.
//!
//! #EDE-65: ONE index per machine, [`machine_root`] (the IDE's data
//! directory), for the IDE and eDE alike, so eDE's switcher and the IDE
//! name the same desktops. eDE's own index from before
//! (`$XDG_CONFIG_HOME/exponential-desktop`, [`ede_legacy_root`]) is imported
//! once by [`Desktops::open_importing`]: copied, never moved, its files left
//! in place with a [`MIGRATED_MARKER`] beside them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::blobs::FsBlobStore;
use crate::executor::LocalExecutor;
use crate::identity::LocalIdentity;
use crate::parity;
use crate::store::LocalStore;
use crate::time;

/// How long an archived desktop's files stay after a Share (the plan's 30 days).
pub const ARCHIVE_DAYS: i64 = 30;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedAs {
    pub account: String,
    pub team_id: String,
    pub slug: String,
    pub shared_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_as: Option<SharedAs>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopsFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    desktops: Vec<DesktopEntry>,
}

/// A desktop's sidecar: what the synced shapes do not carry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMeta {
    pub identity: LocalIdentity,
    pub device_id: String,
    #[serde(default)]
    pub prompt_bodies: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub agent_instructions: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub agent_prompt: String,
    /// #EXP-1727: folders on this machine linked as the desktop's
    /// repositories (a board's `repository_id` names one).
    #[serde(default)]
    pub repositories: Vec<LocalRepository>,
    /// #EXP-1799: the desktop's issue trash (issue-lifecycle.json `trash`):
    /// a trashed issue's row and its children, out of the synced tables so
    /// every read hides them, until a restore puts them back or a purge
    /// drops them. An older sidecar without it reads as an empty trash.
    #[serde(default)]
    pub issue_trash: Vec<TrashedIssue>,
}

/// #EXP-1799: one issue in a Local desktop's trash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashedIssue {
    /// The issue row as it left the store, plus `deleted_at`, `deleted_by`
    /// and `deleted_by_agent_id`.
    pub issue: Map<String, Value>,
    /// Its child rows by table, as they left the store.
    #[serde(default)]
    pub children: std::collections::BTreeMap<String, Vec<Map<String, Value>>>,
}

impl TrashedIssue {
    pub fn id(&self) -> &str {
        self.issue.get("id").and_then(Value::as_str).unwrap_or("")
    }

    pub fn deleted_at(&self) -> Option<&str> {
        self.issue.get("deleted_at").and_then(Value::as_str)
    }
}

/// A git working copy on disk, a Local desktop's repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRepository {
    pub id: String,
    /// The folder's name, what the screens show where an instance shows `owner/name`.
    pub name: String,
    pub path: std::path::PathBuf,
    pub default_branch: String,
}

pub struct Desktops {
    root: PathBuf,
    file: DesktopsFile,
}

/// #EDE-65: the machine's one Local desktops index (the IDE's data
/// directory, `EXP_DATA_DIR` honoured), what the IDE and eDE both open.
pub fn machine_root() -> PathBuf {
    api::default_data_dir().join("desktops")
}

/// eDE's index before #EDE-65: `desktops.json` beside `compositor.toml`.
pub fn ede_legacy_root() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("exponential-desktop"))
}

/// Written beside an imported index: where it went, so it is imported once.
pub const MIGRATED_MARKER: &str = "desktops.migrated.json";

/// What [`Desktops::open_importing`] did with the old index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    /// Desktops now in the index (their folders copied when they had one).
    pub ids: Vec<String>,
    /// Desktops the index already held: kept as they are, the old copy untouched.
    pub kept: Vec<String>,
    /// Never-opened desktops (no folder, so no data) an imported one of the
    /// same name replaced: the IDE's first open made one before the import.
    pub replaced: Vec<String>,
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

impl Desktops {
    /// Opens the index under `root`, making the first desktop when there is none.
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        Self::open_importing(root, None).map(|(desktops, _)| desktops)
    }

    /// [`Self::open`] after importing the index under `legacy` once
    /// (#EDE-65): its entries join this one, their folders COPIED (a staging
    /// folder renamed into place, so a crash leaves no half desktop), the old
    /// files stay where they are with [`MIGRATED_MARKER`] beside them. An id
    /// this index already holds keeps this index's desktop and folder; a
    /// folder already at the target (a crash after the copy) is adopted.
    pub fn open_importing(root: impl Into<PathBuf>, legacy: Option<&Path>) -> std::io::Result<(Self, Imported)> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        let lock = Self::lock(&root)?;
        let mut file = Self::read(&root).unwrap_or_default();
        let mut imported = Imported::default();
        if let Some(legacy) = legacy.filter(|l| l.join("desktops.json").is_file() && !l.join(MIGRATED_MARKER).exists()) {
            let same = std::fs::canonicalize(legacy).ok().zip(std::fs::canonicalize(&root).ok()).is_some_and(|(a, b)| a == b);
            if !same {
                imported = Self::import(&root, &mut file, legacy)?;
                Self::settle(&mut file);
                Self::write(&root, &file)?;
                let marker = serde_json::json!({"migratedTo": root, "migratedAt": time::now(), "imported": imported.ids, "kept": imported.kept});
                std::fs::write(legacy.join(MIGRATED_MARKER), serde_json::to_string_pretty(&marker).map_err(std::io::Error::other)?)?;
            }
        }
        Self::settle(&mut file);
        Self::write(&root, &file)?;
        drop(lock);
        Ok((Self { root, file }, imported))
    }

    fn import(root: &Path, file: &mut DesktopsFile, legacy: &Path) -> std::io::Result<Imported> {
        let text = std::fs::read_to_string(legacy.join("desktops.json"))?;
        let old: DesktopsFile = serde_json::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}: {e}", legacy.join("desktops.json").display())))?;
        let mut out = Imported::default();
        let folders = root.join("desktops");
        std::fs::create_dir_all(&folders)?;
        // A crashed import's staging copies (never in the index) go first.
        for leftover in std::fs::read_dir(&folders)?.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".import-")) {
            let _ = std::fs::remove_dir_all(leftover.path());
        }
        for entry in old.desktops {
            if file.desktops.iter().any(|d| d.id == entry.id) {
                out.kept.push(entry.id);
                continue;
            }
            let (from, to) = (legacy.join("desktops").join(&entry.id), folders.join(&entry.id));
            if from.is_dir() && !to.exists() {
                let staging = folders.join(format!(".import-{}", entry.id));
                let _ = std::fs::remove_dir_all(&staging);
                copy_dir(&from, &staging)?;
                std::fs::rename(&staging, &to)?;
            }
            if entry.archived_at.is_none() {
                let pristine: Vec<String> = file.desktops.iter().filter(|d| d.archived_at.is_none() && d.name == entry.name && !folders.join(&d.id).exists()).map(|d| d.id.clone()).collect();
                file.desktops.retain(|d| !pristine.contains(&d.id));
                out.replaced.extend(pristine);
            }
            out.ids.push(entry.id.clone());
            file.desktops.push(entry);
        }
        let current_gone = file.current.as_ref().is_none_or(|c| !file.desktops.iter().any(|d| &d.id == c && d.archived_at.is_none()));
        if current_gone {
            file.current = old.current.filter(|c| out.ids.contains(c));
        }
        Ok(out)
    }

    /// The index's lock: every write is a read-modify-write under it, so the
    /// IDE and eDE never drop each other's desktops.
    fn lock(root: &Path) -> std::io::Result<std::fs::File> {
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join("desktops.lock"))?;
        lock.lock()?;
        Ok(lock)
    }

    fn read(root: &Path) -> Option<DesktopsFile> {
        std::fs::read_to_string(root.join("desktops.json")).ok().and_then(|t| serde_json::from_str(&t).ok())
    }

    fn write(root: &Path, file: &DesktopsFile) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(file).map_err(std::io::Error::other)?;
        let staging = root.join("desktops.json.tmp");
        std::fs::write(&staging, text)?;
        std::fs::rename(staging, root.join("desktops.json"))
    }

    /// One desktop at least, a current one among the active.
    fn settle(file: &mut DesktopsFile) {
        if file.desktops.iter().all(|d| d.archived_at.is_some()) {
            let entry = DesktopEntry { id: uuid::Uuid::new_v4().to_string(), name: "Local".to_string(), created_at: time::now(), archived_at: None, shared_as: None };
            file.current = Some(entry.id.clone());
            file.desktops.push(entry);
        }
        if file.current.as_ref().and_then(|c| file.desktops.iter().find(|d| &d.id == c && d.archived_at.is_none())).is_none() {
            file.current = file.desktops.iter().find(|d| d.archived_at.is_none()).map(|d| d.id.clone());
        }
        file.version = 1;
    }

    /// A change against the index on disk (another process may have written since).
    fn update<R>(&mut self, change: impl FnOnce(&mut DesktopsFile) -> R) -> std::io::Result<R> {
        let lock = Self::lock(&self.root)?;
        if let Some(file) = Self::read(&self.root) {
            self.file = file;
        }
        let out = change(&mut self.file);
        Self::settle(&mut self.file);
        Self::write(&self.root, &self.file)?;
        drop(lock);
        Ok(out)
    }

    /// Re-reads the index (the other app may have made, renamed or switched one).
    pub fn reload(&mut self) {
        if let Some(mut file) = Self::read(&self.root) {
            Self::settle(&mut file);
            self.file = file;
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every desktop, the archived ones included.
    pub fn all(&self) -> &[DesktopEntry] {
        &self.file.desktops
    }

    /// The desktops the switcher lists (not archived).
    pub fn active(&self) -> Vec<&DesktopEntry> {
        self.file.desktops.iter().filter(|d| d.archived_at.is_none()).collect()
    }

    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.file.desktops.iter().find(|d| d.id == id)
    }

    /// The current desktop's id (always one of the active ones).
    pub fn current(&self) -> Option<&str> {
        self.file.current.as_deref()
    }

    pub fn set_current(&mut self, id: &str) -> std::io::Result<bool> {
        self.update(|file| {
            let active = file.desktops.iter().any(|d| d.id == id && d.archived_at.is_none());
            if active {
                file.current = Some(id.to_string());
            }
            active
        })
    }

    /// A new desktop, current at once.
    pub fn create(&mut self, name: &str) -> std::io::Result<DesktopEntry> {
        let name = name.trim();
        let entry = DesktopEntry { id: uuid::Uuid::new_v4().to_string(), name: if name.is_empty() { "Local".to_string() } else { name.to_string() }, created_at: time::now(), archived_at: None, shared_as: None };
        self.update(|file| {
            file.desktops.push(entry.clone());
            file.current = Some(entry.id.clone());
        })?;
        Ok(entry)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> std::io::Result<bool> {
        self.update(|file| {
            let Some(entry) = file.desktops.iter_mut().find(|d| d.id == id) else { return false };
            entry.name = name.trim().to_string();
            true
        })
    }

    /// After Share as a team (#EDE-57): the desktop becomes the team, its
    /// files stay `ARCHIVE_DAYS` ([`Self::purge_expired`]).
    pub fn archive(&mut self, id: &str, shared_as: Option<SharedAs>) -> std::io::Result<bool> {
        let now = time::now();
        self.update(|file| {
            let Some(entry) = file.desktops.iter_mut().find(|d| d.id == id) else { return false };
            entry.archived_at = Some(now);
            entry.shared_as = shared_as;
            if file.current.as_deref() == Some(id) {
                file.current = file.desktops.iter().find(|d| d.archived_at.is_none()).map(|d| d.id.clone());
            }
            true
        })
    }

    /// Deletes the files of desktops archived more than `ARCHIVE_DAYS` ago.
    pub fn purge_expired(&mut self, now_ms: i64) -> std::io::Result<Vec<String>> {
        let cutoff = now_ms - ARCHIVE_DAYS * 24 * 60 * 60 * 1000;
        let expired = self.update(|file| {
            let expired: Vec<String> = file.desktops.iter().filter(|d| d.archived_at.as_deref().and_then(domain::instant::parse_instant).is_some_and(|at| at < cutoff)).map(|d| d.id.clone()).collect();
            file.desktops.retain(|d| !expired.contains(&d.id));
            expired
        })?;
        for id in &expired {
            let _ = std::fs::remove_dir_all(self.desktop_dir(id));
        }
        Ok(expired)
    }

    pub fn desktop_dir(&self, id: &str) -> PathBuf {
        self.root.join("desktops").join(id)
    }

    /// Opens a desktop: its store (seeded on first open), identity, blobs
    /// and executor.
    pub fn open_desktop(&self, id: &str) -> Result<LocalDesktop, String> {
        let entry = self.get(id).ok_or_else(|| format!("no desktop {id}"))?.clone();
        LocalDesktop::open(self.desktop_dir(id), entry)
    }
}

/// An opened Local desktop.
pub struct LocalDesktop {
    pub entry: DesktopEntry,
    pub dir: PathBuf,
    pub store: Arc<LocalStore>,
    pub blobs: FsBlobStore,
    pub executor: Arc<LocalExecutor>,
}

impl LocalDesktop {
    pub fn open(dir: PathBuf, entry: DesktopEntry) -> Result<Self, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let meta_path = dir.join("desktop.json");
        let fresh = !meta_path.exists();
        let meta: DesktopMeta = if fresh {
            DesktopMeta { identity: LocalIdentity::for_machine(), device_id: format!("local-{}", &entry.id[..entry.id.len().min(8)]), prompt_bodies: Default::default(), agent_instructions: Default::default(), agent_prompt: String::new(), repositories: Vec::new(), issue_trash: Vec::new() }
        } else {
            let text = std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| format!("desktop.json: {e}"))?
        };
        let store = Arc::new(LocalStore::open(&dir.join("local.sqlite")).map_err(|e| e.to_string())?);
        if store.rows("teams")?.is_empty() {
            let now = time::now();
            let mut ids = std::iter::repeat_with(|| uuid::Uuid::new_v4().to_string());
            for (table, row) in parity::seed_rows(&entry.id, &entry.name, &meta.identity, &now, &mut || ids.next().unwrap()) {
                store.write(table, row)?;
            }
        }
        if fresh {
            std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        let blobs = FsBlobStore::new(dir.join("blobs"));
        let executor = Arc::new(LocalExecutor::new(store.clone(), blobs.clone(), meta, meta_path, entry.id.clone()));
        // #EXP-1799: the trash sweep, as the server's watchdog runs it.
        executor.purge_due_issues(jiff::Timestamp::now().as_millisecond());
        Ok(Self { entry, dir, store, blobs, executor })
    }

    /// The desktop's team row (the switcher's name comes from the index, the
    /// store's row follows a rename through `teams.update`).
    pub fn team(&self) -> Option<Map<String, Value>> {
        self.store.rows("teams").ok()?.into_iter().next()
    }

    /// A client over this desktop.
    pub fn client(&self) -> api::TrpcClient {
        api::TrpcClient::with_transport(self.executor.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_always_has_one_desktop_and_switches() {
        let dir = tempfile::tempdir().unwrap();
        let mut desktops = Desktops::open(dir.path()).unwrap();
        assert_eq!(desktops.active().len(), 1);
        let first = desktops.current().unwrap().to_string();
        let second = desktops.create("Work").unwrap();
        assert_eq!(desktops.current(), Some(second.id.as_str()));
        assert!(desktops.set_current(&first).unwrap());
        assert!(!desktops.set_current("nope").unwrap());
        assert!(desktops.archive(&first, Some(SharedAs { account: "a".into(), team_id: "t".into(), slug: "s".into(), shared_at: time::now() })).unwrap());
        assert_eq!(desktops.current(), Some(second.id.as_str()));
        assert_eq!(desktops.active().len(), 1);
        let reopened = Desktops::open(dir.path()).unwrap();
        assert_eq!(reopened.all().len(), 2);
        assert_eq!(reopened.current(), Some(second.id.as_str()));
        // Nothing expires before ARCHIVE_DAYS; after, the files go.
        let mut again = Desktops::open(dir.path()).unwrap();
        let now = jiff::Timestamp::now().as_millisecond();
        assert!(again.purge_expired(now).unwrap().is_empty());
        assert_eq!(again.purge_expired(now + (ARCHIVE_DAYS + 1) * 86_400_000).unwrap(), vec![first.clone()]);
        assert_eq!(again.all().len(), 1);
    }

    #[test]
    fn an_opened_desktop_is_seeded_once() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = Desktops::open(dir.path()).unwrap();
        let id = desktops.current().unwrap().to_string();
        let desktop = desktops.open_desktop(&id).unwrap();
        assert_eq!(desktop.store.rows("issue_statuses").unwrap().len(), 6);
        assert_eq!(desktop.store.rows("boards").unwrap().len(), 1);
        assert_eq!(desktop.team().unwrap()["id"], serde_json::json!(id));
        drop(desktop);
        let again = desktops.open_desktop(&id).unwrap();
        assert_eq!(again.store.rows("issue_statuses").unwrap().len(), 6);
        assert_eq!(again.executor.identity().email, LocalIdentity::for_machine().email);
    }

    /// eDE's index as it is on a machine today (a copy of the real layout:
    /// one desktop, never opened, so no folder).
    fn ede_index(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("desktops.json"), format!(r#"{{"version": 1, "current": "{id}", "desktops": [{{"id": "{id}", "name": "Local", "createdAt": "2026-11-06T23:40:45.434Z"}}]}}"#)).unwrap();
    }

    #[test]
    fn edes_desktops_join_the_machines_index_once_and_stay_where_they_were() {
        let (ede, machine) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let id = "87858284-4a75-4d80-9c77-fdebc809cbab";
        ede_index(ede.path(), id);
        let before = std::fs::read(ede.path().join("desktops.json")).unwrap();
        let (desktops, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported, Imported { ids: vec![id.into()], kept: vec![], replaced: vec![] });
        assert_eq!(desktops.active().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), [id]);
        assert_eq!(desktops.current(), Some(id));
        // The old index is untouched, marked migrated.
        assert_eq!(std::fs::read(ede.path().join("desktops.json")).unwrap(), before);
        let marker: Value = serde_json::from_str(&std::fs::read_to_string(ede.path().join(MIGRATED_MARKER)).unwrap()).unwrap();
        assert_eq!(marker["imported"], serde_json::json!([id]));
        // Idempotent: the next open imports nothing and changes nothing.
        let (again, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported, Imported::default());
        assert_eq!(again.all(), desktops.all());
        // Even with the marker gone (a crash before it was written): nothing doubles.
        std::fs::remove_file(ede.path().join(MIGRATED_MARKER)).unwrap();
        let (again, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported.kept, [id]);
        assert_eq!(again.all().len(), 1);
    }

    #[test]
    fn an_opened_desktop_is_copied_with_its_data_and_the_ides_own_stay() {
        let (ede, machine) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // eDE's desktop, opened and written to.
        let theirs = Desktops::open(ede.path()).unwrap();
        let ede_id = theirs.current().unwrap().to_string();
        let desktop = theirs.open_desktop(&ede_id).unwrap();
        let board = desktop.store.rows("boards").unwrap()[0]["id"].as_str().unwrap().to_string();
        api::issues::issues_create(&desktop.client(), &api::issues::IssuesCreateInput::new(&board, "Written in eDE")).unwrap();
        drop(desktop);
        // The IDE's own desktop, opened too: same name, different data.
        let ours = Desktops::open(machine.path()).unwrap();
        let ide_id = ours.current().unwrap().to_string();
        drop(ours.open_desktop(&ide_id).unwrap());

        let (desktops, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported.ids, [ede_id.clone()]);
        assert!(imported.replaced.is_empty(), "an opened desktop is never replaced");
        assert_eq!(desktops.active().len(), 2);
        assert_eq!(desktops.current(), Some(ide_id.as_str()), "the IDE's current stays");
        let copied = desktops.open_desktop(&ede_id).unwrap();
        let titles: Vec<Value> = copied.store.rows("issues").unwrap().into_iter().map(|r| r["title"].clone()).collect();
        assert_eq!(titles, [serde_json::json!("Written in eDE")]);
        assert_eq!(copied.team().unwrap()["id"], serde_json::json!(ede_id));
        // The old folder is still there.
        assert!(ede.path().join("desktops").join(&ede_id).join("local.sqlite").exists());
    }

    #[test]
    fn a_never_opened_desktop_of_the_same_name_gives_way() {
        let (ede, machine) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let id = "87858284-4a75-4d80-9c77-fdebc809cbab";
        ede_index(ede.path(), id);
        // The IDE opened the index first (its own pristine Local, no folder).
        let pristine = Desktops::open(machine.path()).unwrap().current().unwrap().to_string();
        let (desktops, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported.replaced, [pristine]);
        assert_eq!(desktops.active().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), [id]);
        assert_eq!(desktops.current(), Some(id));
    }

    #[test]
    fn an_id_both_hold_keeps_the_machines_and_a_crashed_copy_is_adopted() {
        let (ede, machine) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (both, crashed) = ("11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222");
        std::fs::write(ede.path().join("desktops.json"), format!(r#"{{"current": "{both}", "desktops": [{{"id": "{both}", "name": "Theirs", "createdAt": "2026-11-01T00:00:00.000Z"}}, {{"id": "{crashed}", "name": "Work", "createdAt": "2026-11-02T00:00:00.000Z"}}]}}"#)).unwrap();
        for id in [both, crashed] {
            std::fs::create_dir_all(ede.path().join("desktops").join(id)).unwrap();
            std::fs::write(ede.path().join("desktops").join(id).join("mark"), "ede").unwrap();
        }
        std::fs::write(machine.path().join("desktops.json"), format!(r#"{{"current": "{both}", "desktops": [{{"id": "{both}", "name": "Mine", "createdAt": "2026-11-01T00:00:00.000Z"}}]}}"#)).unwrap();
        std::fs::create_dir_all(machine.path().join("desktops").join(both)).unwrap();
        std::fs::write(machine.path().join("desktops").join(both).join("mark"), "ide").unwrap();
        // A crash: the copy was renamed into place, the index never saved; a staging leftover.
        std::fs::create_dir_all(machine.path().join("desktops").join(crashed)).unwrap();
        std::fs::write(machine.path().join("desktops").join(crashed).join("mark"), "copied").unwrap();
        std::fs::create_dir_all(machine.path().join("desktops").join(format!(".import-{both}"))).unwrap();

        let (desktops, imported) = Desktops::open_importing(machine.path(), Some(ede.path())).unwrap();
        assert_eq!(imported.kept, [both]);
        assert_eq!(imported.ids, [crashed]);
        assert_eq!(desktops.get(both).unwrap().name, "Mine");
        let mark = |root: &Path, id: &str| std::fs::read_to_string(root.join("desktops").join(id).join("mark")).unwrap();
        assert_eq!(mark(machine.path(), both), "ide");
        assert_eq!(mark(machine.path(), crashed), "copied");
        assert_eq!(mark(ede.path(), both), "ede");
        assert_eq!(mark(ede.path(), crashed), "ede");
        assert!(!machine.path().join("desktops").join(format!(".import-{both}")).exists());
    }

    #[test]
    fn two_apps_on_one_index_never_drop_each_others_desktops() {
        let dir = tempfile::tempdir().unwrap();
        let mut ide = Desktops::open(dir.path()).unwrap();
        let mut ede = Desktops::open(dir.path()).unwrap();
        let first = ide.current().unwrap().to_string();
        let work = ide.create("Work").unwrap();
        // eDE's copy predates Work; its write keeps it.
        assert!(ede.set_current(&first).unwrap());
        assert!(ede.get(&work.id).is_some());
        let reopened = Desktops::open(dir.path()).unwrap();
        assert_eq!(reopened.active().len(), 2);
        assert_eq!(reopened.current(), Some(first.as_str()));
        // A reload sees the other app's rename.
        ede.rename(&work.id, "Office").unwrap();
        ide.reload();
        assert_eq!(ide.get(&work.id).unwrap().name, "Office");
        assert_eq!(ide.current(), Some(first.as_str()));
    }
}
