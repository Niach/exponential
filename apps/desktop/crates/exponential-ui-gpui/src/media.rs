//! VAPP-91: the media loader. `Image`, `Avatar` and `Video` posters load
//! through the host's [`crate::host::HostPlugin::media_request`]: the
//! absolute url plus the headers its media rules add (auth for
//! `/api/attachments`, …), fetched by this crate — gpui's own `img(url)`
//! goes through the app's `http_client`, which is a `NullHttpClient` unless
//! the app installs one, and it cannot carry per-request headers.
//!
//! VAPP-103: every src passes the media policy (`catalog/host.json` media:
//! schemes, hosts; no local file unless the host lists `file`), and so does
//! every redirect hop (the HOST's policy, never an https→http downgrade,
//! the hop's headers = the rules matching the NEW url). Every image load
//! keeps the media limits: connect and each read within
//! `MEDIA_TIMEOUT_MS`, the body (Content-Length up front, then as it
//! streams) within `MEDIA_MAX_BYTES`, width × height × frames within
//! `MEDIA_MAX_PIXELS` BEFORE any decode (an SVG at gpui's 2× raster, an
//! unreadable raster refused). Video / AudioPlayer hand-offs stream to a
//! file with no byte cap (`catalog/host.json`: the byte limits are images').

use std::sync::Arc;

use exponential_ui::host::{media_image_within_limits, media_request, media_within_limits, svg_dimensions, MediaOptions, MediaRequest, MEDIA_MAX_BYTES};
#[cfg(feature = "net")]
use exponential_ui::host::MEDIA_TIMEOUT_MS;
use gpui::{App, Asset, Image, ImageCacheError, ImageFormat, ImageSource, RenderImage, SharedString, Window};

use crate::host::HostPlugin;

/// One media load: the url, the headers (sorted, so the asset key is
/// stable) and the host's media policy as JSON (every redirect hop passes
/// it; empty = the default policy).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MediaKey {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub policy: String,
}

impl MediaKey {
    /// The key for a request the host's `options` allowed.
    pub fn new(r: &MediaRequest, options: &MediaOptions) -> Self {
        MediaKey { url: r.url.clone(), headers: r.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), policy: serde_json::to_string(options).unwrap_or_default() }
    }

    /// The host's media policy this load runs under.
    pub fn options(&self) -> MediaOptions {
        serde_json::from_str(&self.policy).unwrap_or_default()
    }
}

/// The gpui asset that fetches + decodes a [`MediaKey`] (cached by gpui's
/// asset system per key).
pub enum MediaLoader {}

