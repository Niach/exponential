package at.exponential.ui.primitives

import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

/**
 * #FEED-224 (`fixtures/table-fit.json`): THE column fit of a Markdown table,
 * the twin of packages/exponential-ui/src/table-fit.ts. Each column comes
 * measured in content px ([min] = its widest unbreakable piece, a chip
 * whole; [max] = its widest cell on one line); [fit] returns whole-pixel
 * content widths: under `available` when every column fits on one line (the
 * table hugs its content), over it when the minimums do not fit (the table
 * scrolls sideways).
 */
object MarkdownTableFit {
    /** The widest a column's min-content counts (the fixture's `wordCap`). */
    const val WORD_CAP: Double = 160.0

    data class Column(val min: Double, val max: Double)

    fun fit(columns: List<Column>, available: Double, cap: Double = WORD_CAP): List<Double> {
        val avail = floor(max(available, 0.0))
        val lo = columns.map { min(ceil(max(it.min, 0.0)), cap) }
        val hi = columns.mapIndexed { i, c -> max(ceil(max(c.max, 0.0)), lo[i]) }
        if (lo.sum() >= avail) return lo
        if (hi.sum() <= avail) return hi
        // Water-fill: one level L with Σ clamp(L, lo, hi) = avail.
        fun at(level: Double): Double = lo.indices.sumOf { min(max(level, lo[it]), hi[it]) }
        val points = (lo + hi).distinct().sorted()
        var k = 0
        while (at(points[k + 1]) < avail) k++
        val a = points[k]
        val growing = lo.indices.filter { lo[it] <= a && hi[it] > a }
        val level = a + (avail - at(a)) / growing.size
        val out = lo.indices.map { min(max(floor(level), lo[it]), hi[it]) }.toMutableList()
        var left = avail - out.sum()
        for (i in growing) {
            if (left <= 0.0) break
            if (out[i] < hi[i]) {
                out[i] += 1.0
                left -= 1.0
            }
        }
        return out
    }
}
