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
import com.exponential.app.ui.terminal.VtEngine
import com.exponential.app.ui.terminal.VtSnapshot
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
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// Viewer side of the steer relay wire protocol
// (apps/steer-relay/src/protocol.ts): binary frames are terminal output (one
// 0x01 opcode byte + verbatim PTY bytes) fed into the self-written VtEngine;
// TEXT frames are JSON control messages (presence / resize / bye / error).
// Mirrors the web reference viewer apps/web/src/components/steer-terminal.tsx.

private const val OUTPUT_OPCODE: Byte = 0x01
// Relay rejects input frames > 8 KiB; chunk pastes well under that.
private const val INPUT_CHUNK_CHARS = 4096
private const val RENDER_INTERVAL_MS = 33L

@Serializable
data class PresenceViewer(
    val userId: String,
    val name: String = "",
    val perm: String = "view",
)

sealed interface SteerPhase {
    data object Idle : SteerPhase
    data object Connecting : SteerPhase
    data object Live : SteerPhase

    /** The session ended (relay `bye`, or the room was never live). */
    data class Ended(val detail: String? = null) : SteerPhase

    /** Unexpected socket loss — offer a manual Reconnect (fresh ticket). */
    data class Closed(val detail: String? = null) : SteerPhase
}

@HiltViewModel
class SteerTerminalViewModel @Inject constructor(
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

    private val _phase = MutableStateFlow<SteerPhase>(SteerPhase.Idle)
    val phase: StateFlow<SteerPhase> = _phase

    private val _perm = MutableStateFlow("view")
    val perm: StateFlow<String> = _perm

    private val _viewers = MutableStateFlow<List<PresenceViewer>>(emptyList())
    val viewers: StateFlow<List<PresenceViewer>> = _viewers

    private val _steererId = MutableStateFlow<String?>(null)
    val steererId: StateFlow<String?> = _steererId

    // The engine is single-owner: fed + snapshotted only on the socket
    // coroutine, published as immutable snapshots for Compose.
    private var engine = VtEngine()
    private val _snapshot = MutableStateFlow(engine.snapshot())
    val snapshot: StateFlow<VtSnapshot> = _snapshot

    private var ws: DefaultClientWebSocketSession? = null
    private var connectJob: Job? = null

    /** Auto-connect once when the screen opens; reconnects are explicit. */
    fun connectIfIdle() {
        if (_phase.value == SteerPhase.Idle) connect()
    }

    fun connect() {
        connectJob?.cancel()
        connectJob = viewModelScope.launch {
            _phase.value = SteerPhase.Connecting
            _viewers.value = emptyList()
            _steererId.value = null

            // `bye` / no_such_session must win over the generic close handler.
            var sawEnd = false
            var detail: String? = null

            var socket: DefaultClientWebSocketSession? = null
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                val minted = steerApi.mintViewerTicket(accountId, codingSessionId)
                if (!minted.isUsable) {
                    _phase.value =
                        SteerPhase.Closed("Live steering is unavailable on this instance.")
                    return@launch
                }
                _perm.value = decodeSteerTicketPerm(minted.ticket!!)

                engine = VtEngine()
                _snapshot.value = engine.snapshot()

                // The server-returned url is the full ws(s)://…/ws?ticket=… dial URL.
                socket = client.webSocketSession(urlString = minted.url!!)
                ws = socket
                socket.send(Frame.Text("""{"t":"join"}"""))
                _phase.value = SteerPhase.Live

                // Coalesce PTY bursts: feed marks dirty, a ~30fps ticker publishes.
                var dirty = false
                val renderJob = launch {
                    while (isActive) {
                        delay(RENDER_INTERVAL_MS)
                        if (dirty) {
                            dirty = false
                            _snapshot.value = engine.snapshot()
                        }
                    }
                }

                try {
                    for (frame in socket.incoming) {
                        when (frame) {
                            is Frame.Binary -> {
                                val bytes = frame.data
                                if (bytes.isNotEmpty() && bytes[0] == OUTPUT_OPCODE) {
                                    engine.feed(bytes, offset = 1)
                                    dirty = true
                                }
                            }
                            is Frame.Text -> {
                                val result = handleControlFrame(frame.readText())
                                if (result != null) {
                                    sawEnd = sawEnd || result.sawEnd
                                    result.detail?.let { detail = it }
                                    if (result.resized) dirty = true
                                }
                            }
                            else -> Unit
                        }
                    }
                } finally {
                    renderJob.cancel()
                    _snapshot.value = engine.snapshot()
                }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                if (detail == null) {
                    detail = trpcErrorMessage(t, t.message ?: "Connection failed")
                }
            } finally {
                ws = null
                runCatching { socket?.cancel() }
            }

            _viewers.value = emptyList()
            _steererId.value = null
            _phase.value = if (sawEnd) SteerPhase.Ended(detail) else SteerPhase.Closed(detail)
        }
    }

    private data class FrameResult(
        val sawEnd: Boolean = false,
        val detail: String? = null,
        val resized: Boolean = false,
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
                FrameResult()
            }
            "resize" -> {
                val cols = (obj["cols"] as? JsonPrimitive)?.intOrNull
                val rows = (obj["rows"] as? JsonPrimitive)?.intOrNull
                if (cols != null && rows != null) engine.resize(cols, rows)
                FrameResult(resized = true)
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
                    // Not live on the relay (yet) — the desktop may still be
                    // dialing its publisher socket. Stay retryable (Closed,
                    // with Reconnect); the synced running row is the truth.
                    FrameResult(
                        detail = "The terminal isn't live on the relay yet — the desktop may still be connecting.",
                    )
                } else {
                    FrameResult(
                        detail = (obj["message"] as? JsonPrimitive)?.contentOrNull ?: code,
                    )
                }
            }
            else -> null // input/resync/kill/start_session — not viewer-relevant
        }
    }

    // ── Steering (claim + keystrokes; relay enforces the single claim) ───────

    val isSteering: Boolean
        get() = _steererId.value != null && _steererId.value == currentUserId.value

    fun claim() = sendControl(buildJsonObject { put("t", "claim") })

    fun release() = sendControl(buildJsonObject { put("t", "release") })

    /** Forward keystrokes while holding the steer claim (chunked ≤4 KiB). */
    fun sendInput(data: String) {
        if (data.isEmpty() || !isSteering) return
        val socket = ws ?: return
        viewModelScope.launch {
            runCatching {
                var i = 0
                while (i < data.length) {
                    val chunk = data.substring(i, minOf(i + INPUT_CHUNK_CHARS, data.length))
                    val frame = buildJsonObject {
                        put("t", "input")
                        put("data", chunk)
                    }
                    socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
                    i += INPUT_CHUNK_CHARS
                }
            }
        }
    }

    private fun sendControl(frame: JsonObject) {
        val socket = ws ?: return
        viewModelScope.launch {
            runCatching {
                socket.send(Frame.Text(json.encodeToString(JsonObject.serializer(), frame)))
            }
        }
    }

    fun stopWatching() {
        connectJob?.cancel()
        connectJob = null
        _phase.value = SteerPhase.Idle
        _viewers.value = emptyList()
        _steererId.value = null
    }

    override fun onCleared() {
        connectJob?.cancel()
        super.onCleared()
    }
}
