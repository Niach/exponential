//! The single image path of the markdown editor (masterplan-v3
//! §4.5 "Images"): clipboard paste, drag-drop and the toolbar file picker all
//! funnel through one staging + upload seam.
//!
//! Mirrors the web semantics exactly:
//! - **Detail editor** (issue exists): upload immediately via
//!   `POST /api/issues/{issueId}/files` (multipart `file` field —
//!   `apps/web/src/routes/api/issues/$issueId/files.ts`), then insert the
//!   canonical **relative** form `![alt](/api/attachments/{id})`.
//! - **Create dialog** (no issue yet): stage bytes under a `draft://<id>`
//!   placeholder (web keeps `blob:` object URLs the same way), create the
//!   issue, then [`upload_staged_images`] + [`rewrite_image_urls`] and update
//!   the description. Upload is atomic/all-or-nothing per the contract.
//!
//! Rendering reads attachment bytes through the same transport (the
//! `/api/attachments/{id}` route is auth-gated — a bearer header, never a
//! bare `img src`).
//!
//! #EDE-65 lane L: the HTTP transport, the caps and the file reads live in
//! `api::attachment_upload` (gpui-free, shared with eDE's shell); this module
//! re-exports them and keeps the editor's `draft://` staging.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub use api::attachment_upload::{
    build_multipart, content_type_for_path, is_inline_image_path, max_upload_bytes_for,
    read_any_file, read_image_file, validate_image, AttachmentFetchStatus, AttachmentTransport,
    HttpAttachmentTransport, UploadedImage, ACCEPTED_IMAGE_CONTENT_TYPES, MAX_FILE_UPLOAD_BYTES,
    MAX_IMAGE_UPLOAD_BYTES,
};

/// Scheme of staged (not-yet-uploaded) image URLs.
pub const DRAFT_SCHEME: &str = "draft://";

static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);

/// A process-unique `draft://` placeholder URL for a staged image.
pub fn new_draft_url() -> String {
    let n = NEXT_DRAFT.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{DRAFT_SCHEME}{t:x}-{n:x}")
}

/// An image picked/pasted locally but not yet uploaded, keyed by its
/// `draft://` URL (mirrors iOS `PendingImage` / web `DraftImage`).
#[derive(Clone)]
pub struct StagedImage {
    pub draft_url: String,
    pub filename: String,
    pub content_type: String,
    pub bytes: Arc<Vec<u8>>,
}

/// Upload every staged image and return `draft://` → canonical relative URL.
/// **All-or-nothing** (the interchange contract's atomic-upload rule): the
/// first failure aborts and the caller keeps the drafts staged.
pub fn upload_staged_images(
    transport: &dyn AttachmentTransport,
    issue_id: &str,
    staged: &[StagedImage],
) -> anyhow::Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for image in staged {
        let uploaded = transport.upload(
            issue_id,
            &image.filename,
            &image.content_type,
            &image.bytes,
        )?;
        resolved.insert(image.draft_url.clone(), uploaded.url);
    }
    Ok(resolved)
}

/// Rewrite image destinations (`![alt](draft://…)` → `![alt](/api/attachments/{id})`)
/// after upload. Only URL occurrences inside `](…)` are touched.
pub fn rewrite_image_urls(markdown: &str, resolved: &HashMap<String, String>) -> String {
    let mut out = markdown.to_string();
    for (draft, real) in resolved {
        out = out.replace(&format!("]({draft})"), &format!("]({real})"));
    }
    out
}

/// Filename + mime for a pasted clipboard image (gpui `ClipboardEntry::Image`).
pub fn pasted_image_parts(format: gpui::ImageFormat) -> (&'static str, String) {
    let mime = match format {
        gpui::ImageFormat::Png => "image/png",
        gpui::ImageFormat::Jpeg => "image/jpeg",
        gpui::ImageFormat::Webp => "image/webp",
        gpui::ImageFormat::Gif => "image/gif",
        _ => "image/png",
    };
    let ext = mime.rsplit('/').next().unwrap_or("png");
    (mime, format!("pasted-image.{ext}"))
}

/// The one save-bytes derivation of the description surfaces (EXP-261): a
/// `draft://` URL must never reach the server, so every persist site (blur
/// save, structural-commit save, the detail view's tab-switch flush, the
/// create-dialog submit) derives its bytes through here. Draft-free markdown
/// passes through byte-identical.
pub fn markdown_for_save(markdown: String) -> String {
    if markdown.contains(DRAFT_SCHEME) {
        strip_draft_images(&markdown)
    } else {
        markdown
    }
}

/// Remove every `![alt](draft://…)` occurrence whose src is a
/// still-unresolved staging URL — a draft URL must never reach the server.
///
/// The strip is STRUCTURAL (EXP-261): exactly the draft occurrences are
/// removed via the same occurrence scan `delete_image` uses, wherever they
/// sit — standalone paragraphs, but also inline in list items, blockquotes,
/// headings and table cells (the vendored editor serializes those inline, so
/// the old whole-line filter missed them, and a line holding a real image
/// AND a draft lost both). Every other byte is preserved apart from a final
/// trim; a line the removal leaves empty (the standalone image paragraph a
/// plain-paragraph paste produces) is dropped along with the doubled blank
/// line it leaves behind. Deliberately NO comrak `canonicalize` pass: this
/// runs on vendored-WYSIWYG output, which legitimately round-trips
/// constructs (tables, soft breaks) the block editor's canonicalize would
/// destroy (see wysiwyg_parity.rs).
pub fn strip_draft_images(markdown: &str) -> String {
    let mut out = markdown.to_string();
    loop {
        let occurrence = crate::attachments_row::extract_image_occurrences(&out)
            .into_iter()
            .find(|occurrence| occurrence.url.starts_with(DRAFT_SCHEME));
        let Some(occurrence) = occurrence else {
            break;
        };
        remove_draft_occurrence(&mut out, occurrence.start, occurrence.end);
    }
    out.trim().to_string()
}

