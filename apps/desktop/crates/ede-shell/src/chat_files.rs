//! The chat composer's attachments (#EDE-65 lane L): what `ede_chat attach`
//! picked (or a paste dropped), staged until the send uploads it, with
//! sequential idempotent uploads (`api::attachment_upload::upload_all`) so a
//! retry uploads only the rest. The strip's rules are every app composer's
//! (#EXP-1845): `domain::composer_attachments`, the Rust mirror of
//! `composer-attachments.json`, owns the caps (four images, four files),
//! the `[Image #N]` markers dropped at the field's caret, the X's
//! renumbering and the toasts; this module only classifies a pick (an
//! inline image type up to 10 MB = image, any other file up to 50 MB =
//! file, empty, unreadable or over its cap = refused) and keeps the bytes.
//! The strip above the composer is genUI Chips (`/chatFiles`): the image's
//! own picture (`image`, a `data:` url; "" = none) in the chip's leading
//! slot, else the file's glyph, each removable.

use std::path::Path;
use std::sync::Arc;

use api::attachment_upload::{self, UploadJob, Uploaded};
use base64::Engine as _;
use domain::composer_attachments::{clamp_to_char_boundary, drop_attachment, stage_attachments, AttachmentKind, PickKind};
use serde_json::{json, Value};

/// A chip's picture is the image itself up to this size (a `data:` url);
/// a bigger one shows the image glyph.
const CHIP_PICTURE_MAX: usize = 2 * 1024 * 1024;

/// One picked entry: (filename, content type, bytes).
pub type Picked = (String, String, Vec<u8>);

/// One staged attachment.
#[derive(Clone)]
pub struct Pending {
    pub key: u64,
    pub filename: String,
    pub content_type: String,
    pub bytes: Arc<[u8]>,
    /// A file (no marker) rather than an image.
    pub is_file: bool,
    /// Set once it landed: a retry never uploads it again.
    pub uploaded_id: Option<String>,
    pub uploaded_name: Option<String>,
}

/// What a pick did to the composer.
#[derive(Debug, PartialEq, Eq)]
pub struct StageOutcome {
    /// The markers to type into the field: (byte offset, slice), `None` =
    /// no image was taken.
    pub inserted: Option<(usize, String)>,
    /// The draft after the pick.
    pub text: String,
    /// The error toasts the pick owes, in order (each its own toast).
    pub toasts: Vec<&'static str>,
}

/// The composer's staged attachments.
#[derive(Default)]
pub struct ChatFiles {
    pending: Vec<Pending>,
    next_key: u64,
}

/// A picked entry classified the app composers' way.
fn classify((_, content_type, bytes): &Picked) -> PickKind {
    if bytes.is_empty() || bytes.len() > attachment_upload::max_upload_bytes_for(content_type) {
        PickKind::Rejected
    } else if attachment_upload::is_inline_image(content_type) {
        PickKind::Image
    } else {
        PickKind::File
    }
}

impl ChatFiles {
    /// The strip as `domain::composer_attachments` reads it.
    fn kinds(&self) -> Vec<AttachmentKind> {
        self.pending.iter().map(|p| if p.is_file { AttachmentKind::File } else { AttachmentKind::Image }).collect()
    }

    /// Stages `entries` (pick order) against the draft `text` with its
    /// `caret` (bytes): `domain::composer_attachments::stage_attachments`.
    pub fn stage(&mut self, entries: Vec<Picked>, text: &str, caret: usize) -> StageOutcome {
        let picks: Vec<PickKind> = entries.iter().map(classify).collect();
        let at = clamp_to_char_boundary(text, caret);
        let staged = stage_attachments(&self.kinds(), &picks, text, at);
        for ((filename, content_type, bytes), (pick, taken)) in entries.into_iter().zip(picks.iter().zip(&staged.taken)) {
            if !taken {
                continue;
            }
            self.next_key += 1;
            self.pending.push(Pending { key: self.next_key, filename, content_type, bytes: bytes.into(), is_file: *pick == PickKind::File, uploaded_id: None, uploaded_name: None });
        }
        // Markers only ever go in at the caret: the draft after = the draft
        // before with one slice at `at`.
        let inserted = (staged.caret > at).then(|| staged.text.get(at..staged.caret).map(|slice| (at, slice.to_string()))).flatten();
        StageOutcome { inserted, toasts: staged.toasts(), text: staged.text }
    }

