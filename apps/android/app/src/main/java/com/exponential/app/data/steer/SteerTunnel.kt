package com.exponential.app.data.steer

import android.util.Log
import com.exponential.app.data.sealed.SealStatus
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedCrypto
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
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
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// EXP-1529: the sealed relay TUNNEL, eNet's fallback carrier
// (fixtures/steer-enet.json `fallback`). The same exp-steer envelopes ride a
// relay WebSocket instead of an eNet chat stream, every one SEALED to the
// session tree key (fixtures/sealed-content.json): the relay sees the
// sessionId, the tunnel id and ciphertext, nothing else (EXP-1457 intact).
// [TunnelSteerSocket] is a [SteerSocket] exactly like [EnetSteerSocket]:
// the join replay is held until `activity_synced`.

private const val TAG = "SteerTunnel"

/** One relay WebSocket: TEXT frames as they arrive; [incoming] closes with the socket. */
internal interface TunnelLink {
    val incoming: ReceiveChannel<String>
    suspend fun send(text: String)
    fun close()
}

/** Dials the ticket's full `ws(s)://…?ticket=` URL (60 s connect window). */
internal fun interface TunnelWire {
    suspend fun connect(url: String): TunnelLink
}

/** The production wire: the ping-free steer client (OkHttp engine, EXP-656). */
internal class KtorTunnelWire(private val client: HttpClient) : TunnelWire {
    override suspend fun connect(url: String): TunnelLink {
        val session = client.webSocketSession(urlString = url)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val texts = Channel<String>(Channel.UNLIMITED)
        scope.launch {
            try {
                for (frame in session.incoming) if (frame is Frame.Text) texts.send(frame.readText())
            } catch (err: Throwable) {
                if (err is CancellationException) throw err
            } finally {
                texts.close()
            }
        }
        return object : TunnelLink {
            override val incoming: ReceiveChannel<String> = texts
            override suspend fun send(text: String) = session.send(Frame.Text(text))
            override fun close() {
                scope.launch {
                    runCatching { session.close() }
                    scope.cancel()
                }
            }
        }
    }
}

/**
 * Seals / opens one envelope for the tunnel: `data` = the sealed-value
 * grammar after the body prefix, `v1:<epoch>:<b64url push_seal(...)>`.
 */
internal interface SteerSealer {
    fun seal(envelopeJson: String): String
    /** The envelope JSON, or null when [data] does not open (noKey, refused, malformed, unsupported). */
    fun open(data: String): String?
}

/** The tree key of the session's ROOT: its public key + current epoch, the secrets this install holds. */
internal class TreeSteerSealer(
    private val crypto: SealedCrypto,
    private val publicKey: String,
    private val epoch: Int,
    private val keys: () -> Map<Int, ByteArray>,
) : SteerSealer {
    override fun seal(envelopeJson: String): String =
        crypto.sealBody(publicKey, epoch, envelopeJson).removePrefix(SealedContent.BODY_PREFIX)

    override fun open(data: String): String? = runCatching {
        crypto.openBody(SealedContent.BODY_PREFIX + data, keys()).takeIf { it.status == SealStatus.Opened }?.text
    }.getOrNull()
}

/** `steer.tunnelTicket`: what the server answered. */
internal sealed interface TunnelTicket {
    /** No relay on this instance: eNet only, never a failure. */
    data object Disabled : TunnelTicket

    /** The host device is offline on the relay or announced no `tunnel: 1`. */
    data object Unavailable : TunnelTicket
    data class Granted(val ticket: String, val url: String) : TunnelTicket
}

/** How a join probe over the tunnel ended. */
internal sealed interface TunnelJoin {
    data object Joined : TunnelJoin
    data class Failed(val errorClass: String) : TunnelJoin

    /** `tunnel_error` (device_offline | device_unsupported): no relay leg. */
    data class Unavailable(val code: String) : TunnelJoin
}

/**
 * One viewer feed over the tunnel for [sessionId]. Applies only envelopes
 * that open AND parse as envelopes of this session; a `bye` or an `error`
 * ends it like on eNet, a `tunnel_error` or the socket closing ends it too.
 */
