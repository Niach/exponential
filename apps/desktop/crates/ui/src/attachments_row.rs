//! The issue-detail attachment strip (masterplan-v3 §4.2; web reference:
//! `apps/web/src/components/issue-editor/attachment-rail.tsx` +
//! `lib/storage/issue-attachments.ts`).
//!
//! Web derives the rail from the **description markdown's image
//! occurrences** (`extractMarkdownImageOccurrences`), renders one chip per
//! occurrence, and always shows the trailing **"N images"** count — even at
//! zero. This module mirrors that: [`extract_image_occurrences`] is the
//! Rust port of the web's `markdownImagePattern` scan, and the strip renders
//! unconditionally under the description (web `px-4 py-3 border-t`).
//!
//! Desktop v1 chips are compact (image glyph + label) rather than bitmap
//! thumbnails — fetching bitmaps needs authenticated `/api/attachments/{id}`
//! requests, which belong to the §4.5 editor's image pipeline; the rail
//! upgrades when that lands. Upload/removal entry points live with the
//! editor on the detail screen (§4.5 one-upload-path rule); the create
//! dialog's rail (create_issue_dialog.rs) reuses [`image_chip`] with a
//! remove ✕ exactly like web.

use gpui::{div, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;

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

/// The detail strip, derived from the issue's description markdown exactly
/// like web (always rendered — "0 images" included). `None` only when the
/// issue row itself is gone from the synced collection.
pub(crate) fn attachments_row(issue_id: &str, cx: &App) -> Option<impl IntoElement> {
    let description = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .get(issue_id)?
        .description
        .clone()
        .unwrap_or_default();
    let occurrences = extract_image_occurrences(&description);

    Some(
        h_flex()
            .w_full()
            .px_4()
            .py_3()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1p5()
                    .overflow_hidden()
                    .children(occurrences.iter().enumerate().map(|(ix, occurrence)| {
                        // Detail chips are read-only in v1: removal edits the
                        // description markdown, which is the §4.5 editor's job.
                        image_chip(occurrence_label(occurrence, ix), None, cx)
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(image_count_label(occurrences.len()))),
            ),
    )
}

/// Remove-✕ handler of one chip (element id + click callback).
pub(crate) type ChipRemove = (
    SharedString,
    Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static>,
);

/// One attachment chip (web chip layout: thumbnail + truncating label + the
/// optional remove ✕). Desktop v1 shows the image glyph in the thumbnail
/// slot; `on_remove` renders the web's ✕ button when given.
pub(crate) fn image_chip(
    label: String,
    on_remove: Option<ChipRemove>,
    cx: &App,
) -> gpui::AnyElement {
    let mut row = h_flex()
        .flex_shrink_0()
        .gap_1()
        .px_1p5()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border.opacity(0.5))
        .bg(cx.theme().secondary.opacity(0.4))
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
                .child(SharedString::from(label)),
        );

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
                .on_click(on_click),
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
