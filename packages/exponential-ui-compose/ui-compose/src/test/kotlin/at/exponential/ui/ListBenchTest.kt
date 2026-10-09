package at.exponential.ui

import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.scrollTo
import at.exponential.ui.model.scrollToIndex
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * The shared list bench (`fixtures/bench-list.json`, round 2 §5) on the
 * JVM: 100,000 `ListRow`s on a 390 × 800 surface (neutral), the painter's
 * model with the REAL Compose text measure (Robolectric, density 2.75).
 * `firstPaintMs` = reduce + bind + measure + lay out the first window;
 * `scrollStepMs` = the mean of 100 one-viewport steps; `scrollToIndexMs` =
 * `scrollToIndex(50000, start)`; `renderedItems` = items alive after the
 * jump. Prints the numbers (README "Numbers") and writes
 * `build/bench-list.json`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ListBenchTest {
    @Before
    fun setUp() = Fixtures.require()

    @Test
    fun hundredThousandRows() {
        val f = Fixtures.json("bench-list.json")
        val rows = f["rows"]!!
        val count = rows["count"]!!.number!!.toInt()
        val data = JsonValue.Arr(
            List(count) { i ->
                JsonValue.Obj(mapOf("id" to JsonValue.Str("r$i"), "title" to JsonValue.Str("Row $i"), "meta" to JsonValue.Str((i % 97).toString())))
            },
        )
        val shaper = robolectricShaper(density = 2.75f)
        val vw = f["viewport"]!!["width"]!!.number!!.toFloat()
        val vh = f["viewport"]!!["height"]!!.number!!.toFloat()
        fun run(): Map<String, Double> {
            val m = SurfaceModel("bench", SurfaceOptions(catalogId = f["catalogId"]!!.string!!, theme = ThemeHandle.builtin(f["theme"]!!.string!!), mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
            val t0 = System.nanoTime()
            m.setComponents(f["components"]!!.orderedJson)
            m.setData("/rows", data)
            m.setViewport(vw, vh)
            val first = (System.nanoTime() - t0) / 1e6
            val steps = 100
            val t1 = System.nanoTime()
            for (i in 1..steps) m.scrollTo("root", 0f, i * vh)
            val step = (System.nanoTime() - t1) / 1e6 / steps
            val t2 = System.nanoTime()
            m.scrollToIndex("root", 50000, "start")
            val jump = (System.nanoTime() - t2) / 1e6
            val list = m.indexOf("root")
            val items = m.nodes.count { !it.hidden && it.parent == list && it.id.startsWith("bench-row") }
            check(m.nodes.any { !it.hidden && it.props["title"]?.string == "Row 50000" || it.props["text"]?.string == "Row 50000" }) { "row 50000 rendered after the jump" }
            return mapOf("firstPaintMs" to first, "scrollStepMs" to step, "scrollToIndexMs" to jump, "renderedItems" to items.toDouble())
        }
        run() // warm the JIT and the font caches
        val r = run()
        val line = r.entries.joinToString(", ") { (k, v) -> "$k ${"%.2f".format(java.util.Locale.ROOT, v)}" }
        println("bench-list (JVM, Robolectric native text, ${System.getProperty("os.name")} ${System.getProperty("os.arch")}, ${Runtime.getRuntime().availableProcessors()} cpus): $line")
        File("build").mkdirs()
        File("build/bench-list.json").writeText(JsonValue.Obj(r.mapValues { JsonValue.Num(it.value) }).json + "\n")
        // The window (800 / the row) + 2 × overscan (5).
        assertTrue("renderedItems ${r["renderedItems"]}", r["renderedItems"]!! in 10.0..40.0)
    }
}
