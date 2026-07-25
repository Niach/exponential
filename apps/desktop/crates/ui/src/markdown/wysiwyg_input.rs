//! EXP-271: what the vendored WYSIWYG engine needs done to markdown BEFORE it
//! sees it, so a description written on any other client renders the way it
//! does everywhere else. Two transforms, one line scan, one entry point
//! ([`normalize_for_wysiwyg`]) — applied at both of `WysiwygDescription`'s
//! load paths — plus one paired transform on the way back out
//! ([`restore_blank_line_markers`], documented at its definition).
//!
//! **1. HTML entities are decoded.** The vendored engine has no notion of
//! them, so `&nbsp;` (the interchange form of an intentional blank line — see
//! `apps/web/src/components/issue-editor/markdown-paragraph.ts`) and the
//! `&gt;`/`&amp;` the web's serializer emits for literal `>`/`&` all showed up
//! as raw source text. Decoding is delegated to comrak — the same library
//! behind this crate's block editor ([`super::parse`]), so the two desktop
//! editors display and save identical text, including comrak's own edges (an
//! `&amp;nbsp;` decodes one level per save, and code contexts decode not at
//! all).
//!
//! **2. An image sitting INLINE in a paragraph is lifted into its own block.**
//! Every other implementation of the contract treats an image as a BLOCK:
//! web's TipTap `image` node is block-level (ProseMirror lifts an inline image
//! straight out of its paragraph), iOS `ContentBlock` and Android's block
//! editor split on images, and so does [`super::parse`]'s `emit_image`. The
//! vendored engine is the odd one out — it renders an image only when a block
//! holds NOTHING but the image (`parse_standalone_image`), and keeps every
//! other `![alt](src)` as literal text. That mattered because real
//! descriptions carry the inline form: web's save path serialized a block
//! image WITHOUT closing its block, so `![](/api/attachments/x)\n\nafter` came
//! back as `![](/api/attachments/x)after` (fixed in
//! `apps/web/src/lib/markdown-image.tsx`, same issue).
//!
//! Both transforms are deliberately narrow. Canonical input is returned
//! byte-identical; code — fenced, indented, or a span — is never touched at
//! all; and only paragraph text is split, so images inside lists, quotes,
//! tables, headings, HTML blocks or link labels stay exactly where they are
//! (breaking the construct around an image is worse than an image that stays
//! text). Content already stored in a non-canonical form converges on the
//! next save.

use std::collections::HashMap;
use std::ops::Range;

use crate::attachments_row::{extract_image_occurrences, ImageOccurrence};

/// Prepare `markdown` for the vendored WYSIWYG engine. Returns the input
/// unchanged when there is nothing to normalize.
pub fn normalize_for_wysiwyg(markdown: &str) -> String {
    if !markdown.contains('&') && !markdown.contains("![") {
        return markdown.to_string();
    }

    let mut out: Vec<String> = Vec::new();
    let mut entities = EntityCache::default();
    let mut fence: Option<(u8, usize)> = None;
    let mut context = LineContext::Fresh;

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
        // An indented code block only starts at a block boundary — four
        // leading spaces inside a running paragraph are just prose.
        if context == LineContext::Fresh && strip_block_indent(line).is_none() {
            context = LineContext::Opaque;
            out.push(line.to_string());
            continue;
        }

        // Entities first: decoding can change what block a line opens
        // (`&gt;` becomes a blockquote marker), and the split below must see
        // the text the engine will actually hold.
        let line = decode_entities(line, &mut entities);
        if context == LineContext::Fresh {
            context = if opens_opaque_block(&line) {
                LineContext::Opaque
            } else {
                LineContext::Paragraph
            };
        }
        if context == LineContext::Opaque {
            out.push(line);
            continue;
        }
        match split_paragraph_line(&line) {
            Some(parts) => {
                // The split ends the paragraph this line belonged to, so a
                // running paragraph above it needs its own terminator.
                if out.last().is_some_and(|prev| !prev.trim().is_empty()) {
                    out.push(String::new());
                }
                out.extend(parts);
            }
            None => out.push(line),
        }
    }

    let normalized = out.join("\n");
    if normalized == markdown {
        markdown.to_string()
    } else {
        normalized
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

// -- 1. HTML entities ------------------------------------------------------

/// Decoded value per entity reference, so a document repeating `&nbsp;` on
/// every blank line only pays for one comrak parse.
#[derive(Default)]
struct EntityCache(HashMap<String, Option<String>>);

impl EntityCache {
    /// The character(s) `reference` (`&nbsp;`, `&#160;`, …) stands for, or
    /// `None` when it is not a recognized entity and stays literal text.
    fn decode(&mut self, reference: &str) -> Option<&str> {
        if !self.0.contains_key(reference) {
            let decoded = decode_entity_with_comrak(reference);
            self.0.insert(reference.to_string(), decoded);
        }
        self.0[reference].as_deref()
    }
}

/// Ask comrak what an entity reference decodes to. The reference is pure
/// `&[#0-9A-Za-z]+;`, which carries no markdown meaning, so the text of the
/// parsed document IS the decoded value — and it is decoded by exactly the
/// table the block editor uses.
fn decode_entity_with_comrak(reference: &str) -> Option<String> {
    fn collect_text<'a>(node: &'a comrak::nodes::AstNode<'a>, out: &mut String) {
        if let comrak::nodes::NodeValue::Text(text) = &node.data.borrow().value {
            out.push_str(text);
        }
        for child in node.children() {
            collect_text(child, out);
        }
    }
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, reference, &comrak::Options::default());
    let mut text = String::new();
    collect_text(root, &mut text);
    (text != reference).then_some(text)
}

