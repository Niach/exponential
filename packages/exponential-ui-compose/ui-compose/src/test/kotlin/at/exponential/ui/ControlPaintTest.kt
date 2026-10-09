package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
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
import org.junit.Assert.assertTrue
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

    private fun render(json: String, theme: String = "exponential"): SurfaceModel {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("paint", SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
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
}
