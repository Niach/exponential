//! EXP-825 — the ONE pending-images strip of a message composer: staged
//! bytes, `[Image #N]` markers, the thumbnail strip, sequential idempotent
//! uploads. Two composers use it: the steer viewer's reply composer (uploads
//! to the SESSION route, EXP-702) and Home's start composer (uploads to the
//! TEAM route BEFORE a session exists — the start binds them through
//! `sessions.send`'s `attachmentIds`, EXP-1294). The strip and its rules
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
//! path and ride the message's file block (`domain::image_message::build_steer_message`).
//!
//! #EXP-1841: the strip's rules (caps, numbering, marker insertion, the X's
//! renumbering, the toasts) are `domain::composer_attachments`'s, the ONE
//! implementation replaying `composer-attachments.json`; this module only
//! classifies picks and applies the result to a field. EVERY composer
//! stages through it: the launch composers (Home, an agent's first message)
//! on their own `TextareaState` ([`PendingImages::stage`]/[`PendingImages::remove`]),
//! a session chat's document composer on its drawn field's text and caret
//! ([`PendingImages::stage_text`]/[`PendingImages::drop_text`]).

use std::sync::Arc;

use gpui::{
    div, px, AnyElement, App, ClickEvent, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, StyledImage as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::TextareaState;
use gpui_component::{h_flex, ActiveTheme as _, Disableable as _, Sizable as _};

use domain::composer_attachments::{
    clamp_to_char_boundary, drop_attachment, stage_attachments, AttachmentKind, PickKind,
};
use domain::composer_send::{after_send, ComposerContent, SendOutcome};

use crate::icons::registry;
use crate::markdown::image_paste::{self, max_upload_bytes_for, pasted_image_parts};

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

/// One upload the sender performs (`api::attachment_upload::UploadJob`),
/// its bytes the thumbnail's own buffer.
pub(crate) type UploadJob = api::attachment_upload::UploadJob<PreviewBytes>;

/// The staged bytes as the upload reads them: an `Arc` clone of the
/// thumbnail's `gpui::Image`, so the bytes themselves are never copied.
#[derive(Clone)]
pub(crate) struct PreviewBytes(pub(crate) Arc<gpui::Image>);

impl AsRef<[u8]> for PreviewBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0.bytes
    }
}

/// A raw staged file: `(filename, content type, bytes)`.
pub(crate) type StagedFile = (String, String, Vec<u8>);

