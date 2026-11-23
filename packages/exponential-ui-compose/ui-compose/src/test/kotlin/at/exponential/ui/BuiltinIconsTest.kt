package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import at.exponential.ui.compose.BuiltinIcons
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
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
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.roundToInt

/**
 * #VAPP-301: with NO icon registered by the host, a catalog icon name draws
 * its built-in Lucide glyph (the set web's `FALLBACK_ICONS` and the SwiftUI
 * `BuiltinIcons` cover), an unknown name the `circle-dashed` placeholder,
 * and a host icon still wins over both.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class BuiltinIconsTest {
    @get:Rule
    val compose = createComposeRule()

    /** Every name the catalog's own parts draw (`builtinIcons`) resolves to a glyph. */
    @Test
    fun everyCatalogBuiltinNameHasAGlyph() {
        val missing = ExponentialUICatalog.builtinIconNames.toSet().filter { it != "ui-icon-placeholder" && BuiltinIcons.glyph(it) == null }
        assertEquals("builtinIcons without a Compose glyph", emptyList<String>(), missing)
        assertEquals("check", BuiltinIcons.glyph("ui-check"))
        assertEquals(null, BuiltinIcons.glyph("no-such-icon"))
    }

    private fun render(name: String, host: HostPlugin): Pair<SurfaceModel, Bitmap> {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("icon", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested("""{"id": "icon", "component": "Icon", "props": {"name": "$name", "size": "lg"}}""")
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) } }
        compose.waitForIdle()
        val file = File("build/tmp/builtin-icons/$name-${host.javaClass.simpleName}.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        return m to BitmapFactory.decodeFile(file.absolutePath)
    }

    /** Is the icon's 24-unit point (x, y) painted (not the background)? */
    private fun painted(m: SurfaceModel, pixels: Bitmap, x: Float, y: Float): Boolean {
        val box = m.frame(m.indexOf("icon")!!)
        val d = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val px = ((box.left + box.width * x / 24f) * d).roundToInt().coerceIn(0, pixels.width - 1)
        val py = ((box.top + box.height * y / 24f) * d).roundToInt().coerceIn(0, pixels.height - 1)
        val bg = m.color("background")!!.toArgb()
        val c = pixels.getPixel(px, py)
        return listOf(16, 8, 0).any { kotlin.math.abs((c shr it and 255) - (bg shr it and 255)) > 24 }
    }

    @Test
    fun uiCheckDrawsTheCheckWithNoHostIcons() {
        val (m, pixels) = render("ui-check", RecordingHost())
        // Lucide `check`: M20 6 L9 17 L4 12 (its corner at 9,17); the placeholder ring never crosses it.
        assertTrue("the check's corner is painted", painted(m, pixels, 9f, 17f))
        assertTrue("the check's arm is painted", painted(m, pixels, 14.5f, 11.5f))
        assertTrue("the top of the ring is not (not the placeholder)", !painted(m, pixels, 12f, 2.2f))
    }

    @Test
    fun anUnknownNameDrawsTheDashedPlaceholder() {
        val (m, pixels) = render("no-such-icon", RecordingHost())
        // Lucide `circle-dashed`: its top dash crosses (12, 2.1); the check's corner stays empty.
        assertTrue("the top dash is painted", painted(m, pixels, 12f, 2.1f))
        assertTrue("no check", !painted(m, pixels, 9f, 17f))
    }

    private object SquareHost : HostPlugin {
        override fun icon(name: String, size: Float): (@Composable () -> Unit)? = { Box(Modifier.fillMaxSize().background(Color.Magenta)) }
    }

    @Test
    fun aHostIconWinsOverTheBuiltin() {
        val (m, pixels) = render("ui-check", SquareHost)
        assertTrue("the host's square fills the centre", painted(m, pixels, 12f, 12f))
        assertTrue("and a corner the check never reaches", painted(m, pixels, 3f, 3f))
    }
}
