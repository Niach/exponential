package com.exponential.app.ui.session

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.decodeSteerTicketPerm
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// Viewer side of the steer relay's ACTIVITY channel (EXP-32 — the mobile
// "Agent session" chat view; apps/steer-relay/src/protocol.ts): the socket
// joins with {"t":"join","channel":"activity"} and receives scrubbed
// {t:'activity', event} frames (narration / tool headlines / worktree diffs)
// instead of raw PTY bytes. TEXT frames are JSON control messages; stray
// BINARY frames (0x01 PTY output, a relay/desktop misroute) are ignored.
// Steering is message-shaped: a steal-claim + chunked input + a separate \r.

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
private const val INPUT_CHUNK_CHARS = 4096

/** Client-side feed cap — old events fall off the top. */
private const val FEED_CAP = 500

/** Auto-release the steer claim after this long with no sends. */
private const val IDLE_RELEASE_MS = 60_000L

/** Redial cadence while the desktop's publisher socket is still starting. */
private const val STARTING_RETRY_MS = 3_000L

/** Echo-FIFO bounds (EXP-78): a mid-turn steered message can take a while to
 *  hit the transcript, but an unmatched echo must not swallow an identical
 *  message sent much later. */
private const val ECHO_CAP = 8
private const val ECHO_TTL_MS = 300_000L

@Serializable
data class PresenceViewer(
    val userId: String,
    val name: String = "",
    val perm: String = "view",
)

/** One answer choice of a [AgentFeedItem.Question] — `key` is the raw
 *  keystroke that selects it in the desktop TUI picker (mapped desktop-side). */
data class QuestionOption(val label: String, val key: String)

/** One rendered feed entry. Diffs never enter the feed — see [AgentSessionViewModel.latestDiff]. */
sealed interface AgentFeedItem {
    val id: Long

    data class Narration(override val id: Long, val text: String) : AgentFeedItem
    data class Tool(override val id: Long, val name: String, val detail: String?) : AgentFeedItem

    /** A human turn (EXP-78): the initial prompt or a steered message. */
    data class UserMessage(override val id: Long, val text: String) : AgentFeedItem

    /** An interactive question (AskUserQuestion / plan approval, EXP-78).
     *  [planMode] marks an ExitPlanMode plan-approval picker (EXP-97) —
     *  presentation-only, absent on events from older desktops/relays. */
    data class Question(
        override val id: Long,
        val text: String,
        val options: List<QuestionOption>,
        val multiSelect: Boolean,
        val planMode: Boolean = false,
    ) : AgentFeedItem
}

/**
 * Ids of the TRAILING consecutive run of [AgentFeedItem.Question] items — the
 * only ones still answerable (any later event means the desktop TUI moved on).
 */
fun trailingQuestionIds(feed: List<AgentFeedItem>): Set<Long> {
    val ids = mutableSetOf<Long>()
    for (item in feed.asReversed()) {
        if (item !is AgentFeedItem.Question) break
        ids.add(item.id)
    }
    return ids
}

/** One render row over the flat feed (EXP-97): a single item, or a run of ≥2
 *  CONSECUTIVE tool calls collapsed into one "N tool calls" row. A run's id is
 *  the FIRST tool's id, so the row key (and its expanded state) stays stable
 *  while the trailing run keeps growing. */
sealed interface AgentFeedRow {
    val id: Long

    data class Single(val item: AgentFeedItem) : AgentFeedRow {
        override val id get() = item.id
    }

    data class ToolRun(val items: List<AgentFeedItem.Tool>) : AgentFeedRow {
        override val id get() = items.first().id
    }
}

/** Group consecutive runs of ≥2 [AgentFeedItem.Tool] items into
 *  [AgentFeedRow.ToolRun] rows — a pure render-time projection: the flat feed
 *  (and [trailingQuestionIds] over it) is never restructured. */
fun groupToolRuns(feed: List<AgentFeedItem>): List<AgentFeedRow> {
    val rows = mutableListOf<AgentFeedRow>()
    var i = 0
    while (i < feed.size) {
        val item = feed[i]
        if (item !is AgentFeedItem.Tool) {
            rows.add(AgentFeedRow.Single(item))
            i++
            continue
        }
        var end = i
        while (end + 1 < feed.size && feed[end + 1] is AgentFeedItem.Tool) end++
        if (end == i) {
            rows.add(AgentFeedRow.Single(item))
        } else {
            rows.add(AgentFeedRow.ToolRun(feed.subList(i, end + 1).map { it as AgentFeedItem.Tool }))
        }
        i = end + 1
    }
    return rows
}

