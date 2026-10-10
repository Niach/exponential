//! EXP-825 — the ONE pending-images strip of a message composer: staged
//! bytes, `[Image #N]` markers, the thumbnail strip, sequential idempotent
//! uploads. Two composers use it: the steer viewer's reply composer (uploads
//! to the SESSION route, EXP-702) and the Agent page's start composer
//! (uploads to the TEAM route BEFORE a session exists — the start binds them
//! through `codingSessions.start`'s `attachmentIds`). The strip and its rules
//! used to be private to `steer_viewer`; the start composer needed the same
//! behaviour byte for byte (web `lib/pending-images.ts` parity), so it moved
//! here.
//!
//! EXP-698: staging the k-th image also drops `[Image #k]` at the caret, so a
//! sentence can NAME the picture it means ("crop [Image #2]") and the agent's
//! numbered manifest lines up with it. Removing one renumbers the markers
//! behind it. The insertion is the contract's (`insert_image_marker`) — only
//! the caret handling is the component's, so a marker never splits a word.
//!
//! Wave D: the strip takes ANY file too. A non-image (or a non-inline image
//! type) stages as a FILE tile — the Files section's type glyph + filename +
//! the same ✕ — with NO marker; files cap at [`MAX_STEER_FILES`] × 50 MB
//! beside the 4 × 10 MB images, upload through the same sequential idempotent
//! path and ride the message's file block (`steer::build_steer_message`).

use std::sync::Arc;

use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, StyledImage as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::TextareaState;
use gpui_component::{h_flex, ActiveTheme as _, Disableable as _, Sizable as _};

use steer::{
    build_steer_message, insert_image_marker, renumber_image_markers, SteerFile, MAX_STEER_FILES,
    MAX_STEER_IMAGES,
};

use crate::icons::registry;
use crate::markdown::image_paste::{
    self, max_upload_bytes_for, pasted_image_parts, ACCEPTED_IMAGE_CONTENT_TYPES,
};

/// Wave D: the ONE rejection notice ×4 (type/size/empty).
pub(crate) const ATTACH_REJECTED: &str = "Images up to 10 MB and files up to 50 MB can be attached";

/// Wave D: the file cap notice ×4.
pub(crate) const FILES_CAP: &str = "Up to 4 files per message";

/// Release train 2026-10-10: the pinned sentence ×4 (and the server's
/// refusal) when the target machine lacks the `steer-files` cap — an older
/// host that localizes images only, so a file line would reach its agent as
/// an unusable link. The pick then takes images alone.
pub(crate) const FILES_NEED_NEWER_DEVICE: &str =
    domain::contract::COMPOSER_UI_FILES_NEED_NEWER_DEVICE;

/// The FILE tile's width — glyph + a truncated filename.
const FILE_TILE_W: f32 = 160.;

/// The thumbnail tile's edge (web `PendingImageStrip` parity).
const PENDING_THUMB: f32 = 48.;

/// One image staged in a composer, uploaded on send.
pub(crate) struct PendingImage {
    pub(crate) key: u64,
    pub(crate) filename: String,
    pub(crate) content_type: String,
    /// EXP-698: the staged bytes, wrapped for `img()` — the thumbnail's
    /// source AND the upload's. `gpui::Image` owns a public `bytes: Vec<u8>`,
    /// so the ONE buffer serves both: a separate `Arc<Vec<u8>>` beside it
    /// would hold a second copy of every pasted screenshot for as long as the
    /// draft lives. Built once here, never per repaint.
    pub(crate) preview: Arc<gpui::Image>,
    /// Set once the attachment landed — a retry after a mid-batch failure
    /// never re-uploads what already succeeded.
    pub(crate) uploaded_id: Option<String>,
    /// Wave D: the server's sanitized filename from the upload response —
    /// a file line's link text.
    pub(crate) uploaded_name: Option<String>,
    /// Wave D: a non-image FILE (no marker, no thumbnail, its own cap).
    pub(crate) is_file: bool,
}

