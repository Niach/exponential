package com.exponential.app.data.steer

import android.util.Log
import com.exponential.app.data.enet.PeerNode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.onTimeout
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// VAPP-33: the steer viewer over eNet's chat channel
// (packages/domain-contract/fixtures/steer-enet.json is the spec). One chat
// message = one envelope {t:"exp-steer", v:1, sessionId, frames} carrying the
// relay's own wire frames unchanged, so [SteerConnection] reads an eNet feed
// exactly like the relay socket it replaced: [EnetSteerSocket] is a
// [SteerSocket]. VAPP-43: the relay carries no session content; EXP-1529:
// when [EnetSteerDialer] says null (or stalls, or drops) the sealed relay
// tunnel takes over ([FallbackSteerTransport]).

private const val TAG = "SteerEnet"

internal const val STEER_ENET_TAG = "exp-steer"
internal const val STEER_ENET_VERSION = 1

/** The fixture's timeouts; the tests shrink them. */
internal data class SteerEnetTimings(
    val joinTimeoutMs: Long = 8_000,
    val staleMs: Long = 75_000,
    val sendTimeoutMs: Long = 20_000,
)

internal class SteerEnvelope(val sessionId: String, val frames: List<JsonObject>) {
    /** The frames' `t` tags, in order. */
    val types: List<String> get() = frames.map { (it["t"] as JsonPrimitive).content }
}

internal fun encodeSteerEnvelope(sessionId: String, frames: List<JsonObject>): ByteArray =
    JsonObject(
        mapOf(
            "t" to JsonPrimitive(STEER_ENET_TAG),
            "v" to JsonPrimitive(STEER_ENET_VERSION),
            "sessionId" to JsonPrimitive(sessionId),
            "frames" to JsonArray(frames),
        ),
    ).toString().encodeToByteArray()

/** A chat body → the envelope, or null when it is none (the fixture's `parse` rules). */
internal fun parseSteerEnvelope(body: String): SteerEnvelope? {
    if (!body.startsWith("{")) return null
    val value = runCatching { Json.parseToJsonElement(body).jsonObject }.getOrNull() ?: return null
    val tag = (value["t"] as? JsonPrimitive)?.takeIf { it.isString }?.content
    val version = (value["v"] as? JsonPrimitive)?.takeIf { !it.isString }?.intOrNull
    if (tag != STEER_ENET_TAG || version != STEER_ENET_VERSION) return null
    val sessionId = (value["sessionId"] as? JsonPrimitive)?.takeIf { it.isString }?.content
    if (sessionId.isNullOrEmpty()) return null
    val frames = value["frames"] as? JsonArray ?: return null
    if (frames.isEmpty()) return null
    val objects = frames.map { frame ->
        val obj = frame as? JsonObject ?: return null
        if ((obj["t"] as? JsonPrimitive)?.isString != true) return null
        obj
    }
    return SteerEnvelope(sessionId, objects)
}

private fun frameTag(frame: JsonObject): String? = (frame["t"] as? JsonPrimitive)?.contentOrNull

private val JOIN = buildJsonObject {
    put("t", "join")
    put("channel", "activity")
}
private val LEAVE = buildJsonObject { put("t", "leave") }

/**
 * EXP-1487 SUBSCRIPTIONS: the open [EnetSteerSocket]s per (viewer node,
 * session, device endpoint). The device keys viewers by endpoint, so a
 * `leave` ends EVERY feed of that pair on this install: only the last open
 * socket of a pair sends it (the desktop's `ViewerSubscription::shared`).
 */
internal object EnetSteerSubscribers {
    private val open = mutableMapOf<Triple<String, String, String>, MutableSet<EnetSteerSocket>>()

    fun add(key: Triple<String, String, String>, socket: EnetSteerSocket) = synchronized(open) {
        open.getOrPut(key) { mutableSetOf() }.add(socket)
    }

    /** Drops [socket]; true = no other socket of [key] is open. */
    fun remove(key: Triple<String, String, String>, socket: EnetSteerSocket): Boolean = synchronized(open) {
        val sockets = open[key] ?: return true
        sockets.remove(socket)
        if (sockets.isEmpty()) open.remove(key)
        sockets.isEmpty()
    }
}

/** How a join probe of one endpoint ended. */
internal enum class EnetJoin { Joined, NoSuchSession, Refused, SendFailed, Timeout }

