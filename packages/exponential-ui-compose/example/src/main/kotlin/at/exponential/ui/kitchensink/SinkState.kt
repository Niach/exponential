package at.exponential.ui.kitchensink

import android.content.Context
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import at.exponential.ui.ExponentialUi
import at.exponential.ui.ffi.benchTreeJson
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The logcat tag of every example line (passes, actions, the bench series). */
const val LOG_TAG = "ExponentialUI"

/**
 * The example's state: ONE surface model (the kitchen sink, or the bench
 * tree), the theme / mode the chrome picks, and the host's echo line. The
 * host echoes every input after 150 ms (`setData` on the bound path + the
 * `host:` line), logs every action, and maps the icon concepts.
 */
class SinkState(context: Context, val options: LaunchOptions, private val scope: CoroutineScope) {
    private val assets = context.assets

    /** The last value a host-owned field reported (the `host:` line). */
    var echo by mutableStateOf("")
        private set

    /** The picked theme id (`brand` = the third-party test theme). */
    var themeId by mutableStateOf(options.theme)
        private set

    /** The picked mode. */
    var mode by mutableStateOf(if (options.mode == "light") Mode.Light else Mode.Dark)
        private set

    /** The `specimens.json` entry the `shot` extra names (VAPP-93), else null (the kitchen sink). */
    private val specimenId: String? =
        options.shot?.takeIf { it.startsWith("exponential-ui-") && it != "exponential-ui-kitchen-sink" }

    /** The surface. */
    val model: SurfaceModel

    init {
        ExponentialUi.appContext = context.applicationContext
        val density = context.resources.displayMetrics.density
        // The surface replaces this with its own (same density, fontScale 1)
        // on first composition; a pass never runs before that, it is the
        // honest fallback for a headless pass.
        val fallback = TextShaper(
            TextMeasurer(createFontFamilyResolver(context), Density(density, 1f), LayoutDirection.Ltr),
            density,
        ) { null }
        model = SurfaceModel(
            id = if (options.bench > 0) "bench" else specimenId ?: "kitchen-sink",
            options = SurfaceOptions(
                theme = theme(themeId),
                mode = mode,
                overlays = if (options.painted) OverlayPresentation.Painted else OverlayPresentation.Native,
            ),
            host = ClosureHost(
                icons = ::sinkIcon,
                actions = { e -> Log.i(LOG_TAG, "action ${e.name} ${e.componentId} ${e.context.json} ${e.payload?.json ?: ""}") },
                inputs = { e ->
                    scope.launch {
                        delay(150)
                        e.path?.let { model.setData(it, e.value) }
                        echo = e.value.displayText
                    }
                },
            ),
            shaper = { fallback },
        )
        val specimen = specimenId?.let(::specimenNode)
        if (options.bench > 0) {
            model.setNested(benchTreeJson(options.bench.toUInt()))
        } else if (specimen != null) {
            model.setNested(specimen)
        } else {
            var json = assets.open("kitchen-sink.json").bufferedReader().use { it.readText() }
            if (options.rtl) json = json.replace("\"direction\": \"ltr\"", "\"direction\": \"rtl\"")
            model.setNested(json)
            model.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        }
    }

    /** The nested node of the specimen [id] (`fixtures/specimens.json`, copied into the assets). */
    private fun specimenNode(id: String): String? {
        val fixture = JsonValue.parse(assets.open("specimens.json").bufferedReader().use { it.readText() })
        val node = fixture["specimens"]?.array?.firstOrNull { it["id"]?.string == id }?.get("node")
        if (node == null) Log.w(LOG_TAG, "specimen $id: not in specimens.json")
        return node?.json
    }

    /** Switch the theme (a built-in id or `brand`). */
    fun pickTheme(id: String) {
        themeId = id
        theme(id)?.let(model::setTheme)
    }

    /** Switch light / dark. */
    fun pickMode(m: Mode) {
        mode = m
        model.setMode(m)
    }

    /** A built-in, or `brand`: the `theme-extends.json` acceptance case's theme. */
    fun theme(id: String): ThemeHandle? {
        if (id != "brand") return ThemeHandle.builtin(id)
        val fixture = JsonValue.parse(assets.open("theme-extends.json").bufferedReader().use { it.readText() })
        val case = fixture["cases"]?.array?.firstOrNull { it["theme"]?.get("id")?.string == "brand" } ?: return null
        return runCatching { ThemeHandle.load(case["theme"]!!.json) }
            .onFailure { Log.w(LOG_TAG, "brand theme: ${it.message}") }
            .getOrNull()
    }

    companion object {
        /** The theme ids the chrome offers: the built-ins + `brand`. */
        val themeIds: List<String> get() = ExponentialUi.builtinThemes + "brand"
    }
}
