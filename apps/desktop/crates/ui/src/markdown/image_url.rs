//! Attachment-image URL helpers (EXP-256; straight port of the web's
//! `apps/web/src/lib/markdown-image.tsx` L29-61).
//!
//! The display width of an embedded image is persisted as a `?w=<int>` query
//! param on the attachment src — the markdown stays plain GFM
//! (`![alt](/api/attachments/{id}?w=480)`) and clients that don't understand
//! the param simply ignore it. The query-stripped src is the canonical
//! full-size form: it keys the [`super::editor::ImageCache`] fetch (the
//! server ignores `?w=` — resizing is purely a client display hint) and
//! feeds the lightbox / copy-link.

/// Drag clamps (web `minResizeWidth`/`fallbackMaxResizeWidth`): min keeps the
/// image usable/grabbable; max is the natural probed width (dragging back to
/// it removes the param so the markdown stays canonical-clean).
pub(crate) const MIN_RESIZE_WIDTH: f32 = 120.;
pub(crate) const FALLBACK_MAX_RESIZE_WIDTH: f32 = 4000.;

/// Pull the attachment id out of a `/api/attachments/{id}` (relative or
/// absolute) image src (web `attachmentIdFromSrc`).
pub(crate) fn attachment_id_from_src(src: &str) -> Option<&str> {
    let marker = "/api/attachments/";
    let start = src.find(marker)? + marker.len();
    let rest = &src[start..];
    let end = rest
        .find(|c| matches!(c, '/' | '?' | '#'))
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

/// EXP-824 media-lift gate (web `isOwnAttachmentHref`, iOS
/// `AttachmentLinks.attachmentId(fromUrl:baseURL:)`): does `src` name one of
/// OUR attachments — the relative `/api/attachments/{id}` form, or an
/// absolute URL on `origin` (the active account's instance URL)? A foreign
/// host's `/api/attachments/…` path is never ours, so a pasted link to it
/// stays a plain link instead of a media tile that can never resolve. With
/// no known origin, only the relative form passes.
pub(crate) fn is_own_attachment_src(src: &str, origin: Option<&str>) -> bool {
    if attachment_id_from_src(src).is_none() {
        return false;
    }
    let Some((scheme, authority, path)) = split_absolute_url(src) else {
        // No scheme: relative. `//host/…` (scheme-relative) and `x/api/…`
        // both fail this prefix test, as they should.
        return src.starts_with("/api/attachments/");
    };
    let Some(origin) = origin else {
        return false;
    };
    let Some((origin_scheme, origin_authority, _)) = split_absolute_url(origin) else {
        return false;
    };
    scheme.eq_ignore_ascii_case(origin_scheme)
        && authority.eq_ignore_ascii_case(origin_authority)
        && path.starts_with("/api/attachments/")
}

/// `(scheme, authority, path-and-after)` of an absolute URL, `None` when
/// `url` carries no `scheme://` prefix.
fn split_absolute_url(url: &str) -> Option<(&str, &str, &str)> {
    let colon = url.find("://")?;
    let scheme = &url[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        return None;
    }
    let rest = &url[colon + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some((scheme, &rest[..end], &rest[end..]))
}

/// The `?w=<int>` display width (web `widthParamFromSrc`): a positive integer
/// `w` query param, else `None`.
pub(crate) fn width_param_from_src(src: &str) -> Option<f32> {
    let query_start = src.find('?')?;
    let query = &src[query_start + 1..];
    let query = query.split('#').next().unwrap_or(query);
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("w=") {
            let parsed: u32 = value.parse().ok()?;
            if parsed > 0 {
                return Some(parsed as f32);
            }
            return None;
        }
    }
    None
}

/// The src stripped of query/hash — the canonical full-size attachment form
/// (web `stripQuery`).
pub(crate) fn strip_query(src: &str) -> &str {
    let end = src.find(['?', '#']).unwrap_or(src.len());
    &src[..end]
}

/// Rebuild an attachment src carrying (`Some`) or dropping (`None`) the
/// `?w=` width param (web `srcWithWidth`).
pub(crate) fn src_with_width(src: &str, width: Option<u32>) -> String {
    let base = strip_query(src);
    match width {
        Some(width) => format!("{base}?w={width}"),
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_id_parses_relative_and_absolute_srcs() {
        assert_eq!(
            attachment_id_from_src("/api/attachments/abc-123"),
            Some("abc-123")
        );
        assert_eq!(
            attachment_id_from_src("https://app.exponential.at/api/attachments/abc?w=480"),
            Some("abc")
        );
        assert_eq!(
            attachment_id_from_src("/api/attachments/abc#frag"),
            Some("abc")
        );
        assert_eq!(attachment_id_from_src("/api/attachments/"), None);
        assert_eq!(attachment_id_from_src("draft://xyz"), None);
        assert_eq!(attachment_id_from_src("https://elsewhere.example/x.png"), None);
    }

    /// EXP-824: the media LIFT is stricter than the id parse — only the
    /// relative form or an absolute URL on the instance origin is ours.
    #[test]
    fn own_attachment_src_requires_relative_or_same_origin() {
        let origin = Some("https://app.exponential.at");
        assert!(is_own_attachment_src("/api/attachments/abc", origin));
        assert!(is_own_attachment_src("/api/attachments/abc?w=480", origin));
        assert!(is_own_attachment_src("/api/attachments/abc", None));
        assert!(is_own_attachment_src(
            "https://app.exponential.at/api/attachments/abc?w=480",
            origin
        ));
        assert!(is_own_attachment_src(
            "HTTPS://App.Exponential.at/api/attachments/abc",
            Some("https://app.exponential.at/")
        ));
        // Foreign host, scheme mismatch, scheme-relative, unknown origin.
        assert!(!is_own_attachment_src(
            "https://other-host.example/api/attachments/abc",
            origin
        ));
        assert!(!is_own_attachment_src(
            "http://app.exponential.at/api/attachments/abc",
            origin
        ));
        assert!(!is_own_attachment_src("//app.exponential.at/api/attachments/abc", origin));
        assert!(!is_own_attachment_src(
            "https://app.exponential.at/api/attachments/abc",
            None
        ));
        // Not an attachment path at all, or not at the path root.
        assert!(!is_own_attachment_src("https://app.exponential.at/x.png", origin));
        assert!(!is_own_attachment_src("proxy/api/attachments/abc", origin));
        assert!(!is_own_attachment_src("draft://abc", origin));
    }

    #[test]
    fn width_param_parses_like_the_web_regex() {
        assert_eq!(width_param_from_src("/api/attachments/a?w=480"), Some(480.));
        assert_eq!(
            width_param_from_src("/api/attachments/a?x=1&w=320#frag"),
            Some(320.)
        );
        assert_eq!(width_param_from_src("/api/attachments/a"), None);
        assert_eq!(width_param_from_src("/api/attachments/a?w=0"), None);
        assert_eq!(width_param_from_src("/api/attachments/a?w=abc"), None);
        assert_eq!(width_param_from_src("/api/attachments/a?width=9"), None);
    }

    #[test]
    fn strip_and_rebuild_round_trip() {
        assert_eq!(strip_query("/api/attachments/a?w=480"), "/api/attachments/a");
        assert_eq!(strip_query("/api/attachments/a#frag"), "/api/attachments/a");
        assert_eq!(strip_query("/api/attachments/a"), "/api/attachments/a");
        assert_eq!(
            src_with_width("/api/attachments/a?w=480", Some(320)),
            "/api/attachments/a?w=320"
        );
        assert_eq!(
            src_with_width("/api/attachments/a?w=480", None),
            "/api/attachments/a"
        );
        assert_eq!(
            src_with_width("/api/attachments/a", Some(480)),
            "/api/attachments/a?w=480"
        );
    }
}
