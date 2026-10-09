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

/// Width × height from an image's header (PNG, JPEG, GIF, WebP, BMP)
/// without decoding it — what a loader checks against
/// [`super::contract::MEDIA_MAX_PIXELS`] BEFORE decoding. `None` = not a
/// header this reads (SVG, truncated bytes).
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
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
        assert_eq!(image_dimensions(b"<svg/>"), None);
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
