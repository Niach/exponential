package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.click
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import at.exponential.ui.compose.ExponentialSurface
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

/**
 * Gestures through the composed surface (not the model calls): a Resizable
 * handle dragged by touch follows the finger even though the handle moves
 * under it, and a `visibility: hidden` button takes no tap.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class Round2GestureTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun render(json: String, host: RecordingHost = RecordingHost()): SurfaceModel {
        Fixtures.require()
        val m = SurfaceModel("gesture", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested(json)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    @Test
    fun aResizableHandleFollowsTheFinger() {
        val m = render("""{"id":"root","component":"Box","children":[{"id":"split","component":"Resizable","style":{"height":200},"children":[{"id":"a","component":"Text","props":{"text":"A"}},{"id":"b","component":"Text","props":{"text":"B"}}]}]}""")
        val panel = m.indexOf("split.panel.0")!!
        val before = m.frame(panel).width
        val handle = m.frame(m.indexOf("split.handle.0")!!)
        val d = density
        val start = Offset(handle.center.x * d, handle.center.y * d)
        // 60 dp to the right in 12 steps: every step lands after the
        // handle has already moved with the split.
        compose.onNodeWithTag("shot").performTouchInput {
            down(start)
            for (i in 1..12) moveTo(start + Offset(5f * d * i, 0f))
        }
        compose.waitForIdle()
        assertEquals("mid-drag the panel follows the finger", before + 60f, m.frame(m.indexOf("split.panel.0")!!).width, 1.5f)
        compose.onNodeWithTag("shot").performTouchInput { up() }
        compose.waitForIdle()
        assertEquals("released where the finger lifted", before + 60f, m.frame(m.indexOf("split.panel.0")!!).width, 1.5f)
    }

    @Test
    fun aHiddenButtonTakesNoTap() {
        val host = RecordingHost()
        val m = render(
            """{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","gap":8},"children":[
              {"id":"shown","component":"Button","props":{"label":"Shown"},"on":{"press":{"event":{"name":"shown"}}}},
              {"id":"gone","component":"Button","props":{"label":"Gone"},"style":{"visibility":"hidden"},"on":{"press":{"event":{"name":"gone"}}}},
              {"id":"wrap","component":"Box","style":{"visibility":"hidden"},"children":[{"id":"inner","component":"Button","props":{"label":"Inner"},"on":{"press":{"event":{"name":"inner"}}}}]}
            ]}""",
            host,
        )
        val d = density
        for (id in listOf("shown", "gone", "inner")) {
            val f = m.frame(m.indexOf(id)!!)
            compose.onNodeWithTag("shot").performTouchInput { click(Offset(f.center.x * d, f.center.y * d)) }
            compose.waitForIdle()
        }
        assertEquals(listOf("shown"), host.actions.map { it.name })
        assertTrue("an invisible button is never focused", m.keyboardFocus == null)
    }
}