/// One upload the sender performs, snapshotted off the strip so the
/// background task owns nothing of the view.
pub(crate) struct UploadJob {
    pub(crate) key: u64,
    pub(crate) uploaded_id: Option<String>,
    pub(crate) uploaded_name: Option<String>,
    pub(crate) filename: String,
    pub(crate) content_type: String,
    /// An `Arc` clone — the bytes themselves are never copied.
    pub(crate) image: Arc<gpui::Image>,
}

/// A raw staged file: `(filename, content type, bytes)`.
pub(crate) type StagedFile = (String, String, Vec<u8>);

/// One upload that landed: `(strip key, attachment id, the server's
/// sanitized filename)`.
pub(crate) type Uploaded = (u64, String, String);

/// Is `content_type` one of the five inline image types (an IMAGE tile with
/// a marker)? Everything else stages as a file.
pub(crate) fn is_inline_image(content_type: &str) -> bool {
    ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type)
}

/// The composer's target gate: with `files_allowed` (the local machine, or
/// a remote one advertising `steer-files`) every entry passes; without it
/// only the inline images do. Returns what stays and whether a FILE was
/// dropped (the caller shows [`FILES_NEED_NEWER_DEVICE`] then). Pure.
pub(crate) fn gate_files_for_target(
    entries: Vec<StagedFile>,
    files_allowed: bool,
) -> (Vec<StagedFile>, bool) {
    if files_allowed {
        return (entries, false);
    }
    let before = entries.len();
    let kept: Vec<StagedFile> = entries
        .into_iter()
        .filter(|(_, content_type, _)| is_inline_image(content_type))
        .collect();
    let dropped = kept.len() < before;
    (kept, dropped)
}

/// The composer's staged images.
#[derive(Default)]
pub(crate) struct PendingImages {
    pending: Vec<PendingImage>,
    next_key: u64,
}

