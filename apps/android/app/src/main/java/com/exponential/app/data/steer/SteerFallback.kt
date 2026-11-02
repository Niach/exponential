package com.exponential.app.data.steer

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject

// EXP-1529: eNet PRIMARY, the sealed relay tunnel an invisible FALLBACK
// ([steerCarrierStep], fixtures/steer-enet.json `fallback`). One
// [CarrierSteerSocket] is the feed [SteerConnection] reads: it dials eNet,
// opens the relay leg only after `stallMs` without eNet, on eNet NO FEED or
// on a drop of the active eNet feed, keeps redialing eNet in the background
// while the relay carries, and hands the feed back to eNet on its
// `activity_reset`. The connection never learns which carrier it reads: the
// takeover is one more `activity_reset` + replay, which it already stages.

private const val TAG = "SteerFallback"

/** What a view's analytics hear from the carriers (fixtures/steer-enet.json `connStats`). */
internal interface SteerCarrierListener {
    /** The relay leg opened (`fallbacks`). */
    fun carrierFallback() {}

    /** One carrier failure, by `errorClasses`. */
    fun carrierFailure(errorClass: String) {}
}

internal data class SteerFallbackTimings(
    val stallMs: Long = STEER_FALLBACK_STALL_MS,
    /** eNet's background redial cadence while it is not the active carrier. */
    val enetRedialMs: List<Long> = STEER_RELAY_RETRY_MS,
    /** How long a steering frame waits for a carrier during a switch. */
    val carrierWaitMs: Long = SteerEnetTimings().joinTimeoutMs,
)

/** eNet first, the sealed relay as its fallback ([relay] null = no relay leg on this build). */
internal class FallbackSteerTransport(
    private val enet: SteerTransport,
    private val relay: SteerTunnelDialer?,
    private val timings: SteerFallbackTimings = SteerFallbackTimings(),
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val dispatcher: CoroutineDispatcher = Dispatchers.Default,
) : SteerTransport {
    @Volatile var listener: SteerCarrierListener? = null

    /**
     * The app is in the background: Android 14+ freezes a cached process
     * ~10 s after it leaves the foreground and cuts its sockets, so a carrier
     * that fails now failed because of the OS. While paused no relay leg
     * opens, eNet does not redial, nothing counts as a failure, and a drop
     * ends the feed (the connection parks it; the foreground redials).
     */
    @Volatile var paused: Boolean = false

    override suspend fun open(codingSessionId: String): SteerSocket? {
        val socket = CarrierSteerSocket(codingSessionId, enet, relay, timings, nowMs, dispatcher, listener) { paused }
        return if (socket.start()) socket else null
    }
}

/** Failure class of a carrier socket that closed under us (fixture `errorClasses`). */
internal fun steerDropClass(carrier: SteerActiveCarrier, reason: String?): String = when {
    reason == "stale" -> "stale"
    reason == "send failed" -> "send_failed"
    reason != null && reason.startsWith("tunnel_error ") -> reason.removePrefix("tunnel_error ")
        .takeIf { it in STEER_TUNNEL_ERROR_CODES } ?: "other"
    carrier == SteerActiveCarrier.Relay -> "relay_closed"
    else -> "closed"
}

/**
 * The feed over whichever carrier [steerCarrierStep] made active. Every
 * transition runs on ONE actor coroutine (the [events] channel), so the
 * reducer state, the legs and the generation counters never race.
 */
