package com.exponential.app.data.steer

import android.net.Uri
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.INLINE_IMAGE_CONTENT_TYPES
import com.exponential.app.domain.MAX_IMAGE_UPLOAD_BYTES
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.ui.components.PendingAttachment
import com.exponential.app.ui.session.ActivityFeedState
import com.exponential.app.ui.session.AgentPhase
import com.exponential.app.ui.session.AnswerState
import com.exponential.app.ui.session.MAX_STEER_IMAGES
import com.exponential.app.ui.session.appendUserMessage
import com.exponential.app.ui.session.applyActivityEvent
import com.exponential.app.ui.session.buildSteerImageMessage
import com.exponential.app.ui.session.failUnacknowledged
import com.exponential.app.ui.session.lockAnswer
import com.exponential.app.ui.session.locksCard
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.http.HttpStatusCode
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import kotlin.math.pow
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
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
// BINARY frame from an old desktop is ignored. Steering is message-shaped and
// fully seamless (EXP-312 — no operator claim, no view/steer perm split; the
// ticket mint is owner-only, so a live connection just steers): chunked input
// + a separate \r; answers are semantic {t:'answer'} frames, with the
// raw-keystroke path kept only for question cards published by pre-EXP-249
// desktops (no wire id).
//
// EXP-621: all of this lives in an app singleton keyed by coding session
// ([SteerConnectionStore]), NOT in the screen's ViewModel — the socket, the
// feed and the composer draft have to survive a back-navigation and a
// rotation, so reopening a session re-attaches to what is already running
// instead of replaying the whole log behind a "Connecting…" spinner.

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
private const val INPUT_CHUNK_CHARS = 4096

/** How long an unacknowledged answer keeps its card locked (EXP-249): the
 *  desktop confirms injection with `answer_ack`, and a silently dropped frame
 *  must not strand the card as un-answerable forever. Derived from the
 *  desktop's worst-case ack budget (EXP-347): ANSWER_RETRY_TTL 4s +
 *  ANSWER_SETTLE 2s + PLAN_SUBMIT_PROBE 0.5s + ~1.5s tick/relay margin —
 *  web/iOS parity, move all three in lockstep. */
private const val ANSWER_ACK_TIMEOUT_MS = 8_000L

/** Redial cadence while the desktop's publisher socket is still starting. */
private const val STARTING_RETRY_MS = 3_000L

/** Auto-reconnect backoff after an unexpected drop (EXP-243): jittered
 *  exponential 3s→30s, mirroring the web viewer's starting retry — the jitter
 *  desyncs a herd of viewers all foregrounding at once. */
private const val RECONNECT_BASE_MS = 3_000L
private const val RECONNECT_MAX_MS = 30_000L

/** Shown when the session's row no longer exists — a swept row (or one that
 *  left this client's sync scope) is over as far as any client can tell, and
 *  nothing about it is retryable. */
private const val SESSION_GONE_DETAIL = "This session is no longer available."

/** Echo-FIFO bounds (EXP-78): a mid-turn steered message can take a while to
 *  hit the transcript, but an unmatched echo must not swallow an identical
 *  message sent much later. */
private const val ECHO_CAP = 8
private const val ECHO_TTL_MS = 300_000L

/** A closed socket's close reason is already settled — this only guards
 *  against a dial that somehow left it pending. */
private const val CLOSE_REASON_TIMEOUT_MS = 1_000L

/**
 * One live viewer connection to a coding session's relay room.
 *
 * Created and owned by [SteerConnectionStore]; the screen's ViewModel is a
 * façade over it. Everything the user would hate to lose on a back-tap lives
 * here: the socket, the activity feed, the pending images and the composer
 * draft.
 */