impl PendingImages {
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }

    fn image_count(&self) -> usize {
        self.pending.iter().filter(|item| !item.is_file).count()
    }

    fn file_count(&self) -> usize {
        self.pending.iter().filter(|item| item.is_file).count()
    }

    /// Any non-image FILE staged (the target gate's launch blocker: the
    /// machine may have been switched after the pick).
    pub(crate) fn has_files(&self) -> bool {
        self.file_count() > 0
    }

    /// Add clipboard / picked images AND files to the draft, applying the web
    /// composer's caps (images: inline type + 10 MB + at most
    /// [`MAX_STEER_IMAGES`]; files: any type + 50 MB + at most
    /// [`MAX_STEER_FILES`]; an empty entry is refused), and drop the k-th
    /// marker at `input`'s caret for each IMAGE staged. Returns the notice to
    /// show (`None` = everything staged).
    pub(crate) fn stage(
        &mut self,
        images: Vec<StagedFile>,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<SharedString> {
        let notice = self.stage_entries(images);
        for index in notice.markers {
            insert_marker(input, index, window, cx);
        }
        notice.message
    }

    /// The gpui-free half of [`Self::stage`]: stages what passes and reports
    /// the image numbers whose markers the caller drops, plus the notice.
    fn stage_entries(&mut self, entries: Vec<StagedFile>) -> StageOutcome {
        let mut rejected = false;
        let mut images_full = false;
        let mut files_full = false;
        let mut markers = Vec::new();
        for (filename, content_type, bytes) in entries {
            if bytes.is_empty() || bytes.len() > max_upload_bytes_for(&content_type) {
                rejected = true;
                continue;
            }
            let is_file = !is_inline_image(&content_type);
            if is_file && self.file_count() >= MAX_STEER_FILES {
                files_full = true;
                continue;
            }
            if !is_file && self.image_count() >= MAX_STEER_IMAGES {
                images_full = true;
                continue;
            }
            let preview = pending_preview(&content_type, bytes);
            self.pending.push(PendingImage {
                key: self.next_key,
                filename,
                content_type,
                preview,
                uploaded_id: None,
                uploaded_name: None,
                is_file,
            });
            self.next_key += 1;
            if !is_file {
                markers.push(self.image_count() as u32);
            }
        }
        let message = if images_full {
            Some(SharedString::from(format!(
                "Up to {MAX_STEER_IMAGES} images per message"
            )))
        } else if files_full {
            Some(SharedString::from(FILES_CAP))
        } else if rejected {
            Some(SharedString::from(ATTACH_REJECTED))
        } else {
            None
        };
        StageOutcome { markers, message }
    }

    /// Wave D: the message to send once every upload landed (call after
    /// [`Self::note_uploaded`]) — the prose, the image embeds, then the file
    /// lines with the server's filenames (`steer::build_steer_message`).
    pub(crate) fn message(&self, text: &str) -> String {
        let images: Vec<String> = self
            .pending
            .iter()
            .filter(|item| !item.is_file)
            .filter_map(|item| item.uploaded_id.clone())
            .collect();
        let files: Vec<SteerFile> = self
            .pending
            .iter()
            .filter(|item| item.is_file)
            .filter_map(|item| {
                let id = item.uploaded_id.clone()?;
                let name = item
                    .uploaded_name
                    .clone()
                    .unwrap_or_else(|| item.filename.clone());
                Some(SteerFile::new(id, name))
            })
            .collect();
        build_steer_message(text, &images, &files)
    }

    /// Drop a staged image and renumber the draft's markers behind it: the
    /// removed image's own `[Image #k]` goes and every higher one slides
    /// down, so the markers keep naming the right pictures.
    pub(crate) fn remove(
        &mut self,
        key: u64,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(position) = self.pending.iter().position(|image| image.key == key) else {
            return;
        };
        let removed = self.pending.remove(position);
        if removed.is_file {
            // Files carry no marker — nothing in the draft to renumber.
            return;
        }
        // The removed image's NUMBER counts images only.
        let position = self.pending[..position]
            .iter()
            .filter(|item| !item.is_file)
            .count();
        input.update(cx, |state, cx| {
            let text = state.value().to_string();
            let next = renumber_image_markers(&text, position as u32 + 1);
            if next == text {
                return;
            }
            // `set_value` parks the caret at the start of a multi-line field
            // (upstream `InputState::set_value`), which would throw the
            // writer back to the top of their draft for removing a
            // thumbnail. Carry the caret over, clamped into the shortened
            // text and snapped to a char boundary — the same restore the
            // markdown toolbar's transforms do. It focuses the field, which
            // is where the writer was anyway: they are mid-draft.
            let caret = clamp_to_char_boundary(&next, state.cursor());
            let caret = crate::markdown::byte_offset_to_position(&next, caret);
            state.set_value(next, window, cx);
            state.set_cursor_position(caret, window, cx);
        });
    }

    /// Record the ids that landed — a retry after a mid-batch failure only
    /// uploads the rest (web parity).
    pub(crate) fn note_uploaded(&mut self, resolved: &[Uploaded]) {
        for (key, id, name) in resolved {
            if let Some(image) = self.pending.iter_mut().find(|image| image.key == *key) {
                image.uploaded_id = Some(id.clone());
                image.uploaded_name = Some(name.clone());
            }
        }
    }

    /// The uploads to perform, in strip order.
    pub(crate) fn jobs(&self) -> Vec<UploadJob> {
        self.pending
            .iter()
            .map(|image| UploadJob {
                key: image.key,
                uploaded_id: image.uploaded_id.clone(),
                uploaded_name: image.uploaded_name.clone(),
                filename: image.filename.clone(),
                content_type: image.content_type.clone(),
                image: image.preview.clone(),
            })
            .collect()
    }

    /// The thumbnail strip (EXP-698): 48px tiles with a ✕ on the corner.
    /// `remove` is the host's handler for the ✕ (it should call
    /// [`Self::remove`] and notify).
    pub(crate) fn render_strip<V: Render>(
        &self,
        prefix: &'static str,
        sending: bool,
        remove: fn(&mut V, u64, &mut Window, &mut gpui::Context<V>),
        cx: &mut gpui::Context<V>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut strip = h_flex().w_full().flex_wrap().gap_1p5();
        for image in &self.pending {
            let key = image.key;
            // EXP-698: a 24px hit target (`size::CONTROL_SM`) overlaid on the
            // 48px tile — `xsmall()` alone sized the glyph, not the box, and
            // left a corner ✕ that was hard to actually hit.
            let remove_button = Button::new((prefix, key as usize))
                .ghost()
                .cursor_pointer()
                .with_size(px(theme::tokens::size::CONTROL_SM))
                .rounded_full()
                .icon(registry::UI_CLOSE)
                .tooltip(if image.is_file { "Remove file" } else { "Remove image" })
                .disabled(sending)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    remove(this, key, window, cx);
                }));
            if image.is_file {
                let glyph = crate::issue_files::icon_for_content_type(Some(&image.content_type));
                strip = strip.child(
                    h_flex()
                        .relative()
                        .flex_shrink_0()
                        .h(px(PENDING_THUMB))
                        .w(px(FILE_TILE_W))
                        .pl_2()
                        .pr(px(theme::tokens::size::CONTROL_SM))
                        .gap_1p5()
                        .items_center()
                        .rounded(px(theme::tokens::radius::SM))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
                        .child(
                            gpui_component::Icon::from(glyph)
                                .with_size(px(16.))
                                .text_color(muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .truncate()
                                .child(SharedString::from(image.filename.clone())),
                        )
                        .child(div().absolute().top_0().right_0().child(remove_button)),
                );
                continue;
            }
            let preview = image.preview.clone();
            let filename = SharedString::from(image.filename.clone());
            strip = strip.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .size(px(PENDING_THUMB))
                    .rounded(px(theme::tokens::radius::SM))
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                    .bg(theme::tokens::glass::FILL_CARD.to_hsla())
                    .overflow_hidden()
                    .child(
                        gpui::img(preview)
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover)
                            // Bytes gpui cannot decode fall back to the
                            // filename, so a tile is never a silent blank
                            // square — that is the old chip's whole job.
                            .with_fallback(move || {
                                div()
                                    .size_full()
                                    .p_1()
                                    .text_xs()
                                    .truncate()
                                    .text_color(muted)
                                    .child(filename.clone())
                                    .into_any_element()
                            }),
                    )
                    // The ✕ rides the tile's top-right corner (web/iOS).
                    .child(div().absolute().top_0().right_0().child(remove_button)),
            );
        }
        strip.into_any_element()
    }
}

