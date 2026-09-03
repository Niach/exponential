//! The block document model of the GFM markdown editor (masterplan-v3 §4.5).
//!
//! A faithful Rust port of the proven iOS `ContentBlock` /
//! `MarkdownConversion.swift` model and its Android twin
//! (`apps/android/.../ui/markdown/model/ContentBlock.kt`): **only images and
//! tables split the document into blocks** (EXP-726 added tables). Headings,
//! lists, quotes and fenced code become
//! *paragraph-level attributes* inside a single [`ContentBlock::Text`];
//! inline formatting is a list of [`InlineMark`] ranges. Markdown is derived
//! from blocks only at save time — never round-tripped per keystroke.
//!
//! Offsets: [`InlineMark`] ranges and paragraph boundaries are **byte offsets
//! into [`RichText::text`]**, always on char boundaries. (iOS uses UTF-16 and
//! Android UTF-16 code units — the unit is an internal detail; the byte-parity
//! contract lives at the serialized-markdown level, not the offset level.)

use std::sync::atomic::{AtomicU64, Ordering};

/// Marker glyph used in place of a `---` thematic break inside the editable
/// text (render-only; the serializer re-emits the canonical `---`).
pub const THEMATIC_BREAK_GLYPH: &str = "───";

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

/// Process-unique id for stable gpui `ElementId`s across re-renders.
pub fn next_block_id() -> u64 {
    NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed)
}

/// Paragraph-level block kind (the Rust analog of iOS's `markdown*`
/// paragraph attribute keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockKind {
    #[default]
    Paragraph,
    Heading,
    ListItem,
    Blockquote,
    CodeBlock,
    ThematicBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    Bullet,
    Ordered,
    Checklist,
}

/// Attributes of one `'\n'`-delimited paragraph of a [`RichText`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParagraphAttrs {
    pub kind: BlockKind,
    /// 1..=6 when `kind == Heading` (the toolbar only emits 1..=3).
    pub heading_level: u8,
    /// Set when `kind == ListItem`.
    pub list_type: Option<ListType>,
    /// Visible number for ordered list items (0 otherwise).
    pub ordered_index: u32,
    /// 0-based nesting depth of a list item.
    pub list_depth: u32,
    /// Checklist state.
    pub checked: bool,
    /// Fence info for code blocks (`None` = no language).
    pub code_lang: Option<String>,
}

impl ParagraphAttrs {
    pub const PLAIN: ParagraphAttrs = ParagraphAttrs {
        kind: BlockKind::Paragraph,
        heading_level: 0,
        list_type: None,
        ordered_index: 0,
        list_depth: 0,
        checked: false,
        code_lang: None,
    };
}

/// Inline formatting kind. **No underline on purpose** — it has no GFM
/// representation and does not round-trip (CLAUDE.md contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    Bold,
    Italic,
    Strikethrough,
    InlineCode,
    Link,
}

/// An inline mark over the `[start, end)` byte range of [`RichText::text`].
#[derive(Debug, Clone, PartialEq)]
pub struct InlineMark {
    pub start: usize,
    pub end: usize,
    pub kind: InlineKind,
    /// Set when `kind == Link` — the RAW destination, re-emitted verbatim
    /// (relative `/api/...` links stay relative, round-trip safe).
    pub href: Option<String>,
}

impl InlineMark {
    pub fn new(start: usize, end: usize, kind: InlineKind) -> Self {
        Self {
            start,
            end,
            kind,
            href: None,
        }
    }
}

/// A text block's content: the raw editable string plus a parallel
/// per-paragraph attribute list and per-range inline marks.
///
/// Invariant: paragraphs are the `'\n'`-delimited lines of `text`, so
/// `text.split('\n').count() == paragraphs.len()`. Blank lines never appear
/// inside a text block — block separators are stored as a single `'\n'` and
/// re-expanded to `"\n\n"` only at serialize time, exactly like iOS/Android.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichText {
    pub text: String,
    pub paragraphs: Vec<ParagraphAttrs>,
    pub marks: Vec<InlineMark>,
}

