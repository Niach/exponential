package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.measure.MarkdownShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.primitives.MARKDOWN_BLOCK_TAG
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * #VAPP-258 step 5 (catalog/host.json `markdown.options`): the host's render
 * options reach the measurer as the painter reads them, and a long document
 * composes only its visible blocks at the measured height.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w411dp-h914dp-420dpi")
class MarkdownOptionsTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private class OptionsHost(override val markdownOptions: MarkdownOptions) : HostPlugin

    private fun model(options: MarkdownOptions, nested: String): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("md", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), OptionsHost(options), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(nested)
        m.setViewport(320f, 900f)
        return m
    }

    private fun markdown(text: String) = """{"id":"md","component":"Markdown","props":{"text":"${text.replace("\n", "\\n")}"}}"""

    private fun measured(m: SurfaceModel): Float = m.frame(m.indexOf("md")!!).height

    private fun laidOut(m: SurfaceModel, text: String): Float {
        val i = m.indexOf("md")!!
        val node = m.node(i)!!
        val styles = MarkdownPainter.styles(m.effectiveTheme, m.mode, m.textStyle(i), MarkdownPainter.props(node.props, m.host.markdownOptions.chat))
        return Markdown.layout(m.markdownBlocks(text), styles, m.frame(i).width, MarkdownShaper(m.textShaper(), m.effectiveTheme?.monoFamily)).height
    }

    @Test
    fun chatDensityMeasuresAsItPaints() {
        val text = "# Title\n\nOne paragraph.\n\nTwo.\n\nThree."
        val doc = model(MarkdownOptions(), markdown(text))
        val chat = model(MarkdownOptions(chat = true), markdown(text))
        assertEquals(laidOut(chat, text), measured(chat), 0.5f)
        assertEquals(laidOut(doc, text), measured(doc), 0.5f)
        assertNotEquals(measured(doc), measured(chat))
        val i = chat.indexOf("md")!!
        val styles = MarkdownPainter.styles(chat.effectiveTheme, chat.mode, chat.textStyle(i), MarkdownPainter.props(chat.node(i)!!.props, true))
        assertEquals(22f, styles.body.lineHeight)
    }

    @Test
    fun hardBreaksMeasureEveryLine() {
        val soft = measured(model(MarkdownOptions(), markdown("a\nb\nc")))
        val hard = model(MarkdownOptions(hardBreaks = true), markdown("a\nb\nc"))
        assertEquals(3 * soft, measured(hard), 0.5f)
        assertEquals(laidOut(hard, "a\nb\nc"), measured(hard), 0.5f)
    }

    @Test
    fun streamingHoldsTheTailBack() {
        val m = model(MarkdownOptions(streaming = true), markdown("Say **bo"))
        assertEquals("Say bo", m.markdownBlocks("Say **bo")[0].inlines.joinToString("") { it.text })
        assertEquals(laidOut(m, "Say **bo"), measured(m), 0.5f)
    }

    @Test
    fun aLongDocumentComposesOnlyVisibleBlocks() {
        val text = (0 until 80).joinToString("\n\n") { "Paragraph $it of a long description." }
        val m = model(MarkdownOptions(), """{"id":"root","component":"ScrollArea","style":{"height":300},"children":[${markdown(text)}]}""")
        compose.setContent { Box(Modifier.width(320.dp)) { ExponentialSurface(m) } }
        compose.waitForIdle()
        val composed = compose.onAllNodes(hasTestTag(MARKDOWN_BLOCK_TAG), useUnmergedTree = true).fetchSemanticsNodes().size
        assertTrue("$composed of 80 blocks composed", composed in 1 until 40)
        assertEquals(laidOut(m, text), measured(m), 0.5f)
    }
}
