package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.setOpen
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.ThresholdValidator
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * The painted pixels: `ExponentialSurface` under Robolectric's native
 * graphics (xhdpi), one PNG per catalog COMPONENT (its first
 * `catalog-components.json` case, exponential theme, dark, 366 dp inside a
 * 12 dp page margin of a 390 dp window) and the kitchen sink at 390 dp under
 * every built-in theme (dark, painted overlays): the images the report
 * compares with the React reference. `src/test/snapshots/images/` is the
 * lock: a missing PNG is recorded, an existing one is VERIFIED (Roborazzi,
 * 0.5% changed-pixel tolerance; the diff lands under
 * `build/outputs/roborazzi`); `EXPONENTIAL_UI_RECORD=1` re-records them all.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ComponentSnapshotImagesTest {
    @get:Rule
    val compose = createComposeRule()

    private val dir = File("src/test/snapshots/images")

    /** Never touches the network: every media source fails to load (placeholders, initials). */
    private object OfflineHost : HostPlugin {
        override fun resolveUrl(src: String): String = ""
    }

    @Before
    fun setUp() = Fixtures.require()

    companion object {
        /** Root components (after macros: a Sheet is a Drawer) whose content is a layer (a Toast: the toast layer). */
        val OVERLAYS = setOf("Dialog", "Drawer", "Popover", "Tooltip", "DropdownMenu", "Toast")

        /** Components that stretch to their ROW (round 2 §7): alone they are 0 tall. */
        val ROW_STRETCHED = setOf("TreeGuides")
    }

    private fun model(id: String, theme: String, overlays: OverlayPresentation = OverlayPresentation.Native): SurfaceModel {
        val options = SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = Mode.Dark, overlays = overlays)
        // The window's density until ExponentialSurface hands the model its own shaper.
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        return SurfaceModel(id, options, OfflineHost, CoroutineScope(Dispatchers.Unconfined)) { shaper }
    }

    private fun options(file: File) = RoborazziOptions(
        taskType = if (recording || !file.exists()) RoborazziTaskType.Record else RoborazziTaskType.Verify,
        compareOptions = RoborazziOptions.CompareOptions(resultValidator = ThresholdValidator(0.005f)),
    )

    private fun capture(name: String) {
        compose.waitForIdle()
        val file = File(dir, name)
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, options(file))
        assertTrue("$file", file.exists())
    }

    @Test
    fun componentImages() {
        val cases = Fixtures.json("catalog-components.json")["cases"]?.array ?: emptyList()
        val firsts = LinkedHashMap<String, JsonValue>()
        for (c in cases) {
            val component = (c["name"]?.string ?: "?").substringBefore("/")
            if (!firsts.containsKey(component)) firsts[component] = c
        }
        assertTrue("components ${firsts.size}", firsts.size >= 55)
        var current by mutableStateOf<SurfaceModel?>(null)
        compose.setContent {
            val m = current ?: return@setContent
            key(m) {
                val bg = m.color("background") ?: Color.Black
                val overlay = m.layers.isNotEmpty()
                Box(Modifier.testTag("shot").fillMaxWidth().then(if (overlay) Modifier.heightIn(min = 444.dp) else Modifier).background(bg).padding(12.dp)) {
                    ExponentialSurface(m)
                }
            }
        }
        for ((component, c) in firsts) {
            // Overlays paint OPEN inside the surface (their layer at the core's frame over a scrim).
            val m = model("img-$component", "exponential", OverlayPresentation.Painted)
            m.setNested(c["node"]!!.json)
            m.setViewport(366f, 420f)
            val overlay = m.node(0)?.component in OVERLAYS
            if (overlay) m.setOpen(m.node(0)!!.id, true)
            current = m
            compose.waitForIdle()
            assertEquals("$component: issues ${m.issues}", 0, m.issues.size)
            assertTrue("$component laid out", m.passCount > 0)
            if (overlay) {
                assertEquals("$component: one open layer", 1, m.layers.size)
            } else if (component !in ROW_STRETCHED) {
                assertTrue("$component height ${m.surfaceSize.height}", m.surfaceSize.height > 0f)
            }
            assertEquals("$component: the surface got its width", 366f, m.width, 0.5f)
            capture("components/$component.png")
        }
    }

    private fun kitchenSink(theme: String) {
        val m = model("ks-$theme", theme, OverlayPresentation.Painted)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) {
                ExponentialSurface(m)
            }
        }
        compose.waitForIdle()
        assertEquals(0, m.issues.size)
        assertEquals(390f, m.width, 0.5f)
        assertTrue("height ${m.surfaceSize.height}", m.surfaceSize.height in 1000f..7900f)
        // The real measure batches (the views' shaper, not the fixed one).
        assertTrue("upcalls ${m.stats.upcalls}", m.stats.upcalls <= 3)
        capture("kitchen-sink-$theme.png")
    }

    @Test
    @Config(qualifiers = "w390dp-h8000dp-xhdpi")
    fun kitchenSinkExponential() = kitchenSink("exponential")

    @Test
    @Config(qualifiers = "w390dp-h8000dp-xhdpi")
    fun kitchenSinkThemes() {
        for (theme in at.exponential.ui.ffi.builtinThemeIds()) if (theme != "exponential") kitchenSinkTheme(theme)
    }

    private fun kitchenSinkTheme(theme: String) {
        // One setContent per test: the other themes swap the model in place.
        val m = model("ks-$theme", theme, OverlayPresentation.Painted)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        holder.value = m
        if (!contentSet) {
            contentSet = true
            compose.setContent {
                val cur = holder.value ?: return@setContent
                key(cur) {
                    Box(Modifier.testTag("shot").fillMaxWidth().background(cur.color("background") ?: Color.Black)) {
                        ExponentialSurface(cur)
                    }
                }
            }
        }
        compose.waitForIdle()
        assertEquals("$theme issues", 0, m.issues.size)
        assertTrue("$theme height ${m.surfaceSize.height}", m.surfaceSize.height in 1000f..7900f)
        capture("kitchen-sink-$theme.png")
    }

    private val holder = mutableStateOf<SurfaceModel?>(null)
    private var contentSet = false
}