/// Remove one draft occurrence's byte range. When the containing line is
/// left whitespace-only (the standalone-paragraph case), drop the line too,
/// plus the doubled blank line that leaves between its neighbors; inline
/// residue (list item, table cell, …) keeps its line untouched.
fn remove_draft_occurrence(text: &mut String, start: usize, end: usize) {
    text.replace_range(start..end, "");
    let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(text.len());
    if !text[line_start..line_end].trim().is_empty() {
        return;
    }
    if line_end < text.len() {
        text.replace_range(line_start..line_end + 1, "");
        // "a\n\n<img>\n\nb" would otherwise become "a\n\n\nb".
        if line_start >= 2
            && text.as_bytes().get(line_start) == Some(&b'\n')
            && text.as_bytes()[line_start - 1] == b'\n'
            && text.as_bytes()[line_start - 2] == b'\n'
        {
            text.remove(line_start);
        }
    } else if line_start > 0 {
        text.replace_range(line_start - 1..line_end, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_replaces_only_link_destinations() {
        let mut map = HashMap::new();
        map.insert("draft://1".to_string(), "/api/attachments/xyz".to_string());
        let md = "text draft://1 and ![a](draft://1) end";
        assert_eq!(
            rewrite_image_urls(md, &map),
            "text draft://1 and ![a](/api/attachments/xyz) end"
        );
    }

    #[test]
    fn draft_urls_are_unique() {
        assert_ne!(new_draft_url(), new_draft_url());
    }

    // EXP-261 regression: the strip is structural — inline drafts inside
    // list items, blockquotes and table cells (which the vendored editor
    // serializes inline, never as standalone `![…` lines) are removed too,
    // and the surrounding construct survives.
    #[test]
    fn strip_removes_inline_drafts_in_list_blockquote_and_table() {
        assert_eq!(
            strip_draft_images("- first\n- ![img](draft://a) item\n- last"),
            "- first\n-  item\n- last"
        );
        assert_eq!(
            strip_draft_images("> quoted ![img](draft://b) text"),
            "> quoted  text"
        );
        assert_eq!(
            strip_draft_images("| a | ![img](draft://c) |\n| --- | --- |\n| 1 | 2 |"),
            "| a |  |\n| --- | --- |\n| 1 | 2 |"
        );
    }

    // EXP-261 regression: a line holding a real image AND a draft keeps the
    // real one (the old whole-line filter dropped both).
    #[test]
    fn strip_keeps_real_image_sharing_a_line_with_a_draft() {
        assert_eq!(
            strip_draft_images("![keep](/api/attachments/xyz) ![lose](draft://d)"),
            "![keep](/api/attachments/xyz)"
        );
        assert_eq!(
            strip_draft_images("![lose](draft://d) ![keep](/api/attachments/xyz)"),
            "![keep](/api/attachments/xyz)"
        );
    }

    // EXP-261 regression: NO comrak canonicalize — a table elsewhere in the
    // document survives a draft-racing save byte-identically (the old strip
    // flattened it into one space-joined paragraph).
    #[test]
    fn strip_leaves_tables_and_soft_breaks_byte_identical() {
        let table = "| col a | col b |\n| --- | --- |\n| 1 | 2 |";
        let with_draft = format!("intro\n\n![shot](draft://e)\n\n{table}");
        assert_eq!(strip_draft_images(&with_draft), format!("intro\n\n{table}"));
        // Draft-free input passes through untouched (soft breaks included).
        let soft = "line one\nline two\n\n| a |\n| --- |\n| 1 |";
        assert_eq!(strip_draft_images(soft), soft);
        assert_eq!(markdown_for_save(soft.to_string()), soft);
    }

    #[test]
    fn strip_removes_standalone_draft_paragraphs_without_blank_residue() {
        assert_eq!(
            strip_draft_images("Intro\n\n![shot](draft://f)\n\nOutro"),
            "Intro\n\nOutro"
        );
        assert_eq!(strip_draft_images("![only](draft://g)"), "");
        assert_eq!(strip_draft_images("![a](draft://h)\n\ntail"), "tail");
        assert_eq!(strip_draft_images("head\n\n![a](draft://i)"), "head");
        assert_eq!(
            strip_draft_images("![a](draft://j)\n\n![b](draft://k)"),
            ""
        );
        assert_eq!(strip_draft_images(""), "");
    }

    #[test]
    fn markdown_for_save_strips_only_when_drafts_present() {
        assert_eq!(
            markdown_for_save("a\n\n![x](draft://m)\n\nb".to_string()),
            "a\n\nb"
        );
        // Untouched — not even a trim — when no draft is present.
        let clean = "text with trailing newline\n".to_string();
        assert_eq!(markdown_for_save(clean.clone()), clean);
    }
}
