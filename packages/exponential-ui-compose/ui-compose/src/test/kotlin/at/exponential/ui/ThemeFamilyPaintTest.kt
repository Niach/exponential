package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
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
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * #VAPP-166: text with no family of its own paints in the theme's family
 * (`type.family.sans`), never the platform default (Roboto). The spezl
 * drafts' harness built its model with the kitchen sink's pre-surface
 * shaper (a `{ null }` font resolver); `ExponentialSurface` then installs
 * the surface's own shaper, but leaves whose frame did not move never
 * recomposed and kept the first pass's Roboto style. The model's shaper is
 * snapshot state now: swapping it repaints every text leaf.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34])
class ThemeFamilyPaintTest {
    @get:Rule
    val compose = createComposeRule()

    private val repo: File get() = Fixtures.dir.parentFile.parentFile.parentFile

    private class FontHost(val nunito: FontFamily) : HostPlugin {
        override fun fontFamily(name: String): FontFamily? = nunito.takeIf { name == "Nunito" }
        override fun resolveUrl(src: String): String = ""
    }

    @Test
    fun aTextWithoutAFamilyPaintsInTheThemeFamily() {
        Fixtures.require()
        val nunito = FontFamily(Font(File(repo, "packages/exponential-ui/conformance/fonts/Nunito-Regular.ttf"), FontWeight(400)))
        val options = SurfaceOptions(theme = ThemeHandle.builtin("playful"), mode = Mode.Light)
        // The kitchen sink's pre-surface shaper: it resolves no family.
        val first = robolectricShaper(FontResolver { null })
        val m = SurfaceModel("family", options, FontHost(nunito), CoroutineScope(Dispatchers.Unconfined)) { first }
        m.setNested(
            """{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","alignItems":"flex-start"},"children":[""" +
                """{"id":"hug","component":"Text","props":{"text":"spezl hug"}},""" +
                """{"id":"fixed","component":"Text","props":{"text":"spezl fixed"},"style":{"width":200}}]}""",
        )
        m.setViewport(390f, 600f)
        compose.setContent { Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        for (text in listOf("spezl hug", "spezl fixed")) {
            val nodes = compose.onAllNodesWithText(text, useUnmergedTree = true).fetchSemanticsNodes()
            assertTrue("\"$text\" painted", nodes.isNotEmpty())
            for (n in nodes) {
                val results = ArrayList<TextLayoutResult>()
                n.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
                val r = results.firstOrNull() ?: continue
                assertEquals("\"$text\" paints in the theme's sans (Nunito), not the default", nunito, r.layoutInput.style.fontFamily)
            }
        }
    }
}
