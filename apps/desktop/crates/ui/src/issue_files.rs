//! The EXP-297 files rail's pure helpers: attachment classification, the
//! synced-collection read behind the issue-detail "Files" section, and the
//! small formatting / path derivations its rows need.
//!
//! **Classification (the cross-client contract).** A row is an *inline image*
//! iff its `content_type` is exactly one of
//! [`ACCEPTED_IMAGE_CONTENT_TYPES`] — those live in the description markdown
//! as `![alt](/api/attachments/{id})` and are rendered by the editor, never
//! listed here. EVERYTHING else (pdf/zip/video/audio/plain text, and also
//! non-inline `image/*` types like tiff or svg) belongs to the Files section.
//! Web, iOS and Android apply the identical rule.
//!
//! Nothing here talks to the network: uploads/downloads go through the
//! [`crate::markdown::AttachmentTransport`] and deletion through
//! `api::attachments::attachments_delete`.

use gpui::App;
use sync::Store;

use domain::rows::Attachment;

use crate::icons::ExpIcon;
use crate::markdown::image_paste::ACCEPTED_IMAGE_CONTENT_TYPES;

/// Is this content type one of the five inline (markdown-embedded) image
/// types? Absent/blank types are never inline — the server stores
/// `application/octet-stream` for those.
pub(crate) fn is_inline_image(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| ACCEPTED_IMAGE_CONTENT_TYPES.contains(&value))
}

