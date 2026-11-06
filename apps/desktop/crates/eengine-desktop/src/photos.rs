//! `photos`: the platform Pictures folder as the library (list, get) and the
//! platform's file picker filtered to images (pick).
//!
//! Library ids = the path relative to the folder with `/` separators; a
//! picked file outside the folder gets `picked:<n>` (this process only).
//! Cursors = an opaque offset token over the folder's photos newest first
//! (mtime, then id); the folder is rescanned per page, so a page may shift
//! when files change in between.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use eengine::providers::Failure;
use eengine::Capability;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::mcp::{self, answer};
use crate::{mime, Dialogs, NativeError, NativeResult, OpenRequest};

/// The most files one scan looks at (a huge folder still answers).
const SCAN_CAP: usize = 200_000;

pub struct PhotosCapability {
    root: Option<PathBuf>,
    dialogs: Option<Arc<dyn Dialogs>>,
    picked: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListArgs {
    #[serde(default = "default_list_limit")]
    limit: usize,
    #[serde(default)]
    cursor: Option<String>,
}

fn default_list_limit() -> usize {
    50
}

#[derive(Deserialize)]
struct GetArgs {
    id: String,
}

#[derive(Deserialize)]
struct PickArgs {
    #[serde(default = "default_pick_limit")]
    limit: usize,
}

fn default_pick_limit() -> usize {
    1
}

/// One library photo found by a scan.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub mime_type: &'static str,
    pub modified: Option<std::time::SystemTime>,
    pub taken_at: Option<String>,
}

impl PhotosCapability {
    /// `root` = the library folder (None = no library: list answers empty);
    /// `dialogs` = the platform's picker (None = pick answers `unavailable`).
    pub fn new(root: Option<PathBuf>, dialogs: Option<Arc<dyn Dialogs>>) -> Self {
        PhotosCapability { root, dialogs, picked: Mutex::new(HashMap::new()) }
    }

    fn list(&self, args: ListArgs) -> NativeResult<Value> {
        let offset = match &args.cursor {
            None => 0,
            Some(cursor) => decode_cursor(cursor).ok_or_else(|| NativeError::Failed("invalid cursor".into()))?,
        };
        let limit = args.limit.clamp(1, 100);
        let all = self.root.as_deref().map(scan).unwrap_or_default();
        let page: Vec<Value> = all.iter().skip(offset).take(limit).map(reference).collect();
        let next = offset + page.len();
        let cursor = if next < all.len() { Value::String(encode_cursor(next)) } else { Value::Null };
        Ok(json!({"photos": page, "cursor": cursor}))
    }

    fn get(&self, args: GetArgs) -> NativeResult<Value> {
        let Some(entry) = self.resolve(&args.id) else { return Err(NativeError::NotFound) };
        let bytes = mcp::read_file(&entry.path)?;
        Ok(json!({"photo": with_data(&entry, &bytes)}))
    }

    fn pick(&self, args: PickArgs) -> NativeResult<Value> {
        let Some(dialogs) = &self.dialogs else { return Err(NativeError::Unavailable) };
        let limit = args.limit.clamp(1, 50);
        let request = OpenRequest { title: "Choose photos".into(), filter: mime::photo_filter(), multiple: limit > 1 };
        let Some(paths) = dialogs.open(&request)? else { return Ok(json!({"photos": []})) };
        let mut photos = Vec::new();
        for path in paths.into_iter().take(limit) {
            let Some(entry) = self.picked_entry(&path) else { continue };
            let bytes = mcp::read_file(&entry.path)?;
            photos.push(with_data(&entry, &bytes));
        }
        Ok(json!({"photos": photos}))
    }

    /// A picked path's entry: its library id when it lies in the folder,
    /// else a `picked:<n>` id photos.get resolves for this process.
    fn picked_entry(&self, path: &Path) -> Option<Entry> {
        let name = mcp::file_name(path);
        let mime_type = mime::photo_type(&name).unwrap_or("application/octet-stream");
        let meta = std::fs::metadata(path).ok()?;
        let in_library = self.root.as_deref().and_then(|root| {
            let root = root.canonicalize().ok()?;
            let rel = path.canonicalize().ok()?.strip_prefix(&root).ok()?.to_path_buf();
            relative_id(&rel)
        });
        let id = match in_library {
            Some(id) => id,
            None => {
                let mut picked = self.picked.lock().unwrap();
                if let Some((id, _)) = picked.iter().find(|(_, p)| p.as_path() == path) {
                    id.clone()
                } else {
                    let id = format!("picked:{}", picked.len() + 1);
                    picked.insert(id.clone(), path.to_path_buf());
                    id
                }
            }
        };
        Some(Entry { id, path: path.to_path_buf(), name, mime_type, modified: meta.modified().ok(), taken_at: mcp::mtime_rfc3339(&meta) })
    }

