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

#[cfg(feature = "net")]
fn fetch_http(key: &MediaKey) -> Result<Vec<u8>, String> {
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
    let mut out = Vec::new();
    res.take(MEDIA_MAX_BYTES + 1).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(not(feature = "net"))]
fn fetch_http(key: &MediaKey) -> Result<Vec<u8>, String> {
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
}
