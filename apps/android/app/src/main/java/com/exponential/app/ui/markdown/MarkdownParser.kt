package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText
import com.exponential.app.ui.markdown.model.TableAlignment
import com.exponential.app.ui.markdown.model.TableCell
import com.exponential.app.ui.markdown.model.TableData
import com.exponential.app.ui.markdown.model.normalizeBlocks
import org.commonmark.ext.gfm.strikethrough.Strikethrough
import org.commonmark.ext.gfm.strikethrough.StrikethroughExtension
import org.commonmark.ext.gfm.tables.TableBlock as CmTableBlock
import org.commonmark.ext.gfm.tables.TableBody as CmTableBody
import org.commonmark.ext.gfm.tables.TableCell as CmTableCell
import org.commonmark.ext.gfm.tables.TableHead as CmTableHead
import org.commonmark.ext.gfm.tables.TableRow as CmTableRow
import org.commonmark.ext.gfm.tables.TablesExtension
import org.commonmark.node.BlockQuote
import org.commonmark.node.BulletList
import org.commonmark.node.Code
import org.commonmark.node.Document
import org.commonmark.node.Emphasis
import org.commonmark.node.FencedCodeBlock
import org.commonmark.node.HardLineBreak
import org.commonmark.node.Heading
import org.commonmark.node.HtmlBlock
import org.commonmark.node.HtmlInline
import org.commonmark.node.Image
import org.commonmark.node.IndentedCodeBlock
import org.commonmark.node.Link
import org.commonmark.node.ListItem
import org.commonmark.node.Node
import org.commonmark.node.OrderedList
import org.commonmark.node.Paragraph
import org.commonmark.node.SoftLineBreak
import org.commonmark.node.StrongEmphasis
import org.commonmark.node.Text
import org.commonmark.node.ThematicBreak
import org.commonmark.parser.Parser

/**
 * Parses GFM markdown into [ContentBlock]s by walking the commonmark-java AST by
 * hand — a faithful port of iOS `renderNodeToBlocks` / `BlockCollector` /
 * `RenderContext` (`apps/ios/Exponential/UI/Markdown/MarkdownConversion.swift`).
 *
 * Only images split blocks. Headings, lists, quotes and fenced code become
 * paragraph-level attributes inside a [ContentBlock.TextBlock]. Task-list items
 * are detected manually (NOT via a tasklist extension) so unchecked boxes don't
 * degrade to plain bullets — the same reasoning the iOS implementation documents.
 */
object MarkdownParser {

    // NOTE: no autolink extension — web (tiptap-markdown) leaves bare URLs bare,
    // so autolinking here would rewrite `https://x` to `[https://x](https://x)`
    // on the next mobile save, diverging the stored bytes from the web client.
    private val parser: Parser = Parser.builder()
        .extensions(listOf(StrikethroughExtension.create(), TablesExtension.create()))
        .build()

    /**
     * [softBreaksAsNewlines] renders a single `\n` in the source as its own
     * line instead of the GFM space (EXP-440) — agent narration is written for
     * a terminal, so re-flowing its lines into one paragraph mangles it. Only
     * ever set by read-only renderers: the editor must keep GFM semantics or
     * the next save would rewrite the stored bytes.
     */
    fun parse(markdown: String, softBreaksAsNewlines: Boolean = false): List<ContentBlock> {
        if (markdown.isBlank()) {
            return mutableListOf<ContentBlock>(ContentBlock.TextBlock(content = RichText.EMPTY))
                .also { normalizeBlocks(it) }
        }
        val doc = parser.parse(markdown)
        val collector = BlockCollector()
        val ctx = RenderContext(softBreaksAsNewlines)
        collector.renderChildren(doc, ctx)
        return collector.finalize()
    }

    // --- A single paragraph (one '\n'-delimited line) under construction. ---
    private class ParaBuild(var attrs: ParagraphAttrs) {
        val sb = StringBuilder()
        val marks = mutableListOf<InlineMark>()
        val len get() = sb.length
    }

    private class OpenMark(val kind: InlineKind, val href: String?, var start: Int)

    private class ListFrame(val ordered: Boolean, var itemIndex: Int, val depth: Int)

    private class RenderContext(val softBreaksAsNewlines: Boolean = false) {
        val listStack = ArrayDeque<ListFrame>()
        /**
         * Inside a table cell (EXP-726). A cell is ONE inline paragraph: an
         * image degrades to its literal `![alt](url)` text and every break
         * (soft or hard, [softBreaksAsNewlines] notwithstanding) becomes a
         * space, so the cell can never grow a second line the pipe syntax
         * cannot carry.
         */
        var inTableCell = false
        var inBlockquote = false
        /** Attrs the next [Paragraph] should adopt (set when a list item opens). */
        var pendingItemAttrs: ParagraphAttrs? = null
        /** Strip the `[ ] `/`[x] ` task marker from the next [Text] literal. */
        var stripTaskPrefix = false
    }

