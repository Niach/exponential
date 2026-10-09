package at.exponential.ui.kitchensink

import android.util.Log
import at.exponential.ui.ffi.FfiHeightRequest
import at.exponential.ui.ffi.FfiIntrinsics
import at.exponential.ui.ffi.FfiLeaf
import at.exponential.ui.ffi.Measurer
import at.exponential.ui.measure.SurfaceMeasurer
import at.exponential.ui.model.PassStats
import at.exponential.ui.model.SurfaceModel
import kotlinx.coroutines.delay

/**
 * The timed pass series behind `--ez benchLoop true` (and a tap on the host
 * line). Every pass logs one `ExponentialUI` line
 * `exponential-ui: phase=… pass=… layout_ns=… wall_ns=… upcalls=… measure_calls=… nodes=…`
 * (`layout_ns` = the core's pass incl. the JNA upcalls and the Kotlin
 * measurer, `wall_ns` = the model's whole `pass()`); each phase ends with a
 * `summary` line (best / median). Phases:
 *
 * - `warm`: 30 full passes flipping the width 390 / 411 dp (the core's
 *   measure memo answers widths it has seen: what a resize costs).
 * - `cached`: 30 passes with nothing dirty.
 * - `remeasure`: 10 passes after `invalidateMeasures()` (a new measure
 *   identity: EVERY leaf crosses to Kotlin again, JIT warm).
 * - `fixed`: 30 width flips under the core's fixed measure (no JNA, no
 *   Kotlin: the engine alone).
 * - `split-warm` / `split-remeasure`: the same passes driven straight on
 *   the core surface with a timing wrapper around [SurfaceMeasurer], so
 *   `kotlin_ns` (time inside the Kotlin measurer) and `jna_ns` (layout −
 *   kotlin − the engine's fixed-measure median) split the cost.
 */
object Bench {
    private class TimedMeasurer(val inner: SurfaceMeasurer) : Measurer {
        var kotlinNs = 0L

        override fun measureId(): ULong = inner.measureId()

        override fun measureIntrinsics(leaves: List<FfiLeaf>): List<FfiIntrinsics> {
            val t = System.nanoTime()
            return inner.measureIntrinsics(leaves).also { kotlinNs += System.nanoTime() - t }
        }

        override fun measureHeights(leaves: List<FfiLeaf>, requests: List<FfiHeightRequest>): List<Float> {
            val t = System.nanoTime()
            return inner.measureHeights(leaves, requests).also { kotlinNs += System.nanoTime() - t }
        }
    }

    private fun line(label: String, phase: String, pass: Int, s: PassStats, extra: String = "") {
        Log.i(LOG_TAG, "exponential-ui: surface=$label phase=$phase pass=$pass layout_ns=${s.layoutNs} wall_ns=${s.wallNs} upcalls=${s.upcalls} measure_calls=${s.measureCalls} nodes=${s.nodes}$extra")
    }

    private fun summary(label: String, phase: String, values: List<Long>, key: String) {
        if (values.isEmpty()) return
        val sorted = values.sorted()
        Log.i(LOG_TAG, "exponential-ui: summary surface=$label phase=$phase $key best=${sorted.first()} median=${sorted[sorted.size / 2]} worst=${sorted.last()} n=${sorted.size}")
    }

    /** Log the first pass of the process (cold: JNA load, JIT, first text layout). */
    fun logCold(model: SurfaceModel) {
        line(model.id, "cold", model.passCount, model.stats)
    }

    /** Run the whole series on [model] (main thread), then restore its viewport. */
    suspend fun run(model: SurfaceModel) {
        val label = model.id
        val width = model.width
        val height = model.viewportHeight
        val widths = floatArrayOf(390f, 411f)
        suspend fun series(phase: String, n: Int, step: (Int) -> Unit): List<PassStats> {
            val out = ArrayList<PassStats>(n)
            repeat(n) { i ->
                step(i)
                val s = model.stats
                out += s
                line(label, phase, i, s)
                delay(20)
            }
            summary(label, phase, out.map { it.layoutNs }, "layout_ns")
            summary(label, phase, out.map { it.wallNs }, "wall_ns")
            return out
        }
        series("warm", 30) { i -> model.setViewport(widths[i % 2], height) }
        series("cached", 30) { model.pass() }
        series("remeasure", 10) { model.invalidateMeasures() }
        model.fixedMeasure = true
        val fixed = series("fixed", 30) { i -> model.setViewport(widths[i % 2] + 0.5f * (i % 3), height) }
        model.fixedMeasure = false
        val engine = fixed.map { it.layoutNs }.sorted()[fixed.size / 2]

        // The split: the core surface directly, the Kotlin measurer timed.
        var generation = 1_000_000L
        fun measurer(fresh: Boolean): TimedMeasurer {
            if (fresh) generation += 1
            return TimedMeasurer(SurfaceMeasurer(model.effectiveTheme, model.mode, model.extensions, emptyMap(), generation, model.textShaper()))
        }
        for ((phase, fresh, n) in listOf(Triple("split-warm", false, 30), Triple("split-remeasure", true, 10))) {
            val layouts = ArrayList<Long>()
            val kotlins = ArrayList<Long>()
            val jnas = ArrayList<Long>()
            val perUpcall = ArrayList<Long>()
            measurer(true)
            repeat(n) { i ->
                val m = measurer(fresh)
                model.surface.setViewport(widths[i % 2], height, null)
                val t0 = System.nanoTime()
                val out = model.surface.layout(m)
                val wall = System.nanoTime() - t0
                val layout = out.layoutNs.toLong()
                val jna = (layout - m.kotlinNs - engine).coerceAtLeast(0)
                layouts += layout
                kotlins += m.kotlinNs
                jnas += jna
                if (out.upcalls.toInt() > 0) perUpcall += jna / out.upcalls.toLong()
                Log.i(LOG_TAG, "exponential-ui: surface=$label phase=$phase pass=$i layout_ns=$layout wall_ns=$wall upcalls=${out.upcalls} measure_calls=${m.inner.calls} kotlin_ns=${m.kotlinNs} engine_ns=$engine jna_ns=$jna")
                delay(20)
            }
            summary(label, phase, layouts, "layout_ns")
            summary(label, phase, kotlins, "kotlin_ns")
            summary(label, phase, jnas, "jna_ns")
            summary(label, phase, perUpcall, "jna_per_upcall_ns")
        }
        model.invalidateMeasures()
        model.setViewport(width, height)
        Log.i(LOG_TAG, "exponential-ui: bench done surface=$label")
    }
}
