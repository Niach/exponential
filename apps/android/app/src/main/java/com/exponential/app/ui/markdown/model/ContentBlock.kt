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
}

/**
 * A text block's content: the raw editable string plus a parallel per-paragraph
 * attribute list and per-range inline marks.
 *
 * Invariant: paragraphs are the `'\n'`-delimited lines of [text], so
 * `text.split("\n").size == paragraphs.size`. (Blank lines never appear inside a
 * text block — block separators are stored as a single `'\n'` and re-expanded to
 * `"\n\n"` only at serialize time, exactly like iOS.)
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
