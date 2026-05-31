package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark

/** Add / remove / query inline marks over a character range within one paragraph. */
object MarkOps {

    fun hasMarkOver(marks: List<InlineMark>, start: Int, end: Int, kind: InlineKind): Boolean {
        if (end <= start) return false
        // Every character in [start,end) must be covered by some mark of this kind.
        var pos = start
        while (pos < end) {
            val covering = marks.firstOrNull { it.kind == kind && it.start <= pos && it.end > pos }
                ?: return false
            pos = covering.end
        }
        return true
    }

    fun addMark(
        marks: List<InlineMark>,
        start: Int,
        end: Int,
        kind: InlineKind,
        href: String? = null,
    ): List<InlineMark> {
        if (end <= start) return marks
        val same = marks.filter { it.kind == kind }
        val others = marks.filter { it.kind != kind }
        // Merge the new range with overlapping/adjacent same-kind ranges.
        var lo = start
        var hi = end
        val disjoint = mutableListOf<InlineMark>()
        for (m in same) {
            if (m.end < lo || m.start > hi) {
                disjoint.add(m)
            } else {
                lo = minOf(lo, m.start)
                hi = maxOf(hi, m.end)
            }
        }
        disjoint.add(InlineMark(lo, hi, kind, href))
        return others + disjoint
    }

    fun removeMark(marks: List<InlineMark>, start: Int, end: Int, kind: InlineKind): List<InlineMark> {
        if (end <= start) return marks
        val out = mutableListOf<InlineMark>()
        for (m in marks) {
            if (m.kind != kind || m.end <= start || m.start >= end) {
                out.add(m)
                continue
            }
            // Keep the portion before the removed range.
            if (m.start < start) out.add(m.copy(end = start))
            // Keep the portion after the removed range.
            if (m.end > end) out.add(m.copy(start = end))
        }
        return out
    }

    /** Shift marks for a sub-range starting at [from] into local (0-based) coordinates. */
    fun slice(marks: List<InlineMark>, from: Int, to: Int): List<InlineMark> =
        marks.mapNotNull { m ->
            val s = maxOf(m.start, from)
            val e = minOf(m.end, to)
            if (e > s) m.copy(start = s - from, end = e - from) else null
        }

    /** Offset all marks by [delta]. */
    fun offset(marks: List<InlineMark>, delta: Int): List<InlineMark> =
        marks.map { it.copy(start = it.start + delta, end = it.end + delta) }
}
