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
import at.exponential.ui.ffi.HostCalls
import at.exponential.ui.ffi.HostRouter
import at.exponential.ui.ffi.HostZone
import at.exponential.ui.ffi.actionMessageJson
import at.exponential.ui.ffi.rendererCapabilitiesJson
import at.exponential.ui.ffi.combineDecisions
import at.exponential.ui.ffi.decideFunction
import at.exponential.ui.ffi.decideUrlJson
import at.exponential.ui.ffi.defaultThemeId
import at.exponential.ui.ffi.errorMessageJson
import at.exponential.ui.ffi.extensionErrors
import at.exponential.ui.ffi.packagePolicyJson
import at.exponential.ui.ffi.upgrade09Json
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
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
    /** Declarative app packages (JSON) installed at start (`applyTemplate`). */
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
    /** VAPP-194: the member a package action runs as (its `me`). */
    val me: String? = null,
    /** VAPP-194: the clock (epoch ms) a package action reads (null = the wall clock). */
    val now: (() -> Double)? = null,
    /** VAPP-194: the team zone a package action reads (null = UTC). */
    val zone: HostZone? = null,
    /** VAPP-194: a package action that landed nothing (a false `require`'s message, a refused write or host call): the host's failed-action toast. */
    val onActionFailed: ((ActionFailure) -> Unit)? = null,
    /** VAPP-218 (U1): open one of the surface's package screens (`app_navigate`, checked by [Screens.target]); the host's shell opens it. Null = no shell, so a navigate is refused. True when it navigated. */
    val navigate: ((NavigateTarget) -> Boolean)? = null,
    /** Round 9 §6.5: the transport's far end reads renderer → agent messages: a surface an untrusted `createSurface` made has an agent. */
    val agent: Boolean = false,
)

