//! The attachment bytes of a Local desktop: one file per attachment id in
//! the desktop's `blobs/` directory, its content type beside it. Served to
//! the clients as `/api/attachments/{id}` over the Transport seam, read by
//! Share as a team for the upload, deleted with the row.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FsBlobStore {
    dir: PathBuf,
}

impl FsBlobStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn safe(id: &str) -> Option<&str> {
        (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then_some(id)
    }

    /// Stores the bytes under `id`; the content type beside them.
    pub fn put(&self, id: &str, bytes: &[u8], content_type: &str) -> std::io::Result<PathBuf> {
        let id = Self::safe(id).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "an attachment id"))?;
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(id);
        std::fs::write(&path, bytes)?;
        std::fs::write(self.dir.join(format!("{id}.type")), content_type.trim())?;
        Ok(path)
    }

    /// The bytes and the content type, or `None` when there is no such blob.
    pub fn get(&self, id: &str) -> Option<(Vec<u8>, Option<String>)> {
        let id = Self::safe(id)?;
        let bytes = std::fs::read(self.dir.join(id)).ok()?;
        let content_type = std::fs::read_to_string(self.dir.join(format!("{id}.type"))).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        Some((bytes, content_type))
    }

    pub fn remove(&self, id: &str) {
        if let Some(id) = Self::safe(id) {
            let _ = std::fs::remove_file(self.dir.join(id));
            let _ = std::fs::remove_file(self.dir.join(format!("{id}.type")));
        }
    }

    /// The ids stored, for the uploader.
    pub fn ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| !name.ends_with(".type"))
            .collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_bytes_and_type() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = FsBlobStore::new(dir.path().join("blobs"));
        blobs.put("a-1", b"hello", "text/plain").unwrap();
        assert_eq!(blobs.get("a-1"), Some((b"hello".to_vec(), Some("text/plain".to_string()))));
        assert_eq!(blobs.ids(), vec!["a-1".to_string()]);
        assert!(blobs.put("../x", b"", "").is_err());
        assert_eq!(blobs.get("../x"), None);
        blobs.remove("a-1");
        assert_eq!(blobs.get("a-1"), None);
    }
}