impl RichText {
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            paragraphs: vec![ParagraphAttrs::PLAIN],
            marks: Vec::new(),
        }
    }

    /// Build a mark-less RichText whose paragraphs are all plain.
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let count = if text.is_empty() {
            1
        } else {
            text.split('\n').count()
        };
        Self {
            text,
            paragraphs: vec![ParagraphAttrs::PLAIN; count],
            marks: Vec::new(),
        }
    }

    /// The `'\n'`-delimited lines; always at least one entry.
    pub fn lines(&self) -> Vec<&str> {
        if self.text.is_empty() {
            vec![""]
        } else {
            self.text.split('\n').collect()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Column alignment declared by a GFM table's delimiter row (EXP-726). The
/// unmarked form (`---`) stays distinct from an explicit `:---` so a column
/// nobody aligned is never rewritten with a colon on the next save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableAlignment {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// One block of the document. Images and tables split blocks.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    Text {
        id: u64,
        content: RichText,
    },
    Image {
        id: u64,
        /// Either a `draft://<uuid>` staging placeholder or the canonical
        /// relative `/api/attachments/{id}` form.
        url: String,
        alt: String,
    },
    /// A GFM pipe table (EXP-726). Row 0 is ALWAYS the header, so `rows` holds
    /// only the body; `header`, every body row and `alignments` are all
    /// `alignments.len()` wide (parse pads short rows and drops extra cells).
    /// Cells reuse [`RichText`] so marks and decoration pills apply unchanged,
    /// but each holds exactly ONE inline paragraph — a `\n` inside a cell has
    /// no GFM spelling.
    Table {
        id: u64,
        header: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
        alignments: Vec<TableAlignment>,
    },
}

impl ContentBlock {
    pub fn text(content: RichText) -> Self {
        Self::Text {
            id: next_block_id(),
            content,
        }
    }

    pub fn image(url: impl Into<String>, alt: impl Into<String>) -> Self {
        Self::Image {
            id: next_block_id(),
            url: url.into(),
            alt: alt.into(),
        }
    }

    pub fn table(
        header: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
        alignments: Vec<TableAlignment>,
    ) -> Self {
        Self::Table {
            id: next_block_id(),
            header,
            rows,
            alignments,
        }
    }

    pub fn id(&self) -> u64 {
        match self {
            Self::Text { id, .. } | Self::Image { id, .. } | Self::Table { id, .. } => *id,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// True for the blocks that are NOT editable text — the ones
    /// [`normalize_blocks`] pads with text neighbours.
    pub fn is_block_level(&self) -> bool {
        matches!(self, Self::Image { .. } | Self::Table { .. })
    }
}

/// Enforce the structural invariants of a block document — a verbatim port of
/// iOS `ContentBlock.normalize` / Android `normalizeBlocks`:
///
/// 1. An empty document becomes exactly one empty text block.
/// 2. The first block is always a text block.
/// 3. The last block is always a text block.
/// 4. No two block-level blocks (image, table) are adjacent.
///
/// These guarantee every image and table has a text block above and below it,
/// so backspace merges and caret placement always have somewhere to land.
pub fn normalize_blocks(blocks: &mut Vec<ContentBlock>) {
    if blocks.is_empty() {
        blocks.push(ContentBlock::text(RichText::empty()));
        return;
    }
    if blocks.first().is_some_and(ContentBlock::is_block_level) {
        blocks.insert(0, ContentBlock::text(RichText::empty()));
    }
    if blocks.last().is_some_and(ContentBlock::is_block_level) {
        blocks.push(ContentBlock::text(RichText::empty()));
    }
    let mut i = 1;
    while i < blocks.len() {
        if blocks[i].is_block_level() && blocks[i - 1].is_block_level() {
            blocks.insert(i, ContentBlock::text(RichText::empty()));
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_empty_produces_one_text_block() {
        let mut blocks = Vec::new();
        normalize_blocks(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], ContentBlock::Text { content, .. } if content.is_empty()));
    }

    #[test]
    fn normalize_wraps_images_with_text_blocks() {
        let mut blocks = vec![
            ContentBlock::image("/api/attachments/a", "a"),
            ContentBlock::image("/api/attachments/b", "b"),
        ];
        normalize_blocks(&mut blocks);
        // text, image, text, image, text
        assert_eq!(blocks.len(), 5);
        assert!(!blocks[0].is_image());
        assert!(blocks[1].is_image());
        assert!(!blocks[2].is_image());
        assert!(blocks[3].is_image());
        assert!(!blocks[4].is_image());
    }

    #[test]
    fn normalize_wraps_tables_with_text_blocks() {
        let mut blocks = vec![
            ContentBlock::table(vec![RichText::plain("a")], Vec::new(), vec![TableAlignment::None]),
            ContentBlock::image("/api/attachments/a", "a"),
        ];
        normalize_blocks(&mut blocks);
        // text, table, text, image, text
        assert_eq!(blocks.len(), 5);
        assert!(!blocks[0].is_block_level());
        assert!(matches!(&blocks[1], ContentBlock::Table { .. }));
        assert!(!blocks[2].is_block_level());
        assert!(blocks[3].is_image());
        assert!(!blocks[4].is_block_level());
    }

    #[test]
    fn rich_text_lines_always_at_least_one() {
        assert_eq!(RichText::empty().lines(), vec![""]);
        assert_eq!(RichText::plain("a\nb").lines(), vec!["a", "b"]);
        assert_eq!(RichText::plain("a\nb").paragraphs.len(), 2);
    }
}
