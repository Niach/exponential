//! EXP-271: lift an image that sits INLINE in a paragraph out into its own
//! block, so the WYSIWYG description editor renders it as a picture.
//!
//! Every other implementation of the markdown contract treats an image as a
//! BLOCK: web's TipTap `image` node is block-level (ProseMirror lifts an
//! inline image straight out of its paragraph), iOS `ContentBlock` and
//! Android's block editor split on images, and so does this crate's own block
//! pipeline ([`super::parse`]'s `emit_image` flushes the pending text first).
//! The vendored WYSIWYG engine is the odd one out — it renders an image only
//! when a block holds NOTHING but the image (`parse_standalone_image`), and
//! keeps every other `![alt](src)` as literal text.
//!
//! That only ever mattered because real descriptions carry the inline form:
//! web's save path serialized a block image WITHOUT closing its block, so
//! `![](/api/attachments/x)\n\nafter` came back as
//! `![](/api/attachments/x)after` (fixed in `apps/web/src/lib/markdown-image.tsx`,
//! same issue). Descriptions already stored that way must still render, so
//! this normalizer runs over markdown on its way INTO the editor: canonical
//! input is returned byte-identical, and a glued image converges to the
//! block form on the next save.
//!
//! Deliberately narrow: only paragraph text is split. Images inside lists,
//! quotes, tables, headings, fenced/indented code, HTML blocks, code spans or
//! link labels are left exactly where they are — splitting those would break
//! the construct around them, which is worse than an image that stays text.

use std::ops::Range;

use crate::attachments_row::{extract_image_occurrences, ImageOccurrence};

/// Rewrite `markdown` so every inline paragraph image becomes its own block.
/// Returns the input unchanged when there is nothing to lift.
pub fn split_inline_images_into_blocks(markdown: &str) -> String {
    if !markdown.contains("![") {
        return markdown.to_string();
    }

    let mut out: Vec<String> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut context = LineContext::Fresh;
    let mut changed = false;

    for line in markdown.split('\n') {
        if let Some((ch, len)) = fence {
            out.push(line.to_string());
            if closes_fence(line, ch, len) {
                fence = None;
                context = LineContext::Fresh;
            }
            continue;
        }
        if line.trim().is_empty() {
            context = LineContext::Fresh;
            out.push(line.to_string());
            continue;
        }
        if let Some(opener) = fence_opener(line) {
            fence = Some(opener);
            out.push(line.to_string());
            continue;
        }
        if context == LineContext::Fresh {
            context = if opens_opaque_block(line) {
                LineContext::Opaque
            } else {
                LineContext::Paragraph
            };
        }
        if context == LineContext::Opaque {
            out.push(line.to_string());
            continue;
        }
        match split_paragraph_line(line) {
            Some(parts) => {
                changed = true;
                // The split ends the paragraph this line belonged to, so a
                // running paragraph above it needs its own terminator.
                if out.last().is_some_and(|prev| !prev.trim().is_empty()) {
                    out.push(String::new());
                }
                out.extend(parts);
            }
            None => out.push(line.to_string()),
        }
    }

    if changed {
        out.join("\n")
    } else {
        markdown.to_string()
    }
}

/// What the current run of non-blank lines is: prose we may split, or a
/// construct we must not touch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LineContext {
    /// At a block boundary — the next non-blank line decides.
    Fresh,
    Paragraph,
    Opaque,
}

/// Split one paragraph line into `[text, image, text, …]` parts separated by
/// blank lines. `None` = nothing to do (no eligible image, or the line is
/// already nothing but one image).
fn split_paragraph_line(line: &str) -> Option<Vec<String>> {
    let images = eligible_images(line);
    if images.is_empty() {
        return None;
    }
    if images.len() == 1
        && line[..images[0].start].trim().is_empty()
        && line[images[0].end..].trim().is_empty()
    {
        // Already standalone — leave the bytes (and any indentation the
        // vendored parser tolerates) exactly as they are.
        return None;
    }

    let mut parts: Vec<&str> = Vec::new();
    let mut cursor = 0;
    for image in &images {
        // Trimmed: a part must not inherit boundary whitespace, and four
        // leading spaces would turn the new block into indented code.
        let before = line[cursor..image.start].trim();
        if !before.is_empty() {
            parts.push(before);
        }
        parts.push(&line[image.start..image.end]);
        cursor = image.end;
    }
    let after = line[cursor..].trim();
    if !after.is_empty() {
        parts.push(after);
    }

    let mut lines = Vec::with_capacity(parts.len() * 2);
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(part.to_string());
    }
    Some(lines)
}

