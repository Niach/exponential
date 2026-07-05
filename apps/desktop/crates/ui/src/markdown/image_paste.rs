//! The single image path of the markdown editor (masterplan-v3
//! §4.5 "Images"): clipboard paste, drag-drop and the toolbar file picker all
//! funnel through one staging + upload seam.
//!
//! Mirrors the web semantics exactly:
//! - **Detail editor** (issue exists): upload immediately via
//!   `POST /api/issues/{issueId}/images` (multipart `file` field —
//!   `apps/web/src/routes/api/issues/$issueId/images.ts`), then insert the
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

/// Response of `POST /api/issues/{issueId}/images`.
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

    /// GET attachment bytes. `url` may be the canonical relative form or
    /// absolute; relative resolves against the instance base URL.
    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>>;
}

/// [`AttachmentTransport`] over `ureq`, authenticated with the account's
/// call-time bearer (same §5.7 rule as `api::TrpcClient` — a re-login is
/// picked up by the very next request).
pub struct HttpAttachmentTransport {
    base_url: String,
    token: Arc<dyn api::TokenProvider>,
    agent: ureq::Agent,
}

impl HttpAttachmentTransport {
    pub fn new(instance_url: &str, token: Arc<dyn api::TokenProvider>) -> Self {
        Self {
            base_url: instance_url.trim_end_matches('/').to_string(),
            token,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(60))
                .build(),
        }
    }

    fn absolute(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}{}", self.base_url, url)
        }
    }

    fn authorize(&self, request: ureq::Request) -> ureq::Request {
        match self.token.token() {
            Some(token) => request.set("Authorization", &format!("Bearer {token}")),
            None => request,
        }
    }
}

impl AttachmentTransport for HttpAttachmentTransport {
    fn upload(
        &self,
        issue_id: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<UploadedImage> {
        let boundary = format!("----ExpMarkdownEditor{}", new_draft_url().len() as u64 + rand_ish());
        let body = build_multipart(&boundary, filename, content_type, bytes);
        let url = format!("{}/api/issues/{issue_id}/images", self.base_url);
        let request = self
            .authorize(self.agent.post(&url))
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .set("Accept", "application/json");
        let response = request
            .send_bytes(&body)
            .map_err(|e| anyhow!("image upload failed: {e}"))?;
        let text = response.into_string().context("image upload response")?;
        serde_json::from_str(&text).with_context(|| format!("decode upload response: {text}"))
    }

    fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let absolute = self.absolute(url);
        let response = self
            .authorize(self.agent.get(&absolute))
            .call()
            .map_err(|e| anyhow!("attachment fetch failed: {e}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .context("attachment body")?;
        Ok(bytes)
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
        assert!(request.starts_with("POST /api/issues/issue-1/images HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer tok-9"));
        assert!(request.contains("multipart/form-data; boundary="));
        assert!(request.contains("name=\"file\"; filename=\"a.png\""));
    }

    #[test]
    fn fetch_resolves_relative_against_base() {
        let (base, captured) = one_shot_server(200, "BYTES");
        let transport = HttpAttachmentTransport::new(&base, Arc::new(NullToken));
        let bytes = transport.fetch("/api/attachments/att-2").expect("fetch");
        assert_eq!(bytes, b"BYTES");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/attachments/att-2 HTTP/1.1"));
        assert!(request.contains("Authorization: Bearer tok-9"));
    }
}