    private class BlockCollector {
        private val blocks = mutableListOf<ContentBlock>()
        private val paras = mutableListOf<ParaBuild>()
        private val openMarks = ArrayDeque<OpenMark>()

        fun renderChildren(node: Node, ctx: RenderContext) {
            var child = node.firstChild
            while (child != null) {
                val next = child.next
                visit(child, ctx)
                child = next
            }
        }

        private fun currentPara(): ParaBuild {
            if (paras.isEmpty()) paras.add(ParaBuild(ParagraphAttrs.PLAIN))
            return paras.last()
        }

        private fun startPara(attrs: ParagraphAttrs) {
            paras.add(ParaBuild(attrs))
        }

        private fun append(text: String) {
            currentPara().sb.append(text)
        }

        // -- Mark stack --------------------------------------------------------

        private fun pushMark(kind: InlineKind, href: String? = null) {
            openMarks.addLast(OpenMark(kind, href, currentPara().len))
        }

        private fun popMark(kind: InlineKind) {
            // Find and remove the nearest open mark of this kind.
            val idx = openMarks.indexOfLast { it.kind == kind }
            if (idx < 0) return
            val mark = openMarks.removeAt(idx)
            val para = currentPara()
            if (para.len > mark.start) {
                para.marks.add(InlineMark(mark.start, para.len, mark.kind, mark.href))
            }
        }

        /** Close all open marks at the end of a paragraph and reopen them at 0 in a fresh para. */
        private fun breakParaPreservingMarks(attrs: ParagraphAttrs) {
            val para = currentPara()
            for (m in openMarks) {
                if (para.len > m.start) {
                    para.marks.add(InlineMark(m.start, para.len, m.kind, m.href))
                }
            }
            startPara(attrs)
            for (m in openMarks) m.start = 0
        }

        // -- Text-block flushing ----------------------------------------------

        private fun flushText() {
            // Drop a single trailing empty paragraph — the artifact of a block
            // separator before an image (or end of input). Mirrors iOS flushText
            // stripping one trailing '\n' so an image-only paragraph doesn't leave
            // a stray blank line in the preceding text block.
            if (paras.size > 1 && paras.last().sb.isEmpty() && paras.last().marks.isEmpty()) {
                paras.removeAt(paras.size - 1)
            }
            val text = paras.joinToString("\n") { it.sb.toString() }
            val attrsList = paras.map { it.attrs }
            val marks = mutableListOf<InlineMark>()
            var offset = 0
            for ((i, p) in paras.withIndex()) {
                for (m in p.marks) marks.add(m.copy(start = m.start + offset, end = m.end + offset))
                offset += p.sb.length
                if (i < paras.size - 1) offset += 1 // the '\n' separator
            }
            val richText = if (attrsList.isEmpty()) {
                RichText.EMPTY
            } else {
                RichText(text = text, paragraphs = attrsList, marks = marks)
            }
            blocks.add(ContentBlock.TextBlock(content = richText))
            paras.clear()
        }

        private fun emitImage(url: String, alt: String) {
            flushText()
            blocks.add(ContentBlock.ImageBlock(url = url, alt = alt))
        }

        fun finalize(): List<ContentBlock> {
            flushText()
            normalizeBlocks(blocks)
            return blocks
        }

        // -- AST dispatch ------------------------------------------------------

