//! The chat composer's attachments (#EDE-65 lane L): what `ede_chat attach`
//! picked, staged until the send uploads it, the IDE's rules
//! (`ui::composer_images`): images up to 4 × 10 MB and files up to 4 × 50 MB,
//! one rejection notice, sequential idempotent uploads
//! (`api::attachment_upload::upload_all`) so a retry uploads only the rest.
//! The strip above the composer is genUI Chips (`/chatFiles`): the image's
//! own picture (`image`, a `data:` url; "" = none) in the chip's leading
//! slot, else the file's glyph, each removable.

use std::path::Path;
use std::sync::Arc;

use api::attachment_upload::{self, UploadJob, Uploaded};
use base64::Engine as _;
use domain::image_message::{MAX_STEER_FILES, MAX_STEER_IMAGES};
use serde_json::{json, Value};

/// The one rejection notice ×4 (type/size/empty), the IDE's.
pub const ATTACH_REJECTED: &str = "Images up to 10 MB and files up to 50 MB can be attached";

/// A chip's picture is the image itself up to this size (a `data:` url);
/// a bigger one shows the image glyph.
const CHIP_PICTURE_MAX: usize = 2 * 1024 * 1024;

/// One staged attachment.
#[derive(Clone)]
pub struct Pending {
    pub key: u64,
    pub filename: String,
    pub content_type: String,
    pub bytes: Arc<[u8]>,
    /// Set once it landed: a retry never uploads it again.
    pub uploaded_id: Option<String>,
    pub uploaded_name: Option<String>,
}

/// The composer's staged attachments.
#[derive(Default)]
pub struct ChatFiles {
    pending: Vec<Pending>,
    next_key: u64,
}

impl ChatFiles {
    fn count(&self, image: bool) -> usize {
        self.pending.iter().filter(|p| attachment_upload::is_inline_image(&p.content_type) == image).count()
    }

    /// Stages `(filename, content type, bytes)` entries under the caps; the
    /// notice for what did not fit (`None` = all staged).
    pub fn stage(&mut self, entries: Vec<(String, String, Vec<u8>)>) -> Option<String> {
        let mut notice = None;
        for (filename, content_type, bytes) in entries {
            if bytes.is_empty() || bytes.len() > attachment_upload::max_upload_bytes_for(&content_type) {
                notice = Some(ATTACH_REJECTED.to_string());
                continue;
            }
            let image = attachment_upload::is_inline_image(&content_type);
            if image && self.count(true) >= MAX_STEER_IMAGES {
                notice = Some(format!("Up to {MAX_STEER_IMAGES} images per message"));
                continue;
            }
            if !image && self.count(false) >= MAX_STEER_FILES {
                notice = Some(format!("Up to {MAX_STEER_FILES} files per message"));
                continue;
            }
            self.next_key += 1;
            self.pending.push(Pending { key: self.next_key, filename, content_type, bytes: bytes.into(), uploaded_id: None, uploaded_name: None });
        }
        notice
    }

    /// Stages picked paths, each read whole under its type's cap.
    pub fn stage_paths(&mut self, paths: &[impl AsRef<Path>]) -> Option<String> {
        let mut notice = None;
        let mut entries = Vec::new();
        for path in paths {
            match attachment_upload::read_any_file(path.as_ref()) {
                Ok(entry) => entries.push(entry),
                Err(_) => notice = Some(ATTACH_REJECTED.to_string()),
            }
        }
        self.stage(entries).or(notice)
    }

    pub fn remove(&mut self, key: u64) {
        self.pending.retain(|p| p.key != key);
    }

    /// The uploads to perform, in strip order.
    pub fn jobs(&self) -> Vec<UploadJob<Arc<[u8]>>> {
        self.pending
            .iter()
            .map(|p| UploadJob { key: p.key, uploaded_id: p.uploaded_id.clone(), uploaded_name: p.uploaded_name.clone(), filename: p.filename.clone(), content_type: p.content_type.clone(), bytes: p.bytes.clone() })
            .collect()
    }

    /// Records what landed, so a retry uploads only the rest.
    pub fn note_uploaded(&mut self, resolved: &[Uploaded]) {
        for (key, id, name) in resolved {
            if let Some(p) = self.pending.iter_mut().find(|p| p.key == *key) {
                p.uploaded_id = Some(id.clone());
                p.uploaded_name = Some(name.clone());
            }
        }
    }

    /// A send landed: what went out leaves the strip, what was added
    /// meanwhile stays.
    pub fn settle_sent(&mut self, sent: &[u64]) {
        self.pending.retain(|p| !sent.contains(&p.key));
    }