/// Insert `[Image #index]` at the composer's caret, padded exactly as the
/// shared contract pads it. The component does the actual insert so it owns
/// the caret and the undo entry; the SLICE it inserts is the one
/// [`insert_image_marker`] would have produced.
fn insert_marker(input: &Entity<TextareaState>, index: u32, window: &mut Window, cx: &mut App) {
    let (text, caret) = {
        let state = input.read(cx);
        (state.value().to_string(), state.cursor())
    };
    let (next, after) = insert_image_marker(&text, caret, index);
    let Some(inserted) = next.get(caret..after) else {
        return;
    };
    let inserted = inserted.to_string();
    input.update(cx, |state, cx| state.insert(inserted, window, cx));
}

/// Run `jobs` SEQUENTIALLY and idempotently: an already-uploaded image is
/// skipped, a mid-batch failure returns what landed so the caller can keep
/// the draft and a retry uploads only the rest (web parity). `upload` is the
/// route-specific call (session files or team session files).
pub(crate) fn upload_all(
    jobs: Vec<UploadJob>,
    upload: impl Fn(&str, &str, &[u8]) -> anyhow::Result<image_paste::UploadedImage>,
) -> Result<Vec<Uploaded>, (Vec<Uploaded>, String)> {
    let mut resolved: Vec<Uploaded> = Vec::with_capacity(jobs.len());
    for job in jobs {
        match job.uploaded_id {
            Some(id) => {
                let name = job.uploaded_name.unwrap_or(job.filename);
                resolved.push((job.key, id, name));
            }
            None => {
                let image = upload(&job.filename, &job.content_type, &job.image.bytes)
                    .map_err(|err| (resolved.clone(), err.to_string()))?;
                // The SERVER's sanitized filename is the file line's text.
                let name = image.filename.clone().unwrap_or(job.filename);
                resolved.push((job.key, image.id, name));
            }
        }
    }
    Ok(resolved)
}

