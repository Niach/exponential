package com.exponential.app.data.steer

import android.net.Uri
import android.os.SystemClock
import android.util.Log
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.INLINE_IMAGE_CONTENT_TYPES
import com.exponential.app.domain.MAX_IMAGE_UPLOAD_BYTES
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.appendUserMessage
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.buildSteerImageMessage
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.failUnacknowledged
import com.exponential.app.domain.lockAnswer
import com.exponential.app.domain.locksCard
import io.ktor.http.HttpStatusCode
import kotlin.math.pow
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ChannelResult
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.onTimeout
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the mobile
// "Agent session" chat view; apps/steer-relay/src/protocol.ts): the socket
// joins with {"t":"join","channel":"activity"} and receives scrubbed
// {t:'activity', event} frames (narration / tool headlines / questions /
// subagents / worktree diffs). The PTY mirror is gone (EXP-249) — a stray
// BINARY frame from an old desktop is dropped by the transport. Steering is
// message-shaped and fully seamless (EXP-312 — no operator claim, no
// view/steer perm split; the ticket mint is owner-only, so a live connection
// just steers): chunked input + a separate \r; answers are ALWAYS semantic
// {t:'answer'} frames keyed by the card's wire id (EXP-672 retired the
// raw-keystroke fallback — an id-less card from a pre-EXP-249 desktop renders
// read-only).
//
// EXP-621: all of this lives in an app singleton keyed by coding session
// ([SteerConnectionStore]), NOT in the screen's ViewModel — the socket, the
// feed and the composer draft have to survive a back-navigation and a
// rotation, so reopening a session re-attaches to what is already running
// instead of replaying the whole log behind a "Connecting…" spinner.
//
// EXP-625: the dial loop is the connection's only source of truth about
// whether it has a future. Every wait it takes is BOUNDED and interruptible,
// nothing but our own cancellation may end it, and the single revival entry
// [SteerConnection.kick] asks the loop (not the phase) what to do. A
// connection stuck on "Connecting…" after a background trip was a loop that
// had silently died under a phase no revival path would touch.

private const val TAG = "SteerConnection"

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
private const val INPUT_CHUNK_CHARS = 4096

/** How long an unacknowledged answer keeps its card locked (EXP-249): the
 *  desktop confirms injection with `answer_ack`, and a silently dropped frame
 *  must not strand the card as un-answerable forever. Derived from the
 *  desktop's worst-case ack budget (EXP-347): ANSWER_RETRY_TTL 4s +
 *  ANSWER_SETTLE 2s + PLAN_SUBMIT_PROBE 0.5s + ~1.5s tick/relay margin —
 *  web/iOS parity, move all three in lockstep. */
private const val ANSWER_ACK_TIMEOUT_MS = 8_000L

/** Shown when the session's row no longer exists — a swept row (or one that
 *  left this client's sync scope) is over as far as any client can tell, and
 *  nothing about it is retryable. */
private const val SESSION_GONE_DETAIL = "This session is no longer available."

/** Shown when a dial gets no answer at all: the relay ALWAYS answers a join
 *  (activity_reset + replay, or an error frame then close), so silence means a
 *  dead socket, not a slow one. */
private const val NO_ANSWER_DETAIL = "The live relay didn't answer."

/** Echo-FIFO bounds (EXP-78): a mid-turn steered message can take a while to
 *  hit the transcript, but an unmatched echo must not swallow an identical
 *  message sent much later. */
private const val ECHO_CAP = 8
private const val ECHO_TTL_MS = 300_000L

/**
 * Every wait the dial loop takes (EXP-625). All of them are bounded: an
 * unbounded one is how a viewer ends up parked on "Connecting…" forever.
 *
 * [joinAckMs] and [upgradeMs] exist because ktor's HttpTimeout does not cover
 * a websocket upgrade or a socket that upgrades and then says nothing.
 * [liveStaleMs] is how long a nominally-live socket may stay silent before a
 * revival kick treats it as dead. There is no pong to probe with, so the
 * probe is a silent redial. The relay sends every joined viewer a
 * `keepalive` frame every 15s (EXP-648), so 45s is three missed ticks: a
 * quiet agent (parked on a question or a plan approval) no longer reads as
 * a dead socket.
 */
data class SteerTimings(
    /** Redial cadence while the desktop's publisher socket is still starting. */
    val startingRetryMs: Long = 3_000,
    /** Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
     *  exponential 3s→30s, mirroring the web viewer's starting retry — the
     *  jitter desyncs a herd of viewers all foregrounding at once. */
    val reconnectBaseMs: Long = 3_000,
    val reconnectMaxMs: Long = 30_000,
    val joinAckMs: Long = 15_000,
    val upgradeMs: Long = 20_000,
    /** 3x the relay's viewer keepalive interval (apps/steer-relay/src/hub.ts). */
    val liveStaleMs: Long = 45_000,
    /**
     * EXP-656: how long a staged join replay may stay quiet before it is
     * committed anyway. The relay marks the end of its replay with
     * `activity_synced`, so this only covers a publisher-driven republish (old
     * desktops give no end marker) — long enough that a chunky replay does not
     * commit in halves, short enough that the feed is never visibly late.
     */
    val replayQuietMs: Long = 400,
    /** Hard cap on a staged replay: a stream that never goes quiet commits
     *  what it has (and then appends normally) rather than staging forever. */
    val replayMaxMs: Long = 3_000,
)

