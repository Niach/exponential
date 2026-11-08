//! The attachment upload path, gpui-free (#EDE-65 lane L): the multipart
//! HTTP transport every upload route rides (`POST /api/issues/{id}/files`,
//! `/api/sessions/{id}/files`, `/api/teams/{id}/session-files`, issue drafts,
//! apps, the team image), the bearer GET of `/api/attachments/{id}`, the
//! client-side type and size caps, and the composers' sequential idempotent
//! [`upload_all`] + [`uploaded_message`]. The IDE (`ui::markdown::image_paste`,
//! `ui::composer_images`) and eDE's shell (`ede_chat attach`) share it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context as _};
use serde::Deserialize;

use domain::image_message::{build_steer_message, SteerFile};

/// EXP-1003: a non-2xx `/api/attachments/{id}` fetch, typed so callers can
/// tell a 404 ("no longer available") from any other status via
/// `anyhow::Error::downcast_ref`. `Display` keeps the legacy text
/// (`attachment fetch failed: HTTP 404 Not Found`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachmentFetchStatus(pub u16);

impl std::fmt::Display for AttachmentFetchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match reqwest::StatusCode::from_u16(self.0) {
            Ok(status) => write!(f, "attachment fetch failed: HTTP {status}"),
            Err(_) => write!(f, "attachment fetch failed: HTTP {}", self.0),
        }
    }
}

impl std::error::Error for AttachmentFetchStatus {}

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

