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

use std::sync::Arc;

use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, StyledImage as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::TextareaState;
use gpui_component::{h_flex, ActiveTheme as _, Disableable as _, Sizable as _};

use steer::{insert_image_marker, renumber_image_markers, MAX_STEER_IMAGES};

use crate::icons::registry;
use crate::markdown::image_paste::{
    self, max_upload_bytes_for, pasted_image_parts, read_image_file, validate_image,
};

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
}

/// One upload the sender performs, snapshotted off the strip so the
/// background task owns nothing of the view.
pub(crate) struct UploadJob {
    pub(crate) key: u64,
    pub(crate) uploaded_id: Option<String>,
    pub(crate) filename: String,
    pub(crate) content_type: String,
    /// An `Arc` clone — the bytes themselves are never copied.
    pub(crate) image: Arc<gpui::Image>,
}

/// A raw staged file: `(filename, content type, bytes)`.
pub(crate) type StagedFile = (String, String, Vec<u8>);

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

    /// Add clipboard / picked images to the draft, applying the same caps as
    /// the web composer (type + 10 MB + at most [`MAX_STEER_IMAGES`]), and
    /// drop the k-th marker at `input`'s caret for each one staged. Returns
    /// the notice to show (`None` = everything staged).
    pub(crate) fn stage(
        &mut self,
        images: Vec<StagedFile>,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<SharedString> {
        let mut rejected = false;
        let mut overflow = false;
        for (filename, content_type, bytes) in images {
            if validate_image(&content_type, bytes.len()).is_err()
                || bytes.len() > max_upload_bytes_for(&content_type)
            {
                rejected = true;
                continue;
            }
            if self.pending.len() >= MAX_STEER_IMAGES {
                overflow = true;
                continue;
            }
            let preview = pending_preview(&content_type, bytes);
            self.pending.push(PendingImage {
                key: self.next_key,
                filename,
                content_type,
                preview,
                uploaded_id: None,
            });
            self.next_key += 1;
            insert_marker(input, self.pending.len() as u32, window, cx);
        }
        if overflow {
            Some(SharedString::from(format!(
                "Up to {MAX_STEER_IMAGES} images per message"
            )))
        } else if rejected {
            Some(SharedString::from("Only images up to 10 MB can be attached"))
        } else {
            None
        }
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
        self.pending.remove(position);
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
    pub(crate) fn note_uploaded(&mut self, resolved: &[(u64, String)]) {
        for (key, id) in resolved {
            if let Some(image) = self.pending.iter_mut().find(|image| image.key == *key) {
                image.uploaded_id = Some(id.clone());
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
                .tooltip("Remove image")
                .disabled(sending)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    remove(this, key, window, cx);
                }));
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
) -> Result<Vec<(u64, String)>, (Vec<(u64, String)>, String)> {
    let mut resolved: Vec<(u64, String)> = Vec::with_capacity(jobs.len());
    for job in jobs {
        match job.uploaded_id {
            Some(id) => resolved.push((job.key, id)),
            None => {
                let image = upload(&job.filename, &job.content_type, &job.image.bytes)
                    .map_err(|err| (resolved.clone(), err.to_string()))?;
                resolved.push((job.key, image.id));
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
                for path in paths.paths() {
                    if let Ok((filename, mime, bytes)) = read_image_file(path) {
                        images.push((filename, mime, bytes));
                    }
                }
            }
            gpui::ClipboardEntry::String(_) => {}
        }
    }
    images
}

/// The attach button: a native file prompt (images only), the picked files
/// handed back to the host's `apply` on the foreground.
pub(crate) fn pick_image_files<V: 'static>(
    window: &mut Window,
    cx: &mut gpui::Context<V>,
    apply: impl FnOnce(&mut V, Vec<StagedFile>, &mut Window, &mut gpui::Context<V>) + 'static,
) {
    let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Attach".into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return;
        };
        let read: Vec<StagedFile> = paths
            .into_iter()
            .filter(|path| image_paste::is_inline_image_path(path))
            .filter_map(|path| read_image_file(&path).ok())
            .collect();
        if read.is_empty() {
            return;
        }
        let _ = this.update_in(cx, |this, window, cx| apply(this, read, window, cx));
    })
    .detach();
}

/// EXP-698: wrap staged bytes for `img()`, using the same magic-byte sniff
/// the editor's image slots use. Bytes gpui cannot decode simply paint the
/// element's `with_fallback` (the filename), so this never has to guess right.
fn pending_preview(content_type: &str, bytes: Vec<u8>) -> Arc<gpui::Image> {
    let format = crate::markdown::sniff_format(content_type, &bytes);
    Arc::new(gpui::Image::from_bytes(format, bytes))
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
            filename: format!("{key}.png"),
            content_type: "image/png".into(),
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, vec![key as u8])),
        }
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
        assert_eq!(ok, vec![(1, "att-1".to_string()), (2, "att-2".to_string())]);
        assert_eq!(calls.borrow().as_slice(), &["2.png".to_string()]);

        let failed = upload_all(vec![job(3, None), job(4, None)], |filename, _, _| {
            if filename == "4.png" {
                anyhow::bail!("boom");
            }
            Ok(uploaded("att-3"))
        });
        match failed {
            Err((resolved, error)) => {
                assert_eq!(resolved, vec![(3, "att-3".to_string())]);
                assert_eq!(error, "boom");
            }
            Ok(_) => panic!("expected the failure"),
        }
    }

    #[test]
    fn clamp_snaps_to_a_char_boundary() {
        let text = "aé b";
        assert_eq!(clamp_to_char_boundary(text, 2), 1);
        assert_eq!(clamp_to_char_boundary(text, 99), text.len());
        assert_eq!(clamp_to_char_boundary(text, 0), 0);
    }
}
