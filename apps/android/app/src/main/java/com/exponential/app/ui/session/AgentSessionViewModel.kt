package com.exponential.app.ui.session

import android.net.Uri
import android.os.SystemClock
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.DeviceFreshness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.SlashCommand
import com.exponential.app.domain.SlashCommands
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.ui.issue.StartIssueOption
import com.exponential.app.ui.markdown.AttachmentDims
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerLaunchDelegate
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
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
    private val issuesApi: IssuesApi,
    private val store: SteerConnectionStore,
    private val steerLaunch: SteerLaunchDelegate,
    stats: SyncStats,
) : ViewModel() {

    val codingSessionId: String = savedStateHandle["codingSessionId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The live connection for this session — shared with every other screen
     *  that has it open, and released in [onCleared]. */
    private val connection = store.acquire(codingSessionId)

    /** The synced coding_sessions row — flips to ended via Electric. Shared
     *  from the connection, which needs it eagerly for its redial loop. */
    val session: StateFlow<CodingSessionEntity?> = connection.session

    /** EXP-656: when our own `devices` shape last completed a poll — presence
     *  derived from an unrefreshed cursor is unknown, never offline. */
    @OptIn(ExperimentalCoroutinesApi::class)
    private val devicesPolledAt: Flow<Long> =
        auth.activeAccountId.flatMapLatest { stats.devicesPolledAt(it) }

    /**
     * EXP-549/550: the session's host machine, joined to its LIVE devices row
     * — the CURRENT label (a rename must land here, not the start-time
     * snapshot) and whether the machine dropped offline. Recomputed on the
     * device ticker, since Room flows only re-emit on writes and the 90s
     * freshness window elapses on its own.
     */
    /** Every synced machine row — the host join and the usage bar read it. */
    private val deviceRows: Flow<List<DeviceEntity>> =
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() }

    val hostDevice: StateFlow<SessionDevicePresentation> = combine(
        session,
        deviceRows,
        DeviceLiveness.ticker(),
        devicesPolledAt,
    ) { row, devices, now, polledAt ->
        if (row == null) {
            SessionDevicePresentation.Unknown
        } else {
            resolveSessionDevice(
                row,
                devices,
                now,
                // The stamp is elapsedRealtime, so the window is measured on
                // that clock — not on the wall clock the ticker emits.
                devicesFresh = DeviceFreshness.isTrustworthy(
                    polledAt,
                    SystemClock.elapsedRealtime(),
                ),
            )
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
     * EXP-484: the host machine's usage for the agent THIS run launched with —
     * the strip above the feed. Null whenever anything is missing (an ended
     * run, a row with no agent, a machine that never reported, numbers older
     * than the freshness window): the rules live in
     * [AgentUsagePresentation.sessionUsage], which the four clients share.
     *
     * Recomputed on the device ticker for the same reason [hostDevice] is —
     * usage ages out on its own clock, with no write to re-emit on.
     */
    val agentUsage: StateFlow<AgentUsage?> = combine(
        session,
        deviceRows,
        DeviceLiveness.ticker(),
    ) { row, devices, now ->
        row?.let { AgentUsagePresentation.sessionUsage(it, devices, now) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-688: this run's OWN issue — the header names what is being worked
     * on, the way the Agents list row does. Null for batch and action runs
     * (they have none) and until an issue-scoped run's issue syncs in.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val issue: StateFlow<IssueEntity?> = session
        .map { it?.issueId }
        .distinctUntilChanged()
        .flatMapLatest { issueId ->
            if (issueId == null) {
                flowOf(null)
            } else {
                dbFlow.scopedQuery(null) { it.issueDao().observeById(issueId) }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-688: the host machine's sign-in for the SAME agent — the Usage
     * sheet's `signed in as …` caption. Same row-resolution rule as
     * [AgentUsagePresentation.sessionUsage]; null whenever the machine never
     * reported an account for it.
     */
    val agentAccount: StateFlow<AgentAccount?> = combine(
        session,
        deviceRows,
    ) { row, devices ->
        val agent = row?.agent?.takeIf { it.isNotBlank() } ?: return@combine null
        val deviceId = row.deviceId ?: return@combine null
        val matches = devices.filter { it.deviceId == deviceId }
        val device = matches.firstOrNull { it.userId == row.userId }
            ?: matches.firstOrNull()
            ?: return@combine null
        AgentUsagePresentation.parseAccounts(device.agentAccounts)?.get(agent)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

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

    /**
     * EXP-724: the curated slash commands the `/` menu should offer for the
     * CURRENT draft — empty whenever the menu must stay shut. Filtered by the
     * run's own agent (a row with none ran the default one), so the menu never
     * offers a command the desktop could not execute.
     */
    val slashMatches: StateFlow<List<SlashCommand>> = combine(
        draft,
        session,
    ) { text, row ->
        SlashCommands.matches(text, row?.agent)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The catalog command the draft would RUN if sent, if any (EXP-724) — the
     *  screen confirms before sending the ones that discard context. */
    fun pendingSlashCommand(): SlashCommand? =
        SlashCommands.commandFor(draft.value, session.value?.agent)

    /**
     * EXP-678: the issue whose PR the Merge pill above the composer merges —
     * this run's own issue, or, for an issueless batch run in review, the
     * batch PR's representative issue resolved client-side (EXP-535: batch
     * sessions carry no issue linkage, only the branch). An action run merges
     * nothing, so [isBatchInReview] excludes it and the pill stays hidden.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val mergeIssue: StateFlow<IssueEntity?> = session
        .flatMapLatest { row ->
            when {
                row == null -> flowOf(null)
                row.issueId != null ->
                    dbFlow.scopedQuery(null) { it.issueDao().observeById(row.issueId) }
                row.isBatchInReview -> combine(
                    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
                    dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
                ) { issues, boards ->
                    resolveBatchPrIssue(
                        openBatchPrRepresentatives(issues, boards, row.teamId),
                        row.branch,
                    )
                }
                else -> flowOf(null)
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging

    /**
     * A failed merge (EXP-678) — the same banner shape as [killError], cleared
     * by the next attempt. EXP-706: typed, not a bare string, because the bar
     * swaps its Merge pill for the "Fix conflicts" run on a REAL conflict
     * (EXP-533's rule, already modelled for Agents and Reviews).
     */
    private val _mergeError = MutableStateFlow<MergeFailure?>(null)
    val mergeError: StateFlow<MergeFailure?> = _mergeError

    /**
     * Squash-merge [mergeIssue]'s PR. The server merges AND ends this session
     * (EXP-498), so there is nothing to change locally: the row flips to
     * `ended` and the PR to `merged` via Electric, and the screen reacts.
     */
    fun merge() {
        val issueId = mergeIssue.value?.id ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _mergeError.value = null
            _merging.value = true
            runCatching { issuesApi.mergePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    // Conflicts, branch protection and GitHub App errors are the
                    // common, persistent failures — same copy as Agents/Reviews.
                    _mergeError.value =
                        MergeFailure.from(t, "The pull request could not be merged")
                }
            _merging.value = false
        }
    }

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

    // ── Remote start (EXP-706) ───────────────────────────────────────────────
    // A conflict-refused merge swaps the bar's Merge pill for the builtin
    // "Fix merge conflicts" run — the same rails Reviews and the Changes tab
    // already ride, so the plumbing is the shared delegate, not a copy.
    val steerLaunchEnabled: StateFlow<Boolean?> get() = steerLaunch.enabled
    val steerDevices: StateFlow<List<SteerDevice>?> get() = steerLaunch.devices
    val startCandidates: StateFlow<List<StartIssueOption>> get() = steerLaunch.startCandidates
    val runState: StateFlow<ActionRunState> get() = steerLaunch.runState
    val startedSessionId: StateFlow<String?> get() = steerLaunch.startedSessionId

    fun consumeStartedSession() = steerLaunch.consumeStartedSession()

    fun runAction(
        device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: Map<String, String>,
    ) = steerLaunch.runAction(device, action, options, inputs)

    fun startCoding(device: SteerDevice, issueIds: List<String>, options: SteerStartOptions) =
        steerLaunch.startCoding(device, issueIds, options)

    init {
        steerLaunch.attach(viewModelScope)
    }

    /** Detach from the connection — it keeps streaming for the next visit,
     *  and is dropped only once it has finished and nobody is watching. */
    override fun onCleared() {
        store.release(codingSessionId)
        super.onCleared()
    }
}