internal class CarrierSteerSocket(
    private val sessionId: String,
    private val enet: SteerTransport,
    private val relay: SteerTunnelDialer?,
    private val timings: SteerFallbackTimings,
    private val nowMs: () -> Long,
    dispatcher: CoroutineDispatcher,
    private val listener: SteerCarrierListener?,
    private val paused: () -> Boolean = { false },
) : SteerSocket {
    private sealed interface Ev {
        data class EnetOpened(val gen: Int, val socket: SteerSocket?) : Ev
        data class RelayOpened(val gen: Int, val result: TunnelOpen) : Ev
        data class Frame(val carrier: SteerActiveCarrier, val gen: Int, val text: String) : Ev
        data class Closed(val carrier: SteerActiveCarrier, val gen: Int, val reason: String?) : Ev
        data object Start : Ev
        data object Stall : Ev
        data object RedialEnet : Ev
        data object Wake : Ev
        data object Cancel : Ev
    }

    private val sid = sessionId.take(8)
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private val events = Channel<Ev>(Channel.UNLIMITED)
    private val texts = Channel<String>(Channel.UNLIMITED)
    private val started = CompletableDeferred<Boolean>()

    // Actor-owned.
    private var state = SteerCarrierState()
    private var enetSocket: SteerSocket? = null
    private var relaySocket: SteerSocket? = null
    private var enetGen = 0
    private var relayGen = 0
    private var enetDial: Job? = null
    private var relayDial: Job? = null
    private var enetRedial: Job? = null
    private var stallJob: Job? = null
    private var enetAttempt = 0
    private var everActive = false
    private var enetFailed = false
    private var relayFailed = false
    private var finished = false

    private val activeSocket = MutableStateFlow<SteerSocket?>(null)
    @Volatile private var swallowJoin = true
    @Volatile private var reason: String? = null
    @Volatile private var downSince: Long? = null
    @Volatile private var carrier = SteerActiveCarrier.None

    override val incoming: ReceiveChannel<String> get() = texts

    override val path: String
        get() = if (carrier == SteerActiveCarrier.Relay) "server_fallback" else activeSocket.value?.path ?: "direct"

    override fun downSinceMs(): Long? = downSince

    /** Runs the rule until a carrier is active (true) or both failed: NO FEED (false). */
    suspend fun start(): Boolean {
        scope.launch {
            for (ev in events) {
                try {
                    handle(ev)
                } catch (err: CancellationException) {
                    throw err
                } catch (err: Throwable) {
                    Log.w(TAG, "[$sid] carrier step failed: ${err.message}")
                }
                if (finished) break
            }
        }
        events.trySend(Ev.Start)
        return try {
            started.await()
        } catch (err: CancellationException) {
            cancel()
            throw err
        }
    }

    override fun wake() {
        events.trySend(Ev.Wake)
    }

    override suspend fun send(text: String) {
        val tag = runCatching { (Json.parseToJsonElement(text).jsonObject["t"] as? JsonPrimitive)?.contentOrNull }.getOrNull()
        // Both legs' probes already joined: the connection's own join would only restart the feed.
        if (swallowJoin && tag == "join") {
            swallowJoin = false
            return
        }
        val socket = awaitCarrier(null) ?: throw IllegalStateException("no live carrier")
        try {
            socket.send(text)
        } catch (err: Throwable) {
            if (err is CancellationException) throw err
            // The carrier died under the send: the frame goes out on the one that takes over.
            val next = awaitCarrier(socket) ?: throw err
            next.send(text)
        }
    }

    private suspend fun awaitCarrier(not: SteerSocket?): SteerSocket? =
        activeSocket.value?.takeIf { it !== not }
            ?: withTimeoutOrNull(timings.carrierWaitMs) { activeSocket.first { it != null && it !== not } }

    override fun cancel() {
        texts.close()
        if (events.trySend(Ev.Cancel).isFailure) scope.cancel()
    }

    override suspend fun closeReason(): String? = reason

    // ── the actor ──────────────────────────────────────────────────────────

    private fun handle(ev: Ev) {
        when (ev) {
            Ev.Start -> run(steerCarrierStep(state, SteerCarrierEvent.Start))
            Ev.Stall -> run(steerCarrierStep(state, SteerCarrierEvent.Stall))
            is Ev.EnetOpened -> {
                if (ev.gen != enetGen) {
                    ev.socket?.cancel()
                    return
                }
                enetDial = null
                val socket = ev.socket
                if (socket == null) {
                    enetFailed = true
                    failure(enet.lastFailure ?: "no_endpoint")
                    run(steerCarrierStep(state, SteerCarrierEvent.EnetNoFeed))
                } else {
                    enetFailed = false
                    enetSocket = socket
                    pump(SteerActiveCarrier.Enet, enetGen, socket)
                }
            }
            is Ev.RelayOpened -> {
                if (ev.gen != relayGen) {
                    (ev.result as? TunnelOpen.Joined)?.socket?.cancel()
                    return
                }
                relayDial = null
                when (val result = ev.result) {
                    is TunnelOpen.Joined -> {
                        relaySocket = result.socket
                        pump(SteerActiveCarrier.Relay, relayGen, result.socket)
                    }
                    is TunnelOpen.Unavailable -> {
                        relayFailed = true
                        result.errorClass?.let(::failure)
                        run(steerCarrierStep(state, SteerCarrierEvent.RelayUnavailable))
                    }
                    is TunnelOpen.Failed -> {
                        relayFailed = true
                        failure(result.errorClass)
                        run(steerCarrierStep(state, SteerCarrierEvent.RelayDropped))
                    }
                }
            }
            is Ev.Frame -> {
                val current = if (ev.carrier == SteerActiveCarrier.Enet) enetGen else relayGen
                if (ev.gen != current) return
                val tag = runCatching {
                    (Json.parseToJsonElement(ev.text).jsonObject["t"] as? JsonPrimitive)?.contentOrNull
                }.getOrNull()
                val reset = tag == "activity_reset"
                val event = when (ev.carrier) {
                    SteerActiveCarrier.Enet -> if (reset) SteerCarrierEvent.EnetReset else SteerCarrierEvent.EnetSynced
                    else -> if (reset) SteerCarrierEvent.RelayReset else SteerCarrierEvent.RelaySynced
                }
                val step = steerCarrierStep(state, event)
                val apply = SteerCarrierAction.Apply in step.actions
                if (apply) texts.trySend(ev.text)
                run(step)
                // `bye` / `error` from the active carrier end the feed like the socket they came on.
                if (apply && (tag == "bye" || tag == "error")) finish(tag)
            }
            is Ev.Closed -> {
                val current = if (ev.carrier == SteerActiveCarrier.Enet) enetGen else relayGen
                if (ev.gen != current || finished) return
                if (ev.carrier == SteerActiveCarrier.Enet) {
                    enetSocket?.cancel()
                    enetSocket = null
                    enetGen++
                } else {
                    relaySocket?.cancel()
                    relaySocket = null
                    relayGen++
                }
                reason = ev.reason
                failure(steerDropClass(ev.carrier, ev.reason))
                Log.i(TAG, "[$sid] ${ev.carrier.wire} dropped (active=${state.active.wire})")
                val event = if (ev.carrier == SteerActiveCarrier.Enet) SteerCarrierEvent.EnetDropped else SteerCarrierEvent.RelayDropped
                run(steerCarrierStep(state, event))
            }
            Ev.RedialEnet -> {
                enetRedial = null
                if (paused()) return
                if (state.active != SteerActiveCarrier.Enet && enetDial == null && enetSocket == null) dialEnet()
            }
            Ev.Wake -> {
                // Foreground: re-evaluate eNet at once instead of sitting out its backoff.
                if (state.active != SteerActiveCarrier.Enet && enetDial == null && enetSocket == null) {
                    enetRedial?.cancel()
                    enetRedial = null
                    enetAttempt = 0
                    dialEnet()
                }
            }
            Ev.Cancel -> finish("cancelled")
        }
    }

    private fun run(step: SteerCarrierStep) {
        state = step.state
        for (action in step.actions) {
            when (action) {
                SteerCarrierAction.DialEnet -> dialEnet()
                SteerCarrierAction.ArmStall -> stallJob = scope.launch {
                    delay(timings.stallMs)
                    events.send(Ev.Stall)
                }
                SteerCarrierAction.OpenRelay -> openRelay()
                SteerCarrierAction.RejoinRelay -> {
                    // The stand-by leg's frames were dropped: a fresh join replays from the top.
                    if (relaySocket != null || relayDial == null) {
                        closeRelayLeg()
                        openRelay()
                    }
                }
                SteerCarrierAction.CloseRelay -> closeRelayLeg()
                SteerCarrierAction.RedialEnetLater -> redialEnetLater()
                // The connection's backoff redials both legs (the feed ends below).
                SteerCarrierAction.ReopenRelayLater,
                SteerCarrierAction.Apply,
                SteerCarrierAction.Drop -> Unit
            }
            if (finished) return
        }
        settle()
    }

    /** After every step: who carries, when the feed went down, and whether it is over. */
    private fun settle() {
        if (finished) return
        val active = state.active
        carrier = active
        activeSocket.value = when (active) {
            SteerActiveCarrier.Enet -> enetSocket
            SteerActiveCarrier.Relay -> relaySocket
            SteerActiveCarrier.None -> null
        }
        if (active != SteerActiveCarrier.None) {
            downSince = null
            if (active == SteerActiveCarrier.Enet) {
                enetAttempt = 0
                enetRedial?.cancel()
                enetRedial = null
            }
            if (!everActive) {
                everActive = true
                stallJob?.cancel()
                started.complete(true)
            }
            return
        }
        if (everActive) {
            if (downSince == null) downSince = nowMs()
            // Neither carrier, none opening: the connection's own drop rule takes it from here.
            if (state.relay == SteerRelayLeg.Closed) finish("lost")
            return
        }
        // Before the first frame: NO FEED once eNet failed and the relay leg is closed.
        if (enetFailed && state.relay == SteerRelayLeg.Closed && enetDial == null) finish("no feed")
    }

    private fun dialEnet() {
        if (enetDial != null) return
        enetSocket?.cancel()
        enetSocket = null
        val gen = ++enetGen
        enetDial = scope.launch {
            val socket = try {
                enet.open(sessionId)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                Log.w(TAG, "[$sid] eNet dial failed: ${err.message}")
                null
            }
            if (events.trySend(Ev.EnetOpened(gen, socket)).isFailure) socket?.cancel()
        }
    }

    /** One carrier failure for the analytics; none while [paused] (the OS cut it). */
    private fun failure(errorClass: String) {
        if (!paused()) listener?.carrierFailure(errorClass)
    }

    private fun openRelay() {
        val dialer = relay
        if (dialer == null || paused() || (!everActive && relayFailed)) {
            // No relay leg on this install, the app is in the background, or
            // it already failed for this open: eNet only.
            run(steerCarrierStep(state, SteerCarrierEvent.RelayUnavailable))
            return
        }
        if (relayDial != null || relaySocket != null) return
        listener?.carrierFallback()
        Log.i(TAG, "[$sid] relay fallback opens")
        val gen = ++relayGen
        relayDial = scope.launch {
            val result = try {
                dialer.open(sessionId)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                TunnelOpen.Failed("other")
            }
            if (events.trySend(Ev.RelayOpened(gen, result)).isFailure) (result as? TunnelOpen.Joined)?.socket?.cancel()
        }
    }

    private fun closeRelayLeg() {
        relayDial?.cancel()
        relayDial = null
        relaySocket?.cancel()
        relaySocket = null
        relayGen++
    }

    private fun redialEnetLater() {
        if (enetRedial != null || enetDial != null || paused()) return
        val delays = timings.enetRedialMs
        val wait = delays[minOf(enetAttempt, delays.size - 1)]
        enetAttempt++
        enetRedial = scope.launch {
            delay(wait)
            events.send(Ev.RedialEnet)
        }
    }

    private fun pump(carrier: SteerActiveCarrier, gen: Int, socket: SteerSocket) {
        scope.launch {
            try {
                for (text in socket.incoming) events.send(Ev.Frame(carrier, gen, text))
            } catch (err: Throwable) {
                if (err is CancellationException) throw err
            }
            events.trySend(Ev.Closed(carrier, gen, runCatching { socket.closeReason() }.getOrNull()))
        }
    }

    private fun finish(why: String) {
        if (finished) return
        finished = true
        if (reason == null) reason = why
        activeSocket.value = null
        texts.close()
        started.complete(false)
        enetDial?.cancel()
        enetSocket?.cancel()
        relayDial?.cancel()
        relaySocket?.cancel()
        enetGen++
        relayGen++
        // Every leg's own cancel sends its `leave` on its own scope.
        scope.cancel()
    }
}
