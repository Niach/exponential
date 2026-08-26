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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as _};
use serde::Deserialize;

/// Accepted upload content types (mirror of web
/// `acceptedImageContentTypes`, `issue-attachments.ts`).
pub const ACCEPTED_IMAGE_CONTENT_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/avif",
];

/// Mirror of web `maxImageUploadBytes` (10 MB).
pub const MAX_IMAGE_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// Mirror of web `maxFileUploadBytes` (50 MB) — the EXP-297 cap for every
/// NON-inline-image attachment. Inline images keep [`MAX_IMAGE_UPLOAD_BYTES`].
pub const MAX_FILE_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

/// Scheme of staged (not-yet-uploaded) image URLs.
pub const DRAFT_SCHEME: &str = "draft://";

/// Total-request budget of an attachment UPLOAD (EXP-297): generous enough
/// for a 50 MB upload on a slow uplink, still bounded so a hung server
/// eventually fails. Fetches keep the shorter [`ATTACHMENT_TIMEOUT`].
const UPLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

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

/// Response of the issue upload routes (`/files`, legacy `/images`).
#[derive(Debug, Clone, Deserialize)]
pub struct UploadedImage {
    pub id: String,
    /// The canonical relative form `/api/attachments/{id}`.
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default, rename = "contentType")]
    pub content_type: Option<String>,
    #[serde(default, rename = "sizeBytes")]
    pub size_bytes: Option<i64>,
    /// Probed dimensions so clients can pre-size (may be null).
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
}

/// Reject unsupported/oversized images with the web's reasons.
pub fn validate_image(content_type: &str, len: usize) -> Result<(), String> {
    if !ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type) {
        return Err("Unsupported image type".into());
    }
    if len > MAX_IMAGE_UPLOAD_BYTES {
        return Err("Images must be 10 MB or smaller".into());
    }
    Ok(())
}

/// The HTTP seam the editor talks through — upload for paste/drop/picker,
/// fetch for rendering `/api/attachments/{id}` bytes (auth-gated). Object
/// safety keeps the editor testable without a server.
pub trait AttachmentTransport: Send + Sync {
    /// Upload one image to an issue (atomic on the server; multipart `file`).
    fn upload(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// EXP-297: upload one ARBITRARY file to an issue —
    /// `POST /api/issues/{id}/files`, same single-part multipart contract and
    /// same response shape as [`Self::upload`] (`width`/`height` are null for
    /// non-images). [`Self::upload`] posts here too since EXP-613 (the
    /// legacy `/images` route stays server-side for old builds); the two
    /// methods survive for their distinct size caps and error labels.
    fn upload_file(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// GET attachment bytes. `url` may be the canonical relative form or
    /// absolute; relative resolves against the instance base URL.
    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>>;
}

/// [`AttachmentTransport`] over the app's one shared HTTP client, authenticated
/// with the account's call-time bearer (same §5.7 rule as `api::TrpcClient` — a
/// re-login is picked up by the very next request).
pub struct HttpAttachmentTransport {
    base_url: String,
    token: Arc<dyn api::TokenProvider>,
    client: reqwest::blocking::Client,
}

/// Attachments can be megabytes on a slow link, so they get a longer budget
/// than the shared client's 30s default.
const ATTACHMENT_TIMEOUT: Duration = Duration::from_secs(60);

impl HttpAttachmentTransport {
    pub fn new(instance_url: &str, token: Arc<dyn api::TokenProvider>) -> Self {
        Self {
            base_url: instance_url.trim_end_matches('/').to_string(),
            token,
            client: api::http::shared().clone(),
        }
    }

    fn absolute(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}{}", self.base_url, url)
        }
    }

    fn authorize(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        // EXP-104: the client-version header rides the attachment upload/fetch
        // requests too, so a stale build is 426-gated everywhere.
        let request = request.timeout(ATTACHMENT_TIMEOUT).header(
            domain::client_version::CLIENT_VERSION_HEADER,
            domain::client_version::client_version_header_value(),
        );
        match self.token.token() {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        }
    }