/// The `![alt](src)` occurrences on a line that may be lifted out of it.
fn eligible_images(line: &str) -> Vec<ImageOccurrence> {
    let bytes = line.as_bytes();
    let code_spans = code_span_ranges(line);
    extract_image_occurrences(line)
        .into_iter()
        .filter(|image| {
            // `\![x](y)` — the escaped `!` makes this literal text plus a link.
            if image.start > 0 && bytes[image.start - 1] == b'\\' {
                return false;
            }
            // `[![alt](img)](href)` — a linked image belongs to its link.
            if image.start > 0
                && bytes[image.start - 1] == b'['
                && line[image.end..].starts_with("](")
            {
                return false;
            }
            !code_spans.iter().any(|span| span.contains(&image.start))
        })
        .collect()
}

/// Byte ranges covered by inline code spans (CommonMark: a run of N backticks
/// is closed by the next run of exactly N). Unmatched runs are literal text.
fn code_span_ranges(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let open_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let run = i - open_start;
        let mut probe = i;
        let mut close_end = None;
        while probe < bytes.len() {
            if bytes[probe] != b'`' {
                probe += 1;
                continue;
            }
            let close_start = probe;
            while probe < bytes.len() && bytes[probe] == b'`' {
                probe += 1;
            }
            if probe - close_start == run {
                close_end = Some(probe);
                break;
            }
        }
        if let Some(end) = close_end {
            spans.push(open_start..end);
            i = end;
        }
    }
    spans
}

/// The fence character + length when `line` opens a fenced code block.
fn fence_opener(line: &str) -> Option<(u8, usize)> {
    let body = strip_block_indent(line)?;
    let ch = *body.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = body.bytes().take_while(|byte| *byte == ch).count();
    (len >= 3).then_some((ch, len))
}

fn closes_fence(line: &str, ch: u8, len: usize) -> bool {
    let Some(body) = strip_block_indent(line) else {
        return false;
    };
    let run = body.bytes().take_while(|byte| *byte == ch).count();
    run >= len && body[run..].trim().is_empty()
}

/// Whether this line starts a construct whose inner images must stay put.
fn opens_opaque_block(line: &str) -> bool {
    let Some(body) = strip_block_indent(line) else {
        // Four or more leading spaces: indented code block.
        return true;
    };
    if is_thematic_break(body) {
        return true;
    }
    let bytes = body.as_bytes();
    match bytes[0] {
        // Heading, blockquote, HTML block, table row.
        b'#' | b'>' | b'<' | b'|' => return true,
        // Bullet-list marker (`- `, `* `, `+ `, or a bare marker line).
        b'-' | b'*' | b'+' => {
            if bytes.len() == 1 || matches!(bytes[1], b' ' | b'\t') {
                return true;
            }
        }
        // Link-reference / footnote definition.
        b'[' => {
            if body.contains("]:") {
                return true;
            }
        }
        _ => {}
    }
    // Ordered-list marker: up to nine digits then `.`/`)` then a space.
    let digits = bytes.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
        && matches!(bytes.get(digits + 1), None | Some(b' ') | Some(b'\t'))
    {
        return true;
    }
    // A table row need not start with `|` (`a | b\n--- | ---`).
    line.contains('|')
}

/// `---` / `***` / `___` (three or more of one marker, spaces allowed).
fn is_thematic_break(body: &str) -> bool {
    let marker = match body.as_bytes()[0] {
        marker @ (b'-' | b'*' | b'_') => marker,
        _ => return false,
    };
    let mut count = 0;
    for byte in body.bytes() {
        match byte {
            b' ' | b'\t' => {}
            byte if byte == marker => count += 1,
            _ => return false,
        }
    }
    count >= 3
}

/// The line past up to three leading spaces, or `None` at four or more (an
/// indented code block).
fn strip_block_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent >= 4 {
        return None;
    }
    let body = &line[indent..];
    (!body.is_empty()).then_some(body)
}

#[cfg(test)]
mod tests {
    use super::split_inline_images_into_blocks as split;

