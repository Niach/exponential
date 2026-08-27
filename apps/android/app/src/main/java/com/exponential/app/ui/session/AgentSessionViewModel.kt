package com.exponential.app.ui.session

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.ui.markdown.AttachmentDims
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// EXP-554: the steer composer's own PendingSteerImage became the shared
// `PendingAttachment` (domain/PendingAttachment.kt), which the comment
// composers use too — same upload-on-send, uploadedId-stamped semantics.

/**
 * The steer screen's ViewModel — a façade over the app-held SteerConnection
 * (EXP-621). It owns only what is genuinely screen-scoped (the host-device
 * presentation, the linked issue's attachment sizes, a failed kill's banner);
 * the socket, the feed, the pending images and the composer draft belong to
 * the connection, which is why a back-tap no longer throws them away.
 */
@HiltViewModel
class AgentSessionViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    private val store: SteerConnectionStore,
) : ViewModel() {

    val codingSessionId: String = savedStateHandle["codingSessionId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The live connection for this session — shared with every other screen
     *  that has it open, and released in [onCleared]. */
    private val connection = store.acquire(codingSessionId)

    /** The synced coding_sessions row — flips to ended via Electric. Shared
     *  from the connection, which needs it eagerly for its redial loop. */
    val session: StateFlow<CodingSessionEntity?> = connection.session

    /**
     * EXP-549/550: the session's host machine, joined to its LIVE devices row
     * — the CURRENT label (a rename must land here, not the start-time
     * snapshot) and whether the machine dropped offline. Recomputed on the
     * device ticker, since Room flows only re-emit on writes and the 90s
     * freshness window elapses on its own.
     */
    val hostDevice: StateFlow<SessionDevicePresentation> = combine(
        session,
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
        DeviceLiveness.ticker(),
    ) { row, devices, now ->
        if (row == null) {
            SessionDevicePresentation.Unknown
        } else {
            resolveSessionDevice(row, devices, now)
        }
    }.stateIn(
        viewModelScope,
        SharingStarted.WhileSubscribed(5_000),
        SessionDevicePresentation.Unknown,
    )

    /**
     * EXP-550: the machine running this session is offline while the session
     * is still supposed to be WORKING — the run is paused (lid closed), not
     * lost and not ended, and it continues when the machine comes back. A row
     * already in review / ended is parked on its own outcome, so its
     * machine's presence says nothing. The relay redial loop is untouched:
     * this only changes what the screen says while it keeps trying.
     */
    val hostDeviceOffline: StateFlow<Boolean> = combine(hostDevice, session) { device, row ->
        device.offline && row != null && row.status == DomainContract.codingSessionStatusRunning
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /**
     * Probed sizes of the linked issue's attachments (REV2-79) — the feed
     * renders markdown since EXP-440, so an `![](/api/attachments/…)` in
     * narration or a steered message pre-sizes instead of jumping when the
     * bitmap lands. Batch and action runs have no issue: nothing to pre-size.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val attachmentDims: StateFlow<AttachmentDims> = session
        .map { it?.issueId }
        .distinctUntilChanged()
        .flatMapLatest { issueId ->
            if (issueId == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList()) { it.attachmentDao().observeByIssue(issueId) }
            }
        }
        .map { rows ->
            AttachmentDims(
                rows.mapNotNull { row ->
                    val width = row.width
                    val height = row.height
                    if (width == null || height == null) null else row.id to (width to height)
                }.toMap(),
            )
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AttachmentDims.Empty)

    val currentUserId: StateFlow<String?> = auth.userId

    // ── The live connection's state, re-exposed unchanged (EXP-621) ─────────

    val phase: StateFlow<AgentPhase> = connection.phase

    /** Whether the socket is up right now — false through the silent 4008
     *  redial, which holds the Live phase on purpose. The composer gates its
     *  send button on it, so a tap is never a silent no-op. */
    val connected: StateFlow<Boolean> = connection.connected

    val activity: StateFlow<ActivityFeedState> = connection.activity
    val pendingImages: StateFlow<List<PendingAttachment>> = connection.pendingImages
    val steerSending: StateFlow<Boolean> = connection.steerSending
    val steerImageError: StateFlow<String?> = connection.steerImageError

    /** The composer's typed text — held by the connection, so it survives a
     *  reconnect, a back-tap and a rotation alike. */
    val draft: StateFlow<String> = connection.draft

    /** A failed [killSession] call's message (EXP-268) — rendered as a banner. */
    private val _killError = MutableStateFlow<String?>(null)
    val killError: StateFlow<String?> = _killError

    /** Revive the connection on attach (EXP-625). A no-op when the socket is
     *  healthy (which, since EXP-621, is the common case on a return visit),
     *  but it redials a dial loop that died while the screen was away, which
     *  no phase-gated entry could. */
    fun ensureConnected() = connection.kick("screen-attach")

    fun setDraft(text: String) = connection.setDraft(text)

    /** Send the composed message. The draft and the thumbnails clear only if
     *  it actually goes out. */
    fun sendDraft() = connection.sendDraft()

    fun addPendingImage(uri: Uri, bytes: ByteArray, filename: String, mime: String) =
        connection.addPendingImage(uri, bytes, filename, mime)

    fun removePendingImage(index: Int) = connection.removePendingImage(index)

    fun sendQuestionAnswer(
        questionId: String,
        askId: String?,
        keys: List<String>,
        /** EXP-513: the typed reply for a `freeText` option. */
        text: String? = null,
        /** EXP-588: the picked labels, shown for the step until the desktop
         *  resolves the ask. */
        labels: List<String> = emptyList(),
    ) = connection.sendQuestionAnswer(questionId, askId, keys, text, labels)

    fun sendLegacyAnswer(lockKey: String, key: String, lock: Boolean = true) =
        connection.sendLegacyAnswer(lockKey, key, lock)

    fun sendSubmit() = connection.sendSubmit()

    /**
     * Kill the session (EXP-268): tRPC `steer.killSession` flips the synced
     * row to `ended` (which this screen already reacts to) and best-effort
     * kills the live terminal through the relay — so no local state change on
     * success; a failure surfaces via [killError].
     */
    fun killSession() {
        viewModelScope.launch {
            _killError.value = null
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                steerApi.killSession(accountId, codingSessionId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _killError.value = trpcErrorMessage(t, "Couldn't kill the session")
            }
        }
    }

    /** Detach from the connection — it keeps streaming for the next visit,
     *  and is dropped only once it has finished and nobody is watching. */
    override fun onCleared() {
        store.release(codingSessionId)
        super.onCleared()
    }
}