sealed interface AgentPhase {
    data object Idle : AgentPhase
    data object Connecting : AgentPhase
    data object Live : AgentPhase

    /**
     * The relay reported no_such_session while the synced row still says
     * running — the desktop is still dialing its publisher socket. The VM
     * auto-redials (fresh ticket) every ~3s until the room is live.
     */
    data object Starting : AgentPhase

    /** The session ended (relay `bye`, or the synced row flipped to ended). */
    data class Ended(val detail: String? = null) : AgentPhase

    /** Unexpected socket loss — offer a manual Reconnect (fresh ticket). */
    data class Closed(val detail: String? = null) : AgentPhase
}

@HiltViewModel
class AgentSessionViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    private val client: HttpClient,
    private val json: Json,
) : ViewModel() {

    val codingSessionId: String = savedStateHandle["codingSessionId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced coding_sessions row — flips to ended via Electric. */
    val session: StateFlow<CodingSessionEntity?> =
        dbFlow.scopedQuery<CodingSessionEntity?>(null) {
            it.codingSessionDao().observeById(codingSessionId)
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    val currentUserId: StateFlow<String?> = auth.userId

    private val _phase = MutableStateFlow<AgentPhase>(AgentPhase.Idle)
    val phase: StateFlow<AgentPhase> = _phase

    private val _perm = MutableStateFlow("view")
    val perm: StateFlow<String> = _perm

    private val _viewers = MutableStateFlow<List<PresenceViewer>>(emptyList())
    val viewers: StateFlow<List<PresenceViewer>> = _viewers

    private val _steererId = MutableStateFlow<String?>(null)
    val steererId: StateFlow<String?> = _steererId

    // The feed survives reconnects and the session end — only a fresh screen
    // (new VM) starts empty.
    private val _feed = MutableStateFlow<List<AgentFeedItem>>(emptyList())
    val feed: StateFlow<List<AgentFeedItem>> = _feed

    /** The most recent worktree diff — each one replaces the previous. */
    private val _latestDiff = MutableStateFlow<String?>(null)
    val latestDiff: StateFlow<String?> = _latestDiff

    private var nextEventId = 0L

    /** Locally-echoed sent messages awaiting their transcript-derived
     *  `user_message` event (EXP-78 dedupe): text → sent-at millis. */
    private val recentEchoes = ArrayDeque<Pair<String, Long>>()
    private var ws: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null
    private var idleReleaseJob: Job? = null

    /** Auto-connect once when the screen opens; reconnects are explicit. */
    fun connectIfIdle() {
        if (_phase.value == AgentPhase.Idle) connect()
    }

    fun connect() {
        connectJob?.cancel()
        connectJob = viewModelScope.launch {
            while (isActive) {
                when (val outcome = dialOnce()) {
                    DialOutcome.RetryStarting -> {
                        // The desktop hasn't published the room yet. Keep
                        // redialing (fresh ticket each time) while the synced
                        // row still says running.
                        _phase.value = AgentPhase.Starting
                        delay(STARTING_RETRY_MS)
                        if (session.value?.status == DomainContract.codingSessionStatusEnded) {
                            _phase.value = AgentPhase.Ended()
                            return@launch
                        }
                    }
                    is DialOutcome.Ended -> {
                        _phase.value = AgentPhase.Ended(outcome.detail)
                        return@launch
                    }
                    is DialOutcome.Closed -> {
                        _phase.value = AgentPhase.Closed(outcome.detail)
                        return@launch
                    }
                }
            }
        }
    }

    private sealed interface DialOutcome {
        /** no_such_session while the synced row says running — auto-retry. */
        data object RetryStarting : DialOutcome
        data class Ended(val detail: String? = null) : DialOutcome
        data class Closed(val detail: String? = null) : DialOutcome
    }

    private suspend fun dialOnce(): DialOutcome {
        // Hold the Starting phase steady across auto-retry redials — flipping
        // to Connecting per attempt made the header flicker every ~3s while
        // the desktop was still dialing its publisher.
        if (_phase.value != AgentPhase.Starting) _phase.value = AgentPhase.Connecting
        _viewers.value = emptyList()
        _steererId.value = null

        // `bye` / no_such_session must win over the generic close handler.
        var sawEnd = false
        var retryStarting = false
        var detail: String? = null

        var socket: DefaultClientWebSocketSession? = null
        try {
            val accountId = auth.activeAccountId.value
                ?: throw IllegalStateException("No active account")
            val minted = steerApi.mintViewerTicket(accountId, codingSessionId)
            if (!minted.isUsable) {
                return DialOutcome.Closed("Live sessions are unavailable on this instance.")
            }
            _perm.value = decodeSteerTicketPerm(minted.ticket!!)

            // The server-returned url is the full ws(s)://…/ws?ticket=… dial URL.
            socket = client.webSocketSession(urlString = minted.url!!)
            ws = socket
            // The relay replays the room's whole activity log (+ last diff) to
            // every joining socket — start from a clean slate or each
            // reconnect would append the full history a second time.
            _feed.value = emptyList()
            _latestDiff.value = null
            nextEventId = 0L
            // After a reconnect the replayed transcript event is the ONLY copy
            // of a sent message — it must render, so no stale echo may swallow it.
            recentEchoes.clear()
            socket.send(Frame.Text("""{"t":"join","channel":"activity"}"""))
            // NOT Live yet — the relay may answer the join with no_such_session
            // (desktop still starting). The phase flips to Live on the first
            // confirming server frame instead (the relay sends presence
            // immediately on a successful join), so the Starting retry loop
            // never flashes the Live header/composer/empty state.

            for (frame in socket.incoming) {
                when (frame) {
                    // Stray PTY output (relay/desktop misroute) — never render.
                    is Frame.Binary -> Unit
                    is Frame.Text -> {
                        val result = handleControlFrame(frame.readText())
                        if (result != null) {
                            if (result.live && _phase.value != AgentPhase.Live) {
                                _phase.value = AgentPhase.Live
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
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            if (detail == null) {
                detail = trpcErrorMessage(t, t.message ?: "Connection failed")
            }
        } finally {
            ws = null
            idleReleaseJob?.cancel()
            runCatching { socket?.cancel() }
        }

        _viewers.value = emptyList()
        _steererId.value = null
        return when {
            sawEnd -> DialOutcome.Ended(detail)
            retryStarting && session.value?.status == DomainContract.codingSessionStatusRunning ->
                DialOutcome.RetryStarting
            else -> DialOutcome.Closed(detail)
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
            "presence" -> {
                _viewers.value = runCatching {
                    json.decodeFromJsonElement(
                        ListSerializer(PresenceViewer.serializer()),
                        obj["viewers"] ?: return@runCatching emptyList(),
                    )
                }.getOrDefault(emptyList())
                _steererId.value = (obj["steererId"] as? JsonPrimitive)?.contentOrNull
                FrameResult(live = true)
            }
            "activity" -> {
                handleActivityEvent(obj["event"]?.jsonObject)
                FrameResult(live = true)
            }
            "bye" -> {
                val outcome = (obj["outcome"] as? JsonPrimitive)?.contentOrNull
                if (outcome == "publisher_lost") {
                    // The desktop's relay socket dropped but the session may
                    // still be running — the synced row is the truth. Stay
                    // retryable (Closed, with Reconnect).
                    FrameResult(
                        detail = "The desktop's connection to the relay dropped — retry once it reconnects.",
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
                        detail = "The live stream isn't up yet — the desktop may still be connecting.",
                        retryStarting = true,
                    )
                } else {
                    FrameResult(
                        detail = (obj["message"] as? JsonPrimitive)?.contentOrNull ?: code,
                    )
                }
            }
            else -> null // input/resize/resync/kill — not activity-viewer-relevant
        }
    }

    private fun handleActivityEvent(event: JsonObject?) {
        if (event == null) return
        when ((event["kind"] as? JsonPrimitive)?.contentOrNull) {
            "narration" -> {
                val text = (event["text"] as? JsonPrimitive)?.contentOrNull ?: return
                if (text.isNotBlank()) append(AgentFeedItem.Narration(nextEventId++, text))
            }
            "tool" -> {
                val name = (event["name"] as? JsonPrimitive)?.contentOrNull ?: return
                val toolDetail = (event["detail"] as? JsonPrimitive)?.contentOrNull
                append(AgentFeedItem.Tool(nextEventId++, name, toolDetail?.takeIf { it.isNotBlank() }))
            }
            // Diffs never enter the feed — the latest replaces the previous
            // one behind the pinned "Latest changes" chip.
            "diff" -> {
                _latestDiff.value = (event["diff"] as? JsonPrimitive)?.contentOrNull
                    ?.takeIf { it.isNotBlank() }
            }
            "user_message" -> {
                val text = (event["text"] as? JsonPrimitive)?.contentOrNull ?: return
                if (text.isBlank()) return
                // A message this client just sent was already echoed locally —
                // skip its transcript-derived twin (EXP-78).
                if (consumeEcho(text)) return
                append(AgentFeedItem.UserMessage(nextEventId++, text))
            }
            "question" -> {
                val text = (event["text"] as? JsonPrimitive)?.contentOrNull ?: return
                if (text.isBlank()) return
                val options = runCatching {
                    event["options"]!!.jsonArray.mapNotNull { raw ->
                        val o = raw.jsonObject
                        val label = (o["label"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
                        val key = (o["key"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
                        QuestionOption(label, key)
                    }
                }.getOrDefault(emptyList())
                if (options.isEmpty()) return
                val multiSelect = (event["multiSelect"] as? JsonPrimitive)?.booleanOrNull == true
                val planMode = (event["planMode"] as? JsonPrimitive)?.booleanOrNull == true
                append(AgentFeedItem.Question(nextEventId++, text, options, multiSelect, planMode))
            }
        }
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

    private fun append(item: AgentFeedItem) {
        _feed.value = (_feed.value + item).takeLast(FEED_CAP)
    }

    // ── Steering (message-shaped; relay enforces the single claim) ───────────

    val isSteering: Boolean
        get() = _steererId.value != null && _steererId.value == currentUserId.value

    /**
     * Send one message to the agent: steal the claim, forward the text
     * (chunked ≤4 KiB), then a SEPARATE `\r` frame — bundled into one write
     * TUI apps treat the trailing return as a paste, which inserts instead of
     * submitting. The claim is ALWAYS sent: the relay tracks the steerer per
     * CONNECTION while presence only carries a user id, so `isSteering` can't
     * tell this socket from the same user's web/second-device claim — skipping
     * the claim there made the relay silently drop every input frame.
     */
    fun sendMessage(text: String) {
        if (text.isEmpty() || _perm.value != "steer") return
        val socket = ws ?: return
        // Local echo (EXP-78): show the sent message immediately; its
        // transcript-derived `user_message` event is deduped via the FIFO.
        recentEchoes.addLast(text.trim() to System.currentTimeMillis())
        while (recentEchoes.size > ECHO_CAP) recentEchoes.removeFirst()
        append(AgentFeedItem.UserMessage(nextEventId++, text))
        viewModelScope.launch {
            runCatching {
                socket.send(Frame.Text("""{"t":"claim","steal":true}"""))
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
            scheduleIdleRelease()
        }
    }

    /**
     * Answer an interactive question (EXP-78): steal-claim + raw keystrokes —
     * the desktop passes single-byte frames to the PTY unwrapped, so the TUI
     * sees keypresses, not a paste. Verified against the real picker: a digit
     * SELECTS but does not submit, so single-select answers pass
     * `submit = true` to follow up with a separate `\r` (multi-select taps
     * toggle with the digit alone; [sendSubmit] sends the Enter).
     */
    fun sendAnswer(key: String, submit: Boolean = false) {
        if (key.isEmpty() || _perm.value != "steer") return
        val socket = ws ?: return
        viewModelScope.launch {
            runCatching {
                socket.send(Frame.Text("""{"t":"claim","steal":true}"""))
                val frame = buildJsonObject {
                    put("t", "input")
                    put("data", key)
                }
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
                if (submit && key != "\r") {
                    socket.send(Frame.Text("""{"t":"input","data":"\r"}"""))
                }
            }
            scheduleIdleRelease()
        }
    }

    /** Submit a multi-select question (Enter). */
    fun sendSubmit() = sendAnswer("\r")

    /** Auto-release the claim after 60s of no sends (timer resets per send). */
    private fun scheduleIdleRelease() {
        idleReleaseJob?.cancel()
        idleReleaseJob = viewModelScope.launch {
            delay(IDLE_RELEASE_MS)
            releaseNow()
        }
    }

    /**
     * Best-effort synchronous release — safe from onCleared/onDispose where
     * suspending is impossible (`outgoing.trySend` never blocks). Closing the
     * socket also releases the claim relay-side; this just makes it prompt.
     */
    fun releaseNow() {
        idleReleaseJob?.cancel()
        if (!isSteering) return
        ws?.outgoing?.trySend(Frame.Text("""{"t":"release"}"""))
    }

    override fun onCleared() {
        releaseNow()
        connectJob?.cancel()
        super.onCleared()
    }
}
