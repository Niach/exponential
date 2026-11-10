package at.exponential.ui

import at.exponential.ui.ffi.placeOverlay
import at.exponential.ui.ffi.placeOverlayAligned
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.Insets
import at.exponential.ui.model.SurfaceEnvironment
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.dismissLayer
import at.exponential.ui.model.escape
import at.exponential.ui.model.press
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

    /** #VAPP-253: under the window's width a top or bottom sheet spans it; unset, the surface's. */
    @Test
    fun aSheetSpansTheLayerViewport() {
        for (side in listOf("bottom", "top")) {
            val tree = JsonValue.parse("""{"id":"root","component":"Box","children":[{"id":"d","component":"Drawer","props":{"open":true,"side":"$side"},"children":[{"id":"x","component":"Text","props":{"text":"Sheet"}}]}]}""")
            val m = SurfaceModel("sheet", SurfaceOptions(theme = null, mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(tree.json)
            m.setViewport(358f, 600f)
            assertEquals(358f, m.layers.single().frame.width, 0.001f)
            m.overlayLayerWidth(800f)
            assertEquals(side, m.layers.single().position)
            assertEquals(0f, m.layers.single().frame.left, 0.001f)
            assertEquals(800f, m.layers.single().frame.width, 0.001f)
            m.overlayLayerWidth(null)
            assertEquals(358f, m.layers.single().frame.width, 0.001f)
        }
    }

    /**
     * #VAPP-252: with the keyboard up a Dialog's cap is the window less its insets
     * (the safe area above the keyboard), not the surface below the app's header
     * less those insets; it fits there whole, and only a truly short window
     * scrolls its body. Closing gives back the height the surface has by then.
     */
    @Test
    fun aDialogIsCappedByTheWindowNotTheSurfacesViewport() {
        val fields = (0 until 8).joinToString(",") { """{"id":"f$it","component":"Box","props":{"style":{"height":50,"flexShrink":0}}}""" }
        val tree = JsonValue.parse("""{"id":"root","component":"Box","children":[{"id":"d","component":"Dialog","props":{"open":true,"title":"Log hours"},"children":[$fields]}]}""")
        val m = SurfaceModel("dialog", SurfaceOptions(theme = null, mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested(tree.json)
        // The phone: 914 dp, the surface starts below a 129 dp header, status bar 24, keyboard 336.
        val keyboard = SurfaceEnvironment(locale = "en-US", timeZone = "UTC", insets = Insets(top = 24f, bottom = 336f))
        m.setEnvironment(keyboard)
        m.setSurfaceWidth(412f)
        m.autoViewportHeight(785f)
        assertEquals("the surface's viewport: 785 - 24 - 336 - 16", 409f, m.layers.single().frame.height, 0.001f)

        m.overlayViewportHeight(914f)
        val whole = m.layers.single().frame.height
        assertTrue("the whole Dialog fits the safe area above the keyboard: $whole", whole > 409f && whole < 538f)
        val body = m.nodes.first { it.part == "body" && it.ownerComponent == "Dialog" }
        assertEquals("nothing hides in the body", m.scrolls[body.index]?.contentHeight ?: m.frame(body.index).height, m.frame(body.index).height, 0.5f)

        // A taller keyboard: the body shrinks, the rest stays reachable by scrolling.
        m.setEnvironment(keyboard.copy(insets = Insets(top = 24f, bottom = 674f)))
        assertEquals(914f - 24f - 674f - 16f, m.layers.single().frame.height, 0.001f)
        val scroll = m.scrolls[body.index]!!
        assertTrue(scroll.scrollY)
        assertTrue(scroll.contentHeight >= 8 * 50f)

        // The keyboard went while it was up: closing gives back the surface's height now.
        m.autoViewportHeight(900f)
        m.overlayViewportHeight(null)
        assertEquals(900f, m.viewportHeight, 0.001f)
    }

    /** #EXP-1765: overlay-geometry.json `inset` (the pill popup: the inset, the clamp, then whole dp from the pill), replayed at density 2 in px. */
    @Test
    fun insetOverlayPlacement() {
        val cases = Fixtures.json("overlay-geometry.json")["inset"]?.array ?: emptyList()
        assertTrue(cases.size >= 5)
        val d = 2f
        for (c in cases) {
            val px = { v: JsonValue?, k: String -> (v!![k]!!.number!! * d).toInt() }
            val a = c["anchor"]
            val anchor = androidx.compose.ui.unit.IntRect(px(a, "x"), px(a, "y"), px(a, "x") + px(a, "width"), px(a, "y") + px(a, "height"))
            val window = androidx.compose.ui.unit.IntSize(px(c["viewport"], "width"), px(c["viewport"], "height"))
            val p = at.exponential.ui.compose.selectPopupOrigin(c["align"]!!.string!!, false, px(c, "offset"), px(c, "padding"), px(c, "inset"), d, anchor, window, px(c["size"], "width"), px(c["size"], "height"))
            val name = c["name"]?.string ?: ""
            assertEquals(name, px(c["expected"], "x"), p.x)
            assertEquals(name, px(c["expected"], "y"), p.y)
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
            // VAPP-276: `align` along the side (centre without one).
            val p = placeOverlayAligned(
                a["x"]!!.number!!, a["y"]!!.number!!, a["width"]!!.number!!, a["height"]!!.number!!,
                s["width"]!!.number!!, s["height"]!!.number!!, v["width"]!!.number!!, v["height"]!!.number!!,
                c["side"]!!.string!!, c["align"]?.string ?: "center",
            )
            if (c["align"] == null) {
                val q = placeOverlay(
                    a["x"]!!.number!!, a["y"]!!.number!!, a["width"]!!.number!!, a["height"]!!.number!!,
                    s["width"]!!.number!!, s["height"]!!.number!!, v["width"]!!.number!!, v["height"]!!.number!!,
                    c["side"]!!.string!!,
                )
                assertEquals(p, q)
            }
            val e = c["expected"]!!
            val name = c["name"]?.string ?: ""
            assertEquals(name, e["x"]!!.number!!, p.x, 0.0)
            assertEquals(name, e["y"]!!.number!!, p.y, 0.0)
            assertEquals(name, e["side"]!!.string!!, p.side)
            e["flipped"]?.bool?.let { assertEquals(name, it, p.flipped) }
        }
    }

    /** VAPP-276: a Popover / Menu `anchor`ed to a panel button elsewhere in
     *  the tree: the core makes the button its trigger and places the layer
     *  against its frame; a press opens it, Escape closes it. */
    @Test
    fun anAnchorByIdIsTheTriggerAndPlacesTheLayer() {
        val m = makeModel("anchor", theme = null, fixed = true, width = 800f)
        m.setViewport(800f, 600f)
        m.setNested(
            """{"id": "root", "component": "Box", "style": {"position": "relative", "width": 800, "height": 600}, "children": [
              {"id": "panel", "component": "Box", "style": {"position": "absolute", "left": 0, "top": 556, "width": 800, "height": 44, "display": "flex", "flexDirection": "row", "justifyContent": "space-between"}, "children": [
                {"id": "start", "component": "Button", "props": {"label": "Start"}},
                {"id": "clock", "component": "Button", "props": {"label": "12:00"}}]},
              {"id": "startMenu", "component": "Menu", "props": {"anchor": "start", "side": "top", "align": "start", "items": [{"label": "Files", "value": "files"}]}},
              {"id": "calendar", "component": "Popover", "props": {"anchor": "clock", "side": "top", "align": "end"}, "children": [{"id": "month", "component": "Text", "props": {"text": "November"}}]}]}""",
        )
        assertTrue("no default Menu trigger", m.nodes.none { it.id == "startMenu.trigger" && !it.removed })
        val clock = m.nodes.first { it.id == "clock" && !it.removed }
        assertEquals("calendar", clock.triggerFor)
        m.press("clock")
        val layer = m.layers.first { it.owner == "calendar" }
        assertEquals("top", layer.placementSide)
        assertEquals(m.frame(clock.index), layer.anchorFrame)
        assertEquals("end-aligned (shifted 8 px inside the surface)", minOf(m.frame(clock.index).right, 792f), layer.frame.right, 0.01f)
        assertTrue("above the clock", layer.frame.bottom <= m.frame(clock.index).top)
        assertTrue(m.escape())
        assertTrue(m.layers.none { it.owner == "calendar" })
        m.press("start")
        val menu = m.layers.first { it.owner == "startMenu" }
        assertEquals("top", menu.placementSide)
        assertEquals(m.frame(m.nodes.first { it.id == "start" && !it.removed }.index), menu.anchorFrame)
        m.press("start")
        assertTrue("the anchor toggles it closed", m.layers.none { it.owner == "startMenu" })
    }

    @Test
    fun openLayersCarryFrames() {
        val m = makeModel("ov", fixed = true)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        val dialogs = m.nodes.filter { it.component == "Dialog" }
        assumeTrue("no dialog in the kitchen sink", dialogs.isNotEmpty())
        val owner = dialogs[0]
        m.setOpen(owner.id, true)
        // The kitchen sink's Toast is open too (the toast class stacks above every overlay).
        assertEquals(listOf(owner.id), m.layers.filter { !it.isToast }.map { it.owner })
        val layer = m.layers.first { it.owner == owner.id }
        assertTrue(layer.isModal)
        assertEquals("Dialog", layer.kind)
        assertTrue(layer.frame.width > 100f)
        assertEquals("centered", layer.position)
        // The layer's nodes have frames and the content root is in layer 1.
        assertEquals(1, m.node(layer.root)?.layer)
        assertEquals(layer.frame, m.frame(layer.root))
        m.dismissLayer(owner.id)
        assertTrue(m.layers.none { it.owner == owner.id })
    }
}