    /// One multipart POST to an issue upload route (`/images` or `/files` —
    /// identical request and response shapes, so both go through here).
    fn post_multipart(
        &self,
        url: &str,
        what: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let boundary = format!("----ExpMarkdownEditor{}", new_draft_url().len() as u64 + rand_ish());
        let body = build_multipart(&boundary, filename, content_type, bytes);
        let response = self
            .authorize(self.client.post(url))
            // EXP-297: the total-request budget has to cover a 50 MB file
            // upload (the shared 60 s attachment cap aborted those on any
            // ordinary uplink — 50 MB in 60 s needs ~7 Mbit/s sustained), so
            // uploads override the timeout `authorize` set.
            .timeout(UPLOAD_REQUEST_TIMEOUT)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("Accept", "application/json")
            .body(body)
            .send()
            .map_err(|e| anyhow!("{what} upload failed: {e}"))?;
        // reqwest returns Ok for non-2xx, so the status check is explicit.
        let status = response.status();
        let text = response.text().context("upload response")?;
        if !status.is_success() {
            return Err(anyhow!(
                "{what} upload failed: {}",
                upload_error_message(status, &text)
            ));
        }
        serde_json::from_str(&text).with_context(|| format!("decode upload response: {text}"))
    }
}

/// The server's own message beats a bare "HTTP 400" summary. The upload
/// routes answer `{"error": "<string>"}` (errorToResponse in
/// apps/web/src/lib/http-errors.ts — "Files must be 50 MB or smaller", the 412
/// storage-limit text, …); the nested `/error/message` and `/message` forms
/// are kept as fallbacks for tRPC-envelope-shaped callers.
fn upload_error_message(status: reqwest::StatusCode, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error")
                .filter(|error| error.is_string())
                .or_else(|| value.pointer("/error/message"))
                .or_else(|| value.pointer("/message"))
                .and_then(|message| message.as_str())
                .map(|message| message.to_string())
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"))
}

impl AttachmentTransport for HttpAttachmentTransport {
    fn upload(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        // EXP-613: inline images ride the general /files route too — the
        // legacy image-only /images route stays server-side for old builds.
        let url = format!("{}/api/issues/{issue_id}/files", self.base_url);
        self.post_multipart(&url, "image", filename, content_type, bytes)
    }

    fn upload_file(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/issues/{issue_id}/files", self.base_url);
        self.post_multipart(&url, "file", filename, content_type, bytes)
    }

    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let absolute = self.absolute(url);
        let response = self
            .authorize(self.client.get(&absolute))
            .send()
            .map_err(|e| anyhow!("attachment fetch failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("attachment fetch failed: HTTP {status}"));
        }
        Ok(response.bytes().context("attachment body")?.to_vec())
    }
}

fn rand_ish() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

/// Encode one `file` part exactly as a browser `FormData` would — the web
/// route reads `formData.get("file")`.
pub fn build_multipart(
    boundary: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename.replace('"', "_")
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
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

/// Read an image file (drag-drop / file picker), inferring the mime from the
/// extension. Returns `(filename, mime, bytes)`.
pub fn read_image_file(path: &std::path::Path) -> anyhow::Result<(String, String, Vec<u8>)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        other => return Err(anyhow!("unsupported image extension: .{other}")),
    };
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    Ok((filename, mime.to_string(), bytes))
}

/// The content type [`read_any_file`] WILL send for `path`, derived from the
/// extension alone (no read). EXP-554 uses it to classify a just-picked
/// comment attachment as image-or-file before the bytes are ever loaded.
pub(crate) fn content_type_for_path(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    content_type_for_extension(&ext)
}

/// Extension → content type for the EXP-297 files rail. Deliberately a small
/// hand-kept table (no mime-guess dependency): the server stores whatever we
/// send and only the five inline-image types get special treatment, so an
/// unknown extension is perfectly serviceable as `application/octet-stream`.
fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        // Images (the five inline types plus the common non-inline ones).
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "ico" => "image/vnd.microsoft.icon",
        // Documents.
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "rtf" => "application/rtf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Archives.
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "tar" => "application/x-tar",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        // Audio / video (plain files — EXP-297 ships NO player anywhere).
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "application/octet-stream",
    }
}

/// Does this path's extension map to one of the five inline (markdown-
/// embedded) image types? The toolbar's attach picker uses this to route a
/// pick: inline images embed at the caret like the image button, everything
/// else goes to the host's Files flow (EXP-335).
pub fn is_inline_image_path(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type_for_extension(&ext))
}

