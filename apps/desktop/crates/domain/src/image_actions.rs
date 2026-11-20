//! #EXP-1926 / #EXP-1947 (`packages/domain-contract/fixtures/image-actions.json`,
//! TS reference `src/image-actions.ts`): Copy image on every content picture
//! and which picture sources load at all. The desktop app and eui-gpui hosts
//! (and so eDE) read these; the tests replay `menuCases`, `pressCases`,
//! `encodingCases`, `shareNameCases` and `sourceCases`.

/// One input kind (`inputs`): the desktop and eDE only have `Pointer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageInput {
    Pointer,
    Touch,
}

/// One image menu item (`menus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMenuItem {
    CopyImage,
    Share,
}

/// The image menu's items in order: pointer = Copy image, touch = Copy image
/// then Share (only when the platform can share a file).
pub fn image_menu(input: ImageInput, can_share_file: bool) -> Vec<ImageMenuItem> {
    match input {
        ImageInput::Pointer => vec![ImageMenuItem::CopyImage],
        ImageInput::Touch if can_share_file => vec![ImageMenuItem::CopyImage, ImageMenuItem::Share],
        ImageInput::Touch => vec![ImageMenuItem::CopyImage],
    }
}

/// Which menu a press opens (`pressCases`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressMenu {
    /// The picture's own image menu.
    Image,
    /// The element around it (a message bubble's, a row's).
    Owner,
    /// Nothing (the platform default).
    None,
}

/// The image menu on a rendered picture, else the owner's, else none. Never
/// merged.
pub fn press_menu(on_image: bool, image_ready: bool, owner_menu: bool) -> PressMenu {
    if on_image && image_ready {
        PressMenu::Image
    } else if owner_menu {
        PressMenu::Owner
    } else {
        PressMenu::None
    }
}

/// `copyFormat.pngSignature`.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// How a picture's bytes reach the clipboard (`encodingCases`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyEncoding {
    /// PNG bytes go as they are.
    Passthrough,
    /// Anything else is decoded and re-encoded to PNG (an animation = its
    /// first frame).
    Reencode,
}

pub fn copy_encoding(head: &[u8]) -> CopyEncoding {
    if head.starts_with(&PNG_SIGNATURE) {
        CopyEncoding::Passthrough
    } else {
        CopyEncoding::Reencode
    }
}

/// The shared PNG's file name: the last extension becomes `.png`, path
/// separators become `-`, an empty stem (or no name) = `image.png`.
pub fn share_file_name(name: Option<&str>) -> String {
    let clean: String = name.unwrap_or("").trim().chars().map(|c| if c == '/' || c == '\\' { '-' } else { c }).collect();
    let stem = match clean.rfind('.') {
        Some(dot) if dot > 0 => &clean[..dot],
        _ => clean.as_str(),
    }
    .trim();
    format!("{}.png", if stem.is_empty() { "image" } else { stem })
}

/// The src a copy fetches: the thumbnail width parameter `w` dropped from
/// the query (the ORIGINAL, never a thumbnail); any other parameter and the
/// fragment stay.
pub fn original_src(src: &str) -> String {
    if starts_with_ci(src, "data:") {
        return src.to_string();
    }
    let (head, fragment) = match src.find('#') {
        Some(i) => (&src[..i], &src[i..]),
        None => (src, ""),
    };
    let Some(q) = head.find('?') else { return src.to_string() };
    let kept: Vec<&str> = head[q + 1..].split('&').filter(|p| !p.is_empty() && *p != "w" && !p.starts_with("w=")).collect();
    let mut out = head[..q].to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    out.push_str(fragment);
    out
}

/// What a picture's src renders as (`imageSource`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// Ours, data or local: it loads.
    Load,
    /// Another website: an ordinary link in the picture's place, never
    /// fetched.
    Link { href: String, text: String },
    /// Anything else: the alt as plain text (nothing when empty).
    Text(String),
}

const ATTACHMENT_PATH: &str = "/api/attachments/";
const LOCAL_SCHEMES: [&str; 2] = ["blob:", "app-file:"];
const LINK_SCHEMES: [&str; 2] = ["https:", "http:"];
const DENIED_DATA_MIMES: [&str; 1] = ["image/svg+xml"];

/// `s` starts with the ASCII `prefix`, ASCII case-insensitively.
fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn edge_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0c' | '\r')
}

fn not_url(s: &str) -> bool {
    s.chars().any(|c| c <= ' ' || c == '\x7f')
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36 && b.iter().enumerate().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { *c == b'-' } else { c.is_ascii_hexdigit() })
}

