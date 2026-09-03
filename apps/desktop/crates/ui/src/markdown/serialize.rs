//! [`ContentBlock`]s → GFM markdown — the save path, where byte parity with
//! the web (tiptap-markdown), iOS (cmark-gfm) and Android clients matters.
//! Ports iOS `blocksToMarkdown` / `attributedStringToMarkdown` /
//! `extractInlineMarkdown` via the Android `MarkdownSerializer.kt` port,
//! producing the canonical forms: bullet marker `-`, tight lists (single
//! `\n` between items), `\n\n` between blocks, `**`/`*`/`~~`/`***`
//! delimiters, ATX headings, fenced code blocks, and `![alt](url)` images.

use super::blocks::{
    BlockKind, ContentBlock, InlineKind, InlineMark, ListType, ParagraphAttrs, RichText,
    TableAlignment,
};

/// The GFM interchange form of an intentional blank line — a paragraph
/// holding only a no-break space, written as the entity so it survives every
/// client's parser as a visually empty paragraph (EXP-7 on web, EXP-689 on
/// the natives; the WYSIWYG path writes the same marker via
/// `restore_blank_line_markers`).
pub const BLANK_LINE_MARKER: &str = "&nbsp;";

/// Serialize the block model to canonical GFM.
pub fn blocks_to_markdown(blocks: &[ContentBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text { content, .. } => {
                let md = serialize_text(content);
                if !md.is_empty() {
                    parts.push(md);
                }
            }
            ContentBlock::Image { url, alt, .. } => {
                parts.push(format!("![{alt}]({url})"));
            }
            ContentBlock::Table {
                header,
                rows,
                alignments,
                ..
            } => {
                parts.push(serialize_table(header, rows, alignments));
            }
        }
    }
    parts.join("\n\n")
}

// -- Table block (EXP-726) ------------------------------------------------

