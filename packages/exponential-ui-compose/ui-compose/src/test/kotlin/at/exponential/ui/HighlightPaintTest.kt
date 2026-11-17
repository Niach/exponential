package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.text.AnnotatedString
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
 * current one, as background spans over the painted text (the text keeps
 * its colour; frames unchanged). Pictures: build/tmp/highlight-paint/ (one png per test).
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

    private fun capture(name: String) {
        val file = File("build/tmp/highlight-paint/$name.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
    }

    /** Every painted text's marks in document order: (marked text, current?). */
    private fun marks(m: SurfaceModel): List<Pair<String, Boolean>> {
        val match = m.color("match")
        val current = m.color("matchCurrent")
        assertNotNull("the theme has `match`", match)
        assertNotNull("the theme has `matchCurrent`", current)
        val texts: List<AnnotatedString> = compose.onAllNodes(SemanticsMatcher.keyIsDefined(SemanticsProperties.Text), useUnmergedTree = true)
            .fetchSemanticsNodes().flatMap { it.config.getOrNull(SemanticsProperties.Text).orEmpty() }
        return texts.flatMap { t ->
            t.spanStyles.filter { it.item.background == match || it.item.background == current }
                .sortedBy { it.start }
                .map { t.text.substring(it.start, it.end) to (it.item.background == current) }
        }
    }

    @Test
    fun textAndRow() {
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "lg"}, "style": {"width": 360}, "children": [
            {"id": "t", "component": "Text", "props": {"text": "Résumé review: the resume link, the RESUME button and the résumé PDF.", "highlight": {"query": "resume", "current": 1}}},
            {"id": "row", "component": "Row", "props": {"title": "Printer offline after update", "subtitle": "The printer driver crashed on resume", "icon": "settings-issues", "highlight": {"query": "printer", "current": 1}}}]}""")
        capture("text-row")
        assertEquals(
            listOf("Résumé" to false, "resume" to true, "RESUME" to false, "résumé" to false, "Printer" to false, "printer" to true),
            marks(m),
        )
    }

    @Test
    fun markdownAndBubbles() {
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "lg"}, "style": {"width": 360}, "children": [
            {"id": "md", "component": "Markdown", "props": {"text": "The **printer** queue: run `printer --reset`, then see [printer notes](https://example.com/printer).\n\n- printer driver updated\n- paper tray\n\n```\nprinter\n```", "highlight": {"query": "printer", "current": 2}}},
            {"id": "b1", "component": "MessageBubble", "props": {"text": "Is the **printer** fixed?", "side": "out", "time": "18:00", "status": "read", "highlight": {"query": "printer"}}},
            {"id": "b2", "component": "MessageBubble", "props": {"text": "Yes, the printer driver is updated. Print again.", "time": "18:02", "highlight": {"query": "printer", "current": 0}}}]}""")
        capture("markdown-bubbles")
        // The code block is never searched; the link's label is, its target never.
        assertEquals(
            listOf("printer" to false, "printer" to false, "printer" to true, "printer" to false, "printer" to false, "printer" to true),
            marks(m),
        )
    }
}