/** The A2UI protocol version the host's own messages carry (the catalog's). */
val A2UI_VERSION: String = at.exponential.ui.ExponentialUICatalog.a2uiVersion

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
    /** Per surface, one cancel per bound path (VAPP-122: a re-bind of a path cancels the earlier subscription at that path). */
    private val subscriptions = HashMap<String, MutableList<Pair<String, () -> Unit>>>()
    /** Reported paint failures: surfaceId → component ids. */
    private val paintErrors = HashMap<String, MutableSet<String>>()
    private val bridge = Bridge()
    /** Round 9 §6.5: the routed calls (the core's `CallHub`). */
    private val calls = HostCalls(options.agent)
    /** The press waiting for each action call to the agent. */
    private val agentWaiters = HashMap<String, CompletableDeferred<JsonValue>>()
    private var flushingCalls = false
    /** The one timer, armed at `calls.nextDeadlineMs()`. */
    private var deadlineTimer: Job? = null
    private var armedDeadline: Double? = null
    private val sendDataModel = HashSet<String>()

    /** The plugin every surface of this host sees (the policy's hooks over [HostOptions.plugin]; tests). */
    internal val surfaceHost: HostPlugin get() = bridge

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
        calls.setRegistered(functions.keys.toList())
        calls.setPolicyJson(options.policy.functions?.json)
        for (p in options.packages) installPackage(p)
    }

    /** False for a local-only host (no `host_offline` state to show). */
    val hasTransport: Boolean get() = options.transport != null

    // Negotiation + registration

    /** The core, the core lite, the basic catalog, then the extension ids. */
    val supportedCatalogIds: List<String> get() = router.supportedCatalogIds()

    /** A2UI `rendererCapabilities`. */
    fun rendererCapabilities(): JsonValue = JsonValue.parse(rendererCapabilitiesJson(extensions.map { it.id }, null))

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
        calls.setRegistered(functions.keys.toList())
    }

    fun registerSource(scheme: String, resolver: SourceResolver) {
        sources[scheme.lowercase()] = resolver
    }

    /** Install a declarative package (JSON); its issues `[{path, message}]` (installed only when empty). */
    fun installPackage(packageJson: String): List<JsonValue> {
        val issues = JsonValue.parse(router.installPackage(packageJson)).array ?: emptyList()
        if (issues.isEmpty()) {
            var pkg = JsonValue.parse(packageJson)
            // Round 9 §12: the router installed a format-5 package UPGRADED (flat
            // function names): the policy gate, the actions and navigation read
            // that copy too, or `records.*` never matches `records_upsert`.
            if (pkg["formatVersion"]?.number == 5.0) {
                val request = JsonValue.Obj(mapOf("kind" to JsonValue.Str("package"), "input" to pkg)).json
                runCatching { JsonValue.parse(upgrade09Json(request))["package"] }.getOrNull()?.takeUnless { it is JsonValue.Null }?.let { pkg = it }
            }
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
        val ops = surfaceIds().map { JsonValue.Obj(mapOf("op" to JsonValue.Str("delete"), "surfaceId" to JsonValue.Str(it))) }
        perform(ops)
        observe(JsonValue.Arr(ops).json)
        status = TransportStatus.Closed
    }

    /** Close and stop the scope (a host that will not be reused). */
    fun dispose() {
        close()
        agentWaiters.values.forEach { it.cancel() }
        agentWaiters.clear()
        scope.cancel()
    }

    /**
     * One agent message (JSON text); the ops performed. Round 9 §2: a transport
     * is untrusted; only host code passes `trusted = true` (`applyTemplate`,
     * `bindDataModel`).
     */
    fun receive(json: String, trusted: Boolean = false): List<JsonValue> {
        // VAPP-206: `${today}`, `${now}`, `${month}` at this bind, in the team zone.
        val opsJson = router.routeJson(HostParams.adding(json, options.now?.invoke(), options.zone), trusted)
        val ops = perform(JsonValue.parse(opsJson).array ?: emptyList())
        observe(opsJson)
        return ops
    }

    /** One server message; the ops performed. On the host's thread. */
    fun receive(message: JsonValue, trusted: Boolean = false): List<JsonValue> = receive(message.json, trusted)

    /**
     * Round 9 §1.4: a fresh id to re-open a surface under: mint it, apply the
     * template onto it, repaint with the new id, then `deleteSurface` the old
     * one, so the old surface paints until the new one exists.
     */
    fun mintSurfaceId(base: String): String = router.mintSurfaceId(base)

    private fun perform(ops: List<JsonValue>): List<JsonValue> {
        for (op in ops) perform(op)
        return ops
    }

    /** A client message out (the transport, then [HostOptions.onSend]). */
    fun send(message: JsonValue) {
        options.transport?.let { t ->
            val metadata = dataModelMetadata(message)
            scope.launch {
                try {
                    if (metadata != null) t.send(message, metadata) else t.send(message)
                } catch (e: Exception) {
                    statusDetail = e.message ?: e.toString()
                }
            }
        }
        options.onSend?.invoke(message)
    }

    /** Round 9 §1.3: the metadata beside a message about a `sendDataModel` surface. */
    private fun dataModelMetadata(message: JsonValue): JsonValue? {
        if (sendDataModel.isEmpty()) return null
        val id = listOf("action", "callAgentFunction", "error").firstNotNullOfOrNull { message[it]?.get("surfaceId")?.string } ?: return null
        val model = surfaces[id]?.takeIf { id in sendDataModel } ?: return null
        return calls.dataModelMetadataJson(message.json, model.surface.dataJson())?.let(JsonValue::parse)
    }

    // Routed calls (round 9 §6.5–§6.8, §18.3)

    /** Every router answer, AFTER its ops ran: the hub follows the surfaces and settles the `respond` ops, then the one flush. */
    private fun observe(opsJson: String) {
        answerPresses(calls.observeJson(opsJson, router))
        flushCalls()
    }

    /** `[SettledCall]` JSON: the presses waiting for those calls go on. */
    private fun answerPresses(settledJson: String) {
        for (call in JsonValue.parse(settledJson).array ?: emptyList()) {
            agentWaiters.remove(call["functionCallId"]?.string)?.complete(call)
        }
    }

    /**
     * What the hub queued: messages out, reactive effects run, surfaces whose
     * routed value settled bound again, until all three are empty; then the
     * timer. A flush a rebind's layout asks for is this loop's next round.
     */
    private fun flushCalls() {
        if (flushingCalls) return
        flushingCalls = true
        try {
            while (true) {
                val outbox = JsonValue.parse(calls.takeOutboxJson()).array ?: emptyList()
                val effects = JsonValue.parse(calls.takeEffectsJson()).array ?: emptyList()
                val dirty = calls.takeDirty()
                if (outbox.isEmpty() && effects.isEmpty() && dirty.isEmpty()) break
                outbox.forEach(::send)
                effects.forEach(::runEffect)
                dirty.forEach { surfaces[it]?.rebind() }
            }
        } finally {
            flushingCalls = false
        }
        armDeadline()
    }

    private fun runEffect(effect: JsonValue) {
        val ticket = effect["ticket"]?.number?.toULong() ?: return
        val call = FunctionCallInfo(effect["surfaceId"]?.string ?: "", "", effect["name"]?.string ?: "", effect["args"]?.obj ?: emptyMap())
        when (effect["effect"]?.string) {
            "ask" -> scope.launch {
                val allowed = try {
                    options.policy.onFunctionCall?.invoke(call) == true
                } catch (e: Exception) {
                    if (e is kotlinx.coroutines.CancellationException) throw e
                    false
                }
                calls.consent(ticket, allowed)
                flushCalls()
            }
            "runLocal" -> scope.launch {
                var value: String? = null
                var failure: String? = null
                try {
                    value = functions[call.name]?.invoke(call.args, call)?.let { JsonValue.of(it).json }
                } catch (e: Exception) {
                    if (e is kotlinx.coroutines.CancellationException) throw e
                    failure = e.message ?: e.toString()
                }
                calls.settleLocal(ticket, value, failure)
                flushCalls()
            }
        }
    }

    private fun armDeadline() {
        val next = calls.nextDeadlineMs()
        if (next == armedDeadline) return
        deadlineTimer?.cancel()
        deadlineTimer = null
        armedDeadline = next
        if (next == null) return
        deadlineTimer = scope.launch {
            delay((next - System.currentTimeMillis()).toLong().coerceAtLeast(0) + 1)
            deadlineTimer = null
            armedDeadline = null
            answerPresses(calls.expireJson())
            flushCalls()
        }
    }

    /**
     * Round 9 §5.2, §6.8: a layout may leave issues, failures or routed calls:
     * to the hub, then out.
     */
    private fun afterLayout(surfaceId: String) {
        val model = surfaces[surfaceId] ?: return
        if (!model.surface.needsFlush()) return
        calls.flushSurface(surfaceId, model.surface)
        flushCalls()
    }

    /** An action call to the agent: the policy, the consent, one new `callAgentFunction`, its answer or the timeout (§6.5: never shared). */
    private suspend fun callAgent(call: FunctionCallInfo): FunctionOutcome {
        val args = JsonValue.Obj(call.args).json
        fun press(consent: Boolean?) = JsonValue.parse(calls.pressJson(call.surfaceId, call.name, null, args, consent))
        var route = press(null)
        if (route["route"]?.string == "ask") {
            val allowed = try {
                options.policy.onFunctionCall?.invoke(call) == true
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                false
            }
            route = press(allowed)
        }
        val id = route["functionCallId"]?.string
        return when (route["route"]?.string) {
            "agent" -> {
                val waiter = CompletableDeferred<JsonValue>()
                agentWaiters[id ?: ""] = waiter
                flushCalls()
                val answer = waiter.await()
                FunctionOutcome(FunctionDecision.Allow, result = answer["value"], error = answer["error"]?.string)
            }
            "notFound" -> {
                flushCalls()
                FunctionOutcome(FunctionDecision.NotFound)
            }
            "local" -> {
                flushCalls()
                runLocal(call)
            }
            else -> {
                flushCalls()
                FunctionOutcome(FunctionDecision.Deny)
            }
        }
    }

    /** [run]; §6.8: a local function that threw is an EXECUTION_ERROR. */
    private suspend fun runLocal(call: FunctionCallInfo): FunctionOutcome {
        val outcome = run(call)
        outcome.error?.let {
            calls.localFailed(call.surfaceId, it)
            flushCalls()
        }
        return outcome
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
                // Round 9 §8: an unusable `at_exponential_theme` never fails the
                // host: the surface paints with the host's theme and the
                // server hears VALIDATION_FAILED (the TS / gpui hosts' rule).
                val opTheme = op["theme"]?.takeIf { it != JsonValue.Null }?.let { t ->
                    val named = t.string
                    if (named != null) {
                        ThemeHandle.builtin(named) ?: null.also { sendError("VALIDATION_FAILED", sid, "createSurface.metadata.extensions.at_exponential_theme is unusable: unknown built-in theme \"$named\"", "/createSurface/metadata/extensions/at_exponential_theme") }
                    } else {
                        var refused: List<at.exponential.ui.theme.ThemeIssue> = emptyList()
                        val loaded = ThemeHandle.loadOrDefault(t.json) { refused = it }
                        if (refused.isEmpty()) loaded else null.also { sendError("VALIDATION_FAILED", sid, "createSurface.metadata.extensions.at_exponential_theme is unusable: ${refused.joinToString("; ") { i -> "${i.path}: ${i.message}" }}", "/createSurface/metadata/extensions/at_exponential_theme") }
                    }
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
                if (op["sendDataModel"]?.bool == true) sendDataModel.add(sid) else sendDataModel.remove(sid)
                surfaces[sid] = model
                calls.attachSurface(sid, model.surface)
            }
            "components" -> {
                val model = surfaces[sid] ?: return
                val byId = components.getOrPut(sid) { LinkedHashMap() }
                for (c in op["components"]?.array ?: emptyList()) {
                    val cid = c["id"]?.string ?: continue
                    byId[cid] = c
                    paintErrors[sid]?.remove(cid)
                }
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
                val earlier = subscriptions.getOrPut(sid) { ArrayList() }
                earlier.filter { it.first == path }.forEach { runCatching { it.second() } }
                earlier.removeAll { it.first == path }
                val cancel = resolver.subscribe(source) { v -> post { surfaces[sid]?.setData(path, v) } }
                if (cancel != null) earlier.add(path to cancel)
            }
            "delete" -> {
                unbind(sid)
                forgetPaintErrors(sid)
                surfaces.remove(sid)
                components.remove(sid)
                packageOf.remove(sid)
                sendDataModel.remove(sid)
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
        subscriptions.remove(surfaceId)?.forEach { runCatching { it.second() } }
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

    /** An action `functionCall` to a host function: gate, consent, run; `actions_<name>` = the surface's package action (VAPP-194). A built-in other than `openUrl` and `bind` has no effect as an action. */
    suspend fun callFunction(call: FunctionCallInfo): FunctionOutcome {
        runBuiltIn(call)?.let { return it }
        if (call.name == Screens.NAVIGATE_FUNCTION) return navigate(call)
        Actions.actionName(call.name)?.let { return runPackageAction(call, it) }
        // Round 9 §6.5: nothing local + an agent: the hub routes it (a built-in
        // answers `local`). No agent: the gate answers UNKNOWN_FUNCTION.
        if (call.name !in functions && calls.hasAgent(call.surfaceId)) return callAgent(call)
        val verdict = gate(call)
        if (verdict.decision != FunctionDecision.Allow) return verdict
        return runLocal(call)
    }

    /** VAPP-218: `app.navigate {screen, params?}` → the host's shell, when the surface's package has that screen. A refusal = INVALID_FUNCTION_CALL. */
    fun navigate(call: FunctionCallInfo): FunctionOutcome {
        fun refuse(message: String): FunctionOutcome {
            sendError("INVALID_FUNCTION_CALL", call.surfaceId, message)
            return FunctionOutcome(FunctionDecision.Deny, error = message)
        }
        val pkgId = packageOf[call.surfaceId] ?: return refuse("navigate: the surface has no package")
        val pkg = packages[pkgId] ?: return refuse("navigate: the surface has no package")
        val (screen, params) = Screens.target(pkg, call.args).getOrElse { return refuse(it.message ?: "navigate takes {screen, params?}") }
        val open = options.navigate ?: return refuse("navigate: this host has no screens")
        return if (open(NavigateTarget(call.surfaceId, pkgId, screen, params))) FunctionOutcome(FunctionDecision.Allow) else refuse("navigate: $screen was not opened")
    }

    /** The policy gate and the consent hook: `Allow`, or the refusal (sent as UNKNOWN_FUNCTION / INVALID_FUNCTION_CALL). */
    private suspend fun gate(call: FunctionCallInfo): FunctionOutcome {
        // VAPP-218: app_navigate is always allowed (checked when it runs).
        if (call.name == Screens.NAVIGATE_FUNCTION) return FunctionOutcome(FunctionDecision.Allow)
        var decision = decide(call.surfaceId, call.name)
        if (decision == FunctionDecision.NotFound) {
            sendError("UNKNOWN_FUNCTION", call.surfaceId, "no function ${call.name}")
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
            sendError("INVALID_FUNCTION_CALL", call.surfaceId, "${call.name} was not allowed")
            return FunctionOutcome(decision)
        }
        return FunctionOutcome(FunctionDecision.Allow)
    }

    /** VAPP-205: `openUrl` and `bind`, for a press and an action's `call` step alike. Null for any other name. */
    private fun runBuiltIn(call: FunctionCallInfo): FunctionOutcome? {
        if (call.name == "openUrl") {
            val url = call.args["url"]?.string ?: ""
            return FunctionOutcome(if (openUrl(url)) FunctionDecision.Allow else FunctionDecision.Deny)
        }
        if (call.name == "bind") return FunctionOutcome(if (bind(call.surfaceId, call.args)) FunctionDecision.Allow else FunctionDecision.Deny)
        return null
    }

    private suspend fun run(call: FunctionCallInfo): FunctionOutcome {
        runBuiltIn(call)?.let { return it }
        if (call.name == Screens.NAVIGATE_FUNCTION) return navigate(call)
        val fn = functions[call.name] ?: return FunctionOutcome(FunctionDecision.Allow)
        return try {
            FunctionOutcome(FunctionDecision.Allow, result = fn(call.args, call))
        } catch (e: Exception) {
            FunctionOutcome(FunctionDecision.Allow, error = e.message ?: e.toString())
        }
    }

    /**
     * VAPP-194: `actions_<name>` = the surface's package action, all or
     * nothing: the plan over one snapshot (the args = params, the surface
     * data, `me`, the clock), every host call's gate and consent, then the
     * record writes as ONE `records_batch`, then the sets, then the calls. A
     * failure before the commit lands nothing: EXECUTION_ERROR +
     * [HostOptions.onActionFailed] (the toast), the outcome's `error`.
     * VAPP-271: a result call (`call` + `result`) is gated and run before
     * the commit and the action re-runs over the same snapshot with its
     * answer.
     */
    private suspend fun runPackageAction(call: FunctionCallInfo, action: String): FunctionOutcome {
        fun failed(message: String): FunctionOutcome {
            sendError("EXECUTION_ERROR", call.surfaceId, message)
            options.onActionFailed?.invoke(ActionFailure(call.surfaceId, action, message))
            return FunctionOutcome(FunctionDecision.Allow, error = message)
        }
        val model = surfaces[call.surfaceId]
        val pkg = packageOf[call.surfaceId]?.let { packages[it] }
        if (model == null || pkg == null) return failed("no action $action")
        // VAPP-271: a `call` step with a `result` runs when the plan reaches
        // it (its gate first, before the commit; a refusal or a failure = the
        // action's failure); the action runs again over the SAME snapshot
        // with its answer as `results.<name>`.
        val data = model.data
        val nowMs = options.now?.invoke()
        val results = mutableMapOf<String, JsonValue>()
        fun runAction() = Actions.run(pkg, action, JsonValue.Obj(call.args), data, options.me, nowMs, options.zone, results = results.toMap())
        var outcome = runAction()
        while (outcome is Actions.Outcome.Waiting) {
            val pending = outcome.pending
            val c = FunctionCallInfo(call.surfaceId, call.componentId, pending.name, pending.args)
            val verdict = gate(c)
            if (verdict.decision != FunctionDecision.Allow) return failed(verdict.error ?: "${c.name} was not allowed")
            val answer = run(c)
            answer.error?.let { return failed(it) }
            if (answer.decision != FunctionDecision.Allow) return failed("${c.name} was not allowed")
            results[pending.result] = JsonValue.of(answer.result)
            outcome = runAction()
        }
        val plan = when (outcome) {
            is Actions.Outcome.Error -> return failed(outcome.message)
            is Actions.Outcome.Planned -> outcome.plan
            is Actions.Outcome.Waiting -> return failed("no action $action")
        }
        val calls = plan.calls.map { FunctionCallInfo(call.surfaceId, call.componentId, it.name, it.args) }
        for (c in calls) {
            val verdict = gate(c)
            if (verdict.decision != FunctionDecision.Allow) return failed(verdict.error ?: "${c.name} was not allowed")
        }
        var committed: Any? = mapOf("ids" to emptyList<String>())
        val writes = plan.writes
        if (writes.isNotEmpty()) {
            val fn = functions[Actions.BATCH_FUNCTION] ?: return failed("this host keeps no records")
            val args = mapOf("writes" to JsonValue.Arr(writes))
            committed = try {
                fn(args, call.copy(name = Actions.BATCH_FUNCTION, args = args))
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                return failed(e.message ?: e.toString())
            }
        }
        for (s in plan.sets) model.assignData(s.path, s.value)
        for (c in calls) {
            val outcome = run(c)
            val error = outcome.error ?: if (outcome.decision == FunctionDecision.Allow) null else "${c.name} was not allowed"
            if (error != null) return FunctionOutcome(FunctionDecision.Allow, result = committed, error = error)
        }
        return FunctionOutcome(FunctionDecision.Allow, result = committed)
    }

    /**
     * VAPP-122: the `bind` action: re-follow `path` from `source` with
     * `params` appended ([sourceWithParams]), through the router like a
     * server's bindDataModel (the surface's package allowlist applies; a
     * refusal is the usual SOURCE_DENIED / VALIDATION_FAILED error). True
     * when the bind op ran.
     */
    fun bind(surfaceId: String, args: Map<String, JsonValue>): Boolean {
        val path = args["path"]?.string ?: ""
        val source = args["source"]?.string ?: ""
        if (source.isEmpty() || surfaces[surfaceId] == null) return false
        val params = args["params"] as? JsonValue.Obj
        val message = JsonValue.Obj(
            mapOf(
                "version" to JsonValue.Str(A2UI_VERSION),
                "bindDataModel" to JsonValue.Obj(
                    mapOf(
                        "surfaceId" to JsonValue.Str(surfaceId),
                        "path" to JsonValue.Str(path),
                        "source" to JsonValue.Str(sourceWithParams(source, params)),
                    ),
                ),
            ),
        )
        return receive(message, trusted = true).any { it["op"]?.string == "bind" }
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
     * the media policy's [MediaOptions.allow] on the raw src, the plugin's
     * `resolveUrl` rewrite, then the media policy (defaults without one).
     * null = denied or unresolvable: nothing loads.
     */
    fun mediaRequest(src: String): MediaRequest? = policedRawMediaRequest(src, mediaOptions, options.plugin::resolveUrl)

    /** The media policy (the host policy's, else the plugin's; null = the contract defaults). */
    val mediaOptions: MediaOptions? get() = options.policy.media ?: options.plugin.mediaOptions

    /**
     * `onPaintError` (catalog/host.json `paint`): a component's painter
     * failed. Forwarded ONCE per surface + component as an A2UI
     * `RENDER_FAILED` error at `/components/<componentId>` (the host issue),
     * until an `updateComponents` names that component again (or the
     * surface is created / deleted).
     */
    fun paintError(error: PaintError) {
        if (!paintErrors.getOrPut(error.surfaceId) { HashSet() }.add(error.componentId)) return
        sendError(RENDER_FAILED, error.surfaceId, error.message, "/components/${error.componentId}")
    }

    private fun forgetPaintErrors(surfaceId: String) {
        paintErrors.remove(surfaceId)
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

        override fun afterLayout(surfaceId: String) = this@ExponentialHost.afterLayout(surfaceId)

        override fun onFunctionCall(event: SurfaceFunctionCallEvent) {
            val call = FunctionCallInfo(event.surfaceId, event.componentId, event.name, event.args)
            scope.launch { callFunction(call) }
            base.onFunctionCall(event)
        }

        override fun onFunctionCallThen(event: SurfaceFunctionCallEvent, then: () -> Unit) {
            val call = FunctionCallInfo(event.surfaceId, event.componentId, event.name, event.args)
            scope.launch { if (!callFunction(call).failed) then() }
            base.onFunctionCall(event)
        }

        override val urlPolicy: UrlPolicy get() = this@ExponentialHost.urlPolicy

        override val mediaOptions: MediaOptions? get() = this@ExponentialHost.mediaOptions

        override fun openUrl(url: String) {
            this@ExponentialHost.openUrl(url)
        }

        override fun textLinks(text: String): List<TextLinkSegment>? =
            options.policy.textLinks?.invoke(text) ?: base.textLinks(text)

        override fun imageSize(src: String): ImageSize? = options.policy.imageSize?.invoke(src) ?: base.imageSize(src)

        override val onImagePress: ((String) -> Unit)? get() = options.policy.onImagePress ?: base.onImagePress

        override fun attachment(href: String): HostAttachment? = options.policy.attachment?.invoke(href) ?: base.attachment(href)

        override val markdownOptions: at.exponential.ui.primitives.MarkdownOptions get() = options.policy.markdownOptions ?: base.markdownOptions

        override fun resolveUrl(src: String): String = base.resolveUrl(src)

        override val appFile: AppFileLoader? get() = options.policy.appFile ?: base.appFile

        override fun onPaintError(error: PaintError) {
            paintError(error)
            base.onPaintError(error)
        }

        override fun onUnknown(component: String, catalogId: String?, id: String) = base.onUnknown(component, catalogId, id)

        override fun fontFamily(name: String): FontFamily? = base.fontFamily(name)

        override fun markdown(text: String, width: Float): (@Composable () -> Unit)? = base.markdown(text, width)

        override fun copy(text: String) = base.copy(text)

        override fun pickFiles(request: FilePickRequest) = base.pickFiles(request)
        override fun onUpload(event: UploadEvent) = base.onUpload(event)
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