/// The canonical cross-client pipe table: one space each side of every cell,
/// NO column-width padding, `---` / `:---` / `:---:` / `---:` delimiters, an
/// empty cell as `|  |`, rows joined by a single `\n`. The blank lines around
/// it fall out of `parts.join("\n\n")` above.
fn serialize_table(
    header: &[RichText],
    rows: &[Vec<RichText>],
    alignments: &[TableAlignment],
) -> String {
    let columns = alignments.len().max(header.len()).max(1);
    let mut lines = Vec::with_capacity(2 + rows.len());
    lines.push(serialize_table_row(header, columns));
    lines.push(format!(
        "| {} |",
        (0..columns)
            .map(|column| match alignments.get(column).copied().unwrap_or_default() {
                TableAlignment::None => "---",
                TableAlignment::Left => ":---",
                TableAlignment::Center => ":---:",
                TableAlignment::Right => "---:",
            })
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.extend(rows.iter().map(|row| serialize_table_row(row, columns)));
    lines.join("\n")
}

fn serialize_table_row(cells: &[RichText], columns: usize) -> String {
    let rendered = (0..columns)
        .map(|column| match cells.get(column) {
            Some(cell) => serialize_table_cell(cell),
            None => String::new(),
        })
        .collect::<Vec<_>>();
    format!("| {} |", rendered.join(" | "))
}

/// A cell is ONE inline paragraph. Only GFM's two cell-level rewrites apply:
/// an unescaped `|` would end the cell and a newline would end the row (the
/// parser already collapses in-cell breaks to spaces, so the `\n` guard is
/// belt-and-braces for hand-built blocks).
fn serialize_table_cell(cell: &RichText) -> String {
    inline(&cell.text, &cell.marks, false)
        .replace('|', "\\|")
        .replace('\n', " ")
}

// -- Text block ----------------------------------------------------------

fn serialize_text(rich: &RichText) -> String {
    let lines = rich.lines();
    if lines.len() == 1 && lines[0].is_empty() {
        return String::new();
    }
    let plain = ParagraphAttrs::PLAIN;
    let attrs: Vec<&ParagraphAttrs> = (0..lines.len())
        .map(|i| rich.paragraphs.get(i).unwrap_or(&plain))
        .collect();

    // Per-line inline marks, offset to line-local byte coordinates.
    let mut line_marks: Vec<Vec<InlineMark>> = Vec::with_capacity(lines.len());
    let mut char_start = 0usize;
    for line in &lines {
        let line_start = char_start;
        let line_end = char_start + line.len();
        let local: Vec<InlineMark> = rich
            .marks
            .iter()
            .filter_map(|m| {
                let s = m.start.max(line_start);
                let e = m.end.min(line_end);
                if e > s {
                    Some(InlineMark {
                        start: s - line_start,
                        end: e - line_start,
                        kind: m.kind,
                        href: m.href.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        line_marks.push(local);
        char_start = line_end + 1; // + '\n'
    }

    // Group consecutive code-block lines into single fenced segments; every
    // other line is its own segment.
    let all_segments = segment(&attrs);
    // EXP-689: an intentional blank line (two Enters) is an empty plain
    // paragraph. GFM cannot carry one as bare newlines — every parser folds
    // `A\n\n\n\nB` into `A\n\nB` — so INTERIOR blank paragraphs are written
    // as the contract's `&nbsp;` line (web's MarkdownParagraph does exactly
    // this) and leading/trailing ones are dropped as meaningless spacing.
    // Blank lines inside a fence are code, untouched.
    let is_blank_paragraph = |seg: &Segment| {
        !seg.is_code
            && attrs[seg.start_line].kind == BlockKind::Paragraph
            && lines[seg.start_line].trim().is_empty()
    };
    let Some(first_content) = all_segments.iter().position(|seg| !is_blank_paragraph(seg)) else {
        return String::new();
    };
    let last_content = all_segments
        .iter()
        .rposition(|seg| !is_blank_paragraph(seg))
        .unwrap_or(first_content);
    let segments = &all_segments[first_content..=last_content];
    let mut out = String::new();
    for (seg_index, seg) in segments.iter().enumerate() {
        if seg_index > 0 {
            let prev = &segments[seg_index - 1];
            let tight =
                attrs[prev.end_line].list_type.is_some() && attrs[seg.start_line].list_type.is_some();
            out.push_str(if tight { "\n" } else { "\n\n" });
        }
        if seg.is_code {
            let lang = attrs[seg.start_line].code_lang.as_deref().unwrap_or("");
            out.push_str("```");
            out.push_str(lang);
            out.push('\n');
            out.push_str(
                &(seg.start_line..=seg.end_line)
                    .map(|i| lines[i])
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            out.push_str("\n```");
        } else if is_blank_paragraph(seg) {
            out.push_str(BLANK_LINE_MARKER);
        } else {
            let i = seg.start_line;
            out.push_str(&serialize_line(lines[i], attrs[i], &line_marks[i]));
        }
    }
    out.trim().to_string()
}

struct Segment {
    start_line: usize,
    end_line: usize,
    is_code: bool,
}

fn segment(attrs: &[&ParagraphAttrs]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut i = 0;
    while i < attrs.len() {
        if attrs[i].kind == BlockKind::CodeBlock {
            let mut j = i;
            while j + 1 < attrs.len() && attrs[j + 1].kind == BlockKind::CodeBlock {
                j += 1;
            }
            segments.push(Segment {
                start_line: i,
                end_line: j,
                is_code: true,
            });
            i = j + 1;
        } else {
            segments.push(Segment {
                start_line: i,
                end_line: i,
                is_code: false,
            });
            i += 1;
        }
    }
    segments
}

fn serialize_line(line: &str, a: &ParagraphAttrs, marks: &[InlineMark]) -> String {
    match a.kind {
        BlockKind::Heading => {
            let level = a.heading_level.clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), inline(line, marks, true))
        }
        BlockKind::Blockquote => format!("> {}", inline(line, marks, false)),
        BlockKind::ListItem => {
            let indent = "  ".repeat(a.list_depth as usize);
            let prefix = match a.list_type {
                Some(ListType::Ordered) => format!("{}. ", a.ordered_index),
                Some(ListType::Checklist) => {
                    if a.checked {
                        "- [x] ".to_string()
                    } else {
                        "- [ ] ".to_string()
                    }
                }
                Some(ListType::Bullet) | None => "- ".to_string(),
            };
            format!("{indent}{prefix}{}", inline(line, marks, false))
        }
        // Re-emit the canonical `---` so a horizontal rule round-trips on all
        // clients (the in-editor glyph `───` is render-only).
        BlockKind::ThematicBreak => "---".to_string(),
        BlockKind::Paragraph | BlockKind::CodeBlock => inline(line, marks, false),
    }
}

// -- Inline marks --------------------------------------------------------

#[derive(PartialEq, Clone)]
struct RunFlags {
    code: bool,
    link: bool,
    href: Option<String>,
    bold: bool,
    italic: bool,
    strike: bool,
}

fn inline(text: &str, marks: &[InlineMark], is_heading: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    if marks.is_empty() {
        return text.to_string();
    }

    let n = text.len();
    let mut boundaries: Vec<usize> = vec![0, n];
    for m in marks {
        if m.start <= n {
            boundaries.push(m.start);
        }
        if m.end <= n {
            boundaries.push(m.end);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut out = String::new();
    let mut pending_text = String::new();
    let mut pending_flags: Option<RunFlags> = None;

    fn flush(
        out: &mut String,
        pending_text: &mut String,
        pending_flags: &mut Option<RunFlags>,
        is_heading: bool,
    ) {
        let Some(flags) = pending_flags.take() else {
            return;
        };
        let s = std::mem::take(pending_text);
        if s.is_empty() {
            return;
        }
        if flags.code {
            out.push('`');
            out.push_str(&s);
            out.push('`');
        } else if flags.link {
            out.push('[');
            out.push_str(&s);
            out.push_str("](");
            out.push_str(flags.href.as_deref().unwrap_or(""));
            out.push(')');
        } else {
            let mut t = s;
            if flags.strike {
                t = format!("~~{t}~~");
            }
            let bold = flags.bold && !is_heading;
            if bold && flags.italic {
                t = format!("***{t}***");
            } else if bold {
                t = format!("**{t}**");
            } else if flags.italic {
                t = format!("*{t}*");
            }
            out.push_str(&t);
        }
    }

    for k in 0..boundaries.len().saturating_sub(1) {
        let a = boundaries[k];
        let b = boundaries[k + 1];
        if b <= a {
            continue;
        }
        let active: Vec<&InlineMark> = marks
            .iter()
            .filter(|m| m.start <= a && m.end >= b)
            .collect();
        let link = active.iter().rev().find(|m| m.kind == InlineKind::Link);
        let flags = RunFlags {
            code: active.iter().any(|m| m.kind == InlineKind::InlineCode),
            link: link.is_some(),
            href: link.and_then(|m| m.href.clone()),
            bold: active.iter().any(|m| m.kind == InlineKind::Bold),
            italic: active.iter().any(|m| m.kind == InlineKind::Italic),
            strike: active.iter().any(|m| m.kind == InlineKind::Strikethrough),
        };
        if pending_flags.as_ref() != Some(&flags) {
            flush(&mut out, &mut pending_text, &mut pending_flags, is_heading);
            pending_flags = Some(flags);
            pending_text = String::new();
        }
        pending_text.push_str(&text[a..b]);
    }
    flush(&mut out, &mut pending_text, &mut pending_flags, is_heading);
    out
}

/// The byte-parity contract corpus: every canonical GFM string here must
/// survive `serialize(parse(md))` unchanged in BOTH desktop editors — the
/// block editor below (comrak + `blocks_to_markdown`) and the vendored
/// WYSIWYG editor (EXP-261, see `wysiwyg_parity.rs`). The strings mirror the
/// Android suite that locks the same cross-client contract
/// (`apps/android/.../MarkdownRoundTripTest.kt`); web (tiptap-markdown) and
/// iOS (cmark-gfm) honor it too. Add fixtures here, never in only one suite.
#[cfg(test)]
pub(crate) const CONTRACT_FIXTURES: &[(&str, &str)] = &[
    ("plain_paragraph", "Hello world"),
    ("bold", "This is **bold** text"),
    ("italic", "This is *italic* text"),
    ("bold_italic", "This is ***both*** text"),
    ("strikethrough", "This is ~~struck~~ text"),
    ("inline_code", "This is `code` text"),
    ("link", "A [link](https://example.com) here"),
    ("relative_link", "See [docs](/help/page) now"),
    ("heading1", "# Heading 1"),
    ("heading2", "## Heading 2"),
    ("heading3", "### Heading 3"),
    ("heading_then_paragraph", "# Title\n\nSome body text"),
    ("bullet_list", "- one\n- two\n- three"),
    ("ordered_list", "1. one\n2. two\n3. three"),
    ("task_list", "- [ ] todo\n- [x] done"),
    ("blockquote", "> quoted text"),
    ("code_block_with_lang", "```js\nconst x = 1\n```"),
    ("code_block_no_lang", "```\nplain code\n```"),
    ("multi_line_code_block", "```kotlin\nval a = 1\nval b = 2\n```"),
    ("block_image", "![diagram](/api/attachments/abc123)"),
    ("text_image_text", "before\n\n![alt](/api/attachments/abc)\n\nafter"),
    ("nested_bullet_list", "- parent\n  - child"),
    (
        "mixed_document",
        "# Title\n\nA paragraph with **bold**.\n\n- item 1\n- item 2\n\n> a quote",
    ),
    ("multiple_paragraphs", "First paragraph.\n\nSecond paragraph."),
    // EXP-689: intentional blank lines. GFM folds bare blank-line runs, so
    // the contract stores each interior empty paragraph as an `&nbsp;` line
    // (web MarkdownParagraph, EXP-7).
    ("blank_line_between_paragraphs", "First\n\n&nbsp;\n\nSecond"),
    (
        "two_blank_lines_between_paragraphs",
        "First\n\n&nbsp;\n\n&nbsp;\n\nSecond",
    ),
    ("bold_at_start", "**Bold** start"),
    ("multiple_marks_one_line", "A **bold** and *italic* and `code` mix"),
    ("mention_and_issue_ref", "cc @jane@example.com see #EXP-42"),
    ("bare_url_stays_bare", "see https://example.com here"),
    ("thematic_break", "---"),
    ("thematic_break_between_paragraphs", "before\n\n---\n\nafter"),
    ("unicode", "Grüße **münchen** 🚀 *ünïcodé*"),
    // Literal marker characters in prose must NOT get escaped on save
    // (EXP-261: locked when the vendored WYSIWYG serializer's eager
    // escaping was made context-sensitive).
    ("literal_asterisk_in_math", "2 * 3 = 6"),
    ("literal_tilde_in_prose", "approx ~5 items"),
    ("snake_case_identifier", "snake_case_name here"),
    ("literal_backslash", "back\\slash"),
    ("space_flanked_underscore", "underscore _ alone"),
    ("tilde_at_word_end", "tilde~end"),
    ("dollar_amount", "price is $5"),
    ("angle_brackets", "5 < 6 > 4"),
    ("brackets_without_url", "1 + 1 = 2 [not a link]"),
    // EXP-726: the GFM table contract. Byte-mirrored by the Android
    // `MarkdownRoundTripTest.kt`, the iOS table suite and the web
    // `markdown-table.test.ts` — add a fixture in all four or in none.
    ("table_basic", "| a | b |\n| --- | --- |\n| 1 | 2 |"),
    (
        "table_alignments",
        "| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |",
    ),
    (
        "table_inline_marks",
        "| **bold** | [link](https://example.com) |\n| --- | --- |\n| `code` | *em* |",
    ),
    ("table_escaped_pipe", "| a \\| b | c |\n| --- | --- |\n| 1 | 2 |"),
    ("table_empty_cell", "| a | b |\n| --- | --- |\n| 1 |  |"),
    ("table_header_only", "| a | b |\n| --- | --- |"),
    (
        "table_between_paragraphs",
        "before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter",
    ),
    (
        "table_chip_cells",
        "| @jane@example.com | #EXP-42 |\n| --- | --- |\n| x | y |",
    ),
    ("table_unicode", "| Grüße | 🚀 |\n| --- | --- |\n| ü | é |"),
];

#[cfg(test)]
mod tests {
    //! Byte-parity fixtures for the block markdown parser + serializer —
    //! **the Phase-3 markdown sub-gate** (masterplan-v3 §4.5). Each canonical
    //! GFM string must survive `serialize(parse(md))` unchanged. The corpus
    //! is ported verbatim from the Android suite that locks the same
    //! contract (`apps/android/.../MarkdownRoundTripTest.kt`), which the web
    //! (tiptap-markdown) and iOS (cmark-gfm) clients also honor.

    use super::super::parse::markdown_to_blocks;
    use super::*;

    fn round_trip(md: &str) -> String {
        blocks_to_markdown(&markdown_to_blocks(md))
    }

    #[track_caller]
    fn assert_stable(md: &str) {
        assert_eq!(round_trip(md), md, "round-trip diverged for {md:?}");
    }

    // The whole contract corpus must be stable through THIS (block editor)
    // pipeline — the same table drives the WYSIWYG suite in
    // `wysiwyg_parity.rs`, guaranteeing both editors lock identical bytes.
    #[test]
    fn contract_fixtures_stable() {
        for (name, md) in super::CONTRACT_FIXTURES {
            assert_eq!(&round_trip(md), md, "fixture {name} diverged");
        }
    }

    #[test]
    fn plain_paragraph() {
        assert_stable("Hello world");
    }

    // --- EXP-689: intentional blank lines. ---

    #[test]
    fn blank_line_parses_to_an_empty_editor_line() {
        // No invisible U+00A0 in the editor: the marker folds to an empty line.
        let blocks = super::super::parse::markdown_to_blocks("First\n\n&nbsp;\n\nSecond");
        assert_eq!(blocks.len(), 1);
        let ContentBlock::Text { content, .. } = &blocks[0] else {
            panic!("expected a text block");
        };
        assert_eq!(content.text, "First\n\nSecond");
    }

    #[test]
    fn editor_typed_blank_line_is_written_as_the_marker() {
        // Two Enters in the editor = an empty line inside the text block.
        let blocks = vec![ContentBlock::text(RichText::plain("First\n\nSecond"))];
        assert_eq!(blocks_to_markdown(&blocks), "First\n\n&nbsp;\n\nSecond");
    }

    #[test]
    fn leading_and_trailing_blank_lines_are_dropped() {
        let blocks = vec![ContentBlock::text(RichText::plain("\n\nOnly line\n"))];
        assert_eq!(blocks_to_markdown(&blocks), "Only line");
        assert_eq!(round_trip("&nbsp;\n\nOnly line\n\n&nbsp;"), "Only line");
    }

    #[test]
    fn literal_no_break_space_paragraph_converges_to_the_marker() {
        assert_eq!(
            round_trip("First\n\n\u{a0}\n\nSecond"),
            "First\n\n&nbsp;\n\nSecond"
        );
    }

    #[test]
    fn blank_lines_inside_a_fence_stay_code() {
        assert_stable("```\na\n\nb\n```");
    }

    #[test]
    fn bold() {
        assert_stable("This is **bold** text");
    }

    #[test]
    fn italic() {
        assert_stable("This is *italic* text");
    }

    #[test]
    fn bold_italic() {
        assert_stable("This is ***both*** text");
    }

    #[test]
    fn strikethrough() {
        assert_stable("This is ~~struck~~ text");
    }

    #[test]
    fn inline_code() {
        assert_stable("This is `code` text");
    }

    #[test]
    fn link() {
        assert_stable("A [link](https://example.com) here");
    }

    #[test]
    fn relative_link() {
        assert_stable("See [docs](/help/page) now");
    }

    #[test]
    fn heading1() {
        assert_stable("# Heading 1");
    }

    #[test]
    fn heading2() {
        assert_stable("## Heading 2");
    }

    #[test]
    fn heading3() {
        assert_stable("### Heading 3");
    }

    #[test]
    fn heading_then_paragraph() {
        assert_stable("# Title\n\nSome body text");
    }

    #[test]
    fn bullet_list() {
        assert_stable("- one\n- two\n- three");
    }

    #[test]
    fn ordered_list() {
        assert_stable("1. one\n2. two\n3. three");
    }

    #[test]
    fn task_list() {
        assert_stable("- [ ] todo\n- [x] done");
    }

    #[test]
    fn blockquote() {
        assert_stable("> quoted text");
    }

    #[test]
    fn code_block_with_lang() {
        assert_stable("```js\nconst x = 1\n```");
    }

    #[test]
    fn code_block_no_lang() {
        assert_stable("```\nplain code\n```");
    }

    #[test]
    fn multi_line_code_block() {
        assert_stable("```kotlin\nval a = 1\nval b = 2\n```");
    }

    #[test]
    fn block_image() {
        assert_stable("![diagram](/api/attachments/abc123)");
    }

    #[test]
    fn text_image_text() {
        assert_stable("before\n\n![alt](/api/attachments/abc)\n\nafter");
    }

    #[test]
    fn nested_bullet_list() {
        assert_stable("- parent\n  - child");
    }

    #[test]
    fn mixed_document() {
        assert_stable("# Title\n\nA paragraph with **bold**.\n\n- item 1\n- item 2\n\n> a quote");
    }

    #[test]
    fn multiple_paragraphs() {
        assert_stable("First paragraph.\n\nSecond paragraph.");
    }

    #[test]
    fn bold_at_start() {
        assert_stable("**Bold** start");
    }

    #[test]
    fn multiple_marks_one_line() {
        assert_stable("A **bold** and *italic* and `code` mix");
    }

    // --- Mentions + issue refs are plain GFM text (the interchange form). ---

    #[test]
    fn mention_and_issue_ref_round_trip() {
        assert_stable("cc @jane@example.com see #EXP-42");
    }

    // --- EXP-726: GFM tables. ---

    #[test]
    fn table_pipe_in_a_cell_is_escaped() {
        let blocks = markdown_to_blocks("| a \\| b | c |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(
            blocks_to_markdown(&blocks),
            "| a \\| b | c |\n| --- | --- |\n| 1 | 2 |"
        );
    }

    #[test]
    fn header_only_table_round_trips() {
        assert_stable("| a | b |\n| --- | --- |");
    }

    #[test]
    fn padded_github_style_table_converges_to_the_canonical_form() {
        // What GitHub's own editor (and most hand-authoring) produces: aligned
        // pipes, padded cells, ragged body rows, `:-:` delimiters.
        let padded = "| Name  | Score |\n|:------|------:|\n| Alice |    10 |\n| Bob   |";
        let once = round_trip(padded);
        assert_eq!(
            once,
            "| Name | Score |\n| :--- | ---: |\n| Alice | 10 |\n| Bob |  |"
        );
        // …and the canonical form is a fixpoint.
        assert_eq!(round_trip(&once), once);
    }

    #[test]
    fn a_table_cell_never_emits_a_newline() {
        // Hand-built blocks may carry one; the cell serializer flattens it so
        // the row can never be split in two.
        let blocks = vec![ContentBlock::table(
            vec![RichText::plain("a\nb"), RichText::plain("c")],
            Vec::new(),
            vec![
                super::super::blocks::TableAlignment::None,
                super::super::blocks::TableAlignment::None,
            ],
        )];
        assert_eq!(blocks_to_markdown(&blocks), "| a b | c |\n| --- | --- |");
    }

    #[test]
    fn table_is_separated_from_its_neighbours_by_blank_lines() {
        assert_stable("before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter");
    }

    // --- Idempotency: a second round-trip must equal the first. ---

    #[test]
    fn idempotent_mixed() {
        let once = round_trip("# T\n\ntext **b** *i*\n\n- a\n- b\n\n> q\n\n```js\nx\n```");
        assert_eq!(once, round_trip(&once));
    }

    // --- Normalization (intentionally lossy, matches iOS/Android). ---

    #[test]
    fn bold_suppressed_in_heading() {
        assert_eq!(round_trip("# **bold** title"), "# bold title");
    }

    #[test]
    fn blank_input_produces_empty() {
        assert_eq!(round_trip(""), "");
    }

    // --- Regression: bare URLs stay bare (no autolink — web parity). ---

    #[test]
    fn bare_url_stays_bare() {
        assert_stable("see https://example.com here");
    }

    // --- Regression: thematic break round-trips to canonical `---`. ---

    #[test]
    fn thematic_break_round_trips() {
        assert_eq!(round_trip("---"), "---");
    }

    #[test]
    fn thematic_break_between_paragraphs() {
        assert_eq!(round_trip("before\n\n---\n\nafter"), "before\n\n---\n\nafter");
    }

    // --- Unicode: mark offsets are bytes; multi-byte text must survive. ---

    #[test]
    fn unicode_round_trips() {
        assert_stable("Grüße **münchen** 🚀 *ünïcodé*");
    }

    // --- Soft breaks: READ semantics keep GFM's space (all four clients
    //     render a stored lone `\n` as a space — parity locked). ---

    #[test]
    fn read_path_soft_break_is_a_space() {
        assert_eq!(round_trip("line1\nline2"), "line1 line2");
    }

    // --- EXP-118: EDITOR-INPUT canonicalization — a plain Enter (a lone
    //     `\n` in the raw source blocks) means a paragraph break, matching
    //     what Enter produces on web (TipTap), iOS and Android. ---

    use super::super::parse::{markdown_to_blocks_with, SoftBreakMode};

    fn editor_round_trip(md: &str) -> String {
        blocks_to_markdown(&markdown_to_blocks_with(md, SoftBreakMode::ParagraphBreak))
    }

    #[test]
    fn editor_single_newline_becomes_paragraph_break() {
        assert_eq!(editor_round_trip("line1\nline2"), "line1\n\nline2");
    }

    #[test]
    fn editor_double_newline_stays_one_paragraph_break() {
        assert_eq!(editor_round_trip("line1\n\nline2"), "line1\n\nline2");
    }

    #[test]
    fn editor_code_fence_content_untouched() {
        assert_eq!(
            editor_round_trip("```js\nconst a = 1\nconst b = 2\n```"),
            "```js\nconst a = 1\nconst b = 2\n```"
        );
    }

    #[test]
    fn editor_tight_list_untouched() {
        assert_eq!(editor_round_trip("- one\n- two\n- three"), "- one\n- two\n- three");
    }

    // A lazy continuation (Enter inside a list item WITHOUT typing a marker)
    // keeps GFM's join — splitting would mint a bullet/ordinal the user
    // never wrote.
    #[test]
    fn editor_list_lazy_continuation_joins() {
        assert_eq!(editor_round_trip("- one\ntwo"), "- one two");
        assert_eq!(editor_round_trip("1. one\ntwo"), "1. one two");
    }

    // Hard breaks inside ORDERED items must split into canonically numbered
    // siblings — a cloned index (`1. one\n1. two`) phantom-diffs on every
    // client's next re-save (read path shares the LineBreak arm).
    #[test]
    fn hard_break_in_ordered_item_renumbers_canonically() {
        assert_eq!(round_trip("1. one  \ntwo"), "1. one\n2. two");
        let once = round_trip("1. one  \ntwo\n2. three");
        assert_eq!(once, round_trip(&once), "hard-break split must be idempotent");
    }

    #[test]
    fn editor_blockquote_lines_split() {
        assert_eq!(editor_round_trip("> a\n> b"), "> a\n\n> b");
    }

    #[test]
    fn editor_marks_survive_the_split() {
        assert_eq!(
            editor_round_trip("**bold\nstill bold**"),
            "**bold**\n\n**still bold**"
        );
    }

    #[test]
    fn editor_canonical_output_is_idempotent() {
        let once = editor_round_trip("# T\n\nline1\nline2\n\n- a\n- b\n\n```js\nx\ny\n```");
        assert_eq!(once, editor_round_trip(&once));
        // And the READ path must not change it further — what the editor
        // saves is exactly what every client re-serializes.
        assert_eq!(once, round_trip(&once));
    }
}
