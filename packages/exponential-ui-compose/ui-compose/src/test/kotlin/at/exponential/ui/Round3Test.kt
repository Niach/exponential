package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.click
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.TreeGuideOp
import at.exponential.ui.compose.treeGuideGeometry
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.closeSubmenu
import at.exponential.ui.model.isSubmenuLayer
import at.exponential.ui.model.press
import at.exponential.ui.model.segmentedValues
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * Round 3 (VAPP-102) on the Compose painter: the `bar` Segmented (the old
 * TabBar), the ONE Menu (a `contextmenu` long press, checkbox rows, a
 * submenu as the core's second layer, native and painted) and the tree
 * guides' geometry (14 / 3 / 1).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class Round3Test {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    /** The painter's own measurer (the real Compose shaper at the window's density). */
    private fun render(json: String, host: RecordingHost = RecordingHost(), overlays: OverlayPresentation = OverlayPresentation.Native): SurfaceModel {
        Fixtures.require()
        val options = SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light, overlays = overlays)
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("round3", options, host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    private fun tap(m: SurfaceModel, id: String) {
        val f = m.frame(m.indexOf(id)!!)
        val d = density
        compose.onNodeWithTag("shot").performTouchInput { click(Offset(f.center.x * d, f.center.y * d)) }
        compose.waitForIdle()
    }

    // -- Segmented `bar` -----------------------------------------------

    @Test
    fun aBarSegmentedFillsTheRowAndNavigates() {
        val host = RecordingHost()
        val m = render(
            """{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column"},"children":[{"id":"bar","component":"Segmented","props":{"variant":"bar","value":"inbox","items":[
                 {"label":"Inbox","value":"inbox","icon":"nav-inbox"},{"label":"Issues","value":"issues","icon":"nav-issues"},{"label":"Agent","value":"agent"}]},
               "on":{"change":{"event":{"name":"nav"}}}}]}""",
            host,
        )
        val bar = m.indexOf("bar")!!
        // Full width (a bar always fills), `$control.tabBar` tall.
        assertEquals(m.width, m.frame(bar).width, 0.5f)
        assertEquals(m.control("tabBar", 56f), m.frame(bar).height, 0.5f)
        // Navigation semantics: every item a tab, the current page selected.
        val tab = SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Tab)
        compose.onNode(hasContentDescription("Inbox")).assert(tab).assertIsSelected()
        compose.onNode(hasContentDescription("Issues")).assert(tab).assertIsNotSelected()
        compose.onNode(hasContentDescription("Issues")).performClick()
        compose.waitForIdle()
        assertEquals(listOf("issues"), m.segmentedValues(bar))
        assertEquals(listOf("nav"), host.actions.map { it.name })
        compose.onNode(hasContentDescription("Issues")).assertIsSelected()
        compose.onNode(hasContentDescription("Inbox")).assertIsNotSelected()
    }

    // -- Menu ----------------------------------------------------------

    private val menuJson = """{"id":"root","component":"Box","children":[{"id":"ctx","component":"Menu","props":{"openOn":"contextmenu","items":[
          {"label":"Copy","value":"copy","shortcut":"C"},
          {"kind":"checkbox","label":"Pin","value":"pin","checked":{"path":"/pinned"}},
          {"kind":"separator"},
          {"kind":"submenu","label":"Move","items":[{"label":"Backlog","value":"backlog"},{"label":"Done","value":"done"}]}]},
        "on":{"select":{"event":{"name":"select"}}},
        "children":[{"id":"target","component":"Text","props":{"text":"Long-press me"}}]}]}"""

    private fun openByLongPress(m: SurfaceModel) {
        val f = m.frame(m.indexOf("target")!!)
        val d = density
        compose.onNodeWithTag("shot").performTouchInput { longClick(Offset(f.center.x * d, f.center.y * d)) }
        compose.waitForIdle()
    }

    @Test
    fun aContextMenuOpensOnALongPressWithCheckboxAndSubmenuRows() {
        val host = RecordingHost()
        val m = render(menuJson, host)
        m.setData("/pinned", JsonValue.Bool(true))
        assertTrue("the target is the ONE child, inline", m.node(m.indexOf("ctx")!!)!!.isContextMenu)
        assertTrue(m.layers.isEmpty())
        openByLongPress(m)
        assertEquals("a long press opens the menu", listOf("ctx"), m.layers.map { it.owner })
        // The checkbox row: the core's `checked` state + the `Menu.check` glyph part, a checkbox to a11y.
        val pin = m.node(m.indexOf("ctx.item.1")!!)!!
        assertTrue(pin.checked)
        assertTrue("the check glyph part", m.nodes.any { it.owner == "ctx" && it.part == "check" && it.parent == pin.index })
        compose.onNode(hasContentDescription("Pin", substring = true))
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.ToggleableState, ToggleableState.On))
        // The submenu row: its indicator; a press opens the submenu as the core's second layer.
        val move = m.indexOf("ctx.item.3")!!
        assertTrue(m.nodes.any { it.owner == "ctx" && it.part == "submenuIndicator" && it.parent == move })
        m.press(move)
        compose.waitForIdle()
        assertEquals(2, m.layers.size)
        val sub = m.layers.last()
        assertTrue("the second layer is the submenu", m.isSubmenuLayer(sub))
        assertFalse(m.isSubmenuLayer(m.layers.first()))
        assertTrue("the open row", m.node(m.indexOf("ctx.item.3")!!)!!.open)
        compose.onNode(hasContentDescription("Backlog")).assertExists()
        // An outside tap on the submenu closes only the submenu.
        m.closeSubmenu("ctx")
        compose.waitForIdle()
        assertEquals(listOf("ctx"), m.layers.map { it.owner })
        // A submenu entry selects with its value and closes the menu.
        m.press(m.indexOf("ctx.item.3")!!)
        m.press(m.indexOf("ctx.item.3.0")!!)
        compose.waitForIdle()
        assertTrue(m.layers.isEmpty())
        assertEquals(listOf("select"), host.actions.map { it.name })
        assertEquals(JsonValue.Str("backlog"), (host.actions.last().payload as? JsonValue.Obj)?.v?.get("value"))
    }

    @Test
    fun aCheckboxRowWritesItsBinding() {
        val host = RecordingHost()
        val m = render(menuJson, host, OverlayPresentation.Painted)
        m.setData("/pinned", JsonValue.Bool(false))
        openByLongPress(m)
        assertFalse(m.node(m.indexOf("ctx.item.1")!!)!!.checked)
        tap(m, "ctx.item.1")
        assertEquals(listOf("select"), host.actions.map { it.name })
        assertEquals(JsonValue.Bool(true), (host.actions.last().payload as? JsonValue.Obj)?.v?.get("checked"))
        // The binding flipped: the row reopens checked.
        if (m.layers.isEmpty()) openByLongPress(m)
        assertTrue("the binding flips", m.node(m.indexOf("ctx.item.1")!!)!!.checked)
    }

    @Test
    fun aPaintedSubmenuOpensBesideItsRowWithoutItsOwnCatcher() {
        val m = render(menuJson, overlays = OverlayPresentation.Painted)
        openByLongPress(m)
        tap(m, "ctx.item.3")
        assertEquals(2, m.layers.size)
        val row = m.frame(m.indexOf("ctx.item.3")!!)
        val sub = m.layers.last().frame
        assertTrue("beside its row (inline end): ${sub.left} vs ${row.right}", sub.left >= row.right - 0.5f)
        // The parent rows stay pressable while the submenu is open (no catcher over them).
        tap(m, "ctx.item.3")
        assertEquals("the row toggles the submenu closed", listOf("ctx"), m.layers.map { it.owner })
    }

    @Test
    fun aPressMenuOpensFromItsChildTrigger() {
        val m = render(
            """{"id":"root","component":"Box","children":[{"id":"more","component":"Menu","props":{"items":[{"label":"Rename","value":"rename"}]},
                 "children":[{"id":"btn","component":"Button","props":{"label":"More"}}]}]}""",
        )
        assertEquals("more", m.node(m.indexOf("btn")!!)!!.triggerFor)
        tap(m, "btn")
        assertEquals(listOf("more"), m.layers.map { it.owner })
        compose.onNode(hasContentDescription("Rename")).assertExists()
    }

    // -- TreeGuides ----------------------------------------------------

    @Test
    fun treeGuideGeometryIsFourteenThreeOne() {
        assertEquals(14f, CatalogConstants.TREE_GUIDE_COLUMN)
        assertEquals(3f, CatalogConstants.TREE_GUIDE_RADIUS)
        assertEquals(1f, CatalogConstants.TREE_GUIDE_BRIDGE)
        fun props(json: String) = JsonValue.parse(json).obj!!
        fun ops(json: String, rtl: Boolean = false, width: Float = 28f) =
            ArrayList<TreeGuideOp>().also { out -> treeGuideGeometry(props(json), rtl).paint(width, 32f, 1f) { out.add(it) } }
        // Depth 2, the elbow in column 1, column 0 passing through.
        assertEquals(
            listOf(
                TreeGuideOp.Vertical(7f, -1f, 32f),
                TreeGuideOp.Vertical(21f, -1f, 13f),
                TreeGuideOp.Elbow(21.5f, 16f, 3f, 1f, 28f),
            ),
            ops("""{"depth":2,"elbowAt":1,"tee":false,"passThrough":[0]}"""),
        )
        // A tee carries the vertical to the bottom.
        assertEquals(
            listOf(TreeGuideOp.Vertical(7f, -1f, 32f), TreeGuideOp.Elbow(7.5f, 16f, 3f, 1f, 14f)),
            ops("""{"depth":1,"elbowAt":0,"tee":true,"passThrough":[]}""", width = 14f),
        )
        // RTL mirrors the gutter.
        assertEquals(
            listOf(TreeGuideOp.Vertical(6f, -1f, 13f), TreeGuideOp.Elbow(6.5f, 16f, 3f, -1f, 0f)),
            ops("""{"depth":1,"elbowAt":0,"tee":false,"passThrough":[]}""", rtl = true, width = 14f),
        )
    }

    @Test
    fun aNestedRowReservesItsGuideColumns() {
        Fixtures.require()
        val m = makeModel(theme = "neutral")
        m.setNested(
            """{"id":"s","component":"Section","props":{"tree":true},"children":[
                 {"id":"a","component":"Row","props":{"title":"A"}},
                 {"id":"b","component":"Row","props":{"title":"B","depth":1}},
                 {"id":"c","component":"Row","props":{"title":"C","depth":2}}]}""",
        )
        val guides = m.nodes.filter { it.component == "TreeGuides" }
        assertEquals(2, guides.size)
        for (g in guides) {
            val depth = g.props["depth"]?.number ?: 0.0
            assertEquals("${g.id} = depth × 14", depth.toFloat() * 14f, m.frame(g.index).width, 0.5f)
        }
        // The core filled the deepest row's guides: its elbow in column 1.
        val deepest = guides.first { it.props["depth"]?.number == 2.0 }
        assertEquals(1.0, deepest.props["elbowAt"]?.number)
    }
}