internal class TunnelSteerSocket(
    private val link: TunnelLink,
    private val sessionId: String,
    private val sealer: SteerSealer,
    private val timings: SteerEnetTimings,
    private val nowMs: () -> Long,
    dispatcher: CoroutineDispatcher,
) : SteerSocket {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val sendLock = Mutex()
    private val inbox = Channel<String>(Channel.UNLIMITED)
    private val texts = Channel<String>(Channel.UNLIMITED)

    @Volatile private var lastFrameAtMs = nowMs()
    @Volatile private var closed = false
    @Volatile private var reason: String? = null
    @Volatile private var tunnelError: String? = null
    @Volatile private var swallowJoin = false

    init {
        scope.launch {
            for (raw in link.incoming) {
                val obj = runCatching { Json.parseToJsonElement(raw).jsonObject }.getOrNull() ?: continue
                when ((obj["t"] as? JsonPrimitive)?.contentOrNull) {
                    "sealed" -> {
                        val data = (obj["data"] as? JsonPrimitive)?.contentOrNull ?: continue
                        if (data.length > STEER_TUNNEL_MAX_DATA_BYTES) continue
                        val envelope = sealer.open(data)?.let(::parseSteerEnvelope) ?: continue
                        if (envelope.sessionId != sessionId) continue
                        lastFrameAtMs = nowMs()
                        for (frame in envelope.frames) {
                            inbox.trySend(frame.toString())
                            when ((frame["t"] as? JsonPrimitive)?.contentOrNull) {
                                "bye" -> end("bye")
                                "error" -> end("error")
                            }
                        }
                    }
                    "tunnel_error" -> {
                        tunnelError = (obj["code"] as? JsonPrimitive)?.contentOrNull ?: "device_offline"
                        end("tunnel_error $tunnelError")
                    }
                }
            }
            end("relay_closed")
        }
    }

    override val incoming: ReceiveChannel<String> get() = texts

    /** Sends `join` and holds the replay until `activity_synced` (bounded by joinTimeoutMs). */
    suspend fun probe(): TunnelJoin {
        val held = mutableListOf<String>()
        try {
            sendFrames(listOf(JOIN))
        } catch (err: CancellationException) {
            throw err
        } catch (_: Throwable) {
            return TunnelJoin.Failed("send_failed")
        }
        val deadline = nowMs() + timings.joinTimeoutMs
        while (true) {
            val left = deadline - nowMs()
            if (left <= 0) return TunnelJoin.Failed("join_timeout")
            val text = select<String?> {
                inbox.onReceiveCatching { it.getOrNull() ?: "" }
                onTimeout(left) { null }
            } ?: return TunnelJoin.Failed("join_timeout")
            if (text.isEmpty()) {
                return tunnelError?.let {
                    if (it in STEER_TUNNEL_ERROR_CODES) TunnelJoin.Unavailable(it) else TunnelJoin.Failed("other")
                } ?: TunnelJoin.Failed("relay_closed")
            }
            val frame = Json.parseToJsonElement(text).jsonObject
            when ((frame["t"] as? JsonPrimitive)?.contentOrNull) {
                "error", "bye" -> {
                    end("refused")
                    return TunnelJoin.Failed("refused")
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
                    return TunnelJoin.Joined
                }
                else -> held += text
            }
        }
    }

    /** The eNet socket's rule: silent staleMs = re-join; still nothing a staleMs later = gone. */
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
            rejoinedAt = now
            runCatching { sendFrames(listOf(JOIN)) }
        }
    }

    override suspend fun send(text: String) {
        val frame = Json.parseToJsonElement(text).jsonObject
        if (swallowJoin && (frame["t"] as? JsonPrimitive)?.contentOrNull == "join") {
            swallowJoin = false
            return
        }
        try {
            sendFrames(listOf(frame))
        } catch (err: Throwable) {
            if (err !is CancellationException) end("send failed")
            throw err
        }
    }

    private suspend fun sendFrames(frames: List<JsonObject>) {
        if (closed) throw IllegalStateException("tunnel closed")
        val data = sealer.seal(encodeSteerEnvelope(sessionId, frames).decodeToString())
        val wire = buildJsonObject {
            put("t", "sealed")
            put("data", data)
        }.toString()
        sendLock.withLock {
            withTimeoutOrNull(timings.sendTimeoutMs) { link.send(wire) }
                ?: throw IllegalStateException("tunnel send timed out")
        }
    }

    private fun end(why: String) {
        if (closed) return
        closed = true
        reason = why
        inbox.close()
    }

    override fun cancel() {
        val wasOpen = !closed
        end("cancelled")
        texts.close()
        scope.launch {
            if (wasOpen) {
                withTimeoutOrNull(2_000) { runCatching { sendLock.withLock { link.send(sealedLeave()) } } }
            }
            link.close()
            scope.cancel()
        }
    }

    private fun sealedLeave(): String = buildJsonObject {
        put("t", "sealed")
        put("data", sealer.seal(encodeSteerEnvelope(sessionId, listOf(LEAVE)).decodeToString()))
    }.toString()

    override suspend fun closeReason(): String? = reason

    private companion object {
        val JOIN = buildJsonObject {
            put("t", "join")
            put("channel", "activity")
        }
        val LEAVE = buildJsonObject { put("t", "leave") }
    }
}

