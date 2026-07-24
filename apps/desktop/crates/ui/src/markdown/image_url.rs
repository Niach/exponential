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
