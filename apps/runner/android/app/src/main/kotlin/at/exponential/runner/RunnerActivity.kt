package at.exponential.runner

import android.content.Intent
import android.content.res.Configuration
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.lifecycleScope
import at.exponential.engine.capabilities.AndroidCapabilities
import at.exponential.engine.capabilities.ComponentActivityHost
import at.exponential.engine.capabilities.NotificationTaps
import at.exponential.engine.ffi.Engine
import at.exponential.engine.ffi.EngineHost
import at.exponential.engine.ffi.EngineIdentity
import at.exponential.engine.ffi.EngineProviders
import at.exponential.engine.ffi.RunnerFormFactor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.launch
import java.io.File
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.Executors

/**
 * #VAPP-267: the Android runner's one activity. It reads the bundle from
 * `assets/app`, runs it in ONE Engine instance (identity `local`, host kind
 * `exported`, SQLite + files under `files/apps/<id>`, the Keystore Vault, the
 * E5 capabilities) and drives lifecycle.json from its own events: create =
 * `launch` (or the cold `deepLink` / `notificationTap` it was started with),
 * start = `foreground`, stop = `background`, finishing = `terminate`, a new
 * intent = `deepLink` / `notificationTap`. Configuration changes do not
 * recreate it (a capability's pending picker survives a rotation); the
 * form factor follows the window width across genUI's `md` (768 dp).
 */
class RunnerActivity : ComponentActivity() {
    private val worker = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
    private val mainHandler = Handler(Looper.getMainLooper())
    private lateinit var controller: RunnerController
    private var config: RunnerConfig? = null
    private var launched = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val dark = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
        enableEdgeToEdge(statusBarStyle = dark, navigationBarStyle = dark)
        val painter = ComposePainter(lifecycleScope)
        controller = RunnerController(painter, { mainHandler.post(it) }, worker, lifecycleScope)
        painter.controller = controller
        setContent {
            RunnerWindow(painter) { width -> controller.setFormFactor(formFactor(width)) }
        }

        val bundle = RunnerBundle.read { name -> runCatching { assets.open("${RunnerBundle.DIR}/$name").bufferedReader().use { it.readText() } }.getOrNull() }
        config = (bundle as? BundleLoad.Ready)?.config
        val activity = this
        val started = controller.start(bundle) { cfg ->
            val vault = KeystoreVault(applicationContext)
            val capabilities = AndroidCapabilities.all(ComponentActivityHost(applicationContext) { activity.takeIf { !it.isDestroyed } })
            Engine(
                EngineHost(
                    kind = "exported",
                    platform = "android",
                    formFactor = if (resources.configuration.smallestScreenWidthDp >= WIDE_DP) "tablet" else "phone",
                    locale = Locale.getDefault().toLanguageTag(),
                    zone = TimeZone.getDefault().id,
                    network = cfg.network,
                    dataDir = File(filesDir, "apps/${cfg.id}").absolutePath,
                ),
                EngineIdentity.Local(RunnerController.deviceId(vault)),
                EngineProviders(vault, controller.window, capabilities = capabilities),
            )
        }
        if (!started) return
        controller.setFormFactor(formFactor(resources.configuration.screenWidthDp.toFloat()))
        val cold = linkEvent(intent)
        lifecycleScope.launch {
            controller.lifecycle(cold ?: """{"type":"launch"}""")
            launched = true
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val event = linkEvent(intent) ?: return
        lifecycleScope.launch { controller.lifecycle(event) }
    }

    override fun onStart() {
        super.onStart()
        if (launched) lifecycleScope.launch { controller.lifecycle("""{"type":"foreground"}""") }
    }

    override fun onStop() {
        super.onStop()
        if (launched) lifecycleScope.launch { controller.lifecycle("""{"type":"background"}""") }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        controller.setFormFactor(formFactor(newConfig.screenWidthDp.toFloat()))
    }

    override fun onDestroy() {
        if (isFinishing && launched) {
            // Blocking on purpose: the instance persists its screen before the process may go.
            kotlinx.coroutines.runBlocking(Dispatchers.Default) { controller.lifecycle("""{"type":"terminate"}""") }
        }
        super.onDestroy()
        worker.close()
    }

    /** A notification tap (the E5 capability's extra) or this runner's deep link; null = a plain start. */
    private fun linkEvent(intent: Intent?): String? {
        NotificationTaps.fromIntent(intent)?.let { return it }
        val id = config?.id ?: return null
        val data = intent?.takeIf { it.action == Intent.ACTION_VIEW }?.dataString ?: return null
        return RunnerLinks.event(data, id)
    }

    private fun formFactor(widthDp: Float) = if (widthDp >= WIDE_DP) RunnerFormFactor.WIDE else RunnerFormFactor.COMPACT

    private companion object {
        /** genUI's `$breakpoint.md`. */
        const val WIDE_DP = 768
    }
}
