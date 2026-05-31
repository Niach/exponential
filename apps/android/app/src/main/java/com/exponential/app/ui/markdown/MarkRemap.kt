package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineMark

/**
 * Adjusts a paragraph's inline marks when its text is edited, using a
 * prefix/suffix diff to locate the changed span. Keeps bold/italic/etc. ranges
 * attached to the right characters as the user types or deletes — without this,
 * marks would drift on every keystroke.
 */
object MarkRemap {

    fun remap(oldText: String, newText: String, marks: List<InlineMark>): List<InlineMark> {
        if (marks.isEmpty() || oldText == newText) return marks

        // Common prefix length.
        var p = 0
        val maxPrefix = minOf(oldText.length, newText.length)
        while (p < maxPrefix && oldText[p] == newText[p]) p++

        // Common suffix length (not overlapping the prefix).
        var s = 0
        while (
            s < (minOf(oldText.length, newText.length) - p) &&
            oldText[oldText.length - 1 - s] == newText[newText.length - 1 - s]
        ) s++

        val removedStart = p
        val removedEnd = oldText.length - s          // exclusive, in old coords
        val insertedLen = newText.length - s - p
        val removedLen = removedEnd - removedStart
        val delta = insertedLen - removedLen
        val pureInsertion = removedLen == 0

        // A mark's start and end bias differently at the edit boundary: text
        // inserted exactly at a mark's start pushes the start right (the typed
        // text is outside the mark), while text inserted exactly at a mark's end
        // leaves the end put (the typed text is also outside). This keeps
        // "type before bold" non-bold and "type after bold" non-bold, while
        // "type inside bold" extends it.
        fun adjustStart(pos: Int): Int = when {
            pos < removedStart -> pos
            pos == removedStart -> if (pureInsertion) pos + insertedLen else pos
            pos >= removedEnd -> pos + delta
            else -> removedStart
        }

        fun adjustEnd(pos: Int): Int = when {
            pos <= removedStart -> pos
            pos >= removedEnd -> pos + delta
            else -> removedStart
        }

        val out = ArrayList<InlineMark>(marks.size)
        for (m in marks) {
            val start = adjustStart(m.start).coerceIn(0, newText.length)
            val end = adjustEnd(m.end).coerceIn(0, newText.length)
            if (end > start) out.add(m.copy(start = start, end = end))
        }
        return out
    }
}