/// The images on the clipboard right now — pasted bitmaps and image FILES
/// (a Finder copy). Empty when the clipboard holds none, so the host lets
/// the paste fall through to the text field.
pub(crate) fn clipboard_images(cx: &App) -> Vec<StagedFile> {
    let Some(item) = cx.read_from_clipboard() else {
        return Vec::new();
    };
    let mut images = Vec::new();
    for entry in item.entries() {
        match entry {
            gpui::ClipboardEntry::Image(image) => {
                let (mime, filename) = pasted_image_parts(image.format());
                images.push((filename, mime.to_string(), image.bytes().to_vec()));
            }
            gpui::ClipboardEntry::ExternalPaths(paths) => {
                // Wave D: a Finder copy of ANY file stages too.
                for path in paths.paths() {
                    if let Some(entry) = read_pick(path) {
                        images.push(entry);
                    }
                }
            }
            gpui::ClipboardEntry::String(_) => {}
        }
    }
    images
}

/// The attach button (the Agent composer's "+" → Add file or image, the
/// steer reply's attach): a native file prompt with NO type filter (wave D),
/// the picked files handed back to the host's `apply` on the foreground. An
/// unreadable or oversized pick rides to [`PendingImages::stage`] as a bare
/// entry (type, no bytes) and comes back as its rejection notice instead of
/// vanishing (EXP-1249).
pub(crate) fn pick_attachment_files<V: 'static>(
    window: &mut Window,
    cx: &mut gpui::Context<V>,
    apply: impl FnOnce(&mut V, Vec<StagedFile>, &mut Window, &mut gpui::Context<V>) + 'static,
) {
    let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Attach".into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return;
        };
        let read: Vec<StagedFile> = paths.iter().filter_map(|path| read_pick(path)).collect();
        if read.is_empty() {
            return;
        }
        let _ = this.update_in(cx, |this, window, cx| apply(this, read, window, cx));
    })
    .detach();
}

/// One picked path as a staged entry, read whole under its type's cap
/// (`read_any_file`: images 10 MB, files 50 MB, checked before the read).
/// A directory, an unreadable, empty or oversized file comes back as its
/// name and type with NO bytes, which `stage` rejects with its notice.
fn read_pick(path: &std::path::Path) -> Option<StagedFile> {
    if !path.is_file() {
        return None;
    }
    if let Ok(entry) = image_paste::read_any_file(path) {
        return Some(entry);
    }
    let filename = path.file_name()?.to_string_lossy().into_owned();
    Some((filename, image_paste::content_type_for_path(path).to_string(), Vec::new()))
}

/// EXP-698: wrap staged bytes for `img()`, using the same magic-byte sniff
/// the editor's image slots use. Bytes gpui cannot decode simply paint the
/// element's `with_fallback` (the filename), so this never has to guess right.
fn pending_preview(content_type: &str, bytes: Vec<u8>) -> Arc<gpui::Image> {
    let format = crate::markdown::sniff_format(content_type, &bytes);
    Arc::new(gpui::Image::from_bytes(format, bytes))
}

