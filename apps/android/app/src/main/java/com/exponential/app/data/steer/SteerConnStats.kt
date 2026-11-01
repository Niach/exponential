package com.exponential.app.data.steer

import android.util.Log
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject

// EXP-1529: privacy-safe connection analytics (fixtures/steer-enet.json
// `connStats`): eCore envelopes {v:1, id, kind, at, source, data}, no
// content, no user, team, session or device id. One `steer.view` per viewer
// OPEN of a feed, recorded `settled` (first frame or settleAfterMs) and
// `ended` (park / close). Buffered and posted to `connStats.record` every
// flushMs and on app background; never blocks or errors the UI.

private const val TAG = "SteerConnStats"

internal object SteerConnStatsSpec {
    val kinds = setOf("steer.view", "steer.serve")
    val sources = setOf("steer-viewer", "steer-device")
    val phases = setOf("settled", "ended")
    val platforms = setOf("web", "desktop", "ios", "android", "cli")
    val paths = setOf("direct", "iroh_relay", "server_fallback")
    val outcomes = setOf("ok", "failed")
    val errorClasses = setOf(
        "no_endpoint", "no_node", "refused", "unauthorized", "join_timeout", "stale", "send_failed",
        "closed", "no_key", "device_offline", "device_unsupported", "ticket_failed", "relay_closed", "other",
    )
    const val SETTLE_AFTER_MS = 10_000L
    const val FLUSH_MS = 60_000L
    const val BATCH_MAX = 50
    const val VERSION_MAX = 64
}

private val EVENT_KEYS = setOf("v", "id", "kind", "at", "source", "data")
private val DATA_KEYS = setOf(
    "phase", "platform", "version", "outcome", "path", "firstFrameMs", "errorClass",
    "fallbacks", "reconnects", "durationMs", "failures",
)

private fun JsonElement?.isCount(): Boolean {
    val p = this as? JsonPrimitive ?: return false
    if (p.isString) return false
    val n = p.longOrNull ?: return false
    return p.content == n.toString() && n in 0..2_147_483_647
}

