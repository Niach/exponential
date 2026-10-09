package at.exponential.ui.host

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.ffi.HostRouter
import at.exponential.ui.ffi.actionMessageJson
import at.exponential.ui.ffi.clientCapabilitiesJson
import at.exponential.ui.ffi.combineDecisions
import at.exponential.ui.ffi.decideFunction
import at.exponential.ui.ffi.decideUrlJson
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.ffi.errorMessageJson
import at.exponential.ui.ffi.extensionErrors
import at.exponential.ui.ffi.packagePolicyJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import java.time.Instant

// VAPP-91: the Kotlin host runtime, the twin of the TS reference
// (`packages/exponential-ui/src/host/runtime.ts`). It owns the transport,
// the core's router (through the facade), one `SurfaceModel` per surface,
// the source subscriptions, the function registry and the policy hooks;
// `HostSurface` paints one of its surfaces and the painter's interactions
// come back through the bridge plugin.

/** An extension catalog (its JSON definition) and the painters of its natives, by kind. */
class HostExtension(val json: String, val painters: Map<String, ExtensionPainter> = emptyMap()) {
    /** The catalog id. */
    val id: String = JsonValue.parse(json)["id"]?.string ?: ""
}

/** What an [ExponentialHost] is built from. Every member has a default. */
data class HostOptions(
    /** Messages in, client messages out (null = a local-only host: packages, [ExponentialHost.receive]). */
    val transport: Transport? = null,
    /** The host functions an `on.<event>` `functionCall` may run (after the policy gate). */
    val functions: Map<String, HostFunction> = emptyMap(),
    /** Binding source resolvers by scheme (`exp` → …). */
    val sources: Map<String, SourceResolver> = emptyMap(),
    /** Extension catalogs (+ their native painters), negotiated in registration order. */
    val extensions: List<HostExtension> = emptyList(),
    /** Declarative vapp packages (JSON) installed at start (`applyTemplate`). */
    val packages: List<String> = emptyList(),
    val policy: HostPolicy = HostPolicy(),
    /** The theme every surface paints with (null = a `createSurface` theme, else the default built-in). */
    val theme: ThemeHandle? = null,
    val mode: Mode = Mode.Dark,
    val overlays: OverlayPresentation = OverlayPresentation.Native,
    /** Locale, strings, density, contrast, motion, insets, the formatter: every surface's. */
    val settings: SurfaceSettings = SurfaceSettings(),
    /** Icons, fonts, markdown, input observers… (actions, functions, urls and media route through the host first). */
    val plugin: HostPlugin = NoHost,
    /** Every client message that leaves (after the transport got it). */
    val onSend: ((JsonValue) -> Unit)? = null,
    /** Every op the host performs (tests, logging). */
    val onOp: ((JsonValue) -> Unit)? = null,
)

/** The A2UI error code of a failed painter (`catalog/host.json` `paint.errorCode`). */
const val RENDER_FAILED = "RENDER_FAILED"

/**
 * The host runtime: transport → the core's router → ops performed on one
 * [SurfaceModel] per surface; painter events → client messages, gated host
 * functions, policed urls. Its state ([surfaces], [status],
 * [unsupportedCatalog]) is Compose snapshot state. Main thread (or the
 * given [scope]'s): transport and source callbacks hop onto [scope].
 */