/// The path exactly `/api/attachments/<uuid>`, then nothing or a `?query`
/// and/or `#fragment`.
fn is_attachment_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(ATTACHMENT_PATH) else { return false };
    if rest.len() < 36 || !rest.is_char_boundary(36) || !is_uuid(&rest[..36]) {
        return false;
    }
    let tail = &rest[36..];
    tail.is_empty() || tail.starts_with('?') || tail.starts_with('#')
}

fn is_loadable_data(lower: &str) -> bool {
    let Some(comma) = lower.find(',') else { return false };
    let mime = lower[5..comma].split(';').next().unwrap_or("");
    mime.len() > "image/".len() && mime.starts_with("image/") && !DENIED_DATA_MIMES.contains(&mime)
}

/// The host of a `scheme://` or `//` url for the link label: lowercased,
/// userinfo and port dropped, IPv6 brackets kept; empty = no host.
fn link_host(after_slashes: &str) -> String {
    let end = after_slashes.find(['/', '?', '#', '\\']).unwrap_or(after_slashes.len());
    let mut authority = &after_slashes[..end];
    if let Some(at) = authority.rfind('@') {
        authority = &authority[at + 1..];
    }
    let close = authority.rfind(']');
    if let Some(colon) = authority.rfind(':') {
        if close.is_none_or(|close| colon > close) {
            authority = &authority[..colon];
        }
    }
    authority.to_lowercase()
}

/// What a picture's src renders as against the viewer's instance `origin`:
/// [`ImageSource::Load`] (ours, data or local), a [`ImageSource::Link`]
/// (another website: never fetched) or plain [`ImageSource::Text`].
pub fn image_source(src: &str, alt: &str, origin: &str) -> ImageSource {
    let alt_text = alt.trim_matches(edge_space).to_string();
    let s = src.trim_matches(edge_space);
    if s.is_empty() || not_url(s) {
        return ImageSource::Text(alt_text);
    }
    if starts_with_ci(s, "data:") {
        return if is_loadable_data(&s.to_lowercase()) { ImageSource::Load } else { ImageSource::Text(alt_text) };
    }
    if LOCAL_SCHEMES.iter().any(|scheme| starts_with_ci(s, scheme)) {
        return ImageSource::Load;
    }
    if s.starts_with('/') && !s.starts_with("//") {
        return if is_attachment_path(s) { ImageSource::Load } else { ImageSource::Text(alt_text) };
    }
    let base = origin.trim_end_matches('/');
    if !base.is_empty() && starts_with_ci(s, &format!("{base}/")) && is_attachment_path(&s[base.len()..]) {
        return ImageSource::Load;
    }
    let (href, rest) = if let Some(rest) = s.strip_prefix("//") {
        (format!("https:{s}"), rest)
    } else {
        let Some(scheme) = LINK_SCHEMES.iter().find(|sc| starts_with_ci(s, &format!("{sc}//"))) else {
            return ImageSource::Text(alt_text);
        };
        (s.to_string(), &s[scheme.len() + 2..])
    };
    let host = link_host(rest);
    if host.is_empty() {
        return ImageSource::Text(alt_text);
    }
    let text = if alt_text.is_empty() { crate::contract::IMAGE_ACTIONS_LINK_HOST.replace("{host}", &host) } else { alt_text };
    ImageSource::Link { href, text }
}

/// True when the picture may be fetched at all.
pub fn image_source_allowed(src: &str, origin: &str) -> bool {
    image_source(src, "", origin) == ImageSource::Load
}

