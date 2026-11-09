//! #EDE-65 R (interactions-round1.json composerPaste): an image pasted into
//! a Composer's field fires the Composer's `attach` {files: [{name, size,
//! type}]} (only the images, in clipboard order; FileUpload's file shape)
//! and hands the files to the host's `on_upload` (`name` = the Composer's
//! id); the field inserts nothing. Files without an image do nothing (a
//! copied file's path is no draft); a paste without files is the field's
//! own. A clipboard image has no file name: it is written to a
//! temporary `image.<ext>` the host reads like a picked file.

use std::path::PathBuf;

use gpui::{ClipboardEntry, ClipboardItem, Context};
use serde_json::{json, Value};

use super::events::mime_of;
use super::SurfaceView;
use crate::host::UploadEvent;

/// One pasted image: its file name, MIME type and where its bytes are.
#[derive(Debug, Clone, PartialEq)]
pub struct PastedImage {
    pub name: String,
    pub mime: String,
    pub path: PathBuf,
    pub size: u64,
}

/// Does the clipboard item carry files (copied files or raw images)?
pub fn clipboard_has_files(item: &ClipboardItem) -> bool {
    item.entries().iter().any(|e| !matches!(e, ClipboardEntry::String(_)))
}

/// The images of a clipboard item, in clipboard order: copied image files
/// (by their type) and raw images (written to a temporary `image.<ext>`).
pub fn clipboard_images(item: &ClipboardItem) -> Vec<PastedImage> {
    let mut out = Vec::new();
    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                let ext = image.format.extension();
                let dir = std::env::temp_dir().join(format!("eui-paste-{}-{}", std::process::id(), image.id));
                let path = dir.join(format!("image.{ext}"));
                if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &image.bytes)).is_ok() {
                    out.push(PastedImage { name: format!("image.{ext}"), mime: image.format.mime_type().to_string(), path, size: image.bytes.len() as u64 });
                }
            }
            ClipboardEntry::ExternalPaths(paths) => {
                for p in paths.paths() {
                    let mime = mime_of(p).to_string();
                    if mime.starts_with("image/") {
                        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                        out.push(PastedImage { name: p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(), mime, path: p.clone(), size });
                    }
                }
            }
            ClipboardEntry::String(_) => {}
        }
    }
    out
}

impl SurfaceView {
    /// A paste into Composer `id` carrying `images`: `true` = taken (the
    /// host got the files, `attach` {files} fired); none = the field's own.
    /// Public for automation and tests.
    pub fn composer_paste(&mut self, id: &str, images: Vec<PastedImage>, cx: &mut Context<Self>) -> bool {
        if images.is_empty() {
            return false;
        }
        let Some(i) = self.cache.index_of(id) else { return false };
        let files: Vec<Value> = images.iter().map(|f| json!({"name": f.name, "size": f.size, "type": f.mime})).collect();
        let paths = images.into_iter().map(|f| f.path).collect();
        self.host.on_upload(&UploadEvent { surface_id: self.surface.id.clone(), component_id: id.to_string(), name: id.to_string(), paths }, cx);
        self.fire(i, "attach", Some(json!({"files": files})), cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{clipboard_has_files, clipboard_images};
    use gpui::{ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat};

    #[test]
    fn only_images_in_clipboard_order() {
        let dir = std::env::temp_dir().join(format!("eui-paste-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (jpg, pdf) = (dir.join("photo.jpg"), dir.join("spec.pdf"));
        std::fs::write(&jpg, b"12345").unwrap();
        std::fs::write(&pdf, b"1234567").unwrap();
        let item = ClipboardItem::new_string("text".into());
        assert!(clipboard_images(&item).is_empty() && !clipboard_has_files(&item), "text alone is the field's");
        let item = ClipboardItem { entries: vec![ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, b"abc".to_vec())), ClipboardEntry::ExternalPaths(ExternalPaths(vec![pdf, jpg].into()))] };
        assert!(clipboard_has_files(&item));
        let got = clipboard_images(&item);
        assert_eq!(got.iter().map(|f| (f.name.as_str(), f.mime.as_str(), f.size)).collect::<Vec<_>>(), vec![("image.png", "image/png", 3), ("photo.jpg", "image/jpeg", 5)]);
        assert_eq!(std::fs::read(&got[0].path).unwrap(), b"abc");
    }
}
