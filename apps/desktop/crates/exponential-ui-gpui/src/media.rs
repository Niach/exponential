//! VAPP-91: the media loader. `Image`, `Avatar` and `Video` posters load
//! through the host's [`crate::host::HostPlugin::media_request`]: the
//! absolute url plus the headers its media rules add (auth for
//! `/api/attachments`, …), fetched by this crate — gpui's own `img(url)`
//! goes through the app's `http_client`, which is a `NullHttpClient` unless
//! the app installs one, and it cannot carry per-request headers.
//!
//! VAPP-103: every src passes the media policy (`catalog/host.json` media:
//! schemes, hosts; no local file unless the host lists `file`) and every
//! load the media limits: the whole request within `MEDIA_TIMEOUT_MS`, the
//! body (Content-Length up front, then as it streams) within
//! `MEDIA_MAX_BYTES`, the header's width × height within `MEDIA_MAX_PIXELS`
//! BEFORE any decode.

use std::sync::Arc;

use exponential_ui::host::{image_dimensions, media_request, media_within_limits, MediaOptions, MediaRequest, MEDIA_MAX_BYTES, MEDIA_TIMEOUT_MS};
use gpui::{App, Asset, Image, ImageCacheError, ImageFormat, ImageSource, RenderImage, SharedString, Window};

use crate::host::HostPlugin;

/// One media load: the url and the headers (sorted, so the asset key is
/// stable).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaKey {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl From<&MediaRequest> for MediaKey {
    fn from(r: &MediaRequest) -> Self {
        MediaKey { url: r.url.clone(), headers: r.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect() }
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
    media_within_limits(bytes.len() as u64, image_dimensions(&bytes))?;
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
    fetch_http_typed(key).map(|(bytes, _)| bytes)
}

/// The body of an http(s) request under the media limits, with its
/// `Content-Type`.
#[cfg(feature = "net")]
fn fetch_http_typed(key: &MediaKey) -> Result<(Vec<u8>, Option<String>), String> {
    use std::io::Read;
    use std::time::Duration;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(MEDIA_TIMEOUT_MS))
        // Every redirect hop passes the media policy again.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || media_request(attempt.url().as_str(), &MediaOptions::default()).is_none() {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&key.url);
    for (k, v) in &key.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let res = req.send().map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {} for {}", res.status().as_u16(), key.url));
    }
    if let Some(len) = res.content_length() {
        media_within_limits(len, None)?;
    }
    let mime = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|v| v.split(';').next().unwrap_or("").trim().to_ascii_lowercase());
    let mut out = Vec::new();
    res.take(MEDIA_MAX_BYTES + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok((out, mime))
}

#[cfg(not(feature = "net"))]
fn fetch_http_typed(key: &MediaKey) -> Result<(Vec<u8>, Option<String>), String> {
    Err(format!("{}: http(s) media needs the `net` feature", key.url))
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
    let key = MediaKey::from(&req);
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
    /// A `file:` src the host's media schemes list: its path, opened as is
    /// through [`HostPlugin::open_media_file`].
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
    match scheme.as_str() {
        "file" => url::Url::parse(&req.url).ok()?.to_file_path().ok().map(Handoff::File),
        "data" => Some(Handoff::Fetch(MediaKey::from(&req))),
        "http" | "https" if req.headers.is_empty() => Some(Handoff::Open(req.url)),
        "http" | "https" => Some(Handoff::Fetch(MediaKey::from(&req))),
        _ => None,
    }
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

/// The file extension the system opener needs: the MIME type's, else the
/// url path's, else `fallback`.
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
            if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return ext.to_ascii_lowercase();
            }
        }
    }
    fallback.to_string()
}

/// Fetch a policed request under the media limits (timeout, Content-Length,
/// the body as it streams) into a fresh temporary file named with the
/// media's extension (`fallback` when neither the type nor the url says).
pub fn fetch_to_file(key: &MediaKey, fallback: &str) -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (bytes, mime) = if key.url.starts_with("data:") { data_payload(&key.url)? } else { fetch_http_typed(key)? };
    media_within_limits(bytes.len() as u64, None)?;
    let dir = std::env::temp_dir().join("exponential-ui-media");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{}-{nanos}-{}.{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), extension(mime.as_deref(), &key.url, fallback));
    let path = dir.join(name);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A Video / AudioPlayer press: hand `src` to the system player through
/// [`handoff`] (a denied src does nothing). A fetch runs on the background
/// executor; its file then goes to [`HostPlugin::open_media_file`].
/// `fallback` = the extension when the media names none (`mp4`, `m4a`).
pub fn play(host: std::rc::Rc<dyn HostPlugin>, src: &str, fallback: &'static str, cx: &mut App) {
    match handoff(host.as_ref(), src) {
        None => {}
        Some(Handoff::Open(url)) => host.open_url(&url, cx),
        Some(Handoff::File(path)) => host.open_media_file(&path, cx),
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
        let err = fetch(&MediaKey { url: format!("data:image/png;base64,{b64}"), headers: vec![] }).unwrap_err();
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
        let err = fetch(&MediaKey { url, headers: vec![] }).unwrap_err();
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
        let path = fetch_to_file(&MediaKey { url: "data:audio/wav;base64,UklGRg==".into(), headers: vec![] }, "m4a").unwrap();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("wav"));
        assert_eq!(std::fs::read(&path).unwrap(), b"RIFF");
        std::fs::remove_file(&path).ok();
        assert_eq!(extension(None, "https://x.example/clip.MOV?sig=1", "mp4"), "mov");
        assert_eq!(extension(Some("application/octet-stream"), "https://x.example/api/attachments/7", "m4a"), "m4a");
        // Over the byte cap: refused before decoding, no file.
        let big = format!("data:video/mp4;base64,{}", "A".repeat((MEDIA_MAX_BYTES / 3 * 4 + 8) as usize));
        assert!(fetch_to_file(&MediaKey { url: big, headers: vec![] }, "mp4").unwrap_err().contains("bytes"));
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
