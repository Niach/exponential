//! GFM markdown → [`ContentBlock`]s by walking the comrak AST by hand — a
//! faithful port of iOS `renderNodeToBlocks` / `BlockCollector` /
//! `RenderContext` (`apps/ios/ExpUI/Sources/MarkdownConversion.swift`) and its
//! Android twin (`ui/markdown/MarkdownParser.kt`). comrak was chosen over
//! pulldown-cmark for parity-fidelity with iOS's cmark-gfm (masterplan-v3
//! §12.6#1): it is a line-for-line port of cmark-gfm, so edge cases (text-node
//! consolidation, bracket fallback, fence handling) behave identically.
//!
//! Only images split blocks. Headings, lists, quotes and fenced code become
//! paragraph-level attributes inside a [`ContentBlock::Text`]. Task-list items
//! are detected manually (NOT via comrak's tasklist extension) so unchecked
//! boxes don't degrade to plain bullets — the same reasoning the iOS
//! implementation documents.
//!
//! NOTE: no autolink extension — web (tiptap-markdown) leaves bare URLs bare,
//! so autolinking here would rewrite `https://x` to `[https://x](https://x)`
//! on the next save, diverging the stored bytes from the web client.

use comrak::nodes::{AstNode, ListType as MdListType, NodeValue};
use comrak::{parse_document, Arena, Options};

use super::blocks::{
    normalize_blocks, BlockKind, ContentBlock, InlineKind, InlineMark, ListType, ParagraphAttrs,
    RichText, THEMATIC_BREAK_GLYPH,
};

/// How a GFM SOFT break (a lone `\n` inside a paragraph) is interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoftBreakMode {
    /// GFM read semantics — a space. The default for every render/read path:
    /// all four clients render stored soft breaks as spaces, and that parity
    /// must hold (CLAUDE.md interchange contract).
    #[default]
    Space,
    /// Editor-input semantics (EXP-118): a paragraph boundary. The desktop
    /// editor's text blocks hold raw GFM source where a plain Enter inserts a
    /// lone `\n` — a soft break GFM would collapse to a space at save time,
    /// silently discarding the user's line break. Enter means "new paragraph"
    /// on web (TipTap), iOS and Android (block editors both), so the save
    /// path splits here and serializes the canonical `\n\n` instead.
    ParagraphBreak,
}

/// Parse GFM markdown into the normalized block model (read semantics).
pub fn markdown_to_blocks(markdown: &str) -> Vec<ContentBlock> {
    markdown_to_blocks_with(markdown, SoftBreakMode::Space)
}

/// [`markdown_to_blocks`] with an explicit [`SoftBreakMode`] — only the
/// editor's save path passes [`SoftBreakMode::ParagraphBreak`].
pub fn markdown_to_blocks_with(markdown: &str, soft_breaks: SoftBreakMode) -> Vec<ContentBlock> {
    if markdown.trim().is_empty() {
        let mut blocks = vec![ContentBlock::text(RichText::empty())];
        normalize_blocks(&mut blocks);
        return blocks;
    }

    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    let doc = parse_document(&arena, markdown, &options);

    let mut collector = BlockCollector::default();
    let mut ctx = RenderContext {
        soft_breaks,
        ..RenderContext::default()
    };
    render_children(doc, &mut collector, &mut ctx);
    collector.finalize()
}

// --- A single paragraph (one '\n'-delimited line) under construction. ---
struct ParaBuild {
    attrs: ParagraphAttrs,
    text: String,
    marks: Vec<InlineMark>,
}

