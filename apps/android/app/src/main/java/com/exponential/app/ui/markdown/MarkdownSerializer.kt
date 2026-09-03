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

/**
 * Serializes [ContentBlock]s back to GFM markdown — the save path, where byte
 * parity with the web (tiptap-markdown) and iOS clients matters. Ports iOS
 * `blocksToMarkdown` / `attributedStringToMarkdown` / `extractInlineMarkdown`
 * (`apps/ios/Exponential/UI/Markdown/MarkdownConversion.swift`), producing the
 * canonical forms: bullet marker `-`, tight lists (single `\n` between items),
 * `\n\n` between blocks, `**`/`*`/`~~`/`***` delimiters, ATX headings, fenced
 * code blocks, and `![alt](url)` images.
 */
object MarkdownSerializer {

    /**
     * The GFM interchange form of an intentional blank line — a paragraph
     * holding only a no-break space, written as the entity so it survives
     * every client's parser as a visually empty paragraph (EXP-7 on web,
     * EXP-689 here).
     */
    const val BLANK_LINE_MARKER = "&nbsp;"

    fun blocksToMarkdown(blocks: List<ContentBlock>): String {
        val parts = mutableListOf<String>()
        for (block in blocks) {
            when (block) {
                is ContentBlock.TextBlock -> {
                    val md = serializeText(block.content)
                    if (md.isNotEmpty()) parts.add(md)
                }
                is ContentBlock.ImageBlock -> parts.add("![${block.alt}](${block.url})")
                is ContentBlock.TableBlock -> parts.add(serializeTable(block.table))
            }
        }
        return parts.joinToString("\n\n")
    }

    // -- Tables (EXP-726) --------------------------------------------------
    //
    // The cross-client canonical form, byte-locked by MarkdownRoundTripTest and
    // mirrored on web, iOS and the desktop: one space each side of every cell,
    // no column-width padding, rows joined by '\n', delimiter cells
    // `---`/`:---`/`:---:`/`---:`, an empty cell written `|  |`, and `|` inside
    // a cell escaped as `\|`.

    private fun serializeTable(table: TableData): String {
        val width = table.columnCount
        if (width == 0) return ""
        val out = mutableListOf(serializeTableRow(table.header, width))
        out.add(delimiterRow(table.alignments, width))
        for (row in table.rows) out.add(serializeTableRow(row, width))
        return out.joinToString("\n")
    }

    private fun serializeTableRow(cells: List<TableCell>, width: Int): String {
        val sb = StringBuilder("|")
        for (i in 0 until width) {
            val cell = cells.getOrNull(i)
            sb.append(" ").append(if (cell == null) "" else serializeCell(cell)).append(" |")
        }
        return sb.toString()
    }

    /**
     * One cell's inline markdown. A cell is never a heading (bold survives), a
     * stray newline folds to a space, and every `|` is escaped LAST so the
     * escape also covers pipes a link destination brought along.
     */
    private fun serializeCell(cell: TableCell): String =
        escapeCellPipes(inline(cell.text, cell.marks, isHeading = false).replace("\n", " "))

    /**
     * Escape every `|` for the pipe-table row syntax, backslash-aware.
     *
     * A GFM row is split on pipes BEFORE inline parsing, and that splitter
     * treats a `\` immediately before a `|` as the escape — so a cell whose
     * text really contains `\|` (backslash then pipe) must ship FOUR
     * backslashes' worth of intent: the run of backslashes in front of the
     * pipe is doubled (each becomes an escaped backslash the inline parser
     * collapses back to one) and only then is the pipe escaped. Writing a bare
     * `\|` there would have handed the splitter `...\` + `\|` and re-cut the
     * cell in two, losing everything after the pipe (`a\|b` was written
     * `a\\|b`). Backslashes not sitting in front of a pipe are left alone —
     * the inline parser sees the same text either way.
     */
    private fun escapeCellPipes(s: String): String {
        if (!s.contains('|')) return s
        val sb = StringBuilder(s.length + 8)
        var backslashes = 0
        for (ch in s) {
            when (ch) {
                '\\' -> {
                    backslashes++
                    sb.append(ch)
                }
                '|' -> {
                    repeat(backslashes) { sb.append('\\') }
                    sb.append("\\|")
                    backslashes = 0
                }
                else -> {
                    backslashes = 0
                    sb.append(ch)
                }
            }
        }
        return sb.toString()
    }

