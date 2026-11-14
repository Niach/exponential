package com.exponential.app

import android.app.Application
import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import android.provider.Settings
import android.util.Log
import io.ktor.client.HttpClient
import io.ktor.client.request.get
import io.ktor.client.statement.bodyAsText
import io.sentry.Breadcrumb
import io.sentry.RequestDetails
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.android.core.SentryAndroid
import io.sentry.okhttp.SentryOkHttpInterceptor
import io.sentry.protocol.User
import java.net.URI
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.Interceptor

/** `GET <server>/api/monitoring/config?platform=android` (#LOGS-2). */
@Serializable
data class MonitoringConfig(
    val enabled: Boolean = false,
    val dsn: String? = null,
    val environment: String? = null,
    val tracesSampleRate: Double? = null,
)

/**
 * #LOGS-2: crashes (JVM + native), ANRs, app-start/activity/navigation traces,
 * network spans and breadcrumbs, sent THROUGH the signed-in server's
 * `/api/monitoring` tunnel, never straight to GlitchTip. Off unless that
 * server's config says enabled AND this device's "Send diagnostics" is on.
 * The last config is cached so the next launch starts before the network
 * (an early crash is still caught). The manifest drops Sentry's auto-init.
 */
object Observability {
    private const val PREFS = "observability"
    private const val KEY_SEND = "send_diagnostics"
    private const val KEY_SERVER = "config_server"
    private const val KEY_CONFIG = "config_json"
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    private var prefs: SharedPreferences? = null
    private var app: Application? = null
    private var server: String? = null
    private var config: MonitoringConfig? = null
    /** The server + config Sentry is running with (null = off). */
    private var running: Pair<String, MonitoringConfig>? = null
    private var userId: String? = null

    private val _sendDiagnostics = MutableStateFlow(true)
    /** The per-device "Send diagnostics" toggle (Settings › Account › Privacy), default on. */
    val sendDiagnostics: StateFlow<Boolean> = _sendDiagnostics.asStateFlow()

    /** First thing in `Application.onCreate`: starts from the cached config. */
    fun start(app: Application) {
        this.app = app
        val p = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs = p
        _sendDiagnostics.value = p.getBoolean(KEY_SEND, true)
        server = p.getString(KEY_SERVER, null)
        config = p.getString(KEY_CONFIG, null)?.let { runCatching { json.decodeFromString<MonitoringConfig>(it) }.getOrNull() }
        apply()
    }

    /**
     * Asks [serverUrl] (the active account's server) for its config and
     * applies it. A 4xx (an older or self-hosted server without the endpoint)
     * = off; a network failure keeps the cached answer.
     */
    suspend fun refresh(client: HttpClient, serverUrl: String?) {
        val base = serverUrl?.trimEnd('/') ?: return
        val fetched = runCatching {
            val res = client.get("$base/api/monitoring/config?platform=android")
            when (res.status.value) {
                in 200..299 -> json.decodeFromString<MonitoringConfig>(res.bodyAsText())
                in 400..499 -> MonitoringConfig(enabled = false)
                else -> null
            }
        }.getOrNull() ?: return
        withContext(Dispatchers.Main) {
            server = base
            config = fetched
            prefs?.edit()?.putString(KEY_SERVER, base)?.putString(KEY_CONFIG, json.encodeToString(MonitoringConfig.serializer(), fetched))?.apply()
            apply()
        }
    }

    fun setSendDiagnostics(on: Boolean) {
        _sendDiagnostics.value = on
        prefs?.edit()?.putBoolean(KEY_SEND, on)?.apply()
        apply()
    }

    /** The Exponential user id only: never email, name or IP. */
    fun setUser(userId: String?) {
        this.userId = userId
        if (running != null) Sentry.setUser(userId?.let { User().apply { id = it } })
    }

