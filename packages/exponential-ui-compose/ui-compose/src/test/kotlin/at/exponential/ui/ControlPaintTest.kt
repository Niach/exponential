package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
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
import at.exponential.ui.model.press
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.captureRoboImage
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File
import kotlin.math.roundToInt

/**
 * Painted controls read their value: sampled pixels along a control's
 * frame, under Robolectric's native graphics.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ControlPaintTest {
    @get:Rule
    val compose = createComposeRule()

    private fun render(json: String, theme: String = "exponential", hook: ((SurfaceModel) -> Unit)? = null): SurfaceModel {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("paint", SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        hook?.invoke(m)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    /** The `shot` node's pixels (Roborazzi's Robolectric capture; `captureToImage` times out on PixelCopy here). */
    private fun capture(name: String): Bitmap {
        val file = File("build/tmp/control-paint/$name.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        return BitmapFactory.decodeFile(file.absolutePath)
    }

    /**
     * The exponential theme's Slider is the NATIVE (Material 3) slider: at
     * value 40 of 0..100 the active range (the theme's `primary`) covers the
     * track's first 40% and stops there (the frame is the core's 160 dp
     * track; the slider must be as wide as it).
     */
    @Test
    fun sliderPaintsItsValue() {
        val m = render("""{"id": "slider", "component": "Slider", "props": {"label": "Volume", "min": 0, "max": 100, "value": 40}}""")
        val track = m.frame(m.indexOf("slider.track")!!)
        val primary = m.color("primary")!!.toArgb()
        val d = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val pixels = capture("slider")
        val y = (track.center.y * d).roundToInt()
        fun at(f: Float) = pixels.getPixel(((track.left + track.width * f) * d).roundToInt().coerceIn(0, pixels.width - 1), y)
        fun near(a: Int, b: Int) = listOf(16, 8, 0).all { kotlin.math.abs((a shr it and 255) - (b shr it and 255)) <= 6 }
        fun hex(c: Int) = "#%06x".format(c and 0xffffff)
        assertTrue("the active range reaches 15% of the track (${hex(at(0.15f))}, primary ${hex(primary)})", near(at(0.15f), primary))
        assertTrue("the active range stops at the value (85% of the track is ${hex(at(0.85f))})", !near(at(0.85f), primary))
    }
    /**
     * NEW-77: a listening VoiceWave paints its waves around the ring's
     * radius on the right, nothing in the corner; TalkBack reads one image
     * named by the label with the activity.
     */
    @Test
    fun voiceWavePaintsItsRingsAndReadsAsOneImage() {
        val m = render("""{"id": "w", "component": "VoiceWave", "props": {"activity": "listening", "size": "lg", "label": "Voice"}, "children": [{"id": "w.icon", "component": "Icon", "props": {"name": "mic"}}]}""")
        val box = m.frame(m.indexOf("w")!!)
        val d = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val pixels = capture("voice-wave")
        val bg = m.color("background")!!.toArgb()
        fun near(a: Int, b: Int, tol: Int = 10) = listOf(16, 8, 0).all { kotlin.math.abs((a shr it and 255) - (b shr it and 255)) <= tol }
        fun px(x: Float, y: Float) = pixels.getPixel((x * d).roundToInt().coerceIn(0, pixels.width - 1), (y * d).roundToInt().coerceIn(0, pixels.height - 1))
        // The outer ring sits at r = 0.33 × 160 (± its amplitude) right of the centre.
        val cy = box.top + box.height / 2f
        val hit = (44..60).any { r -> !near(px(box.left + 80f + r, cy), bg) }
        assertTrue("a wave crosses the right radius", hit)
        assertTrue("the corner stays the background", near(px(box.left + 4f, box.top + 4f), bg, 2))
        val semantics = compose.onAllNodes(androidx.compose.ui.test.hasContentDescription("Voice, listening")).fetchSemanticsNodes()
        assertTrue("one image named `Voice, listening`", semantics.size == 1)
        val image = semantics.first().boundsInRoot
        assertTrue("the image is the wave's 160 dp box", kotlin.math.abs(image.width / d - 160f) < 1f)
    }

    /**
     * NEW-77 THE CHILD RULE, measured (`Round2Suites.voiceWaveChild`): each
     * fixture child case as a VoiceWave of the case's edge around a solid
     * child of the case's own size; the child's DRAWN bounds (after the
     * painter's scale about the centre) within 0.5 px of the fixture's
     * `child` box, and its colour at the centre exactly when the rule
     * paints it (never at `sm`).
     */
    @Test
    fun voiceWaveChildrenAreDrawnAtTheFixturesChildBox() {
        val cases = Round2Suites.voiceWaveChildCases()
        assertTrue("the fixture has child cases", cases.size >= 4)
        val sizes = mapOf(32f to "sm", 56f to "md", 160f to "lg", 200f to "xl")
        val waves = cases.mapIndexed { i, c ->
            val size = sizes[c.size] ?: error("no VoiceWave size is ${c.size} px")
            """{"id": "w$i", "component": "VoiceWave", "props": {"activity": "${c.activity}", "size": "$size"}, "children": [{"id": "w$i.c", "component": "Box", "style": {"width": ${c.width}, "height": ${c.height}, "backgroundColor": "${'$'}color.destructive"}}]}"""
        }
        val bounds = HashMap<String, Rect>()
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "sm"}, "children": [${waves.joinToString(",")}]}""") { m ->
            m.nodeModifier = { index ->
                val id = m.node(index)?.id
                if (id != null && id.startsWith("w")) Modifier.onGloballyPositioned {
                    // The corners through every layer (unclipped: a scale-0 child is a point).
                    bounds[id] = Rect(it.localToRoot(Offset.Zero), it.localToRoot(Offset(it.size.width.toFloat(), it.size.height.toFloat())))
                } else null
            }
        }
        val d = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val pixels = capture("voice-wave-children")
        val red = m.color("destructive")!!.toArgb()
        fun near(a: Int, b: Int) = listOf(16, 8, 0).all { kotlin.math.abs((a shr it and 255) - (b shr it and 255)) <= 6 }
        fun index(c: Round2Suites.VoiceWaveChildCase) = cases.indexOfFirst { it.name == c.name }
        val failed = Round2Suites.voiceWaveChild(
            drawn = { c ->
                val wave = bounds["w${index(c)}"]
                val child = bounds["w${index(c)}.c"]
                if (wave == null || child == null) null else Rect((child.left - wave.left) / d, (child.top - wave.top) / d, (child.right - wave.left) / d, (child.bottom - wave.top) / d)
            },
            painted = { c ->
                val f = m.frame(m.indexOf("w${index(c)}")!!)
                near(pixels.getPixel((f.center.x * d).roundToInt(), (f.center.y * d).roundToInt()), red)
            },
        ).filter { it.second != null }.map { "${it.first}: ${it.second}" }
        assertEquals(emptyList<String>(), failed)
    }

    /**
     * #EXP-1778: a `button` Select (App detail's Accounts picker) is the
     * 32 high outline trigger; pressed, its pointer menu lists the options
     * (SelectPopup: the rows `selectButtonItemGap` apart, the check on the
     * chosen row only); a `truncate: middle` Text shows start…end in its box. Pictures: build/tmp/control-paint/exp1778-*.png.
     */
    @Test
    fun selectButtonAndMiddleTruncatePaint() {
        val m = render("""{"id": "root", "component": "Stack", "props": {"gap": "lg"}, "children": [
            {"id": "row", "component": "Stack", "props": {"direction": "horizontal"}, "children": [
                {"id": "upstream", "component": "Select", "props": {"label": "github", "name": "upstream", "triggerVariant": "button", "value": "s1", "options": [{"label": "Not set up", "value": "none"}, {"label": "GitHub", "value": "s1"}, {"label": "GitHub (work account)", "value": "s2"}]}, "style": {"width": 144}}]},
            {"id": "col", "component": "Stack", "style": {"width": 220}, "children": [
                {"id": "url", "component": "Text", "props": {"text": "https://app.exponential.at/api/apps/hooks/0f6a3c1e-7d2b-4a8e-9c51-2be4d7a1f093/github-push", "truncate": "middle"}, "style": {"fontFamily": "${'$'}type.family.mono", "fontSize": 10.5}}]}]}""")
        val trigger = m.frame(m.indexOf("upstream.trigger")!!)
        assertTrue("the trigger is 32 high (${trigger.height})", kotlin.math.abs(trigger.height - 32f) < 0.5f)
        capture("exp1778-closed")
        val texts = compose.onAllNodes(androidx.compose.ui.test.hasText("https", substring = true), useUnmergedTree = true).fetchSemanticsNodes().flatMap { node -> node.config.getOrNull(SemanticsProperties.Text).orEmpty().map { t -> t.text } }
        val url = m.frame(m.indexOf("url")!!)
        assertTrue("the URL shows start…end in its ${url.width} box: $texts", texts.any { it.startsWith("https://") && it.contains("…") && it.endsWith("push") && it.length < 60 })
        m.press("upstream.trigger")
        compose.waitForIdle()
        val rows = listOf("Not set up", "GitHub (work account)").map { compose.onAllNodes(androidx.compose.ui.test.hasText(it), useUnmergedTree = true).fetchSemanticsNodes().firstOrNull()?.boundsInRoot }
        assertTrue("the menu shows its options: $rows", rows.all { it != null })
        capture("exp1778-open")
    }

}