class SteerConnection(
    val codingSessionId: String,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val steerApi: SteerApi,
    private val issueImagesApi: IssueImagesApi,
    private val client: HttpClient,
    private val json: Json,
) {

    /** The connection's own lifetime — outlives every screen that attaches. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** Screens currently attached (EXP-621). Read and written only under the
     *  store's lock, which is what decides when a connection may be reaped. */
    internal var refCount = 0

    /** The synced coding_sessions row — flips to ended via Electric. Shared
     *  EAGERLY: the redial loop reads it while no screen is subscribed, and a
     *  frozen row would keep it dialing a session that already finished. */
    val session: StateFlow<CodingSessionEntity?> =
        accountDatabaseFlow(auth, holder).scopedQuery<CodingSessionEntity?>(null) {
            it.codingSessionDao().observeById(codingSessionId)
        }.stateIn(scope, SharingStarted.Eagerly, null)

    private val _phase = MutableStateFlow<AgentPhase>(AgentPhase.Idle)
    val phase: StateFlow<AgentPhase> = _phase

    // The feed survives reconnects, navigation and the session end — only the
    // relay's activity_reset starts it empty (EXP-249).
    private val _activity = MutableStateFlow(ActivityFeedState())
    val activity: StateFlow<ActivityFeedState> = _activity

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
    private var ws: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null

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
        get() = _phase.value.let { it is AgentPhase.Ended || (it is AgentPhase.Closed && !it.reconnecting) }

    fun setDraft(text: String) {
        _draft.value = text
    }

    /** Auto-connect once when a screen attaches; drops after that
     *  auto-reconnect (EXP-243). Also the revival path out of a background
     *  park, which leaves the connection Idle with its feed intact. */
    fun connectIfIdle() {
        if (_phase.value == AgentPhase.Idle) connect()
    }

    fun connect() {
        connectJob?.cancel()
        reconnectAttempts = 0
        connectJob = scope.launch {
            // Set by a slow-consumer close (EXP-621): the next dial must not
            // touch the phase, so the redial stays invisible.
            var silent = false
            while (isActive) {
                val outcome = dialOnce(silent)
                silent = false
                when (outcome) {
                    DialOutcome.RetryStarting -> {
                        // The desktop hasn't published the room yet. Keep
                        // redialing (fresh ticket each time) while the synced
                        // row still says running.
                        _phase.value = AgentPhase.Starting
                        delay(STARTING_RETRY_MS)
                        if (sessionIsOver()) {
                            _phase.value = AgentPhase.Ended()
                            return@launch
                        }
                    }
                    // The relay dropped a viewer that fell behind its send
                    // buffer (4008). The room is untouched, so redial straight
                    // away: no delay, no attempt counted, and the phase is left
                    // exactly as it was — a busy agent must not flap the
                    // "Connection lost" banner every few seconds (EXP-621).
                    DialOutcome.RedialNow -> silent = true
                    is DialOutcome.Ended -> {
                        _phase.value = AgentPhase.Ended(outcome.detail)
                        return@launch
                    }
                    is DialOutcome.Closed -> {
                        if (!outcome.retryable) {
                            _phase.value = AgentPhase.Closed(outcome.detail)
                            return@launch
                        }
                        // Never park on a dead socket behind a manual button
                        // (EXP-243) — auto-redial on backoff; the phase
                        // carries the reconnecting flag so the UI shows
                        // "Reconnecting…" instead of a Reconnect action.
                        _phase.value = AgentPhase.Closed(outcome.detail, reconnecting = true)
                        delay(reconnectDelayMs(reconnectAttempts++))
                        if (sessionIsOver()) {
                            _phase.value = AgentPhase.Ended()
                            return@launch
                        }
                    }
                }
            }
        }
    }

    /** Foreground revival (EXP-243): skip any pending reconnect backoff and
     *  redial immediately; while nominally live, ping-probe the socket so a
     *  connection that died in the background surfaces (and auto-reconnects)
     *  now instead of on the next failed read. */
    fun reconnectNow() {
        val p = _phase.value
        if (p is AgentPhase.Closed && p.reconnecting) {
            connect()
        } else if (p == AgentPhase.Live) {
            ws?.outgoing?.trySend(Frame.Ping(ByteArray(0)))
        }
    }

    /**
     * Background park (EXP-621): drop the socket, keep everything the user can
     * see — feed, draft, pending images. A parked connection reads as Idle, so
     * [resume] revives it through the ordinary first-open path; a finished one
     * has nothing to park.
     */
    internal fun park() {
        if (isFinished) return
        connectJob?.cancel()
        connectJob = null
        runCatching { ws?.cancel() }
        ws = null
        _phase.value = AgentPhase.Idle
    }

    /** Come back to the foreground: revive a parked connection, and kick a
     *  live/backing-off one the way the screen used to on resume. */
    internal fun resume() {
        if (isFinished) return
        connectIfIdle()
        reconnectNow()
    }

    /** Tear the connection down for good — sign-out, account switch, or the
     *  store reaping a finished session nothing is looking at. */
    internal fun close() {
        connectJob?.cancel()
        connectJob = null
        runCatching { ws?.cancel() }
        ws = null
        scope.cancel()
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
            RECONNECT_MAX_MS.toDouble(),
            RECONNECT_BASE_MS * 2.0.pow(attempt),
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

    private suspend fun dialOnce(silent: Boolean): DialOutcome {
        // Hold the Starting / reconnecting-Closed phase steady across
        // auto-retry redials — flipping to Connecting per attempt made the
        // header flicker every ~3s while the desktop was still dialing its
        // publisher (and would flicker the reconnect banner the same way). A
        // silent (4008) redial holds whatever phase it had, Live included.
        val held = _phase.value
        if (!silent && held != AgentPhase.Starting && !(held is AgentPhase.Closed && held.reconnecting)) {
            _phase.value = AgentPhase.Connecting
        }

        // `bye` / no_such_session must win over the generic close handler.
        var sawEnd = false
        var retryStarting = false
        var detail: String? = null
        // A server "no" that can never turn into a yes — wins over everything.
        var terminal: DialOutcome? = null
        // The relay's close code, once the socket closed on its own.
        var closeCode: Int? = null

        var socket: DefaultClientWebSocketSession? = null
        try {
            val accountId = auth.activeAccountId.value
                ?: throw IllegalStateException("No active account")
            val minted = steerApi.mintViewerTicket(accountId, codingSessionId)
            if (!minted.isUsable) {
                // Config state, not a transient failure — retrying can't help.
                return DialOutcome.Closed("Live sessions are unavailable on this instance.", retryable = false)
            }
            // The server-returned url is the full ws(s)://…/ws?ticket=… dial URL.
            val opened = client.webSocketSession(urlString = minted.url!!)
            socket = opened
            ws = opened
            // The feed is NOT wiped here (EXP-249): the relay sends an explicit
            // {t:'activity_reset'} immediately before replaying the room's log,
            // so a dial that never reaches a replay leaves the visible history
            // alone. After a reconnect the replayed transcript event is the
            // ONLY copy of a sent message — no stale echo may swallow it.
            recentEchoes.clear()
            opened.send(Frame.Text("""{"t":"join","channel":"activity"}"""))
            // NOT Live yet — the relay may answer the join with no_such_session
            // (desktop still starting). The phase flips to Live on the first
            // confirming server frame instead (the relay sends activity_reset
            // immediately on a successful join), so the Starting retry loop
            // never flashes the Live header/composer/empty state.

            for (frame in opened.incoming) {
                when (frame) {
                    // The PTY mirror is gone (EXP-249); an old desktop's binary
                    // output frames are silently dropped.
                    is Frame.Binary -> Unit
                    is Frame.Text -> {
                        val result = handleControlFrame(frame.readText())
                        if (result != null) {
                            if (result.live && _phase.value != AgentPhase.Live) {
                                _phase.value = AgentPhase.Live
                                reconnectAttempts = 0
                            }
                            sawEnd = sawEnd || result.sawEnd
                            result.detail?.let { detail = it }
                            if (result.retryStarting) {
                                retryStarting = true
                                break
                            }
                        }
                    }
                    else -> Unit
                }
            }
            // The incoming channel drained: the relay closed us and its close
            // code says why (EXP-621). A `break` above left the socket open —
            // there is no reason to wait on it.
            if (!retryStarting) {
                closeCode = withTimeoutOrNull(CLOSE_REASON_TIMEOUT_MS) {
                    opened.closeReason.await()
                }?.code?.toInt()
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
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
            ws = null
            runCatching { socket?.cancel() }
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
                handleActivityEvent(obj["event"] as? JsonObject)
                FrameResult(live = true)
            }
            // The relay's uniform "clear your feed now" signal (EXP-249) — sent
            // to every activity viewer right before its join replay, and fanned
            // out whenever the publisher resets the log. It is the ONLY thing
            // that wipes the feed.
            "activity_reset" -> {
                resetActivity()
                FrameResult(live = true)
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

    /** Wipe everything the relay's activity log owns (EXP-249). */
    private fun resetActivity() {
        recentEchoes.clear()
        ackTimeouts.values.forEach { it.cancel() }
        ackTimeouts.clear()
        _activity.value = ActivityFeedState()
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
        scope.launch {
            runCatching {
                var i = 0
                while (i < text.length) {
                    val chunk = text.substring(i, minOf(i + INPUT_CHUNK_CHARS, text.length))
                    val frame = buildJsonObject {
                        put("t", "input")
                        put("data", chunk)
                    }
                    socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
                    i += INPUT_CHUNK_CHARS
                }
                socket.send(Frame.Text("""{"t":"input","data":"\r"}"""))
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
     * (EXP-511): upload each image to the session's issue, then send ONE
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
            val issueId = session.value?.issueId
            if (ws == null || issueId == null) {
                // Batch and action runs have no issue to attach to, and the
                // composer hides the attach button for them — so this only
                // guards a session that ended mid-compose.
                _steerImageError.value = "Images can't be sent right now"
                return@launch
            }
            val accountId = auth.activeAccountId.value
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
                        issueImagesApi.upload(
                            accountId,
                            issueId,
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
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
            }
        }
    }

    /**
     * Answer a question card published by a pre-EXP-249 desktop (no wire id):
     * the option's raw TUI keystroke. The digit ALONE — bundling
     * a `\r` submitted the picker AND cascaded into whatever picker claude
     * opened next. [lock] is off for multi-select toggles, which tap repeatedly
     * before [sendSubmit] advances the picker.
     */
    fun sendLegacyAnswer(lockKey: String, key: String, lock: Boolean = true) {
        if (key.isEmpty()) return
        if (lock && _activity.value.answerLocks[lockKey].locksCard()) return
        val socket = ws ?: return
        if (lock) lockAnswer(lockKey)
        scope.launch {
            runCatching {
                val frame = buildJsonObject {
                    put("t", "input")
                    put("data", key)
                }
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
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

    /** Advance a legacy multi-select question. Tab, NOT Enter: with the cursor
     *  on an option row Enter TOGGLES it (verified against claude v2.1.215 —
     *  silently corrupting the selection), while Tab moves to the next
     *  tab/review step, whose picker the grid watcher publishes as its own
     *  card (EXP-197). */
    fun sendSubmit() = sendLegacyAnswer("", "\t", lock = false)
}
