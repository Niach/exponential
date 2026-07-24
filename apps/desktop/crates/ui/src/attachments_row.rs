//! Attachment-chip helpers for the create-issue dialog (web reference:
//! `apps/web/src/components/issue-editor/attachment-rail.tsx` +
//! `lib/storage/issue-attachments.ts`).
//!
//! [`extract_image_occurrences`] is the Rust port of the web's
//! `markdownImagePattern` scan over a description markdown; the create
//! dialog's rail (create_issue_dialog.rs) renders one [`image_chip`] per
//! occurrence with a remove ✕ exactly like web. The issue-detail strip that
//! used to render these chips under the description is gone (EXP-256 —
//! removed on web too): the description editor's image blocks are the one
//! attachment surface on the detail screen.

use gpui::{
    div, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

use crate::icons::ExpIcon;

/// One `![alt](url)` occurrence in a markdown string — the web's
/// `MarkdownImageOccurrence` (byte offsets over the source).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageOccurrence {
    pub alt: String,
    pub url: String,
    pub start: usize,
    pub end: usize,
}

/// Rust port of the web `markdownImagePattern`
/// (`/!\[([^\]]*)]\(([^)\s]+)(?:\s+"[^"]*")?\)/g`): alt without `]`, URL
/// without `)`/whitespace, optional quoted title.
pub(crate) fn extract_image_occurrences(text: &str) -> Vec<ImageOccurrence> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find("![") {
        let start = i + rel;
        // Resume one byte past the `!` on any failed match — the same spot a
        // regex engine would retry from.
        let retry = start + 1;

        // `[^\]]*` — alt runs to the first `]`.
        let Some(alt_rel) = text[start + 2..].find(']') else {
            break;
        };
        let alt_end = start + 2 + alt_rel;
        if bytes.get(alt_end + 1) != Some(&b'(') {
            i = retry;
            continue;
        }

        // `[^)\s]+` — the URL.
        let url_start = alt_end + 2;
        let mut j = url_start;
        while j < bytes.len() && !matches!(bytes[j], b')' | b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        if j == url_start {
            i = retry;
            continue;
        }

        // `(?:\s+"[^"]*")?` — optional quoted title, then `)`.
        let mut k = j;
        if k < bytes.len() && bytes[k] != b')' {
            let mut w = k;
            while w < bytes.len() && matches!(bytes[w], b' ' | b'\t' | b'\n' | b'\r') {
                w += 1;
            }
            let Some(&b'"') = bytes.get(w) else {
                i = retry;
                continue;
            };
            let Some(quote_rel) = text[w + 1..].find('"') else {
                i = retry;
                continue;
            };
            k = w + 1 + quote_rel + 1;
        }
        if bytes.get(k) != Some(&b')') {
            i = retry;
            continue;
        }
        let end = k + 1;

        out.push(ImageOccurrence {
            alt: text[start + 2..alt_end].to_string(),
            url: text[url_start..j].to_string(),
            start,
            end,
        });
        i = end;
    }
    out
}

/// Web `removeMarkdownImageByOccurrence`: drop the nth occurrence, leave the
/// rest of the text untouched.
pub(crate) fn remove_image_occurrence(text: &str, occurrence_index: usize) -> String {
    let occurrences = extract_image_occurrences(text);
    let Some(occurrence) = occurrences.get(occurrence_index) else {
        return text.to_string();
    };
    format!("{}{}", &text[..occurrence.start], &text[occurrence.end..])
}

/// Web `getAttachmentLabel`: alt → URL filename → "Image N".
pub(crate) fn occurrence_label(occurrence: &ImageOccurrence, occurrence_index: usize) -> String {
    let alt = occurrence.alt.trim();
    if !alt.is_empty() {
        return alt.to_string();
    }
    let filename = occurrence
        .url
        .rsplit('/')
        .next()
        .and_then(|segment| segment.split('?').next())
        .unwrap_or_default();
    if !filename.is_empty() {
        return filename.to_string();
    }
    format!("Image {}", occurrence_index + 1)
}

/// Web `imageCountLabel`: "1 image" / "N images".
pub(crate) fn image_count_label(count: usize) -> String {
    if count == 1 {
        "1 image".to_string()
    } else {
        format!("{count} images")
    }
}

/// Remove-✕ handler of one chip (element id + click callback).
pub(crate) type ChipRemove = (
    SharedString,
    Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static>,
);