        private fun visit(node: Node, ctx: RenderContext) {
            when (node) {
                is Document -> renderChildren(node, ctx)

                is Paragraph -> {
                    val attrs = ctx.pendingItemAttrs
                        ?: if (ctx.inBlockquote) ParagraphAttrs(kind = BlockKind.Blockquote) else ParagraphAttrs.PLAIN
                    ctx.pendingItemAttrs = null
                    startPara(attrs)
                    renderChildren(node, ctx)
                    // EXP-689: a whitespace-only plain paragraph is the stored
                    // form of an intentional blank line (`&nbsp;`, decoded by
                    // commonmark to U+00A0) — fold it to a genuinely empty
                    // line so the editor shows no invisible character and the
                    // serializer writes the `&nbsp;` form back. `lastOrNull`,
                    // not `currentPara()`: an image inside the paragraph has
                    // already flushed the run, and a fresh empty paragraph
                    // here would prepend a blank line to the next block.
                    val para = paras.lastOrNull()
                    if (para != null && attrs.kind == BlockKind.Paragraph &&
                        para.marks.isEmpty() && para.sb.isNotEmpty() && para.sb.isBlank()
                    ) {
                        para.sb.clear()
                    }
                }

                is Heading -> {
                    startPara(ParagraphAttrs(kind = BlockKind.Heading, headingLevel = node.level.coerceIn(1, 6)))
                    renderChildren(node, ctx)
                }

                is Text -> {
                    var literal = node.literal
                    if (ctx.stripTaskPrefix) {
                        ctx.stripTaskPrefix = false
                        for (marker in TASK_MARKERS) {
                            if (literal.startsWith(marker)) {
                                literal = literal.substring(marker.length)
                                break
                            }
                        }
                    }
                    append(literal)
                }

                // A soft break is a space in GFM. Read-only renderers can ask
                // for the terminal reading instead, where it is a real line —
                // and a line is a paragraph boundary here, not a raw '\n':
                // RichText's invariant is one ParagraphAttrs per '\n'-line.
                is SoftLineBreak -> if (ctx.softBreaksAsNewlines && !ctx.inTableCell) {
                    breakParaPreservingMarks(currentPara().attrs)
                } else {
                    append(" ")
                }

                is HardLineBreak -> if (ctx.inTableCell) {
                    append(" ")
                } else {
                    // A hard break becomes a paragraph boundary carrying the same attrs,
                    // matching how iOS re-splits the run by line at serialize time.
                    breakParaPreservingMarks(currentPara().attrs)
                }

                is StrongEmphasis -> {
                    pushMark(InlineKind.Bold)
                    renderChildren(node, ctx)
                    popMark(InlineKind.Bold)
                }

                is Emphasis -> {
                    pushMark(InlineKind.Italic)
                    renderChildren(node, ctx)
                    popMark(InlineKind.Italic)
                }

                is Strikethrough -> {
                    pushMark(InlineKind.Strikethrough)
                    renderChildren(node, ctx)
                    popMark(InlineKind.Strikethrough)
                }

                is Code -> {
                    val start = currentPara().len
                    append(node.literal)
                    val end = currentPara().len
                    if (end > start) {
                        currentPara().marks.add(InlineMark(start, end, InlineKind.InlineCode))
                    }
                }

                is Link -> {
                    // Store the RAW destination so serialization re-emits it verbatim
                    // (relative `/api/...` links stay relative — round-trip safe).
                    pushMark(InlineKind.Link, href = node.destination)
                    renderChildren(node, ctx)
                    popMark(InlineKind.Link)
                }

                is Image -> {
                    val url = node.destination ?: ""
                    val alt = collectText(node)
                    // A cell cannot hold a block, so an image inside one stays
                    // the literal markdown it was written as (contract §cells).
                    if (ctx.inTableCell) append("![" + alt + "](" + url + ")")
                    else emitImage(url = url, alt = alt)
                }

                is CmTableBlock -> emitTable(node, ctx)

                is FencedCodeBlock -> emitCodeBlock(node.literal, node.info?.takeIf { it.isNotBlank() })

                is IndentedCodeBlock -> emitCodeBlock(node.literal, null)

                is BlockQuote -> {
                    val prev = ctx.inBlockquote
                    ctx.inBlockquote = true
                    renderChildren(node, ctx)
                    ctx.inBlockquote = prev
                }

                is BulletList -> {
                    ctx.listStack.addLast(ListFrame(ordered = false, itemIndex = 0, depth = ctx.listStack.size))
                    renderChildren(node, ctx)
                    ctx.listStack.removeLast()
                }

                is OrderedList -> {
                    val start = node.markerStartNumber ?: 1
                    ctx.listStack.addLast(ListFrame(ordered = true, itemIndex = start, depth = ctx.listStack.size))
                    renderChildren(node, ctx)
                    ctx.listStack.removeLast()
                }

                is ListItem -> {
                    val frame = ctx.listStack.lastOrNull()
                    val ordered = frame?.ordered ?: false
                    val depth = frame?.depth ?: 0
                    val index = frame?.itemIndex ?: 1
                    val (isTask, checked) = taskItemState(node)
                    val listType = when {
                        isTask -> ListType.Checklist
                        ordered -> ListType.Ordered
                        else -> ListType.Bullet
                    }
                    ctx.pendingItemAttrs = ParagraphAttrs(
                        kind = BlockKind.ListItem,
                        listType = listType,
                        orderedIndex = if (ordered) index else 0,
                        listDepth = depth,
                        checked = checked,
                    )
                    if (isTask) ctx.stripTaskPrefix = true
                    frame?.let { it.itemIndex += 1 }
                    renderChildren(node, ctx)
                    ctx.pendingItemAttrs = null
                    ctx.stripTaskPrefix = false
                }

                is ThematicBreak -> {
                    startPara(ParagraphAttrs(kind = BlockKind.ThematicBreak))
                    append(THEMATIC_BREAK_GLYPH)
                }

                is HtmlBlock -> {
                    startPara(ParagraphAttrs.PLAIN)
                    append(node.literal.trim())
                }

                is HtmlInline -> append(node.literal)

                else -> renderChildren(node, ctx)
            }
        }

