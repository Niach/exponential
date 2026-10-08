package at.exponential.ui

import androidx.compose.ui.graphics.Color
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.json.JsonValue
import at.exponential.ui.theme.PaintStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import kotlin.math.floor

/**
 * One snapshot per catalog component: the painted tree (id, component,
 * part, frame, box visual, states) of every `catalog-components.json` case
 * under the exponential theme (dark) at 390 dp with the core's FIXED
 * measure, so the file is byte-stable across machines.
 * `src/test/snapshots/components.json` is the lock;
 * `EXPONENTIAL_UI_RECORD=1 ./gradlew :ui-compose:testDebugUnitTest`
 * rewrites it (a missing file is recorded). The same tree is what the
 * SwiftUI painter locks, so it must equal the Swift snapshot too.
 */
class SnapshotTest {
    companion object {
        val path = File("src/test/snapshots/components.json")
        val swiftPath = File("../../exponential-ui-swift/Tests/ExponentialUITests/__Snapshots__/components.json")

        private fun r2(v: Float): JsonValue {
            val x = v.toDouble() * 100
            val r = if (x >= 0) floor(x + 0.5) else -floor(-x + 0.5)
            return JsonValue.Num(r / 100)
        }

        /** `#rrggbb` (+ `aa` below full alpha), the core's colour as given. */
        fun hex(c: Color): String {
            fun b(f: Float) = "%02x".format(floor(f * 255.0 + 0.5).toInt())
            val h = "#" + b(c.red) + b(c.green) + b(c.blue)
            return if (c.alpha < 0.999f) h + b(c.alpha) else h
        }

        val PaintStyle.backgroundHex: String? get() = background?.let(::hex)

        /** The snapshot text of every component case. */
        fun render(): String {
            val cases = Fixtures.json("catalog-components.json")["cases"]?.array ?: emptyList()
            val snapshot = LinkedHashMap<String, JsonValue>()
            for (c in cases) {
                val name = c["name"]?.string ?: "?"
                val m = makeModel("snap", fixed = true)
                m.setNested(c["node"]!!.json)
                val rows = ArrayList<JsonValue>()
                for (n in m.nodes) {
                    val f = m.frame(n.index)
                    val s = m.boxStyle(n.index)
                    val row = LinkedHashMap<String, JsonValue>()
                    row["id"] = JsonValue.Str(n.id)
                    row["component"] = JsonValue.Str(n.component)
                    row["frame"] = JsonValue.Arr(listOf(f.left, f.top, f.width, f.height).map(::r2))
                    n.part?.let { row["part"] = JsonValue.Str(it) }
                    if (n.hidden) row["hidden"] = JsonValue.Bool(true)
                    m.style(n.index).backgroundHex?.let { row["bg"] = JsonValue.Str(it) }
                    if (s.borderWidth > 0f) row["border"] = JsonValue.Num(s.borderWidth.toDouble())
                    if (s.radius > 0f) row["radius"] = JsonValue.Num(s.radius.toDouble())
                    s.opacity?.let { row["opacity"] = JsonValue.Num(it.toDouble()) }
                    if (n.partStates.isNotEmpty()) row["states"] = JsonValue.Arr(n.partStates.map { JsonValue.Str(it) })
                    rows.add(JsonValue.Obj(row))
                }
                snapshot[name] = JsonValue.Obj(mapOf("height" to JsonValue.Num(m.surfaceSize.height.toDouble()), "nodes" to JsonValue.Arr(rows)))
            }
            return prettyJson(JsonValue.Obj(snapshot)) + "\n"
        }
    }

    @Before
    fun setUp() = Fixtures.require()

    @Test
    fun componentSnapshots() {
        val text = render()
        if (recording || !path.exists()) {
            path.parentFile.mkdirs()
            path.writeText(text)
            if (recording) return
        }
        val stored = path.readText()
        if (stored != text) {
            val tmp = File(System.getProperty("java.io.tmpdir"), "components.compose.snapshot.json")
            tmp.writeText(text)
            fail("component snapshot drifted; compare ${tmp.absolutePath} with ${path.absolutePath} (EXPONENTIAL_UI_RECORD=1 ./gradlew :ui-compose:testDebugUnitTest rewrites it)")
        }
    }

    /** The Compose painter's model paints the SAME tree as the SwiftUI one (same core, same fixed measure). */
    @Test
    fun matchesTheSwiftSnapshot() {
        assumeTrue("no Swift snapshot in this checkout", swiftPath.exists())
        val ours = JsonValue.parse(render()).obj!!
        val swift = JsonValue.parse(swiftPath.readText()).obj!!
        assertEquals(swift.keys.sorted(), ours.keys.sorted())
        val drift = swift.keys.sorted().filter { !jsonEqual(ours.getValue(it).json, swift.getValue(it).json) }
        if (drift.isNotEmpty()) {
            val first = drift.first()
            fail("${drift.size} cases differ from the Swift snapshot, first $first: ${jsonDiff(ours.getValue(first).json, swift.getValue(first).json)}\nall: $drift")
        }
    }
}
