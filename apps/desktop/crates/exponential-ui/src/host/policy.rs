//! The policy rules every host applies before a client function, a URL open
//! or a media load (`catalog/host.json` functions / urls / media). Pure;
//! `fixtures/host-policy.json` locks them on every platform.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use super::contract::{DEFAULT_MEDIA_SCHEMES, DEFAULT_URL_SCHEMES};
use super::js_trim;

/// The catalog's built-in client functions: the basic 14 + round 1's 15
/// core ones (`FUNCTION_NAMES`).
pub const BUILTIN_FUNCTIONS: &[&str] = crate::generated::catalog::FUNCTION_NAMES;

pub fn is_builtin_function(name: &str) -> bool {
    BUILTIN_FUNCTIONS.contains(&name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionDecision {
    Allow,
    Ask,
    Deny,
    NotFound,
}

impl FunctionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            FunctionDecision::Allow => "allow",
            FunctionDecision::Ask => "ask",
            FunctionDecision::Deny => "deny",
            FunctionDecision::NotFound => "not_found",
        }
    }

    pub fn parse(s: &str) -> Option<FunctionDecision> {
        Some(match s {
            "allow" => FunctionDecision::Allow,
            "ask" => FunctionDecision::Ask,
            "deny" => FunctionDecision::Deny,
            "not_found" => FunctionDecision::NotFound,
            _ => return None,
        })
    }

    fn rank(self) -> u8 {
        match self {
            FunctionDecision::Allow => 0,
            FunctionDecision::Ask => 1,
            FunctionDecision::Deny => 2,
            FunctionDecision::NotFound => 3,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    /// For a registered name no list matches; default `allow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<FunctionDecision>,
}

/// `harness.*` matches `harness.toast`; anything else is exact.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// The gate: built-ins always run; an unregistered name is `not_found`; then
/// deny wins, then allow, then ask, then the policy's default.
pub fn decide_function(policy: Option<&FunctionPolicy>, name: &str, registered: bool) -> FunctionDecision {
    if is_builtin_function(name) {
        return FunctionDecision::Allow;
    }
    if !registered {
        return FunctionDecision::NotFound;
    }
    let empty = FunctionPolicy::default();
    let p = policy.unwrap_or(&empty);
    let any = |list: &Option<Vec<String>>| list.iter().flatten().any(|pattern| matches_pattern(pattern, name));
    if any(&p.deny) {
        return FunctionDecision::Deny;
    }
    if any(&p.allow) {
        return FunctionDecision::Allow;
    }
    if any(&p.ask) {
        return FunctionDecision::Ask;
    }
    p.default.unwrap_or(FunctionDecision::Allow)
}

/// Two policies stacked (a package's allowlist under the host's own): the
/// stricter decision wins.
pub fn combine_decisions(a: FunctionDecision, b: FunctionDecision) -> FunctionDecision {
    if a.rank() >= b.rank() {
        a
    } else {
        b
    }
}

/// A declarative package's `functions` list as a policy: listed = allow,
/// anything else = deny.
pub fn package_policy(functions: Option<&[String]>) -> FunctionPolicy {
    FunctionPolicy { allow: Some(functions.map(<[String]>::to_vec).unwrap_or_default()), ask: None, deny: None, default: Some(FunctionDecision::Deny) }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UrlPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemes: Option<Vec<String>>,
    /// http(s) hosts allowed: exact or `*.example.com`; unset = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<Vec<String>>,
    #[serde(default, rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlDecision {
    pub allowed: bool,
    /// The absolute url when it parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// `invalid` | `scheme` | `host`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `new URL(url)`, else `new URL(url, baseUrl)` (WHATWG both).
fn absolute(url: &str, base_url: Option<&str>) -> Option<Url> {
    if let Ok(u) = Url::parse(url) {
        return Some(u);
    }
    let base = Url::parse(base_url?).ok()?;
    base.join(url).ok()
}

fn host_matches(pattern: &str, host: &str) -> bool {
    let p = pattern.to_lowercase();
    match p.strip_prefix('*') {
        Some(suffix) if p.starts_with("*.") => host.ends_with(suffix) && host.len() > suffix.len(),
        _ => host == p,
    }
}

pub fn decide_url(policy: Option<&UrlPolicy>, url: &str) -> UrlDecision {
    let Some(parsed) = absolute(js_trim(url), policy.and_then(|p| p.base_url.as_deref())) else {
        return UrlDecision { allowed: false, url: None, reason: Some("invalid".into()) };
    };
    let scheme = parsed.scheme().to_lowercase();
    let href = parsed.as_str().to_string();
    let allowed_scheme = match policy.and_then(|p| p.schemes.as_ref()) {
        Some(list) => list.contains(&scheme),
        None => DEFAULT_URL_SCHEMES.contains(&scheme.as_str()),
    };
    if !allowed_scheme {
        return UrlDecision { allowed: false, url: Some(href), reason: Some("scheme".into()) };
    }
    if let Some(hosts) = policy.and_then(|p| p.hosts.as_ref()) {
        if scheme == "http" || scheme == "https" {
            let host = parsed.host_str().unwrap_or("").to_lowercase();
            if !hosts.iter().any(|h| host_matches(h, &host)) {
                return UrlDecision { allowed: false, url: Some(href), reason: Some("host".into()) };
            }
        }
    }
    UrlDecision { allowed: true, url: Some(href), reason: None }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaRule {
    pub prefix: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaOptions {
    #[serde(default, rename = "baseUrl", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<MediaRule>>,
    /// The schemes a src may use; default [`DEFAULT_MEDIA_SCHEMES`] (https,
    /// http, data). `file` only when listed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemes: Option<Vec<String>>,
    /// http(s) hosts media may load from: exact or `*.example.com`; unset = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRequest {
    pub url: String,
    /// Sorted by name.
    pub headers: BTreeMap<String, String>,
}

/// The image / media loader's request: the absolute url plus the headers of
/// every rule whose prefix it starts with (later rules win per header).
/// `None` when the url does not resolve or the media policy (schemes, hosts)
/// denies it: nothing loads.
pub fn media_request(url: &str, options: &MediaOptions) -> Option<MediaRequest> {
    let policy = UrlPolicy {
        schemes: Some(options.schemes.clone().unwrap_or_else(|| DEFAULT_MEDIA_SCHEMES.iter().map(|s| s.to_string()).collect())),
        hosts: options.hosts.clone(),
        base_url: options.base_url.clone(),
    };
    let decision = decide_url(Some(&policy), url);
    if !decision.allowed {
        return None;
    }
    let href = decision.url?;
    let mut headers = BTreeMap::new();
    for rule in options.rules.iter().flatten() {
        if href.starts_with(&rule.prefix) {
            headers.extend(rule.headers.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
    Some(MediaRequest { url: href, headers })
}

/// The href a renderer may navigate to (Link, markdown links, FileUpload
/// file urls), or `None` when the URL policy denies it.
pub fn safe_href(policy: Option<&UrlPolicy>, url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let d = decide_url(policy, url);
    if d.allowed {
        d.url
    } else {
        None
    }
}

/// Width × height from an image's header (PNG, JPEG, GIF, WebP, BMP) or an
/// SVG's root `width`/`height`/`viewBox` without decoding it — what a
/// loader checks against [`super::contract::MEDIA_MAX_PIXELS`] BEFORE
/// decoding. `None` = not a header this reads (truncated bytes, another
/// format). VAPP-103 rfix: an SVG has a size ([`svg_dimensions`]).
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if is_svg(bytes) {
        return svg_dimensions(bytes);
    }
    let be16 = |i: usize| bytes.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32);
    let le16 = |i: usize| bytes.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32);
    let be32 = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le32 = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let le24 = |i: usize| bytes.get(i..i + 3).map(|b| u32::from_le_bytes([b[0], b[1], b[2], 0]));
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some((le16(6)?, le16(8)?));
    }
    if bytes.starts_with(b"BM") {
        let h = le32(22)? as i32;
        return Some((le32(18)?, h.unsigned_abs()));
    }
    if bytes.len() >= 30 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return match &bytes[12..16] {
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            b"VP8L" => {
                let b = le32(21)?;
                Some(((b & 0x3fff) + 1, ((b >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    if bytes.starts_with(b"\xff\xd8") {
        let mut i = 2;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if marker == 0xff {
                i += 1;
                continue;
            }
            if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                i += 2;
                continue;
            }
            let len = be16(i + 2)? as usize;
            // SOF0..SOF15 except DHT (c4), JPG (c8), DAC (cc).
            if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc {
                return Some((be16(i + 7)?, be16(i + 5)?));
            }
            i += 2 + len;
        }
        return None;
    }
    None
}

/// An SVG by its first non-blank bytes (`<svg` or `<?xml`), like the
/// loaders sniff it.
fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(256)];
    let start = head.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(head.len());
    let head = &head[start..];
    let lower = |p: &[u8]| head.len() >= p.len() && head[..p.len()].eq_ignore_ascii_case(p);
    lower(b"<svg") || lower(b"<?xml")
}

/// The size an SVG rasterizes at: its root's `width`/`height` (px, or
/// pt/pc/in/cm/mm/em/ex converted at 96 dpi and a 16 px em; `%` = unset);
/// a missing one follows the `viewBox` aspect, both missing = the
/// `viewBox` size, no `viewBox` = 100 (usvg's default). `None` = no `<svg`
/// root tag.
pub fn svg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let at = bytes.windows(4).position(|w| w == b"<svg")?;
    let end = bytes[at..].iter().position(|b| *b == b'>').map_or(bytes.len(), |e| at + e);
    let tag = String::from_utf8_lossy(&bytes[at + 4..end]);
    let (mut width, mut height, mut view_box) = (None, None, None);
    let t = tag.as_bytes();
    let mut i = 0;
    while i < t.len() {
        if !(t[i].is_ascii_alphabetic() || t[i] == b'_' || t[i] == b':') || i == 0 || !t[i - 1].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let name_start = i;
        while i < t.len() && (t[i].is_ascii_alphanumeric() || matches!(t[i], b'_' | b':' | b'-' | b'.')) {
            i += 1;
        }
        let name = &tag[name_start..i];
        while i < t.len() && t[i].is_ascii_whitespace() {
            i += 1;
        }
        if t.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < t.len() && t[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(&q) = t.get(i).filter(|q| **q == b'"' || **q == b'\'') else { continue };
        let value_start = i + 1;
        let value_end = t[value_start..].iter().position(|b| *b == q).map_or(t.len(), |e| value_start + e);
        let value = &tag[value_start..value_end];
        match name {
            "width" => width = svg_length(value),
            "height" => height = svg_length(value),
            "viewBox" => view_box = svg_view_box(value),
            _ => {}
        }
        i = value_end + 1;
    }
    let (w, h) = match (width, height, view_box) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((vw, vh))) => (w, w * vh / vw),
        (None, Some(h), Some((vw, vh))) => (h * vw / vh, h),
        (Some(w), None, None) => (w, 100.0),
        (None, Some(h), None) => (100.0, h),
        (None, None, Some((vw, vh))) => (vw, vh),
        (None, None, None) => (100.0, 100.0),
    };
    let px = |v: f64| if v.is_finite() { v.ceil().clamp(0.0, u32::MAX as f64) as u32 } else { u32::MAX };
    Some((px(w), px(h)))
}

/// An SVG length in px (`None` = a percentage, unknown unit, negative or
/// not a number).
fn svg_length(value: &str) -> Option<f64> {
    let v = value.trim();
    let split = v.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))).unwrap_or(v.len());
    // An exponent `e` followed by no digit belongs to the unit (`em`, `ex`).
    let (mut num, mut unit) = v.split_at(split);
    if let Some(stripped) = num.strip_suffix(['e', 'E']) {
        num = stripped;
        unit = &v[num.len()..];
    }
    let n: f64 = num.parse().ok()?;
    let scale = match unit.trim() {
        "" | "px" => 1.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "in" => 96.0,
        "cm" => 96.0 / 2.54,
        "mm" => 96.0 / 25.4,
        "em" => 16.0,
        "ex" => 8.0,
        _ => return None,
    };
    (n.is_finite() && n >= 0.0).then_some(n * scale)
}