class ExponentialHost(
    val options: HostOptions = HostOptions(),
    /** Where transport / source callbacks, function calls and sends run. */
    val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
) {
    private val extensions = ArrayList<HostExtension>()
    private val functions = HashMap(options.functions)
    private val sources = HashMap<String, SourceResolver>().apply { options.sources.forEach { (k, v) -> put(k.lowercase(), v) } }
    private val packages = HashMap<String, JsonValue>()
    private val packageOf = HashMap<String, String>()
    private val components = HashMap<String, LinkedHashMap<String, JsonValue>>()
    private val subscriptions = HashMap<String, MutableList<() -> Unit>>()
    private val paintErrors = HashSet<PaintError>()
    private val bridge = Bridge()

    /** The core's router (one per connection). */
    val router: HostRouter

    /** The live surfaces by id (snapshot state: [HostSurface] recomposes as they come and go). */
    val surfaces = mutableStateMapOf<String, SurfaceModel>()

    /** The transport's state (`host_offline` when not [TransportStatus.Open]). */
    var status: TransportStatus by mutableStateOf(TransportStatus.Closed)
        private set

    /** The transport's last status detail (an error message). */
    var statusDetail: String? by mutableStateOf(null)
        private set

    /** The last UNSUPPORTED_CATALOG id (the catalog-update banner). */
    var unsupportedCatalog: String? by mutableStateOf(null)
        private set

    private var themeState: ThemeHandle? by mutableStateOf(options.theme)
    private var modeState: Mode by mutableStateOf(options.mode)

    /** The theme new surfaces paint with (null = a `createSurface` theme, else the default built-in). */
    val theme: ThemeHandle? get() = themeState

    /** The mode every surface paints in. */
    val mode: Mode get() = modeState

    init {
        for (e in options.extensions) require(extensionErrors(e.json).isEmpty()) { "extension ${e.id}: ${extensionErrors(e.json)}" }
        extensions.addAll(options.extensions.distinctBy { it.id })
        router = HostRouter(extensions.map { it.id })
        for (p in options.packages) installPackage(p)
    }

    /** False for a local-only host (no `host_offline` state to show). */
    val hasTransport: Boolean get() = options.transport != null

    // Negotiation + registration

    /** The core, the core lite, the basic catalog, then the extension ids. */
    val supportedCatalogIds: List<String> get() = router.supportedCatalogIds()

    /** A2UI `a2uiClientCapabilities`. */
    fun clientCapabilities(): JsonValue = JsonValue.parse(clientCapabilitiesJson(extensions.map { it.id }))

    /** The registered extensions. */
    val extensionDefs: List<HostExtension> get() = extensions.toList()

    /** Add an extension (new surfaces know it). Throws [IllegalArgumentException] when invalid. */
    fun registerExtension(extension: HostExtension) {
        if (extensions.any { it.id == extension.id }) return
        val errors = extensionErrors(extension.json)
        require(errors.isEmpty()) { "extension ${extension.id}: $errors" }
        extensions.add(extension)
        router.registerExtension(extension.id)
    }

    fun registerFunction(name: String, fn: HostFunction) {
        functions[name] = fn
    }

    fun registerSource(scheme: String, resolver: SourceResolver) {
        sources[scheme.lowercase()] = resolver
    }

    /** Install a declarative package (JSON); its issues `[{path, message}]` (installed only when empty). */
    fun installPackage(packageJson: String): List<JsonValue> {
        val issues = JsonValue.parse(router.installPackage(packageJson)).array ?: emptyList()
        if (issues.isEmpty()) {
            val pkg = JsonValue.parse(packageJson)
            pkg["id"]?.string?.let { packages[it] = pkg }
        }
        return issues
    }

    // Surfaces

    /** The surface `id`. */
    fun surface(id: String): SurfaceModel? = surfaces[id]

    /** The live surface ids. */
    fun surfaceIds(): List<String> = surfaces.keys.toList()

    /** The package whose template created `surfaceId`. */
    fun packageIdOf(surfaceId: String): String? = packageOf[surfaceId]

    /** Switch every surface's theme (and new ones'). */
    fun setTheme(theme: ThemeHandle) {
        themeState = theme
        for (m in surfaces.values) m.setTheme(theme)
    }

    /** Switch every surface's mode (and new ones'). */
    fun setMode(mode: Mode) {
        modeState = mode
        for (m in surfaces.values) m.setMode(mode)
    }

    // Transport

    private fun post(block: () -> Unit) {
        scope.launch { block() }
    }

    /** Start the transport. */
    fun connect() {
        val t = options.transport ?: return
        t.start(
            { m -> post { receive(m) } },
            { s, detail ->
                post {
                    status = s
                    statusDetail = detail
                }
            },
        )
    }

    /** Close the transport and drop every surface (and its subscriptions). */
    fun close() {
        options.transport?.close()
        for (id in surfaceIds()) perform(JsonValue.Obj(mapOf("op" to JsonValue.Str("delete"), "surfaceId" to JsonValue.Str(id))))
        status = TransportStatus.Closed
    }

    /** Close and stop the scope (a host that will not be reused). */
    fun dispose() {
        close()
        scope.cancel()
    }

    /** One server message (JSON text); the ops performed. */
    fun receive(json: String): List<JsonValue> = perform(JsonValue.parse(router.route(json)).array ?: emptyList())

    /** One server message; the ops performed. On the host's thread. */
    fun receive(message: JsonValue): List<JsonValue> = receive(message.json)

    private fun perform(ops: List<JsonValue>): List<JsonValue> {
        for (op in ops) perform(op)
        return ops
    }

    /** A client message out (the transport, then [HostOptions.onSend]). */
    fun send(message: JsonValue) {
        options.transport?.let { t ->
            scope.launch {
                try {
                    t.send(message)
                } catch (e: Exception) {
                    statusDetail = e.message ?: e.toString()
                }
            }
        }
        options.onSend?.invoke(message)
    }

    private fun sendError(code: String, surfaceId: String, message: String, path: String? = null) =
        send(JsonValue.parse(errorMessageJson(code, surfaceId, message, path)))

    private fun perform(op: JsonValue) {
        options.onOp?.invoke(op)
        val sid = op["surfaceId"]?.string ?: ""
        when (op["op"]?.string) {
            "create" -> {
                unbind(sid)
                forgetPaintErrors(sid)
                surfaces.remove(sid)
                val opTheme = op["theme"]?.let { t ->
                    t.string?.let { ThemeHandle.builtin(it) } ?: runCatching { ThemeHandle.load(t.json) }.getOrNull()
                }
                val model = SurfaceModel(
                    sid,
                    SurfaceOptions(
                        catalogId = op["catalogId"]?.string ?: at.exponential.ui.ffi.coreCatalogId(),
                        theme = theme ?: opTheme ?: ThemeHandle.builtin(defaultThemeId()),
                        mode = mode,
                        overlays = options.overlays,
                        settings = options.settings,
                    ),
                    bridge,
                    scope,
                ) { error("exponential-ui: the shaper is set by ExponentialSurface") }
                for (e in extensions) runCatching { model.register(e.json, e.painters) }
                components[sid] = LinkedHashMap()
                val pkgId = router.packageIdOf(sid)
                if (pkgId != null) packageOf[sid] = pkgId else packageOf.remove(sid)
                surfaces[sid] = model
            }
            "components" -> {
                forgetPaintErrors(sid)
                val model = surfaces[sid] ?: return
                val byId = components.getOrPut(sid) { LinkedHashMap() }
                for (c in op["components"]?.array ?: emptyList()) byId[c["id"]?.string ?: continue] = c
                runCatching { model.setComponents(JsonValue.Arr(byId.values.toList()).json) }
            }
            "data" -> surfaces[sid]?.setData(op["path"]?.string ?: "", op["value"])
            "bind" -> {
                val path = op["path"]?.string ?: ""
                val source = ParsedSource.parse(op["source"]?.string ?: "")
                if (surfaces[sid] == null || source == null) return
                val resolver = sources[source.scheme]
                if (resolver == null) {
                    sendError("VALIDATION_FAILED", sid, "no resolver for the source scheme ${source.scheme}", path.ifEmpty { "/" })
                    return
                }
                val cancel = resolver.subscribe(source) { v -> post { surfaces[sid]?.setData(path, v) } }
                if (cancel != null) subscriptions.getOrPut(sid) { ArrayList() }.add(cancel)
            }
            "delete" -> {
                unbind(sid)
                forgetPaintErrors(sid)
                surfaces.remove(sid)
                components.remove(sid)
                packageOf.remove(sid)
            }
            "send" -> {
                val message = op["message"] ?: return
                val err = message["error"]
                if (err?.get("code")?.string == "UNSUPPORTED_CATALOG") {
                    val text = err["message"]?.string ?: ""
                    unsupportedCatalog = Regex("^catalog (\\S+).*$").find(text)?.groupValues?.get(1) ?: text
                }
                send(message)
            }
        }
    }

    private fun unbind(surfaceId: String) {
        subscriptions.remove(surfaceId)?.forEach { runCatching { it() } }
    }

    // Interactions out

    /** A component's server event: the A2UI client action message, sent. */
    fun action(event: SurfaceActionEvent, timestamp: String = Instant.now().toString()): JsonValue {
        val message = JsonValue.parse(
            actionMessageJson(event.surfaceId, event.componentId, event.name, event.context.json, event.payload?.json, timestamp),
        )
        send(message)
        return message
    }

    /** The policy decision for a call, before any consent hook (the package's allowlist narrows it). */
    fun decide(surfaceId: String, name: String): FunctionDecision {
        val registered = functions.containsKey(name)
        var decision = decideFunction(options.policy.functions?.json, name, registered)
        val pkg = packageOf[surfaceId]?.let { packages[it] }
        if (pkg != null) decision = combineDecisions(decision, decideFunction(packagePolicyJson(pkg["functions"]?.json), name, registered))
        return FunctionDecision.of(decision)
    }

    /** An action `functionCall` to a host function: gate, consent, run. A built-in other than `openUrl` has no effect as an action. */
    suspend fun callFunction(call: FunctionCallInfo): FunctionOutcome {
        if (call.name == "openUrl") {
            val url = call.args["url"]?.string ?: ""
            return FunctionOutcome(if (openUrl(url)) FunctionDecision.Allow else FunctionDecision.Deny)
        }
        var decision = decide(call.surfaceId, call.name)
        if (decision == FunctionDecision.NotFound) {
            sendError("FUNCTION_NOT_FOUND", call.surfaceId, "no function ${call.name}")
            return FunctionOutcome(decision)
        }
        if (decision == FunctionDecision.Ask) {
            val ok = try {
                options.policy.onFunctionCall?.invoke(call) == true
            } catch (_: Exception) {
                false
            }
            if (!ok) decision = FunctionDecision.Deny
        }
        if (decision == FunctionDecision.Deny) {
            sendError("FUNCTION_DENIED", call.surfaceId, "${call.name} was not allowed")
            return FunctionOutcome(decision)
        }
        val fn = functions[call.name] ?: return FunctionOutcome(FunctionDecision.Allow)
        return try {
            FunctionOutcome(FunctionDecision.Allow, result = fn(call.args, call))
        } catch (e: Exception) {
            FunctionOutcome(FunctionDecision.Allow, error = e.message ?: e.toString())
        }
    }

    /** The URL policy every href passes (relative urls against the urls' or the media `baseUrl`). */
    val urlPolicy: UrlPolicy
        get() {
            val urls = options.policy.urls ?: options.plugin.urlPolicy ?: UrlPolicy()
            return urls.copy(baseUrl = urls.baseUrl ?: mediaOptions?.baseUrl)
        }

    /** openUrl / Link through the URL policy. True when it opened. */
    fun openUrl(url: String): Boolean {
        if (url.isEmpty()) return false
        val d = JsonValue.parse(decideUrlJson(urlPolicy.json(null), url))
        val resolved = d["url"]?.string
        if (d["allowed"]?.bool != true || resolved == null) return false
        val open = options.policy.openUrl ?: { u: String -> options.plugin.openUrl(u) }
        open(resolved)
        return true
    }

    /**
     * The media loader's request for `src` (absolute url + auth headers):
     * the plugin's `resolveUrl` rewrite, then the media policy (defaults
     * without one). null = denied or unresolvable: nothing loads.
     */
    fun mediaRequest(src: String): MediaRequest? = policedMediaRequest(options.plugin.resolveUrl(src), mediaOptions)

    /** The media policy (the host policy's, else the plugin's; null = the contract defaults). */
    val mediaOptions: MediaOptions? get() = options.policy.media ?: options.plugin.mediaOptions

    /**
     * `onPaintError` (catalog/host.json `paint`): a component's painter
     * failed. Forwarded ONCE per surface + component + message as an A2UI
     * `RENDER_FAILED` error at `/components/<componentId>` (the host issue).
     */
    fun paintError(error: PaintError) {
        if (!paintErrors.add(error)) return
        sendError(RENDER_FAILED, error.surfaceId, error.message, "/components/${error.componentId}")
    }

    private fun forgetPaintErrors(surfaceId: String) {
        paintErrors.removeAll { it.surfaceId == surfaceId }
    }

    /** What the painter's surfaces call: actions, functions, urls and media go through the host, the rest to [HostOptions.plugin]. */
    private inner class Bridge : HostPlugin {
        private val base: HostPlugin get() = options.plugin

        override fun icon(name: String, size: Float): (@Composable () -> Unit)? = base.icon(name, size)

        override fun onAction(event: SurfaceActionEvent) {
            action(event)
            base.onAction(event)
        }

        override fun onInput(event: SurfaceInputEvent) = base.onInput(event)

        override fun onFunctionCall(event: SurfaceFunctionCallEvent) {
            val call = FunctionCallInfo(event.surfaceId, event.componentId, event.name, event.args)
            scope.launch { callFunction(call) }
            base.onFunctionCall(event)
        }

        override val urlPolicy: UrlPolicy get() = this@ExponentialHost.urlPolicy

        override val mediaOptions: MediaOptions? get() = this@ExponentialHost.mediaOptions

        override fun openUrl(url: String) {
            this@ExponentialHost.openUrl(url)
        }

        override fun resolveUrl(src: String): String = base.resolveUrl(src)

        override fun onPaintError(error: PaintError) {
            paintError(error)
            base.onPaintError(error)
        }

        override fun onUnknown(component: String, catalogId: String?, id: String) = base.onUnknown(component, catalogId, id)

        override fun fontFamily(name: String): FontFamily? = base.fontFamily(name)

        override fun markdown(text: String, width: Float): (@Composable () -> Unit)? = base.markdown(text, width)

        override fun copy(text: String) = base.copy(text)

        override fun pickFiles(request: FilePickRequest) = base.pickFiles(request)
    }
}

/**
 * A surface the host's transport feeds: [fallback] while the host has no
 * surface `surfaceId`, then the painted surface (as wide as its container,
 * as tall as its content; wrap it in your scroller).
 */
@Composable
fun HostSurface(host: ExponentialHost, surfaceId: String, modifier: Modifier = Modifier, fallback: @Composable () -> Unit = {}) {
    val model = host.surfaces[surfaceId]
    if (model == null) {
        fallback()
    } else {
        key(model) { ExponentialSurface(model, modifier) }
    }
}
