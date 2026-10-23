package at.exponential.ui

import at.exponential.ui.host.NoHost
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.BenchCounters
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.scrollTo
import at.exponential.ui.model.scrollToIndex
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers

/**
 * VAPP-105: the Exponential UI benchmark suite driven through THIS painter's
 * model (`packages/exponential-ui-bench/scenarios/<id>.json`, the contract every
 * renderer runs). One "pass" here = the model's [SurfaceModel.pass] as the
 * setters trigger it: the core through JNA, the real Compose text measure,
 * the nodes and visuals read back — no composition. The same file compiles
 * in the JVM (Robolectric) and the instrumented source set: `read` hands
 * over a scenario's JSON, `shaper` the platform's text measure, `memoryMb`
 * the process's footprint. [run] repeats the whole suite `repeat` times and
 * reports the median per metric (`coldStartMs` only from the first run, the
 * only cold one).
 */
class BenchSuite(
    private val read: (String) -> JsonValue,
    private val shaper: () -> TextShaper,
    private val memoryMb: () -> Double,
) {
    companion object {
        val SCENARIOS = listOf("cold-start", "card-grid", "list-100k", "streaming", "theme-switch", "resize", "deep-nesting", "template-10k", "overlay", "table-1k")

        /** `item` templates: `{i}` = the index, `{i % N}` = the index modulo N; every field a string (src/scenarios.ts generateItems). */
        fun generateItems(spec: JsonValue): JsonValue {
            val count = spec["count"]!!.number!!.toInt()
            val item = spec["item"]!!.obj!!
            val mod = Regex("\\{i % (\\d+)\\}")
            return JsonValue.Arr(
                List(count) { i ->
                    JsonValue.Obj(item.mapValues { (_, t) -> JsonValue.Str(mod.replace(t.string!!) { m -> (i % m.groupValues[1].toInt()).toString() }.replace("{i}", i.toString())) })
                },
            )
        }

        fun median(v: List<Double>): Double {
            if (v.isEmpty()) return 0.0
            val s = v.sorted()
            return if (s.size % 2 == 1) s[s.size / 2] else (s[s.size / 2 - 1] + s[s.size / 2]) / 2
        }

        fun percentile(v: List<Double>, p: Double): Double {
            if (v.isEmpty()) return 0.0
            val s = v.sorted()
            return s[kotlin.math.min(s.size - 1, kotlin.math.ceil(p * s.size).toInt() - 1).coerceAtLeast(0)]
        }
    }

    private var coldDone = false

    /** Every scenario `repeat` times → scenario id → metric → the median. */
    fun run(repeat: Int): Map<String, Map<String, Double>> {
        val samples = LinkedHashMap<String, LinkedHashMap<String, MutableList<Double>>>()
        repeat(repeat) {
            for (id in SCENARIOS) {
                val r = runOnce(id)
                val m = samples.getOrPut(id) { LinkedHashMap() }
                for ((k, v) in r) m.getOrPut(k) { ArrayList() }.add(v)
            }
        }
        return samples.mapValues { (_, m) -> m.mapValues { (_, v) -> median(v) } }
    }

    fun runOnce(id: String): Map<String, Double> {
        val s = read(id)
        return when (id) {
            "cold-start" -> coldStart(s)
            "card-grid", "resize", "deep-nesting" -> warmWidths(s, id == "deep-nesting")
            "list-100k" -> list100k(s)
            "streaming" -> streaming(s)
            "theme-switch" -> themeSwitch(s)
            "template-10k" -> template10k(s)
            "overlay" -> overlay(s)
            "table-1k" -> table1k(s)
            else -> error("unknown scenario $id")
        }
    }

    // ---- helpers ----

    private fun model(s: JsonValue): SurfaceModel {
        val options = SurfaceOptions(
            catalogId = s["catalogId"]!!.string!!,
            theme = ThemeHandle.builtin(s["theme"]!!.string!!),
            mode = if (s["mode"]?.string == "dark") Mode.Dark else Mode.Light,
        )
        return SurfaceModel("bench", options, NoHost, CoroutineScope(Dispatchers.Unconfined)) { shaper() }
    }

    /** The surface and the data handed over (no viewport yet, so no pass);
     *  `items` = the generated rows, built before the clock (VAPP-149). */
    private fun load(m: SurfaceModel, s: JsonValue, items: JsonValue?) {
        val surface = s["surface"]!!
        surface["nested"]?.let { m.setNested(it.orderedJson) }
        surface["components"]?.let { m.setComponents(it.orderedJson) }
        s["data"]?.let { data ->
            val gen = data["generate"]
            if (gen != null) m.setData(gen["path"]!!.string!!, items!!) else m.setData("", data)
        }
    }

    private fun viewport(s: JsonValue): Pair<Float, Float> = s["viewport"]!!["width"]!!.number!!.toFloat() to s["viewport"]!!["height"]!!.number!!.toFloat()

    private inline fun ms(block: () -> Unit): Double {
        val t = System.nanoTime()
        block()
        return (System.nanoTime() - t) / 1e6
    }

    /** First paint: the content loaded and the viewport set (the first pass). */
    private fun firstPaint(m: SurfaceModel, s: JsonValue): Double {
        val (w, h) = viewport(s)
        // VAPP-149: the rows are generated OUTSIDE the timed window (firstPaintMs times the renderer).
        val items = s["data"]?.get("generate")?.let { generateItems(it) }
        return ms {
            load(m, s, items)
            m.setViewport(w, h)
        }
    }

    private fun liveNodes(m: SurfaceModel) = m.nodes.count { !it.hidden }.toDouble()

    private fun depthOf(m: SurfaceModel): Double {
        var deepest = 0
        for (n in m.nodes) {
            if (n.hidden) continue
            var d = 1
            var p = n.parent
            while (p != null) {
                d += 1
                p = m.nodes[p].parent
            }
            deepest = kotlin.math.max(deepest, d)
        }
        return deepest.toDouble()
    }

    // ---- scenarios ----

    private fun coldStart(s: JsonValue): Map<String, Double> {
        val out = LinkedHashMap<String, Double>()
        if (!coldDone) {
            coldDone = true
            val cold = ms {
                val m = model(s)
                firstPaint(m, s)
            }
            out["coldStartMs"] = cold
        }
        val m = model(s)
        out["firstPaintMs"] = firstPaint(m, s)
        out["nodes"] = liveNodes(m)
        out["memoryMb"] = memoryMb()
        return out
    }

    private fun warmWidths(s: JsonValue, depth: Boolean): Map<String, Double> {
        val m = model(s)
        val first = firstPaint(m, s)
        val (_, h) = viewport(s)
        val widths = s["steps"]!!["widths"]!!.array!!.map { it.number!!.toFloat() }
        val passes = s["steps"]!!["passes"]!!.number!!.toInt()
        val times = ArrayList<Double>(passes)
        var upcalls = 0
        var measures = 0
        for (i in 0 until passes) {
            val w = widths[i % widths.size]
            times.add(ms { m.setViewport(w, h) })
            upcalls = kotlin.math.max(upcalls, m.stats.upcalls)
            measures = kotlin.math.max(measures, m.stats.measureCalls)
        }
        val out = LinkedHashMap<String, Double>()
        if (depth) {
            out["firstPaintMs"] = first
            out["warmPassMs"] = median(times)
            out["depth"] = depthOf(m)
        } else if (s["id"]!!.string == "resize") {
            out["resizeMs"] = median(times)
            out["upcallsPerResize"] = upcalls.toDouble()
        } else {
            out["warmPassMs"] = median(times)
            out["upcallsPerPass"] = upcalls.toDouble()
            out["measureCallsPerPass"] = measures.toDouble()
        }
        return out
    }

    private fun list100k(s: JsonValue): Map<String, Double> {
        val m = model(s)
        val first = firstPaint(m, s)
        val (_, h) = viewport(s)
        val steps = s["steps"]!!["scroll"]!!["count"]!!.number!!.toInt()
        val counters = BenchCounters()
        SurfaceModel.benchCounters = counters
        var measures = 0
        val t1 = System.nanoTime()
        for (i in 1..steps) {
            m.scrollTo("root", 0f, i * h)
            measures += m.stats.measureCalls
        }
        val step = (System.nanoTime() - t1) / 1e6 / steps
        SurfaceModel.benchCounters = null
        val index = s["steps"]!!["scrollToIndex"]!!["index"]!!.number!!.toInt()
        val align = s["steps"]!!["scrollToIndex"]!!["align"]!!.string
        val jump = ms { m.scrollToIndex("root", index, align) }
        val list = m.indexOf("root")
        val items = m.nodes.count { !it.hidden && it.parent == list && it.id.startsWith("bench-row") }
        check(m.nodes.any { !it.hidden && (it.props["title"]?.string == "Row $index" || it.props["text"]?.string == "Row $index") }) { "row $index rendered after the jump" }
        return linkedMapOf(
            "firstPaintMs" to first,
            "scrollStepMs" to step,
            "scrollToIndexMs" to jump,
            "renderedItems" to items.toDouble(),
            "measureCallsPerStep" to measures.toDouble() / steps,
            "ffiCallsPerStep" to counters.calls.toDouble() / steps,
            "ffiBytesPerStep" to (counters.bytesIn + counters.bytesOut).toDouble() / steps,
            "memoryMb" to memoryMb(),
        )
    }

    private fun streaming(s: JsonValue): Map<String, Double> {
        val m = model(s)
        firstPaint(m, s)
        val messages = s["steps"]!!["messages"]!!.array!!
        val counters = BenchCounters()
        SurfaceModel.benchCounters = counters
        val times = ArrayList<Double>(messages.size)
        for (msg in messages) times.add(ms { m.apply(msg.orderedJson) })
        SurfaceModel.benchCounters = null
        return linkedMapOf(
            "applyMs" to times.average(),
            "p95ApplyMs" to percentile(times, 0.95),
            "totalMs" to times.sum(),
            "nodes" to liveNodes(m),
            "ffiBytesTotal" to (counters.bytesIn + counters.bytesOut).toDouble(),
        )
    }

    private fun themeSwitch(s: JsonValue): Map<String, Double> {
        val m = model(s)
        firstPaint(m, s)
        val steps = s["steps"]!!
        val themes = steps["themes"]!!.array!!.map { it.string!! }
        val cycles = steps["cycles"]!!.number!!.toInt()
        val times = ArrayList<Double>()
        var restyled = 0
        repeat(cycles) {
            for (id in themes) {
                times.add(ms { m.setTheme(ThemeHandle.builtin(id)!!) })
                restyled = kotlin.math.max(restyled, m.stats.restyled)
            }
        }
        val modes = steps["modes"]!!.array!!.map { if (it.string == "dark") Mode.Dark else Mode.Light }
        val modeCycles = steps["modeCycles"]!!.number!!.toInt()
        val modeTimes = ArrayList<Double>()
        repeat(modeCycles) { for (mode in modes) modeTimes.add(ms { m.setMode(mode) }) }
        return linkedMapOf("themeSwitchMs" to median(times), "modeSwitchMs" to median(modeTimes), "restyled" to restyled.toDouble())
    }

    private fun template10k(s: JsonValue): Map<String, Double> {
        val m = model(s)
        val first = firstPaint(m, s)
        val rebind = s["steps"]!!["rebind"]!!
        val items = generateItems(rebind)
        val t = ms { m.setData(rebind["path"]!!.string!!, items) }
        return linkedMapOf("firstPaintMs" to first, "rebindMs" to t, "nodes" to liveNodes(m))
    }

    private fun overlay(s: JsonValue): Map<String, Double> {
        val m = model(s)
        firstPaint(m, s)
        val path = s["steps"]!!["path"]!!.string!!
        val toggles = s["steps"]!!["toggles"]!!.number!!.toInt()
        val opens = ArrayList<Double>()
        val closes = ArrayList<Double>()
        repeat(toggles) {
            opens.add(ms { m.setData(path, JsonValue.Bool(true)) })
            check(m.layers.isNotEmpty()) { "the dialog opened" }
            closes.add(ms { m.setData(path, JsonValue.Bool(false)) })
            check(m.layers.isEmpty()) { "the dialog closed" }
        }
        return linkedMapOf("openMs" to median(opens), "closeMs" to median(closes))
    }

    private fun table1k(s: JsonValue): Map<String, Double> {
        val m = model(s)
        val first = firstPaint(m, s)
        val (_, h) = viewport(s)
        val steps = s["steps"]!!["scroll"]!!["count"]!!.number!!.toInt()
        // The Table reports its windowed rows under its own id (or its scroller part's).
        val listId = m.lists.values.firstOrNull { it.windowed }?.id ?: m.lists.keys.firstOrNull() ?: "table"
        val t1 = System.nanoTime()
        for (i in 1..steps) m.scrollTo(listId, 0f, i * h)
        val step = (System.nanoTime() - t1) / 1e6 / steps
        val rows = m.lists[listId]?.let { (it.end - it.start).toDouble() } ?: m.nodes.count { !it.hidden && it.part == "row" }.toDouble()
        val sort = ms { m.apply(s["steps"]!!["sort"]!!.orderedJson) }
        return linkedMapOf("firstPaintMs" to first, "scrollStepMs" to step, "sortMs" to sort, "renderedRows" to rows)
    }
}

/** A fragment (`packages/exponential-ui-bench/src/results.ts`) as JSON. */
fun benchFragment(renderer: String, platform: String, device: String?, scenarios: Map<String, Map<String, Double>>): String {
    val fields = LinkedHashMap<String, JsonValue>()
    fields["renderer"] = JsonValue.Str(renderer)
    fields["platform"] = JsonValue.Str(platform)
    if (device != null) fields["device"] = JsonValue.Str(device)
    fields["date"] = JsonValue.Str(java.time.Instant.now().toString())
    fields["scenarios"] = JsonValue.Obj(scenarios.mapValues { (_, m) -> JsonValue.Obj(m.mapValues { (_, v) -> JsonValue.Num(kotlin.math.round(v * 1000) / 1000) }) })
    return JsonValue.Obj(fields).orderedJson
}
