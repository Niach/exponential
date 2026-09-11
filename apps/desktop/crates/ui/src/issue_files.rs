//! The EXP-297 files rail's pure helpers: attachment classification, the
//! synced-collection read behind the issue-detail "Files" section, and the
//! small formatting / path derivations its rows need.
//!
//! **Classification (the cross-client contract).** A row is an *inline image*
//! iff its `content_type` is exactly one of
//! [`ACCEPTED_IMAGE_CONTENT_TYPES`] — those live in the description markdown
//! as `![alt](/api/attachments/{id})` and are rendered by the editor, never
//! listed here. EXP-824: a row is *inline media* iff its `content_type`
//! starts with `video/` or `audio/` — those live in the markdown as a plain
//! link on its own paragraph (`[clip.mp4](/api/attachments/{id})`) and render
//! as a media tile, never listed here either. EVERYTHING else (pdf/zip/plain
//! text, and also non-inline `image/*` types like tiff or svg) belongs to the
//! Files section. Web, iOS and Android apply the identical rules.
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

/// EXP-824: is this a video row (`video/*`)? Inline media is a PREFIX match,
/// unlike the exact five-type image contract — the server accepts any
/// `video/*` / `audio/*` upload and probes what it can.
pub(crate) fn is_inline_video(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| value.starts_with("video/"))
}

/// EXP-824: is this an audio row (`audio/*`)?
pub(crate) fn is_inline_audio(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| value.starts_with("audio/"))
}

/// EXP-824: video or audio — the rows that embed as a media tile.
pub(crate) fn is_inline_media(content_type: Option<&str>) -> bool {
    is_inline_video(content_type) || is_inline_audio(content_type)
}

/// EXP-824: a media duration chip — `0:07`, `2:34`, `1:02:03` (hours only
/// once there are any; seconds floor, never round up past the real length).
/// Mirrors the web/iOS/Android formatter byte for byte.
pub(crate) fn format_duration(duration_ms: i64) -> String {
    // Nearest second, like web `formatDuration` and iOS `MediaDuration`.
    let total_seconds = (duration_ms.max(0) + 500) / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// How a just-uploaded file joins the DESCRIPTION (EXP-316 images, EXP-824
/// media) — `None` means it stays a Files row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DescriptionEmbed {
    /// `![filename](url)`
    Image,
    /// `[filename](url)` on its own paragraph — the inline-media form.
    Media,
}

pub(crate) fn description_embed(content_type: &str) -> Option<DescriptionEmbed> {
    if is_inline_image(Some(content_type)) {
        Some(DescriptionEmbed::Image)
    } else if is_inline_media(Some(content_type)) {
        Some(DescriptionEmbed::Media)
    } else {
        None
    }
}

/// The markdown paragraph appended for an embed (the upload's canonical
/// relative `url`; the filename is the alt / link label).
pub(crate) fn description_fragment(
    embed: DescriptionEmbed,
    filename: Option<&str>,
    url: &str,
) -> String {
    let name = filename.map(str::trim).filter(|name| !name.is_empty());
    match embed {
        DescriptionEmbed::Image => format!("![{}]({url})", name.unwrap_or("image")),
        DescriptionEmbed::Media => format!("[{}]({url})", name.unwrap_or("file")),
    }
}

/// The issue's FILE attachments (non-inline-image, non-media rows) from the synced
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
        .filter(|attachment| !is_inline_media(attachment.content_type.as_deref()))
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

// ---------------------------------------------------------------------------
// EXP-824: the media cache behind "open in the system player"
// ---------------------------------------------------------------------------

/// A cached clip older than this is pruned on the next cache write.
pub(crate) const MEDIA_CACHE_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);
/// The cache's total size budget; past it the OLDEST clips go first.
pub(crate) const MEDIA_CACHE_MAX_BYTES: u64 = 500 * 1024 * 1024;

/// `{data_dir}/media-cache` — beside the steer journal, so member-only clip
/// bytes live in the app's own data directory (pruned by
/// [`prune_media_cache`]) rather than piling up in the OS temp dir forever.
pub(crate) fn media_cache_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("media-cache")
}