/**
 * One viewer feed from the install at [endpoint] for [sessionId]. Applies
 * only envelopes of that session from that endpoint (the VIEWER SIDE rule);
 * a `bye` or an `error` ends it like the relay's close after one.
 */
internal class EnetSteerSocket(
    private val node: PeerNode,
    private val sessionId: String,
    val endpoint: String,
    private val timings: SteerEnetTimings,
    private val nowMs: () -> Long,
    dispatcher: CoroutineDispatcher,
) : SteerSocket {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val sendLock = Mutex()

    // Frames as they arrive; [texts] is what the connection reads once joined.
    private val inbox = Channel<String>(Channel.UNLIMITED)
    private val texts = Channel<String>(Channel.UNLIMITED)

    @Volatile private var lastFrameAtMs = nowMs()
    @Volatile private var closed = false
    @Volatile private var reason: String? = null

    // The probe already joined: the connection's own join would only restart the feed.
    @Volatile private var swallowJoin = false

    private val key = Triple(node.endpointId, sessionId, endpoint)

    init {
        EnetSteerSubscribers.add(key, this)
        // UNDISPATCHED: subscribed before the probe's join can be answered.
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            node.chats.collect { chat ->
                if (chat.from != endpoint) return@collect
                val envelope = parseSteerEnvelope(chat.body.decodeToString()) ?: return@collect
                if (envelope.sessionId != sessionId) return@collect
                lastFrameAtMs = nowMs()
                for (frame in envelope.frames) {
                    inbox.trySend(frame.toString())
                    when (frameTag(frame)) {
                        "bye" -> end("bye")
                        "error" -> end("error ${(frame["code"] as? JsonPrimitive)?.contentOrNull}")
                    }
                }
            }
        }
        // EXP-1531: the last connection to the device closed: the feed is
        // gone now, not a staleMs from now (dropped exactly like a failed send).
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            node.disconnects.collect { peer -> if (peer == endpoint) end("peer disconnected") }
        }
    }

    override val incoming: ReceiveChannel<String> get() = texts

    /** EXP-1531: the node's path kind to the device (`direct` | `iroh_relay`), `direct` when it does not know. */
    override val path: String
        get() = if (node.pathKind(endpoint) == "iroh_relay") "iroh_relay" else "direct"

    /**
     * Sends `join` and holds the replay until `activity_synced`; on
     * [EnetJoin.Joined] the held frames are the first [incoming] delivers.
     */
    suspend fun probe(): EnetJoin {
        val held = mutableListOf<String>()
        try {
            sendFrames(listOf(JOIN))
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "join send to ${endpoint.take(8)} failed: ${err.javaClass.simpleName}")
            // #EXP-1678: not the device saying no: the relay may still reach it.
            return EnetJoin.SendFailed
        }
        val deadline = nowMs() + timings.joinTimeoutMs
        while (true) {
            val left = deadline - nowMs()
            if (left <= 0) return EnetJoin.Timeout
            val text = select<String?> {
                inbox.onReceiveCatching { it.getOrNull() ?: "" }
                onTimeout(left) { null }
            } ?: return EnetJoin.Timeout
            // The subscription closed under us: no answer from the device.
            if (text.isEmpty()) return EnetJoin.SendFailed
            val frame = Json.parseToJsonElement(text).jsonObject
            when (frameTag(frame)) {
                // Ended here, not only in the collector: it may not have run
                // yet, and [cancel] must not send `leave` to a refusal.
                "error" -> {
                    end("error")
                    return if ((frame["code"] as? JsonPrimitive)?.contentOrNull == "no_such_session") {
                        EnetJoin.NoSuchSession
                    } else {
                        EnetJoin.Refused
                    }
                }
                "bye" -> {
                    end("bye")
                    return EnetJoin.Refused
                }
                "activity_synced" -> {
                    held += text
                    held.forEach { texts.trySend(it) }
                    swallowJoin = true
                    scope.launch {
                        // cancel() closes [texts] while [inbox] may still hold
                        // frames: a send then threw ClosedSendChannelException
                        // into this handler-less scope and killed the app.
                        for (next in inbox) if (texts.trySend(next).isClosed) break
                        texts.close()
                    }
                    scope.launch { watchStale() }
                    return EnetJoin.Joined
                }
                else -> held += text
            }
        }
    }

    /** No frame for staleMs = re-send `join`; still nothing a staleMs later = the feed is gone. */
    private suspend fun watchStale() {
        var rejoinedAt: Long? = null
        while (!closed) {
            delay((timings.staleMs / 4).coerceAtLeast(1))
            val now = nowMs()
            val sentAt = rejoinedAt
            if (sentAt != null && lastFrameAtMs > sentAt) rejoinedAt = null
            if (now - lastFrameAtMs < timings.staleMs) continue
            if (rejoinedAt != null) {
                if (now - rejoinedAt >= timings.staleMs) end("stale")
                continue
            }
            Log.i(TAG, "${endpoint.take(8)} silent for ${now - lastFrameAtMs}ms: re-join")
            rejoinedAt = now
            runCatching { sendFrames(listOf(JOIN)) }
        }
    }

    override suspend fun send(text: String) {
        val frame = Json.parseToJsonElement(text).jsonObject
        if (swallowJoin && frameTag(frame) == "join") {
            swallowJoin = false
            return
        }
        try {
            sendFrames(listOf(frame))
        } catch (err: Throwable) {
            // A send that failed drops this feed: the loop redials.
            if (err !is CancellationException) end("send failed")
            throw err
        }
    }

    private suspend fun sendFrames(frames: List<JsonObject>) {
        if (closed) throw IllegalStateException("eNet steer feed closed")
        val body = encodeSteerEnvelope(sessionId, frames)
        sendLock.withLock {
            withTimeoutOrNull(timings.sendTimeoutMs) { node.sendChat(endpoint, body) }
                ?: throw IllegalStateException("eNet send timed out")
        }
    }

    /** Ends the feed; true = this call ended it and it was the pair's last open socket. */
    @Synchronized
    private fun end(why: String): Boolean {
        if (closed) return false
        closed = true
        reason = why
        inbox.close()
        return EnetSteerSubscribers.remove(key, this)
    }

    override fun cancel() {
        // An ended feed was already dropped by the device, and a `leave` while
        // another socket of the pair is open would end ITS feed (EXP-1487).
        val leave = end("cancelled")
        texts.close()
        if (!leave) {
            scope.cancel()
            return
        }
        scope.launch {
            withTimeoutOrNull(2_000) {
                runCatching {
                    sendLock.withLock { node.sendChat(endpoint, encodeSteerEnvelope(sessionId, listOf(LEAVE))) }
                }
            }
            scope.cancel()
        }
    }

    override suspend fun closeReason(): String? = reason
}

