package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineMark

/**
 * Adjusts a text run's inline marks when its text is edited, using a
 * prefix/suffix diff ([TextDiff]) to locate the changed span. Keeps
 * bold/italic/etc. ranges attached to the right characters as the user types or
 * deletes — without this, marks would drift on every keystroke.
 */
object MarkRemap {

    fun remap(oldText: String, newText: String, marks: List<InlineMark>): List<InlineMark> =
        remap(TextDiff.of(oldText, newText), newText, marks)

    internal fun remap(diff: TextDiff, newText: String, marks: List<InlineMark>): List<InlineMark> {
        if (marks.isEmpty() || (diff.removedLen == 0 && diff.insertedLen == 0)) return marks

        val removedStart = diff.removedStart
        val removedEnd = diff.removedEnd
        val insertedLen = diff.insertedLen
        val delta = diff.delta
        val pureInsertion = diff.isPureInsertion

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