impl ParaBuild {
    fn new(attrs: ParagraphAttrs) -> Self {
        Self {
            attrs,
            text: String::new(),
            marks: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.text.len()
    }
}

struct OpenMark {
    kind: InlineKind,
    href: Option<String>,
    start: usize,
}

struct ListFrame {
    ordered: bool,
    item_index: u32,
    depth: u32,
}

#[derive(Default)]
struct RenderContext {
    list_stack: Vec<ListFrame>,
    in_blockquote: bool,
    /// Attrs the next `Paragraph` should adopt (set when a list item opens).
    pending_item_attrs: Option<ParagraphAttrs>,
    /// Strip the `[ ] `/`[x] ` task marker from the next `Text` literal.
    strip_task_prefix: bool,
    /// Soft-break interpretation (see [`SoftBreakMode`]).
    soft_breaks: SoftBreakMode,
}

#[derive(Default)]
struct BlockCollector {
    blocks: Vec<ContentBlock>,
    paras: Vec<ParaBuild>,
    open_marks: Vec<OpenMark>,
}

impl BlockCollector {
    fn current_para(&mut self) -> &mut ParaBuild {
        if self.paras.is_empty() {
            self.paras.push(ParaBuild::new(ParagraphAttrs::PLAIN));
        }
        self.paras.last_mut().expect("non-empty")
    }

    fn start_para(&mut self, attrs: ParagraphAttrs) {
        self.paras.push(ParaBuild::new(attrs));
    }

    fn append(&mut self, text: &str) {
        self.current_para().text.push_str(text);
    }

    // -- Mark stack --------------------------------------------------------

    fn push_mark(&mut self, kind: InlineKind, href: Option<String>) {
        let start = self.current_para().len();
        self.open_marks.push(OpenMark { kind, href, start });
    }

    fn pop_mark(&mut self, kind: InlineKind) {
        // Find and remove the nearest open mark of this kind.
        let Some(idx) = self.open_marks.iter().rposition(|m| m.kind == kind) else {
            return;
        };
        let mark = self.open_marks.remove(idx);
        let para = self.current_para();
        if para.len() > mark.start {
            para.marks.push(InlineMark {
                start: mark.start,
                end: para.len(),
                kind: mark.kind,
                href: mark.href,
            });
        }
    }

    /// Close all open marks at the end of a paragraph and reopen them at 0 in
    /// a fresh para (hard line break inside a styled run).
    fn break_para_preserving_marks(&mut self, attrs: ParagraphAttrs) {
        let para_len = self.current_para().len();
        let closes: Vec<InlineMark> = self
            .open_marks
            .iter()
            .filter(|m| para_len > m.start)
            .map(|m| InlineMark {
                start: m.start,
                end: para_len,
                kind: m.kind,
                href: m.href.clone(),
            })
            .collect();
        self.current_para().marks.extend(closes);
        self.start_para(attrs);
        for m in &mut self.open_marks {
            m.start = 0;
        }
    }

    // -- Text-block flushing ----------------------------------------------

    fn flush_text(&mut self) {
        // Drop a single trailing empty paragraph — the artifact of a block
        // separator before an image (or end of input). Mirrors iOS flushText
        // stripping one trailing '\n'.
        if self.paras.len() > 1
            && self
                .paras
                .last()
                .is_some_and(|p| p.text.is_empty() && p.marks.is_empty())
        {
            self.paras.pop();
        }
        let text = self
            .paras
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let attrs_list: Vec<ParagraphAttrs> = self.paras.iter().map(|p| p.attrs.clone()).collect();
        let mut marks = Vec::new();
        let mut offset = 0usize;
        for (i, p) in self.paras.iter().enumerate() {
            for m in &p.marks {
                marks.push(InlineMark {
                    start: m.start + offset,
                    end: m.end + offset,
                    kind: m.kind,
                    href: m.href.clone(),
                });
            }
            offset += p.text.len();
            if i < self.paras.len() - 1 {
                offset += 1; // the '\n' separator
            }
        }
        let rich = if attrs_list.is_empty() {
            RichText::empty()
        } else {
            RichText {
                text,
                paragraphs: attrs_list,
                marks,
            }
        };
        self.blocks.push(ContentBlock::text(rich));
        self.paras.clear();
    }

    fn emit_image(&mut self, url: String, alt: String) {
        self.flush_text();
        self.blocks.push(ContentBlock::image(url, alt));
    }

