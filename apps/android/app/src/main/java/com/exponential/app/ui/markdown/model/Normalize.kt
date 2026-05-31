package com.exponential.app.ui.markdown.model

/**
 * Enforce the structural invariants of a block document — a verbatim port of
 * iOS `ContentBlock.normalize` (`MarkdownConversion.swift`):
 *
 * 1. An empty document becomes exactly one empty [ContentBlock.TextBlock].
 * 2. The first block is always a text block (insert empty text before a leading image).
 * 3. The last block is always a text block (append empty text after a trailing image).
 * 4. No two image blocks are adjacent (insert empty text between them).
 *
 * These guarantee every image has a text block above and below it, so backspace
 * merges and caret placement always have somewhere to land.
 */
fun normalizeBlocks(blocks: MutableList<ContentBlock>) {
    if (blocks.isEmpty()) {
        blocks.add(ContentBlock.TextBlock(content = RichText.EMPTY))
        return
    }
    if (blocks.first() is ContentBlock.ImageBlock) {
        blocks.add(0, ContentBlock.TextBlock(content = RichText.EMPTY))
    }
    if (blocks.last() is ContentBlock.ImageBlock) {
        blocks.add(ContentBlock.TextBlock(content = RichText.EMPTY))
    }
    var i = 1
    while (i < blocks.size) {
        if (blocks[i] is ContentBlock.ImageBlock && blocks[i - 1] is ContentBlock.ImageBlock) {
            blocks.add(i, ContentBlock.TextBlock(content = RichText.EMPTY))
        }
        i++
    }
}

/** Convenience for immutable callers. */
fun List<ContentBlock>.normalized(): List<ContentBlock> {
    val mutable = toMutableList()
    normalizeBlocks(mutable)
    return mutable
}