/** What opening the relay leg came to. */
internal sealed interface TunnelOpen {
    data class Joined(val socket: SteerSocket) : TunnelOpen

    /** No relay leg (keyless tree, no secret, relay disabled, device offline or older): eNet only. */
    data class Unavailable(val errorClass: String?) : TunnelOpen

    /** The leg was there but did not deliver (ticket, socket, join timeout, refusal). */
    data class Failed(val errorClass: String) : TunnelOpen
}

/** The relay leg of a viewer, behind a seam so the carrier switch is testable without a relay. */
internal fun interface SteerTunnelDialer {
    suspend fun open(sessionId: String): TunnelOpen

    /** #EXP-1678: whether this install could seal for the leg at all (no
     *  network); false = `no_key`, never counted as a fallback. */
    suspend fun hasKey(sessionId: String): Boolean = true
}

/**
 * AVAILABILITY (the fixture): the ROOT has a seal key AND this install holds
 * the tree secret of its current epoch ([sealer] non-null) AND
 * `steer.tunnelTicket` grants a ticket. Then the join, bounded like eNet's.
 */
internal class RelayTunnelDialer(
    private val sealer: suspend (sessionId: String) -> SteerSealer?,
    private val ticket: suspend (sessionId: String) -> TunnelTicket,
    private val wire: TunnelWire,
    private val timings: SteerEnetTimings = SteerEnetTimings(),
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : SteerTunnelDialer {
    override suspend fun hasKey(sessionId: String): Boolean = try {
        sealer(sessionId) != null
    } catch (err: CancellationException) {
        throw err
    } catch (_: Throwable) {
        false
    }

    override suspend fun open(sessionId: String): TunnelOpen {
        val seal = try {
            sealer(sessionId)
        } catch (err: CancellationException) {
            throw err
        } catch (_: Throwable) {
            null
        } ?: return TunnelOpen.Unavailable("no_key")
        val granted = try {
            withTimeoutOrNull(timings.joinTimeoutMs) { ticket(sessionId) } ?: return TunnelOpen.Failed("ticket_failed")
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "tunnelTicket failed: ${err.javaClass.simpleName}")
            return TunnelOpen.Failed("ticket_failed")
        }
        val url = when (granted) {
            TunnelTicket.Disabled -> return TunnelOpen.Unavailable(null)
            TunnelTicket.Unavailable -> return TunnelOpen.Unavailable("device_offline")
            is TunnelTicket.Granted -> granted.url
        }
        val link = try {
            withTimeoutOrNull(timings.joinTimeoutMs) { wire.connect(url) } ?: return TunnelOpen.Failed("relay_closed")
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "tunnel dial failed: ${err.javaClass.simpleName}")
            return TunnelOpen.Failed("relay_closed")
        }
        val socket = TunnelSteerSocket(link, sessionId, seal, timings, nowMs, dispatcher)
        val result = try {
            socket.probe()
        } catch (err: Throwable) {
            socket.cancel()
            throw err
        }
        Log.i(TAG, "tunnel join: $result")
        return when (result) {
            TunnelJoin.Joined -> TunnelOpen.Joined(socket)
            is TunnelJoin.Unavailable -> {
                socket.cancel()
                TunnelOpen.Unavailable(result.code)
            }
            is TunnelJoin.Failed -> {
                socket.cancel()
                TunnelOpen.Failed(result.errorClass)
            }
        }
    }
}