/// A `viewBox`'s width and height (`None` unless both are > 0).
fn svg_view_box(value: &str) -> Option<(f64, f64)> {
    let parts: Vec<f64> = value.split(|c: char| c.is_ascii_whitespace() || c == ',').filter(|p| !p.is_empty()).map(str::parse).collect::<Result<_, _>>().ok()?;
    match parts[..] {
        [_, _, w, h] if w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() => Some((w, h)),
        _ => None,
    }
}

/// The frames an image decodes into: a GIF's image descriptors, an APNG's
/// `acTL` count, an animated WebP's `ANMF` chunks; 1 otherwise (and for a
/// truncated header, the frames found).
pub fn image_frames(bytes: &[u8]) -> u32 {
    let le32 = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let be32 = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let frames = if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let skip_blocks = |mut i: usize| -> usize {
            while let Some(&n) = bytes.get(i) {
                i += 1 + n as usize;
                if n == 0 {
                    break;
                }
            }
            i
        };
        let table = |packed: u8| if packed & 0x80 != 0 { 3usize << ((packed & 7) + 1) } else { 0 };
        let mut i = 13 + bytes.get(10).map_or(0, |p| table(*p));
        let mut count = 0u32;
        loop {
            match bytes.get(i) {
                Some(0x2c) => {
                    count = count.saturating_add(1);
                    let Some(&packed) = bytes.get(i + 9) else { break };
                    i = skip_blocks(i + 10 + table(packed) + 1);
                }
                Some(0x21) => i = skip_blocks(i + 2),
                _ => break,
            }
        }
        count
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut i = 8;
        let mut count = 1;
        while let (Some(len), Some(kind)) = (be32(i), bytes.get(i + 4..i + 8)) {
            if kind == b"acTL" {
                count = be32(i + 8).map_or(1, |n| n.min(u32::MAX as usize) as u32);
                break;
            }
            if kind == b"IDAT" {
                break;
            }
            i = match i.checked_add(12 + len) {
                Some(next) => next,
                None => break,
            };
        }
        count
    } else if bytes.len() >= 21 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" && &bytes[12..16] == b"VP8X" && bytes[20] & 0x02 != 0 {
        let mut i = 12;
        let mut count = 0u32;
        while let (Some(kind), Some(size)) = (bytes.get(i..i + 4), le32(i + 4)) {
            if kind == b"ANMF" {
                count = count.saturating_add(1);
            }
            i = match i.checked_add(8 + size + (size & 1)) {
                Some(next) => next,
                None => break,
            };
        }
        count
    } else {
        1
    };
    frames.max(1)
}