/// The format of an image by its magic bytes; `None` = not one gpui decodes
/// (SVG is sniffed by an `<svg` / `<?xml` prefix).
pub fn sniff_format(bytes: &[u8]) -> Option<ImageFormat> {
    let starts = |m: &[u8]| bytes.starts_with(m);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if starts(b"\xff\xd8\xff") {
        Some(ImageFormat::Jpeg)
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else if starts(b"BM") {
        Some(ImageFormat::Bmp)
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_lowercase();
        let head = head.trim_start();
        (head.starts_with("<svg") || head.starts_with("<?xml")).then_some(ImageFormat::Svg)
    }
}

/// The bytes of a [`MediaKey`] under the media limits (the policy already
/// passed: [`image_source`] builds keys only from allowed requests).
pub fn fetch(key: &MediaKey) -> Result<Vec<u8>, String> {
    let bytes = if key.url.starts_with("data:") {
        let bytes = crate::paint::natives::data_bytes(&key.url).ok_or_else(|| format!("{}: not a data image", truncate(&key.url)))?;
        media_within_limits(bytes.len() as u64, None)?;
        bytes
    } else if key.url.starts_with("file:") {
        read_file(&key.url)?
    } else {
        fetch_http(key)?
    };
    media_image_within_limits(&bytes)?;
    // gpui rasterizes an SVG at twice its size (`SMOOTH_SVG_SCALE_FACTOR`).
    if sniff_format(&bytes) == Some(ImageFormat::Svg) {
        let (w, h) = svg_dimensions(&bytes).unwrap_or((100, 100));
        media_within_limits(bytes.len() as u64, Some((w.saturating_mul(2), h.saturating_mul(2))))?;
    }
    Ok(bytes)
}

fn truncate(url: &str) -> &str {
    &url[..url.char_indices().nth(64).map_or(url.len(), |(i, _)| i)]
}

/// A `file:` url the host's media schemes allowed.
fn read_file(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let path = url::Url::parse(url).ok().and_then(|u| u.to_file_path().ok()).ok_or_else(|| format!("{url}: not a file path"))?;
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    media_within_limits(len, None)?;
    let mut out = Vec::new();
    file.take(MEDIA_MAX_BYTES + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn fetch_http(key: &MediaKey) -> Result<Vec<u8>, String> {
    let (res, _url) = send(key)?;
    if let Some(len) = res.content_length() {
        media_within_limits(len, None)?;
    }
    read_capped(res)
}

/// The response's body within `MEDIA_MAX_BYTES` (one byte over = refused).
#[cfg(feature = "net")]
fn read_capped(res: reqwest::blocking::Response) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    res.take(MEDIA_MAX_BYTES + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(not(feature = "net"))]
fn read_capped(_res: Response) -> Result<Vec<u8>, String> {
    Err("http(s) media needs the `net` feature".into())
}

#[cfg(not(feature = "net"))]
struct Response;

#[cfg(not(feature = "net"))]
impl Response {
    fn content_length(&self) -> Option<u64> {
        None
    }
}

/// The next hop of a redirect from `from` to `location` under the host's
/// media `options`: `None` = refused (the policy denies the new url, or it
/// downgrades https to http); else the new url and ITS headers (the rules
/// matching it: a header the old url's rule added never follows to a host
/// it does not cover).
pub fn redirect_hop(from: &str, location: &str, options: &MediaOptions) -> Option<MediaRequest> {
    let next = url::Url::parse(from).ok()?.join(location).ok()?;
    if from.starts_with("https:") && next.scheme() != "https" {
        return None;
    }
    let base = MediaOptions { base_url: None, ..options.clone() };
    media_request(next.as_str(), &base)
}

/// Send a policed GET: redirects followed BY HAND (at most 5), each hop
/// through [`redirect_hop`]; connect and every read within
/// `MEDIA_TIMEOUT_MS` (an idle bound, never the whole body). The final
/// response (2xx) and its url.
#[cfg(feature = "net")]
fn send(key: &MediaKey) -> Result<(reqwest::blocking::Response, String), String> {
    use std::time::Duration;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_millis(MEDIA_TIMEOUT_MS))
        .timeout(Duration::from_millis(MEDIA_TIMEOUT_MS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let options = key.options();
    let mut url = key.url.clone();
    let mut headers = key.headers.clone();
    for _ in 0..=5 {
        let mut req = client.get(&url);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = req.send().map_err(|e| e.to_string())?;
        let status = res.status();
        if status.is_redirection() {
            let location = res.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).ok_or_else(|| format!("HTTP {} without a Location for {url}", status.as_u16()))?;
            let hop = redirect_hop(&url, location, &options).ok_or_else(|| format!("{url}: a redirect the media policy refuses"))?;
            url = hop.url;
            headers = hop.headers.into_iter().collect();
            continue;
        }
        if !status.is_success() {
            return Err(format!("HTTP {} for {url}", status.as_u16()));
        }
        return Ok((res, url));
    }
    Err(format!("{}: too many redirects", key.url))
}

#[cfg(not(feature = "net"))]
fn send(key: &MediaKey) -> Result<(Response, String), String> {
    Err(format!("{}: http(s) media needs the `net` feature", key.url))
}

/// The `Content-Type` of a response (lowercase, no parameters).
#[cfg(feature = "net")]
fn content_type(res: &reqwest::blocking::Response) -> Option<String> {
    res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|v| v.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
}

impl Asset for MediaLoader {
    type Source = MediaKey;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(source: Self::Source, cx: &mut App) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let svg = cx.svg_renderer();
        async move {
            let err = |e: String| ImageCacheError::Asset(SharedString::from(e));
            let bytes = fetch(&source).map_err(err)?;
            let format = sniff_format(&bytes).ok_or_else(|| err(format!("{}: not an image", source.url)))?;
            Image::from_bytes(format, bytes).to_image_data(svg).map_err(|e| err(e.to_string()))
        }
    }
}

/// The source `img()` paints for a media `src` (`None` = nothing to load):
/// the host's media request (policy + header rules), re-checked against its
/// media schemes and hosts (a custom hook cannot widen them), fetched and
/// decoded by [`MediaLoader`] under the media limits.
pub fn image_source(host: &dyn HostPlugin, src: &str) -> Option<ImageSource> {
    let req = allowed_request(host, src)?;
    let key = MediaKey::new(&req, &host.media_options());
    Some(ImageSource::Custom(Arc::new(move |window: &mut Window, cx: &mut App| window.use_asset::<MediaLoader>(&key, cx))))
}

/// The host's request for `src` when the media policy allows it.
pub fn allowed_request(host: &dyn HostPlugin, src: &str) -> Option<MediaRequest> {
    if src.is_empty() {
        return None;
    }
    let req = host.media_request(src)?;
    let options = host.media_options();
    let recheck = MediaOptions { schemes: options.schemes, hosts: options.hosts, ..Default::default() };
    media_request(&req.url, &recheck).map(|_| req)
}

// VAPP-103: Video / AudioPlayer playback. gpui has no audio or video
// pipeline (and none is in the lockfile), so a press hands the policed
// source to the system player: the honest desktop equivalent of the
// platform player the other renderers open inline.

/// How a policed Video / AudioPlayer src reaches the system player (Swift's
/// `MediaLoader.Playback`: stream unless the request carries headers or is
/// a `data:` url).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handoff {
    /// An http(s) url without headers: opened through
    /// [`HostPlugin::open_url`] (the URL policy, then the opener).
    Open(String),
    /// A `file:` src the host's media schemes list, with an audio/video
    /// extension: its path, opened through [`HostPlugin::open_media_file`]
    /// only when it is a regular file (never a directory or a bundle).
    File(std::path::PathBuf),
    /// Headers (auth) or a `data:` url: fetched under the media limits into
    /// a temporary file ([`fetch_to_file`]) that
    /// [`HostPlugin::open_media_file`] opens.
    Fetch(MediaKey),
}

/// The hand-off for a media `src`; `None` = no src or the media policy
/// DENIES it (nothing loads, the play control stays inert).
pub fn handoff(host: &dyn HostPlugin, src: &str) -> Option<Handoff> {
    let req = allowed_request(host, src)?;
    let scheme = req.url.split(':').next().unwrap_or("").to_ascii_lowercase();
    let key = || MediaKey::new(&req, &host.media_options());
    match scheme.as_str() {
        "file" => url::Url::parse(&req.url).ok()?.to_file_path().ok().filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(media_extension)).map(Handoff::File),
        "data" => Some(Handoff::Fetch(key())),
        "http" | "https" if req.headers.is_empty() => Some(Handoff::Open(req.url.clone())),
        "http" | "https" => Some(Handoff::Fetch(key())),
        _ => None,
    }
}

