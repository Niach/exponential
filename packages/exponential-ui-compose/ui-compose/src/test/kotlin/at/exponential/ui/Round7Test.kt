package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.HostedPopup
import at.exponential.ui.compose.LocalPopupHost
import at.exponential.ui.compose.PopupHost
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.model.press
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
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
 * VAPP-281 (docs/round-7-contract.md): with `SurfaceSettings.hostedPopups`
 * and a [LocalPopupHost], an anchored Popover is the HOST's: the host hears
 * the layer (anchor frame, side, align, the unclipped size above the 44 dp
 * panel surface), composes its content where it wants (here a tagged Box),
 * a tap inside it routes back to the surface (an action), and its dismiss
 * closes the layer; the surface paints no popup of its own for it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class Round7Test {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private class Host : PopupHost {
        val heard = ArrayList<List<HostedPopup>>()
        var dismiss: ((String) -> Unit)? = null

        @Composable
        override fun Popups(popups: List<HostedPopup>, dismiss: (owner: String) -> Unit) {
            heard.add(popups)
            this.dismiss = dismiss
            for (p in popups) Box(Modifier.testTag("hosted-${p.owner}")) { p.content() }
        }
    }

    @Test
    fun hostedPopupsReachTheHostAndRouteBack() {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val actions = RecordingHost()
        val m = SurfaceModel(
            "r7",
            SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark, settings = SurfaceSettings(hostedPopups = true)),
            actions,
            CoroutineScope(Dispatchers.Unconfined),
        ) { shaper }
        m.setNested(
            """{"id": "panel", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "space-between", "alignItems": "center", "width": 390, "height": 44},
              "children": [
                {"id": "start", "component": "Button", "props": {"label": "Start"}},
                {"id": "clock", "component": "Button", "props": {"label": "12:00"}},
                {"id": "calendar", "component": "Popover", "props": {"anchor": "clock", "side": "top", "align": "end"},
                 "children": [{"id": "today", "component": "Button", "props": {"label": "Today"}, "on": {"press": {"event": {"name": "today"}}}}]}]}""",
        )
        m.setViewport(390f, 44f)
        val host = Host()
        compose.setContent {
            CompositionLocalProvider(LocalPopupHost provides host) {
                Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) }
            }
        }
        compose.waitForIdle()
        m.press("clock")
        compose.waitForIdle()
        val layer = m.layers.first { it.owner == "calendar" }
        assertTrue(layer.hosted)
        val popup = host.heard.last().single()
        assertEquals("calendar", popup.owner)
        assertEquals("top", popup.side)
        assertEquals("end", popup.align)
        assertTrue("above the 44 dp surface: ${layer.frame}", layer.frame.top < 0f)
        assertEquals("end-aligned", popup.anchor.right, layer.frame.right, 0.6f)
        // The content composes in the host's Box; a tap routes to the surface.
        compose.onNodeWithTag("hosted-calendar").assertExists()
        compose.onNodeWithTag("hosted-calendar").performClick()
        compose.waitForIdle()
        assertTrue(actions.actions.any { it.name == "today" })
        // The host's window system dismissed it.
        compose.runOnIdle { host.dismiss!!("calendar") }
        compose.waitForIdle()
        assertTrue(m.layers.none { it.owner == "calendar" })
        assertTrue(host.heard.last().isEmpty())
    }
}