        /**
         * A GFM pipe table (EXP-726). Row 0 is the header and carries the
         * column alignments; ragged body rows are padded with empty cells and
         * extra cells are dropped, so [TableData] is always rectangular.
         */
        private fun emitTable(node: CmTableBlock, ctx: RenderContext) {
            var header: List<TableCell> = emptyList()
            var alignments: List<TableAlignment> = emptyList()
            val body = mutableListOf<List<TableCell>>()
            var section = node.firstChild
            while (section != null) {
                when (section) {
                    is CmTableHead -> for (row in sectionRows(section)) {
                        if (header.isEmpty()) {
                            header = row.map { renderCellInline(it, ctx) }
                            alignments = row.map { alignmentOf(it) }
                        }
                    }
                    is CmTableBody -> for (row in sectionRows(section)) {
                        body.add(row.map { renderCellInline(it, ctx) })
                    }
                }
                section = section.next
            }
            if (header.isEmpty()) return
            val width = header.size
            val rows = body.map { cells ->
                if (cells.size == width) cells
                else List(width) { i -> cells.getOrNull(i) ?: TableCell(text = "") }
            }
            flushText()
            blocks.add(
                ContentBlock.TableBlock(
                    table = TableData(
                        header = header,
                        rows = rows,
                        alignments = List(width) { alignments.getOrElse(it) { TableAlignment.None } },
                    ),
                ),
            )
        }

        /** The cells of every [CmTableRow] directly under a head/body section. */
        private fun sectionRows(section: Node): List<List<CmTableCell>> {
            val out = mutableListOf<List<CmTableCell>>()
            var row = section.firstChild
            while (row != null) {
                if (row is CmTableRow) {
                    val cells = mutableListOf<CmTableCell>()
                    var cell = row.firstChild
                    while (cell != null) {
                        if (cell is CmTableCell) cells.add(cell)
                        cell = cell.next
                    }
                    out.add(cells)
                }
                row = row.next
            }
            return out
        }

        private fun alignmentOf(cell: CmTableCell): TableAlignment = when (cell.alignment) {
            CmTableCell.Alignment.LEFT -> TableAlignment.Left
            CmTableCell.Alignment.CENTER -> TableAlignment.Center
            CmTableCell.Alignment.RIGHT -> TableAlignment.Right
            else -> TableAlignment.None
        }

        /**
         * Render one cell's inline children into a FRESH collector so the
         * enclosing document's paragraph stack is untouched; the result is one
         * inline paragraph (text + marks).
         */
        private fun renderCellInline(cell: CmTableCell, ctx: RenderContext): TableCell {
            val sub = BlockCollector()
            val cellCtx = RenderContext(ctx.softBreaksAsNewlines)
            cellCtx.inTableCell = true
            sub.renderChildren(cell, cellCtx)
            val rich = sub.finalizeInline()
            return TableCell(text = rich.text, marks = rich.marks)
        }

        /** The collected paragraphs folded into ONE inline line (no separators). */
        fun finalizeInline(): RichText {
            val sb = StringBuilder()
            val marks = mutableListOf<InlineMark>()
            for (p in paras) {
                val offset = sb.length
                sb.append(p.sb)
                for (m in p.marks) marks.add(m.copy(start = m.start + offset, end = m.end + offset))
            }
            return RichText(sb.toString(), listOf(ParagraphAttrs.PLAIN), marks)
        }

        private fun emitCodeBlock(literal: String, lang: String?) {
            // Each source line of the fenced block becomes its own CodeBlock paragraph;
            // the serializer detects the consecutive run and emits a single fence.
            val body = literal.removeSuffix("\n")
            val lines = body.split("\n")
            for (line in lines) {
                startPara(ParagraphAttrs(kind = BlockKind.CodeBlock, codeLang = lang))
                append(line)
            }
        }

        private fun collectText(node: Node): String {
            val sb = StringBuilder()
            var child = node.firstChild
            while (child != null) {
                if (child is Text) sb.append(child.literal)
                child = child.next
            }
            return sb.toString()
        }
    }

    // -- Task-list detection (manual, no extension) ---------------------------

    private val TASK_MARKERS = listOf("[ ] ", "[x] ", "[X] ")

    /** True/checked when the item's first text literal begins with a task marker. */
    private fun taskItemState(item: ListItem): Pair<Boolean, Boolean> {
        val para = item.firstChild as? Paragraph ?: return false to false
        val text = para.firstChild as? Text ?: return false to false
        val lit = text.literal
        return when {
            lit.startsWith("[ ] ") -> true to false
            lit.startsWith("[x] ") || lit.startsWith("[X] ") -> true to true
            else -> false to false
        }
    }

    const val THEMATIC_BREAK_GLYPH = "───"
}
