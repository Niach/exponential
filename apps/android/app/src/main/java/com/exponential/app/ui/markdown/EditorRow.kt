package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText
import com.exponential.app.ui.markdown.model.TableData
import java.util.UUID

/**
 * An editing-friendly projection of the block document: a flat, ordered list of
 * multi-line text runs and images. Each [TextRun] is one run of '\n'-separated
 * paragraphs between two images, backed by ONE multi-line `BasicTextField` —
 * the iOS architecture (one `UITextView` per run), which is what lets text
 * selection span paragraphs, headings, list items, quotes and code fences
 * (EXP-534). Per-paragraph styling rides the parallel [TextRun.paragraphs]
 * list; only images split the document into separate fields.
 *
 * Rows convert losslessly to/from [ContentBlock]: a [TextRun] IS a
 * [ContentBlock.TextBlock]'s [RichText] plus a stable row id.
 */
sealed interface EditorRow {
    val id: String

    data class TextRun(
        override val id: String = UUID.randomUUID().toString(),
        /** Multi-line, '\n'-separated. */
        val text: String,
        /** Invariant: `text.split("\n").size == paragraphs.size`. */
        val paragraphs: List<ParagraphAttrs>,
        /** Offsets are text-global (same coordinates as [RichText.marks]). */
        val marks: List<InlineMark>,
    ) : EditorRow {
        /** The '\n'-delimited lines; always at least one entry. */
        val lines: List<String> get() = if (text.isEmpty()) listOf("") else text.split("\n")
    }

    data class Image(
        override val id: String = UUID.randomUUID().toString(),
        val url: String,
        val alt: String,
    ) : EditorRow

    /**
     * A GFM table (EXP-726). Like [Image] it is NOT a text run: the row itself
     * takes no caret, its CELLS do — each is its own field keyed by the cell
     * id, which [EditorModel] routes through the same `updateRun` entry point.
     */
    data class Table(
        override val id: String = UUID.randomUUID().toString(),
        val table: TableData,
    ) : EditorRow
}

/** Whether this row is an editable text run (everything else is block-level). */
internal val EditorRow.isText: Boolean get() = this is EditorRow.TextRun

object EditorRows {

    internal fun emptyRun(): EditorRow.TextRun =
        EditorRow.TextRun(text = "", paragraphs = listOf(ParagraphAttrs.PLAIN), marks = emptyList())

    /** Blocks → rows: a text block becomes one [EditorRow.TextRun] verbatim. */
    fun fromBlocks(blocks: List<ContentBlock>): List<EditorRow> {
        val rows = mutableListOf<EditorRow>()
        for (block in blocks) {
            when (block) {
                is ContentBlock.ImageBlock -> rows.add(EditorRow.Image(url = block.url, alt = block.alt))
                is ContentBlock.TableBlock -> rows.add(EditorRow.Table(table = block.table))
                is ContentBlock.TextBlock -> {
                    val rich = block.content
                    val lines = rich.lines
                    rows.add(
                        EditorRow.TextRun(
                            text = rich.text,
                            // Defensive padding — RichText guarantees the sizes
                            // match, but a drifted list must not brick editing.
                            paragraphs = List(lines.size) {
                                rich.paragraphs.getOrElse(it) { ParagraphAttrs.PLAIN }
                            },
                            marks = rich.marks,
                        ),
                    )
                }
            }
        }
        return normalize(rows)
    }

    /** Rows → blocks. Consecutive runs (transient states only) join on '\n'. */
    fun toBlocks(rows: List<EditorRow>): List<ContentBlock> {
        val blocks = mutableListOf<ContentBlock>()
        var run: EditorRow.TextRun? = null
        fun flush() {
            val r = run ?: return
            blocks.add(
                ContentBlock.TextBlock(
                    content = RichText(text = r.text, paragraphs = r.paragraphs, marks = r.marks),
                ),
            )
            run = null
        }
        for (row in rows) {
            when (row) {
                is EditorRow.TextRun -> {
                    val prev = run
                    run = if (prev == null) {
                        row
                    } else {
                        prev.copy(
                            text = prev.text + "\n" + row.text,
                            paragraphs = prev.paragraphs + row.paragraphs,
                            marks = prev.marks + MarkOps.offset(row.marks, prev.text.length + 1),
                        )
                    }
                }
                is EditorRow.Image -> {
                    flush()
                    blocks.add(ContentBlock.ImageBlock(url = row.url, alt = row.alt))
                }
                is EditorRow.Table -> {
                    flush()
                    blocks.add(ContentBlock.TableBlock(table = row.table))
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
            out.add(emptyRun())
            return out
        }
        if (!out.first().isText) {
            out.add(0, emptyRun())
        }
        if (!out.last().isText) {
            out.add(emptyRun())
        }
        var i = 1
        while (i < out.size) {
            if (!out[i].isText && !out[i - 1].isText) {
                out.add(i, emptyRun())
            }
            i++
        }
        return out
    }
}