/// `{data_dir}/media-cache/<attachment id>.<ext>`: ONE file per attachment
/// (a re-open reuses it), keyed by the id so a renamed clip never leaves a
/// stale twin behind. The extension is the label's, lowercased and clamped,
/// so the system player picks the right decoder; a label without one gets
/// none.
pub(crate) fn media_cache_path(
    data_dir: &std::path::Path,
    attachment_id: &str,
    filename: &str,
) -> std::path::PathBuf {
    let ext: String = std::path::Path::new(&sanitize_filename(filename))
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(|ext| ext.chars().take(16).collect())
        .unwrap_or_default();
    let stem = sanitize_filename(attachment_id);
    media_cache_dir(data_dir).join(if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    })
}

/// One cache file as [`media_cache_evictions`] sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MediaCacheEntry {
    pub path: std::path::PathBuf,
    pub modified: std::time::SystemTime,
    pub size: u64,
}

/// The prune rule, pure: every entry older than `max_age` goes, then, while
/// the survivors' total exceeds `max_bytes`, the OLDEST goes first. `keep`
/// (the file the caller just wrote) is never a candidate, so a clip larger
/// than the whole budget still opens once. Entries dated in the future count
/// as fresh (a clock skew is not "old").
pub(crate) fn media_cache_evictions(
    entries: &[MediaCacheEntry],
    now: std::time::SystemTime,
    max_age: std::time::Duration,
    max_bytes: u64,
    keep: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let mut evict: Vec<std::path::PathBuf> = Vec::new();
    let mut survivors: Vec<&MediaCacheEntry> = Vec::new();
    for entry in entries {
        if keep.is_some_and(|keep| keep == entry.path) {
            continue;
        }
        let age = now
            .duration_since(entry.modified)
            .unwrap_or(std::time::Duration::ZERO);
        if age > max_age {
            evict.push(entry.path.clone());
        } else {
            survivors.push(entry);
        }
    }
    let kept_bytes: u64 = entries
        .iter()
        .filter(|entry| keep.is_some_and(|keep| keep == entry.path))
        .map(|entry| entry.size)
        .sum();
    let mut total: u64 = kept_bytes + survivors.iter().map(|entry| entry.size).sum::<u64>();
    survivors.sort_by_key(|entry| entry.modified);
    for entry in survivors {
        if total <= max_bytes {
            break;
        }
        total = total.saturating_sub(entry.size);
        evict.push(entry.path.clone());
    }
    evict
}

/// Apply [`media_cache_evictions`] to the cache directory. Best-effort:
/// an unreadable directory prunes nothing; returns the number removed.
pub(crate) fn prune_media_cache(
    data_dir: &std::path::Path,
    keep: Option<&std::path::Path>,
) -> usize {
    let Ok(read) = std::fs::read_dir(media_cache_dir(data_dir)) else {
        return 0;
    };
    let entries: Vec<MediaCacheEntry> = read
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some(MediaCacheEntry {
                path: entry.path(),
                modified: meta.modified().ok()?,
                size: meta.len(),
            })
        })
        .collect();
    let mut removed = 0;
    for path in media_cache_evictions(
        &entries,
        std::time::SystemTime::now(),
        MEDIA_CACHE_MAX_AGE,
        MEDIA_CACHE_MAX_BYTES,
        keep,
    ) {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("[ui] media cache: pruned {removed} clip(s)");
    }
    removed
}

