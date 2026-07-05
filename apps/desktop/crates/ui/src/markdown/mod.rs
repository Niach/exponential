//! The GFM markdown editor (masterplan-v3 §4.5/§4.6 — the Phase-3 sub-gate
//! surface) — the desktop's fourth implementation of the cross-client GFM
//! interchange contract (CLAUDE.md): plain-text GFM that round-trips
//! byte-for-byte across web (TipTap + tiptap-markdown), iOS/macOS
//! (cmark-gfm) and Android (block editor).
//!
//! Layout:
//! - [`blocks`] — the `[ContentBlock]` document model (single source of
//!   truth; markdown derived only at save — iOS `IssueEditorModel` semantics).
//! - [`parse`] / [`serialize`] — GFM ⇄ blocks, byte-parity locked by the
//!   fixture suite in `serialize.rs` (ported from Android's
//!   `MarkdownRoundTripTest.kt`).
//! - [`editor`] — the editable surface ([`MarkdownEditor`], block-based:
//!   text blocks are gpui-component `InputState`s holding GFM source, image
//!   blocks render inline) + the read-only rendered view ([`MarkdownView`])
//!   with clickable `#IDENT` / `@email` pills and toggleable task checkboxes.
//! - [`toolbar`] — the static toolbar (**no** selection popover).
//! - [`autocomplete`] — the caret-anchored `@`-member / `#`-issue completion
//!   overlay (§4.6).
//! - [`image_paste`] — the single image path: clipboard paste /
//!   file picker → staged `draft://` blocks → atomic upload via
//!   `/api/issues/{id}/images` → canonical `![alt](/api/attachments/{id})`.
//!
//! Screens B/C consume the seam: build a [`MarkdownEditor`] entity, feed it
//! [`MarkdownEditor::set_markdown`], read [`MarkdownEditor::markdown`] on
//! save, and wire [`MarkdownEditor::set_on_change`] for dirty tracking. For
//! pre-save flows (create dialog), stage images and resolve them at submit
//! with [`image_paste::upload_staged_images`] +
//! [`image_paste::rewrite_image_urls`].

pub mod autocomplete;
pub mod blocks;
mod editor;
pub mod image_paste;
pub mod parse;
pub mod serialize;
mod toolbar;

pub use autocomplete::{
    detect_trigger, store_completion_source, CompletionItem, CompletionSource, CompletionTrigger,
    PendingToken,
};
pub use blocks::{ContentBlock, RichText};
pub(crate) use editor::byte_offset_to_position;
pub use editor::{ImageCache, MarkdownEditor, MarkdownView, RefResolver};
pub use image_paste::{
    AttachmentTransport, HttpAttachmentTransport, StagedImage, UploadedImage,
};
pub use parse::markdown_to_blocks;
pub use serialize::blocks_to_markdown;

/// Normalize arbitrary GFM to the canonical cross-client form
/// (`serialize(parse(md))`). Canonical input is untouched (the fixture
/// suite guarantees it); non-canonical input converges after one pass.
pub fn canonicalize(markdown: &str) -> String {
    blocks_to_markdown(&markdown_to_blocks(markdown))
}
