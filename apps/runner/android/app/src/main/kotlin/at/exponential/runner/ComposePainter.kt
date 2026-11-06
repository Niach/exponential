package at.exponential.runner

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import at.exponential.ui.ExponentialUICatalog
import at.exponential.ui.host.A2UI_VERSION
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.FunctionDecision
import at.exponential.ui.host.FunctionPolicy
import at.exponential.ui.host.HostFunction
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.HostSurface
import at.exponential.ui.host.SourceResolver
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.exponential.app.ui.icons.ExpIcons
import kotlinx.coroutines.CoroutineScope

/**
 * The runner's PAINTER: genUI's Compose host ([ExponentialHost]) with the
 * runner's four surfaces. Every app function goes to the Engine
 * ([RunnerController.call]); `records:` sources resolve through the
 * instance; the runner's own events (`runner.*`, `consent.*`) go to
 * [RunnerController.onEvent]. Main thread.
 */
class ComposePainter(scope: CoroutineScope) : RunnerPainter {
    lateinit var controller: RunnerController
    private var packageId: String? = null

    private val plugin = object : HostPlugin {
        override fun icon(name: String, size: Float): (@Composable () -> Unit)? {
            val glyph = ExpIcons.byConcept(name) ?: ExpIcons.byName(name) ?: return null
            return { Icon(glyph, contentDescription = null, modifier = Modifier.fillMaxSize()) }
        }

        override fun onAction(event: SurfaceActionEvent) {
            controller.onEvent(event.name, event.context)
        }
    }

    /** One Engine-backed function per name the runner hands genUI (the Engine's broker is the authority, so genUI's gate allows). */
    private fun engineFunction(name: String): HostFunction = { args, _ ->
        controller.call(name, JsonValue.Obj(args)) ?: throw IllegalStateException("$name was refused")
    }

    val host = ExponentialHost(
        HostOptions(
            functions = RECORD_FUNCTIONS.associateWith(::engineFunction),
            sources = mapOf("records" to SourceResolver { source, emit -> controller.bindSource(source.uri, emit) }),
            policy = HostPolicy(functions = FunctionPolicy(default = FunctionDecision.Allow)),
            mode = Mode.Dark,
            plugin = plugin,
            navigate = { target ->
                controller.onEvent("runner.navigate", JsonValue.Obj(mapOf("screen" to JsonValue.Str(target.screen))))
                true
            },
        ),
        scope,
    )

    override fun install(packageJson: String) {
        val pkg = JsonValue.parse(packageJson)
        // The package's capability calls (`photos.pick`, …) reach the Engine like the records functions.
        pkg["functions"]?.array?.mapNotNull { it.string }
            ?.filter { !it.endsWith("*") && it !in BUILT_INS }
            ?.forEach { host.registerFunction(it, engineFunction(it)) }
        pkg["theme"]?.string?.let(ThemeHandle::builtin)?.let(host::setTheme)
        host.installPackage(packageJson)
        packageId = pkg["id"]?.string
    }

    override fun open(packageId: String, screen: String, params: Map<String, String>) {
        drop(RunnerSurfaces.APP)
        val data = JsonValue.Obj(mapOf("params" to JsonValue.Obj(params.mapValues { JsonValue.Str(it.value) })))
        send(
            "applyTemplate",
            mapOf(
                "surfaceId" to JsonValue.Str(RunnerSurfaces.APP),
                "templateId" to JsonValue.Str(screen),
                "packageId" to JsonValue.Str(packageId),
                "data" to data,
            ),
        )
    }

    override fun paint(surface: String, componentsJson: String, dataJson: String?) {
        drop(surface)
        val surfaceId = JsonValue.Str(surface)
        send("createSurface", mapOf("surfaceId" to surfaceId, "catalogId" to JsonValue.Str(ExponentialUICatalog.catalogId)))
        send("updateComponents", mapOf("surfaceId" to surfaceId, "components" to JsonValue.parse(componentsJson)))
        if (dataJson != null) send("updateDataModel", mapOf("surfaceId" to surfaceId, "path" to JsonValue.Str("/"), "value" to JsonValue.parse(dataJson)))
    }

    override fun close(surface: String) = drop(surface)

    private fun drop(surface: String) {
        if (host.surface(surface) != null) send("deleteSurface", mapOf("surfaceId" to JsonValue.Str(surface)))
    }

    private fun send(kind: String, body: Map<String, JsonValue>) {
        host.receive(JsonValue.Obj(mapOf("version" to JsonValue.Str(A2UI_VERSION), kind to JsonValue.Obj(body))))
    }

    private companion object {
        val RECORD_FUNCTIONS = listOf("records.upsert", "records.delete", "records.batch")

        /** genUI's own (bind, openUrl, set, app.navigate) and the package's actions stay genUI's. */
        val BUILT_INS = setOf("bind", "openUrl", "set", "app.navigate")
    }
}

/**
 * The window: surface `app` fills it (the shell's root is `height: 100%`),
 * the sheets and the toast are overlay surfaces above it. The background is
 * the theme's, so there is not one pixel of the runner's own.
 */
@Composable
fun RunnerWindow(painter: ComposePainter, onWidth: (Float) -> Unit) {
    val host = painter.host
    val app = host.surfaces[RunnerSurfaces.APP]
    val background = app?.color("background") ?: Color(0xFF0A0A0A)
    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(background)
            .windowInsetsPadding(WindowInsets.safeDrawing),
    ) {
        val width = maxWidth.value
        LaunchedEffect(width) { onWidth(width) }
        val density = LocalDensity.current.density
        Box(
            Modifier
                .fillMaxSize()
                // runner.json: the `app` surface is laid out at the WINDOW's height (the page
                // scrolls inside the shell, never the window); the height reaches the core
                // before the surface measures (the kitchen sink's rule).
                .layout { measurable, constraints ->
                    if (app != null && constraints.hasBoundedHeight) {
                        val h = constraints.maxHeight / density
                        if (h != app.viewportHeight) app.setViewport(app.width, h)
                    }
                    val placeable = measurable.measure(constraints)
                    layout(placeable.width, placeable.height) { placeable.place(0, 0) }
                },
        ) {
            HostSurface(host, RunnerSurfaces.APP, Modifier.fillMaxWidth())
        }
        Column(Modifier.fillMaxWidth().align(Alignment.BottomCenter)) {
            HostSurface(host, RunnerSurfaces.CONSENT, Modifier.fillMaxWidth())
            HostSurface(host, RunnerSurfaces.PERMISSIONS, Modifier.fillMaxWidth())
            HostSurface(host, RunnerSurfaces.TOAST, Modifier.fillMaxWidth())
        }
    }
}
