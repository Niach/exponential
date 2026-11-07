//! The Local desktops of a machine: `desktops.json` in the host's directory
//! (eDE: `$XDG_CONFIG_HOME/exponential-desktop`), one entry per desktop with
//! its name, when it was made, and after Share as a team which team it
//! became (its file is then archived for `ARCHIVE_DAYS`, #EDE-57). Each
//! desktop owns `desktops/<id>/`: `local.sqlite` (the store), `blobs/` and
//! `desktop.json` (the sidecar: the identity, the device id, what the shapes
//! leave out). A machine always has one desktop: the first open makes
//! `Local`.

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
}

pub struct Desktops {
    root: PathBuf,
    file: DesktopsFile,
}

impl Desktops {
    /// Opens the index under `root`, making the first desktop when there is none.
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        let path = root.join("desktops.json");
        let mut file: DesktopsFile = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut desktops = Self { root, file: DesktopsFile::default() };
        if file.desktops.iter().all(|d| d.archived_at.is_some()) {
            let entry = DesktopEntry { id: uuid::Uuid::new_v4().to_string(), name: "Local".to_string(), created_at: time::now(), archived_at: None, shared_as: None };
            file.current = Some(entry.id.clone());
            file.desktops.push(entry);
        }
        if file.current.as_ref().and_then(|c| file.desktops.iter().find(|d| &d.id == c && d.archived_at.is_none())).is_none() {
            file.current = file.desktops.iter().find(|d| d.archived_at.is_none()).map(|d| d.id.clone());
        }
        file.version = 1;
        desktops.file = file;
        desktops.save()?;
        Ok(desktops)
    }

    fn save(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(&self.file).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(self.root.join("desktops.json"), text)
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
        if self.get(id).is_some_and(|d| d.archived_at.is_none()) {
            self.file.current = Some(id.to_string());
            self.save()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// A new desktop, current at once.
    pub fn create(&mut self, name: &str) -> std::io::Result<DesktopEntry> {
        let name = name.trim();
        let entry = DesktopEntry { id: uuid::Uuid::new_v4().to_string(), name: if name.is_empty() { "Local".to_string() } else { name.to_string() }, created_at: time::now(), archived_at: None, shared_as: None };
        self.file.desktops.push(entry.clone());
        self.file.current = Some(entry.id.clone());
        self.save()?;
        Ok(entry)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> std::io::Result<bool> {
        let Some(entry) = self.file.desktops.iter_mut().find(|d| d.id == id) else { return Ok(false) };
        entry.name = name.trim().to_string();
        self.save()?;
        Ok(true)
    }

    /// After Share as a team (#EDE-57): the desktop becomes the team, its
    /// files stay `ARCHIVE_DAYS` ([`Self::purge_expired`]).
    pub fn archive(&mut self, id: &str, shared_as: Option<SharedAs>) -> std::io::Result<bool> {
        let now = time::now();
        let Some(entry) = self.file.desktops.iter_mut().find(|d| d.id == id) else { return Ok(false) };
        entry.archived_at = Some(now);
        entry.shared_as = shared_as;
        if self.file.current.as_deref() == Some(id) {
            self.file.current = self.file.desktops.iter().find(|d| d.archived_at.is_none()).map(|d| d.id.clone());
        }
        self.save()?;
        Ok(true)
    }

    /// Deletes the files of desktops archived more than `ARCHIVE_DAYS` ago.
    pub fn purge_expired(&mut self, now_ms: i64) -> std::io::Result<Vec<String>> {
        let cutoff = now_ms - ARCHIVE_DAYS * 24 * 60 * 60 * 1000;
        let expired: Vec<String> = self
            .file
            .desktops
            .iter()
            .filter(|d| d.archived_at.as_deref().and_then(domain::instant::parse_instant).is_some_and(|at| at < cutoff))
            .map(|d| d.id.clone())
            .collect();
        for id in &expired {
            let _ = std::fs::remove_dir_all(self.desktop_dir(id));
            self.file.desktops.retain(|d| &d.id != id);
        }
        if !expired.is_empty() {
            self.save()?;
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
            DesktopMeta { identity: LocalIdentity::for_machine(), device_id: format!("local-{}", &entry.id[..entry.id.len().min(8)]), prompt_bodies: Default::default(), agent_instructions: Default::default(), agent_prompt: String::new() }
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
}