private fun JsonElement?.str(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull

/** The server's validation of one event (fixture `validCases`); the client drops what fails before buffering. */
internal fun steerConnEventValid(event: JsonElement): Boolean {
    val e = event as? JsonObject ?: return false
    if (e.keys.any { it !in EVENT_KEYS }) return false
    if ((e["v"] as? JsonPrimitive)?.takeIf { !it.isString }?.content != "1") return false
    val id = e["id"].str() ?: return false
    if (id.isEmpty() || id.length > 64) return false
    if (e["kind"].str() !in SteerConnStatsSpec.kinds || e["source"].str() !in SteerConnStatsSpec.sources) return false
    val at = (e["at"] as? JsonPrimitive)?.takeIf { !it.isString } ?: return false
    val atMs = at.longOrNull ?: return false
    if (at.content != atMs.toString() || atMs <= 0) return false
    val d = e["data"] as? JsonObject ?: return false
    if (d.keys.any { it !in DATA_KEYS }) return false
    if (d["phase"].str() !in SteerConnStatsSpec.phases || d["platform"].str() !in SteerConnStatsSpec.platforms) return false
    val version = d["version"].str() ?: return false
    if (version.isEmpty() || version.length > SteerConnStatsSpec.VERSION_MAX) return false
    if (d["outcome"].str() !in SteerConnStatsSpec.outcomes) return false
    if (d.containsKey("path") && d["path"].str() !in SteerConnStatsSpec.paths) return false
    if (d["outcome"].str() == "ok" && !d.containsKey("path")) return false
    if (d.containsKey("errorClass") && d["errorClass"].str() !in SteerConnStatsSpec.errorClasses) return false
    for (k in listOf("firstFrameMs", "fallbacks", "reconnects", "durationMs")) {
        if (d.containsKey(k) && !d[k].isCount()) return false
    }
    if (d.containsKey("failures")) {
        val f = d["failures"] as? JsonObject ?: return false
        for ((k, n) in f) if (k !in SteerConnStatsSpec.errorClasses || !n.isCount()) return false
    }
    return true
}

/** Where a view's events go ([SteerConnStatsClient] in the app). */
internal fun interface SteerConnStatsSink {
    fun record(event: JsonObject)
}

/**
 * One `steer.view` (fixture `connStats`): [open] on a viewer open, [firstFrame]
 * on the first applied frame, [close] on park / close. Failures and fallbacks
 * arrive from the carriers ([SteerCarrierListener]). Main-thread owned, like
 * the connection; the carriers' callbacks only bump counters.
 */
internal class SteerViewStats(
    private val sink: SteerConnStatsSink,
    private val version: String,
    private val nowMs: () -> Long,
    private val wallMs: () -> Long = System::currentTimeMillis,
    private val newId: () -> String = { UUID.randomUUID().toString() },
) : SteerCarrierListener {
    private var openedAtMs: Long? = null
    private var settled = false
    private var path: String? = null
    private var firstFrameMs: Long? = null
    @Volatile private var fallbacks = 0
    private var reconnects = 0
    private var sawFirst = false
    @Volatile private var lastError: String? = null
    private val failures = mutableMapOf<String, Int>()

    val isOpen: Boolean get() = openedAtMs != null

    fun open() {
        if (openedAtMs != null) return
        openedAtMs = nowMs()
        settled = false
        path = null
        firstFrameMs = null
        fallbacks = 0
        reconnects = 0
        sawFirst = false
        lastError = null
        synchronized(failures) { failures.clear() }
    }

    /** The first applied frame of a feed (every dial); the view's first one settles it. */
    fun firstFrame(carrierPath: String) {
        val opened = openedAtMs ?: return
        if (sawFirst) {
            reconnects++
            return
        }
        sawFirst = true
        path = carrierPath
        firstFrameMs = (nowMs() - opened).coerceAtLeast(0)
        if (!settled) {
            settled = true
            emit("settled", ok = true)
        }
    }

    /** A view that sat settleAfterMs without a frame settles failed (its frame may still come). */
    fun settleTimeout() {
        if (openedAtMs == null || settled) return
        settled = true
        emit("settled", ok = false)
    }

    override fun carrierFallback() {
        fallbacks++
    }

    override fun carrierFailure(errorClass: String) {
        val known = errorClass.takeIf { it in SteerConnStatsSpec.errorClasses } ?: "other"
        lastError = known
        synchronized(failures) { failures[known] = (failures[known] ?: 0) + 1 }
    }

    fun close() {
        val opened = openedAtMs ?: return
        emit("ended", ok = sawFirst, durationMs = (nowMs() - opened).coerceAtLeast(0))
        openedAtMs = null
    }

    private fun emit(phase: String, ok: Boolean, durationMs: Long? = null) {
        val event = buildJsonObject {
            put("v", 1)
            put("id", newId())
            put("kind", "steer.view")
            put("at", wallMs())
            put("source", "steer-viewer")
            putJsonObject("data") {
                put("phase", phase)
                put("platform", "android")
                put("version", version.take(SteerConnStatsSpec.VERSION_MAX).ifEmpty { "0" })
                put("outcome", if (ok) "ok" else "failed")
                if (ok) path?.let { put("path", it) }
                if (ok) firstFrameMs?.let { put("firstFrameMs", it) }
                lastError?.takeIf { !ok || phase == "ended" }?.let { put("errorClass", it) }
                if (phase == "ended") {
                    put("fallbacks", fallbacks)
                    put("reconnects", reconnects)
                    durationMs?.let { put("durationMs", it) }
                    val counts = synchronized(failures) { failures.toMap() }
                    if (counts.isNotEmpty()) putJsonObject("failures") { counts.forEach { (k, n) -> put(k, n) } }
                }
            }
        }
        if (steerConnEventValid(event)) sink.record(event) else Log.w(TAG, "dropped an off-schema event")
    }
}

/**
 * The batching client: buffers events, posts them in batches of
 * [SteerConnStatsSpec.BATCH_MAX] every [flushMs] and on [flush] (app
 * background). A failed post keeps the batch for the next flush (bounded);
 * nothing here ever throws to a caller.
 */
internal class SteerConnStatsClient(
    private val post: suspend (events: List<JsonObject>) -> Unit,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
    private val flushMs: Long = SteerConnStatsSpec.FLUSH_MS,
    private val bufferMax: Int = SteerConnStatsSpec.BATCH_MAX * 4,
) : SteerConnStatsSink {
    private val lock = Any()
    private val buffer = ArrayDeque<JsonObject>()
    private val posting = Mutex()
    private var timer: Job? = null

    override fun record(event: JsonObject) {
        synchronized(lock) {
            buffer.addLast(event)
            while (buffer.size > bufferMax) buffer.removeFirst()
            if (timer?.isActive != true) {
                timer = scope.launch {
                    while (isActive) {
                        delay(flushMs)
                        if (!flushNow()) break
                    }
                }
            }
        }
    }

    /** Post everything buffered now (background / page hide). */
    fun flush() {
        scope.launch { flushNow() }
    }

    /** True while events remain buffered (the timer keeps running). */
    internal suspend fun flushNow(): Boolean = posting.withLock { drain() }

    private suspend fun drain(): Boolean {
        while (true) {
            val batch = synchronized(lock) { buffer.take(SteerConnStatsSpec.BATCH_MAX) }
            if (batch.isEmpty()) return false
            try {
                post(batch)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                Log.w(TAG, "connStats.record failed: ${err.javaClass.simpleName}")
                return true
            }
            synchronized(lock) { repeat(minOf(batch.size, buffer.size)) { buffer.removeFirst() } }
        }
    }

    internal fun buffered(): Int = synchronized(lock) { buffer.size }
}
