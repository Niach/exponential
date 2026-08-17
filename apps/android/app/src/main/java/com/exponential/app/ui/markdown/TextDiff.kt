package com.exponential.app.ui.markdown

/**
 * The changed span between two versions of a text, located by prefix/suffix
 * diff. One diff feeds BOTH remaps of an edit — inline marks ([MarkRemap]) and
 * paragraph attributes ([ParaRemap]) — so the two can never disagree about
 * where the edit happened.
 */
internal data class TextDiff(
    /** Start of the removed span, old coordinates (== insertion point). */
    val removedStart: Int,
    /** Exclusive end of the removed span, old coordinates. */
    val removedEnd: Int,
    val insertedLen: Int,
) {
    val removedLen: Int get() = removedEnd - removedStart
    val delta: Int get() = insertedLen - removedLen

    /** Exclusive end of the inserted span, NEW coordinates. */
    val insertedEnd: Int get() = removedStart + insertedLen

    val isPureInsertion: Boolean get() = removedLen == 0

    companion object {
        /**
         * Locate the edit. A prefix/suffix diff is ambiguous when the edit
         * repeats its surroundings — "abc\ndef" with Enter pressed at offset 3
         * or 4 both yield "abc\n\ndef", but the paragraph that should inherit
         * the line's attributes differs. The caret AFTER the edit sits at the
         * end of an insertion (and at the deletion point of a backspace), so
         * when [caretAfter] is given and reproduces the same old→new edit, the
         * caret-anchored window wins over the greedy prefix match.
         */
        fun of(oldText: String, newText: String, caretAfter: Int? = null): TextDiff {
            var p = 0
            val maxPrefix = minOf(oldText.length, newText.length)
            while (p < maxPrefix && oldText[p] == newText[p]) p++

            var s = 0
            while (
                s < (minOf(oldText.length, newText.length) - p) &&
                oldText[oldText.length - 1 - s] == newText[newText.length - 1 - s]
            ) s++

            val diff = TextDiff(
                removedStart = p,
                removedEnd = oldText.length - s,
                insertedLen = newText.length - s - p,
            )
            if (caretAfter == null) return diff

            if (diff.isPureInsertion && diff.insertedLen > 0) {
                val start = caretAfter - diff.insertedLen
                if (start != diff.removedStart && start >= 0 && caretAfter <= newText.length &&
                    newText.removeRange(start, caretAfter) == oldText
                ) {
                    return TextDiff(start, start, diff.insertedLen)
                }
            } else if (diff.insertedLen == 0 && diff.removedLen > 0) {
                val start = caretAfter
                if (start != diff.removedStart && start >= 0 &&
                    start + diff.removedLen <= oldText.length &&
                    oldText.removeRange(start, start + diff.removedLen) == newText
                ) {
                    return TextDiff(start, start + diff.removedLen, 0)
                }
            }
            return diff
        }
    }
}