/// Read ANY picked file for the files rail (EXP-297), inferring the content
/// type from the extension. Returns `(filename, content_type, bytes)`.
///
/// Enforces the client-side size cap the server also enforces: inline images
/// keep [`MAX_IMAGE_UPLOAD_BYTES`] (10 MB), everything else
/// [`MAX_FILE_UPLOAD_BYTES`] (50 MB). The error strings mirror the server's
/// wording so the same copy shows whichever side rejects first.
pub fn read_any_file(path: &std::path::Path) -> anyhow::Result<(String, String, Vec<u8>)> {
    let content_type = content_type_for_path(path);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    // Cheap pre-read guard: refuse an oversized file before pulling 50+ MB
    // into memory (the read below still re-checks, since metadata can lie).
    let declared_len = std::fs::metadata(path)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0);
    let max_bytes = max_upload_bytes_for(content_type);
    if declared_len > max_bytes {
        return Err(anyhow!("{}", size_error_message(content_type)));
    }

    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if bytes.is_empty() {
        return Err(anyhow!("File is empty"));
    }
    if bytes.len() > max_bytes {
        return Err(anyhow!("{}", size_error_message(content_type)));
    }
    Ok((filename, content_type.to_string(), bytes))
}

/// The cap that applies to one content type (server
/// `getMaxUploadBytesForContentType`).
pub fn max_upload_bytes_for(content_type: &str) -> usize {
    if ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type) {
        MAX_IMAGE_UPLOAD_BYTES
    } else {
        MAX_FILE_UPLOAD_BYTES
    }
}

