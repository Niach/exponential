//! VAPP-91: the media loader. `Image`, `Avatar` and `Video` posters load
//! through the host's [`crate::host::HostPlugin::media_request`]: the
//! absolute url plus the headers its media rules add (auth for
//! `/api/attachments`, …), fetched by this crate — gpui's own `img(url)`
//! goes through the app's `http_client`, which is a `NullHttpClient` unless
//! the app installs one, and it cannot carry per-request headers.
//!
//! A host without `media_request` keeps gpui's `img(resolve_url(src))`.

use std::sync::Arc;

use exponential_ui::host::MediaRequest;
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

fn fetch(key: &MediaKey) -> Result<Vec<u8>, String> {
    if let Some(path) = key.url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| e.to_string());
    }
    #[cfg(feature = "net")]
    {
        let client = reqwest::blocking::Client::new();
        let mut req = client.get(&key.url);
        for (k, v) in &key.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = req.send().map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("HTTP {} for {}", res.status().as_u16(), key.url));
        }
        res.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "net"))]
    {
        Err(format!("{}: http(s) media needs the `net` feature", key.url))
    }
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

/// The source `img()` paints for a media `src`: the host's media request
/// (fetched with its headers by [`MediaLoader`]) when it has one, else
/// gpui's own loader on `resolve_url(src)`.
pub fn image_source(host: &dyn HostPlugin, src: &str) -> ImageSource {
    match host.media_request(src) {
        Some(req) => {
            let key = MediaKey::from(&req);
            ImageSource::Custom(Arc::new(move |window: &mut Window, cx: &mut App| window.use_asset::<MediaLoader>(&key, cx)))
        }
        None => ImageSource::from(SharedString::from(host.resolve_url(src))),
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
}
