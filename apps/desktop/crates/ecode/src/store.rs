//! `Store`: review and run objects, content-addressed, plus named refs whose
//! [`Store::update_ref`] is the ONE serialization point: every writer reads a
//! ref, writes new objects and swaps the ref from the value it read. The
//! Exponential server backs it with Postgres; without a server a host picks
//! its rule (a [`FileStore`] here; delegates plus a threshold elsewhere).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

use crate::{Result, StoreError};

/// What an object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    /// Review objects: PRs, their events.
    Review,
    /// CI objects: runs, jobs, logs.
    Run,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Run => "run",
        }
    }
}

/// Review and run objects plus refs.
pub trait Store: Send + Sync {
    /// The object's bytes; `None` = unknown id.
    fn get(&self, kind: ObjectKind, id: &str) -> Result<Option<Vec<u8>>, StoreError>;
    /// Stores the bytes and returns their content id (idempotent).
    fn put(&self, kind: ObjectKind, bytes: &[u8]) -> Result<String, StoreError>;
    /// The object id a ref names; `None` = no such ref.
    fn read_ref(&self, name: &str) -> Result<Option<String>, StoreError>;
    /// Compare-and-swap: moves `name` from `expected` (`None` = must not
    /// exist) to `new` (`None` = delete). `Conflict` when it holds anything
    /// else; nothing changes then.
    fn update_ref(&self, name: &str, expected: Option<&str>, new: Option<&str>) -> Result<(), StoreError>;
}

/// JSON on top of any [`Store`].
pub trait StoreExt: Store {
    fn get_json<T: DeserializeOwned>(&self, kind: ObjectKind, id: &str) -> Result<Option<T>> {
        let Some(bytes) = self.get(kind, id)? else { return Ok(None) };
        serde_json::from_slice(&bytes).map(Some).map_err(|e| StoreError::Io(format!("object {id}: {e}")).into())
    }

    fn put_json<T: Serialize>(&self, kind: ObjectKind, value: &T) -> Result<String> {
        let bytes = serde_json::to_vec(value).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(self.put(kind, &bytes)?)
    }
}

impl<S: Store + ?Sized> StoreExt for S {}

/// The content id: sha256, lowercase hex.
pub fn content_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `[A-Za-z0-9._-]` segments joined by `/`, never `.`/`..`/empty, no `.lock`.
fn valid_ref_name(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(".lock")
        && name.split('/').all(|s| {
            !s.is_empty() && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
}

fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

type Objects = BTreeMap<(ObjectKind, String), Vec<u8>>;
type Refs = BTreeMap<String, String>;

/// An in-memory store (tests, hosts with their own persistence).
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<(Objects, Refs)>,
}

impl Store for MemoryStore {
    fn get(&self, kind: ObjectKind, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.inner.lock().unwrap().0.get(&(kind, id.to_string())).cloned())
    }

    fn put(&self, kind: ObjectKind, bytes: &[u8]) -> Result<String, StoreError> {
        let id = content_id(bytes);
        self.inner.lock().unwrap().0.insert((kind, id.clone()), bytes.to_vec());
        Ok(id)
    }

    fn read_ref(&self, name: &str) -> Result<Option<String>, StoreError> {
        Ok(self.inner.lock().unwrap().1.get(name).cloned())
    }

    fn update_ref(&self, name: &str, expected: Option<&str>, new: Option<&str>) -> Result<(), StoreError> {
        if !valid_ref_name(name) {
            return Err(StoreError::InvalidName(name.into()));
        }
        let mut inner = self.inner.lock().unwrap();
        let actual = inner.1.get(name).cloned();
        if actual.as_deref() != expected {
            return Err(StoreError::Conflict { name: name.into(), actual });
        }
        match new {
            Some(new) => inner.1.insert(name.into(), new.into()),
            None => inner.1.remove(name),
        };
        Ok(())
    }
}

/// A store in a directory: `objects/<kind>/<id>` and `refs/<name>`. Refs
/// swap under a `<name>.lock` file created exclusively, so several
/// processes on one machine serialize on it.
pub struct FileStore {
    root: PathBuf,
}

/// How long `update_ref` waits for another writer's lock.
const LOCK_WAIT: Duration = Duration::from_secs(5);

impl FileStore {
    /// Opens (creating) the store at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        for dir in ["objects/review", "objects/run", "refs"] {
            fs::create_dir_all(root.join(dir)).map_err(io)?;
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn object_path(&self, kind: ObjectKind, id: &str) -> PathBuf {
        self.root.join("objects").join(kind.as_str()).join(id)
    }

    fn ref_path(&self, name: &str) -> PathBuf {
        self.root.join("refs").join(name)
    }
}

fn io(e: std::io::Error) -> StoreError {
    StoreError::Io(e.to_string())
}

/// Writes through a temp file + rename: a reader never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp-{}-{seq}", std::process::id()));
    let mut file = fs::File::create(&tmp).map_err(io)?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(io)?;
    fs::rename(&tmp, path).map_err(io)
}