    #[track_caller]
    fn assert_unchanged(markdown: &str) {
        assert_eq!(split(markdown), markdown, "should be left byte-identical");
    }

    #[test]
    fn lifts_the_shape_web_actually_stored() {
        // EXP-249's real description: the image welded onto the paragraph.
        assert_eq!(
            split("![](/api/attachments/abc)on mobile steering the plan.\n\nalso when approving"),
            "![](/api/attachments/abc)\n\non mobile steering the plan.\n\nalso when approving"
        );
    }

    #[test]
    fn lifts_an_image_from_the_middle_of_a_sentence() {
        assert_eq!(
            split("before ![alt](/api/attachments/abc) after"),
            "before\n\n![alt](/api/attachments/abc)\n\nafter"
        );
    }

    #[test]
    fn lifts_several_images_from_one_line() {
        assert_eq!(
            split("![a](/api/attachments/a)mid![b](/api/attachments/b)end"),
            "![a](/api/attachments/a)\n\nmid\n\n![b](/api/attachments/b)\n\nend"
        );
    }

    #[test]
    fn terminates_the_paragraph_the_image_line_continued() {
        assert_eq!(
            split("some text\n![alt](/api/attachments/abc) more"),
            "some text\n\n![alt](/api/attachments/abc)\n\nmore"
        );
    }

    #[test]
    fn keeps_canonical_block_images_byte_identical() {
        assert_unchanged("![diagram](/api/attachments/abc123)");
        assert_unchanged("before\n\n![alt](/api/attachments/abc)\n\nafter");
        assert_unchanged("![a](/api/attachments/a)\n\n![b](/api/attachments/b)");
        assert_unchanged("![alt](/api/attachments/abc?w=480)\n\nafter");
        assert_unchanged("");
        assert_unchanged("no images at all");
    }

    #[test]
    fn leaves_images_inside_code_alone() {
        assert_unchanged("```\n![alt](/api/attachments/abc)text\n```");
        assert_unchanged("~~~md\n![alt](/api/attachments/abc)text\n~~~");
        assert_unchanged("use `![alt](/api/attachments/abc)` to embed");
        assert_unchanged("    ![alt](/api/attachments/abc)text");
    }

    #[test]
    fn leaves_images_inside_other_constructs_alone() {
        assert_unchanged("- ![alt](/api/attachments/abc)text");
        assert_unchanged("- [ ] ![alt](/api/attachments/abc)text");
        assert_unchanged("1. ![alt](/api/attachments/abc)text");
        assert_unchanged("> ![alt](/api/attachments/abc)text");
        assert_unchanged("# ![alt](/api/attachments/abc)text");
        assert_unchanged("| ![alt](/api/attachments/abc)text | b |");
        assert_unchanged("<div>![alt](/api/attachments/abc)text</div>");
        assert_unchanged("- parent\n  ![alt](/api/attachments/abc)text");
        assert_unchanged("[ref]: /api/attachments/abc ![alt](/api/attachments/x)text");
    }

    #[test]
    fn leaves_escaped_and_linked_images_alone() {
        // `\!` makes this a literal `!` plus an ordinary link.
        assert_unchanged("text \\![alt](/api/attachments/abc) more");
        // A linked image is one inline unit; splitting would break the link.
        assert_unchanged("see [![alt](/api/attachments/abc)](https://example.com) here");
    }

    #[test]
    fn splits_around_an_ineligible_image_without_touching_it() {
        assert_eq!(
            split("`![a](/api/attachments/a)` and ![b](/api/attachments/b) end"),
            "`![a](/api/attachments/a)` and\n\n![b](/api/attachments/b)\n\nend"
        );
    }

    #[test]
    fn resumes_splitting_after_a_fenced_block_closes() {
        assert_eq!(
            split("```\n![a](/api/attachments/a)x\n```\n\n![b](/api/attachments/b)y"),
            "```\n![a](/api/attachments/a)x\n```\n\n![b](/api/attachments/b)\n\ny"
        );
    }

    #[test]
    fn is_idempotent() {
        let once = split("before ![alt](/api/attachments/abc) after");
        assert_eq!(split(&once), once);
    }

    #[test]
    fn handles_multibyte_prose() {
        assert_eq!(
            split("Grüße ![münchen](/api/attachments/abc) 🚀"),
            "Grüße\n\n![münchen](/api/attachments/abc)\n\n🚀"
        );
    }
}
