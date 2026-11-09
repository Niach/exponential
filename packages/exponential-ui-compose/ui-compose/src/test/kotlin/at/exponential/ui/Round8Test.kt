package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.geometry.Offset
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.MotionList
import at.exponential.ui.compose.MotionPainter
import at.exponential.ui.ffi.CanvasPlayer
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Round 8 (docs/round-8-contract.md) on the Compose painter: the Canvas
 * leaf (native text after the shapes and its baseline rule, an image named
 * by `label`, `pointer` events in drawing units, inert without it),
 * Checkbox `indeterminate` (ToggleableState.Indeterminate), the Progress
 * `value`'s tabular figures and Stepper `collapse: auto` through the core's
 * `@container` pass. `EXPONENTIAL_UI_CANVAS_PNG=<file>` saves the Canvas
 * specimen as painted.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class Round8Test {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun model(host: RecordingHost = RecordingHost(), theme: String = "exponential", mode: Mode = Mode.Dark): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        return SurfaceModel("r8", SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = mode), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
    }

    private fun show(m: SurfaceModel) {
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) {
                ExponentialSurface(m)
            }
        }
        compose.waitForIdle()
    }

    /** The ink rows (any pixel with alpha) of a bitmap: first..last, or null. */
    private fun inkRows(b: Bitmap): IntRange? {
        var first = -1
        var last = -1
        for (y in 0 until b.height) for (x in 0 until b.width) {
            if ((b.getPixel(x, y) ushr 24) > 40) {
                if (first < 0) first = y
                last = y
                break
            }
        }
        return if (first < 0) null else first..last
    }

    private fun paintText(baseline: String, align: String = "start"): Bitmap {
        val doc = """{"size":[200,100],"shapes":[{"type":"text","text":"HHHH","position":[100,50],"size":20,"align":"$align","baseline":"$baseline","color":"#ffffff"}]}"""
        val list = MotionList.decode(CanvasPlayer(doc).sample(""))!!
        val b = Bitmap.createBitmap(200, 100, Bitmap.Config.ARGB_8888)
        MotionPainter.paint(Canvas(b), list, 200f, 100f)
        return b
    }

    @Test
    fun textOpsFollowTheBaselineAndAlignRules() {
        // alphabetic: the caps sit ON y = 50; top: they hang from it; middle: they straddle it.
        val alphabetic = inkRows(paintText("alphabetic"))!!
        assertTrue("alphabetic $alphabetic", alphabetic.last in 48..51 && alphabetic.first < 40)
        val top = inkRows(paintText("top"))!!
        assertTrue("top $top", top.first >= 50 && top.last > 58)
        val middle = inkRows(paintText("middle"))!!
        assertTrue("middle $middle", middle.first < 50 && middle.last > 50)
        // Physical align: start = the left edge at x, end = the right edge at x.
        fun inkCols(b: Bitmap): IntRange {
            val cols = (0 until b.width).filter { x -> (0 until b.height).any { y -> (b.getPixel(x, y) ushr 24) > 40 } }
            return cols.first()..cols.last()
        }
        val start = inkCols(paintText("alphabetic", "start"))
        val end = inkCols(paintText("alphabetic", "end"))
        val center = inkCols(paintText("alphabetic", "center"))
        assertTrue("start $start", start.first in 99..103)
        assertTrue("end $end", end.last in 96..100)
        assertTrue("center $center", center.first < 100 && center.last > 100)
    }

    @Test
    fun theCanvasCatalogCasesPaintAsNamedImagesInFourFits() {
        // The catalog example and its three fits (catalog-components.json), one column.
        val cases = Fixtures.json("catalog-components.json")["cases"]!!.array!!
            .filter { it["name"]?.string in listOf("Canvas/example", "Canvas/fit=contain", "Canvas/fit=cover", "Canvas/fit=fill") }
        assertEquals(4, cases.size)
        val children = cases.mapIndexed { i, c -> c["node"]!!.json.replaceFirst(Regex("\"id\"\\s*:\\s*\"canvas\""), "\"id\":\"c${i}_canvas\"") }
        val m = model()
        m.setNested("""{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","gap":8},"children":[${children.joinToString(",")}]}""")
        show(m)
        assertEquals(m.issues.toString(), 0, m.issues.size)
        val c0 = m.frame(m.indexOf("c0_canvas")!!)
        // The Motion box: the drawing's 160 px, aspect 16:10.
        assertEquals(160f, c0.width, 0.5f)
        assertEquals(100f, c0.height, 0.5f)
        val images = compose.onAllNodesWithContentDescription("Visits by day", useUnmergedTree = true)
            .fetchSemanticsNodes()
        assertEquals(4, images.size)
        for (n in images) assertEquals(Role.Image, n.config[SemanticsProperties.Role])
        System.getenv("EXPONENTIAL_UI_CANVAS_PNG")?.takeIf { it.isNotEmpty() }?.let { path ->
            compose.onNodeWithTag("shot").captureRoboImage(path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        }
    }

    private fun pointerSurface(pointer: Boolean) =
        """{"id":"root","component":"Box","children":[{"id":"cv","component":"Canvas","props":{"size":[200,100],"label":"Plot","pointer":$pointer,"shapes":[{"type":"rect","position":[100,50],"size":[200,100]},{"type":"fill","color":"${'$'}color.primary"}]},"on":{"press":{"event":{"name":"hit"}},"move":{"event":{"name":"moved"}},"leave":{"event":{"name":"left"}}}}]}"""

    @Test
    fun aPointerCanvasSendsDrawingUnits() {
        val host = RecordingHost()
        val m = model(host)
        m.setNested(pointerSurface(true))
        show(m)
        val f = m.frame(m.indexOf("cv")!!)
        // 200 × 100 drawing in a 200 × 100 dp box (390 wide surface): 1 drawing unit = 1 dp.
        compose.onNodeWithTag("shot").performTouchInput { click(Offset((f.left + 50f) * density, (f.top + 25f) * density)) }
        compose.waitForIdle()
        val names = host.actions.map { it.name }
        assertTrue("events $names", "hit" in names)
        val hit = host.actions.first { it.name == "hit" }
        val ctx = hit.payload ?: hit.context
        assertEquals(50.0, ctx["x"]!!.number!!, 0.6)
        assertEquals(25.0, ctx["y"]!!.number!!, 0.6)
        assertEquals("a lifted finger leaves", "left", names.last())
        // No role, no focus stop: still an image named by its label.
        val node = compose.onAllNodesWithContentDescription("Plot", useUnmergedTree = true).fetchSemanticsNodes().single()
        assertEquals(Role.Image, node.config[SemanticsProperties.Role])
        assertFalse(m.node(m.indexOf("cv")!!)!!.isFocusable)
    }

    @Test
    fun anInertCanvasSendsNothing() {
        val quiet = RecordingHost()
        val inert = model(quiet)
        inert.setNested(pointerSurface(false))
        show(inert)
        val g = inert.frame(inert.indexOf("cv")!!)
        compose.onNodeWithTag("shot").performTouchInput { click(Offset((g.left + 50f) * density, (g.top + 25f) * density)) }
        compose.waitForIdle()
        assertEquals(emptyList<String>(), quiet.actions.map { it.name })
    }

    @Test
    fun aMixedCheckboxIsIndeterminateAndDrawsTheDash() {
        val m = model()
        m.setNested("""{"id":"root","component":"Box","children":[{"id":"all","component":"Checkbox","props":{"label":"Select all","checked":true,"indeterminate":true}}]}""")
        show(m)
        compose.onNode(hasContentDescription("Select all") and SemanticsMatcher.expectValue(SemanticsProperties.ToggleableState, ToggleableState.Indeterminate), useUnmergedTree = true).assertExists()
    }

    @Test
    fun theProgressValueHasTabularFiguresAndTheDetailShows() {
        val m = model()
        m.setNested("""{"id":"p","component":"Progress","props":{"value":65,"label":"Upload","valueText":"65%","detail":"3 of 8 files"}}""")
        show(m)
        val v = m.node(m.indexOf("p.header.value")!!)!!
        assertEquals("tnum", m.textStyle(v.index).forPart(v.ownerComponent ?: v.macroName, v.part, v.id).fontFeatures)
        val label = m.node(m.indexOf("p.header.label")!!)!!
        assertEquals(null, m.textStyle(label.index).forPart(label.ownerComponent ?: label.macroName, label.part, label.id).fontFeatures)
        assertNotNull(m.indexOf("p.detail"))
        assertTrue(m.frame(m.indexOf("p.detail")!!).top > m.frame(m.indexOf("p.track")!!).top)
    }

    @Test
    fun anAutoStepperCollapsesInANarrowParent() {
        val steps = """[{"label":"Account"},{"label":"Team"},{"label":"Done"}]"""
        val m = model()
        m.setNested("""{"id":"root","component":"Box","children":[{"id":"narrow","component":"Box","style":{"width":200},"children":[{"id":"s","component":"Stepper","props":{"steps":$steps,"current":1,"collapse":"auto"}}]}]}""")
        show(m)
        assertEquals(0f, m.frame(m.indexOf("s.full")!!).height, 0.01f)
        assertTrue(m.frame(m.indexOf("s.compact")!!).height > 0f)
    }
}