/// The issue's FILE attachments (non-inline-image rows) from the synced
/// `attachments` shape, oldest first — the exact list the Files section
/// renders. Reads the collection the same way
/// `markdown::attachment_natural_size` does, so an Electric delta re-renders
/// the section through the detail view's existing collection observer.
///
/// EXP-554: rows carrying a `comment_id` belong to ONE comment and render in
/// that comment's strip — the rail lists issue-level files only (web/iOS/
/// Android filter identically).
pub(crate) fn file_attachments(issue_id: &str, cx: &App) -> Vec<Attachment> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut rows: Vec<Attachment> = store
        .collections()
        .attachments
        .read(cx)
        .iter()
        .filter(|attachment| attachment.issue_id.as_deref() == Some(issue_id))
        .filter(|attachment| attachment.comment_id.is_none())
        .filter(|attachment| !is_inline_image(attachment.content_type.as_deref()))
        .cloned()
        .collect();
    // `created_at` is an ISO-8601 UTC string — lexicographic order is
    // chronological. Ties (and missing timestamps) fall back to the id so the
    // list never reshuffles between frames.
    rows.sort_by(|a, b| {
        a.created_at
            .as_deref()
            .unwrap_or_default()
            .cmp(b.created_at.as_deref().unwrap_or_default())
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// EVERY synced attachment id of the issue — inline images included. This is
/// the pending-row dedupe set: deduping against only the Files rows would
/// leave a pending row spinning forever if its upload landed as an
/// inline-image row (e.g. an MCP upload finishing concurrently), because
/// `file_attachments` filters those out.
pub(crate) fn all_attachment_ids(issue_id: &str, cx: &App) -> std::collections::HashSet<String> {
    let Some(store) = Store::try_global(cx) else {
        return Default::default();
    };
    store
        .collections()
        .attachments
        .read(cx)
        .iter()
        .filter(|attachment| attachment.issue_id.as_deref() == Some(issue_id))
        .map(|attachment| attachment.id.clone())
        .collect()
}

/// Display label of one row: the stored filename, else a generic fallback.
pub(crate) fn attachment_label(attachment: &Attachment) -> String {
    attachment
        .filename
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("file")
        .to_string()
}

/// Human-readable byte size (`0 B`, `812.0 KB`, `1.5 MB`). Mirrors the
/// settings pane's `format_size`; a missing/negative size reads `0 B`.
pub(crate) fn format_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes <= 0 {
        return "0 B".to_string();
    }
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// A per-type-family glyph for the row's leading icon. Everything comes from
/// the ONE shared icon registry ([`ExpIcon`]) — no per-platform glyph map.
pub(crate) fn icon_for_content_type(content_type: Option<&str>) -> ExpIcon {
    let content_type = content_type.unwrap_or_default();
    let (family, subtype) = content_type
        .split_once('/')
        .unwrap_or((content_type, ""));
    match (family, subtype) {
        ("image", _) => ExpIcon::Image,
        ("video", _) | ("audio", _) => ExpIcon::Play,
        ("text", _) => ExpIcon::FileText,
        ("application", "pdf") => ExpIcon::FileText,
        ("application", "json" | "xml" | "yaml" | "rtf") => ExpIcon::FileText,
        (
            "application",
            "zip" | "gzip" | "x-tar" | "x-7z-compressed" | "vnd.rar" | "x-bzip2",
        ) => ExpIcon::Package,
        _ => ExpIcon::File,
    }
}

/// Filesystem-safe display name: no path separators, no control characters,
/// no `..` traversal, clamped to a sane length. A name that sanitizes away
/// entirely falls back to `file`.
pub(crate) fn sanitize_filename(filename: &str) -> String {
    let cleaned: String = filename
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        return "file".to_string();
    }
    // Clamp on a CHAR boundary (byte truncation would split a multi-byte
    // glyph and panic).
    let clamped: String = cleaned.chars().take(120).collect();
    let clamped = clamped.trim().to_string();
    if clamped.is_empty() {
        "file".to_string()
    } else {
        clamped
    }
}

/// Where an "Open" click materializes the fetched bytes before handing the
/// path to the OS: `<temp>/exponential-attachments/<attachment id>/<name>`.
///
/// Per-attachment subdirectory so two attachments with the same filename
/// never collide, and so a re-open just overwrites its own copy.
pub(crate) fn temp_open_path(attachment_id: &str, filename: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("exponential-attachments")
        .join(sanitize_filename(attachment_id))
        .join(sanitize_filename(filename))
}

/// Fetch one attachment's bytes through the auth-gated transport into its
/// [`temp_open_path`] and hand back the path for `cx.open_with_system`.
/// BLOCKING (network + disk) — background executor only. Shared by the Files
/// rail's "Open" and the EXP-554 comment attachment chips so both materialize
/// bytes the same way.
pub(crate) fn fetch_attachment_to_temp(
    transport: &dyn crate::markdown::AttachmentTransport,
    attachment_id: &str,
    label: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let path = temp_open_path(attachment_id, label);
    let bytes = transport.fetch(&format!("/api/attachments/{attachment_id}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_matches_the_five_inline_types() {
        for inline in ACCEPTED_IMAGE_CONTENT_TYPES {
            assert!(is_inline_image(Some(inline)), "{inline}");
        }
        // Non-inline image types are FILES (no invisible gap) — as is every
        // other type, and an absent/blank one.
        for file in [
            "image/tiff",
            "image/svg+xml",
            "image/heic",
            "application/pdf",
            "application/zip",
            "video/mp4",
            "text/plain",
            "application/octet-stream",
            "",
        ] {
            assert!(!is_inline_image(Some(file)), "{file}");
        }
        assert!(!is_inline_image(None));
        // Case/parameter variants are NOT the exact contract values.
        assert!(!is_inline_image(Some("IMAGE/PNG")));
        assert!(!is_inline_image(Some("image/png; charset=binary")));
    }

    #[test]
    fn format_bytes_scales_and_floors_at_zero() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(-5), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(50 * 1024 * 1024), "50.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn icons_group_by_type_family() {
        // `ExpIcon` is macro-generated (Clone + IntoElement only), so the
        // assertions compare its asset path.
        use gpui_component::IconNamed as _;
        let path = |content_type: Option<&str>| icon_for_content_type(content_type).path();
        assert_eq!(path(Some("image/tiff")), ExpIcon::Image.path());
        assert_eq!(path(Some("video/mp4")), ExpIcon::Play.path());
        assert_eq!(path(Some("audio/mpeg")), ExpIcon::Play.path());
        assert_eq!(path(Some("text/csv")), ExpIcon::FileText.path());
        assert_eq!(path(Some("application/pdf")), ExpIcon::FileText.path());
        assert_eq!(path(Some("application/zip")), ExpIcon::Package.path());
        assert_eq!(path(Some("application/octet-stream")), ExpIcon::File.path());
        assert_eq!(path(None), ExpIcon::File.path());
    }

    #[test]
    fn sanitize_filename_strips_traversal_and_separators() {
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
        // Separators become `_` and the leading dots go with the trim, so
        // nothing can traverse out of the per-attachment directory (and no
        // dotfile is ever written).
        assert_eq!(sanitize_filename("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_filename("  spaced.txt  "), "spaced.txt");
        assert_eq!(sanitize_filename(""), "file");
        assert_eq!(sanitize_filename("..."), "file");
        assert_eq!(sanitize_filename("with\nnewline.txt"), "with_newline.txt");
        // Multi-byte names clamp on a char boundary, never mid-glyph.
        let long = "é".repeat(400);
        assert_eq!(sanitize_filename(&long).chars().count(), 120);
    }

    #[test]
    fn temp_open_path_is_per_attachment_and_sanitized() {
        let path = temp_open_path("att-1", "../evil.sh");
        assert!(path.starts_with(std::env::temp_dir().join("exponential-attachments")));
        assert_eq!(
            path.parent().and_then(|p| p.file_name()),
            Some(std::ffi::OsStr::new("att-1"))
        );
        assert_eq!(path.file_name(), Some(std::ffi::OsStr::new("_evil.sh")));
    }

    #[test]
    fn labels_fall_back_when_the_filename_is_missing() {
        let mut attachment = Attachment {
            id: "att-1".into(),
            team_id: None,
            issue_id: None,
            comment_id: None,
            uploader_id: None,
            filename: Some("  notes.txt ".into()),
            content_type: None,
            size_bytes: None,
            storage_key: None,
            url: None,
            width: None,
            height: None,
            created_at: None,
            updated_at: None,
        };
        assert_eq!(attachment_label(&attachment), "notes.txt");
        attachment.filename = Some("   ".into());
        assert_eq!(attachment_label(&attachment), "file");
        attachment.filename = None;
        assert_eq!(attachment_label(&attachment), "file");
    }
}