    /// An id from list or pick → its file; None = notFound (unknown,
    /// absolute, `..`, outside the folder, not a library photo type).
    pub(crate) fn resolve(&self, id: &str) -> Option<Entry> {
        if let Some(path) = self.picked.lock().unwrap().get(id).cloned() {
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            let name = mcp::file_name(&path);
            let mime_type = mime::photo_type(&name).unwrap_or("application/octet-stream");
            return Some(Entry { id: id.to_string(), name, mime_type, modified: meta.modified().ok(), taken_at: mcp::mtime_rfc3339(&meta), path });
        }
        let root = self.root.as_deref()?;
        let rel = safe_relative(id)?;
        let path = root.join(&rel);
        // No symlink out of the folder: the real path must stay inside it.
        let real = path.canonicalize().ok()?;
        if !real.starts_with(root.canonicalize().ok()?) {
            return None;
        }
        let meta = std::fs::metadata(&real).ok().filter(|m| m.is_file())?;
        let name = mcp::file_name(&path);
        let mime_type = mime::photo_type(&name)?;
        Some(Entry { id: id.to_string(), path: real, name, mime_type, modified: meta.modified().ok(), taken_at: mcp::mtime_rfc3339(&meta) })
    }
}

impl Capability for PhotosCapability {
    fn name(&self) -> &str {
        "photos"
    }

    fn tools(&self) -> Vec<Value> {
        mcp::tools("photos")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let result = match tool {
            "list" => mcp::arguments(arguments).and_then(|a| self.list(a)),
            "get" => mcp::arguments(arguments).and_then(|a| self.get(a)),
            "pick" => mcp::arguments(arguments).and_then(|a| self.pick(a)),
            _ => return Err(mcp::unknown("photos", tool)),
        };
        Ok(answer(result))
    }
}

fn reference(entry: &Entry) -> Value {
    json!({"id": entry.id, "name": entry.name, "mimeType": entry.mime_type, "takenAt": entry.taken_at})
}

fn with_data(entry: &Entry, bytes: &[u8]) -> Value {
    json!({"id": entry.id, "name": entry.name, "mimeType": entry.mime_type, "takenAt": entry.taken_at, "data": mcp::encode(bytes)})
}

pub(crate) fn encode_cursor(offset: usize) -> String {
    format!("o{offset:x}")
}

pub(crate) fn decode_cursor(cursor: &str) -> Option<usize> {
    let hex = cursor.strip_prefix('o')?;
    if hex.is_empty() || hex.len() > 16 {
        return None;
    }
    usize::from_str_radix(hex, 16).ok()
}

/// A relative path → its id (`/` separators), None when it cannot be one.
fn relative_id(rel: &Path) -> Option<String> {
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(part) => part.to_str().map(str::to_string),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let id = parts.join("/");
    (!id.is_empty() && id.chars().count() <= 500).then_some(id)
}

/// An id → a relative path, refusing absolute ids, `..`, `.` and empty parts.
pub(crate) fn safe_relative(id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.starts_with('/') || id.contains('\\') || id.contains('\0') {
        return None;
    }
    let mut path = PathBuf::new();
    for part in id.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return None;
        }
        path.push(part);
    }
    path.components().all(|c| matches!(c, Component::Normal(_))).then_some(path)
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || name.to_ascii_lowercase().ends_with(".photoslibrary")
}

/// The folder's photos newest first (mtime, then id). Hidden files and
/// folders, the macOS Photos library package and symlinks are skipped.
pub(crate) fn scan(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    let mut seen = 0usize;
    while let Some(rel_dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(root.join(&rel_dir)) else { continue };
        for item in read.flatten() {
            seen += 1;
            if seen > SCAN_CAP {
                break;
            }
            let Some(name) = item.file_name().to_str().map(str::to_string) else { continue };
            let Ok(kind) = item.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let rel = rel_dir.join(&name);
            if kind.is_dir() {
                if !skipped_dir(&name) {
                    stack.push(rel);
                }
                continue;
            }
            if name.starts_with('.') || !kind.is_file() {
                continue;
            }
            let Some(mime_type) = mime::photo_type(&name) else { continue };
            let Some(id) = relative_id(&rel) else { continue };
            if name.chars().count() > 255 {
                continue;
            }
            let meta = item.metadata().ok();
            out.push(Entry {
                id,
                path: root.join(&rel),
                name,
                mime_type,
                modified: meta.as_ref().and_then(|m| m.modified().ok()),
                taken_at: meta.as_ref().and_then(mcp::mtime_rfc3339),
            });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
    out
}