/// Replace every entity reference on `line` outside its code spans.
fn decode_entities(line: &str, cache: &mut EntityCache) -> String {
    if !line.contains('&') {
        return line.to_string();
    }
    let code_spans = code_span_ranges(line);
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(offset) = line[cursor..].find('&') {
        let start = cursor + offset;
        out.push_str(&line[cursor..start]);
        let Some(end) = entity_reference_end(bytes, start) else {
            out.push('&');
            cursor = start + 1;
            continue;
        };
        // Entity references are not recognized inside a code span.
        if code_spans.iter().any(|span| span.contains(&start)) {
            out.push_str(&line[start..end]);
            cursor = end;
            continue;
        }
        match cache.decode(&line[start..end]) {
            Some(decoded) => out.push_str(decoded),
            None => out.push_str(&line[start..end]),
        }
        cursor = end;
    }
    out.push_str(&line[cursor..]);
    out
}

/// End offset of the `&…;` reference starting at `start`, if the shape could
/// be one at all (a named, decimal, or hexadecimal reference).
fn entity_reference_end(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'&');
    let mut index = start + 1;
    // CommonMark caps names at 32 characters and numerics at 7 digits; a
    // generous bound just keeps a stray `&` from scanning the whole line.
    let limit = (index + 34).min(bytes.len());
    if bytes.get(index) == Some(&b'#') {
        index += 1;
        if matches!(bytes.get(index), Some(b'x') | Some(b'X')) {
            index += 1;
        }
    }
    let value_start = index;
    while index < limit && bytes[index].is_ascii_alphanumeric() {
        index += 1;
    }
    (index > value_start && bytes.get(index) == Some(&b';')).then_some(index + 1)
}

