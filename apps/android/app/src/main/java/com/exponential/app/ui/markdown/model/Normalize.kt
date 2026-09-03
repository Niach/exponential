package com.exponential.app.ui.markdown.model

/**
 * Enforce the structural invariants of a block document — a verbatim port of
 * iOS `ContentBlock.normalize` (`MarkdownConversion.swift`):
 *
 * 1. An empty document becomes exactly one empty [ContentBlock.TextBlock].
 * 2. The first block is always a text block (insert empty text before a leading
 *    block-level block).
 * 3. The last block is always a text block (append empty text after a trailing one).
 * 4. No two block-level blocks are adjacent (insert empty text between them).
 *
 * "Block-level" is every non-text block: images and, since EXP-726, tables.
 * These guarantee every one of them has a text block above and below it, so
 * backspace merges and caret placement always have somewhere to land.
 */
fun normalizeBlocks(blocks: MutableList<ContentBlock>) {
    if (blocks.isEmpty()) {
        blocks.add(ContentBlock.TextBlock(content = RichText.EMPTY))
        return
    }
    if (!blocks.first().isText) {
        blocks.add(0, ContentBlock.TextBlock(content = RichText.EMPTY))
    }
    if (!blocks.last().isText) {
        blocks.add(ContentBlock.TextBlock(content = RichText.EMPTY))
    }
    var i = 1
    while (i < blocks.size) {
        if (!blocks[i].isText && !blocks[i - 1].isText) {
            blocks.add(i, ContentBlock.TextBlock(content = RichText.EMPTY))
        }
        i++
    }
}

/** Whether this block is an editable text run (everything else is block-level). */
val ContentBlock.isText: Boolean get() = this is ContentBlock.TextBlock

/** Convenience for immutable callers. */
fun List<ContentBlock>.normalized(): List<ContentBlock> {
    val mutable = toMutableList()
    normalizeBlocks(mutable)
    return mutable
}