    /// `/chatFiles`: the strip's chips (`open` = any staged).
    pub fn feed(&self) -> Value {
        let items: Vec<Value> = self
            .pending
            .iter()
            .map(|p| {
                let image = attachment_upload::is_inline_image(&p.content_type);
                // "" = no picture: the chip shows the glyph.
                let picture = if image && p.bytes.len() <= CHIP_PICTURE_MAX {
                    format!("data:{};base64,{}", p.content_type, base64::engine::general_purpose::STANDARD.encode(&p.bytes))
                } else {
                    String::new()
                };
                json!({
                    "key": p.key.to_string(),
                    "label": p.filename,
                    "image": picture,
                    "icon": if image { "image" } else { "file" },
                })
            })
            .collect();
        json!({"open": !items.is_empty(), "items": items})
    }
}

/// The attachment ids a send carries, in strip order.
pub fn attachment_ids(resolved: &[Uploaded]) -> Vec<String> {
    resolved.iter().map(|(_, id, _)| id.clone()).collect()
}

/// The demo's staged screenshot (the goldens, `EDE_SHELL_DEMO_ATTACH`): a
/// 2×2 PNG under the name it was picked as.
pub fn demo() -> ChatFiles {
    let mut files = ChatFiles::default();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(&[0x6d, 0x28, 0xd9, 0xff].repeat(4)).expect("png data");
    }
    files.stage(vec![("login-error.png".into(), "image/png".into(), png)]);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, content_type: &str, len: usize) -> (String, String, Vec<u8>) {
        (name.into(), content_type.into(), vec![1; len])
    }

    #[test]
    fn images_and_files_cap_apart_and_a_bad_pick_is_the_one_notice() {
        let mut files = ChatFiles::default();
        assert_eq!(files.stage(vec![entry("a.png", "image/png", 3), entry("spec.pdf", "application/pdf", 3)]), None);
        assert_eq!(files.stage(vec![entry("empty.png", "image/png", 0)]).as_deref(), Some(ATTACH_REJECTED));
        assert_eq!(files.stage(vec![entry("big.png", "image/png", attachment_upload::MAX_IMAGE_UPLOAD_BYTES + 1)]).as_deref(), Some(ATTACH_REJECTED));
        let four: Vec<_> = (0..4).map(|i| entry(&format!("{i}.png"), "image/png", 3)).collect();
        assert_eq!(files.stage(four).as_deref(), Some("Up to 4 images per message"));
        assert_eq!(files.count(true), 4);
        assert_eq!(files.count(false), 1);
    }

    #[test]
    fn the_strip_is_chips_with_the_image_s_picture_and_a_file_s_glyph() {
        let mut files = ChatFiles::default();
        assert_eq!(files.feed(), json!({"open": false, "items": []}));
        files.stage(vec![entry("a.png", "image/png", 3), entry("spec.pdf", "application/pdf", 3)]);
        let feed = files.feed();
        assert_eq!(feed["open"], json!(true));
        assert_eq!(feed["items"][0], json!({"key": "1", "label": "a.png", "image": "data:image/png;base64,AQEB", "icon": "image"}));
        assert_eq!(feed["items"][1], json!({"key": "2", "label": "spec.pdf", "image": "", "icon": "file"}));
        files.remove(1);
        assert_eq!(files.feed()["items"].as_array().unwrap().len(), 1);
    }

    /// The upload is idempotent: what landed is reused on the retry, and a
    /// landed send takes out only what went out.
    #[test]
    fn a_retry_uploads_only_the_rest_and_a_send_keeps_what_was_added_meanwhile() {
        let mut files = ChatFiles::default();
        files.stage(vec![entry("a.png", "image/png", 3), entry("b.png", "image/png", 3)]);
        files.note_uploaded(&[(1, "att-1".into(), "a.png".into())]);
        let calls = std::cell::RefCell::new(Vec::new());
        let resolved = attachment_upload::upload_all(files.jobs(), |name, _, _| {
            calls.borrow_mut().push(name.to_string());
            Ok(serde_json::from_value(json!({"id": "att-2", "url": "/api/attachments/att-2"})).unwrap())
        })
        .unwrap();
        assert_eq!(calls.into_inner(), vec!["b.png".to_string()]);
        assert_eq!(attachment_ids(&resolved), vec!["att-1".to_string(), "att-2".to_string()]);
        let sent: Vec<u64> = resolved.iter().map(|(key, _, _)| *key).collect();
        files.stage(vec![entry("c.png", "image/png", 3)]);
        files.settle_sent(&sent);
        assert_eq!(files.feed()["items"][0]["label"], json!("c.png"));
    }

    #[test]
    fn the_demo_stages_one_screenshot() {
        let feed = demo().feed();
        assert_eq!(feed["items"][0]["label"], json!("login-error.png"));
        assert!(feed["items"][0]["image"].as_str().unwrap().starts_with("data:image/png;base64,"));
    }
}
