package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.text.TextLayoutResult
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlin.math.ceil
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * VAPP-164: a content-sized label paints WHOLE at the width the measurer
 * gave it. At a fractional density (420 dpi = 2.625, the phones' and the
 * emulator's) every dp padding and gap rounds to whole px while the core
 * lays out at fractional dp, so a row of segments drifting ~1 px per box
 * ellipsized "Board" to "Boa…" (segmented + toggles; outline's border
 * slack hid it). Every Segmented variant, Tabs, Chip, Button and Toggle
 * label of the specimen must lay out unellipsized.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h914dp-420dpi")
class LabelFitTest {
    @get:Rule
    val compose = createComposeRule()

    private val current = mutableStateOf<SurfaceModel?>(null)

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun render(json: String, theme: String): SurfaceModel {
        Fixtures.require()
        val options = SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = Mode.Dark)
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("fit", options, RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        // One composition per test: later renders swap the model.
        if (current.value == null) {
            current.value = m
            compose.setContent { current.value?.let { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(it) } } }
        } else {
            compose.runOnIdle { current.value = m }
        }
        compose.waitForIdle()
        return m
    }

    /** Every node painting [text]: its layout must neither ellipsize nor overflow. */
    private fun assertWhole(text: String, what: String) {
        val nodes = compose.onAllNodesWithText(text, useUnmergedTree = true).fetchSemanticsNodes()
        assertTrue("$what: \"$text\" painted", nodes.isNotEmpty())
        for (n in nodes) {
            val results = ArrayList<TextLayoutResult>()
            n.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
            val r = results.firstOrNull() ?: continue
            val shown = n.config.getOrNull(SemanticsProperties.Text)?.joinToString { it.text }
            assertFalse("$what: \"$shown\" ellipsized (${r.size.width} px wide, needs ${r.multiParagraph.maxIntrinsicWidth} px)", r.isLineEllipsized(0))
            assertTrue("$what: \"$shown\" laid out narrower than it needs (${r.size.width} < ${r.multiParagraph.maxIntrinsicWidth} px)", r.size.width >= ceil(r.multiParagraph.maxIntrinsicWidth))
        }
    }

    private var serial = 0

    private fun segmented(variant: String?, size: String? = null, items: String = """[{"label":"List","value":"list"},{"label":"Board","value":"board"}]"""): String {
        val v = (variant?.let { ""","variant":"$it"""" } ?: "") + (size?.let { ""","size":"$it"""" } ?: "")
        serial += 1
        return """{"id":"s$serial","component":"Segmented","props":{"items":$items,"value":"list"$v}}"""
    }

    private fun column(vararg children: String) =
        """{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","gap":"${'$'}spacing.lg","padding":"${'$'}spacing.xl","alignItems":"flex-start"},"children":[${children.joinToString(",")}]}"""

    @Test
    fun theSegmentedSpecimenLabelsFitInEveryVariantAndTheme() {
        for (theme in listOf("exponential", "neutral", "playful", "orange")) {
            val cases = ArrayList<String>()
            for (variant in listOf(null, "segmented", "toggles", "outline")) {
                for (size in listOf(null, "sm", "lg")) cases.add(segmented(variant, size))
            }
            // Many segments: the drift grows with every box.
            cases.add(segmented("toggles", items = """[{"label":"List","value":"list"},{"label":"Board","value":"board"},{"label":"Timeline","value":"t"},{"label":"Calendar","value":"c"}]"""))
            // Icons beside labels.
            cases.add(segmented("segmented", items = """[{"label":"List","value":"list","icon":"list"},{"label":"Board","value":"board","icon":"layout-grid"}]"""))
            render(column(*cases.toTypedArray()), theme)
            for (label in listOf("List", "Board", "Timeline", "Calendar")) assertWhole(label, theme)
        }
    }

    @Test
    fun theOtherContentSizedLabelsFit() {
        for (theme in listOf("exponential", "neutral", "playful", "orange")) {
            render(
                column(
                    """{"id":"b1","component":"Button","props":{"label":"Board"}}""",
                    """{"id":"b2","component":"Button","props":{"label":"Boarding","icon":"plus","variant":"outline"}}""",
                    """{"id":"b3","component":"Button","props":{"label":"Boardroom","size":"sm","variant":"ghost"}}""",
                    """{"id":"t1","component":"Toggle","props":{"label":"Bold board","pressed":true}}""",
                    """{"id":"c1","component":"Chip","props":{"label":"Boardwalk"}}""",
                    """{"id":"c2","component":"Chip","props":{"label":"Billboard","removable":true}}""",
                    """{"id":"tb","component":"Tabs","props":{"value":"a","tabs":[{"label":"Overview","value":"a"},{"label":"Board","value":"b"},{"label":"Billing","value":"c","count":3}]},"children":[{"id":"p1","component":"Text","props":{"text":"x"}},{"id":"p2","component":"Text","props":{"text":"y"}},{"id":"p3","component":"Text","props":{"text":"z"}}]}""",
                ),
                theme,
            )
            for (label in listOf("Board", "Boarding", "Boardroom", "Bold board", "Boardwalk", "Billboard", "Overview", "Billing")) assertWhole(label, "$theme $label")
        }
    }
}
