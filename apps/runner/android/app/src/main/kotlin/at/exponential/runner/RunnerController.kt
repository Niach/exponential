package at.exponential.runner

import at.exponential.engine.ffi.AppInstance
import at.exponential.engine.ffi.CallResult
import at.exponential.engine.ffi.ConsentAnswer
import at.exponential.engine.ffi.Engine
import at.exponential.engine.ffi.EngineException
import at.exponential.engine.ffi.GrantValue
import at.exponential.engine.ffi.Presentation
import at.exponential.engine.ffi.RunnerFormFactor
import at.exponential.engine.ffi.Vault
import at.exponential.engine.ffi.WindowHost
import at.exponential.engine.ffi.runnerFailure
import at.exponential.engine.ffi.runnerPackage
import at.exponential.engine.ffi.runnerToast
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.UUID

/** The runner's surfaces (runner.json `SURFACES`). */
object RunnerSurfaces {
    const val APP = "app"
    const val CONSENT = "consent"
    const val PERMISSIONS = "permissions"
    const val TOAST = "toast"
}

/**
 * What the runner paints: genUI surfaces only (runner.json: no chrome of its
 * own). Called on the main thread.
 */
interface RunnerPainter {
    /** Install the package the shell derived (`runner_package`). */
    fun install(packageJson: String)

    /** Surface `app` = the package's template [screen] with the data `{params}`. */
    fun open(packageId: String, screen: String, params: Map<String, String>)

    /** One of the runner's own surfaces from its components (+ data model). */
    fun paint(surface: String, componentsJson: String, dataJson: String? = null)

    fun close(surface: String)
}

/**
 * The runner's glue (runner.json "HOW A RUNNER WORKS"): ONE Engine
 * `AppInstance` for the bundled package, its lifecycle driven by the
 * activity, every app call through the instance, the consent / permissions /
 * toast surfaces, and the form factor the shell is derived for. Engine calls
 * run on [worker] (capabilities block and refuse the main thread); painting
 * hops to the main thread through [main].
 */