/// Total-request budget of an attachment UPLOAD (EXP-297): generous enough
/// for a 50 MB upload on a slow uplink, still bounded so a hung server
/// eventually fails. Fetches keep the shorter [`ATTACHMENT_TIMEOUT`].
const UPLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// Response of the issue upload route (`POST /api/issues/{id}/files`).
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
    /// EXP-824: probed media duration (video/audio uploads; null otherwise).
    #[serde(default, rename = "durationMs")]
    pub duration_ms: Option<i64>,
    /// EXP-824: `/api/attachments/{id}?poster=1` when the server holds a
    /// poster frame for the upload (desktop sends none, the server may still
    /// extract one).
    #[serde(default, rename = "posterUrl")]
    pub poster_url: Option<String>,
    #[serde(default, rename = "videoCodec")]
    pub video_codec: Option<String>,
    #[serde(default, rename = "audioCodec")]
    pub audio_codec: Option<String>,
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
    /// Upload one attachment to an issue — inline images and arbitrary files
    /// alike ride the ONE route `POST /api/issues/{id}/files` (single-part
    /// multipart `file`, atomic on the server, one response shape with
    /// `width`/`height` null for non-images). The cap that applies is
    /// [`max_upload_bytes_for`]; the error label follows the content type.
    fn upload(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// EXP-702: upload one image to a CODING SESSION —
    /// `POST /api/sessions/{id}/files`, same single-part multipart `file`
    /// and the same response shape as [`Self::upload`], only the path
    /// differs. Steered screenshots go here, an issue run's included, so
    /// they never land in the issue's Files section; the server gates on
    /// session ownership. Wave D: ANY file type (images 10 MB, files 50 MB).
    fn upload_session(
        &self,
        session_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// EXP-825: upload one image (wave D: or any file) BEFORE a session exists —
    /// `POST /api/teams/{id}/session-files`, the same multipart part and
    /// response shape as [`Self::upload_session`]. The row is created with
    /// `session_id NULL`; the start that embeds it binds it through
    /// `codingSessions.start`'s `attachmentIds`, and an abandoned upload is
    /// reclaimed by the server's orphan sweep.
    fn upload_team_session_file(
        &self,
        team_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// EXP-878: upload one attachment to an ISSUE DRAFT —
    /// `POST /api/issue-drafts/{id}/files`, the same single-part multipart
    /// `file` and the same response shape as [`Self::upload`], only the path
    /// differs (owner-only). The create-issue dialog uploads EAGERLY onto
    /// the draft it is composing in, so a close never has bytes to lose; the
    /// `issues.create` that files the draft reparents the rows.
    fn upload_draft(
        &self,
        draft_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage>;

    /// EXP-1247: [`Self::upload`] from a FILE button (paperclip / Attach
    /// file): the row is stamped `as_file` (`?asFile=1`), so an image stays a
    /// Files row and never inlines. A test transport may keep the default.
    fn upload_as_file(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        self.upload(issue_id, filename, content_type, bytes)
    }

    /// EXP-1247: [`Self::upload_draft`] from a FILE button (`?asFile=1`).
    fn upload_draft_as_file(
        &self,
        draft_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        self.upload_draft(draft_id, filename, content_type, bytes)
    }

    /// GET attachment bytes. `url` may be the canonical relative form or
    /// absolute; relative resolves against the instance base URL.
    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>>;
}

/// [`AttachmentTransport`] over the app's one shared HTTP client, authenticated
/// with the account's call-time bearer (same §5.7 rule as `api::TrpcClient` — a
/// re-login is picked up by the very next request).
pub struct HttpAttachmentTransport {
    base_url: String,
    token: Arc<dyn crate::TokenProvider>,
}

/// Attachments can be megabytes on a slow link, so they get a longer budget
/// than the shared client's 30s default.
const ATTACHMENT_TIMEOUT: Duration = Duration::from_secs(60);

impl HttpAttachmentTransport {
    pub fn new(instance_url: &str, token: Arc<dyn crate::TokenProvider>) -> Self {
        Self {
            base_url: instance_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    fn absolute(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}{}", self.base_url, url)
        }
    }

    /// Whether `absolute` is on THIS instance's origin: the only host the
    /// account bearer may be sent to. A third-party image URL in a comment
    /// or an uploaded `.md` (`![](https://evil.example/x.png)`) is fetched
    /// like a browser would, with no credentials at all.
    fn is_instance_url(&self, absolute: &str) -> bool {
        absolute
            .strip_prefix(self.base_url.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
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

    /// VAPP-221 (apps.json `appFiles`): one file of an app's FileUpload →
    /// `POST /api/apps/{appId}/files`, the same single-part multipart `file`.
    pub fn upload_app_file(
        &self,
        app_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}{}", self.base_url, domain::apps::app_upload_path(app_id));
        self.post_multipart(&url, filename, content_type, bytes)
    }

    /// #EXP-1645 (team-image.json `upload`): the team image → `POST
    /// /api/teams/{teamId}/icon`, the same single-part multipart `file`. The
    /// answer's `{iconUrl}` is not read: the shape delivers it.
    pub fn upload_team_image(
        &self,
        team_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        let url = format!("{}{}", self.base_url, domain::team_image::upload_path(team_id));
        self.post_multipart_text(&url, "image", filename, content_type, bytes).map(|_| ())
    }

    /// One multipart POST to the issue upload route. The user-facing error
    /// label follows the payload: an inline image is an "image", every other
    /// content type a "file".
    fn post_multipart(
        &self,
        url: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let what = if is_inline_image(content_type) {
            "image"
        } else {
            "file"
        };
        let text = self.post_multipart_text(url, what, filename, content_type, bytes)?;
        serde_json::from_str(&text).with_context(|| format!("decode upload response: {text}"))
    }

    /// The POST itself: the 2xx body as text.
    fn post_multipart_text(
        &self,
        url: &str,
        what: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<String> {
        let boundary = format!("----ExpMarkdownEditor{}", rand_ish());
        let body = build_multipart(&boundary, filename, content_type, bytes);
        let response = self
            .authorize(crate::http::shared().post(url))
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
        Ok(text)
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
        let url = format!("{}/api/issues/{issue_id}/files", self.base_url);
        self.post_multipart(&url, filename, content_type, bytes)
    }

    fn upload_session(
        &self,
        session_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/sessions/{session_id}/files", self.base_url);
        let uploaded = self.post_multipart(&url, filename, content_type, bytes)?;
        domain::sealed::remember_upload(&uploaded.id, filename, content_type);
        Ok(uploaded)
    }

    fn upload_team_session_file(
        &self,
        team_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/teams/{team_id}/session-files", self.base_url);
        let uploaded = self.post_multipart(&url, filename, content_type, bytes)?;
        domain::sealed::remember_upload(&uploaded.id, filename, content_type);
        Ok(uploaded)
    }

    fn upload_draft(
        &self,
        draft_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/issue-drafts/{draft_id}/files", self.base_url);
        self.post_multipart(&url, filename, content_type, bytes)
    }

    fn upload_as_file(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/issues/{issue_id}/files?asFile=1", self.base_url);
        self.post_multipart(&url, filename, content_type, bytes)
    }

    fn upload_draft_as_file(
        &self,
        draft_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let url = format!("{}/api/issue-drafts/{draft_id}/files?asFile=1", self.base_url);
        self.post_multipart(&url, filename, content_type, bytes)
    }

    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let absolute = self.absolute(url);
        let request = crate::http::shared().get(&absolute);
        // Only our own origin is authorized; a foreign image URL rides a
        // bare GET (the web renderer's `<img src>` sends nothing either).
        let request = if self.is_instance_url(&absolute) {
            self.authorize(request)
        } else {
            request.timeout(ATTACHMENT_TIMEOUT)
        };
        let response = request
            .send()
            .map_err(|e| anyhow!("attachment fetch failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AttachmentFetchStatus(status.as_u16()).into());
        }
        Ok(response.bytes().context("attachment body")?.to_vec())
    }
}

static NEXT_BOUNDARY: AtomicU64 = AtomicU64::new(1);

fn rand_ish() -> u64 {
    let n = NEXT_BOUNDARY.fetch_add(1, Ordering::Relaxed);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        .wrapping_add(n)
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
pub fn content_type_for_path(path: &std::path::Path) -> &'static str {
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

/// One upload a composer performs, snapshotted off its strip so the
/// background task owns nothing of the view. `bytes` is whatever holds the
/// staged buffer (the IDE's thumbnail image, eDE's plain `Vec`), so the bytes
/// are never copied.
pub struct UploadJob<B> {
    pub key: u64,
    pub uploaded_id: Option<String>,
    pub uploaded_name: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub bytes: B,
}

/// One upload that landed: `(strip key, attachment id, the server's
/// sanitized filename)`.
pub type Uploaded = (u64, String, String);

/// Is `content_type` one of the five inline image types (an IMAGE tile with
/// a marker)? Everything else stages as a file.
pub fn is_inline_image(content_type: &str) -> bool {
    ACCEPTED_IMAGE_CONTENT_TYPES.contains(&content_type)
}

/// Run `jobs` SEQUENTIALLY and idempotently: an already-uploaded image is
/// skipped, a mid-batch failure returns what landed so the caller can keep
/// the draft and a retry uploads only the rest (web parity). `upload` is the
/// route-specific call (session files or team session files).
pub fn upload_all<B: AsRef<[u8]>>(
    jobs: Vec<UploadJob<B>>,
    upload: impl Fn(&str, &str, &[u8]) -> anyhow::Result<UploadedImage>,
) -> Result<Vec<Uploaded>, (Vec<Uploaded>, String)> {
    let mut resolved: Vec<Uploaded> = Vec::with_capacity(jobs.len());
    for job in jobs {
        match job.uploaded_id {
            Some(id) => {
                let name = job.uploaded_name.unwrap_or(job.filename);
                resolved.push((job.key, id, name));
            }
            None => {
                let image = upload(&job.filename, &job.content_type, job.bytes.as_ref())
                    .map_err(|err| (resolved.clone(), err.to_string()))?;
                // The SERVER's sanitized filename is the file line's text.
                let name = image.filename.clone().unwrap_or(job.filename);
                resolved.push((job.key, image.id, name));
            }
        }
    }
    Ok(resolved)
}

/// EXP-1537: why an upload-then-send did not go out. Both arms carry the
/// uploads that landed, so a retry uploads only the rest.
pub enum UploadSendError {
    Upload(Vec<Uploaded>, String),
    Send(Vec<Uploaded>, crate::ApiError),
}

/// The keys of `jobs` that are FILES (no marker, the message's file block).
pub fn file_keys<B>(jobs: &[UploadJob<B>]) -> Vec<u64> {
    jobs.iter()
        .filter(|job| !is_inline_image(&job.content_type))
        .map(|job| job.key)
        .collect()
}

/// Wave D: the message once every upload landed — the prose, the image
/// embeds, then the file lines with the server's filenames
/// (`domain::image_message::build_steer_message`). Off the snapshot alone,
/// so a send whose view closed under its upload still goes out (EXP-1537).
pub fn uploaded_message(text: &str, file_keys: &[u64], resolved: &[Uploaded]) -> String {
    let images: Vec<String> = resolved
        .iter()
        .filter(|(key, _, _)| !file_keys.contains(key))
        .map(|(_, id, _)| id.clone())
        .collect();
    let files: Vec<SteerFile> = resolved
        .iter()
        .filter(|(key, _, _)| file_keys.contains(key))
        .map(|(_, id, name)| SteerFile::new(id.clone(), name.clone()))
        .collect();
    build_steer_message(text, &images, &files)
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
    fn validate_rejects_bad_type_and_size() {
        assert!(validate_image("image/png", 10).is_ok());
        assert!(validate_image("image/tiff", 10).is_err());
        assert!(validate_image("image/png", MAX_IMAGE_UPLOAD_BYTES + 1).is_err());
    }

    // -- EXP-297 read_any_file ------------------------------------------------

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-read-any-file-{}-{}",
            std::process::id(),
            rand_ish()
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
    impl crate::TokenProvider for NullToken {
        fn token(&self) -> Option<String> {
            Some("tok-9".to_string())
        }
    }

    /// #EXP-1645: the team image is one `file` part to the fixture's route,
    /// with the account's bearer; a refusal surfaces the server's message.
    #[test]
    fn the_team_image_posts_one_file_part_to_the_team_route() {
        let (base, rx) = one_shot_server(200, r#"{"iconUrl":"/api/attachments/a-1"}"#);
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        transport.upload_team_image("t-1", "logo.svg", "image/svg+xml", b"<svg/>").unwrap();
        let request = rx.recv().unwrap();
        assert!(request.starts_with("POST /api/teams/t-1/icon "), "{request}");
        assert!(request.to_lowercase().contains("authorization: bearer tok-9"), "{request}");
        assert!(request.contains("name=\"file\"; filename=\"logo.svg\""), "{request}");
        assert!(request.contains("Content-Type: image/svg+xml"), "{request}");

        let (base, _rx) = one_shot_server(400, r#"{"error":"That image could not be read"}"#);
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let error = transport.upload_team_image("t-1", "x.png", "image/png", b"x").unwrap_err();
        assert!(error.to_string().contains("That image could not be read"), "{error}");
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

    /// EXP-1247: a file button's pick rides the same routes with `asFile=1`.
    #[test]
    fn an_as_file_upload_marks_the_route() {
        for draft in [false, true] {
            let (base, captured) = one_shot_server(
                200,
                r#"{"id":"att-1","url":"/api/attachments/att-1","filename":"a.png","contentType":"image/png","sizeBytes":4}"#,
            );
            let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
            let uploaded = if draft {
                transport.upload_draft_as_file("d-1", "a.png", "image/png", b"PNG!")
            } else {
                transport.upload_as_file("issue-1", "a.png", "image/png", b"PNG!")
            }
            .expect("upload");
            assert_eq!(uploaded.id, "att-1");
            let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
            let line = if draft {
                "POST /api/issue-drafts/d-1/files?asFile=1 HTTP/1.1"
            } else {
                "POST /api/issues/issue-1/files?asFile=1 HTTP/1.1"
            };
            assert!(request.starts_with(line), "{request}");
        }
    }

    // EXP-297: a non-image rides the very same `/files` route — same
    // multipart body, same response shape, `width`/`height` null for it.
    #[test]
    fn file_upload_posts_to_the_files_route() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"id":"att-2","url":"/api/attachments/att-2","filename":"report.pdf","contentType":"application/pdf","sizeBytes":4,"width":null,"height":null}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let uploaded = transport
            .upload("issue-1", "report.pdf", "application/pdf", b"PDF!")
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
        // The real shape the upload route emits: {"error": "<string>"}
        // (errorToResponse in apps/web/src/lib/http-errors.ts).
        let (base, _captured) = one_shot_server(
            400,
            r#"{"error":"Files must be 50 MB or smaller"}"#,
        );
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let error = transport
            .upload("issue-1", "big.zip", "application/zip", b"Z")
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
            .upload("issue-1", "big.zip", "application/zip", b"Z")
            .unwrap_err()
            .to_string();
        assert!(error.contains("Team storage limit reached"), "{error}");
    }

    /// EXP-1003: a 404 surfaces as the typed status (the markdown preview's
    /// "no longer available" copy), the text unchanged.
    #[test]
    fn fetch_failure_carries_the_typed_status() {
        let (base, _captured) = one_shot_server(404, "");
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let error = transport.fetch("/api/attachments/gone").unwrap_err();
        assert_eq!(
            error.downcast_ref::<AttachmentFetchStatus>(),
            Some(&AttachmentFetchStatus(404))
        );
        assert_eq!(error.to_string(), "attachment fetch failed: HTTP 404 Not Found");
    }

    /// A foreign image URL (a third-party `![](https://…)` in a comment or an
    /// uploaded `.md`) is fetched WITHOUT the account bearer: the token goes
    /// to this instance's origin only.
    #[test]
    fn fetch_never_sends_the_bearer_to_a_foreign_host() {
        let (base, _own) = one_shot_server(200, "OWN");
        let (foreign, captured) = one_shot_server(200, "FOREIGN");
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        assert!(transport.is_instance_url(&format!("{base}/api/attachments/a")));
        assert!(transport.is_instance_url(&base));
        assert!(!transport.is_instance_url(&format!("{base}5/api/attachments/a")));
        assert!(!transport.is_instance_url(&format!("{foreign}/img.png")));
        let bytes = transport
            .fetch(&format!("{foreign}/img.png"))
            .expect("fetch");
        assert_eq!(bytes, b"FOREIGN");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /img.png HTTP/1.1"), "{request}");
        assert!(
            !request.to_ascii_lowercase().contains("authorization:"),
            "bearer leaked to a foreign host: {request}"
        );
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

    fn uploaded(id: &str) -> UploadedImage {
        serde_json::from_value(serde_json::json!({ "id": id, "url": format!("/api/attachments/{id}") }))
            .expect("a minimal upload response")
    }

    fn job(key: u64, uploaded: Option<&str>) -> UploadJob<Vec<u8>> {
        UploadJob {
            key,
            uploaded_id: uploaded.map(str::to_string),
            uploaded_name: None,
            filename: format!("{key}.png"),
            content_type: "image/png".into(),
            bytes: vec![key as u8],
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

    /// Wave D: the message = prose, image embeds, then the file lines with
    /// the server's names; `file_keys` picks the files by content type.
    #[test]
    fn uploaded_message_puts_images_before_files() {
        let mut pdf = job(2, Some("33333333-3333-4333-8333-333333333333"));
        pdf.content_type = "application/pdf".into();
        let png = job(1, Some("11111111-1111-4111-8111-111111111111"));
        let jobs = vec![pdf, png];
        let keys = file_keys(&jobs);
        assert_eq!(keys, vec![2]);
        let resolved = upload_all(jobs, |_, _, _| anyhow::bail!("never called")).unwrap();
        let resolved: Vec<Uploaded> = resolved
            .into_iter()
            .map(|(key, id, name)| (key, id, if key == 2 { "notes (1).pdf".into() } else { name }))
            .collect();
        assert_eq!(
            uploaded_message("look [Image #1]", &keys, &resolved),
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
}
