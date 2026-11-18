package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.TextUnit
import at.exponential.ui.primitives.HIGHLIGHT_MARK_TAG
import at.exponential.ui.primitives.highlightFillRects
import at.exponential.ui.primitives.highlightMarkFill
import at.exponential.ui.compose.ExponentialSurface
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
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * #EDE-95: the `highlight` specimens (fixtures/specimens.json) paint
 * `$color.match` behind each match and `$color.matchCurrent` behind the
 * current one (the text keeps its colour; frames unchanged), each fill in
 * the GLYPH box (the run's font's ascent and descent around the line's
 * baseline, as inline code's), never the line box: never a span background.
 * Pictures: build/tmp/highlight-paint/ (one png per test).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HighlightPaintTest {
    @get:Rule
    val compose = createComposeRule()

    private fun render(json: String): SurfaceModel {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("paint", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    private fun capture(name: String): Bitmap {
        val file = File("build/tmp/highlight-paint/$name.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        return BitmapFactory.decodeFile(file.absolutePath)
    }

    private fun textNodes(): List<SemanticsNode> =
        compose.onAllNodes(SemanticsMatcher.keyIsDefined(SemanticsProperties.Text), useUnmergedTree = true).fetchSemanticsNodes()

    /** Every painted text's marks in document order: (marked text, current?). No mark is a span background. */
    private fun marks(m: SurfaceModel): List<Pair<String, Boolean>> {
        val match = m.color("match")
        val current = m.color("matchCurrent")
        assertNotNull("the theme has `match`", match)
        assertNotNull("the theme has `matchCurrent`", current)
        val texts: List<AnnotatedString> = textNodes().flatMap { it.config.getOrNull(SemanticsProperties.Text).orEmpty() }
        return texts.flatMap { t ->
            assertTrue("no mark paints as a span background (the line box)", t.spanStyles.none { it.item.background == match || it.item.background == current })
            t.getStringAnnotations(HIGHLIGHT_MARK_TAG, 0, t.length)
                .onEach { assertTrue("a mark is `match` or `matchCurrent`", highlightMarkFill(it.item) in setOf(match, current)) }
                .map { t.text.substring(it.start, it.end) to (highlightMarkFill(it.item) == current) }
        }
    }

    /**
     * Every mark piece's fill box per line: the font of the run's own ascent
     * and descent around the baseline (measured here apart), strictly inside
     * the line box; in [shot] the fill shows inside that box and never on the
     * line box's top or bottom row (where a line-height fill would be).
     */
    private fun assertGlyphBoxes(shot: Bitmap): Int {
        val origin = compose.onNodeWithTag("shot").fetchSemanticsNode().positionInRoot
        var checked = 0
        for (node in textNodes()) {
            val results = ArrayList<TextLayoutResult>()
            node.config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
            val layout = results.firstOrNull() ?: continue
            val input = layout.layoutInput
            val text = input.text
            val measurer = TextMeasurer(input.fontFamilyResolver, input.density, LayoutDirection.Ltr)
            val at = node.positionInRoot - origin
            for (mark in text.getStringAnnotations(HIGHLIGHT_MARK_TAG, 0, text.length)) {
                // The run's own font: the text's style with the spans over the mark.
                val runStyle = text.spanStyles.filter { it.start <= mark.start && mark.start < it.end }
                    .fold(input.style) { s, r -> s.merge(r.item) }
                    .copy(lineHeight = TextUnit.Unspecified, lineHeightStyle = null, platformStyle = PlatformTextStyle(includeFontPadding = false))
                val x = measurer.measure("x", runStyle)
                val ascent = x.firstBaseline
                val descent = x.size.height - x.firstBaseline
                val rects = highlightFillRects(layout, mark.start, mark.end, ascent, descent)
                assertTrue("\"${text.substring(mark.start, mark.end)}\" has a fill", rects.isNotEmpty())
                for (r in rects) {
                    val line = layout.getLineForVerticalPosition(r.center.y)
                    val top = layout.getLineTop(line)
                    val bottom = layout.getLineBottom(line)
                    val label = "\"${text.substring(mark.start, mark.end)}\" line $line: fill ${r.top}..${r.bottom}, line box $top..$bottom"
                    assertEquals("$label: the fill is ascent + descent tall", ascent + descent, r.height, 0.01f)
                    assertTrue("$label: inside the line box, shorter than it", r.top > top && r.bottom < bottom)
                    // Pixels: the mark's columns (its middle half) against a column just
                    // outside it (before it; after it when it starts at the text's edge).
                    val x0 = (at.x + r.left + r.width / 4f).toInt()
                    val x1 = (at.x + r.right - r.width / 4f).toInt()
                    val ref = (if (at.x + r.left >= 2f) at.x + r.left - 1f else at.x + r.right + 1f).toInt().coerceIn(0, shot.width - 1)
                    fun row(y: Float): Int = (at.y + y).toInt().coerceIn(0, shot.height - 1)
                    for (y in listOf(row(top + 0.5f), row(bottom - 0.5f))) {
                        if (y < row(r.top) || y > row(r.bottom)) {
                            assertTrue("$label: no fill on the line box's row $y", (x0..x1).all { shot.getPixel(it, y) == shot.getPixel(ref, y) })
                        }
                    }
                    val inside = row(r.top + 1.5f)
                    assertTrue("$label: the fill shows inside the glyph box (row $inside)", (x0..x1).any { shot.getPixel(it, inside) != shot.getPixel(ref, inside) })
                    checked++
                }
            }
        }
        return checked
    }

    @Test
    fun textAndRow() {
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "lg"}, "style": {"width": 360}, "children": [
            {"id": "t", "component": "Text", "props": {"text": "Résumé review: the resume link, the RESUME button and the résumé PDF.", "highlight": {"query": "resume", "current": 1}}},
            {"id": "row", "component": "Row", "props": {"title": "Printer offline after update", "subtitle": "The printer driver crashed on resume", "icon": "settings-issues", "highlight": {"query": "printer", "current": 1}}}]}""")
        val shot = capture("text-row")
        assertEquals(
            listOf("Résumé" to false, "resume" to true, "RESUME" to false, "résumé" to false, "Printer" to false, "printer" to true),
            marks(m),
        )
        assertTrue("every fill checked", assertGlyphBoxes(shot) >= 6)
    }

    @Test
    fun markdownAndBubbles() {
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "lg"}, "style": {"width": 360}, "children": [
            {"id": "md", "component": "Markdown", "props": {"text": "The **printer** queue: run `printer --reset`, then see [printer notes](https://example.com/printer).\n\n- printer driver updated\n- paper tray\n\n```\nprinter\n```", "highlight": {"query": "printer", "current": 2}}},
            {"id": "b1", "component": "MessageBubble", "props": {"text": "Is the **printer** fixed?", "side": "out", "time": "18:00", "status": "read", "highlight": {"query": "printer"}}},
            {"id": "b2", "component": "MessageBubble", "props": {"text": "Yes, the printer driver is updated. Print again.", "time": "18:02", "highlight": {"query": "printer", "current": 0}}}]}""")
        val shot = capture("markdown-bubbles")
        // The code block is never searched; the link's label is, its target never.
        assertEquals(
            listOf("printer" to false, "printer" to false, "printer" to true, "printer" to false, "printer" to false, "printer" to true),
            marks(m),
        )
        assertTrue("every fill checked", assertGlyphBoxes(shot) >= 6)
    }
}
