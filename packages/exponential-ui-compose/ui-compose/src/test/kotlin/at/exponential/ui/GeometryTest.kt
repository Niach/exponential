package at.exponential.ui

import at.exponential.ui.ffi.placeOverlay
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.dismissLayer
import at.exponential.ui.model.setOpen
import at.exponential.ui.theme.Mode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test

/**
 * The EXACT taffy frames of `layout-geometry.json` (900/390, LTR/RTL)
 * reach the model unchanged, and the overlay placements match.
 */
class GeometryTest {
    @Before
    fun setUp() = Fixtures.require()

    @Test
    fun layoutGeometryFixture() {
        val fixture = Fixtures.json("layout-geometry.json")
        val cases = fixture["cases"]?.obj ?: emptyMap()
        assertEquals(4, cases.size)
        for ((name, c) in cases) {
            val tree = fixture["surface"]!!.obj!!.toMutableMap()
            val style = (tree["style"]?.obj ?: emptyMap()).toMutableMap()
            style["direction"] = c["direction"]!!
            tree["style"] = JsonValue.Obj(style)
            val m = SurfaceModel("geometry", SurfaceOptions(theme = null, mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(JsonValue.Obj(tree).json)
            // The fixture's measure overrides apply through the facade.
            val width = c["width"]!!.number!!.toFloat()
            m.surface.setViewport(width, 0f, null)
            val out = m.surface.layoutFixed(fixture["measures"]!!.json, false)
            m.setViewport(width, 0f)
            val expected = c["frames"]!!.array!!
            assertEquals(name, expected.size, out.frames.size)
            for ((i, want) in expected.withIndex()) {
                if (i >= out.frames.size) break
                val f = out.frames[i]
                val id = want["id"]!!.string!!
                assertEquals("$name: order at $i", id, m.nodes[f.index.toInt()].id)
                assertEquals("$name $id.x", want["x"]!!.number!!, f.x.toDouble(), 0.001)
                assertEquals("$name $id.y", want["y"]!!.number!!, f.y.toDouble(), 0.001)
                assertEquals("$name $id.w", want["w"]!!.number!!, f.w.toDouble(), 0.001)
                assertEquals("$name $id.h", want["h"]!!.number!!, f.h.toDouble(), 0.001)
            }
        }
    }

    @Test
    fun overlayPlacement() {
        val cases = Fixtures.json("overlay-geometry.json")["cases"]?.array ?: emptyList()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val a = c["anchor"]!!
            val s = c["size"]!!
            val v = c["viewport"]!!
            val p = placeOverlay(
                a["x"]!!.number!!, a["y"]!!.number!!, a["width"]!!.number!!, a["height"]!!.number!!,
                s["width"]!!.number!!, s["height"]!!.number!!, v["width"]!!.number!!, v["height"]!!.number!!,
                c["side"]!!.string!!,
            )
            val e = c["expected"]!!
            val name = c["name"]?.string ?: ""
            assertEquals(name, e["x"]!!.number!!, p.x, 0.0)
            assertEquals(name, e["y"]!!.number!!, p.y, 0.0)
            assertEquals(name, e["side"]!!.string!!, p.side)
            e["flipped"]?.bool?.let { assertEquals(name, it, p.flipped) }
        }
    }

    @Test
    fun openLayersCarryFrames() {
        val m = makeModel("ov", fixed = true)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        val dialogs = m.nodes.filter { it.component == "Dialog" }
        assumeTrue("no dialog in the kitchen sink", dialogs.isNotEmpty())
        val owner = dialogs[0]
        m.setOpen(owner.id, true)
        assertEquals(listOf(owner.id), m.layers.map { it.owner })
        val layer = m.layers[0]
        assertEquals("Dialog", layer.kind)
        assertTrue(layer.frame.width > 100f)
        assertEquals("centered", layer.position)
        // The layer's nodes have frames and the content root is in layer 1.
        assertEquals(1, m.node(layer.root)?.layer)
        assertEquals(layer.frame, m.frame(layer.root))
        m.dismissLayer(owner.id)
        assertTrue(m.layers.isEmpty())
    }
}
