package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.measure.MarkdownShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.primitives.Markdown
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
 * #VAPP-258 (catalog/host.json `markdown`): the host hooks PAINTED, under
 * the real Compose shaper: an inline chip is measured as painted (its
 * placeholder width in the measurer's line breaking) and never grows a
 * line; a block image press and a chip tap reach the host; attachment
 * blocks paint at the measured heights.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h914dp-420dpi")
class MarkdownHostPaintTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density
    private val current = mutableStateOf<Pair<SurfaceModel, Float>?>(null)
    private val a = "https://app.example/api/attachments/"

    private fun render(text: String, width: Float = 390f, host: FakeMarkdownHost = FakeMarkdownHost(Fixtures.json("markdown-host.json"))): Pair<SurfaceModel, FakeMarkdownHost> {
        Fixtures.require()
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("md", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        val escaped = text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")
        m.setNested("""{"id":"md","component":"Markdown","props":{"text":"$escaped"}}""")
        if (current.value == null) {
            current.value = m to width
            compose.setContent { current.value?.let { (model, w) -> Box(Modifier.width(w.dp)) { ExponentialSurface(model) } } }
        } else {
            compose.runOnIdle { current.value = m to width }
        }
        compose.waitForIdle()
        return m to host
    }

    private fun measured(m: SurfaceModel): Float = m.frame(m.indexOf("md")!!).height

    /** The painted layout of the text node holding [text] (a chip's label is its alternate text). */
    private fun painted(text: String): TextLayoutResult {
        val nodes = compose.onAllNodes(hasText(text, substring = true), useUnmergedTree = true).fetchSemanticsNodes()
        assertTrue("\"$text\" painted", nodes.isNotEmpty())
        val results = ArrayList<TextLayoutResult>()
        nodes.first().config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
        return results.single()
    }

    @Test
    fun aChipIsMeasuredAsPaintedAndNeverGrowsALine() {
        val (m, _) = render("see #ABC-1 now")
        val withChip = measured(m)
        val r = painted("ABC-1 Fix login")
        assertEquals("one line", 1, r.lineCount)
        assertEquals("measure = paint", withChip, r.size.height / density, 0.5f)
        val chip = r.placeholderRects.filterNotNull().single()
        val styles = MarkdownPainter.styles(m.effectiveTheme, m.mode, at.exponential.ui.theme.ResolvedTextStyle(14f, 400, 20f, null), emptyMap())
        assertTrue("the chip is no taller than the line (${chip.height / density})", chip.height / density <= minOf(styles.chip.height, 20f) + 0.5f)
        val plain = measured(render("see ABC-1 Fix login now").first)
        assertEquals("a chip that fits = the plain paragraph's height", plain, withChip, 0.01f)
    }

    @Test
    fun aWrappingChipIsMeasuredAsPainted() {
        // Narrow: the chip wraps as one unbreakable word.
        for (w in listOf(90f, 120f, 160f, 200f)) {
            val (m, _) = render("see the #ABC-1 and #ABC-1 and @ann@example.com too", width = w)
            val r = painted("ABC-1 Fix login")
            assertEquals("width $w: measure = paint (${r.lineCount} lines)", measured(m), r.size.height / density, 0.5f)
            // The measurer's own chip layout = the painter's placeholder widths.
            val styles = MarkdownPainter.styles(m.effectiveTheme, m.mode, at.exponential.ui.theme.ResolvedTextStyle(14f, 400, 20f, null), emptyMap())
            val md = MarkdownShaper(m.textShaper(), null)
            val chipW = md.labelWidth("ABC-1 Fix login", styles.chip).let { styles.chip.padH * 2 + it + styles.chip.iconSize + styles.chip.gap }
            val rect = r.placeholderRects.filterNotNull().first()
            assertEquals("width $w: the chip's painted width", minOf(chipW, w), rect.width / density, 1f)
        }
    }

    @Test
    fun aChipTapOpensItsHrefAndAnImagePressReachesTheHost() {
        val (_, host) = render("see #ABC-1 now")
        compose.onNode(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button), useUnmergedTree = true).performClick()
        compose.waitForIdle()
        assertEquals(listOf("https://app.example/i/ABC-1"), host.urls)
        val src = "data:image/png;base64,iVBORw0KGgo="
        val (_, imageHost) = render("![pic]($src)")
        compose.onNode(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Image), useUnmergedTree = true).performClick()
        compose.waitForIdle()
        assertEquals(listOf(src), imageHost.pressed)
    }

    @Test
    fun aSizedImageChangesTheMeasuredHeight() {
        val (unknown, _) = render("![x](${a}unknown.png)", width = 200f)
        assertEquals(160f, measured(unknown), 0.01f)
        val (wide, _) = render("![x](${a}wide.png)", width = 200f)
        assertEquals("1600 × 800 in 200", 100f, measured(wide), 0.01f)
    }

    @Test
    fun attachmentsPaintAtTheirMeasuredHeights() {
        for ((text, painted) in listOf("[clip](${a}clip.mp4)" to null, "[voice](${a}voice.m4a)" to "voice.m4a", "[the report](${a}report.pdf)" to "report.pdf")) {
            val (m, _) = render(text)
            val blocks = m.markdownBlocks(text)
            val styles = MarkdownPainter.styles(m.effectiveTheme, m.mode, at.exponential.ui.theme.ResolvedTextStyle(14f, 400, 20f, null), emptyMap())
            val expected = Markdown.layout(blocks, styles, 390f, MarkdownShaper(m.textShaper(), null)).height
            assertTrue("$text is an attachment", blocks.single().kind is at.exponential.ui.primitives.MarkdownBlockKind.Attachment)
            assertEquals(text, expected, measured(m), 0.5f)
            if (painted != null) painted(painted)
        }
    }
}