fn size_error_message(content_type: &str) -> String {
    if ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type) {
        "Images must be 10 MB or smaller".to_string()
    } else {
        "Files must be 50 MB or smaller".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn multipart_encodes_single_file_field() {
        let body = build_multipart("BOUND", "a.png", "image/png", b"PNG!");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with("--BOUND\r\n"));
        assert!(text.contains("Content-Disposition: form-data; name=\"file\"; filename=\"a.png\"\r\n"));
        assert!(text.contains("Content-Type: image/png\r\n\r\nPNG!"));
        assert!(text.ends_with("\r\n--BOUND--\r\n"));
    }

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
    fn validate_rejects_bad_type_and_size() {
        assert!(validate_image("image/png", 10).is_ok());
        assert!(validate_image("image/tiff", 10).is_err());
        assert!(validate_image("image/png", MAX_IMAGE_UPLOAD_BYTES + 1).is_err());
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

    // -- EXP-297 read_any_file ------------------------------------------------

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-read-any-file-{}-{}",
            std::process::id(),
            new_draft_url().replace(DRAFT_SCHEME, "")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn read_any_file_maps_known_extensions() {
        for (name, expected) in [
            ("report.pdf", "application/pdf"),
            ("bundle.zip", "application/zip"),
            ("notes.TXT", "text/plain"),
            ("readme.md", "text/markdown"),
            ("rows.csv", "text/csv"),
            ("data.json", "application/json"),
            ("clip.mp4", "video/mp4"),
            ("clip.mov", "video/quicktime"),
            ("clip.webm", "video/webm"),
            ("song.mp3", "audio/mpeg"),
            ("song.wav", "audio/wav"),
            ("memo.doc", "application/msword"),
            (
                "memo.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            ("sheet.xls", "application/vnd.ms-excel"),
            (
                "sheet.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
            (
                "deck.pptx",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            ),
            // Inline-image types keep their exact mime (the classification
            // rule is content-type equality, not extension).
            ("shot.png", "image/png"),
            ("shot.JPEG", "image/jpeg"),
            // Non-inline image types are ordinary files.
            ("vector.svg", "image/svg+xml"),
            ("scan.tiff", "image/tiff"),
        ] {
            let path = temp_file(name, b"data");
            let (filename, content_type, bytes) = read_any_file(&path).expect(name);
            assert_eq!(filename, name);
            assert_eq!(content_type, expected, "{name}");
            assert_eq!(bytes, b"data");
        }
    }

    #[test]
    fn read_any_file_falls_back_to_octet_stream() {
        for name in ["archive.weirdext", "Makefile"] {
            let path = temp_file(name, b"x");
            let (_, content_type, _) = read_any_file(&path).unwrap();
            assert_eq!(content_type, "application/octet-stream");
        }
    }

    #[test]
    fn read_any_file_enforces_the_caps_and_rejects_empty() {
        // Images keep the 10 MB cap; everything else gets 50 MB.
        assert_eq!(max_upload_bytes_for("image/png"), MAX_IMAGE_UPLOAD_BYTES);
        assert_eq!(max_upload_bytes_for("image/svg+xml"), MAX_FILE_UPLOAD_BYTES);
        assert_eq!(max_upload_bytes_for("application/pdf"), MAX_FILE_UPLOAD_BYTES);

        let oversized = temp_file("big.png", &vec![0u8; MAX_IMAGE_UPLOAD_BYTES + 1]);
        let error = read_any_file(&oversized).unwrap_err().to_string();
        assert!(error.contains("10 MB"), "{error}");

        // The same byte count is fine for a non-image.
        let ok = temp_file("big.bin", &vec![0u8; MAX_IMAGE_UPLOAD_BYTES + 1]);
        assert!(read_any_file(&ok).is_ok());

        let empty = temp_file("empty.txt", b"");
        assert!(read_any_file(&empty)
            .unwrap_err()
            .to_string()
            .contains("empty"));

        assert!(read_any_file(std::path::Path::new("/definitely/missing.pdf")).is_err());
    }

    struct NullToken;
    impl api::TokenProvider for NullToken {
        fn token(&self) -> Option<String> {
            Some("tok-9".to_string())
        }
    }

    /// One-shot canned HTTP server (same pattern as `api::trpc` tests).
    fn one_shot_server(status: u16, body: &'static str) -> (String, flume::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = flume::bounded::<String>(1);
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut captured = Vec::new();
            let mut buf = [0u8; 8192];
            let (mut head_end, mut content_length) = (None, 0usize);
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                captured.extend_from_slice(&buf[..n]);
                if head_end.is_none() {
                    if let Some(pos) = captured.windows(4).position(|w| w == b"\r\n\r\n") {
                        head_end = Some(pos + 4);
                        let head = String::from_utf8_lossy(&captured[..pos + 4]);
                        content_length = head
                            .lines()
                            .find_map(|l| {
                                let (name, value) = l.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().ok())?
                            })
                            .unwrap_or(0);
                    }
                }
                if let Some(pos) = head_end {
                    if captured.len() >= pos + content_length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            let _ = tx.send(String::from_utf8_lossy(&captured).into_owned());
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    #[test]
    fn upload_posts_multipart_with_bearer_and_decodes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"id":"att-1","url":"/api/attachments/att-1","filename":"a.png","contentType":"image/png","sizeBytes":4,"width":2,"height":2}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let uploaded = transport
            .upload("issue-1", "a.png", "image/png", b"PNG!")
            .expect("upload");
        assert_eq!(uploaded.id, "att-1");
        assert_eq!(uploaded.url, "/api/attachments/att-1");
        assert_eq!(uploaded.width, Some(2));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/issues/issue-1/files HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer tok-9"),
            "bearer missing (header names go out lowercase on the wire)"
        );
        assert!(request.contains("multipart/form-data; boundary="));
        assert!(request.contains("name=\"file\"; filename=\"a.png\""));
    }

    // EXP-297: the files rail posts to `/files` — same multipart body, same
    // response shape, `width`/`height` null for a non-image.
    #[test]
    fn upload_file_posts_to_the_files_route() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"id":"att-2","url":"/api/attachments/att-2","filename":"report.pdf","contentType":"application/pdf","sizeBytes":4,"width":null,"height":null}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let uploaded = transport
            .upload_file("issue-1", "report.pdf", "application/pdf", b"PDF!")
            .expect("upload");
        assert_eq!(uploaded.id, "att-2");
        assert_eq!(uploaded.width, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/issues/issue-1/files HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer tok-9"),
            "bearer missing (header names go out lowercase on the wire)"
        );
        assert!(request.contains("name=\"file\"; filename=\"report.pdf\""));
        assert!(request.contains("Content-Type: application/pdf"));
    }

    // The server's user-facing message survives — ureq's own summary would
    // only say "status code 400".
    #[test]
    fn upload_failures_surface_the_server_message() {
        // The real shape both upload routes emit: {"error": "<string>"}
        // (errorToResponse in apps/web/src/lib/http-errors.ts).
        let (base, _captured) = one_shot_server(
            400,
            r#"{"error":"Files must be 50 MB or smaller"}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let error = transport
            .upload_file("issue-1", "big.zip", "application/zip", b"Z")
            .unwrap_err()
            .to_string();
        assert!(error.contains("Files must be 50 MB or smaller"), "{error}");
    }

    #[test]
    fn upload_failures_also_read_the_nested_envelope_fallback() {
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"Team storage limit reached"}}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let error = transport
            .upload_file("issue-1", "big.zip", "application/zip", b"Z")
            .unwrap_err()
            .to_string();
        assert!(error.contains("Team storage limit reached"), "{error}");
    }

    #[test]
    fn fetch_resolves_relative_against_base() {
        let (base, captured) = one_shot_server(200, "BYTES");
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let bytes = transport.fetch("/api/attachments/att-2").expect("fetch");
        assert_eq!(bytes, b"BYTES");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/attachments/att-2 HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer tok-9"),
            "bearer missing (header names go out lowercase on the wire)"
        );
    }
}