/// #EXP-1968 (`editModeSource`): what an image block shows in a desktop
/// editor's EDIT mode: [`image_source`], except a refused `text` source
/// with no alt shows its src (trimmed) as plain text, so the block can be
/// found and removed.
pub fn edit_mode_source(src: &str, alt: &str, origin: &str) -> ImageSource {
    match image_source(src, alt, origin) {
        ImageSource::Text(text) if text.is_empty() => ImageSource::Text(src.trim_matches(edge_space).to_string()),
        verdict => verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/image-actions.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("image-actions.json")
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn the_copy_is_the_fixtures() {
        let f = fixture();
        let copy = &f["copy"];
        assert_eq!(copy["copyImage"], crate::contract::IMAGE_ACTIONS_COPY_IMAGE);
        assert_eq!(copy["copied"], crate::contract::IMAGE_ACTIONS_COPIED);
        assert_eq!(copy["copyFailed"], crate::contract::IMAGE_ACTIONS_COPY_FAILED);
        assert_eq!(copy["linkHost"], crate::contract::IMAGE_ACTIONS_LINK_HOST);
        assert_eq!(hex(f["copyFormat"]["pngSignature"].as_str().unwrap()), PNG_SIGNATURE);
        assert_eq!(f["inputs"]["desktop"], serde_json::json!(["pointer"]));
        assert_eq!(f["inputs"]["ede"], serde_json::json!(["pointer"]));
        assert_eq!(f["icons"]["copyImage"], "ui-copy");
        assert_eq!(f["toasts"]["copied"], "success");
        assert_eq!(f["toasts"]["copyFailed"], "error");
        let policy = &f["sourcePolicy"];
        assert_eq!(policy["attachmentPath"], ATTACHMENT_PATH);
        assert_eq!(policy["localSchemes"], serde_json::json!(LOCAL_SCHEMES));
        assert_eq!(policy["linkSchemes"], serde_json::json!(LINK_SCHEMES));
        assert_eq!(policy["deniedDataMimes"], serde_json::json!(DENIED_DATA_MIMES));
    }

    #[test]
    fn menu_cases_replay() {
        for c in fixture()["menuCases"].as_array().unwrap() {
            let input = if c["input"] == "pointer" { ImageInput::Pointer } else { ImageInput::Touch };
            let got: Vec<&str> = image_menu(input, c["canShareFile"].as_bool().unwrap())
                .into_iter()
                .map(|item| match item {
                    ImageMenuItem::CopyImage => "copyImage",
                    ImageMenuItem::Share => "share",
                })
                .collect();
            let want: Vec<&str> = c["expected"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, want, "{}", c["$why"]);
        }
    }

    #[test]
    fn press_cases_replay() {
        for c in fixture()["pressCases"].as_array().unwrap() {
            let got = match press_menu(c["onImage"].as_bool().unwrap(), c["imageReady"].as_bool().unwrap(), !c["ownerMenu"].is_null()) {
                PressMenu::Image => "image",
                PressMenu::Owner => "owner",
                PressMenu::None => "none",
            };
            assert_eq!(got, c["expected"], "{}", c["$why"]);
        }
    }

    #[test]
    fn encoding_cases_replay() {
        for c in fixture()["encodingCases"].as_array().unwrap() {
            let got = match copy_encoding(&hex(c["head"].as_str().unwrap())) {
                CopyEncoding::Passthrough => "passthrough",
                CopyEncoding::Reencode => "reencode",
            };
            assert_eq!(got, c["expected"], "{}", c["$why"]);
        }
    }

    #[test]
    fn share_name_cases_replay() {
        for c in fixture()["shareNameCases"].as_array().unwrap() {
            assert_eq!(share_file_name(c["name"].as_str()), c["expected"].as_str().unwrap(), "{}", c["$why"]);
        }
    }

    #[test]
    fn source_cases_replay() {
        let f = fixture();
        let origin = f["sourcePolicy"]["origin"].as_str().unwrap();
        for c in f["sourceCases"].as_array().unwrap() {
            let (src, alt) = (c["src"].as_str().unwrap(), c["alt"].as_str().unwrap());
            let e = &c["expected"];
            let want = match e["kind"].as_str().unwrap() {
                "load" => ImageSource::Load,
                "link" => ImageSource::Link { href: e["href"].as_str().unwrap().into(), text: e["text"].as_str().unwrap().into() },
                _ => ImageSource::Text(e["text"].as_str().unwrap().into()),
            };
            assert_eq!(image_source(src, alt, origin), want, "{src:?}: {}", c["$why"]);
            assert_eq!(image_source_allowed(src, origin), want == ImageSource::Load, "{src:?}");
        }
    }

    #[test]
    fn edit_mode_cases_replay() {
        let f = fixture();
        let origin = f["sourcePolicy"]["origin"].as_str().unwrap();
        for c in f["editModeCases"].as_array().unwrap() {
            let (src, alt) = (c["src"].as_str().unwrap(), c["alt"].as_str().unwrap());
            let e = &c["expected"];
            let want = match e["kind"].as_str().unwrap() {
                "load" => ImageSource::Load,
                "link" => ImageSource::Link { href: e["href"].as_str().unwrap().into(), text: e["text"].as_str().unwrap().into() },
                _ => ImageSource::Text(e["text"].as_str().unwrap().into()),
            };
            assert_eq!(edit_mode_source(src, alt, origin), want, "{src:?}: {}", c["$why"]);
        }
    }

    #[test]
    fn the_copy_fetches_the_original() {
        assert_eq!(original_src("/api/attachments/a?w=480"), "/api/attachments/a");
        assert_eq!(original_src("/api/attachments/a?x=1&w=480&y=2#f"), "/api/attachments/a?x=1&y=2#f");
        assert_eq!(original_src("/api/attachments/a?width=2"), "/api/attachments/a?width=2");
        assert_eq!(original_src("/api/attachments/a"), "/api/attachments/a");
        assert_eq!(original_src("data:image/png;base64,iVBO?w=1"), "data:image/png;base64,iVBO?w=1", "a data url has no query");
    }
}