fn read_trimmed(path: &Path) -> Result<Option<String>, StoreError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(e)),
    }
}

impl Store for FileStore {
    fn get(&self, kind: ObjectKind, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        if !valid_id(id) {
            return Ok(None);
        }
        match fs::read(self.object_path(kind, id)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn put(&self, kind: ObjectKind, bytes: &[u8]) -> Result<String, StoreError> {
        let id = content_id(bytes);
        let path = self.object_path(kind, &id);
        if !path.exists() {
            write_atomic(&path, bytes)?;
        }
        Ok(id)
    }

    fn read_ref(&self, name: &str) -> Result<Option<String>, StoreError> {
        if !valid_ref_name(name) {
            return Err(StoreError::InvalidName(name.into()));
        }
        read_trimmed(&self.ref_path(name))
    }

    fn update_ref(&self, name: &str, expected: Option<&str>, new: Option<&str>) -> Result<(), StoreError> {
        if !valid_ref_name(name) {
            return Err(StoreError::InvalidName(name.into()));
        }
        let path = self.ref_path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let lock_path = path.with_file_name(format!("{}.lock", path.file_name().unwrap().to_string_lossy()));
        let started = std::time::Instant::now();
        let mut lock = loop {
            match fs::OpenOptions::new().write(true).create_new(true).open(&lock_path) {
                Ok(file) => break file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && started.elapsed() < LOCK_WAIT => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(io(e)),
            }
        };
        let actual = match read_trimmed(&path) {
            Ok(actual) => actual,
            Err(e) => {
                let _ = fs::remove_file(&lock_path);
                return Err(e);
            }
        };
        if actual.as_deref() != expected {
            let _ = fs::remove_file(&lock_path);
            return Err(StoreError::Conflict { name: name.into(), actual });
        }
        let result = match new {
            // The rename releases the lock: never remove `lock_path` after
            // it, another writer may hold it by then.
            Some(new) => match lock.write_all(new.as_bytes()).and_then(|()| lock.sync_all()) {
                Ok(()) => fs::rename(&lock_path, &path).map_err(io),
                Err(e) => Err(io(e)),
            },
            None => match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
                _ => Ok(()),
            },
        };
        if new.is_none() || result.is_err() {
            let _ = fs::remove_file(&lock_path);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(store: &dyn Store) {
        let a = store.put(ObjectKind::Review, b"{\"a\":1}").unwrap();
        assert_eq!(store.put(ObjectKind::Review, b"{\"a\":1}").unwrap(), a);
        assert_eq!(store.get(ObjectKind::Review, &a).unwrap().unwrap(), b"{\"a\":1}");
        assert_eq!(store.get(ObjectKind::Run, &a).unwrap(), None);
        store.update_ref("forge/prs", None, Some(&a)).unwrap();
        let b = store.put(ObjectKind::Review, b"{}").unwrap();
        assert_eq!(
            store.update_ref("forge/prs", None, Some(&b)),
            Err(StoreError::Conflict { name: "forge/prs".into(), actual: Some(a.clone()) })
        );
        store.update_ref("forge/prs", Some(&a), Some(&b)).unwrap();
        assert_eq!(store.read_ref("forge/prs").unwrap(), Some(b.clone()));
        store.update_ref("forge/prs", Some(&b), None).unwrap();
        assert_eq!(store.read_ref("forge/prs").unwrap(), None);
        assert!(matches!(store.update_ref("../x", None, Some(&a)), Err(StoreError::InvalidName(_))));
    }

    #[test]
    fn memory_store_swaps() {
        exercise(&MemoryStore::default());
    }

    #[test]
    fn file_store_swaps_and_serializes_writers() {
        let dir = std::env::temp_dir().join(format!("ecode-store-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        let store = FileStore::open(&dir).unwrap();
        exercise(&store);
        // Eight writers bump one counter by CAS; none is lost.
        let store = std::sync::Arc::new(store);
        let zero = store.put(ObjectKind::Run, b"0").unwrap();
        store.update_ref("counter", None, Some(&zero)).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                std::thread::spawn(move || loop {
                    let current = store.read_ref("counter").unwrap().unwrap();
                    let n: u32 = String::from_utf8(store.get(ObjectKind::Run, &current).unwrap().unwrap()).unwrap().parse().unwrap();
                    let next = store.put(ObjectKind::Run, (n + 1).to_string().as_bytes()).unwrap();
                    match store.update_ref("counter", Some(&current), Some(&next)) {
                        Ok(()) => break,
                        Err(StoreError::Conflict { .. }) => continue,
                        Err(e) => panic!("{e}"),
                    }
                })
            })
            .collect();
        handles.into_iter().for_each(|h| h.join().unwrap());
        let last = store.read_ref("counter").unwrap().unwrap();
        assert_eq!(store.get(ObjectKind::Run, &last).unwrap().unwrap(), b"8");
        let _ = fs::remove_dir_all(&dir);
    }
}