    fn emit_code_block(&mut self, literal: &str, lang: Option<&str>) {
        // Each source line of the fenced block becomes its own CodeBlock
        // paragraph; the serializer detects the consecutive run and emits a
        // single fence.
        let body = literal.strip_suffix('\n').unwrap_or(literal);
        for line in body.split('\n') {
            self.start_para(ParagraphAttrs {
                kind: BlockKind::CodeBlock,
                code_lang: lang.map(str::to_string),
                ..ParagraphAttrs::PLAIN
            });
            self.append(line);
        }
    }

    fn finalize(mut self) -> Vec<ContentBlock> {
        self.flush_text();
        let mut blocks = self.blocks;
        normalize_blocks(&mut blocks);
        blocks
    }
}

fn render_children<'a>(
    node: &'a AstNode<'a>,
    collector: &mut BlockCollector,
    ctx: &mut RenderContext,
) {
    for child in node.children() {
        visit(child, collector, ctx);
    }
}

/// Attrs for the paragraph created by splitting the current one at a line
/// break. Clones the current attrs; for an ORDERED list item the clone's
/// `ordered_index` is advanced (and the open list frame bumped past it) so
/// the split serializes as canonically numbered sibling items — a duplicated
/// index (`1. one\n1. two`) would be renumbered by the next parse on any
/// client, phantom-diffing untouched content.
fn split_attrs(collector: &mut BlockCollector, ctx: &mut RenderContext) -> ParagraphAttrs {
    let mut attrs = collector.current_para().attrs.clone();
    if attrs.kind == BlockKind::ListItem && attrs.list_type == Some(ListType::Ordered) {
        attrs.ordered_index += 1;
        if let Some(frame) = ctx.list_stack.last_mut() {
            frame.item_index = frame.item_index.max(attrs.ordered_index + 1);
        }
    }
    attrs
}