/**
 * One live viewer connection to a coding session's relay room.
 *
 * Created and owned by [SteerConnectionStore]; the screen's ViewModel is a
 * façade over it. Everything the user would hate to lose on a back-tap lives
 * here: the socket, the activity feed, the pending images and the composer
 * draft.
 */
class SteerConnection internal constructor(
    val codingSessionId: String,
    private val transport: SteerTransport,
    /** The synced coding_sessions row, already account-scoped by the store. */
    sessionFlow: Flow<CodingSessionEntity?>,
    private val json: Json,
    /** Only the image path needs these two. They default to null so the
     *  dial-loop tests can build a connection without an Android-backed
     *  account store: such a connection refuses image sends and behaves
     *  identically in every other respect. */
    private val issueImagesApi: IssueImagesApi? = null,
    private val auth: AuthRepository? = null,
    dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
    /** Monotonic clock (SystemClock.elapsedRealtime in production), injected
     *  because the framework class returns a constant 0 in JVM tests. */
    private val nowMs: () -> Long = { SystemClock.elapsedRealtime() },
    private val timings: SteerTimings = SteerTimings(),
) {

    /** The connection's own lifetime — outlives every screen that attaches. */
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)

    /** Log prefix: enough of the session id to follow one connection through
     *  a log without dumping the whole id on every line. */
    private val sid = codingSessionId.take(8)

    /** Screens currently attached (EXP-621). Read and written only under the
     *  store's lock, which is what decides when a connection may be reaped. */
    internal var refCount = 0

    /** The synced coding_sessions row — flips to ended via Electric. Shared
     *  EAGERLY: the redial loop reads it while no screen is subscribed, and a
     *  frozen row would keep it dialing a session that already finished. */
    val session: StateFlow<CodingSessionEntity?> =
        sessionFlow.stateIn(scope, SharingStarted.Eagerly, null)

    private val _phase = MutableStateFlow<AgentPhase>(AgentPhase.Idle)
    val phase: StateFlow<AgentPhase> = _phase

    /**
     * Whether a socket is actually up right now — true from the join frame
     * until the socket goes away, independent of [phase].
     *
     * The two differ during the silent 4008 redial (a slow-consumer eviction
     * redials at once and deliberately HOLDS the Live phase, so the header
     * doesn't flicker). The composer needs the honest answer: without it its
     * send button stayed enabled over a dead socket and a tap no-oped in
     * silence.
     */
    private val _connected = MutableStateFlow(false)
    val connected: StateFlow<Boolean> = _connected

    // The feed survives reconnects, navigation and the session end. Since
    // EXP-656 an `activity_reset` no longer empties it either: the replay that
    // follows is STAGED and swapped in as one emission, so a reader parked in
    // the middle of a plan never sees the list collapse (which is what used to
    // dispose the LazyColumn's scroll state and yank them to the bottom).
    private val _activity = MutableStateFlow(ActivityFeedState())
    val activity: StateFlow<ActivityFeedState> = _activity

    /**
     * The replay being staged, or null when nothing is staging (EXP-656). Holds
     * each `{t:'activity'}` frame's `event` payload, in arrival order; the
     * committed feed is folded from them in one go.
     */
    private var stagedFrames: MutableList<JsonObject>? = null

    /** Messages this client sent WHILE staging: the replay predates them, so
     *  they are re-appended after the fold or they'd vanish on commit. */
    private val stagedLocalEchoes = mutableListOf<String>()

    /** Commit deadlines: [SteerTimings.replayQuietMs] since the last staged
     *  frame, and [SteerTimings.replayMaxMs] since the reset. */
    private var stageQuietJob: Job? = null
    private var stageCapJob: Job? = null
    private var stageStartedAtMs = 0L

    /** Images picked for the next steer message (EXP-511), capped at
     *  [MAX_STEER_IMAGES]; uploaded to the session's issue on send. */
    private val _pendingImages = MutableStateFlow<List<PendingAttachment>>(emptyList())
    val pendingImages: StateFlow<List<PendingAttachment>> = _pendingImages

    /** True while the pending images upload — the composer disables sending. */
    private val _steerSending = MutableStateFlow(false)
    val steerSending: StateFlow<Boolean> = _steerSending

    /** A rejected pick or a failed upload — rendered as a banner. */
    private val _steerImageError = MutableStateFlow<String?>(null)
    val steerImageError: StateFlow<String?> = _steerImageError

    /** The composer's typed text (EXP-621) — it lives with the connection, so
     *  a half-written message survives a reconnect, a back-tap and a rotation,
     *  and is dropped ONLY once the message actually goes out. */
    private val _draft = MutableStateFlow("")
    val draft: StateFlow<String> = _draft

    /** Pending ack deadlines, one per locked card. */
    private val ackTimeouts = mutableMapOf<String, Job>()

    /** Locally-echoed sent messages awaiting their transcript-derived
     *  `user_message` event (EXP-78 dedupe): text → sent-at millis. */
    private val recentEchoes = ArrayDeque<Pair<String, Long>>()
    private var ws: SteerSocket? = null
    private var connectJob: Job? = null

    /** Torn down for good by [close]: the scope is gone, so nothing may
     *  pretend this connection can still be revived (EXP-625). */
    private var closed = false

    /** Monotonic stamp of the last frame received on the current socket, the
     *  only liveness signal this engine offers (EXP-625: an outgoing Ping is
     *  not a probe here, it is a self-inflicted 1011 close). */
    private var lastFrameAtMs = 0L

    /** Wake-ups for the loop's retry waits (EXP-625). CONFLATED, like the
     *  shape loops' kicks: several kicks landing during one wait collapse into
     *  the one thing the loop needs to know: redial now. */
    private val wake = Channel<Unit>(Channel.CONFLATED)

    /** Consecutive failed reconnect dials — indexes the backoff curve; reset
     *  on a successful (live) connection and on an explicit connect(). */
    private var reconnectAttempts = 0

    /** Set once the synced row has actually been observed — [session] starts
     *  null (nothing loaded yet), so only a null AFTER a real row proves the
     *  row is gone rather than still on its way. */
    private var sawSessionRow = false

    /** Nothing left to dial: the store may reap an unreferenced connection in
     *  this state instead of holding its feed forever. */
    internal val isFinished: Boolean
        get() = closed ||
            _phase.value.let { it is AgentPhase.Ended || (it is AgentPhase.Closed && !it.reconnecting) }

    fun setDraft(text: String) {
        _draft.value = text
    }

    /**
     * The ONE revival entry (EXP-625): a screen attaching, the app coming
     * back, a sync kick. What it does is decided by [steerRevivalAction] off
     * the LOOP's liveness rather than the phase. The old phase-gated pair
     * (connectIfIdle + reconnectNow) could not touch a connection whose dial
     * had died while it read Connecting, which is exactly the state a
     * backgrounded viewer came back to.
     */
    fun kick(reason: String) {
        val dialActive = connectJob?.isActive == true
        val sinceFrame = nowMs() - lastFrameAtMs
        val stale = _connected.value && sinceFrame > timings.liveStaleMs
        val action = steerRevivalAction(
            phase = _phase.value,
            dialActive = dialActive,
            finished = isFinished,
            socketStale = stale,
        )
        // sinceFrame is the number the "it reconnects every 30s" reports turn
        // on (EXP-656): a stale verdict is only ever as good as it.
        Log.i(
            TAG,
            "[$sid] kick($reason) phase=${_phase.value} dial=$dialActive " +
                "stale=$stale sinceFrame=${sinceFrame}ms -> $action",
        )
        when (action) {
            SteerRevivalAction.Dial -> connect()
            SteerRevivalAction.WakeRetry -> wake.trySend(Unit)
            SteerRevivalAction.RedialSilently -> connect(silent = true)
            SteerRevivalAction.Nothing -> Unit
        }
    }

    /**
     * Start a fresh dial loop, replacing any running one.
     *
     * [silent] seeds the loop's silent flag so an externally-triggered redial
     * reuses the 4008 mechanics: dial at once and hold the current phase, Live
     * included. A liveness redial of a socket that turns out to be fine must
     * not flash "Reconnecting…" at the user.
     */
    fun connect(silent: Boolean = false) {
        if (closed) return
        connectJob?.cancel()
        reconnectAttempts = 0
        Log.i(TAG, "[$sid] connect(silent=$silent)")
        connectJob = scope.launch {
            val me = currentCoroutineContext()[Job]
            // Set by a slow-consumer close (EXP-621) or by a silent revival:
            // the next dial must not touch the phase.
            var quiet = silent
            var dials = 0
            while (isActive) {
                // A dial must never take the loop down with it (EXP-625): the
                // only throw that ends this loop is our own cancellation.
                val outcome = try {
                    dialOnce(quiet, ++dials)
                } catch (t: Throwable) {
                    ensureActive()
                    Log.w(TAG, "[$sid] dial failed: ${t.message}")
                    DialOutcome.Closed(t.message)
                }
                quiet = false
                when (outcome) {
                    DialOutcome.RetryStarting -> {
                        // The desktop hasn't published the room yet. Keep
                        // redialing (fresh ticket each time) while the synced
                        // row still says running.
                        setPhase(AgentPhase.Starting, "no_such_session")
                        delayOrWake(timings.startingRetryMs)
                        if (connectJob !== me) return@launch
                        if (sessionIsOver()) {
                            setPhase(AgentPhase.Ended(), "row ended")
                            return@launch
                        }
                    }
                    // The relay dropped a viewer that fell behind its send
                    // buffer (4008). The room is untouched, so redial straight
                    // away: no delay, no attempt counted, and the phase is left
                    // exactly as it was — a busy agent must not flap the
                    // "Connection lost" banner every few seconds (EXP-621).
                    DialOutcome.RedialNow -> quiet = true
                    is DialOutcome.Ended -> {
                        setPhase(AgentPhase.Ended(outcome.detail), "ended")
                        return@launch
                    }
                    is DialOutcome.Closed -> {
                        if (!outcome.retryable) {
                            setPhase(AgentPhase.Closed(outcome.detail), "terminal")
                            return@launch
                        }
                        // Never park on a dead socket behind a manual button
                        // (EXP-243) — auto-redial on backoff; the phase
                        // carries the reconnecting flag so the UI shows
                        // "Reconnecting…" instead of a Reconnect action.
                        setPhase(AgentPhase.Closed(outcome.detail, reconnecting = true), "drop")
                        delayOrWake(reconnectDelayMs(reconnectAttempts++))
                        if (connectJob !== me) return@launch
                        if (sessionIsOver()) {
                            setPhase(AgentPhase.Ended(), "row ended")
                            return@launch
                        }
                    }
                }
            }
        }
    }

    /**
     * Background park (EXP-621): drop the socket, keep everything the user can
     * see — feed, draft, pending images. A parked connection reads as Idle
     * with no live loop, so [kick] revives it through the ordinary dial path;
     * a finished one has nothing to park.
     */
    internal fun park() {
        if (isFinished) return
        Log.i(TAG, "[$sid] park")
        connectJob?.cancel()
        connectJob = null
        runCatching { ws?.cancel() }
        ws = null
        _connected.value = false
        discardStaging()
        setPhase(AgentPhase.Idle, "park")
    }

    /** Come back to the foreground: whatever state the connection is in, ask
     *  the revival policy what it needs. */
    internal fun resume() = kick("foreground")

    /** Tear the connection down for good — sign-out, account switch, or the
     *  store reaping a finished session nothing is looking at. */
    internal fun close() {
        val wasFinished = isFinished
        closed = true
        Log.i(TAG, "[$sid] close")
        connectJob?.cancel()
        connectJob = null
        runCatching { ws?.cancel() }
        ws = null
        _connected.value = false
        discardStaging()
        // The phase is state a ViewModel still holding this connection reads;
        // leaving it on Connecting behind a cancelled scope stranded the
        // screen on a spinner nothing could clear (EXP-625).
        if (!wasFinished) setPhase(AgentPhase.Closed(reconnecting = false), "close")
        scope.cancel()
    }

    /** Every phase write goes through here: one log line per transition is
     *  what makes a wedged viewer diagnosable from a bug report (EXP-625). */
    private fun setPhase(next: AgentPhase, why: String) {
        val prev = _phase.value
        if (prev == next) return
        Log.i(TAG, "[$sid] phase $prev -> $next ($why)")
        _phase.value = next
    }

    /** Wait [ms], or return early when [kick] wakes us. A token left over from
     *  a kick that arrived while we were dialing is drained first: it has
     *  already been served by the dial it asked for. */
    private suspend fun delayOrWake(ms: Long): Boolean {
        while (wake.tryReceive().isSuccess) { /* drain */ }
        return withTimeoutOrNull(ms) { wake.receive() } != null
    }

    /**
     * Whether the session is over as far as this client can tell: an
     * explicitly `ended` row, or a row that VANISHED (stale coding_sessions
     * rows get swept, and a row can also leave this client's sync scope). A
     * deleted row can never report `ended` itself, and that status is the only
     * other exit from the retry loops — so treating its disappearance as
     * "still running" would keep them dialing a session that no longer exists.
     */
    private fun sessionIsOver(): Boolean {
        val row = session.value
        if (row != null) {
            sawSessionRow = true
            return row.status == DomainContract.codingSessionStatusEnded
        }
        return sawSessionRow
    }

    /** Equal-jitter exponential backoff (web parity): half the capped
     *  exponential delay fixed, half random. */
    private fun reconnectDelayMs(attempt: Int): Long {
        val capped = minOf(
            timings.reconnectMaxMs.toDouble(),
            timings.reconnectBaseMs * 2.0.pow(attempt),
        )
        return (capped / 2 + Random.nextDouble() * (capped / 2)).toLong()
    }

    private sealed interface DialOutcome {
        /** no_such_session while the synced row says running — auto-retry. */
        data object RetryStarting : DialOutcome

        /** Slow-consumer eviction (4008) — redial at once, invisibly. */
        data object RedialNow : DialOutcome
        data class Ended(val detail: String? = null) : DialOutcome
        data class Closed(val detail: String? = null, val retryable: Boolean = true) : DialOutcome
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private suspend fun dialOnce(silent: Boolean, dial: Int): DialOutcome {
        // Hold the Starting / reconnecting-Closed phase steady across
        // auto-retry redials — flipping to Connecting per attempt made the
        // header flicker every ~3s while the desktop was still dialing its
        // publisher (and would flicker the reconnect banner the same way). A
        // silent (4008 or liveness) redial holds whatever phase it had, Live
        // included.
        val held = _phase.value
        if (!silent && held != AgentPhase.Starting && !(held is AgentPhase.Closed && held.reconnecting)) {
            setPhase(AgentPhase.Connecting, "dial#$dial")
        }

        // `bye` / no_such_session must win over the generic close handler.
        var sawEnd = false
        var retryStarting = false
        var detail: String? = null
        // A server "no" that can never turn into a yes — wins over everything.
        var terminal: DialOutcome? = null
        // The relay's close code, once the socket closed on its own.
        var closeCode: Int? = null

        var opened: SteerSocket? = null
        try {
            val minted = transport.mint(codingSessionId)
            if (!minted.isUsable) {
                // Config state, not a transient failure — retrying can't help.
                return DialOutcome.Closed("Live sessions are unavailable on this instance.", retryable = false)
            }
            Log.i(TAG, "[$sid] dial#$dial mint ok")
            // The upgrade is BOUNDED (EXP-625): ktor's HttpTimeout skips
            // websocket upgrades, so a socket that never completes its
            // handshake would hold the loop here forever. withTimeoutOrNull,
            // never withTimeout: the latter's throw IS a CancellationException
            // and would read as "the loop was cancelled".
            val startedAt = nowMs()
            val socket = withTimeoutOrNull(timings.upgradeMs) {
                // The server-returned url is the full ws(s)://…/ws?ticket=… dial URL.
                transport.open(minted.url!!)
            } ?: run {
                Log.w(TAG, "[$sid] dial#$dial upgrade timeout")
                return DialOutcome.Closed(NO_ANSWER_DETAIL)
            }
            opened = socket
            if (!currentCoroutineContext().isActive) {
                // Cancelled while the handshake was in flight: the socket we
                // just got has no owner left.
                runCatching { socket.cancel() }
                throw CancellationException()
            }
            ws = socket
            Log.i(TAG, "[$sid] dial#$dial upgrade ok in ${nowMs() - startedAt}ms")
            // The feed is NOT wiped here (EXP-249): the relay sends an explicit
            // {t:'activity_reset'} immediately before replaying the room's log,
            // so a dial that never reaches a replay leaves the visible history
            // alone. After a reconnect the replayed transcript event is the
            // ONLY copy of a sent message — no stale echo may swallow it.
            recentEchoes.clear()
            socket.send("""{"t":"join","channel":"activity"}""")
            _connected.value = true
            // Staleness is measured from the join, not from the previous
            // socket's last frame, so a silent redial does not read stale
            // the instant it opens.
            lastFrameAtMs = nowMs()
            // NOT Live yet — the relay may answer the join with no_such_session
            // (desktop still starting). The phase flips to Live on the first
            // confirming server frame instead (the relay sends activity_reset
            // immediately on a successful join), so the Starting retry loop
            // never flashes the Live header/composer/empty state.

            var awaitingJoinAck = true
            while (true) {
                val received = if (awaitingJoinAck) {
                    // The relay ALWAYS answers a join (activity_reset plus a
                    // replay, or an error frame), so silence here is a dead
                    // socket (EXP-625). select, not withTimeoutOrNull around a
                    // receive: the latter can lose a frame it raced.
                    select<ChannelResult<String>?> {
                        socket.incoming.onReceiveCatching { it }
                        onTimeout(timings.joinAckMs) { null }
                    }
                } else {
                    socket.incoming.receiveCatching()
                }
                if (received == null) {
                    Log.w(TAG, "[$sid] dial#$dial join-ack timeout")
                    // Nothing to learn from a close reason that will never come.
                    return DialOutcome.Closed(NO_ANSWER_DETAIL)
                }
                awaitingJoinAck = false
                val text = received.getOrNull() ?: break
                lastFrameAtMs = nowMs()
                val result = handleControlFrame(text) ?: continue
                if (result.live && _phase.value != AgentPhase.Live) {
                    setPhase(AgentPhase.Live, "joined")
                    reconnectAttempts = 0
                }
                sawEnd = sawEnd || result.sawEnd
                result.detail?.let { detail = it }
                if (result.retryStarting) {
                    retryStarting = true
                    break
                }
            }
            // The incoming channel drained: the relay closed us and its close
            // code says why (EXP-621). A `break` above left the socket open —
            // there is no reason to wait on it.
            if (!retryStarting) {
                closeCode = opened.closeCode()
                // EXP-656: the reason names WHO hung up on a close code that
                // otherwise reads as a generic drop.
                Log.i(
                    TAG,
                    "[$sid] dial#$dial closed code=$closeCode reason=${opened.closeReason()}",
                )
            } else {
                Log.i(TAG, "[$sid] dial#$dial relay: no_such_session")
            }
        } catch (t: Throwable) {
            // Only OUR cancellation ends the loop (EXP-625). ktor's OkHttp
            // engine closes `outgoing` with a java.util.concurrent
            // CancellationException when the relay hangs up first, and an
            // awaited close reason on a cancelled call job does the same.
            // Those are ordinary drops, and treating them as cancellation is
            // what killed the dial coroutine silently, with the phase left
            // wherever it stood.
            if (t is CancellationException && !currentCoroutineContext().isActive) throw t
            if (t is CancellationException) {
                Log.w(TAG, "[$sid] dial#$dial foreign cancellation: ${t.message}")
                if (detail == null) detail = "The live connection dropped."
            }
            if (detail == null) {
                detail = trpcErrorMessage(t, t.message ?: "Connection failed")
            }
            // Only the mint throws TrpcException here, and two of its codes are
            // permanent: NOT_FOUND (404) means the coding_sessions row is gone
            // — the mint will refuse forever and the deleted row can never
            // report `ended`, the loop's only other exit — and FORBIDDEN (403)
            // means access to the session was revoked. Retrying either parks
            // the screen at the 30s backoff cap behind a raw error string for
            // as long as it stays open.
            terminal = when {
                t !is TrpcException -> null
                t.status == HttpStatusCode.NotFound -> DialOutcome.Ended(SESSION_GONE_DETAIL)
                t.status == HttpStatusCode.Forbidden -> DialOutcome.Closed(detail, retryable = false)
                else -> null
            }
        } finally {
            // Only the CURRENT socket's owner may clear the shared fields: a
            // superseded dial unwinding late used to null out the socket its
            // replacement had just installed, and flip `connected` false under
            // a live connection (EXP-625).
            if (ws === opened) {
                ws = null
                _connected.value = false
                // A replay this socket never finished delivering (EXP-656):
                // drop the buffer, keep what the reader can see. Guarded like
                // the fields above — a superseded dial unwinding late must not
                // discard its replacement's staging.
                discardStaging()
            }
            runCatching { opened?.cancel() }
        }

        terminal?.let { return it }
        return when {
            sawEnd -> DialOutcome.Ended(detail)
            // Heartbeat-stale rows don't warrant a redial (EXP-153) — the row
            // is a phantom, not a session that's still starting.
            retryStarting && session.value?.let { CodingSessionLiveness.isLive(it) } == true ->
                DialOutcome.RetryStarting
            // The close code decides the rest (EXP-621).
            else -> when (steerCloseAction(closeCode, sessionIsOver())) {
                SteerCloseAction.RedialNow -> DialOutcome.RedialNow
                SteerCloseAction.Ended -> DialOutcome.Ended(detail)
                SteerCloseAction.Terminal -> DialOutcome.Closed(detail, retryable = false)
                SteerCloseAction.Backoff -> DialOutcome.Closed(detail)
            }
        }
    }

    private data class FrameResult(
        val sawEnd: Boolean = false,
        val detail: String? = null,
        val retryStarting: Boolean = false,
        /** The frame proves the join succeeded — the room is live on the relay. */
        val live: Boolean = false,
    )

    private fun handleControlFrame(raw: String): FrameResult? {
        val obj = runCatching { json.parseToJsonElement(raw).jsonObject }.getOrNull() ?: return null
        return when ((obj["t"] as? JsonPrimitive)?.contentOrNull) {
            "activity" -> {
                val event = obj["event"] as? JsonObject
                if (stagedFrames != null) stageFrame(event) else handleActivityEvent(event)
                FrameResult(live = true)
            }
            // The relay's uniform "clear your feed now" signal (EXP-249) — sent
            // to every activity viewer right before its join replay, and fanned
            // out whenever the publisher resets the log. Since EXP-656 it opens
            // a STAGING buffer instead of wiping: the visible feed only changes
            // when the replay is committed, all at once.
            "activity_reset" -> {
                beginStaging()
                FrameResult(live = true)
            }
            // EXP-656: the relay's end-of-replay marker, sent right after the
            // join replay. Nothing else needs it — an old relay falls back to
            // the quiet timer.
            "activity_synced" -> {
                commitStaging("marker")
                FrameResult(live = true)
            }
            // The relay's liveness beat to joined viewers (EXP-648). Already
            // counted: the receive loop stamped lastFrameAtMs before handing
            // the frame here. Never a phase change, never a feed change — but
            // its 15s cadence does prove a staged replay burst is over.
            "keepalive" -> {
                if (stagedFrames != null) commitStaging("keepalive")
                null
            }
            "bye" -> {
                val outcome = (obj["outcome"] as? JsonPrimitive)?.contentOrNull
                if (outcome == "publisher_lost") {
                    // The desktop's relay socket dropped but the session may
                    // still be running — the synced row is the truth. Stay
                    // retryable (Closed, auto-reconnecting).
                    FrameResult(
                        detail = "The desktop's connection to the relay dropped. Waiting for it to come back.",
                    )
                } else {
                    FrameResult(sawEnd = true, detail = outcome?.takeIf { it != "ended" })
                }
            }
            "error" -> {
                val code = (obj["code"] as? JsonPrimitive)?.contentOrNull
                if (code == "no_such_session") {
                    // Not live on the relay (yet). With the synced row still
                    // running this flips into the auto-retrying Starting phase.
                    FrameResult(
                        detail = "The live stream isn't up yet. The desktop may still be connecting.",
                        retryStarting = true,
                    )
                } else {
                    FrameResult(
                        detail = (obj["message"] as? JsonPrimitive)?.contentOrNull ?: code,
                    )
                }
            }
            else -> null // input/answer/kill/legacy presence — not activity-viewer-relevant
        }
    }

    private fun handleActivityEvent(event: JsonObject?) {
        if (event == null) return
        val before = _activity.value
        val after = before.applyActivityEvent(event) { consumeEcho(it) }
        if (after === before) return
        // A settled lock — acknowledged, or released by its card resolving —
        // has no pending deadline left to guard.
        ackTimeouts.keys.toList().forEach { key ->
            if (after.answerLocks[key] != AnswerState.Sending) ackTimeouts.remove(key)?.cancel()
        }
        _activity.value = after
    }

    // ── Staged replay (EXP-656) ──────────────────────────────────────────────
    //
    // The relay answers EVERY viewer join with `activity_reset` + a full replay
    // of the room log, and a publisher reconnect fans out the same thing. Doing
    // what the frame literally says — empty the feed, then re-append N events —
    // collapsed the LazyColumn to nothing for the length of the burst, which
    // disposed its scroll state, its follow flag and the focused subagent tab,
    // and dumped a reader parked mid-plan at the bottom of the replay. So the
    // burst is buffered and swapped in as ONE emission instead: same result,
    // no intermediate empty state, and the row keys the reader is anchored on
    // survive it.

    /** Open (or restart) the staging buffer. The VISIBLE feed is untouched. */
    private fun beginStaging() {
        stageQuietJob?.cancel()
        stageQuietJob = null
        stageCapJob?.cancel()
        stagedFrames = mutableListOf()
        stagedLocalEchoes.clear()
        stageStartedAtMs = nowMs()
        // A republish that never goes quiet still has to land eventually.
        stageCapJob = scope.launch {
            delay(timings.replayMaxMs)
            commitStaging("cap")
        }
    }

    /** Buffer one replayed event and push the quiet deadline out. */
    private fun stageFrame(event: JsonObject?) {
        val staged = stagedFrames ?: return
        if (event != null) staged.add(event)
        stageQuietJob?.cancel()
        stageQuietJob = scope.launch {
            delay(timings.replayQuietMs)
            commitStaging("quiet")
        }
    }

    /**
     * Swap the staged replay in as the feed — ONE `_activity` write.
     *
     * The replay is authoritative (same reasoning as `dialOnce`'s
     * `recentEchoes.clear()`), so it is folded with `isEcho = { false }` from a
     * fresh [ActivityFeedState]: ids restart at 0, which is what keeps the
     * replayed prefix on the SAME LazyColumn keys the reader is anchored to.
     * Two things are then carried across the swap: messages this client sent
     * during the window (the replay predates them) and answer locks still
     * awaiting their ack, whose cards came back in the replay — the tap that
     * locked them may be only milliseconds old.
     */
    private fun commitStaging(why: String) {
        val staged = stagedFrames ?: return
        stageQuietJob?.cancel()
        stageQuietJob = null
        stageCapJob?.cancel()
        stageCapJob = null
        stagedFrames = null
        val echoes = stagedLocalEchoes.toList()
        stagedLocalEchoes.clear()

        val previous = _activity.value
        var next = staged.fold(ActivityFeedState()) { state, event ->
            state.applyActivityEvent(event) { false }
        }
        // The desktop may have published a message we echoed locally before the
        // burst ended, so only re-append the ones the replay does not carry.
        for (text in echoes) {
            val alreadyThere = next.feed.asReversed().take(echoes.size + 1).any { item ->
                item is AgentFeedItem.UserMessage && item.text.trim() == text.trim()
            }
            if (!alreadyThere) next = next.appendUserMessage(text)
        }
        // A card that survived the replay keeps its in-flight lock (and the
        // labels the stepper renders for it); everything else is released.
        val liveKeys = next.feed.filterIsInstance<AgentFeedItem.Question>()
            .mapNotNullTo(mutableSetOf()) { it.wireId }
        val carried = previous.answerLocks
            .filterKeys { it in liveKeys }
            .filterValues { it == AnswerState.Sending }
        next = next.copy(
            answerLocks = next.answerLocks + carried,
            answerLabels = next.answerLabels + previous.answerLabels.filterKeys { it in carried },
        )
        // What `resetActivity` used to do on the reset frame, at commit time:
        // an ack deadline whose lock did not carry over guards nothing.
        ackTimeouts.keys.toList().forEach { key ->
            if (key !in carried) ackTimeouts.remove(key)?.cancel()
        }
        _activity.value = next
        Log.i(
            TAG,
            "[$sid] replay committed (why=$why frames=${staged.size} " +
                "ms=${nowMs() - stageStartedAtMs})",
        )
    }

    /** Drop a staged replay that will never be completed — a socket that died
     *  mid-burst, a park, a close. The VISIBLE feed stays exactly as it is:
     *  the last complete picture beats a half-replayed one. */
    private fun discardStaging() {
        if (stagedFrames == null) return
        stageQuietJob?.cancel()
        stageQuietJob = null
        stageCapJob?.cancel()
        stageCapJob = null
        stagedFrames = null
        stagedLocalEchoes.clear()
        Log.i(TAG, "[$sid] replay discarded")
    }

    /** Whether an incoming `user_message` matches a recent local echo —
     *  consumes the matched entry (and evicts expired ones); true = skip it. */
    private fun consumeEcho(text: String): Boolean {
        val now = System.currentTimeMillis()
        recentEchoes.removeAll { now - it.second > ECHO_TTL_MS }
        val needle = text.trim()
        val match = recentEchoes.firstOrNull { it.first == needle } ?: return false
        recentEchoes.remove(match)
        return true
    }

    // ── Steering (message-shaped; owner-only — the mint refuses others) ──────

    /**
     * Send one message to the agent: the text (chunked ≤4 KiB), then a
     * SEPARATE `\r` frame — bundled into one write TUI apps treat the
     * trailing return as a paste, which inserts instead of submitting.
     *
     * Returns whether the message went out — false means nothing was sent and
     * the caller must keep the composition intact (EXP-621).
     */
    fun sendMessage(text: String): Boolean {
        if (text.isEmpty()) return false
        val socket = ws ?: return false
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        recentEchoes.addLast(text.trim() to System.currentTimeMillis())
        while (recentEchoes.size > ECHO_CAP) recentEchoes.removeFirst()
        _activity.value = _activity.value.appendUserMessage(text)
        // Sent mid-replay (EXP-656): the staged log predates it, so record it
        // to be re-appended when the swap lands, or it disappears on commit.
        stagedFrames?.let { stagedLocalEchoes.add(text) }
        scope.launch {
            runCatching {
                var i = 0
                while (i < text.length) {
                    val chunk = text.substring(i, minOf(i + INPUT_CHUNK_CHARS, text.length))
                    val frame = buildJsonObject {
                        put("t", "input")
                        put("data", chunk)
                    }
                    socket.send(json.encodeToString(JsonObject.serializer(), frame))
                    i += INPUT_CHUNK_CHARS
                }
                socket.send("""{"t":"input","data":"\r"}""")
            }
        }
        return true
    }

    /** Attach a picked image to the next steer message (EXP-511). Rejects
     *  anything the server would refuse and silently drops picks past the cap. */
    fun addPendingImage(uri: Uri, bytes: ByteArray, filename: String, mime: String) {
        val contentType = canonicalContentType(mime)
        if (contentType !in INLINE_IMAGE_CONTENT_TYPES) {
            _steerImageError.value = "That file type can't be attached"
            return
        }
        if (bytes.size > MAX_IMAGE_UPLOAD_BYTES) {
            _steerImageError.value =
                "Images must be ${MAX_IMAGE_UPLOAD_BYTES / (1024 * 1024)} MB or smaller"
            return
        }
        if (_pendingImages.value.size >= MAX_STEER_IMAGES) return
        _steerImageError.value = null
        _pendingImages.value = _pendingImages.value +
            PendingAttachment(uri, bytes, filename, contentType, isImage = true)
    }

    fun removePendingImage(index: Int) {
        val current = _pendingImages.value
        if (index !in current.indices) return
        _pendingImages.value = current.filterIndexed { i, _ -> i != index }
    }

    /**
     * Send the composed draft — the typed text plus any pending images
     * (EXP-511): upload each image to the SESSION (EXP-698 — not to its
     * issue; a batch, action or chat run has none), then send ONE
     * message whose text carries an `![image](/api/attachments/…)` embed per
     * upload — the host swaps those for local file paths so the agent reads
     * the file directly.
     *
     * The draft and the thumbnails are dropped ONLY once the message is
     * actually out (EXP-621). A failed upload keeps both: already-uploaded
     * entries hold their id, so retrying uploads only what is left.
     */
    fun sendDraft() {
        val text = _draft.value
        val images = _pendingImages.value
        if (text.isBlank() && images.isEmpty()) return
        if (_steerSending.value) return
        if (images.isEmpty()) {
            if (sendMessage(text)) _draft.value = ""
            return
        }
        scope.launch {
            val uploads = issueImagesApi
            if (ws == null || uploads == null) {
                // EXP-698: images upload against the SESSION, so a batch,
                // action or chat run can be shown one too — this only guards a
                // session that ended mid-compose.
                _steerImageError.value = "Images can't be sent right now"
                return@launch
            }
            val accountId = auth?.activeAccountId?.value
            if (accountId == null) {
                _steerImageError.value = "You are signed out"
                return@launch
            }
            _steerSending.value = true
            _steerImageError.value = null
            try {
                for ((index, image) in images.withIndex()) {
                    if (image.uploadedId != null) continue
                    val uploaded = try {
                        uploads.uploadSessionImage(
                            accountId,
                            codingSessionId,
                            image.bytes,
                            image.filename,
                            image.contentType,
                        )
                    } catch (cancel: CancellationException) {
                        throw cancel
                    } catch (t: Throwable) {
                        // The 412 body's billing copy never reaches the UI —
                        // the API already replaced it (EXP-216).
                        _steerImageError.value = trpcErrorMessage(t, "The image could not be uploaded")
                        return@launch
                    }
                    _pendingImages.value = _pendingImages.value.mapIndexed { i, entry ->
                        if (i == index) entry.copy(uploadedId = uploaded.id) else entry
                    }
                }
                val ids = _pendingImages.value.mapNotNull { it.uploadedId }
                if (sendMessage(buildSteerImageMessage(text, ids))) {
                    _pendingImages.value = emptyList()
                    _draft.value = ""
                }
            } finally {
                _steerSending.value = false
            }
        }
    }

    /**
     * Answer a question card that carries a wire id (EXP-249): ONE semantic
     * `answer` frame — the desktop owns the mapping onto its TUI and confirms
     * the injection with `answer_ack`. The card locks the moment the frame
     * goes out (no double-tap) and unlocks only if nothing comes back within
     * [ANSWER_ACK_TIMEOUT_MS].
     */
    fun sendQuestionAnswer(
        questionId: String,
        askId: String?,
        keys: List<String>,
        /** EXP-513: the typed reply for a `freeText` option. */
        text: String? = null,
        /** EXP-588: the picked labels, shown for the step until the desktop
         *  resolves the ask. */
        labels: List<String> = emptyList(),
    ) {
        if (keys.isEmpty()) return
        if (_activity.value.answerLocks[questionId].locksCard()) return
        val socket = ws ?: return
        lockAnswer(questionId, labels)
        scope.launch {
            runCatching {
                val frame = buildJsonObject {
                    put("t", "answer")
                    put("questionId", questionId)
                    if (askId != null) put("askId", askId)
                    putJsonArray("keys") { keys.forEach { add(JsonPrimitive(it)) } }
                    if (text != null) put("text", text)
                }
                socket.send(json.encodeToString(JsonObject.serializer(), frame))
            }
        }
    }

    private fun lockAnswer(lockKey: String, labels: List<String> = emptyList()) {
        _activity.value = _activity.value.lockAnswer(lockKey, labels)
        ackTimeouts.remove(lockKey)?.cancel()
        ackTimeouts[lockKey] = scope.launch {
            delay(ANSWER_ACK_TIMEOUT_MS)
            ackTimeouts.remove(lockKey)
            // Nothing came back — free the card WITH a retry hint (EXP-334).
            _activity.value = _activity.value.failUnacknowledged(lockKey)
        }
    }
}
