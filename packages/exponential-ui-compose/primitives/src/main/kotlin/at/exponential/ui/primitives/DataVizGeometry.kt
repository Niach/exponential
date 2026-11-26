package at.exponential.ui.primitives

import kotlin.math.floor
import kotlin.math.max

/**
 * Group D (genUI round 4): the Ring arcs (`sweep`, `rings`) and the
 * AudioPlayer waveform (`peaks`), the SDK's src/data-viz.ts;
 * `DataVizGeometryTest` replays fixtures/data-viz.json through it.
 */
object DataViz {
    /** The waveform's locked bar geometry (px). */
    const val BAR_WIDTH = 2.0
    const val GAP = 2.0
    const val MIN_HEIGHT = 2.0
    const val HEIGHT = 24.0

    /** One ring as authored (a non-number value or max = absent). */
    data class RingSpec(val value: Double?, val max: Double?, val tone: String?)

    /** One drawn ring: centre-line radius, filled fraction, tone. */
    data class RingArc(val r: Double, val fraction: Double, val tone: String)

    /** `start` / `sweep` in degrees clockwise from 12 o'clock. */
    data class RingArcs(val start: Double, val sweep: Double, val rings: List<RingArc>)

    data class WaveBar(val x: Double, val y: Double, val width: Double, val height: Double, val played: Boolean)

    private fun jsRound(n: Double) = floor(n + 0.5)
    private fun round2(n: Double) = jsRound(n * 100) / 100
    private fun Double?.finite(d: Double) = this?.takeIf { it.isFinite() } ?: d

    fun ringArcs(size: Double, stroke: Double, value: Double?, tone: String?, sweep: Double?, rings: List<RingSpec>?): RingArcs {
        val s = sweep.finite(360.0).coerceIn(0.0, 360.0)
        val start = if (s >= 360) 0.0 else 180 + (360 - s) / 2
        val baseTone = tone?.takeIf { it.isNotEmpty() } ?: "primary"
        val list = rings?.takeIf { it.isNotEmpty() } ?: listOf(RingSpec(value, 1.0, null))
        val out = mutableListOf<RingArc>()
        list.forEachIndexed { i, ring ->
            val r = size / 2 - stroke / 2 - i * stroke * 2
            if (r < stroke) return@forEachIndexed
            val m = ring.max.finite(1.0)
            val fraction = if (m > 0) (ring.value.finite(0.0) / m).coerceIn(0.0, 1.0) else 0.0
            out += RingArc(round2(r), jsRound(fraction * 10000) / 10000, ring.tone?.takeIf { it.isNotEmpty() } ?: baseTone)
        }
        return RingArcs(round2(start), round2(s), out)
    }

    fun waveformBars(width: Double, peaks: List<Double?>, progress: Double?): List<WaveBar> {
        val p = peaks.map { it.finite(0.0).coerceIn(0.0, 1.0) }
        if (p.isEmpty()) return emptyList()
        val n = max(0.0, floor((width + GAP) / (BAR_WIDTH + GAP))).toInt()
        if (n == 0) return emptyList()
        val x0 = (width - (n * (BAR_WIDTH + GAP) - GAP)) / 2
        val at = progress.finite(0.0).coerceIn(0.0, 1.0) * width
        return (0 until n).map { i ->
            val peak = if (p.size > n) {
                val from = i * p.size / n
                val to = max(from + 1, (i + 1) * p.size / n)
                p.subList(from, to).max()
            } else {
                p[i * p.size / n]
            }
            val h = max(MIN_HEIGHT, peak * HEIGHT)
            val x = x0 + i * (BAR_WIDTH + GAP)
            WaveBar(round2(x), round2((HEIGHT - h) / 2), BAR_WIDTH, round2(h), at > 0 && x + BAR_WIDTH / 2 <= at)
        }
    }

    /** A press or drag at [x] on a [width] track → the seek fraction 0..1. */
    fun seekFraction(x: Double, width: Double): Double = if (width > 0) (x / width).coerceIn(0.0, 1.0) else 0.0
}