    private fun delimiterRow(alignments: List<TableAlignment>, width: Int): String {
        val sb = StringBuilder("|")
        for (i in 0 until width) {
            val cell = when (alignments.getOrElse(i) { TableAlignment.None }) {
                TableAlignment.Left -> ":---"
                TableAlignment.Center -> ":---:"
                TableAlignment.Right -> "---:"
                TableAlignment.None -> "---"
            }
            sb.append(" ").append(cell).append(" |")
        }
        return sb.toString()
    }

    // -- Text block --------------------------------------------------------

    private fun serializeText(rich: RichText): String {
        val lines = rich.lines
        if (lines.size == 1 && lines[0].isEmpty()) return ""
        val attrs = (0 until lines.size).map { rich.paragraphs.getOrElse(it) { ParagraphAttrs.PLAIN } }

        // Per-line inline marks, offset to line-local coordinates.
        val lineMarks = ArrayList<List<InlineMark>>(lines.size)
        var charStart = 0
        for ((i, line) in lines.withIndex()) {
            val lineStart = charStart
            val lineEnd = charStart + line.length
            val local = rich.marks.mapNotNull { m ->
                val s = maxOf(m.start, lineStart)
                val e = minOf(m.end, lineEnd)
                if (e > s) m.copy(start = s - lineStart, end = e - lineStart) else null
            }
            lineMarks.add(local)
            charStart = lineEnd + 1 // + '\n'
        }

        // Group consecutive code-block lines into single fenced segments; every
        // other line is its own segment.
        val allSegments = segment(attrs)
        // EXP-689: an intentional blank line (two Enters) is an empty plain
        // paragraph. GFM cannot carry one as bare newlines — every parser
        // folds `A\n\n\n\nB` into `A\n\nB` — so INTERIOR blank paragraphs
        // are written as the contract's `&nbsp;` line (web's MarkdownParagraph
        // does exactly this) and leading/trailing ones are dropped as
        // meaningless spacing. Blank lines inside a fence are code, untouched.
        fun isBlankParagraph(seg: Segment) =
            !seg.isCode && attrs[seg.startLine].kind == BlockKind.Paragraph && lines[seg.startLine].isBlank()
        val firstContent = allSegments.indexOfFirst { !isBlankParagraph(it) }
        if (firstContent < 0) return ""
        val lastContent = allSegments.indexOfLast { !isBlankParagraph(it) }
        val segments = allSegments.subList(firstContent, lastContent + 1)
        val out = StringBuilder()
        for ((segIndex, seg) in segments.withIndex()) {
            if (segIndex > 0) {
                val prev = segments[segIndex - 1]
                val tight = attrs[prev.endLine].listType != null && attrs[seg.startLine].listType != null
                out.append(if (tight) "\n" else "\n\n")
            }
            if (seg.isCode) {
                val lang = attrs[seg.startLine].codeLang ?: ""
                out.append("```").append(lang).append("\n")
                out.append((seg.startLine..seg.endLine).joinToString("\n") { lines[it] })
                out.append("\n```")
            } else if (isBlankParagraph(seg)) {
                out.append(BLANK_LINE_MARKER)
            } else {
                val i = seg.startLine
                out.append(serializeLine(lines[i], attrs[i], lineMarks[i]))
            }
        }
        return out.toString().trim()
    }

    private class Segment(val startLine: Int, val endLine: Int, val isCode: Boolean)

    private fun segment(attrs: List<ParagraphAttrs>): List<Segment> {
        val segments = mutableListOf<Segment>()
        var i = 0
        while (i < attrs.size) {
            if (attrs[i].kind == BlockKind.CodeBlock) {
                var j = i
                while (j + 1 < attrs.size && attrs[j + 1].kind == BlockKind.CodeBlock) j++
                segments.add(Segment(i, j, isCode = true))
                i = j + 1
            } else {
                segments.add(Segment(i, i, isCode = false))
                i++
            }
        }
        return segments
    }