/// What a whole image's bytes say before any decode (VAPP-103): no known
/// image format, a raster (PNG, JPEG, GIF, WebP, BMP) or SVG whose size
/// cannot be read, or its size and [`image_frames`]. Every renderer's
/// pixel cap reads this ONE answer (the natives through the FFI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageHeader {
    Unknown,
    Unreadable,
    Size { width: u32, height: u32, frames: u32 },
}

pub fn image_header(bytes: &[u8]) -> ImageHeader {
    let raster = bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"BM")
        || bytes.starts_with(b"\xff\xd8")
        || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP");
    if !raster && !is_svg(bytes) {
        return ImageHeader::Unknown;
    }
    match image_dimensions(bytes) {
        Some((width, height)) => ImageHeader::Size { width, height, frames: image_frames(bytes) },
        None => ImageHeader::Unreadable,
    }
}

/// VAPP-103 rfix: the pixel cap on a whole image before it decodes, under
/// `max_bytes` / `max_pixels`: a raster format or an SVG whose size cannot
/// be read is REFUSED (`media: image dimensions unreadable`), and width ×
/// height × [`image_frames`] must fit `max_pixels` (`media is W×H × N
/// frames, over … pixels`). Bytes of no known image format pass (the
/// decoder refuses them).
pub fn media_image_check(bytes: &[u8], max_bytes: u64, max_pixels: u64) -> Result<(), String> {
    if bytes.len() as u64 > max_bytes {
        return Err(format!("media is over {max_bytes} bytes"));
    }
    match image_header(bytes) {
        ImageHeader::Unknown => Ok(()),
        ImageHeader::Unreadable => Err("media: image dimensions unreadable".into()),
        ImageHeader::Size { width: w, height: h, frames } if frames <= 1 => {
            if w as u128 * h as u128 > max_pixels as u128 {
                return Err(format!("media is {w}×{h}, over {max_pixels} pixels"));
            }
            Ok(())
        }
        ImageHeader::Size { width: w, height: h, frames } => {
            if w as u128 * h as u128 * frames as u128 > max_pixels as u128 {
                return Err(format!("media is {w}×{h} × {frames} frames, over {max_pixels} pixels"));
            }
            Ok(())
        }
    }
}