/// Materialize one media attachment for the system player: the cached copy
/// under [`media_cache_path`] when one exists (its mtime bumped so the prune
/// treats it as fresh), else a fetch through the auth-gated transport
/// written via a `.part` sibling + rename (a killed download never masquerades
/// as a complete clip), followed by a prune that spares this file. BLOCKING
/// (network + disk) — background executor only.
pub(crate) fn fetch_media_to_cache(
    transport: &dyn crate::markdown::AttachmentTransport,
    data_dir: &std::path::Path,
    attachment_id: &str,
    label: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let path = media_cache_path(data_dir, attachment_id, label);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() > 0) {
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = file.set_modified(std::time::SystemTime::now());
        }
        return Ok(path);
    }
    let bytes = transport.fetch(&format!("/api/attachments/{attachment_id}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = path.with_extension(match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => format!("{ext}.part"),
        None => "part".to_string(),
    });
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &path)?;
    prune_media_cache(data_dir, Some(&path));
    Ok(path)
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

    /// EXP-824: media is a PREFIX match on the family, and never overlaps
    /// the image contract or the Files rail.
    #[test]
    fn media_classification_is_a_family_prefix_match() {
        for video in ["video/mp4", "video/quicktime", "video/webm", "video/x-matroska"] {
            assert!(is_inline_video(Some(video)), "{video}");
            assert!(is_inline_media(Some(video)), "{video}");
            assert!(!is_inline_audio(Some(video)), "{video}");
            assert!(!is_inline_image(Some(video)), "{video}");
        }
        for audio in ["audio/mpeg", "audio/wav", "audio/mp4", "audio/ogg"] {
            assert!(is_inline_audio(Some(audio)), "{audio}");
            assert!(is_inline_media(Some(audio)), "{audio}");
            assert!(!is_inline_video(Some(audio)), "{audio}");
        }
        for other in [
            "image/png",
            "application/pdf",
            "application/octet-stream",
            "text/plain",
            "videos/mp4",
            "",
        ] {
            assert!(!is_inline_media(Some(other)), "{other}");
        }
        assert!(!is_inline_media(None));
        // Case variants are not the contract values.
        assert!(!is_inline_video(Some("VIDEO/MP4")));
    }

    /// EXP-824: an upload's destination — image and media embed, the rest
    /// stays a Files row — and the exact paragraph each embed appends.
    #[test]
    fn uploads_route_to_the_description_by_type() {
        assert_eq!(description_embed("image/png"), Some(DescriptionEmbed::Image));
        assert_eq!(description_embed("video/mp4"), Some(DescriptionEmbed::Media));
        assert_eq!(description_embed("audio/mpeg"), Some(DescriptionEmbed::Media));
        assert_eq!(description_embed("image/tiff"), None);
        assert_eq!(description_embed("application/pdf"), None);
        assert_eq!(
            description_fragment(DescriptionEmbed::Image, Some("shot.png"), "/api/attachments/a"),
            "![shot.png](/api/attachments/a)"
        );
        assert_eq!(
            description_fragment(DescriptionEmbed::Media, Some("clip.mp4"), "/api/attachments/a"),
            "[clip.mp4](/api/attachments/a)"
        );
        assert_eq!(
            description_fragment(DescriptionEmbed::Media, Some("  "), "/api/attachments/a"),
            "[file](/api/attachments/a)"
        );
        assert_eq!(
            description_fragment(DescriptionEmbed::Image, None, "/api/attachments/a"),
            "![image](/api/attachments/a)"
        );
    }

    #[test]
    fn duration_chips_format_like_the_other_clients() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(7_000), "0:07");
        assert_eq!(format_duration(7_499), "0:07");
        assert_eq!(format_duration(7_500), "0:08");
        assert_eq!(format_duration(154_000), "2:34");
        assert_eq!(format_duration(3_723_000), "1:02:03");
        assert_eq!(format_duration(36_000_000), "10:00:00");
        // Garbage never panics or goes negative.
        assert_eq!(format_duration(-5_000), "0:00");
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

    /// EXP-824: one cache file per attachment id, the label only lends its
    /// (sanitized, lowercased) extension.
    #[test]
    fn media_cache_path_is_keyed_by_id_with_the_labels_extension() {
        let root = std::path::Path::new("/data");
        assert_eq!(
            media_cache_path(root, "att-1", "Clip.MP4"),
            root.join("media-cache").join("att-1.mp4")
        );
        assert_eq!(
            media_cache_path(root, "att-1", "../evil/x.m4a"),
            root.join("media-cache").join("att-1.m4a")
        );
        assert_eq!(
            media_cache_path(root, "../att", "noext"),
            root.join("media-cache").join("_att")
        );
        // A junk extension (spaces, non-alphanumerics) is dropped, never
        // written into the file name.
        assert_eq!(
            media_cache_path(root, "att-1", "clip.mp 4"),
            root.join("media-cache").join("att-1")
        );
        let (a, b) = (
            media_cache_path(root, "att-1", "one.mp4"),
            media_cache_path(root, "att-1", "two.mp4"),
        );
        assert_eq!(a, b, "a renamed clip reuses its cache file");
    }

    /// EXP-824: age first, then size oldest-first, and the just-written file
    /// is never a candidate.
    #[test]
    fn media_cache_evictions_by_age_then_size_sparing_the_kept_file() {
        use std::time::{Duration, SystemTime};
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let day = Duration::from_secs(24 * 60 * 60);
        let entry = |name: &str, age_days: u64, size: u64| MediaCacheEntry {
            path: std::path::PathBuf::from(format!("/cache/{name}")),
            modified: now - day * age_days as u32,
            size,
        };
        let entries = vec![
            entry("stale.mp4", 9, 10),
            entry("old.mp4", 6, 40),
            entry("mid.mp4", 3, 40),
            entry("new.mp4", 1, 40),
            entry("just-written.mp4", 0, 100),
        ];
        let keep = std::path::PathBuf::from("/cache/just-written.mp4");
        // Age alone: only the 9-day-old file is stale under a roomy budget.
        assert_eq!(
            media_cache_evictions(&entries, now, day * 7, 1_000, Some(&keep)),
            vec![std::path::PathBuf::from("/cache/stale.mp4")]
        );
        // Budget of 160 with 100 kept: 40 + 40 + 40 survivors → shed the
        // oldest until ≤ 160 (old, then mid).
        assert_eq!(
            media_cache_evictions(&entries, now, day * 7, 160, Some(&keep)),
            vec![
                std::path::PathBuf::from("/cache/stale.mp4"),
                std::path::PathBuf::from("/cache/old.mp4"),
                std::path::PathBuf::from("/cache/mid.mp4"),
            ]
        );
        // The kept file alone over budget still survives; everything else
        // goes.
        let evicted = media_cache_evictions(&entries, now, day * 7, 50, Some(&keep));
        assert_eq!(evicted.len(), 4);
        assert!(!evicted.contains(&keep));
        // No kept file: the newest is the last to go.
        let evicted = media_cache_evictions(&entries, now, day * 7, 100, None);
        assert_eq!(
            evicted.last(),
            Some(&std::path::PathBuf::from("/cache/new.mp4"))
        );
        // A future mtime is fresh, not stale.
        let future = vec![MediaCacheEntry {
            path: std::path::PathBuf::from("/cache/future.mp4"),
            modified: now + day,
            size: 1,
        }];
        assert!(media_cache_evictions(&future, now, day * 7, 1_000, None).is_empty());
    }

    /// EXP-824: a second open reuses the cached clip without a fetch, and a
    /// `.part` never survives a completed write.
    #[test]
    fn fetch_media_to_cache_reuses_the_cached_clip() {
        struct CountingTransport(std::sync::Mutex<usize>);
        impl crate::markdown::AttachmentTransport for CountingTransport {
            fn upload(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: &[u8],
            ) -> anyhow::Result<crate::markdown::UploadedImage> {
                unreachable!()
            }
            fn upload_session(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: &[u8],
            ) -> anyhow::Result<crate::markdown::UploadedImage> {
                unreachable!()
            }
            fn upload_team_session_file(
                &self,
                _: &str,
                _: &str,
                _: &str,
                _: &[u8],
            ) -> anyhow::Result<crate::markdown::UploadedImage> {
                unreachable!()
            }
            fn fetch(&self, _: &str) -> anyhow::Result<Vec<u8>> {
                *self.0.lock().unwrap() += 1;
                Ok(b"clip-bytes".to_vec())
            }
        }
        let dir = std::env::temp_dir().join(format!(
            "exp-media-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let transport = CountingTransport(std::sync::Mutex::new(0));
        let first = fetch_media_to_cache(&transport, &dir, "att-1", "clip.mp4").unwrap();
        assert_eq!(first, media_cache_path(&dir, "att-1", "clip.mp4"));
        assert_eq!(std::fs::read(&first).unwrap(), b"clip-bytes");
        assert!(!first.with_extension("mp4.part").exists());
        let second = fetch_media_to_cache(&transport, &dir, "att-1", "clip.mp4").unwrap();
        assert_eq!(second, first);
        assert_eq!(*transport.0.lock().unwrap(), 1, "the cached clip is reused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_fall_back_when_the_filename_is_missing() {
        let mut attachment = Attachment {
            id: "att-1".into(),
            team_id: None,
            issue_id: None,
            board_id: None,
            comment_id: None,
            uploader_id: None,
            filename: Some("  notes.txt ".into()),
            content_type: None,
            size_bytes: None,
            storage_key: None,
            url: None,
            width: None,
            height: None,
            duration_ms: None,
            poster_storage_key: None,
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