/// VAPP-103 rfix: the extensions a hand-off file may carry (the system
/// opener picks the app by it: never `.exe`, `.command` or `.app`).
const MEDIA_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "webm", "ogv", "mp3", "m4a", "aac", "wav", "ogg", "oga", "opus", "flac"];

/// An audio/video extension (any case) of [`MEDIA_EXTENSIONS`].
pub fn media_extension(ext: &str) -> bool {
    MEDIA_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

/// A `data:` url's MIME type and bytes (any type; the byte cap applies).
fn data_payload(url: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let rest = url.strip_prefix("data:").ok_or("not a data url")?;
    let (meta, data) = rest.split_once(',').ok_or("data url without a comma")?;
    if meta.ends_with(";base64") && (data.len() as u64 / 4) * 3 > MEDIA_MAX_BYTES {
        return Err(format!("data url over {MEDIA_MAX_BYTES} bytes"));
    }
    let mime = meta.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let bytes = if meta.ends_with(";base64") { crate::paint::natives::base64_decode(data).ok_or("bad base64")? } else { crate::paint::natives::percent_decode(data) };
    Ok((bytes, (!mime.is_empty()).then_some(mime)))
}

/// The file extension the system opener needs: a known audio/video MIME
/// type's, else the url path's when it is an audio/video one
/// ([`MEDIA_EXTENSIONS`]), else `fallback` (itself one, else `mp4`).
fn extension(mime: Option<&str>, url: &str, fallback: &str) -> String {
    let by_mime = match mime.unwrap_or("") {
        "video/mp4" => Some("mp4"),
        "video/quicktime" => Some("mov"),
        "video/webm" => Some("webm"),
        "video/ogg" => Some("ogv"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => Some("m4a"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/ogg" => Some("ogg"),
        "audio/webm" => Some("webm"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        _ => None,
    };
    if let Some(ext) = by_mime {
        return ext.to_string();
    }
    if !url.starts_with("data:") {
        let path = url.split(['?', '#']).next().unwrap_or("");
        let last = path.rsplit('/').next().unwrap_or("");
        if let Some((_, ext)) = last.rsplit_once('.') {
            if media_extension(ext) {
                return ext.to_ascii_lowercase();
            }
        }
    }
    if media_extension(fallback) { fallback.to_ascii_lowercase() } else { "mp4".to_string() }
}

/// Fetch a policed request into a fresh temporary file named with the
/// media's extension (`fallback` when neither the type nor the url names an
/// audio/video one). A `data:` url decodes within `MEDIA_MAX_BYTES`; an
/// http(s) body STREAMS to the file in chunks with no byte cap (the byte
/// limits are images'), every hop policed, connect and each read within
/// `MEDIA_TIMEOUT_MS`.
pub fn fetch_to_file(key: &MediaKey, fallback: &str) -> Result<std::path::PathBuf, String> {
    if key.url.starts_with("data:") {
        let (bytes, mime) = data_payload(&key.url)?;
        media_within_limits(bytes.len() as u64, None)?;
        let path = temp_media_path(mime.as_deref(), &key.url, fallback)?;
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        return Ok(path);
    }
    stream_to_file(key, fallback)
}

/// A fresh path in the media temp dir with the media's extension.
fn temp_media_path(mime: Option<&str>, url: &str, fallback: &str) -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("exponential-ui-media");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{}-{nanos}-{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), extension(mime, url, fallback));
    Ok(dir.join(name))
}

#[cfg(feature = "net")]
fn stream_to_file(key: &MediaKey, fallback: &str) -> Result<std::path::PathBuf, String> {
    use std::io::{Read, Write};
    let (mut res, url) = send(key)?;
    let path = temp_media_path(content_type(&res).as_deref(), &url, fallback)?;
    let result = (|| {
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = res.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => Ok(path),
        Err(e) => {
            std::fs::remove_file(&path).ok();
            Err(e)
        }
    }
}

#[cfg(not(feature = "net"))]
fn stream_to_file(key: &MediaKey, _fallback: &str) -> Result<std::path::PathBuf, String> {
    Err(format!("{}: http(s) media needs the `net` feature", key.url))
}

/// A Video / AudioPlayer press: hand `src` to the system player through
/// [`handoff`] (a denied src does nothing). A fetch runs on the background
/// executor; its file then goes to [`HostPlugin::open_media_file`].
/// `fallback` = the extension when the media names none (`mp4`, `m4a`).
pub fn play(host: std::rc::Rc<dyn HostPlugin>, src: &str, fallback: &'static str, cx: &mut App) {
    match handoff(host.as_ref(), src) {
        None => {}
        Some(Handoff::Open(url)) => host.open_url(&url, cx),
        // A regular file only (a directory or a bundle is never opened).
        Some(Handoff::File(path)) => {
            if std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                host.open_media_file(&path, cx)
            }
        }
        Some(Handoff::Fetch(key)) => {
            let fetch = cx.background_executor().spawn(async move { fetch_to_file(&key, fallback) });
            cx.spawn(async move |cx| {
                if let Ok(path) = fetch.await {
                    let _ = cx.update(|cx| host.open_media_file(&path, cx));
                }
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sniff_from_magic_bytes() {
        assert_eq!(sniff_format(b"\x89PNG\r\n\x1a\nrest"), Some(ImageFormat::Png));
        assert_eq!(sniff_format(b"\xff\xd8\xff\xe0"), Some(ImageFormat::Jpeg));
        assert_eq!(sniff_format(b"RIFF\0\0\0\0WEBPVP8 "), Some(ImageFormat::Webp));
        assert_eq!(sniff_format(b"  <svg xmlns=\"\"/>"), Some(ImageFormat::Svg));
        assert_eq!(sniff_format(b"{\"json\": true}"), None);
    }

    struct Plain;
    impl HostPlugin for Plain {}
    struct Files;
    impl HostPlugin for Files {
        fn media_options(&self) -> MediaOptions {
            MediaOptions { schemes: Some(vec!["file".into()]), ..Default::default() }
        }
    }
    struct Widening;
    impl HostPlugin for Widening {
        fn media_request(&self, _src: &str) -> Option<MediaRequest> {
            Some(MediaRequest { url: "file:///etc/passwd".into(), headers: Default::default() })
        }
    }

    #[test]
    fn every_src_passes_the_media_policy() {
        assert!(allowed_request(&Plain, "file:///etc/passwd").is_none());
        assert!(allowed_request(&Plain, "/etc/passwd").is_none());
        assert!(allowed_request(&Plain, "javascript:alert(1)").is_none());
        assert!(allowed_request(&Plain, "https://exponential.at/a.png").is_some());
        assert!(allowed_request(&Plain, "data:image/png;base64,iVBORw0KGgo=").is_some());
        assert!(allowed_request(&Files, "file:///tmp/a.png").is_some());
        assert!(allowed_request(&Widening, "https://x.test/a.png").is_none());
    }

    #[test]
    fn a_header_over_the_pixel_cap_is_refused_before_decoding() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&40_000u32.to_be_bytes());
        png.extend_from_slice(&30_000u32.to_be_bytes());
        let b64 = crate::paint::natives::base64_encode(&png);
        let err = fetch(&MediaKey { url: format!("data:image/png;base64,{b64}"), headers: vec![], ..Default::default() }).unwrap_err();
        assert!(err.contains("40000×30000"), "{err}");
    }

    #[test]
    fn a_file_over_the_byte_cap_is_refused_unread() {
        let dir = std::env::temp_dir().join(format!("xui-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.bin");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MEDIA_MAX_BYTES + 1).unwrap();
        let url = url::Url::from_file_path(&path).unwrap().to_string();
        let err = fetch(&MediaKey { url, headers: vec![], ..Default::default() }).unwrap_err();
        assert!(err.contains("bytes"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // VAPP-103: Video / AudioPlayer hand-off.

    struct Signing;
    impl HostPlugin for Signing {
        fn media_options(&self) -> MediaOptions {
            let headers = [("Authorization".to_string(), "Bearer t0k".to_string())].into_iter().collect();
            MediaOptions { rules: Some(vec![exponential_ui::host::MediaRule { prefix: "https://files.example/".into(), headers }]), ..Default::default() }
        }
    }

    /// Records what reaches the system player.
    #[derive(Default, Clone)]
    struct Opener(std::rc::Rc<std::cell::RefCell<Vec<String>>>);
    impl HostPlugin for Opener {
        fn open_url(&self, url: &str, _cx: &mut App) {
            self.0.borrow_mut().push(format!("url {url}"));
        }
        fn open_media_file(&self, path: &std::path::Path, _cx: &mut App) {
            self.0.borrow_mut().push(format!("file {}", path.display()));
        }
    }

    #[test]
    fn a_src_streams_unless_it_carries_headers_or_data() {
        assert_eq!(handoff(&Plain, "https://cdn.example/a.mp4"), Some(Handoff::Open("https://cdn.example/a.mp4".into())));
        let Some(Handoff::Fetch(key)) = handoff(&Signing, "https://files.example/a.mp4") else { panic!("a signed src fetches") };
        assert_eq!(key.headers, vec![("Authorization".to_string(), "Bearer t0k".to_string())]);
        assert!(matches!(handoff(&Plain, "data:audio/wav;base64,UklGRg=="), Some(Handoff::Fetch(_))));
        assert_eq!(handoff(&Files, "file:///tmp/a.mp3"), Some(Handoff::File("/tmp/a.mp3".into())));
        // Denied: nothing loads, nothing opens.
        for src in ["", "file:///tmp/a.mp3", "/a.mp4", "javascript:alert(1)", "ftp://x.example/a.mp4"] {
            assert_eq!(handoff(&Plain, src), None, "{src}");
        }
        assert_eq!(handoff(&Widening, "https://x.test/a.mp4"), None);
    }

    #[test]
    fn a_fetched_src_lands_in_a_file_named_for_its_type() {
        let path = fetch_to_file(&MediaKey { url: "data:audio/wav;base64,UklGRg==".into(), headers: vec![], ..Default::default() }, "m4a").unwrap();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("wav"));
        assert_eq!(std::fs::read(&path).unwrap(), b"RIFF");
        std::fs::remove_file(&path).ok();
        assert_eq!(extension(None, "https://x.example/clip.MOV?sig=1", "mp4"), "mov");
        assert_eq!(extension(Some("application/octet-stream"), "https://x.example/api/attachments/7", "m4a"), "m4a");
        // Over the byte cap: refused before decoding, no file.
        let big = format!("data:video/mp4;base64,{}", "A".repeat((MEDIA_MAX_BYTES / 3 * 4 + 8) as usize));
        assert!(fetch_to_file(&MediaKey { url: big, headers: vec![], ..Default::default() }, "mp4").unwrap_err().contains("bytes"));
    }

    /// VAPP-103 rfix: a hand-off file carries an audio/video extension only
    /// (the system opener picks the app by it), a `file:` src too, and a
    /// directory or bundle is never opened.
    #[test]
    fn hand_off_files_carry_media_extensions_only() {
        assert_eq!(extension(None, "https://x.example/evil.exe", "mp4"), "mp4");
        assert_eq!(extension(None, "https://x.example/run.command?x=1", "m4a"), "m4a");
        assert_eq!(extension(None, "https://x.example/Tool.app", "app"), "mp4", "a fallback outside the list is mp4");
        assert_eq!(extension(Some("application/x-msdownload"), "https://x.example/a.exe", "mp4"), "mp4");
        assert_eq!(extension(Some("audio/flac"), "https://x.example/a.exe", "mp4"), "flac");
        for src in ["file:///tmp/evil.exe", "file:///tmp/run.command", "file:///Applications/Tool.app", "file:///tmp/noext"] {
            assert_eq!(handoff(&Files, src), None, "{src}");
        }
        assert_eq!(handoff(&Files, "file:///tmp/A.MP4"), Some(Handoff::File("/tmp/A.MP4".into())));
    }

    #[gpui::test]
    fn a_directory_named_like_media_is_never_opened(cx: &mut gpui::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("xui-bundle-{}.mp4", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        struct FilesOpener(Opener);
        impl HostPlugin for FilesOpener {
            fn media_options(&self) -> MediaOptions {
                Files.media_options()
            }
            fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
                self.0.open_media_file(path, cx)
            }
        }
        let opener = Opener::default();
        let host: std::rc::Rc<dyn HostPlugin> = std::rc::Rc::new(FilesOpener(opener.clone()));
        let file = std::env::temp_dir().join(format!("xui-clip-{}.mp4", std::process::id()));
        std::fs::write(&file, b"clip").unwrap();
        let (dir_url, file_url) = (url::Url::from_directory_path(&dir).unwrap().to_string(), url::Url::from_file_path(&file).unwrap().to_string());
        cx.update(|cx| {
            play(host.clone(), dir_url.trim_end_matches('/'), "mp4", cx);
            play(host.clone(), &file_url, "mp4", cx);
        });
        assert_eq!(opener.0.borrow().clone(), vec![format!("file {}", file.display())]);
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&file).ok();
    }

    /// Redirects re-check the HOST's policy, never downgrade, and carry only
    /// the headers the rules give the NEW url.
    #[test]
    fn redirect_hops_pass_the_hosts_policy() {
        let rules = vec![exponential_ui::host::MediaRule { prefix: "https://files.example/".into(), headers: [("x-api-key".to_string(), "k".to_string())].into_iter().collect() }];
        let options = MediaOptions { hosts: Some(vec!["files.example".into(), "*.cdn.example".into()]), rules: Some(rules), ..Default::default() };
        let same = redirect_hop("https://files.example/a", "/b", &options).unwrap();
        assert_eq!((same.url.as_str(), same.headers.get("x-api-key").map(String::as_str)), ("https://files.example/b", Some("k")));
        let cdn = redirect_hop("https://files.example/a", "https://eu.cdn.example/a", &options).unwrap();
        assert!(cdn.headers.is_empty(), "the rule header stays with its prefix: {:?}", cdn.headers);
        assert!(redirect_hop("https://files.example/a", "https://evil.example/a", &options).is_none(), "the hosts allowlist holds on every hop");
        assert!(redirect_hop("https://files.example/a", "http://files.example/a", &options).is_none(), "no https→http downgrade");
        assert!(redirect_hop("https://files.example/a", "file:///etc/passwd", &options).is_none());
    }

    /// An SVG's size meets the pixel cap at gpui's 2× raster; animated
    /// frames multiply; an unreadable raster is refused.
    #[test]
    fn svg_and_animated_images_meet_the_pixel_cap() {
        let data = |mime: &str, body: &[u8]| MediaKey { url: format!("data:{mime};base64,{}", crate::paint::natives::base64_encode(body)), ..Default::default() };
        assert!(fetch(&data("image/svg+xml", br#"<svg width="20000" height="20000"/>"#)).unwrap_err().contains("pixels"));
        assert!(fetch(&data("image/svg+xml", br#"<svg width="5000" height="5000"/>"#)).unwrap_err().contains("10000×10000"), "2x raster");
        assert!(fetch(&data("image/svg+xml", br#"<svg width="24" height="24"/>"#)).is_ok());
        assert!(fetch(&data("image/png", b"\x89PNG\r\n\x1a\n\0\0")).unwrap_err().contains("unreadable"));
    }

    /// A tiny HTTP server: `routes` = (path, status, extra headers, body).
    #[cfg(feature = "net")]
    fn serve(routes: Vec<(&'static str, u16, Vec<(&'static str, String)>, Vec<u8>)>) -> String {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).ok();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let mut stream = stream;
                match routes.iter().find(|r| r.0 == path) {
                    Some((_, status, headers, body)) => {
                        let mut head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
                        for (k, v) in headers {
                            head.push_str(&format!("{k}: {v}\r\n"));
                        }
                        head.push_str("\r\n");
                        stream.write_all(head.as_bytes()).ok();
                        stream.write_all(body).ok();
                    }
                    None => {
                        stream.write_all(b"HTTP/1.1 404 X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").ok();
                    }
                }
            }
        });
        base
    }

    /// VAPP-103 rfix: a Video / AudioPlayer fetch streams past the 20 MB
    /// image cap into its file; a redirect to a host the policy refuses
    /// fails the load.
    #[cfg(feature = "net")]
    #[test]
    fn a_media_hand_off_streams_past_the_image_byte_cap() {
        let big = vec![7u8; (MEDIA_MAX_BYTES + 1024) as usize];
        let base = serve(vec![
            ("/clip", 200, vec![("Content-Type", "video/mp4".to_string())], big.clone()),
            ("/moved", 302, vec![("Location", "http://localhost:1/clip".to_string())], vec![]),
        ]);
        let options = MediaOptions { hosts: Some(vec!["127.0.0.1".into()]), ..Default::default() };
        let key = |path: &str| MediaKey::new(&media_request(&format!("{base}{path}"), &options).unwrap(), &options);
        let path = fetch_to_file(&key("/clip"), "mp4").unwrap();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("mp4"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), big.len() as u64);
        std::fs::remove_file(&path).ok();
        assert!(fetch(&key("/clip")).unwrap_err().contains("bytes"), "an image keeps the byte cap");
        assert!(fetch_to_file(&key("/moved"), "mp4").unwrap_err().contains("redirect"), "localhost is not in the hosts list");
    }

    #[gpui::test]
    fn a_press_hands_only_a_policed_src_to_the_system_player(cx: &mut gpui::TestAppContext) {
        let opener = Opener::default();
        let host: std::rc::Rc<dyn HostPlugin> = std::rc::Rc::new(opener.clone());
        cx.update(|cx| {
            play(host.clone(), "https://cdn.example/a.mp4", "mp4", cx);
            play(host.clone(), "javascript:alert(1)", "mp4", cx);
            play(host.clone(), "file:///etc/passwd", "mp4", cx);
            play(host.clone(), "data:audio/wav;base64,UklGRg==", "m4a", cx);
        });
        cx.run_until_parked();
        let log = opener.0.borrow().clone();
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(log[0], "url https://cdn.example/a.mp4");
        assert!(log[1].starts_with("file ") && log[1].ends_with(".wav"), "{log:?}");
        std::fs::remove_file(log[1].trim_start_matches("file ")).ok();
    }

}