// #EDE-65 lane L: the upload loop and the sent message are gpui-free in
// `api::attachment_upload`, shared with eDE's shell.
pub(crate) use api::attachment_upload::{
    file_keys, is_inline_image, upload_all, uploaded_message, UploadSendError, Uploaded,
};

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

    /// #EDE-65 lane O: the staged items in strip order (the chat document's
    /// `/chatFiles` chips).
    pub(crate) fn items(&self) -> &[PendingImage] {
        &self.pending
    }

    #[cfg(test)]
    fn file_count(&self) -> usize {
        self.pending.iter().filter(|item| item.is_file).count()
    }

    /// The strip as `domain::composer_attachments` reads it.
    fn kinds(&self) -> Vec<AttachmentKind> {
        self.pending
            .iter()
            .map(|item| if item.is_file { AttachmentKind::File } else { AttachmentKind::Image })
            .collect()
    }

    /// Add clipboard / picked images AND files to the draft (#EXP-1841:
    /// `domain::composer_attachments::stage_attachments`: images = inline
    /// type + 10 MB, files = any type + 50 MB, four of each, an empty entry
    /// refused) and drop one `[Image #N]` per staged IMAGE at `input`'s
    /// caret. Returns the notice to show (`None` = everything staged).
    pub(crate) fn stage(
        &mut self,
        images: Vec<StagedFile>,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<SharedString> {
        let (text, caret) = {
            let state = input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let outcome = self.stage_text(images, &text, caret);
        if let Some((_, inserted)) = outcome.inserted {
            // The field does the insert so it owns the caret and the undo
            // entry; the slice is the one the contract inserted.
            input.update(cx, |state, cx| state.insert(inserted, window, cx));
        }
        outcome.notice
    }

    /// The field-free half of [`Self::stage`]: stages what passes against
    /// the draft `text` with its caret, and returns what to insert where
    /// plus the notice. Every composer's attach goes through here.
    pub(crate) fn stage_text(&mut self, entries: Vec<StagedFile>, text: &str, caret: usize) -> StageOutcome {
        let picks: Vec<PickKind> = entries
            .iter()
            .map(|(_, content_type, bytes)| {
                if bytes.is_empty() || bytes.len() > max_upload_bytes_for(content_type) {
                    PickKind::Rejected
                } else if is_inline_image(content_type) {
                    PickKind::Image
                } else {
                    PickKind::File
                }
            })
            .collect();
        let at = clamp_to_char_boundary(text, caret);
        let staged = stage_attachments(&self.kinds(), &picks, text, at);
        for ((filename, content_type, bytes), (pick, taken)) in entries.into_iter().zip(picks.iter().zip(&staged.taken)) {
            if !taken {
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
                is_file: *pick == PickKind::File,
            });
            self.next_key += 1;
        }
        // Markers only ever go in at the caret, so the draft after = the
        // draft before with one slice at `at`.
        let inserted = (staged.caret > at)
            .then(|| staged.text.get(at..staged.caret).map(|slice| (at, slice.to_string())))
            .flatten();
        let toasts = staged.toasts();
        let notice = (!toasts.is_empty()).then(|| SharedString::from(toasts.join("\n")));
        StageOutcome { inserted, text: staged.text, notice }
    }

    /// Wave D: the message to send once every upload landed (call after
    /// [`Self::note_uploaded`]) — the prose, the image embeds, then the file
    /// lines with the server's filenames (`domain::image_message::build_steer_message`).
    pub(crate) fn message(&self, text: &str) -> String {
        let resolved: Vec<Uploaded> = self
            .pending
            .iter()
            .filter_map(|item| {
                let id = item.uploaded_id.clone()?;
                let name = item
                    .uploaded_name
                    .clone()
                    .unwrap_or_else(|| item.filename.clone());
                Some((item.key, id, name))
            })
            .collect();
        let file_keys: Vec<u64> = self
            .pending
            .iter()
            .filter(|item| item.is_file)
            .map(|item| item.key)
            .collect();
        uploaded_message(text, &file_keys, &resolved)
    }

    /// EXP-1537: the composer's content as `domain::composer_send` reads it:
    /// `text` (untrimmed) + the strip's keys, in strip order.
    pub(crate) fn content(&self, text: String) -> ComposerContent {
        ComposerContent {
            text,
            attachments: self.pending.iter().map(|item| item.key.to_string()).collect(),
        }
    }

    /// Keep only the staged entries whose key is in `keep`.
    pub(crate) fn retain_keys(&mut self, keep: &[String]) {
        self.pending.retain(|item| keep.contains(&item.key.to_string()));
    }

    /// Drop a staged entry and renumber the draft's markers behind it: the
    /// removed image's own `[Image #k]` goes and every higher one slides
    /// down, so the markers keep naming the right pictures.
    pub(crate) fn remove(
        &mut self,
        key: u64,
        input: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (text, caret) = {
            let state = input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let Some((next, caret)) = self.drop_text(key, &text, caret) else {
            return;
        };
        input.update(cx, |state, cx| {
            // `set_value` parks the caret at the start of a multi-line field
            // (upstream `InputState::set_value`), which would throw the
            // writer back to the top of their draft for removing a
            // thumbnail. The caret stays where it was, moved left by what
            // the removal took out before it (composer-attachments.json
            // `drop.caret`). It focuses the field, which is where the
            // writer was anyway: they are mid-draft.
            let caret = crate::markdown::byte_offset_to_position(&next, caret);
            state.set_value(next, window, cx);
            state.set_cursor_position(caret, window, cx);
        });
    }

    /// The field-free half of [`Self::remove`]
    /// (`domain::composer_attachments::drop_attachment`): drops entry `key`
    /// and returns the renumbered draft with the caret (bytes) carried
    /// along, `None` when the text stays.
    pub(crate) fn drop_text(&mut self, key: u64, text: &str, caret: usize) -> Option<(String, usize)> {
        let at = self.pending.iter().position(|item| item.key == key)?;
        let (_, next, caret) = drop_attachment(&self.kinds(), at, text, caret);
        self.pending.remove(at);
        (next != text).then_some((next, caret))
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
                bytes: PreviewBytes(image.preview.clone()),
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

/// EXP-1537: a send landed: the field clears only if it still holds
/// exactly `sent.text`, and only the attachments that went out leave the
/// strip (`domain::composer_send::after_send`). The field is only written
/// when it changes, so the caret of a writer still typing stays put.
pub(crate) fn settle_sent(
    input: &Entity<TextareaState>,
    images: &mut PendingImages,
    sent: &ComposerContent,
    window: &mut Window,
    cx: &mut App,
) {
    let current = images.content(input.read(cx).value().to_string());
    let next = after_send(SendOutcome::Sent, sent, &current);
    images.retain_keys(&next.attachments);
    if next.text != current.text {
        input.update(cx, |state, cx| state.set_value(next.text, window, cx));
    }
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

/// #EXP-1841: files dropped on a composer (drag-and-drop), read like a
/// pick; they stage through the same [`PendingImages`] path.
pub(crate) fn dropped_files(paths: &gpui::ExternalPaths) -> Vec<StagedFile> {
    paths.paths().iter().filter_map(|path| read_pick(path)).collect()
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
pub(crate) fn read_pick(path: &std::path::Path) -> Option<StagedFile> {
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

/// What [`PendingImages::stage_text`] staged: the slice to insert and the
/// byte offset it goes at (`None` = no image taken), the whole draft after,
/// and the notice (the toasts, in order).
pub(crate) struct StageOutcome {
    pub(crate) inserted: Option<(usize, String)>,
    pub(crate) text: String,
    pub(crate) notice: Option<SharedString>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::composer_attachments::{ATTACHMENT_REJECTED_TOAST as ATTACH_REJECTED, FILE_CAP_TOAST as FILES_CAP};

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

    /// EXP-1537: the view-free message equals the strip's own, and a landed
    /// send takes out only the entries that went out.
    #[test]
    fn a_landed_send_keeps_what_was_added_meanwhile() {
        let mut strip = PendingImages::default();
        strip.stage_text(vec![entry("a.png", "image/png", 4), entry("spec.pdf", "application/pdf", 4)], "", 0);
        let jobs = strip.jobs();
        let resolved: Vec<Uploaded> = jobs
            .iter()
            .map(|job| (job.key, format!("att-{}", job.key), job.filename.clone()))
            .collect();
        let file_keys = file_keys(&jobs);
        strip.note_uploaded(&resolved);
        assert_eq!(uploaded_message("look", &file_keys, &resolved), strip.message("look"));

        let sent = strip.content("look".into());
        strip.stage_text(vec![entry("b.png", "image/png", 4)], "", 0);
        let current = strip.content("look again".into());
        let next = domain::composer_send::after_send(SendOutcome::Sent, &sent, &current);
        strip.retain_keys(&next.attachments);
        assert_eq!(next.text, "look again");
        assert_eq!(strip.content(String::new()).attachments, vec!["2".to_string()]);
    }

    /// Wave D / #EXP-1841: images and files cap apart (4 + 4), markers
    /// count images only and land at the caret, and the ONE rejection copy
    /// covers type, size and emptiness.
    #[test]
    fn staging_caps_images_and_files_apart_and_numbers_images_only() {
        let mut strip = PendingImages::default();
        let outcome = strip.stage_text(
            vec![
                entry("a.png", "image/png", 3),
                entry("notes.pdf", "application/pdf", 3),
                entry("b.jpg", "image/jpeg", 3),
            ],
            "please check",
            0,
        );
        assert_eq!(outcome.inserted, Some((0, "[Image #1] [Image #2] ".to_string())));
        assert_eq!(outcome.text, "[Image #1] [Image #2] please check");
        assert_eq!(outcome.notice, None);
        let outcome = strip.stage_text(
            vec![
                entry("c.zip", "application/zip", 3),
                entry("d.txt", "text/plain", 3),
                entry("e.svg", "image/svg+xml", 3),
                entry("f.md", "text/markdown", 3),
            ],
            "x",
            1,
        );
        assert_eq!(outcome.inserted, None, "files carry no marker");
        assert_eq!(outcome.notice.as_deref(), Some(FILES_CAP));
        assert_eq!(strip.file_count(), 4);
        let outcome = strip.stage_text(
            vec![entry("big.png", "image/png", image_paste::MAX_IMAGE_UPLOAD_BYTES + 1)],
            "",
            0,
        );
        assert_eq!(outcome.notice.as_deref(), Some(ATTACH_REJECTED));
        assert_eq!(ATTACH_REJECTED, "Images up to 10 MB and files up to 50 MB can be attached");
        let outcome = strip.stage_text(
            vec![
                entry("c.png", "image/png", 3),
                entry("d.png", "image/png", 3),
                entry("e.png", "image/png", 3),
            ],
            "look",
            4,
        );
        assert_eq!(outcome.text, "look [Image #3] [Image #4]");
        assert_eq!(outcome.notice.as_deref(), Some("Up to 4 images per message"));
    }

    /// #EXP-1841: the X drops its image's marker and renumbers the rest; a
    /// file's X leaves the draft alone.
    #[test]
    fn removing_an_image_renumbers_and_a_file_leaves_the_text() {
        let mut strip = PendingImages::default();
        let outcome = strip.stage_text(
            vec![entry("a.png", "image/png", 3), entry("spec.pdf", "application/pdf", 3), entry("b.png", "image/png", 3)],
            "",
            0,
        );
        assert_eq!(outcome.text, "[Image #1] [Image #2]");
        let keys: Vec<u64> = strip.pending.iter().map(|item| item.key).collect();
        assert_eq!(strip.drop_text(keys[1], &outcome.text, 21), None);
        assert_eq!(strip.drop_text(keys[0], &outcome.text, 10), Some(("[Image #1]".to_string(), 0)));
        assert_eq!(strip.len(), 1);
    }

    /// Wave D: the sent message = prose, image embeds, then the file lines
    /// with the SERVER's filename (the upload response's), in strip order.
    #[test]
    fn the_message_carries_images_then_files_with_the_server_names() {
        let mut strip = PendingImages::default();
        strip.stage_text(
            vec![entry("notes.pdf", "application/pdf", 3), entry("a.png", "image/png", 3)],
            "",
            0,
        );
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
}
