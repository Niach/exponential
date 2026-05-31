package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText
import java.util.UUID

/**
 * An editing-friendly projection of the block document: a flat, ordered list of
 * single-line paragraphs and images. Each [Para] is exactly one editable line
 * (no embedded `'\n'`), which lets every paragraph be its own `BasicTextField`
 * with independent per-paragraph styling (heading / list glyph / quote / code)
 * — the cleanest mapping onto Compose's text field.
 *
 * Rows convert losslessly to/from [ContentBlock]: consecutive [Para] rows form
 * one [ContentBlock.TextBlock] (joined by `'\n'`), and images split blocks, so
 * the round-trip reconstructs the exact block grouping the serializer expects.
 */
sealed interface EditorRow {
    val id: String

    data class Para(
        override val id: String = UUID.randomUUID().toString(),
        val text: String,
        val attrs: ParagraphAttrs,
        val marks: List<InlineMark>,
    ) : EditorRow

    data class Image(
        override val id: String = UUID.randomUUID().toString(),
        val url: String,
        val alt: String,
    ) : EditorRow
}

object EditorRows {

    /** Flatten blocks → rows, splitting each text block's lines into [EditorRow.Para]s. */
    fun fromBlocks(blocks: List<ContentBlock>): List<EditorRow> {
        val rows = mutableListOf<EditorRow>()
        for (block in blocks) {
            when (block) {
                is ContentBlock.ImageBlock -> rows.add(EditorRow.Image(url = block.url, alt = block.alt))
                is ContentBlock.TextBlock -> {
                    val rich = block.content
                    val lines = rich.lines
                    var charStart = 0
                    for ((i, line) in lines.withIndex()) {
                        val lineStart = charStart
                        val lineEnd = charStart + line.length
                        val local = rich.marks.mapNotNull { m ->
                            val s = maxOf(m.start, lineStart)
                            val e = minOf(m.end, lineEnd)
                            if (e > s) m.copy(start = s - lineStart, end = e - lineStart) else null
                        }
                        rows.add(
                            EditorRow.Para(
                                text = line,
                                attrs = rich.paragraphs.getOrElse(i) { ParagraphAttrs.PLAIN },
                                marks = local,
                            ),
                        )
                        charStart = lineEnd + 1
                    }
                }
            }
        }
        return normalize(rows)
    }

    /** Unflatten rows → blocks, grouping consecutive [EditorRow.Para]s into one text block. */
    fun toBlocks(rows: List<EditorRow>): List<ContentBlock> {
        val blocks = mutableListOf<ContentBlock>()
        var paraRun = mutableListOf<EditorRow.Para>()
        fun flush() {
            if (paraRun.isEmpty()) return
            val text = paraRun.joinToString("\n") { it.text }
            val attrs = paraRun.map { it.attrs }
            val marks = mutableListOf<InlineMark>()
            var offset = 0
            for ((i, p) in paraRun.withIndex()) {
                for (m in p.marks) marks.add(m.copy(start = m.start + offset, end = m.end + offset))
                offset += p.text.length
                if (i < paraRun.size - 1) offset += 1 // '\n'
            }
            blocks.add(ContentBlock.TextBlock(content = RichText(text = text, paragraphs = attrs, marks = marks)))
            paraRun = mutableListOf()
        }
        for (row in rows) {
            when (row) {
                is EditorRow.Para -> paraRun.add(row)
                is EditorRow.Image -> {
                    flush()
                    blocks.add(ContentBlock.ImageBlock(url = row.url, alt = row.alt))
                }
            }
        }
        flush()
        return blocks
    }

    fun toMarkdown(rows: List<EditorRow>): String = MarkdownSerializer.blocksToMarkdown(toBlocks(rows))

    /** Row-level structural invariants mirroring [normalizeBlocks]. Returns a new list. */
    fun normalize(rows: List<EditorRow>): List<EditorRow> {
        val out = rows.toMutableList()
        if (out.isEmpty()) {
            out.add(EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList()))
            return out
        }
        if (out.first() is EditorRow.Image) {
            out.add(0, EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList()))
        }
        if (out.last() is EditorRow.Image) {
            out.add(EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList()))
        }
        var i = 1
        while (i < out.size) {
            if (out[i] is EditorRow.Image && out[i - 1] is EditorRow.Image) {
                out.add(i, EditorRow.Para(text = "", attrs = ParagraphAttrs.PLAIN, marks = emptyList()))
            }
            i++
        }
        return out
    }
}