/// One attachment chip (web chip layout: thumbnail + truncating label + the
/// optional remove ✕). Desktop v1 shows the image glyph in the thumbnail
/// slot; `on_remove` renders the web's ✕ button when given.
///
/// Borderless by design (EXP-33): a soft secondary pill with a hover state —
/// the old bordered chips read as nested boxes inside the bordered rail.
/// Clicking the chip body opens the IN-APP image preview
/// ([`crate::image_preview`]); unresolvable URLs (signed out, `draft://`
/// staging) leave the chip inert. The ✕ stops propagation so a remove never
/// also opens.
pub(crate) fn image_chip(
    id: impl Into<ElementId>,
    label: String,
    url: &str,
    on_remove: Option<ChipRemove>,
    cx: &App,
) -> gpui::AnyElement {
    let openable = crate::queries::absolute_api_url(cx, url).is_some();
    let mut row = h_flex()
        .id(id.into())
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(cx.theme().secondary.opacity(0.5))
        .items_center()
        .child(
            Icon::from(ExpIcon::Image)
                .xsmall()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .max_w(gpui::px(96.))
                .text_xs()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(label.clone())),
        );

    if openable {
        // Query-stripped: the lightbox always shows full size (a `?w=` src
        // is a display-width hint, and stripping keeps the ImageCache keyed
        // on one canonical URL per attachment).
        let url = crate::markdown::image_url::strip_query(url).to_string();
        row = row
            .cursor_pointer()
            .hover(|el| el.bg(cx.theme().secondary))
            .on_click(move |_, window, cx| {
                crate::image_preview::open_image_preview(
                    url.clone(),
                    label.clone(),
                    None,
                    window,
                    cx,
                );
            });
    }

    if let Some((id, on_click)) = on_remove {
        row = row.child(
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(
                    Icon::new(IconName::Close)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .on_click(move |event, window, cx| {
                    // The ✕ must never also fire the chip-open click.
                    cx.stop_propagation();
                    on_click(event, window, cx);
                }),
        );
    }
    row.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_occurrences_like_the_web_pattern() {
        let text = "Intro\n\n![shot](/api/attachments/abc)\n\ntext ![](draft://xyz) end";
        let occurrences = extract_image_occurrences(text);
        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[0].alt, "shot");
        assert_eq!(occurrences[0].url, "/api/attachments/abc");
        assert_eq!(occurrences[1].alt, "");
        assert_eq!(occurrences[1].url, "draft://xyz");
        assert_eq!(
            &text[occurrences[0].start..occurrences[0].end],
            "![shot](/api/attachments/abc)"
        );
    }

    #[test]
    fn optional_title_and_non_matches_follow_the_regex() {
        let with_title = r#"![a](/u/x "hover")"#;
        let occurrences = extract_image_occurrences(with_title);
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].url, "/u/x");
        // `[^)\s]+` requires a non-empty URL; a plain link is not an image.
        assert!(extract_image_occurrences("![x]()").is_empty());
        assert!(extract_image_occurrences("[not-image](/u)").is_empty());
        assert!(extract_image_occurrences("![unclosed](/u").is_empty());
    }

    #[test]
    fn removes_only_the_requested_occurrence() {
        let text = "![a](/one)\n\n![b](/two)";
        assert_eq!(remove_image_occurrence(text, 0), "\n\n![b](/two)");
        assert_eq!(remove_image_occurrence(text, 1), "![a](/one)\n\n");
        assert_eq!(remove_image_occurrence(text, 5), text);
    }

    #[test]
    fn labels_fall_back_alt_then_filename_then_index() {
        let alt = ImageOccurrence {
            alt: " shot ".into(),
            url: "/api/attachments/abc".into(),
            start: 0,
            end: 0,
        };
        assert_eq!(occurrence_label(&alt, 0), "shot");
        let file = ImageOccurrence {
            alt: "".into(),
            url: "/api/attachments/abc-photo.png?w=1".into(),
            start: 0,
            end: 0,
        };
        assert_eq!(occurrence_label(&file, 0), "abc-photo.png");
        let bare = ImageOccurrence {
            alt: "".into(),
            url: "".into(),
            start: 0,
            end: 0,
        };
        assert_eq!(occurrence_label(&bare, 2), "Image 3");
    }

    #[test]
    fn count_labels_pluralize() {
        assert_eq!(image_count_label(0), "0 images");
        assert_eq!(image_count_label(1), "1 image");
        assert_eq!(image_count_label(4), "4 images");
    }
}