    /** A failure the app survives (a native library that would not load): logcat, and Sentry while it runs. */
    fun nonFatal(tag: String, message: String, error: Throwable) {
        Log.e(tag, message, error)
        if (running != null) Sentry.captureException(error)
    }

    /** Network spans for our API + Electric shape requests only (inert while Sentry is off). */
    fun okHttpInterceptor(): Interceptor = SentryOkHttpInterceptor(
        beforeSpan = { span, request, _ -> if (isTracedPath(request.url.encodedPath)) span else null },
    )

    /** What [isPrelaunchDevice] reads off the device. */
    internal data class DeviceProbe(
        /** Settings.System `firebase.test.lab`. */
        val testLabSetting: String?,
        /** Build.MODEL. */
        val model: String,
        val widthPixels: Int,
        val heightPixels: Int,
    )

    /**
     * Play's pre-launch report robots (Firebase Test Lab). Google's documented
     * marker is Settings.System `firebase.test.lab`, but the #ERR-22 robot
     * (2026-11-13, 0.14.137+220) did not set it and reported as prod. Its own
     * telltales: Build.MODEL "OnePlus8Pro" (a retail OnePlus 8 Pro reports
     * IN20xx; "OnePlus8Pro" is only its device codename) and a 288×448 px
     * screen no phone ships with.
     */
    internal fun isPrelaunchDevice(probe: DeviceProbe): Boolean =
        probe.testLabSetting == "true" ||
            probe.model in PRELAUNCH_MODELS ||
            (minOf(probe.widthPixels, probe.heightPixels) to maxOf(probe.widthPixels, probe.heightPixels)) == PRELAUNCH_SCREEN

    private val PRELAUNCH_MODELS = setOf("OnePlus8Pro")
    private val PRELAUNCH_SCREEN = 288 to 448

    private fun isTestLabDevice(app: Application): Boolean = runCatching {
        val metrics = app.resources.displayMetrics
        isPrelaunchDevice(
            DeviceProbe(
                testLabSetting = runCatching { Settings.System.getString(app.contentResolver, "firebase.test.lab") }.getOrNull(),
                model = Build.MODEL.orEmpty(),
                widthPixels = metrics.widthPixels,
                heightPixels = metrics.heightPixels,
            ),
        )
    }.getOrDefault(false)

    /** A Test Lab device reports as [PRELAUNCH_ENVIRONMENT], whatever the server asks for. */
    internal fun environmentFor(configured: String?, testLab: Boolean): String = when {
        testLab -> PRELAUNCH_ENVIRONMENT
        configured != null -> configured
        BuildConfig.DEBUG -> "dev"
        BuildConfig.IS_STAGING -> "staging"
        else -> "prod"
    }

    const val PRELAUNCH_ENVIRONMENT = "prelaunch"

    /** The OkHttp integration's event for a 5xx answer. */
    internal fun isHttpClientError(event: SentryEvent): Boolean =
        event.exceptions?.any { it.type == "SentryHttpClientException" } == true

    internal fun isTracedPath(path: String): Boolean =
        path.startsWith("/api/") && !path.startsWith("/api/attachments/") && !path.startsWith("/api/monitoring")

