package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs

/**
 * Maintains a text run's per-paragraph attribute list across arbitrary IME
 * edits (EXP-534). A run holds every '\n'-separated line between two images in
 * ONE multi-line field, so Enter, backspace-over-a-newline and multi-line paste
 * all arrive as plain text diffs — this maps the old paragraph list onto the
 * new line structure, preserving the `lines == paragraphs.size` invariant.
 *
 * Rules (the per-row split/merge semantics the one-field-per-line editor had):
 *  - the line containing the edit keeps its attributes;
 *  - each ADDITIONAL inserted line continues its list (checklist unchecked) /
 *    code fence / quote, and everything else becomes a plain paragraph;
 *  - deleting a '\n' merges lines keeping the FIRST line's attributes — except
 *    a pure deletion that consumes the first line ENTIRELY (it started at the
 *    line's start), where the surviving content is wholly the last touched
 *    line's and keeps THAT line's attributes.
 */
internal object ParaRemap {

    /** Paragraph (line) index containing [offset] — the count of '\n' before it. */
    fun paraIndexAt(text: String, offset: Int): Int {
        val end = offset.coerceIn(0, text.length)
        var n = 0
        for (i in 0 until end) if (text[i] == '\n') n++
        return n
    }

    /** Half-open char bounds `[start, end)` of line [index] (sans its '\n'). */
    fun paragraphBounds(text: String, index: Int): Pair<Int, Int> {
        var start = 0
        var line = 0
        while (line < index) {
            val nl = text.indexOf('\n', start)
            if (nl < 0) return text.length to text.length
            start = nl + 1
            line++
        }
        val nl = text.indexOf('\n', start)
        return start to (if (nl < 0) text.length else nl)
    }

    fun lineCount(text: String): Int = text.count { it == '\n' } + 1

    /** The attrs a NEW line created below one with [attrs] inherits. */
    fun continuationOf(attrs: ParagraphAttrs): ParagraphAttrs = when (attrs.kind) {
        BlockKind.ListItem ->
            if (attrs.listType == ListType.Checklist) attrs.copy(checked = false) else attrs
        BlockKind.CodeBlock, BlockKind.Blockquote -> attrs
        else -> ParagraphAttrs.PLAIN
    }

    fun remap(
        diff: TextDiff,
        oldText: String,
        newText: String,
        old: List<ParagraphAttrs>,
    ): List<ParagraphAttrs> {
        if (oldText == newText) return old
        val oldLines = lineCount(oldText)
        // Tolerate a drifted list rather than throw — the invariant is healed.
        val paras =
            if (old.size == oldLines) old
            else List(oldLines) { old.getOrElse(it) { ParagraphAttrs.PLAIN } }

        val firstLine = paraIndexAt(oldText, diff.removedStart)
        val lastLine = paraIndexAt(oldText, diff.removedEnd)
        var insertedNewlines = 0
        for (i in diff.removedStart until diff.insertedEnd.coerceAtMost(newText.length)) {
            if (newText[i] == '\n') insertedNewlines++
        }

        val out = ArrayList<ParagraphAttrs>(paras.size + insertedNewlines - (lastLine - firstLine))
        out.addAll(paras.subList(0, firstLine))
        val anchorLine =
            if (diff.insertedLen == 0 && lastLine > firstLine &&
                diff.removedStart == paragraphBounds(oldText, firstLine).first
            ) lastLine else firstLine
        val anchor = paras[anchorLine]
        out.add(anchor)
        if (insertedNewlines > 0) {
            val cont = continuationOf(anchor)
            repeat(insertedNewlines) { out.add(cont) }
        }
        out.addAll(paras.subList(lastLine + 1, paras.size))
        return out
    }
}
