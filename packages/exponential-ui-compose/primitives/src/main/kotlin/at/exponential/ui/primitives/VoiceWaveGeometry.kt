package at.exponential.ui.primitives

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/**
 * VAPP-160: catalog/style.json `voiceWave`'s ring constants (NEW-77),
 * copied; `VoiceWaveGeometryTest` holds them equal to the catalog.
 */
object VoiceWaveStyle {
    const val STEPS = 120
    val RADIUS = doubleArrayOf(0.33, 0.3, 0.26)
    val HARMONICS = arrayOf(intArrayOf(11, 17, 23), intArrayOf(9, 15, 21), intArrayOf(7, 13))
    val PHASE0 = doubleArrayOf(0.4, 1.9, 3.1)
    val RING_AMP = doubleArrayOf(1.0, 0.85, 0.45)
    const val AMP_BASE = 2.0
    const val AMP_LEVEL = 6.0
    const val THINKING_AMP = 2.5
    const val PHASE_SPEED = 0.9
    val FILL_ALPHA = doubleArrayOf(0.1, 0.1, 0.15)
    val STROKE_ALPHA = doubleArrayOf(0.7, 0.8, 0.4)
    val STROKE_WIDTH = doubleArrayOf(1.5, 1.5, 1.0)
    const val CHILD_SCALE = 0.3
    const val CHILD_SCALE_LARGE = 0.22
    const val MAX_BANDS = 8

    /** The recipe parts that colour the rings, in drawing order. */
    val PARTS = listOf("outer", "mid", "inner")
    val ACTIVITIES = setOf("idle", "listening", "thinking", "speaking", "muted")
}

/**
 * VAPP-160: the VoiceWave rings in pure Kotlin (src/voice-wave.ts
 * `voiceWave`, the core's `voice_wave`), so a painter computes a frame
 * without JSON or the native core. One instance per box edge caches what
 * depends on the size alone (each ring's base radius, cos t / sin t per
 * point, the stroke width); [update] rewrites the rings' [Ring.points] IN
 * PLACE for one frame (no allocation), rounded to 0.01 like the reference.
 * Not thread-safe: one instance per painted wave.
 */
class VoiceWaveGeometry(size: Double) {
    /** One ring: `steps + 1` points (x0, y0, x1, y1, …; the last repeats the first), dp from the box's top-left. */
    class Ring(val part: String, val points: FloatArray, val fillAlpha: Float, val strokeAlpha: Float, val strokeWidth: Float)

    val size: Double = if (size.isNaN()) 0.0 else max(0.0, size)
    private val centre = this.size / 2
    private val n = VoiceWaveStyle.STEPS + 1
    private val ts = DoubleArray(n) { i -> i.toDouble() / VoiceWaveStyle.STEPS * PI * 2 }
    private val cosT = DoubleArray(n) { cos(ts[it]) }
    private val sinT = DoubleArray(n) { sin(ts[it]) }
    private val base = DoubleArray(3) { k -> this.size * VoiceWaveStyle.RADIUS[k] }

    val rings: List<Ring> = List(3) { k ->
        Ring(
            part = VoiceWaveStyle.PARTS[k],
            points = FloatArray(2 * n),
            fillAlpha = VoiceWaveStyle.FILL_ALPHA[k].toFloat(),
            strokeAlpha = VoiceWaveStyle.STROKE_ALPHA[k].toFloat(),
            strokeWidth = round2(max(1.0, VoiceWaveStyle.STROKE_WIDTH[k] * this.size / 200)).toFloat(),
        )
    }

    /** The phase of the last [update] (radians, rounded to 0.01). */
    var phase: Double = 0.0
        private set

    init {
        update("idle", 0.0, emptyList(), 0.0, true)
    }

    /** Rewrites [rings] for one frame (the reference's rules; an unknown activity is idle). */
    fun update(rawActivity: String, level: Double, bands: List<Double>, elapsed: Double, reducedMotion: Boolean): VoiceWaveGeometry {
        val activity = activity(rawActivity)
        val lvl = clamp01(level)
        val count = min(bands.size, VoiceWaveStyle.MAX_BANDS)
        val ph = phase(activity, elapsed, reducedMotion)
        val drive = when (activity) {
            "listening", "speaking" -> VoiceWaveStyle.AMP_BASE + VoiceWaveStyle.AMP_LEVEL * lvl
            "thinking" -> VoiceWaveStyle.THINKING_AMP
            else -> 0.0
        }
        val flat = count == 0 || activity == "thinking"
        for (k in 0 until 3) {
            val harmonics = VoiceWaveStyle.HARMONICS[k]
            val amp = VoiceWaveStyle.RING_AMP[k] * size * drive / 200
            val offset = VoiceWaveStyle.PHASE0[k] + ph
            val out = rings[k].points
            for (i in 0 until n) {
                val t = ts[i]
                var r = base[k]
                if (amp != 0.0) {
                    for (j in harmonics.indices) {
                        val band = if (flat) 1.0 else clamp01(bands[(3 * k + j) % count])
                        r += amp / (j + 1) * band * sin(harmonics[j] * t + offset * (j + 1))
                    }
                }
                out[2 * i] = round2(centre + r * cosT[i]).toFloat()
                out[2 * i + 1] = round2(centre + r * sinT[i]).toFloat()
            }
        }
        phase = round2(ph)
        return this
    }

    companion object {
        /** The activity as the catalog names it; anything else is `idle`. */
        fun activity(value: String): String = if (value in VoiceWaveStyle.ACTIVITIES) value else "idle"

        /** Does [activity] move (the phase advances with time)? */
        fun animates(activity: String): Boolean = activity == "listening" || activity == "thinking" || activity == "speaking"

        /** The phase (radians, unrounded) at [elapsed] seconds: 0 when idle, muted or under reduced motion. */
        fun phase(activity: String, elapsed: Double, reducedMotion: Boolean): Double {
            if (reducedMotion || !animates(activity(activity))) return 0.0
            return (if (elapsed.isFinite()) max(0.0, elapsed) else 0.0) * VoiceWaveStyle.PHASE_SPEED
        }

        /** 0..1; NaN and ±∞ are 0 (the reference's `clamp01`). */
        fun clamp01(n: Double): Double = if (n.isFinite()) min(1.0, max(0.0, n)) else 0.0

        /** JS `Math.round(n * 100) / 100`: halves round up (toward +∞), also below zero. */
        fun round2(n: Double): Double {
            val x = n * 100
            val f = floor(x)
            return (if (x - f >= 0.5) f + 1 else f) / 100
        }
    }
}
