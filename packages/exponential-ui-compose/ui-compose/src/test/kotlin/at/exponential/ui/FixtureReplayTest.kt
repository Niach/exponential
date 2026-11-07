package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import at.exponential.ui.compose.CanvasSource
import at.exponential.ui.compose.MotionList
import at.exponential.ui.compose.MotionPainter
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.CanvasPlayer
import at.exponential.ui.ffi.UiException
import at.exponential.ui.ffi.basicCatalogId
import at.exponential.ui.ffi.canvasToDrawing
import at.exponential.ui.ffi.builtinThemeIds
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.reduceNestedJson
import at.exponential.ui.ffi.reduceSurfaceJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.checked
import at.exponential.ui.model.mixed
import at.exponential.ui.model.press
import at.exponential.ui.theme.Mode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Every shared fixture renders through the painter's model: the catalog
 * components, the macros, the basic-catalog map, the extension cases and
 * the kitchen sink, in every built-in theme and both modes. Text is not
 * under test here, so the model uses the core's fixed measure; the
 * extension fixture needs the painter's measurer (a real Compose shaper),
 * hence Robolectric.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class FixtureReplayTest {
    @Before
    fun setUp() = Fixtures.require()

    @Test
    fun kitchenSinkLaysOutEveryNode() {
        val m = makeModel("ks", fixed = true)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        assertEquals(m.issues.toString(), 0, m.issues.size)
        assertTrue("nodes ${m.nodes.size}", m.nodes.size > 150)
        assertEquals(m.nodes.size, m.frames.size)
        assertTrue("height ${m.surfaceSize.height}", m.surfaceSize.height > 1000f)
        assertTrue("upcalls ${m.stats.upcalls}", m.stats.upcalls <= 3)
        // Every visible main-tree node has a frame inside the surface (a
        // scroll container's content overflows it unscrolled; it clips).
        fun inScroller(i: Int): Boolean {
            var p = m.node(i)?.parent
            while (p != null) {
                if (m.style(p).overflowScroll) return true
                p = m.node(p)?.parent
            }
            return false
        }
        for (n in m.nodes) {
            if (n.hidden || n.layer != 0 || inScroller(n.index)) continue
            val f = m.frame(n.index)
            assertTrue(n.id, f.left >= -0.01f)
            assertTrue(n.id, f.right <= m.surfaceSize.width + 0.01f)
        }
        // The header reads first (pre-order = accessibility order).
        assertTrue(m.indexOf("hdr_title")!! < m.indexOf("hdr_scan")!!)
        // A warm pass makes no measure calls.
        m.layoutNeeded = true
        m.pass()
        assertEquals(0, m.stats.upcalls)
        assertEquals(0, m.stats.measureCalls)
    }

    @Test
    fun kitchenSinkUnderEveryThemeAndMode() {
        val themes = builtinThemeIds()
        assertTrue(themes.toString(), themes.size >= 3)
        for (theme in themes) {
            for (mode in Mode.entries) {
                val m = makeModel("ks-$theme-$mode", theme = theme, mode = mode, width = 900f, fixed = true)
                m.setNested(Fixtures.text("kitchen-sink.json"))
                assertEquals("$theme/$mode ${m.issues}", 0, m.issues.size)
                assertTrue("$theme/$mode", m.surfaceSize.height > 500f)
                assertEquals(m.nodes.size, m.styles.size)
                // The theme's background reaches the root box.
                assertNotNull("$theme/$mode root background", m.style(0).background)
            }
        }
    }

    @Test
    fun catalogComponentsRender() {
        val cases = Fixtures.json("catalog-components.json")["cases"]?.array ?: emptyList()
        assertTrue(cases.size > 100)
        val failures = ArrayList<String>()
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val m = makeModel("cc", fixed = true)
            try {
                val out = m.setNested(c["node"]!!.json)
                if (out.issuesJson != "[]") failures.add("$name: ${out.issuesJson}")
            } catch (e: Exception) {
                failures.add("$name: $e")
                continue
            }
            if (m.nodes.isEmpty()) failures.add("$name: no nodes")
            if (m.frames.size != m.nodes.size) failures.add("$name: frames")
            // Every node resolves a box style and an ink; leaves a text style.
            for (n in m.nodes) {
                m.boxStyle(n.index)
                m.ink(n.index)
                if (n.isLeaf) m.textStyle(n.index)
            }
        }
        assertEquals(emptyList<String>(), failures)
    }

    @Test
    fun macrosAndBasicMapReduceLikeTheReference() {
        val macros = Fixtures.json("catalog-macros.json")["cases"]?.array ?: emptyList()
        assertTrue(macros.isNotEmpty())
        for (c in macros) {
            val got = reduceNestedJson(c["input"]!!.json, coreCatalogId(), null)
            val expected = JsonValue.Obj(mapOf("root" to c["expected"]!!, "issues" to JsonValue.Arr(emptyList()))).json
            assertTrue("macro ${c["name"]?.string}: ${jsonDiff(got, expected)}", jsonEqual(got, expected))
        }
        val basic = Fixtures.json("catalog-basic-map.json")["cases"]?.array ?: emptyList()
        assertTrue(basic.isNotEmpty())
        for (c in basic) {
            // A basic-catalog surface: the model accepts the flat list; the
            // mapping is compared through the free reducer.
            val m = makeModel("basic", fixed = true)
            m.setComponents(c["components"]!!.json)
            val got = reduceSurfaceJson(c["components"]!!.json, basicCatalogId(), null)
            val expected = c["expected"]!!.json
            assertTrue("basic ${c["name"]?.string}: ${jsonDiff(got, expected)}", jsonEqual(got, expected))
        }
    }

    private class Probe : ExtensionPainter {
        var measured = 0

        override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size? {
            measured += 1
            return Size(120f, 40f)
        }

        @Composable
        override fun Paint(context: ExtensionContext) {
            Box(Modifier.fillMaxSize().background(Color.Red))
        }
    }

    @Test
    fun extensionFixtureRendersThroughAPainter() {
        val fixture = Fixtures.json("catalog-extension.json")
        val definition = fixture["extension"]!!.json
        val probe = Probe()
        val registry = ExtensionRegistry.shared
        registry.reset()
        try {
            ExponentialUi.register(definition, mapOf("TrendLine" to probe, "StatCard" to probe))
            for (c in fixture["cases"]?.array ?: emptyList()) {
                val name = c["name"]?.string ?: ""
                val options = SurfaceOptions(catalogId = c["catalogId"]?.string ?: coreCatalogId())
                var shaper: at.exponential.ui.measure.TextShaper? = null
                val m = SurfaceModel("ext", options, RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) {
                    shaper ?: robolectricShaper().also { shaper = it }
                }
                m.setViewport(390f, 600f)
                val out = m.setComponents(c["components"]!!.json)
                val expectedIssues = c["expected"]?.get("issues")?.array?.size ?: 0
                assertEquals("$name: ${out.issuesJson}", expectedIssues, JsonValue.parse(out.issuesJson).array?.size ?: 0)
                val leaves = m.nodes.filter { it.component == "Extension" }
                if (expectedIssues == 0 && c["expected"]!!.json.contains("\"Extension\"")) {
                    assertFalse("$name: extension leaves", leaves.isEmpty())
                }
                for (l in leaves) {
                    assertEquals("${l.id} measured by the painter", 40f, m.frame(l.index).height, 0.001f)
                }
            }
        } finally {
            registry.reset()
        }
        assertTrue(probe.measured > 0)
    }

    /**
     * Round 8: `fixtures/canvas.json` over uniffi. `compile`: the slot ids,
     * the packed VERSION 2 frame (with the case's host slots) and its decode
     * (one op per list op, the `text` ops after the final `pop` with their
     * string, font, weight, align and baseline); `refuse`: the issue codes
     * and paths in order; `fit`: [MotionPainter.fit]; `upright`:
     * [MotionPainter.textAnchor] (translation + √|det| only); `pointer`:
     * `canvasToDrawing` within 0.01.
     */
    @Test
    fun canvasFixtureReplays() {
        val f = Fixtures.json("canvas.json")
        val compile = f["compile"]!!.array!!
        assertTrue(compile.size >= 10)
        for (c in compile) {
            val id = c["id"]!!.string!!
            val player = CanvasPlayer(c["canvas"]!!.json)
            assertEquals("$id slotIds", c["slotIds"]!!.array!!.map { it.string }, player.slotIds())
            val size = c["canvas"]!!["size"]!!.array!!.map { it.number!!.toFloat() }
            assertEquals("$id size", size, player.size())
            val packed = player.sample(c["slots"]?.json ?: "")
            val want = c["packed"]!!.array!!.map { it.number!!.toFloat() }
            assertEquals("$id packed length", want.size, packed.size)
            for (i in want.indices) assertEquals("$id packed[$i]", want[i], packed[i], 1e-4f)
            assertEquals("$id version", 2f, packed[0])
            val list = MotionList.decode(packed)
            assertNotNull(id, list)
            val ops = c["list"]!!["ops"]!!.array!!
            assertEquals("$id ops", ops.size, list!!.ops.size)
            ops.forEachIndexed { i, o ->
                val got = list.ops[i]
                if (o["op"]?.string != "text") {
                    assertFalse("$id op $i", got is MotionList.Op.Text)
                    return@forEachIndexed
                }
                assertTrue("$id op $i is text", got is MotionList.Op.Text)
                val t = got as MotionList.Op.Text
                assertEquals("$id text", o["text"]!!.string, t.text)
                assertEquals("$id font", o["font"]!!.string == "mono", t.mono)
                assertEquals("$id weight", mapOf("regular" to 400, "medium" to 500, "semibold" to 600, "bold" to 700)[o["weight"]!!.string], t.weight)
                assertEquals("$id size", o["size"]!!.number!!.toFloat(), t.size, 1e-4f)
                assertEquals("$id align", listOf("start", "center", "end").indexOf(o["align"]!!.string), t.align)
                assertEquals("$id baseline", listOf("top", "middle", "alphabetic").indexOf(o["baseline"]!!.string), t.baseline)
                o["color"]!!.array!!.forEachIndexed { k, v -> assertEquals("$id color", v.number!!.toFloat(), t.color[k], 1e-4f) }
                o["transform"]!!.array!!.forEachIndexed { k, v -> assertEquals("$id transform", v.number!!.toFloat(), t.t[k], 1e-4f) }
            }
            // THE TEXT RULE: every text op after every other op.
            val firstText = list.ops.indexOfFirst { it is MotionList.Op.Text }
            if (firstText >= 0) assertTrue("$id text last", list.ops.drop(firstText).all { it is MotionList.Op.Text })
        }
        for (c in f["refuse"]!!.array!!) {
            val id = c["id"]!!.string!!
            try {
                CanvasPlayer(c["canvas"]!!.json)
                fail("$id: compiled")
            } catch (e: UiException.Invalid) {
                val got = JsonValue.parse(e.reason).array!!
                val want = c["issues"]!!.array!!
                assertEquals("$id codes", want.map { it["code"]?.string }, got.map { it["code"]?.string })
                assertEquals("$id paths", want.map { it["path"]?.string }, got.map { it["path"]?.string })
            }
            assertEquals("$id: the painter paints nothing", null, CanvasSource.compile(c["canvas"]!!.json))
        }
        for (c in f["fit"]!!.array!!) {
            val s = c["size"]!!.array!!.map { it.number!!.toFloat() }
            val b = c["box"]!!.array!!.map { it.number!!.toFloat() }
            val fit = c["fit"]!!.string!!
            val m = MotionPainter.fit(s[0], s[1], b[0], b[1], fit)
            val want = c["matrix"]!!.array!!.map { it.number!!.toFloat() }
            val got = listOf(m[0], 0f, 0f, m[1], m[2], m[3])
            for (k in 0 until 6) assertEquals("fit $fit $s in $b [$k]", want[k], got[k], 1e-4f)
        }
        // `upright`: text takes the translation and √|det| of fit × transform only.
        val upright = f["upright"]!!.array!!
        assertTrue(upright.isNotEmpty())
        for (c in upright) {
            val id = c["id"]!!.string!!
            val list = MotionList.decode(CanvasPlayer(c["canvas"]!!.json).sample(""))!!
            val b = c["box"]!!.array!!.map { it.number!!.toFloat() }
            val fit = MotionPainter.fit(list.width, list.height, b[0], b[1], c["fit"]!!.string!!)
            val got = list.ops.filterIsInstance<MotionList.Op.Text>()
            val want = c["texts"]!!.array!!
            assertEquals("$id texts", want.size, got.size)
            for ((t, w) in got.zip(want)) {
                val at = MotionPainter.textAnchor(fit, t.t, t.size)
                val anchor = w["anchor"]!!.array!!.map { it.number!!.toFloat() }
                assertEquals("$id text", w["text"]!!.string, t.text)
                assertEquals("$id ${t.text} x", anchor[0], at[0], 1e-3f)
                assertEquals("$id ${t.text} y", anchor[1], at[1], 1e-3f)
                assertEquals("$id ${t.text} size", w["size"]!!.number!!.toFloat(), at[2], 1e-3f)
            }
        }
        for (c in f["pointer"]!!.array!!) {
            val s = c["size"]!!.array!!.map { it.number!! }
            val b = c["box"]!!.array!!.map { it.number!! }
            val fit = c["fit"]!!.string!!
            for (p in c["points"]!!.array!!) {
                val at = p["at"]!!.array!!.map { it.number!! }
                val want = p["drawing"]!!.array!!.map { it.number!! }
                val got = canvasToDrawing(at[0], at[1], s[0], s[1], b[0], b[1], fit)
                assertEquals("pointer $fit $at size", 2, got.size)
                assertEquals("pointer $fit $at x", want[0], got[0], 0.01)
                assertEquals("pointer $fit $at y", want[1], got[1], 0.01)
            }
        }
    }

    /**
     * Round 8: `fixtures/interactions-round1.json` `checkbox`: a press on
     * the box through the painter's [press] (its mirror), then the
     * `change` payload, /checked, /mixed and the accessible state (mixed
     * wins over checked: `ToggleableState.Indeterminate`).
     */
    @Test
    fun checkboxIndeterminateCasesReplay() {
        val fx = Fixtures.json("interactions-round1.json")["checkbox"]!!
        val cases = fx["cases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val unbound = c["unbound"]
            var node = fx["surface"]!!.obj!!
            if (unbound != null) node = node + ("props" to JsonValue.Obj(node["props"]!!.obj!! + unbound.obj!!))
            val host = RecordingHost()
            val m = makeModel("cb", fixed = true, host = host)
            m.setNested(JsonValue.Obj(mapOf("id" to JsonValue.Str("root"), "component" to JsonValue.Str("Box"), "children" to JsonValue.Arr(listOf(JsonValue.Obj(node))))).json)
            c["data"]?.obj?.forEach { (k, v) -> m.setData("/$k", v) }
            val box = m.nodes.first { it.ownerComponent == "Checkbox" && it.part == "box" }.id
            fun state(): String {
                val i = m.indexOf(box)!!
                return when {
                    m.mixed(i) -> "mixed"
                    m.checked(i) -> "true"
                    else -> "false"
                }
            }
            assertEquals("$name: before", c["before"]!!.string, state())
            val steps = c["steps"]!!.array!!
            assertEquals(name, c["presses"]!!.number!!.toInt(), steps.size)
            for ((n, s) in steps.withIndex()) {
                host.actions.clear()
                m.press(m.indexOf(box)!!)
                val changes = host.actions.filter { it.name == "change" }
                assertEquals("$name step $n: one change", 1, changes.size)
                val payload = changes[0].payload ?: changes[0].context
                assertEquals("$name step $n: change", s["change"], JsonValue.Obj(mapOf("checked" to payload["checked"]!!, "indeterminate" to payload["indeterminate"]!!)))
                if (unbound == null) {
                    assertEquals("$name step $n: /checked", s["checked"], m.data["checked"] ?: JsonValue.Null)
                    assertEquals("$name step $n: /mixed", s["mixed"] ?: JsonValue.Null, m.data["mixed"] ?: JsonValue.Null)
                }
                assertEquals("$name step $n: state", s["state"]!!.string, state())
            }
        }
    }
}