fn visit<'a>(node: &'a AstNode<'a>, collector: &mut BlockCollector, ctx: &mut RenderContext) {
    let value = &node.data.borrow().value;
    match value {
        NodeValue::Document => render_children(node, collector, ctx),

        NodeValue::Paragraph => {
            let attrs = ctx.pending_item_attrs.take().unwrap_or(if ctx.in_blockquote {
                ParagraphAttrs {
                    kind: BlockKind::Blockquote,
                    ..ParagraphAttrs::PLAIN
                }
            } else {
                ParagraphAttrs::PLAIN
            });
            collector.start_para(attrs);
            render_children(node, collector, ctx);
        }

        NodeValue::Heading(heading) => {
            collector.start_para(ParagraphAttrs {
                kind: BlockKind::Heading,
                heading_level: heading.level.clamp(1, 6),
                ..ParagraphAttrs::PLAIN
            });
            render_children(node, collector, ctx);
        }

        NodeValue::Text(literal) => {
            let mut literal: &str = literal;
            if ctx.strip_task_prefix {
                ctx.strip_task_prefix = false;
                for marker in TASK_MARKERS {
                    if let Some(rest) = literal.strip_prefix(marker) {
                        literal = rest;
                        break;
                    }
                }
            }
            collector.append(literal);
        }

        NodeValue::SoftBreak => match ctx.soft_breaks {
            SoftBreakMode::Space => collector.append(" "),
            // EXP-118: the LineBreak treatment below — a paragraph boundary
            // carrying the same attrs. Fence content is safe (comrak never
            // emits SoftBreak inside a CodeBlock literal) and marker-ed lists
            // (`- a\n- b`) parse as Items, not soft breaks. A soft break
            // INSIDE a list item (a lazy continuation — the user pressed
            // Enter without typing a marker) keeps GFM's join instead:
            // splitting would silently mint a new bullet/checkbox/ordinal out
            // of continuation text, a worse mutation than the joined line.
            SoftBreakMode::ParagraphBreak => {
                if collector.current_para().attrs.kind == BlockKind::ListItem {
                    collector.append(" ");
                } else {
                    let attrs = split_attrs(collector, ctx);
                    collector.break_para_preserving_marks(attrs);
                }
            }
        },

        NodeValue::LineBreak => {
            // A hard break becomes a paragraph boundary carrying the same
            // attrs, matching how iOS re-splits the run by line at serialize
            // time.
            let attrs = split_attrs(collector, ctx);
            collector.break_para_preserving_marks(attrs);
        }

        NodeValue::Strong => {
            collector.push_mark(InlineKind::Bold, None);
            render_children(node, collector, ctx);
            collector.pop_mark(InlineKind::Bold);
        }

        NodeValue::Emph => {
            collector.push_mark(InlineKind::Italic, None);
            render_children(node, collector, ctx);
            collector.pop_mark(InlineKind::Italic);
        }

        NodeValue::Strikethrough => {
            collector.push_mark(InlineKind::Strikethrough, None);
            render_children(node, collector, ctx);
            collector.pop_mark(InlineKind::Strikethrough);
        }

        NodeValue::Code(code) => {
            let start = collector.current_para().len();
            collector.append(&code.literal);
            let para = collector.current_para();
            let end = para.len();
            if end > start {
                para.marks
                    .push(InlineMark::new(start, end, InlineKind::InlineCode));
            }
        }

        NodeValue::Link(link) => {
            // Store the RAW destination so serialization re-emits it verbatim
            // (relative `/api/...` links stay relative — round-trip safe).
            collector.push_mark(InlineKind::Link, Some(link.url.clone()));
            render_children(node, collector, ctx);
            collector.pop_mark(InlineKind::Link);
        }

        NodeValue::Image(link) => {
            let alt = collect_text(node);
            collector.emit_image(link.url.clone(), alt);
        }

        NodeValue::CodeBlock(code_block) => {
            let lang = if code_block.info.trim().is_empty() {
                None
            } else {
                Some(code_block.info.as_str())
            };
            collector.emit_code_block(&code_block.literal, lang);
        }

        NodeValue::BlockQuote => {
            let prev = ctx.in_blockquote;
            ctx.in_blockquote = true;
            render_children(node, collector, ctx);
            ctx.in_blockquote = prev;
        }

        NodeValue::List(list) => {
            let ordered = list.list_type == MdListType::Ordered;
            let start = if ordered { list.start as u32 } else { 0 };
            let depth = ctx.list_stack.len() as u32;
            ctx.list_stack.push(ListFrame {
                ordered,
                item_index: start.max(1),
                depth,
            });
            render_children(node, collector, ctx);
            ctx.list_stack.pop();
        }

        NodeValue::Item(_) => {
            let (ordered, depth, index) = match ctx.list_stack.last() {
                Some(frame) => (frame.ordered, frame.depth, frame.item_index),
                None => (false, 0, 1),
            };
            let (is_task, checked) = task_item_state(node);
            let list_type = if is_task {
                ListType::Checklist
            } else if ordered {
                ListType::Ordered
            } else {
                ListType::Bullet
            };
            ctx.pending_item_attrs = Some(ParagraphAttrs {
                kind: BlockKind::ListItem,
                list_type: Some(list_type),
                ordered_index: if ordered { index } else { 0 },
                list_depth: depth,
                checked,
                ..ParagraphAttrs::PLAIN
            });
            ctx.strip_task_prefix = is_task;
            if let Some(frame) = ctx.list_stack.last_mut() {
                frame.item_index += 1;
            }
            render_children(node, collector, ctx);
            ctx.pending_item_attrs = None;
            ctx.strip_task_prefix = false;
        }

        NodeValue::ThematicBreak => {
            collector.start_para(ParagraphAttrs {
                kind: BlockKind::ThematicBreak,
                ..ParagraphAttrs::PLAIN
            });
            collector.append(THEMATIC_BREAK_GLYPH);
        }

        NodeValue::HtmlBlock(html) => {
            collector.start_para(ParagraphAttrs::PLAIN);
            let trimmed = html.literal.trim().to_string();
            collector.append(&trimmed);
        }

        NodeValue::HtmlInline(literal) => collector.append(literal),

        _ => render_children(node, collector, ctx),
    }
}