    /// Stages picked paths, each read whole under its type's cap (one that
    /// cannot be read is refused like an over-cap one).
    pub fn stage_paths(&mut self, paths: &[impl AsRef<Path>], text: &str, caret: usize) -> StageOutcome {
        let entries = paths
            .iter()
            .map(|path| attachment_upload::read_any_file(path.as_ref()).unwrap_or_else(|_| (String::new(), String::new(), Vec::new())))
            .collect();
        self.stage(entries, text, caret)
    }

    /// The chip's X on `key` (`domain::composer_attachments::drop_attachment`):
    /// an image's markers leave the draft and the higher ones slide down.
    /// Returns the draft with the caret (bytes) carried along, `None` when
    /// the text stays.
    pub fn remove(&mut self, key: u64, text: &str, caret: usize) -> Option<(String, usize)> {
        let at = self.pending.iter().position(|p| p.key == key)?;
        let (_, next, caret) = drop_attachment(&self.kinds(), at, text, caret);
        self.pending.remove(at);
        (next != text).then_some((next, caret))
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
                let image = !p.is_file;
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
    files.stage(vec![("login-error.png".into(), "image/png".into(), png)], "", 0);
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::composer_attachments::{ATTACHMENT_REJECTED_TOAST, FILE_CAP_TOAST, IMAGE_CAP_TOAST};
    use serde::Deserialize;

    const COMPOSER_ATTACHMENTS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/composer-attachments.json");

    fn entry(name: &str, content_type: &str, len: usize) -> Picked {
        (name.into(), content_type.into(), vec![1; len])
    }

    /// A fixture kind as a real pick: an image, a file, or an empty one.
    fn picked(kind: &str, n: usize) -> Picked {
        match kind {
            "image" => entry(&format!("{n}.png"), "image/png", 3),
            "file" => entry(&format!("{n}.pdf"), "application/pdf", 3),
            "rejected" => entry(&format!("{n}.png"), "image/png", 0),
            other => panic!("unknown kind {other}"),
        }
    }

    fn strip(files: &ChatFiles) -> Vec<String> {
        files.pending.iter().map(|p| if p.is_file { "file" } else { "image" }.to_string()).collect()
    }

    fn seeded(kinds: &[String]) -> ChatFiles {
        let mut files = ChatFiles::default();
        for (n, kind) in kinds.iter().enumerate() {
            // Seeded straight in: the fixture's draft already holds its markers.
            let (filename, content_type, bytes) = picked(kind, n);
            files.next_key += 1;
            files.pending.push(Pending { key: files.next_key, filename, content_type, bytes: bytes.into(), is_file: kind == "file", uploaded_id: None, uploaded_name: None });
        }
        files
    }

    #[derive(Deserialize)]
    struct Case<I, E> {
        name: String,
        input: I,
        expected: E,
    }

    #[derive(Deserialize)]
    struct StageInput {
        strip: Vec<String>,
        add: Vec<String>,
        text: String,
        caret: usize,
    }

    #[derive(Deserialize)]
    struct StageExpected {
        strip: Vec<String>,
        text: String,
        caret: usize,
        toasts: Vec<String>,
    }

    #[derive(Deserialize)]
    struct DropInput {
        strip: Vec<String>,
        at: usize,
        text: String,
        caret: usize,
    }

    #[derive(Deserialize)]
    struct DropExpected {
        strip: Vec<String>,
        text: String,
        caret: usize,
    }

    #[derive(Deserialize)]
    struct Fixture {
        stage: Vec<Case<StageInput, StageExpected>>,
        drop: Vec<Case<DropInput, DropExpected>>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(COMPOSER_ATTACHMENTS_JSON).expect("composer-attachments.json parses")
    }

    /// #EXP-1845: the shell's composer replays the app composers' fixture
    /// (its texts are ASCII: the fixture's UTF-16 offsets are bytes here).
    #[test]
    fn replays_every_stage_case_of_the_shared_fixture() {
        let cases = fixture().stage;
        assert!(!cases.is_empty());
        for case in cases {
            let mut files = seeded(&case.input.strip);
            let add = case.input.add.iter().enumerate().map(|(n, kind)| picked(kind, 100 + n)).collect();
            let outcome = files.stage(add, &case.input.text, case.input.caret);
            assert_eq!(strip(&files), case.expected.strip, "{}", case.name);
            assert_eq!(outcome.text, case.expected.text, "{}", case.name);
            assert_eq!(outcome.toasts, case.expected.toasts, "{}", case.name);
            // The field types the slice at the caret, its caret behind it.
            let (typed, caret) = match &outcome.inserted {
                Some((at, slice)) => {
                    let mut typed = case.input.text.clone();
                    typed.insert_str(*at, slice);
                    (typed, at + slice.len())
                }
                None => (case.input.text.clone(), case.input.caret.min(case.input.text.len())),
            };
            assert_eq!((typed, caret), (case.expected.text.clone(), case.expected.caret), "{}", case.name);
        }
    }

    #[test]
    fn replays_every_drop_case_of_the_shared_fixture() {
        let cases = fixture().drop;
        assert!(!cases.is_empty());
        for case in cases {
            let mut files = seeded(&case.input.strip);
            // An X on no entry (out of range) changes nothing.
            let key = files.pending.get(case.input.at).map_or(u64::MAX, |p| p.key);
            let (text, caret) = files.remove(key, &case.input.text, case.input.caret).unwrap_or((case.input.text.clone(), case.input.caret));
            assert_eq!(strip(&files), case.expected.strip, "{}", case.name);
            assert_eq!((text, caret), (case.expected.text, case.expected.caret), "{}", case.name);
        }
    }

    #[test]
    fn a_refused_pick_toasts_the_shared_copy_and_caps_count_apart() {
        let mut files = ChatFiles::default();
        let outcome = files.stage(vec![entry("a.png", "image/png", 3), entry("spec.pdf", "application/pdf", 3)], "see", 3);
        assert_eq!(outcome, StageOutcome { inserted: Some((3, " [Image #1]".into())), text: "see [Image #1]".into(), toasts: vec![] });
        assert_eq!(files.stage(vec![entry("empty.png", "image/png", 0)], "", 0).toasts, vec![ATTACHMENT_REJECTED_TOAST]);
        assert_eq!(files.stage(vec![entry("big.png", "image/png", attachment_upload::MAX_IMAGE_UPLOAD_BYTES + 1)], "", 0).toasts, vec![ATTACHMENT_REJECTED_TOAST]);
        let mut picks: Vec<_> = (0..4).map(|i| entry(&format!("{i}.png"), "image/png", 3)).collect();
        picks.extend((0..4).map(|i| entry(&format!("{i}.txt"), "text/plain", 3)));
        assert_eq!(files.stage(picks, "", 0).toasts, vec![IMAGE_CAP_TOAST, FILE_CAP_TOAST]);
        assert_eq!(strip(&files).iter().filter(|k| *k == "image").count(), 4);
        assert_eq!(strip(&files).iter().filter(|k| *k == "file").count(), 4);
        let unreadable = files.stage_paths(&["/nonexistent/shot.png"], "", 0);
        assert_eq!(unreadable.toasts, vec![ATTACHMENT_REJECTED_TOAST]);
    }

    #[test]
    fn the_strip_is_chips_with_the_image_s_picture_and_a_file_s_glyph() {
        let mut files = ChatFiles::default();
        assert_eq!(files.feed(), json!({"open": false, "items": []}));
        files.stage(vec![entry("a.png", "image/png", 3), entry("spec.pdf", "application/pdf", 3)], "", 0);
        let feed = files.feed();
        assert_eq!(feed["open"], json!(true));
        assert_eq!(feed["items"][0], json!({"key": "1", "label": "a.png", "image": "data:image/png;base64,AQEB", "icon": "image"}));
        assert_eq!(feed["items"][1], json!({"key": "2", "label": "spec.pdf", "image": "", "icon": "file"}));
        assert_eq!(files.remove(2, "[Image #1] x", 12), None);
        assert_eq!(files.remove(1, "[Image #1] x", 12), Some(("x".to_string(), 1)));
        assert_eq!(files.feed()["items"].as_array().unwrap().len(), 0);
    }

    /// The upload is idempotent: what landed is reused on the retry, and a
    /// landed send takes out only what went out.
    #[test]
    fn a_retry_uploads_only_the_rest_and_a_send_keeps_what_was_added_meanwhile() {
        let mut files = ChatFiles::default();
        files.stage(vec![entry("a.png", "image/png", 3), entry("b.png", "image/png", 3)], "", 0);
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
        files.stage(vec![entry("c.png", "image/png", 3)], "", 0);
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