/// What [`PendingImages::stage_entries`] staged: the image numbers whose
/// markers go at the caret, and the notice.
struct StageOutcome {
    markers: Vec<u32>,
    message: Option<SharedString>,
}

/// Snap a byte offset to the nearest char boundary at or before it.
fn clamp_to_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uploaded(id: &str) -> image_paste::UploadedImage {
        serde_json::from_value(serde_json::json!({ "id": id, "url": format!("/api/attachments/{id}") }))
            .expect("a minimal upload response")
    }

    fn job(key: u64, uploaded: Option<&str>) -> UploadJob {
        UploadJob {
            key,
            uploaded_id: uploaded.map(str::to_string),
            uploaded_name: None,
            filename: format!("{key}.png"),
            content_type: "image/png".into(),
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, vec![key as u8])),
        }
    }

    /// The target gate: a machine without `steer-files` takes images only,
    /// and says so once a file was dropped; a qualifying target (the local
    /// machine always) takes everything untouched.
    #[test]
    fn an_old_target_takes_images_only_and_a_qualifying_one_everything() {
        let entries = || {
            vec![
                ("shot.png".to_string(), "image/png".to_string(), vec![1]),
                ("notes.pdf".to_string(), "application/pdf".to_string(), vec![2]),
            ]
        };
        let (kept, dropped) = gate_files_for_target(entries(), false);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].0, "shot.png");
        assert!(dropped);
        let (kept, dropped) = gate_files_for_target(entries(), true);
        assert_eq!(kept.len(), 2);
        assert!(!dropped);
        // Images alone pass an old target silently.
        let images = vec![("shot.png".to_string(), "image/png".to_string(), vec![1])];
        let (kept, dropped) = gate_files_for_target(images, false);
        assert_eq!(kept.len(), 1);
        assert!(!dropped);
        assert_eq!(
            FILES_NEED_NEWER_DEVICE,
            "Attaching files needs the device on 0.14.66 or newer; images still work"
        );
    }

    /// The upload is sequential and idempotent: landed ids are reused, a
    /// failure returns what landed so far so the retry uploads only the rest.
    #[test]
    fn upload_all_skips_landed_images_and_reports_partial_failures() {
        let calls = std::cell::RefCell::new(Vec::new());
        let ok = upload_all(vec![job(1, Some("att-1")), job(2, None)], |filename, _, bytes| {
            calls.borrow_mut().push(filename.to_string());
            Ok(uploaded(&format!("att-{}", bytes[0])))
        })
        .unwrap();
        assert_eq!(
            ok,
            vec![
                (1, "att-1".to_string(), "1.png".to_string()),
                (2, "att-2".to_string(), "2.png".to_string())
            ]
        );
        assert_eq!(calls.borrow().as_slice(), &["2.png".to_string()]);

        let failed = upload_all(vec![job(3, None), job(4, None)], |filename, _, _| {
            if filename == "4.png" {
                anyhow::bail!("boom");
            }
            Ok(uploaded("att-3"))
        });
        match failed {
            Err((resolved, error)) => {
                assert_eq!(resolved, vec![(3, "att-3".to_string(), "3.png".to_string())]);
                assert_eq!(error, "boom");
            }
            Ok(_) => panic!("expected the failure"),
        }
    }

    /// Wave D: a non-image pick is READ (any type, 50 MB) and stages as a
    /// file; an empty one comes back bare and is refused with the notice.
    #[test]
    fn a_non_image_pick_is_read_and_an_empty_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("wd2-pick-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("spec.pdf");
        std::fs::write(&pdf, b"%PDF-1.7").unwrap();
        let (filename, content_type, bytes) = read_pick(&pdf).expect("a read entry");
        assert_eq!(filename, "spec.pdf");
        assert_eq!(content_type, "application/pdf");
        assert_eq!(bytes, b"%PDF-1.7");
        let empty = dir.join("empty.txt");
        std::fs::write(&empty, b"").unwrap();
        let (_, _, bytes) = read_pick(&empty).expect("a bare entry");
        assert!(bytes.is_empty());
        assert!(read_pick(&dir).is_none(), "a directory is never staged");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn entry(name: &str, content_type: &str, len: usize) -> StagedFile {
        (name.to_string(), content_type.to_string(), vec![1u8; len])
    }

    /// Wave D: images and files cap apart (4 + 4), markers count images
    /// only, and the ONE rejection copy covers type, size and emptiness.
    #[test]
    fn staging_caps_images_and_files_apart_and_numbers_images_only() {
        let mut strip = PendingImages::default();
        let outcome = strip.stage_entries(vec![
            entry("a.png", "image/png", 3),
            entry("notes.pdf", "application/pdf", 3),
            entry("b.jpg", "image/jpeg", 3),
        ]);
        assert_eq!(outcome.markers, vec![1, 2]);
        assert_eq!(outcome.message, None);
        let outcome = strip.stage_entries(vec![
            entry("c.zip", "application/zip", 3),
            entry("d.txt", "text/plain", 3),
            entry("e.svg", "image/svg+xml", 3),
            entry("f.md", "text/markdown", 3),
        ]);
        assert!(outcome.markers.is_empty(), "files carry no marker");
        assert_eq!(outcome.message.as_deref(), Some(FILES_CAP));
        assert_eq!(strip.file_count(), 4);
        let outcome = strip.stage_entries(vec![entry(
            "big.png",
            "image/png",
            image_paste::MAX_IMAGE_UPLOAD_BYTES + 1,
        )]);
        assert_eq!(outcome.message.as_deref(), Some(ATTACH_REJECTED));
        assert_eq!(
            ATTACH_REJECTED,
            "Images up to 10 MB and files up to 50 MB can be attached"
        );
        let outcome = strip.stage_entries(vec![
            entry("c.png", "image/png", 3),
            entry("d.png", "image/png", 3),
            entry("e.png", "image/png", 3),
        ]);
        assert_eq!(outcome.markers, vec![3, 4]);
        assert_eq!(outcome.message.as_deref(), Some("Up to 4 images per message"));
    }

    /// Wave D: the sent message = prose, image embeds, then the file lines
    /// with the SERVER's filename (the upload response's), in strip order.
    #[test]
    fn the_message_carries_images_then_files_with_the_server_names() {
        let mut strip = PendingImages::default();
        strip.stage_entries(vec![
            entry("notes.pdf", "application/pdf", 3),
            entry("a.png", "image/png", 3),
        ]);
        let keys: Vec<u64> = strip.pending.iter().map(|item| item.key).collect();
        strip.note_uploaded(&[
            (keys[0], "33333333-3333-4333-8333-333333333333".into(), "notes (1).pdf".into()),
            (keys[1], "11111111-1111-4111-8111-111111111111".into(), "a.png".into()),
        ]);
        assert_eq!(
            strip.message("look [Image #1]"),
            "look [Image #1]\n\n![image](/api/attachments/11111111-1111-4111-8111-111111111111)\n[notes (1).pdf](/api/attachments/33333333-3333-4333-8333-333333333333)"
        );
    }

    /// A retry reuses the landed id AND its server name.
    #[test]
    fn upload_all_reuses_the_landed_name() {
        let mut landed = job(7, Some("att-7"));
        landed.uploaded_name = Some("server.png".into());
        let ok = upload_all(vec![landed], |_, _, _| anyhow::bail!("never called")).unwrap();
        assert_eq!(ok, vec![(7, "att-7".to_string(), "server.png".to_string())]);
    }

    #[test]
    fn clamp_snaps_to_a_char_boundary() {
        let text = "aé b";
        assert_eq!(clamp_to_char_boundary(text, 2), 1);
        assert_eq!(clamp_to_char_boundary(text, 99), text.len());
        assert_eq!(clamp_to_char_boundary(text, 0), 0);
    }
}
