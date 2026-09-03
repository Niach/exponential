package com.exponential.app.ui.markdown.model

import android.net.Uri
import java.util.UUID

/**
 * The document model for the block-based markdown editor — a 1:1 port of the iOS
 * `ContentBlock` enum (`apps/ios/Exponential/UI/Markdown/MarkdownConversion.swift`).
 *
 * Only images split the document into blocks. Headings, lists, quotes and code
 * blocks are *paragraph-level* attributes inside a single [TextBlock] (mirroring
 * how iOS encodes them as `NSAttributedString` paragraph keys). Markdown is
 * derived from blocks only at save time — never round-tripped per keystroke.
 */
sealed interface ContentBlock {
    val id: String

    data class TextBlock(
        override val id: String = UUID.randomUUID().toString(),
        val content: RichText,
    ) : ContentBlock

    data class ImageBlock(
        override val id: String = UUID.randomUUID().toString(),
        /** Either a `draft://<uuid>` placeholder or the relative `/api/attachments/{id}`. */
        val url: String,
        val alt: String = "image",
    ) : ContentBlock

    /** A GFM pipe table (EXP-726) — block-level like an image. */
    data class TableBlock(
        override val id: String = UUID.randomUUID().toString(),
        val table: TableData,
    ) : ContentBlock
}

/** A table column's GFM alignment, from its delimiter cell (`---`/`:---`/`:---:`/`---:`). */
enum class TableAlignment { None, Left, Center, Right }

/**
 * One table cell: ONE inline paragraph. [text] never contains a newline (the
 * editor collapses pasted ones to spaces) and stores the UNESCAPED characters —
 * the serializer re-escapes `|` on the way out. Cell ids live in the same
 * namespace as row/block ids so [EditorModel] can route an edit by id alone.
 */
data class TableCell(
    val id: String = UUID.randomUUID().toString(),
    val text: String,
    val marks: List<InlineMark> = emptyList(),
)

/**
 * A table's grid. Row 0 is ALWAYS the header row (GFM requires one), so
 * [rows] holds only the body; every body row is padded to [columnCount] on
 * parse. Cell coordinates used across the model count the header as row 0,
 * i.e. body row `i` is coordinate row `i + 1`.
 */
data class TableData(
    val header: List<TableCell>,
    val rows: List<List<TableCell>> = emptyList(),
    val alignments: List<TableAlignment> = emptyList(),
) {
    val columnCount: Int get() = header.size

    /** Every cell in row-major order, header first. */
    val allCells: List<TableCell> get() = header + rows.flatten()

    fun alignmentAt(col: Int): TableAlignment = alignments.getOrElse(col) { TableAlignment.None }

    /** The cell at (row, col) — row 0 is the header. */
    fun cellAt(row: Int, col: Int): TableCell? =
        (if (row == 0) header else rows.getOrNull(row - 1))?.getOrNull(col)

    /** The (row, col) of [cellId], counting the header as row 0. */
    fun locate(cellId: String): Pair<Int, Int>? {
        val headerCol = header.indexOfFirst { it.id == cellId }
        if (headerCol >= 0) return 0 to headerCol
        for ((r, cells) in rows.withIndex()) {
            val col = cells.indexOfFirst { it.id == cellId }
            if (col >= 0) return (r + 1) to col
        }
        return null
    }

    /** A copy with (row, col) replaced by [cell]; unchanged when out of range. */
    fun withCell(row: Int, col: Int, cell: TableCell): TableData {
        if (row == 0) {
            if (col !in header.indices) return this
            return copy(header = header.toMutableList().also { it[col] = cell })
        }
        val bodyIndex = row - 1
        val bodyRow = rows.getOrNull(bodyIndex) ?: return this
        if (col !in bodyRow.indices) return this
        val nextRow = bodyRow.toMutableList().also { it[col] = cell }
        return copy(rows = rows.toMutableList().also { it[bodyIndex] = nextRow })
    }
}

/**
 * A text block's content: the raw editable string plus a parallel per-paragraph
 * attribute list and per-range inline marks.
 *
 * Invariant: paragraphs are the `'\n'`-delimited lines of [text], so
 * `text.split("\n").size == paragraphs.size`. Block separators are stored as a
 * single `'\n'` and re-expanded to `"\n\n"` only at serialize time, exactly
 * like iOS; an EMPTY line is an intentional blank paragraph, persisted as the
 * `&nbsp;` interchange line (EXP-689).
 */
data class RichText(
    val text: String,
    val paragraphs: List<ParagraphAttrs>,
    val marks: List<InlineMark>,
) {
    companion object {
        val EMPTY = RichText("", listOf(ParagraphAttrs.PLAIN), emptyList())

        /** Build a single-paragraph plain RichText. */
        fun plain(text: String): RichText {
            val lines = if (text.isEmpty()) listOf("") else text.split("\n")
            return RichText(text, lines.map { ParagraphAttrs.PLAIN }, emptyList())
        }
    }

    /** The `'\n'`-delimited lines; always at least one entry. */
    val lines: List<String> get() = if (text.isEmpty()) listOf("") else text.split("\n")

    val isEmpty: Boolean get() = text.isEmpty()
}

/** Block-level (paragraph) attributes — the Android analog of iOS's `markdown*` paragraph keys. */
data class ParagraphAttrs(
    val kind: BlockKind = BlockKind.Paragraph,
    val headingLevel: Int = 0,        // 1..6 when kind == Heading (toolbar only emits 1..3)
    val listType: ListType? = null,   // set when kind == ListItem
    val orderedIndex: Int = 0,        // visible number for ordered list items
    val listDepth: Int = 0,           // 0-based nesting depth
    val checked: Boolean = false,     // checklist state
    val codeLang: String? = null,     // fence info for code blocks
) {
    companion object {
        val PLAIN = ParagraphAttrs()
    }
}

enum class BlockKind { Paragraph, Heading, ListItem, Blockquote, CodeBlock, ThematicBreak }

enum class ListType { Bullet, Ordered, Checklist }

/** An inline mark over the `[start, end)` char range of [RichText.text]. */
data class InlineMark(
    val start: Int,
    val end: Int,
    val kind: InlineKind,
    val href: String? = null, // set when kind == Link
)

enum class InlineKind { Bold, Italic, Strikethrough, InlineCode, Link }

/**
 * An image picked locally but not yet uploaded, stashed by its `draft://` URL.
 * Carries the probed pixel size so the editor can reserve correct aspect-ratio
 * space before/while uploading (mirrors iOS `PendingImage`).
 */
data class PendingImage(
    val uri: Uri,
    val bytes: ByteArray,
    val filename: String,
    val contentType: String,
    val width: Int?,
    val height: Int?,
) {
    // ByteArray breaks data-class equality; identity is the draft URL key, so
    // compare by stable scalar fields only.
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is PendingImage) return false
        return uri == other.uri && filename == other.filename &&
            contentType == other.contentType && width == other.width && height == other.height
    }

    override fun hashCode(): Int {
        var result = uri.hashCode()
        result = 31 * result + filename.hashCode()
        result = 31 * result + contentType.hashCode()
        result = 31 * result + (width ?: 0)
        result = 31 * result + (height ?: 0)
        return result
    }
}