    private fun apply() {
        val app = app ?: return
        val base = server
        val cfg = config
        val dsn = cfg?.dsn
        val wanted = if (_sendDiagnostics.value && cfg?.enabled == true && !dsn.isNullOrBlank() && base != null) base to cfg else null
        if (wanted == running) return
        if (running != null) Sentry.close()
        running = wanted
        if (wanted == null || dsn == null) return
        val tunnel = "$base/api/monitoring?platform=android"
        val host = runCatching { URI(base).host }.getOrNull()
        SentryAndroid.init(app) { options ->
            options.dsn = dsn
            // The envelope goes to our server, which forwards it to GlitchTip;
            // a 5xx/429 keeps it for a retry (#ERR-17, TunnelTransport.kt).
            options.setTransportFactory { o, details -> TunnelTransport(o, RequestDetails(tunnel, details.headers)) }
            BuildConfig.SENTRY_PROGUARD_UUID.takeIf { it.isNotBlank() }?.let { options.proguardUuid = it }
            options.release = "exponential-android@${BuildConfig.VERSION_NAME}+${BuildConfig.VERSION_CODE}"
            val testLab = isTestLabDevice(app)
            options.environment = environmentFor(cfg.environment, testLab)
            // Play's pre-launch report robots run every uploaded build on
            // Firebase Test Lab devices whose network answers some requests
            // with an empty 504 of its own; those never reach our server, so
            // their HTTP errors stay out (crashes and ANRs still come in).
            options.sampleRate = 1.0
            options.tracesSampleRate = cfg.tracesSampleRate ?: 0.1
            options.isSendDefaultPii = false
            options.isAttachScreenshot = false
            options.isAttachViewHierarchy = false
            options.logs.isEnabled = true
            // Trace headers only to the server we report through.
            options.setTracePropagationTargets(listOfNotNull(host))
            options.setBeforeSend { event, _ -> if (testLab && isHttpClientError(event)) null else SentryScrubber.scrub(event) }
            options.setBeforeBreadcrumb { breadcrumb, _ -> SentryScrubber.scrub(breadcrumb) }
        }
        setUser(userId)
    }
}

/** Strips credentials from everything Sentry sends (unit-tested in SentryScrubberTest). */
object SentryScrubber {
    const val FILTERED = "[Filtered]"
    private val sensitiveKey = Regex("token|secret|password|key|ticket|session|dsn|authorization|cookie", RegexOption.IGNORE_CASE)
    private val queryParam = Regex("([?&])([^=&#]+)=([^&#]*)")

    fun isSensitiveKey(key: String): Boolean = sensitiveKey.containsMatchIn(key)

    /** `?token=abc&live=true` → `?token=[Filtered]&live=true`. */
    fun scrubUrl(url: String): String = queryParam.replace(url) { m ->
        val (sep, name, value) = m.destructured
        "$sep$name=${if (isSensitiveKey(name)) FILTERED else value}"
    }

    fun scrubMap(map: Map<String, Any?>): Map<String, Any?> = map.mapValues { (k, v) ->
        when {
            isSensitiveKey(k) -> FILTERED
            v is String && (k == "url" || k.endsWith("_url")) -> scrubUrl(v)
            else -> v
        }
    }

    fun scrub(event: SentryEvent): SentryEvent {
        event.request?.let { req ->
            req.headers = req.headers?.let { scrubStrings(it) }
            req.cookies = null
            req.url = req.url?.let(::scrubUrl)
            req.queryString = req.queryString?.let { scrubUrl("?$it").removePrefix("?") }
            @Suppress("UNCHECKED_CAST")
            (req.data as? Map<String, Any?>)?.let { req.data = scrubMap(it) }
        }
        event.contexts.response?.let { res ->
            res.headers = res.headers?.let { scrubStrings(it) }
            res.cookies = null
        }
        event.extras?.keys?.toList()?.forEach { k -> if (isSensitiveKey(k)) event.setExtra(k, FILTERED) }
        event.tags?.keys?.toList()?.forEach { k -> if (isSensitiveKey(k)) event.setTag(k, FILTERED) }
        event.breadcrumbs?.forEach { scrub(it) }
        event.user?.let { user -> event.user = user.id?.let { id -> User().apply { this.id = id } } }
        return event
    }

    fun scrub(breadcrumb: Breadcrumb): Breadcrumb {
        val data = breadcrumb.data
        scrubMap(data.toMap()).forEach { (k, v) -> if (v != data[k]) breadcrumb.setData(k, v ?: FILTERED) }
        return breadcrumb
    }

    @Suppress("UNCHECKED_CAST")
    private fun scrubStrings(map: Map<String, String>): Map<String, String> =
        scrubMap(map) as Map<String, String>
}
