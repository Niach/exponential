package at.exponential.ui

import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assume.assumeTrue
import org.robolectric.RuntimeEnvironment
import java.io.File

/**
 * The shared fixtures of `packages/exponential-ui/fixtures` (the directory
 * comes from the `exponential.ui.fixtures` system property the Gradle test
 * task sets; a suite skips when the checkout lacks them).
 */
object Fixtures {
    val dir: File = File(System.getProperty("exponential.ui.fixtures") ?: "../../exponential-ui/fixtures")

    fun available(): Boolean = dir.isDirectory

    /** Skip the calling test when the fixtures are missing. */
    fun require() = assumeTrue("fixtures not in this checkout ($dir)", available())

    fun json(name: String): JsonValue = JsonValue.parse(text(name))

    fun text(name: String): String = File(dir, name).readText()
}

/** `EXPONENTIAL_UI_RECORD=1` (reaches the tests as `exponential.ui.record`). */
val recording: Boolean get() = System.getProperty("exponential.ui.record") == "1"

/** A shaper that must never be called (the fixed-measure models). */
val noShaper: () -> TextShaper = { error("the shaper is never called under fixed measure") }

/**
 * A real Compose [TextShaper] (fontScale 1, Ltr; density 1 = dp are px). Needs
 * Robolectric (the font family resolver reads the Android typefaces).
 */
fun robolectricShaper(fonts: FontResolver = FontResolver { null }, density: Float = 1f): TextShaper {
    val context = RuntimeEnvironment.getApplication()
    val measurer = TextMeasurer(createFontFamilyResolver(context), Density(density, 1f), LayoutDirection.Ltr)
    return TextShaper(measurer, density, fonts)
}

/**
 * A model under a built-in theme (`null` = geometry mode) at `width` dp.
 * `fixed` = the core's fixed measure (8 px per character, 20 px lines; the
 * shaper is never called); else the real Compose shaper (Robolectric).
 * Debounces run on `scope` (default: unconfined, real time).
 */
fun makeModel(
    id: String = "t",
    theme: String? = "exponential",
    mode: Mode = Mode.Dark,
    width: Float = 390f,
    fixed: Boolean = false,
    host: HostPlugin = RecordingHost(),
    scope: CoroutineScope = CoroutineScope(Dispatchers.Unconfined),
    options: SurfaceOptions = SurfaceOptions(theme = theme?.let { ThemeHandle.builtin(it) }, mode = mode),
): SurfaceModel {
    var shaper: TextShaper? = null
    val m = SurfaceModel(id, options, host, scope) {
        check(!fixed) { "the shaper is never called under fixed measure" }
        shaper ?: robolectricShaper().also { shaper = it }
    }
    m.fixedMeasure = fixed
    m.setViewport(width, 800f)
    return m
}

/** A host that records what it receives. */
class RecordingHost : HostPlugin {
    val actions = ArrayList<SurfaceActionEvent>()
    val inputs = ArrayList<SurfaceInputEvent>()
    val urls = ArrayList<String>()
    val unknowns = ArrayList<String>()

    override fun onAction(event: SurfaceActionEvent) {
        actions.add(event)
    }

    override fun onInput(event: SurfaceInputEvent) {
        inputs.add(event)
    }

    override fun openUrl(url: String) {
        urls.add(url)
    }

    override fun onUnknown(component: String, catalogId: String?, id: String) {
        unknowns.add(component)
    }
}

/**
 * Pretty JSON with sorted keys in the layout `JSONSerialization` writes
 * (`"key" : value`, two-space indent, `/` escaped), so the Compose snapshot
 * diffs line for line against the Swift one.
 */
fun prettyJson(v: JsonValue): String = StringBuilder().also { pretty(v, it, 0) }.toString()

private fun pretty(v: JsonValue, out: StringBuilder, depth: Int) {
    val pad = "  ".repeat(depth + 1)
    val close = "  ".repeat(depth)
    when (v) {
        is JsonValue.Obj -> {
            if (v.v.isEmpty()) { out.append("{\n\n").append(close).append("}"); return }
            out.append("{\n")
            val keys = v.v.keys.sorted()
            keys.forEachIndexed { i, k ->
                out.append(pad).append(quote(k)).append(" : ")
                pretty(v.v.getValue(k), out, depth + 1)
                if (i < keys.size - 1) out.append(",")
                out.append("\n")
            }
            out.append(close).append("}")
        }
        is JsonValue.Arr -> {
            if (v.v.isEmpty()) { out.append("[\n\n").append(close).append("]"); return }
            out.append("[\n")
            v.v.forEachIndexed { i, e ->
                out.append(pad)
                pretty(e, out, depth + 1)
                if (i < v.v.size - 1) out.append(",")
                out.append("\n")
            }
            out.append(close).append("]")
        }
        is JsonValue.Str -> out.append(quote(v.v))
        else -> out.append(v.json)
    }
}

private fun quote(s: String): String = JsonValue.Str(s).json.replace("/", "\\/")