/// The save-side counterpart of the `&nbsp;` decode — the ONE transform that
/// has to run on the way back OUT of the vendored editor.
///
/// The engine treats U+00A0 as whitespace and trims it, so a decoded
/// blank-line paragraph reaches the serializer EMPTY and comes out as bare
/// blank lines. Those parse as nothing at all on every client, which would
/// quietly delete the user's blank line on the next save. The engine writes a
/// run of `2n + 1` blank lines for `n` interior empty paragraphs (and it is a
/// fixpoint on its own output), so each one is written back as the `&nbsp;`
/// line the contract stores — exactly what web's `MarkdownParagraph`
/// serializer does. Leading and trailing empties are separators, not
/// paragraphs, and stay blank — and so are blank lines inside code (a fence's
/// interior, tracked with the same [`fence_opener`]/[`closes_fence`] scan
/// [`normalize_for_wysiwyg`] uses, or between two indented-code chunks): those
/// are code bytes the serializer emitted verbatim, never trimmed paragraphs.
pub fn restore_blank_line_markers(markdown: &str) -> String {
    const MARKER: &str = "&nbsp;";

    let lines: Vec<&str> = markdown.split('\n').collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut index = 0;
    let mut changed = false;
    let mut fence: Option<(u8, usize)> = None;
    while index < lines.len() {
        if let Some((ch, len)) = fence {
            if closes_fence(lines[index], ch, len) {
                fence = None;
            }
            out.push(lines[index]);
            index += 1;
            continue;
        }
        if !lines[index].is_empty() {
            fence = fence_opener(lines[index]);
            out.push(lines[index]);
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && lines[index].is_empty() {
            index += 1;
        }
        let run = index - start;
        let interior = start > 0 && index < lines.len();
        if interior
            && run >= 3
            && run % 2 == 1
            && !blank_run_inside_indented_code(&lines, start, index)
        {
            changed = true;
            for step in 0..run {
                out.push(if step % 2 == 0 { "" } else { MARKER });
            }
        } else {
            out.extend(std::iter::repeat_n("", run));
        }
    }

    if changed {
        out.join("\n")
    } else {
        markdown.to_string()
    }
}

/// Whether the blank run `start..end` sits between two indented-code lines.
/// CommonMark folds such blanks into ONE indented code block, so they are
/// code bytes, not trimmed empty paragraphs.
fn blank_run_inside_indented_code(lines: &[&str], start: usize, end: usize) -> bool {
    let indented = |line: &str| !line.is_empty() && strip_block_indent(line).is_none();
    start > 0 && end < lines.len() && indented(lines[start - 1]) && indented(lines[end])
}

// -- 2. Inline images ------------------------------------------------------

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
    use super::normalize_for_wysiwyg as normalize;

    #[track_caller]
    fn assert_unchanged(markdown: &str) {
        assert_eq!(normalize(markdown), markdown, "should be left byte-identical");
    }

    // -- 1. HTML entities --------------------------------------------------

    #[test]
    fn decodes_the_blank_line_marker_every_other_client_understands() {
        // `&nbsp;` is the interchange form of an intentional blank line.
        assert_eq!(normalize("a\n\n&nbsp;\n\nb"), "a\n\n\u{a0}\n\nb");
    }

    #[test]
    fn decodes_named_numeric_and_hex_references() {
        assert_eq!(normalize("laptop =&gt; no internet"), "laptop => no internet");
        assert_eq!(normalize("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(normalize("&copy; 2026"), "© 2026");
        assert_eq!(normalize("a &#160; b"), "a \u{a0} b");
        assert_eq!(normalize("a &#XA0; b"), "a \u{a0} b");
        assert_eq!(normalize("wait&hellip;"), "wait…");
    }

    #[test]
    fn decodes_entities_outside_paragraphs_too() {
        assert_eq!(normalize("# Tom &amp; Jerry"), "# Tom & Jerry");
        assert_eq!(normalize("- a &amp; b"), "- a & b");
        assert_eq!(normalize("> a &amp; b"), "> a & b");
        assert_eq!(normalize("| a &amp; b | c |"), "| a & b | c |");
    }

    #[test]
    fn leaves_non_entities_and_bare_ampersands_alone() {
        assert_unchanged("a &notanentity; b");
        assert_unchanged("Tom & Jerry");
        assert_unchanged("a &; b");
        assert_unchanged("a &#; b");
        assert_unchanged("query?a=1&b=2");
    }

    #[test]
    fn never_decodes_entities_inside_code() {
        assert_unchanged("write `&nbsp;` for a blank line");
        assert_unchanged("```\n&nbsp;\n```");
        assert_unchanged("~~~html\n&amp;\n~~~");
        assert_unchanged("    &nbsp;");
    }

    #[test]
    fn a_decoded_block_marker_reclassifies_its_own_line() {
        // `&gt;` decodes to a blockquote marker, so the line stops being a
        // paragraph the image split may touch.
        assert_eq!(
            normalize("&gt; quoted ![alt](/api/attachments/abc) text"),
            "> quoted ![alt](/api/attachments/abc) text"
        );
    }

    #[test]
    fn writes_trimmed_blank_paragraphs_back_as_markers() {
        use super::restore_blank_line_markers as restore;

        // The engine writes `2n + 1` blank lines for n empty paragraphs.
        assert_eq!(restore("A\n\n\n\nB"), "A\n\n&nbsp;\n\nB");
        assert_eq!(restore("A\n\n\n\n\n\nB"), "A\n\n&nbsp;\n\n&nbsp;\n\nB");
        // An ordinary paragraph separator is not a paragraph.
        assert_eq!(restore("A\n\nB"), "A\n\nB");
        assert_eq!(restore("A\nB"), "A\nB");
        // Leading and trailing blanks are separators, not content.
        assert_eq!(restore("\n\n\n\nA"), "\n\n\n\nA");
        assert_eq!(restore("A\n\n\n\n"), "A\n\n\n\n");
        assert_eq!(restore(""), "");
    }

    #[test]
    fn never_rewrites_blank_lines_inside_code_blocks() {
        use super::restore_blank_line_markers as restore;

        // An odd run of blank lines inside a fence is code the serializer
        // emitted verbatim — injecting the marker would corrupt the block.
        assert_eq!(restore("```\na\n\n\n\nb\n```"), "```\na\n\n\n\nb\n```");
        assert_eq!(
            restore("~~~text\na\n\n\n\n\n\nb\n~~~"),
            "~~~text\na\n\n\n\n\n\nb\n~~~"
        );
        // An unclosed fence runs to the end of the document.
        assert_eq!(restore("```\na\n\n\n\nb"), "```\na\n\n\n\nb");
        // Blank lines between indented-code chunks belong to ONE code block.
        assert_eq!(restore("    a\n\n\n\n    b"), "    a\n\n\n\n    b");
    }

    #[test]
    fn still_writes_markers_outside_a_fenced_block() {
        use super::restore_blank_line_markers as restore;

        // Runs before and after the fence get the marker treatment; the run
        // inside it stays untouched.
        assert_eq!(
            restore("A\n\n\n\nB\n\n```\na\n\n\n\nb\n```\n\n\n\nC"),
            "A\n\n&nbsp;\n\nB\n\n```\na\n\n\n\nb\n```\n\n&nbsp;\n\nC"
        );
    }

    #[test]
    fn the_blank_line_marker_survives_the_full_round_trip() {
        use super::restore_blank_line_markers as restore;

        // What the editor does end to end: decode in, engine trims the
        // U+00A0 paragraph to blank lines, marker written back out.
        assert_eq!(normalize("A\n\n&nbsp;\n\nB"), "A\n\n\u{a0}\n\nB");
        assert_eq!(restore("A\n\n\n\nB"), "A\n\n&nbsp;\n\nB");
    }

    // -- 2. Inline images --------------------------------------------------

    #[test]
    fn lifts_the_shape_web_actually_stored() {
        // EXP-249's real description: the image welded onto the paragraph.
        assert_eq!(
            normalize("![](/api/attachments/abc)on mobile steering the plan.\n\nalso when approving"),
            "![](/api/attachments/abc)\n\non mobile steering the plan.\n\nalso when approving"
        );
    }

    #[test]
    fn lifts_an_image_from_the_middle_of_a_sentence() {
        assert_eq!(
            normalize("before ![alt](/api/attachments/abc) after"),
            "before\n\n![alt](/api/attachments/abc)\n\nafter"
        );
    }

    #[test]
    fn lifts_several_images_from_one_line() {
        assert_eq!(
            normalize("![a](/api/attachments/a)mid![b](/api/attachments/b)end"),
            "![a](/api/attachments/a)\n\nmid\n\n![b](/api/attachments/b)\n\nend"
        );
    }

    #[test]
    fn terminates_the_paragraph_the_image_line_continued() {
        assert_eq!(
            normalize("some text\n![alt](/api/attachments/abc) more"),
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
            normalize("`![a](/api/attachments/a)` and ![b](/api/attachments/b) end"),
            "`![a](/api/attachments/a)` and\n\n![b](/api/attachments/b)\n\nend"
        );
    }

    #[test]
    fn resumes_splitting_after_a_fenced_block_closes() {
        assert_eq!(
            normalize("```\n![a](/api/attachments/a)x\n```\n\n![b](/api/attachments/b)y"),
            "```\n![a](/api/attachments/a)x\n```\n\n![b](/api/attachments/b)\n\ny"
        );
    }

    #[test]
    fn is_idempotent() {
        let once = normalize("before ![alt](/api/attachments/abc) after");
        assert_eq!(normalize(&once), once);
    }

    // -- both together -----------------------------------------------------

    #[test]
    fn normalizes_the_real_exp_249_description() {
        let stored = "![](/api/attachments/457f10aa)on mobile steering the plan.\n\n&nbsp;\n\nclose the laptop =&gt; no internet.";
        assert_eq!(
            normalize(stored),
            "![](/api/attachments/457f10aa)\n\non mobile steering the plan.\n\n\u{a0}\n\nclose the laptop => no internet."
        );
        // …and settling there: a second pass is a fixpoint.
        let once = normalize(stored);
        assert_eq!(normalize(&once), once);
    }

    #[test]
    fn handles_multibyte_prose() {
        assert_eq!(
            normalize("Grüße ![münchen](/api/attachments/abc) 🚀"),
            "Grüße\n\n![münchen](/api/attachments/abc)\n\n🚀"
        );
    }
}