class RunnerController(
    private val painter: RunnerPainter,
    private val main: (Runnable) -> Unit,
    private val worker: CoroutineDispatcher,
    private val scope: CoroutineScope,
    private val newId: () -> String = { UUID.randomUUID().toString() },
) {
    /** The Engine's WindowHost: `present` is the open screen (the runner has no stack, so every presentation replaces). */
    val window: WindowHost = object : WindowHost {
        override fun `present`(`app`: String, `screen`: String, `paramsJson`: String, `how`: Presentation) {
            val params = (runCatching { JsonValue.parse(paramsJson) }.getOrNull()?.obj ?: emptyMap())
                .mapNotNull { (k, v) -> (v.string ?: v.json).let { k to it } }.toMap()
            presented = true
            current = screen to params
            val pkg = packageId ?: return
            main { painter.open(pkg, screen, params) }
        }
    }

    @Volatile private var instance: AppInstance? = null
    @Volatile private var packageId: String? = null
    @Volatile private var presented = false
    @Volatile private var current: Pair<String, Map<String, String>>? = null
    @Volatile private var formFactor: RunnerFormFactor? = null
    @Volatile private var derived: JsonValue? = null
    private var pendingConsent: CompletableDeferred<ConsentAnswer>? = null
    private val sources = ArrayList<Pair<String, (JsonValue?) -> Unit>>()

    /** The instance once the bundle loaded (tests). */
    val app: AppInstance? get() = instance

    /** The failure painted in place of `app` (null = the app runs). */
    @Volatile var failure: String? = null
        private set

    /**
     * Step 1: the bundle → ONE instance over [engine] (made from the config
     * with [window]), or the failure surface (`missing`, `config`,
     * `signature`, `invalid`). Runs on the caller's thread.
     */
    fun start(bundle: BundleLoad, engine: (RunnerConfig) -> Engine): Boolean {
        val ready = when (bundle) {
            is BundleLoad.Failed -> return fail(bundle.kind)
            is BundleLoad.Ready -> bundle
        }
        val app = try {
            engine(ready.config).load(ready.packageFile)
        } catch (e: EngineException.Load) {
            return fail(if (e.kind == "signature") "signature" else "invalid")
        } catch (e: EngineException) {
            return fail("invalid")
        }
        instance = app
        packageId = app.appId()
        return true
    }

    private fun fail(kind: String): Boolean {
        failure = kind
        val components = runCatching { runnerFailure(kind) }.getOrElse { runnerFailure("invalid") }
        main { painter.paint(RunnerSurfaces.APP, components) }
        return false
    }

    /**
     * Step 2: the shell for the window's form factor (`wide` from genUI's
     * `md` up). A change re-installs the derived package and re-applies the
     * open screen.
     */
    fun setFormFactor(form: RunnerFormFactor) {
        val app = instance ?: return
        if (form == formFactor) return
        formFactor = form
        val pkg = runnerPackage(app.packageJson(), form)
        derived = JsonValue.parse(pkg)
        main { painter.install(pkg) }
        val open = current
        val id = packageId
        if (open != null && id != null) main { painter.open(id, open.first, open.second) }
    }

    /** Step 3: a lifecycle.json event (`launch`, `foreground`, `background`, `terminate`, `deepLink`, `notificationTap`). */
    suspend fun lifecycle(eventJson: String) {
        val app = instance ?: return
        withContext(worker) {
            val launching = current == null
            presented = false
            val effects = runCatching { JsonValue.parse(app.event(eventJson)).array }.getOrNull().orEmpty()
            for (effect in effects) {
                if (effect["effect"]?.string == "refused") toast(LINK_REFUSED)
            }
            // A plain launch that opens nothing (a package without screens) opens the derived package's first screen.
            if (launching && !presented && current == null) firstScreen()?.let { window.present(app.appId(), it, "{}", Presentation.REPLACE) }
            if (effects.any { it["effect"]?.string == "sources" && it["live"]?.bool == true }) refreshSources()
        }
    }

    private fun firstScreen(): String? {
        val pkg = derived ?: instance?.let { JsonValue.parse(runnerPackage(it.packageJson(), formFactor ?: RunnerFormFactor.COMPACT)) } ?: return null
        return pkg["screens"]?.array?.firstOrNull()?.get("template")?.string ?: pkg["templates"]?.obj?.keys?.firstOrNull()
    }

    /**
     * Step 4: one of the app's function calls through the instance:
     * `records.*`, `app.navigate`, capabilities through the broker. `Ask`
     * paints `consent` and waits for its `consent.answer`; `Refused` paints
     * `toast`. The value, or null when refused.
     */
    suspend fun call(name: String, args: JsonValue.Obj): JsonValue? {
        val app = instance ?: return null
        val argsJson = args.json
        var result = withContext(worker) { app.call(name, argsJson, newIds(name, args)) }
        while (result is CallResult.Ask) {
            val ask = result
            val answer = CompletableDeferred<ConsentAnswer>()
            synchronized(this) {
                pendingConsent?.complete(ConsentAnswer.DISMISS)
                pendingConsent = answer
            }
            main { painter.paint(RunnerSurfaces.CONSENT, ask.sheet.componentsJson, ask.sheet.dataJson) }
            val picked = answer.await()
            main { painter.close(RunnerSurfaces.CONSENT) }
            result = withContext(worker) { app.answerConsent(name, argsJson, ask.verdictJson, picked) }
        }
        return when (result) {
            is CallResult.Value -> {
                if (name.startsWith("records.")) withContext(worker) { refreshSources() }
                runCatching { JsonValue.parse(result.json) }.getOrNull() ?: JsonValue.Null
            }
            is CallResult.Refused -> {
                toast(result.message)
                null
            }
            is CallResult.Ask -> null
        }
    }

    /** The uuids a call's creates take: one for an upsert without an id, one per write of a batch. */
    private fun newIds(name: String, args: JsonValue.Obj): List<String> = when (name) {
        "records.upsert" -> if (args["id"]?.string.isNullOrEmpty()) listOf(newId()) else emptyList()
        "records.batch" -> List(args["writes"]?.array?.size ?: 0) { newId() }
        else -> emptyList()
    }

    /**
     * Step 5: the events the runner handles itself (true = handled):
     * `runner.navigate {screen}`, `runner.permissions`, `consent.toggle
     * {scope}`, `consent.answer {answer}`.
     */
    fun onEvent(name: String, context: JsonValue): Boolean {
        when (name) {
            "runner.navigate" -> {
                val screen = context["screen"]?.string ?: return true
                scope.launch { call("app.navigate", JsonValue.Obj(mapOf("screen" to JsonValue.Str(screen)))) }
            }
            "runner.permissions" -> scope.launch { withContext(worker) { paintPermissions() } }
            "consent.toggle" -> {
                val scopeId = context["scope"]?.string ?: return true
                scope.launch { withContext(worker) { toggle(scopeId) } }
            }
            "consent.answer" -> {
                val answer = when (context["answer"]?.string) {
                    "allow" -> ConsentAnswer.ALLOW
                    "once" -> ConsentAnswer.ONCE
                    "deny" -> ConsentAnswer.DENY
                    else -> ConsentAnswer.DISMISS
                }
                synchronized(this) {
                    pendingConsent?.complete(answer)
                    pendingConsent = null
                }
            }
            else -> return false
        }
        return true
    }

    private fun paintPermissions() {
        val app = instance ?: return
        val sheet = app.permissionsSheetJson()
        main { painter.paint(RunnerSurfaces.PERMISSIONS, sheet) }
    }

    /** The Switch's gesture: granted → revoke, else granted (the ConsentUi write), then repaint. */
    private fun toggle(scopeId: String) {
        val app = instance ?: return
        val granted = JsonValue.parse(app.grantsJson())[scopeId]?.string == "granted"
        runCatching { app.setGrant(scopeId, if (granted) null else GrantValue.GRANTED) }
            .onFailure { toast(it.message ?: "") }
        paintPermissions()
    }

    private fun toast(message: String) {
        val components = runnerToast(message)
        main { painter.paint(RunnerSurfaces.TOAST, components) }
    }

    /**
     * `records:` sources resolve through `records_value`; every write
     * re-resolves the bound ones (the runner's store has no other writer).
     * Returns the cancel.
     */
    fun bindSource(uri: String, emit: (JsonValue?) -> Unit): () -> Unit {
        val entry = uri to emit
        synchronized(sources) { sources.add(entry) }
        scope.launch { withContext(worker) { resolve(entry) } }
        return { synchronized(sources) { sources.remove(entry) } }
    }

    private fun refreshSources() {
        val bound = synchronized(sources) { sources.toList() }
        bound.forEach(::resolve)
    }

    private fun resolve(entry: Pair<String, (JsonValue?) -> Unit>) {
        val app = instance ?: return
        val value = app.recordsValue(entry.first)?.let { runCatching { JsonValue.parse(it) }.getOrNull() } ?: return
        main { entry.second(value) }
    }

    companion object {
        /** lifecycle.json `copy.refused`: a link or tap that cannot open. */
        const val LINK_REFUSED = "Couldn't open the link"

        /** runner.json `storage.deviceKey`: the `local` identity's device uuid, created once. */
        const val DEVICE_KEY = "eengine/runner/device"

        fun deviceId(vault: Vault, newId: () -> String = { UUID.randomUUID().toString() }): String =
            vault.read(DEVICE_KEY) ?: newId().also { vault.write(DEVICE_KEY, it) }
    }
}
