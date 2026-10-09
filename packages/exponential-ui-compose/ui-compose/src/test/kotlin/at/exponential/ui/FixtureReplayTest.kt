package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.basicCatalogId
import at.exponential.ui.ffi.builtinThemeIds
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.reduceNestedJson
import at.exponential.ui.ffi.reduceSurfaceJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
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
        assertTrue(m.indexOf("hdr-title")!! < m.indexOf("hdr-scan")!!)
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
}
