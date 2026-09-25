//! EXP-1102 — a DEVICE-LOCAL store of small JSON documents, one file per
//! entry, replaced by rename: `{dir}/<name>.json`.
//!
//! settings.json was the one per-install file eight subsystems across two
//! processes merged their keys into; the workflow engine's working state
//! rode in it too, and one torn write of that file cost a running workflow
//! its host (workflow 2f353e88). Engine state therefore moves OUT of the
//! preferences file into a directory of its own where every document has
//! ONE writer (the host that evaluates that workflow) and losing a document
//! costs a re-derivation, never a preference or an identity.
//!
//! Every read-modify-write takes the shared section of
//! [`crate::settings_lock`] on a sibling `.lock` file in the directory, so a
//! desktop foreground writer and its own background pass — or two processes
//! sharing a data dir — never interleave. A document that does not parse
//! reads as absent: the stores here hold caches and at-most-once
//! bookkeeping whose loss is bounded by design.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One directory of JSON documents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonStore {
    dir: PathBuf,
}

impl JsonStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", sanitize(name)))
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(".lock")
    }

    /// The document, or `None` when missing or unreadable.
    pub fn read(&self, name: &str) -> Option<Value> {
        read_value(&self.path(name))
    }

    /// Replace the document whole (by rename).
    pub fn write(&self, name: &str, value: &Value) -> std::io::Result<()> {
        let _guard = crate::settings_lock::locked_at(&self.lock_path());
        write_value(&self.path(name), value)
    }

    /// Read-modify-write under the section. `edit` sees the current document
    /// (`Null` when absent) and leaves the next one behind.
    pub fn update(&self, name: &str, edit: impl FnOnce(&mut Value)) -> std::io::Result<()> {
        let _guard = crate::settings_lock::locked_at(&self.lock_path());
        let path = self.path(name);
        let mut value = read_value(&path).unwrap_or(Value::Null);
        edit(&mut value);
        write_value(&path, &value)
    }

    /// Drop the document; a missing one is not an error.
    pub fn remove(&self, name: &str) -> std::io::Result<()> {
        let _guard = crate::settings_lock::locked_at(&self.lock_path());
        match std::fs::remove_file(self.path(name)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Every document name in the directory, sorted.
    pub fn names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        names.sort();
        names
    }
}

/// A document name is an id the caller controls (a uuid, a device id); the
/// guard only keeps a stray separator from escaping the directory.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect()
}

fn read_value(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn write_value(path: &Path, value: &Value) -> std::io::Result<()> {
    let mut rendered = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    rendered.push('\n');
    crate::atomic_file::write_atomic(path, &rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-store-{tag}-{}-{nanos}", std::process::id()));
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn documents_round_trip_list_and_remove() {
        let dir = TempDir::new("roundtrip");
        let store = JsonStore::new(dir.0.join("workflows"));
        assert_eq!(store.read("wf-1"), None, "missing reads as absent");
        assert!(store.names().is_empty());
        store.write("wf-1", &serde_json::json!({ "a": 1 })).unwrap();
        store.write("wf-2", &serde_json::json!([1, 2])).unwrap();
        assert_eq!(store.read("wf-1"), Some(serde_json::json!({ "a": 1 })));
        assert_eq!(store.names(), ["wf-1", "wf-2"]);
        store
            .update("wf-1", |value| {
                value["b"] = serde_json::json!(2);
            })
            .unwrap();
        assert_eq!(store.read("wf-1"), Some(serde_json::json!({ "a": 1, "b": 2 })));
        // An absent document is edited from Null.
        store
            .update("wf-3", |value| *value = serde_json::json!({ "fresh": true }))
            .unwrap();
        assert_eq!(store.read("wf-3"), Some(serde_json::json!({ "fresh": true })));
        store.remove("wf-1").unwrap();
        store.remove("wf-1").unwrap();
        assert_eq!(store.names(), ["wf-2", "wf-3"]);
        // The lock file lives beside the documents and is not one of them.
        assert!(dir.0.join("workflows").join(".lock").exists());
    }

    #[test]
    fn a_corrupt_document_reads_as_absent_and_is_healed_by_a_write() {
        let dir = TempDir::new("corrupt");
        let store = JsonStore::new(dir.0.join("s"));
        std::fs::create_dir_all(store.dir()).unwrap();
        std::fs::write(store.dir().join("bad.json"), "{not json").unwrap();
        assert_eq!(store.read("bad"), None);
        store.update("bad", |value| *value = serde_json::json!(1)).unwrap();
        assert_eq!(store.read("bad"), Some(serde_json::json!(1)));
    }

    #[test]
    fn a_name_never_escapes_the_directory() {
        let dir = TempDir::new("escape");
        let store = JsonStore::new(dir.0.join("s"));
        store.write("../evil", &serde_json::json!(1)).unwrap();
        assert!(!dir.0.join("evil.json").exists());
        assert_eq!(store.names(), [".._evil"]);
    }
}
