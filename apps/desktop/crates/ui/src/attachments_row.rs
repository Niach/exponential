//! Attachment-chip helpers for the create-issue dialog (web reference:
//! `apps/web/src/components/issue-editor/attachment-rail.tsx` +
//! `lib/storage/issue-attachments.ts`).
//!
//! [`extract_image_occurrences`] is the Rust port of the web's
//! `markdownImagePattern` scan over a description markdown (used by the
//! WYSIWYG editor + image paste). EXP-586: the create dialog no longer lists
//! image chips at all — images live inline in the description only; the
//! dialog's footer carries just [`file_chip`] rows for queued non-image
//! files. The issue-detail strip is gone too (EXP-256).

use gpui::{
    div, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, SharedString, Styled,
};
use gpui_component::{
    h_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::icons::registry;

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
/// (`/!\[((?:\\.|[^\\\]])*)]\(([^)\s]+)(?:\s+"[^"]*")?\)/g`): alt runs to the
/// first UNESCAPED `]` (serializers escape `]`/`\` inside alt — the vendored
/// editor's `pasted_image_markdown`, TipTap via prosemirror-markdown — so a
/// `shot [1].png` filename must not break the scan, REV-6), URL without
/// `)`/whitespace, optional quoted title. `alt` is unescaped display text,
/// matching the web occurrence's `alt`.
pub(crate) fn extract_image_occurrences(text: &str) -> Vec<ImageOccurrence> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find("![") {
        let start = i + rel;
        // Resume one byte past the `!` on any failed match — the same spot a
        // regex engine would retry from.
        let retry = start + 1;

        // `(?:\\.|[^\\\]])*` — alt runs to the first unescaped `]`; a
        // backslash consumes the following char as an escape pair.
        let mut cursor = start + 2;
        let alt_end = loop {
            match bytes.get(cursor) {
                None => break None,
                Some(b']') => break Some(cursor),
                Some(b'\\') => {
                    let Some(escaped) = text[cursor + 1..].chars().next() else {
                        break None;
                    };
                    cursor += 1 + escaped.len_utf8();
                }
                Some(_) => cursor += 1,
            }
        };
        let Some(alt_end) = alt_end else {
            break;
        };
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
            alt: unescape_markdown_text(&text[start + 2..alt_end]),
            url: text[url_start..j].to_string(),
            start,
            end,
        });
        i = end;
    }
    out
}

/// Reverses CommonMark backslash escapes (ASCII punctuation only) — the web
/// `unescapeMarkdownText`, so occurrence alts carry the display text, not the
/// serialized escape form.
fn unescape_markdown_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_punctuation() {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(ch);
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

/// Remove-✕ handler of one chip (element id + click callback).
pub(crate) type ChipRemove = (
    SharedString,
    Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static>,
);

/// EXP-335: one queued NON-image draft file in the create dialog's footer
/// rail (web `issue-attachment-file-chip-*`): type glyph · filename · size ·
/// optional remove ✕. No lightbox — nothing to preview before upload.
pub(crate) fn file_chip(
    id: impl Into<ElementId>,
    filename: String,
    content_type: Option<&str>,
    size_bytes: i64,
    on_remove: Option<ChipRemove>,
    cx: &App,
) -> gpui::AnyElement {
    let glyph = crate::issue_files::icon_for_content_type(content_type);
    let mut row = h_flex()
        .id(id.into())
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .items_center()
        .child(
            Icon::from(glyph)
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
                .child(SharedString::from(filename)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(crate::issue_files::format_bytes(
                    size_bytes,
                ))),
        );

    if let Some((id, on_click)) = on_remove {
        row = row.child(
            // EXP-698: the one 32px glass chrome every trailing action wears.
            crate::controls::glass_icon_button(id, Icon::new(registry::UI_CLOSE), cx)
                .on_click(move |event, window, cx| {
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
    fn parses_serializer_escaped_alt_text() {
        // `pasted_image_markdown` escapes `]`/`\` in alt (and TipTap escapes
        // all markdown punctuation) — the alt scan must consume escape pairs
        // instead of dropping the whole occurrence (REV-6).
        let occurrences = extract_image_occurrences(r"![shot \[1\].png](draft://x)");
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].alt, "shot [1].png");
        assert_eq!(occurrences[0].url, "draft://x");

        let trailing = extract_image_occurrences(r"![trailing\\](draft://x)");
        assert_eq!(trailing.len(), 1);
        assert_eq!(trailing[0].alt, "trailing\\");

        // A backslash before a non-punctuation char stays literal.
        let literal = extract_image_occurrences(r"![a\ b](draft://x)");
        assert_eq!(literal.len(), 1);
        assert_eq!(literal[0].alt, r"a\ b");
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
}
