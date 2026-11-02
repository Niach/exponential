package at.exponential.ui

import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.click
import androidx.compose.ui.test.doubleClick
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.pinch
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.test.requestFocus
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.graphics.ImageBitmap
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * EXP-1586: the `ZoomableImage` native through the composed surface: THE
 * pill (its ends, the percent, fit selected at rest, the preset Menu), the
 * gestures (double tap, pinch, a swipe that belongs to the host at rest and
 * to the picture when zoomed), the keys, and the `zoom` / `tap` events. A
 * 1440 × 900 picture in a 390 dp column = 243.75 dp tall, fit 27 %.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ZoomableImageTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private val node = """{"id":"z","component":"ZoomableImage","props":{"src":"https://example.com/shot.png","alt":"Screenshot","naturalWidth":1440,"naturalHeight":900,"controls":"always"},"on":{"zoom":{"event":{"name":"zoomed"}},"tap":{"event":{"name":"tapped"}}}}"""

    /** The surface over a horizontal draggable (a stand-in for the host's pager): [hostDrag] collects what reaches it. */
    private fun render(json: String = node, host: RecordingHost = RecordingHost(), hostDrag: FloatArray = FloatArray(1)): SurfaceModel {
        Fixtures.require()
        // The picture has loaded (the pill only draws over a loaded picture).
        LeafImages.seed("https://example.com/shot.png", ImageBitmap(1440, 900))
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("zoom", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().draggable(rememberDraggableState { hostDrag[0] += it }, Orientation.Horizontal)) { ExponentialSurface(m) }
        }
        compose.waitForIdle()
        return m
    }

    private fun percent(): String? =
        compose.onNodeWithContentDescription("Zoom level").fetchSemanticsNode().config.getOrNull(SemanticsProperties.StateDescription)

    private fun picture(): SemanticsNodeInteraction = compose.onNodeWithTag("zoomable-image")

    private fun settle() {
        compose.mainClock.advanceTimeBy(1000)
        compose.waitForIdle()
    }

    @Test
    fun thePillReadsTheZoomAndStepsThroughThePresets() {
        val host = RecordingHost()
        val m = render(host = host)
        val f = m.frame(m.indexOf("z")!!)
        assertEquals(390f, f.width, 0.5f)
        assertEquals(243.75f, f.height, 0.5f)
        assertEquals("27%", percent())
        // THE pill: bottom-centre, `pill.inset` (16) above the box's bottom, `pill.height` (36) tall.
        val pill = compose.onNodeWithTag("zoom-pill", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val box = picture().fetchSemanticsNode().boundsInRoot
        assertEquals(36f, pill.height / density, 0.5f)
        assertEquals(16f, (box.bottom - pill.bottom) / density, 0.5f)
        assertEquals(box.center.x, pill.center.x, density)
        compose.onNodeWithContentDescription("Zoom out").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Zoom in").assertIsEnabled()
        compose.onNodeWithContentDescription("Zoom to fit").assertIsSelected()
        compose.onNodeWithContentDescription("Zoom in").performClick()
        settle()
        assertEquals("33%", percent())
        compose.onNodeWithContentDescription("Zoom to fit").assertIsNotSelected()
        compose.onNodeWithContentDescription("Zoom out").assertIsEnabled()
        val zoomed = host.actions.filter { it.name == "zoomed" }
        assertEquals(1, zoomed.size)
        assertEquals(0.33, zoomed[0].payload?.get("scale")?.number ?: -1.0, 1e-4)
        assertEquals(false, zoomed[0].payload?.get("atFit")?.bool)
        compose.onNodeWithContentDescription("Zoom to fit").performClick()
        settle()
        assertEquals("27%", percent())
        assertEquals(true, host.actions.last { it.name == "zoomed" }.payload?.get("atFit")?.bool)
    }

    @Test
    fun thePercentOpensThePresetMenu() {
        render()
        compose.onNodeWithContentDescription("Zoom level").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag("zoom-menu").assertExists()
        compose.onNodeWithTag("zoom-menu-Fit").assertIsSelected()
        compose.onNodeWithTag("zoom-menu-200%").performClick()
        settle()
        assertEquals("200%", percent())
        compose.onNodeWithTag("zoom-menu").assertDoesNotExist()
    }

    @Test
    fun aDoubleTapZoomsTo100AndBack() {
        render()
        picture().performTouchInput { doubleClick(center) }
        settle()
        assertEquals("100%", percent())
        picture().performTouchInput { doubleClick(center) }
        settle()
        assertEquals("27%", percent())
    }

    @Test
    fun aPinchZoomsAboutTheFingers() {
        render()
        picture().performTouchInput {
            pinch(center - Offset(20f * density, 0f), center - Offset(120f * density, 0f), center + Offset(20f * density, 0f), center + Offset(120f * density, 0f))
        }
        settle()
        val p = percent()!!.removeSuffix("%").toInt()
        assertTrue("pinched out from 27 % to $p % (× 6 = 163 %)", p in 120..200)
    }

    @Test
    fun aSwipeWithoutRoomReachesTheHostAndPansWhenZoomed() {
        val drag = FloatArray(1)
        render(hostDrag = drag)
        picture().performTouchInput { swipeLeft() }
        settle()
        assertTrue("at rest the swipe pages the host (${drag[0]})", drag[0] < -50f)
        assertEquals("27%", percent())
        picture().performTouchInput { doubleClick(center) }
        settle()
        assertEquals("100%", percent())
        drag[0] = 0f
        picture().performTouchInput { swipeLeft() }
        settle()
        assertEquals("zoomed, the picture takes the swipe", 0f, drag[0], 0.01f)
        assertEquals("100%", percent())
    }

    @Test
    fun aSingleTapFiresTap() {
        val host = RecordingHost()
        render(host = host)
        picture().performTouchInput { click(center) }
        settle()
        assertEquals(1, host.actions.count { it.name == "tapped" })
        assertEquals("27%", percent())
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun theKeysStepResetAndJumpTo100() {
        render()
        picture().requestFocus()
        picture().performKeyInput { pressKey(Key.Equals) }
        settle()
        assertEquals("33%", percent())
        picture().performKeyInput { pressKey(Key.One) }
        settle()
        assertEquals("100%", percent())
        picture().performKeyInput { pressKey(Key.Minus) }
        settle()
        assertEquals("67%", percent())
        picture().performKeyInput { pressKey(Key.Zero) }
        settle()
        assertEquals("27%", percent())
    }

    @Test
    fun aPictureThatHasNotLoadedDrawsNoPill() {
        // style.json zoomableImage: pending or failed = nothing to zoom, no pill (the placeholder stays).
        val m = render(node.replace("shot.png", "never-loads.png"))
        val f = m.frame(m.indexOf("z")!!)
        assertEquals(243.75f, f.height, 0.5f)
        compose.onNodeWithTag("zoom-pill").assertDoesNotExist()
    }

    @Test
    fun controlsNoneDrawsNoPillAndTheBoundScaleZooms() {
        val m = render(node.replace("\"always\"", "\"none\"").replace("\"controls\"", "\"scale\":{\"path\":\"/scale\"},\"controls\""))
        compose.onNodeWithTag("zoom-pill").assertDoesNotExist()
        compose.runOnIdle { m.setData("/scale", at.exponential.ui.json.JsonValue.Num(2.0)) }
        settle()
        assertEquals("200%", picture().fetchSemanticsNode().config.getOrNull(SemanticsProperties.StateDescription))
        // Two-way: a settled zoom writes the bound `scale` back (the core's write-through on `zoom`).
        picture().performTouchInput { doubleClick(center) }
        settle()
        assertEquals("27%", picture().fetchSemanticsNode().config.getOrNull(SemanticsProperties.StateDescription))
        assertEquals(0.270833, m.data["scale"]?.number ?: -1.0, 1e-4)
    }
}