/// [`media_image_check`] under the contract's `media.limits`.
pub fn media_image_within_limits(bytes: &[u8]) -> Result<(), String> {
    use super::contract::{MEDIA_MAX_BYTES, MEDIA_MAX_PIXELS};
    media_image_check(bytes, MEDIA_MAX_BYTES, MEDIA_MAX_PIXELS)
}

/// Whether a load fits `media.limits`: `bytes` read so far (or the
/// Content-Length) and, once the header is in, the image's pixels.
pub fn media_within_limits(bytes: u64, dimensions: Option<(u32, u32)>) -> Result<(), String> {
    use super::contract::{MEDIA_MAX_BYTES, MEDIA_MAX_PIXELS};
    if bytes > MEDIA_MAX_BYTES {
        return Err(format!("media is over {MEDIA_MAX_BYTES} bytes"));
    }
    if let Some((w, h)) = dimensions {
        if w as u64 * h as u64 > MEDIA_MAX_PIXELS {
            return Err(format!("media is {w}×{h}, over {MEDIA_MAX_PIXELS} pixels"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VAPP-103 rfix: SVGs have a size, frames multiply the pixels, and a
    /// raster whose dimensions cannot be read is refused (it failed open).
    #[test]
    fn svg_sizes_animation_frames_and_unreadable_rasters_meet_the_pixel_cap() {
        assert_eq!(svg_dimensions(br#"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>"#), Some((20_000, 20_000)));
        assert!(media_image_within_limits(br#"<svg width="20000" height="20000"></svg>"#).unwrap_err().contains("20000×20000"));
        assert_eq!(svg_dimensions(br#"<?xml version="1.0"?><svg viewBox="0 0 300 150"/>"#), Some((300, 150)));
        assert_eq!(svg_dimensions(br#"<svg width="100000" viewBox="0 0 1 1">"#), Some((100_000, 100_000)), "a missing height follows the viewBox aspect");
        assert_eq!(svg_dimensions(br#"<svg width="2in" height="10em">"#), Some((192, 160)));
        assert_eq!(svg_dimensions(br#"<svg width="50%">"#), Some((100, 100)));
        assert_eq!(svg_dimensions(b"<svg>"), Some((100, 100)));
        assert!(media_image_within_limits(br#"<svg width="24" height="24"/>"#).is_ok());
        assert!(media_image_within_limits(b"<?xml version='1.0'?><note/>").unwrap_err().contains("unreadable"));
        // A 1000×1000 GIF with 40 frames: 40 Mpx decoded.
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&1000u16.to_le_bytes());
        gif.extend_from_slice(&1000u16.to_le_bytes());
        gif.extend_from_slice(&[0, 0, 0]);
        for _ in 0..40 {
            gif.extend_from_slice(&[0x21, 0xf9, 4, 0, 0, 0, 0, 0]);
            gif.extend_from_slice(&[0x2c, 0, 0, 0, 0, 0xe8, 3, 0xe8, 3, 0, 2, 2, 0x4c, 1, 0]);
        }
        gif.push(0x3b);
        assert_eq!(image_frames(&gif), 40);
        assert!(media_image_within_limits(&gif).unwrap_err().contains("40 frames"));
        assert_eq!(image_frames(&gif[..gif.len() / 2]), 20, "a truncated GIF counts the frames it holds");
        // An APNG announcing 100 frames of 1000×1000.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&1000u32.to_be_bytes());
        png.extend_from_slice(&1000u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        png.extend_from_slice(b"\0\0\0\x08acTL");
        png.extend_from_slice(&100u32.to_be_bytes());
        png.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(image_frames(&png), 100);
        assert!(media_image_within_limits(&png).is_err());
        // An animated WebP canvas of 2000×2000 with 10 ANMF chunks.
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x02\0\0\0".to_vec();
        webp.extend_from_slice(&1999u32.to_le_bytes()[..3]);
        webp.extend_from_slice(&1999u32.to_le_bytes()[..3]);
        for _ in 0..10 {
            webp.extend_from_slice(b"ANMF\x02\0\0\0ab");
        }
        assert_eq!(image_dimensions(&webp), Some((2000, 2000)));
        assert_eq!(image_frames(&webp), 10);
        assert!(media_image_within_limits(&webp).unwrap_err().contains("10 frames"));
        // A raster header too short to read is refused, not passed.
        assert!(media_image_within_limits(b"\x89PNG\r\n\x1a\n\0\0").unwrap_err().contains("unreadable"));
        assert!(media_image_within_limits(b"\xff\xd8\xff\xe0").unwrap_err().contains("unreadable"));
        assert!(media_image_within_limits(b"{\"not\": \"an image\"}").is_ok());
        assert!(media_image_within_limits(b"GIF89a\x10\0\x20\0").is_ok());
    }

    #[test]
    fn image_headers_give_dimensions_without_decoding() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&40_000u32.to_be_bytes());
        png.extend_from_slice(&30_000u32.to_be_bytes());
        assert_eq!(image_dimensions(&png), Some((40_000, 30_000)));
        assert!(media_within_limits(1024, image_dimensions(&png)).is_err());
        assert_eq!(image_dimensions(b"GIF89a\x10\0\x20\0"), Some((16, 32)));
        // JPEG: SOI, an APP0 segment, then SOF0 (height 0x0100, width 0x0200).
        let jpeg = b"\xff\xd8\xff\xe0\0\x04ab\xff\xc0\0\x11\x08\x01\x00\x02\x00\x03";
        assert_eq!(image_dimensions(jpeg), Some((512, 256)));
        assert_eq!(image_dimensions(b"<svg/>"), Some((100, 100)));
        assert!(media_within_limits(crate::host::MEDIA_MAX_BYTES + 1, None).is_err());
        assert!(media_within_limits(10, Some((100, 100))).is_ok());
    }

    #[test]
    fn hrefs_pass_the_url_policy() {
        assert_eq!(safe_href(None, "javascript:alert(1)"), None);
        assert_eq!(safe_href(None, "https://exponential.at/x").as_deref(), Some("https://exponential.at/x"));
        let hosts = UrlPolicy { hosts: Some(vec!["exponential.at".into()]), ..Default::default() };
        assert_eq!(safe_href(Some(&hosts), "https://evil.example/"), None);
    }
}
