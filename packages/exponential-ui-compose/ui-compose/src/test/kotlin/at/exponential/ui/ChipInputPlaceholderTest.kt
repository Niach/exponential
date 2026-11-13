package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import at.exponential.ui.compose.ExponentialSurface
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
 * #EXP-1813: a ChipInput shows its `placeholder` only while it holds no
 * chips. The core blanks the `input` part's placeholder once `values` is
 * non-empty; the inline field used to fall back to the OWNER's prop and kept
 * the hint beside the chips (React, Solid, Swift and gpui read the part).
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class ChipInputPlaceholderTest {
    @get:Rule
    val compose = createComposeRule()

    private val current = mutableStateOf<SurfaceModel?>(null)

    private fun render(values: String) {
        Fixtures.require()
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("chips", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested("""{"id":"req","component":"ChipInput","props":{"name":"req","values":$values,"placeholder":"Check name"}}""")
        if (current.value == null) {
            current.value = m
            compose.setContent { current.value?.let { Box(Modifier.fillMaxWidth()) { ExponentialSurface(it) } } }
        } else {
            compose.runOnIdle { current.value = m }
        }
        compose.waitForIdle()
    }

    private fun hints() = compose.onAllNodesWithText("Check name", useUnmergedTree = true).fetchSemanticsNodes().size

    @Test
    fun theHintShowsWhileEmptyAndHidesOnceAChipIsIn() {
        render("[]")
        assertTrue("an empty ChipInput paints its placeholder", hints() > 0)
        render("""["build"]""")
        assertEquals("a chip hides the placeholder", 0, hints())
    }
}