/**
 * The transport choice (the fixture's DISCOVERY): `installs.forSession`, then
 * a join to each endpoint in order, the next on `no_such_session` (or a
 * refusal). Null = NO FEED (VAPP-43, no relay fallback): no node, no
 * endpoint, every endpoint refused, or no `activity_synced` within
 * joinTimeoutMs.
 */
internal class EnetSteerDialer(
    private val node: () -> PeerNode?,
    private val discover: suspend (sessionId: String) -> List<String>,
    private val timings: SteerEnetTimings = SteerEnetTimings(),
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    /** EXP-1529: why the last [open] said null (connStats `errorClasses`). */
    @Volatile var lastFailure: String? = null
        private set

    suspend fun open(sessionId: String): EnetSteerSocket? {
        lastFailure = "no_node"
        val peer = node() ?: return null
        lastFailure = "no_endpoint"
        val endpoints = try {
            withTimeoutOrNull(timings.joinTimeoutMs) { discover(sessionId) }
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "installs.forSession failed: ${err.javaClass.simpleName}")
            null
        }
        if (endpoints.isNullOrEmpty()) return null
        lastFailure = "refused"
        // #EXP-1678: `refused` only when every endpoint ANSWERED no.
        var sendFailed = false
        for (endpoint in endpoints) {
            if (endpoint == peer.endpointId) continue
            val socket = EnetSteerSocket(peer, sessionId, endpoint, timings, nowMs, dispatcher)
            val result = try {
                socket.probe()
            } catch (err: Throwable) {
                socket.cancel()
                throw err
            }
            Log.i(TAG, "join ${endpoint.take(8)}: $result")
            when (result) {
                EnetJoin.Joined -> {
                    lastFailure = null
                    return socket
                }
                EnetJoin.Timeout -> {
                    lastFailure = "join_timeout"
                    socket.cancel()
                    return null
                }
                EnetJoin.NoSuchSession, EnetJoin.Refused -> socket.cancel()
                EnetJoin.SendFailed -> {
                    sendFailed = true
                    socket.cancel()
                }
            }
        }
        if (sendFailed) lastFailure = "send_failed"
        return null
    }
}