// -- Task-list detection (manual, no extension) ---------------------------
//
// We parse `- [ ]` as a plain bullet so the `[ ]`/`[x]` marker stays in the
// literal and inspect it here — cmark's tasklist extension consumes the
// marker and then can't distinguish an UNCHECKED task item from a regular
// bullet, which made unchecked checkboxes round-trip as bullets (iOS learned
// this the hard way; we inherit the fix).

const TASK_MARKERS: [&str; 3] = ["[ ] ", "[x] ", "[X] "];

/// True/checked when the item's first text literal begins with a task marker.
fn task_item_state<'a>(item: &'a AstNode<'a>) -> (bool, bool) {
    let Some(para) = item.first_child() else {
        return (false, false);
    };
    if !matches!(para.data.borrow().value, NodeValue::Paragraph) {
        return (false, false);
    }
    let Some(text) = para.first_child() else {
        return (false, false);
    };
    let data = text.data.borrow();
    let NodeValue::Text(literal) = &data.value else {
        return (false, false);
    };
    if literal.starts_with("[ ] ") {
        (true, false)
    } else if literal.starts_with("[x] ") || literal.starts_with("[X] ") {
        (true, true)
    } else {
        (false, false)
    }
}

fn collect_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.children() {
        if let NodeValue::Text(literal) = &child.data.borrow().value {
            out.push_str(literal);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_block(blocks: &[ContentBlock], i: usize) -> &RichText {
        match &blocks[i] {
            ContentBlock::Text { content, .. } => content,
            other => panic!("expected text block at {i}, got {other:?}"),
        }
    }

    #[test]
    fn blank_input_is_one_empty_text_block() {
        let blocks = markdown_to_blocks("");
        assert_eq!(blocks.len(), 1);
        assert!(text_block(&blocks, 0).is_empty());
    }

    #[test]
    fn images_split_blocks() {
        let blocks = markdown_to_blocks("before\n\n![alt](/api/attachments/abc)\n\nafter");
        assert_eq!(blocks.len(), 3);
        assert_eq!(text_block(&blocks, 0).text, "before");
        assert!(matches!(
            &blocks[1],
            ContentBlock::Image { url, alt, .. }
                if url == "/api/attachments/abc" && alt == "alt"
        ));
        assert_eq!(text_block(&blocks, 2).text, "after");
    }

    #[test]
    fn unchecked_task_is_distinguished_from_bullet() {
        let blocks = markdown_to_blocks("- [ ] todo\n- [x] done\n- plain");
        let rich = text_block(&blocks, 0);
        assert_eq!(rich.text, "todo\ndone\nplain");
        assert_eq!(rich.paragraphs[0].list_type, Some(ListType::Checklist));
        assert!(!rich.paragraphs[0].checked);
        assert_eq!(rich.paragraphs[1].list_type, Some(ListType::Checklist));
        assert!(rich.paragraphs[1].checked);
        assert_eq!(rich.paragraphs[2].list_type, Some(ListType::Bullet));
    }

    #[test]
    fn inline_marks_carry_byte_ranges() {
        let blocks = markdown_to_blocks("a **b** *c* ~~d~~ `e` [f](/x)");
        let rich = text_block(&blocks, 0);
        assert_eq!(rich.text, "a b c d e f");
        let kinds: Vec<InlineKind> = rich.marks.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&InlineKind::Bold));
        assert!(kinds.contains(&InlineKind::Italic));
        assert!(kinds.contains(&InlineKind::Strikethrough));
        assert!(kinds.contains(&InlineKind::InlineCode));
        let link = rich
            .marks
            .iter()
            .find(|m| m.kind == InlineKind::Link)
            .expect("link mark");
        assert_eq!(link.href.as_deref(), Some("/x"));
        assert_eq!(&rich.text[link.start..link.end], "f");
    }

    #[test]
    fn mentions_and_issue_refs_stay_plain_text() {
        let blocks = markdown_to_blocks("ping @a@b.com about #EXP-12");
        let rich = text_block(&blocks, 0);
        assert_eq!(rich.text, "ping @a@b.com about #EXP-12");
        assert!(rich.marks.is_empty());
    }
}
