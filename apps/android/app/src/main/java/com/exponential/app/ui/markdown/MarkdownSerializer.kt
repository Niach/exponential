package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText

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

    fun blocksToMarkdown(blocks: List<ContentBlock>): String {
        val parts = mutableListOf<String>()
        for (block in blocks) {
            when (block) {
                is ContentBlock.TextBlock -> {
                    val md = serializeText(block.content)
                    if (md.isNotEmpty()) parts.add(md)
                }
                is ContentBlock.ImageBlock -> parts.add("![${block.alt}](${block.url})")
            }
        }
        return parts.joinToString("\n\n")
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
        val segments = segment(attrs)
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

        val out = StringBuilder()
        var pendingText: StringBuilder? = null
        var pendingFlags: RunFlags? = null

        fun flush() {
            val flags = pendingFlags ?: return
            val s = pendingText.toString()
            pendingText = null
            pendingFlags = null
            when {
                s.isEmpty() -> {}
                flags.code -> out.append("`").append(s).append("`")
                flags.link -> out.append("[").append(s).append("](").append(flags.href ?: "").append(")")
                else -> {
                    var t = s
                    if (flags.strike) t = "~~$t~~"
                    val bold = flags.bold && !isHeading
                    when {
                        bold && flags.italic -> t = "***$t***"
                        bold -> t = "**$t**"
                        flags.italic -> t = "*$t*"
                    }
                    out.append(t)
                }
            }
        }

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
            if (flags != pendingFlags) {
                flush()
                pendingFlags = flags
                pendingText = StringBuilder()
            }
            pendingText!!.append(text, a, b)
        }
        flush()
        return out.toString()
    }
}