    private fun serializeLine(line: String, a: ParagraphAttrs, marks: List<InlineMark>): String =
        when (a.kind) {
            BlockKind.Heading -> {
                val level = a.headingLevel.coerceIn(1, 6)
                "#".repeat(level) + " " + inline(line, marks, isHeading = true)
            }
            BlockKind.Blockquote -> "> " + inline(line, marks, isHeading = false)
            BlockKind.ListItem -> {
                val indent = "  ".repeat(a.listDepth)
                val prefix = when (a.listType) {
                    ListType.Ordered -> "${a.orderedIndex}. "
                    ListType.Checklist -> if (a.checked) "- [x] " else "- [ ] "
                    ListType.Bullet, null -> "- "
                }
                indent + prefix + inline(line, marks, isHeading = false)
            }
            // Re-emit the canonical `---` so a horizontal rule round-trips on all
            // three clients (the in-editor glyph `───` is render-only).
            BlockKind.ThematicBreak -> "---"
            BlockKind.Paragraph, BlockKind.CodeBlock -> inline(line, marks, isHeading = false)
        }

    // -- Inline marks -------------------------------------------------------

    private data class RunFlags(
        val code: Boolean,
        val link: Boolean,
        val href: String?,
        val bold: Boolean,
        val italic: Boolean,
        val strike: Boolean,
    )

    private class Run(val text: StringBuilder, val flags: RunFlags)

    private fun inline(text: String, marks: List<InlineMark>, isHeading: Boolean): String {
        if (text.isEmpty()) return ""
        if (marks.isEmpty()) return text

        val n = text.length
        val boundaries = sortedSetOf(0, n)
        for (m in marks) {
            if (m.start in 0..n) boundaries.add(m.start)
            if (m.end in 0..n) boundaries.add(m.end)
        }
        val bounds = boundaries.toList()

        // Collapse the mark boundaries into runs of identical formatting.
        val runs = mutableListOf<Run>()
        for (k in 0 until bounds.size - 1) {
            val a = bounds[k]
            val b = bounds[k + 1]
            if (b <= a) continue
            val active = marks.filter { it.start <= a && it.end >= b }
            val link = active.lastOrNull { it.kind == InlineKind.Link }
            val flags = RunFlags(
                code = active.any { it.kind == InlineKind.InlineCode },
                link = link != null,
                href = link?.href,
                bold = active.any { it.kind == InlineKind.Bold },
                italic = active.any { it.kind == InlineKind.Italic },
                strike = active.any { it.kind == InlineKind.Strikethrough },
            )
            val last = runs.lastOrNull()
            if (last != null && last.flags == flags) last.text.append(text, a, b)
            else runs.add(Run(StringBuilder().append(text, a, b), flags))
        }

        // The parser overlaps a Link mark with the Bold/Italic/Strikethrough/
        // InlineCode marks nested inside its text, so one link can span several
        // runs. Emit the whole link as ONE `[...](href)` with the inner
        // delimiters composed INSIDE the brackets — short-circuiting on the
        // first flag used to strip the formatting, drop the href around inline
        // code, and split `[**bold** rest](u)` into two adjacent links.
        val out = StringBuilder()
        var i = 0
        while (i < runs.size) {
            val flags = runs[i].flags
            if (!flags.link) {
                out.append(styled(runs[i].text.toString(), flags, isHeading))
                i++
                continue
            }
            val href = flags.href
            val inner = StringBuilder()
            while (i < runs.size && runs[i].flags.link && runs[i].flags.href == href) {
                inner.append(styled(runs[i].text.toString(), runs[i].flags, isHeading))
                i++
            }
            if (inner.isNotEmpty()) {
                out.append("[").append(inner).append("](").append(href ?: "").append(")")
            }
        }
        return out.toString()
    }

    /**
     * Inline delimiters for one run, link wrapper excluded. Inline code stays
     * exclusive of the emphasis delimiters — iOS loses the bold/italic traits
     * when it swaps in the monospace font at load, so composing them here would
     * diverge the two native clients' bytes.
     */
    private fun styled(s: String, flags: RunFlags, isHeading: Boolean): String {
        if (s.isEmpty()) return ""
        if (flags.code) return "`$s`"
        var t = s
        if (flags.strike) t = "~~$t~~"
        val bold = flags.bold && !isHeading
        when {
            bold && flags.italic -> t = "***$t***"
            bold -> t = "**$t**"
            flags.italic -> t = "*$t*"
        }
        return t
    }
}
