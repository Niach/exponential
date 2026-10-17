package com.exponential.app.ui.session

import android.net.Uri
import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.DevicesApi
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.agentUsageRefreshCommand
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.SessionIssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncStats
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.AgentAccountsRows
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.ContextSegment
import com.exponential.app.domain.DeviceFreshness
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.HistoryState
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.QueuedMessage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionAccountOption
import com.exponential.app.domain.SessionAccountSwitch
import com.exponential.app.domain.SessionConfigState
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.SessionDeviceMove
import com.exponential.app.domain.SessionDeviceMoveRow
import com.exponential.app.domain.SessionUsageState
import com.exponential.app.domain.SlashCommand
import com.exponential.app.domain.SlashCommands
import com.exponential.app.domain.toSteerDevice
import com.exponential.app.domain.parseLaunchDefaults
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.ui.markdown.AttachmentDims
import com.exponential.app.ui.steer.steerDeviceFlow
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.issueRefTarget
import com.exponential.app.ui.markdown.MentionMember
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withTimeoutOrNull
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
 * EXP-849 phase 3: the account-switch context of ONE live run — everything the
 * gate needs except the turn slot, which the screen already holds. Empty
 * options = the machine reported no login for the run's agent, so there is
 * nothing to switch between and the rows do not render.
 */
data class SessionAccountSwitchState(
    val options: List<SessionAccountOption> = emptyList(),
    val agent: String? = null,
    val mine: Boolean = false,
    val sessionEnded: Boolean = false,
    val deviceOnline: Boolean = false,
    val deviceLabel: String = "",
    /** EXP-909: the run's own `agent_account`, when the row carries one. */
    val currentAccount: String? = null,
) {
    /**
     * Why [option] cannot be switched to right now, or null when it can — the
     * run's half of [SessionAccountSwitch.refusal]; [turnState] is the screen's
     * (EXP-848's live slot, which no flow here holds).
     */
    fun refusal(option: SessionAccountOption, turnState: String): String? =
        SessionAccountSwitch.refusal(
            option = option,
            agent = agent,
            mine = mine,
            sessionEnded = sessionEnded,
            deviceOnline = deviceOnline,
            turnState = turnState,
            currentAccount = currentAccount,
        )

    /**
     * Whether ANY of the machine's logins could take this run right now — what
     * decides if the rate-limit wall offers "Switch account" at all, so the
     * notice never points at a dead end (iOS `canSwitchAnyAccount`).
     */
    fun canSwitchAny(turnState: String): Boolean =
        options.any { refusal(it, turnState) == null }
}

/**
 * EXP-1252: the session composer's Device pin — [SessionDeviceMove]'s rows
 * and gates for ONE session. [visible] false = no pin at all (an agent, an
 * aside); [blockedReason] = the whole pin disabled (can't steer, ended).
 */
data class SessionDeviceMoveState(
    val visible: Boolean = false,
    val blockedReason: String? = null,
    val rows: List<SessionDeviceMoveRow> = emptyList(),
    /** The current device's name, or `Moving to <target>…`. */
    val label: String = "",
    val moving: Boolean = false,
    val deviceId: String? = null,
)

/**
 * The steer screen's ViewModel — a façade over the app-held SteerConnection
 * (EXP-621). It owns only what is genuinely screen-scoped (the host-device
 * presentation, the linked issue's attachment sizes, a failed kill's banner);
 * the socket, the feed, the pending images and the composer draft belong to
 * the connection, which is why a back-tap no longer throws them away.
 */
@HiltViewModel(assistedFactory = AgentSessionViewModel.Factory::class)
class AgentSessionViewModel @AssistedInject constructor(
    /** EXP-893: assisted, not a nav argument — the Work screen acquires one
     *  of these PER SHOWN RUN (`hiltViewModel(key = "session:$id")`), so the
     *  run on show can change under one route. */
    @Assisted val codingSessionId: String,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val issuesApi: IssuesApi,
    private val sessionsApi: SessionsApi,
    private val devicesApi: DevicesApi,
    private val mcpServersApi: com.exponential.app.data.api.McpServersApi,
    private val issueImagesApi: IssueImagesApi,
    private val store: SteerConnectionStore,
    stats: SyncStats,
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(codingSessionId: String): AgentSessionViewModel
    }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** EXP-920: the account DB the transcript's entity-preview sheets resolve
     *  their refs against ([EntityRefResolver]); scoped like every query here. */
    val accountDb: Flow<ExponentialDatabase?> get() = dbFlow

    /** The live connection for this session — shared with every other screen
     *  that has it open, and released in [onCleared]. */
    private val connection = store.acquire(codingSessionId)

    /** The synced `sessions` row — flips to ended via Electric. Shared from
     *  the connection, which needs it eagerly for its redial loop. */
    val session: StateFlow<SessionEntity?> = connection.session

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
        device.offline && row != null && row.status == DomainContract.sessionStatusRunning
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /**
     * EXP-909: the synced `devices` row of the machine this run is on — the
     * ONE join the account, usage and switch flows share. They each resolved
     * it separately before, which meant three copies of the same
     * "prefer the row this user wrote" rule that could disagree the moment one
     * was edited. Same rule as [AgentUsagePresentation.sessionUsage].
     */
    private val hostDeviceRow: Flow<DeviceEntity?> = combine(
        session,
        deviceRows,
    ) { row, devices ->
        val deviceId = row?.deviceId?.takeIf { it.isNotBlank() } ?: return@combine null
        val matches = devices.filter { it.deviceId == deviceId }
        matches.firstOrNull { it.userId == row.userId } ?: matches.firstOrNull()
    }.distinctUntilChanged()

    /**
     * …and that row as the composed [SteerDevice] every capability question is
     * asked of (online-ness, `resume-run`). Recomputed on the device ticker, since presence
     * ages out on its own clock with no write to re-emit on.
     */
    private val hostSteerDevice: StateFlow<SteerDevice?> = combine(
        hostDeviceRow,
        DeviceLiveness.ticker(),
        auth.userId,
    ) { device, now, userId ->
        device?.toSteerDevice(now, userId)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** EXP-1334: the host machine reads the per-run `computerUse` flag (the
     *  "+" menu's Computer use row). */
    val hostCanToggleComputerUse: StateFlow<Boolean> = hostSteerDevice
        .map { it?.canToggleComputerUse == true }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** The host device's synced launch defaults — the composer's model pin
     *  falls back to them before the relay names the model in force. */
    val hostLaunchDefaults: StateFlow<DeviceLaunchDefaults?> = hostDeviceRow
        .map { parseLaunchDefaults(it?.launchDefaults) }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-1291: the issues the session's THREAD links (`session_issues` on its
     * ROOT), in the thread's order — the header names what is worked on.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val issues: StateFlow<List<IssueEntity>> = session
        .map { row -> row?.rootId?.takeIf { it.isNotEmpty() } ?: row?.id }
        .distinctUntilChanged()
        .flatMapLatest { rootId ->
            if (rootId == null) {
                flowOf(emptyList())
            } else {
                combine(
                    dbFlow.scopedQuery(emptyList<SessionIssueEntity>()) { it.sessionIssueDao().observeByRoot(rootId) },
                    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
                ) { links, all ->
                    val byId = all.associateBy { it.id }
                    links.mapNotNull { byId[it.issueId] }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The thread's first issue (null for an issue-less session). */
    val issue: StateFlow<IssueEntity?> = issues
        .map { it.firstOrNull() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-688: the host machine's sign-in for the SAME agent — the Usage
     * overlay's header. Same row-resolution rule as
     * [AgentUsagePresentation.sessionUsage]; null whenever the machine never
     * reported an account for it.
     */
    val agentAccount: StateFlow<AgentAccount?> = combine(
        session,
        hostDeviceRow,
    ) { row, device ->
        val agent = row?.cli?.takeIf { it.isNotBlank() } ?: return@combine null
        AgentUsagePresentation.parseAccounts(device?.agentAccounts)?.get(agent)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-849 phase 3: everything the mid-session account switch gates on,
     * EXCEPT the turn slot (the screen already holds the activity state and
     * passes it into [SessionAccountSwitch.refusal]): the logins the host
     * machine reports for this run's agent, plus whether the run is mine, over,
     * and on a machine that can take the switch.
     *
     * The machine's numbers ride the options, so the rows render their usage
     * bars without a second source. EXP-909: the run's own `agent_account`
     * rides in too, so exactly one option reads as the login the run is ON.
     */
    val accountSwitch: StateFlow<SessionAccountSwitchState> = combine(
        session,
        hostDeviceRow,
        hostSteerDevice,
        // `auth.userId` rather than [currentUserId]: the same flow, declared
        // below this one.
        auth.userId,
    ) { row, device, steerDevice, userId ->
        if (row == null) return@combine SessionAccountSwitchState()
        SessionAccountSwitchState(
            options = SessionAccountSwitch.options(
                AgentUsagePresentation.parseAccounts(device?.agentAccounts),
                row.cli,
                row.account,
            ),
            agent = row.cli,
            // A teammate's run is never steerable (EXP-312) and its account is
            // not this user's to change; the machine must be one we may drive.
            mine = userId != null && row.userId == userId,
            sessionEnded = row.isEnded,
            deviceOnline = steerDevice?.online == true,
            deviceLabel = steerDevice?.deviceLabel?.takeIf { it.isNotBlank() }
                ?: row.deviceId,
            currentAccount = row.account,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SessionAccountSwitchState())

    /**
     * EXP-1252: the Device pin's rows — THE launch composer's machines for the
     * session's team (own + shared into it, offline rows included so they can
     * say why), each with its [SessionDeviceMove] refusal.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val deviceMove: StateFlow<SessionDeviceMoveState> = combine(
        session,
        steerDeviceFlow(dbFlow, session.map { it?.teamId }.distinctUntilChanged(), auth.userId),
        auth.userId,
    ) { row, devices, userId ->
        if (row == null || !SessionDeviceMove.visible(row.kind, row.aside)) {
            return@combine SessionDeviceMoveState()
        }
        val moveTo = row.moveToDeviceId?.takeIf { it.isNotEmpty() }
        val rows = SessionDeviceMove.rows(
            devices.orEmpty(),
            deviceId = row.deviceId.takeIf { it.isNotEmpty() },
            moveToDeviceId = moveTo,
            teamId = row.teamId,
            shared = row.shared,
            cli = row.cli,
        )
        SessionDeviceMoveState(
            visible = true,
            blockedReason = SessionDeviceMove.blockedReason(
                canSteer = SessionDeviceMove.canSteer(row.shared, row.userId, userId),
                ended = row.isEnded,
            ),
            rows = rows,
            label = SessionDeviceMove.pinLabel(rows, row.deviceId, moveTo),
            moving = moveTo != null,
            deviceId = row.deviceId,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SessionDeviceMoveState())

    /**
     * EXP-484/EXP-909: the rate-limit windows the Usage overlay heads with —
     * the numbers of the login THIS RUN spends, not the machine's ambient one.
     * Resolution order (§1, mirrored ×4 in
     * [SessionAccountSwitch.activeAccountIndex]): the run's synced
     * `agent_account`, else the login the machine's top-level report names,
     * else its active login. Only the ACTIVE login's numbers ever ride the
     * pre-profile `agentUsage[agent]` slot, so that is the fallback exactly
     * when the resolved login is the active one — or when it is UNKNOWN, where
     * guessing the ambient login is the one thing this must not do.
     *
     * Null for a run that is over (the overlay retires with it), for a row
     * with no agent, and for a machine that reported nothing. NOT null for
     * numbers that have aged out: EXP-909 dims stale windows and dates them
     * ([AgentUsagePresentation.usageAge]) rather than hiding them.
     */
    val agentUsage: StateFlow<AgentUsage?> = combine(
        session,
        hostDeviceRow,
        accountSwitch,
    ) { row, device, switch ->
        if (row == null || row.isEnded) return@combine null
        val agent = row.cli?.takeIf { it.isNotBlank() } ?: return@combine null
        val ambient = AgentUsagePresentation.parseUsageMap(device?.agentUsage)?.get(agent)
        val current = switch.options.firstOrNull { it.current } ?: return@combine ambient
        current.usage ?: ambient.takeIf { current.active }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-909: one on-demand usage refresh when the overlay OPENS — the same
     * `agent_usage_refresh` command the Devices page queues, on the run's
     * machine and for the run's own login. Once per open, never a polling
     * loop: the machine answers by re-reporting on its next beat (~30s), which
     * arrives through sync.
     *
     * Refused silently when there is nothing to ask (a teammate's machine, an
     * offline one, a build without the cap) or when the last fetch is still
     * inside the device's own rate-limit floor — the device would answer 429,
     * so asking buys a round trip and nothing else.
     */
    fun refreshRunUsage() {
        val row = session.value ?: return
        val agent = row.cli?.takeIf { it.isNotBlank() } ?: return
        val deviceId = row.deviceId.takeIf { it.isNotBlank() } ?: return
        val switch = accountSwitch.value
        if (!switch.mine || !switch.deviceOnline) return
        val profileId = switch.options.firstOrNull { it.current }?.profileId
            ?: row.account?.trim()?.takeIf { it.isNotEmpty() }
            ?: return
        if (AgentAccountsRows.refreshAllowedAt(agentUsage.value, System.currentTimeMillis()) != null) {
            return
        }
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                devicesApi.createCommand(
                    accountId,
                    agentUsageRefreshCommand(deviceId, agent, profileId),
                )
            }
        }
    }

    /**
     * EXP-760 — the run's team issues, newest-first: what resolves the
     * identifiers the agent names in the feed into tappable chips. Scoped to
     * the SESSION's team (a batch / action / chat run has no issue and no
     * board to derive one from), and shaped exactly like
     * IssueDetailViewModel.issueRefCandidates so the chip's status glyph is
     * precomputed here too (EXP-423).
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val issueRefCandidates: StateFlow<List<IssueRefTarget>> = session
        .map { it?.teamId }
        .distinctUntilChanged()
        .flatMapLatest { teamId ->
            if (teamId == null) {
                flowOf(emptyList())
            } else {
                combine(
                    dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() },
                    dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() },
                    dbFlow.scopedQuery(emptyList<IssueStatusEntity>()) {
                        it.issueStatusDao().observeByTeam(teamId)
                    },
                ) { issues, boards, statusRows ->
                    val statuses =
                        if (statusRows.isEmpty()) IssueStatusResolver.builtinDefaults
                        else IssueStatusResolver.teamStatuses(statusRows)
                    val teamBoardIds = boards
                        .filter { it.teamId == teamId }
                        .map { it.id }
                        .toSet()
                    issues
                        .filter { it.boardId in teamBoardIds }
                        .sortedByDescending { it.createdAt }
                        .map { issueRefTarget(it, statuses) }
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * EXP-892: the steer composer's `#` menu server half — `issues.search`
     * over the RUN's team. Failures degrade to local-only.
     */
    suspend fun searchIssueRefs(query: String): List<IssueRefTarget> {
        val accountId = auth.activeAccountId.value ?: return emptyList()
        val teamId = session.value?.teamId ?: return emptyList()
        return try {
            issuesApi.search(accountId, teamId, query).map(::issueRefTarget)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            emptyList()
        }
    }

    /**
     * EXP-802/EXP-805 — the run team's members, the steer composer's `@`
     * vocabulary. The same lookup the issue screens make
     * (`IssueDetailViewModel.teamUsers`, `db.userDao().observeByTeam`), scoped
     * to the SESSION's team for the same reason [issueRefCandidates] is: a
     * batch, action or chat run has no issue and no board to derive one from.
     * A mention is stored as the plain `@email` the server resolves, so the
     * display name only ever labels the row.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val mentionMembers: StateFlow<List<MentionMember>> = session
        .map { it?.teamId }
        .distinctUntilChanged()
        .flatMapLatest { teamId ->
            if (teamId == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeByTeam(teamId) }
            }
        }
        .map { users -> users.map { MentionMember(it.name ?: it.email, it.email) } }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /**
     * Probed sizes of the linked issue's attachments (REV2-79) — the feed
     * renders markdown since EXP-440, so an `![](/api/attachments/…)` in
     * narration or a steered message pre-sizes instead of jumping when the
     * bitmap lands. Batch and action runs have no issue: nothing to pre-size.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val attachmentDims: StateFlow<AttachmentDims> = issue
        .map { it?.id }
        .distinctUntilChanged()
        .flatMapLatest { issueId ->
            if (issueId == null) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList()) { it.attachmentDao().observeByIssue(issueId) }
            }
        }
        .map { rows -> AttachmentDims.fromRows(rows) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AttachmentDims.Empty)

    val currentUserId: StateFlow<String?> = auth.userId

    // ── The live connection's state, re-exposed unchanged (EXP-621) ─────────

    val phase: StateFlow<AgentPhase> = connection.phase

    /** Whether the socket is up right now — false through the silent 4008
     *  redial, which holds the Live phase on purpose. The composer gates its
     *  send button on it, so a tap is never a silent no-op. */
    val connected: StateFlow<Boolean> = connection.connected

    val activity: StateFlow<ActivityFeedState> = connection.activity

    /** EXP-773: an ENDED run's transcript is fetched from the machine that ran
     *  it — this is that fetch's status. Null on a live run. */
    val history: StateFlow<HistoryState?> = connection.history

    val pendingImages: StateFlow<List<PendingAttachment>> = connection.pendingImages

    /** EXP-1291: a `sessions.send` in flight (uploads included). */
    private val _sending = MutableStateFlow(false)
    val steerSending: StateFlow<Boolean> = _sending

    /** A refused pick (the connection's caps) or a failed upload — the text
     *  and the thumbnails survive, so the send is retryable. */
    private val _uploadError = MutableStateFlow<String?>(null)
    val steerImageError: StateFlow<String?> = combine(_uploadError, connection.steerImageError) { own, relay ->
        own ?: relay
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** A failed `sessions.send` / `sessions.unqueue` — FEED-79: the error
     *  toast `Couldn't send the message`, raised once by the screen. */
    private val _sendError = MutableStateFlow<String?>(null)
    val sendError: StateFlow<String?> = _sendError
    fun clearSendError() {
        _sendError.value = null
    }

    /** A send the composer REFUSED before any call (a slash command with
     *  attachments): the plain toast, raised once by the screen. */
    private val _sendRefusal = MutableStateFlow<String?>(null)
    val sendRefusal: StateFlow<String?> = _sendRefusal
    fun clearSendRefusal() {
        _sendRefusal.value = null
    }

    /**
     * EXP-1323: when the agent's newest message on THIS session landed —
     * the session view re-reads the session as news arrives under the
     * reader (the row never turns bold while it is open).
     */
    val newestAgentMessageAt: StateFlow<String?> = dbFlow
        .scopedQuery(emptyList<MessageEntity>()) { it.messageDao().observeBySession(codingSessionId) }
        .map { messages ->
            messages.filter { it.authorKind == DomainContract.messageAuthorKindAgent }.maxOfOrNull { it.createdAt }
        }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-1211: the quick replies above THIS session's composer
     * ([SessionTree.composerReplies] over its synced messages; null = none).
     * The thread's composer reads its ROOT's, the raw view its own.
     */
    val composerReplies: StateFlow<SessionTree.ComposerReplies?> = dbFlow
        .scopedQuery(emptyList<MessageEntity>()) { it.messageDao().observeBySession(codingSessionId) }
        .map { rows -> SessionTree.composerReplies(codingSessionId, rows.map(SessionTree::message)) }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _replyPending = MutableStateFlow(false)
    /** A quick-reply tap or dismiss is in flight: the row is disabled. */
    val replyPending: StateFlow<Boolean> = _replyPending

    /** A refused quick-reply write: `Couldn't send the reply` (or dismiss). */
    private val _replyError = MutableStateFlow<com.exponential.app.ui.components.FailedAction?>(null)
    val replyError: StateFlow<com.exponential.app.ui.components.FailedAction?> = _replyError
    fun clearReplyError() {
        _replyError.value = null
    }

    /** EXP-1211: a tap = `sessions.sendReply` (the text is the next message). */
    fun sendReply(messageId: String, itemId: String) =
        replyCall("send", "the reply") { accountId -> sessionsApi.sendReply(accountId, messageId, itemId) }

    /** EXP-1211: × = `sessions.dismissReplies` (the whole row goes). */
    fun dismissReplies(messageId: String) =
        replyCall("dismiss", "the replies") { accountId -> sessionsApi.dismissReplies(accountId, messageId) }

    private fun replyCall(verb: String, thing: String, call: suspend (accountId: String) -> Unit) {
        if (_replyPending.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _replyPending.value = true
            try {
                call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _replyError.value = com.exponential.app.ui.components.FailedAction(
                    verb,
                    thing,
                    trpcErrorMessage(t, "").ifBlank { null },
                )
            } finally {
                _replyPending.value = false
            }
        }
    }

    /** The session's ROOT id (the thread it belongs to). */
    private val rootId: Flow<String?> = session
        .map { row -> row?.rootId?.takeIf { it.isNotEmpty() } ?: row?.id }
        .distinctUntilChanged()

    /**
     * EXP-1291: the header's back row — the THREAD's title — and this
     * session's own name, both THE shared [SessionTree.sessionTitle]
     * (EXP-1341, fixtures/session-title.json; never `Session N`).
     */
    data class Header(val threadTitle: String, val sessionName: String)

    @OptIn(ExperimentalCoroutinesApi::class)
    val header: StateFlow<Header?> = rootId
        .flatMapLatest { root ->
            if (root == null) {
                flowOf(null)
            } else {
                combine(
                    dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeTree(root) },
                    dbFlow.scopedQuery(emptyList<MessageEntity>()) { it.messageDao().observeByRoot(root) },
                    dbFlow.scopedQuery(emptyList<SessionIssueEntity>()) { it.sessionIssueDao().observeByRoot(root) },
                    issues,
                    session,
                ) { tree, messages, links, linked, row ->
                    val me = row ?: return@combine null
                    val sessions = tree.map(SessionTree::session)
                    val rootRow = sessions.firstOrNull { it.id == root } ?: SessionTree.session(me)
                    val mine = sessions.firstOrNull { it.id == me.id } ?: SessionTree.session(me)
                    val titles = SessionTree.sessionTitles(
                        listOf(rootRow, mine).distinctBy { it.id },
                        links.map(SessionTree::sessionIssue),
                        linked.map { SessionTree.issue(it) },
                        messages.map(SessionTree::message),
                    )
                    val title = titles.getValue(rootRow.id)
                    val name = titles.getValue(mine.id)
                    Header(threadTitle = title, sessionName = name)
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-1291: the viewer's OWN messages the server still holds for this
     * session (`consumed_at` NULL) — the queue bar; X = [unqueueMessage].
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    val queuedMessages: StateFlow<List<QueuedMessage>> = combine(
        dbFlow.scopedQuery(emptyList<MessageEntity>()) { it.messageDao().observeQueued(codingSessionId) },
        auth.userId,
    ) { rows, me ->
        rows.filter { me != null && it.authorId == me }
            .map { QueuedMessage(id = it.id, text = it.body) }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The composer's typed text — held by the connection, so it survives a
     *  reconnect, a back-tap and a rotation alike. */
    val draft: StateFlow<String> = connection.draft

    /**
     * EXP-746: the agent's LIVE configuration behind the composer chips —
     * the options and modes in force, and the commands the agent advertises.
     * Latest-wins state off the activity stream, so a viewer that joins mid-run
     * paints its chips from the replayed snapshot. Null on a PTY run, which
     * publishes none.
     */
    val sessionConfig: StateFlow<SessionConfigState?> = connection.activity
        .map { it.config }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** EXP-746: this run's context window + spend, same latest-wins rule —
     *  the Usage sheet's "Context" block. Null when the engine reports none. */
    val sessionUsage: StateFlow<SessionUsageState?> = connection.activity
        .map { it.usage }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** EXP-1051: how that window is laid out — the device's attribution of
     *  the context it filled before the first turn, the second half of the
     *  Usage sheet's "Context window" block. Null until a device publishes
     *  one: the bar then draws the conversation alone. */
    val sessionContextLayout: StateFlow<List<ContextSegment>?> = connection.activity
        .map { it.contextLayout }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /**
     * EXP-724: the curated slash commands the `/` menu should offer for the
     * CURRENT draft — empty whenever the menu must stay shut. Filtered by the
     * run's own agent (a row with none ran the default one), so the menu never
     * offers a command the desktop could not execute. EXP-746: the run's OWN
     * advertised commands follow the contract rows.
     */
    val slashMatches: StateFlow<List<SlashCommand>> = combine(
        draft,
        session,
        sessionConfig,
    ) { text, row, config ->
        SlashCommands.matches(
            text,
            SlashCommands.agentId(row?.cli, config != null),
            config?.commands.orEmpty(),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** The catalog command the draft would RUN if sent, if any (EXP-724) — the
     *  screen confirms before sending the ones that discard context. EXP-746:
     *  keyed on the run's catalog agent, so an EXTERNAL run (agent-less, but
     *  publishing a `config_state`) never confirms a `/clear` its agent would
     *  only read as prose. */
    fun pendingSlashCommand(): SlashCommand? = SlashCommands.commandFor(
        draft.value,
        SlashCommands.agentId(session.value?.cli, sessionConfig.value != null),
        sessionConfig.value?.commands.orEmpty(),
    )

    // ── EXP-1334: THE session composer's "+" changes a RUNNING session ──────

    /** The person's live picks, ahead of the synced row (null = the row's).
     *  A row pick (plan, effort, ultracode, computer use) stays ONLY until the
     *  synced row says it (web parity: the row then follows the agent, its
     *  `sessions.report` plan mode and its own `/model`); a DISPATCHER's never
     *  reads back, so its picks stay. `mcpServerIds` has no row column: it is
     *  the read itself, seeded by `sessions.mcpPicks`. */
    data class ConfigurePicks(
        val planMode: Boolean? = null,
        val effort: String? = null,
        val ultracode: Boolean? = null,
        val computerUse: Boolean? = null,
        val mcpServerIds: List<String>? = null,
    )

    private val _configurePicks = MutableStateFlow(ConfigurePicks())
    val configurePicks: StateFlow<ConfigurePicks> = _configurePicks

    init {
        // A late sync: whenever the synced row says a pick, the override goes.
        viewModelScope.launch {
            session.filterNotNull().collect { row ->
                if (!row.dispatch) _configurePicks.update { it.dropSettledBy(row) }
            }
        }
    }

    private val _configureError = MutableStateFlow<String?>(null)
    /** A refused `sessions.configure` — `Couldn't change the session options`. */
    val configureError: StateFlow<String?> = _configureError

    fun clearConfigureError() {
        _configureError.value = null
    }

    private val _mcpServers = MutableStateFlow<List<com.exponential.app.domain.McpServerOption>>(emptyList())
    /** The team's MCP servers (`mcpServers.list`, fetched once per team). */
    val mcpServers: StateFlow<List<com.exponential.app.domain.McpServerOption>> = _mcpServers
    private var mcpTeam: String? = null

    /** Fetch the session's team MCP servers once (the "+" menu's row) and,
     *  independently, the session's OWN MCP picks (the row's seed). A failed
     *  list releases the latch, so the next call retries. */
    fun loadMcpServers() {
        val team = session.value?.teamId?.takeIf { it.isNotEmpty() } ?: return
        if (mcpTeam == team) return
        mcpTeam = team
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: run {
                mcpTeam = null
                return@launch
            }
            val servers = runCatching { mcpServersApi.list(accountId, team) }
                .onFailure { if (it is CancellationException) throw it }
                .getOrNull()
            if (servers != null) _mcpServers.value = servers else mcpTeam = null
            // The MCP row starts from the session's OWN picks; a failure
            // leaves the row empty (web parity) until a pick.
            if (_configurePicks.value.mcpServerIds != null) return@launch
            val picks = runCatching { sessionsApi.mcpPicks(accountId, codingSessionId) }
                .onFailure { if (it is CancellationException) throw it }
                .getOrNull() ?: return@launch
            if (_configurePicks.value.mcpServerIds == null) {
                _configurePicks.value = _configurePicks.value.copy(mcpServerIds = picks.mcpServerIds)
            }
        }
    }

    /** The "+"'s Run action › rows: the session team's saved actions. */
    @OptIn(ExperimentalCoroutinesApi::class)
    val menuActions: StateFlow<List<com.exponential.app.data.db.ActionEntity>?> = session
        .map { it?.teamId }
        .distinctUntilChanged()
        .flatMapLatest { teamId ->
            if (teamId.isNullOrEmpty()) {
                flowOf(emptyList())
            } else {
                dbFlow.scopedQuery(emptyList<com.exponential.app.data.db.ActionEntity>()) {
                    it.actionDao().observeByTeam(teamId)
                }
            }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The "+"'s Implement issue ›: link the picks to the session's ROOT
     *  (`sessions.linkIssues`; a manual link wakes the dispatcher). */
    fun implementIssues(issueIds: List<String>) {
        if (issueIds.isEmpty()) return
        val rootId = session.value?.rootId?.takeIf { it.isNotEmpty() } ?: codingSessionId
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: throw IllegalStateException("No active account")
                sessionsApi.linkIssues(accountId, rootId, issueIds)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _configureError.value = trpcErrorMessage(t, "")
            }
        }
    }

    /** `sessions.configure` with the changed fields; the pick shows at once,
     *  stays until the synced row confirms it (bounded: [CONFIGURE_SYNC_WAIT_MS],
     *  web parity) and rolls back on a refusal. A dispatcher's row never
     *  reflects its work picks: those stay as the read. */
    fun configure(change: ConfigurePicks) {
        val previousMcp = _configurePicks.value.mcpServerIds
        _configurePicks.update { it.overriddenBy(change) }
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: throw IllegalStateException("No active account")
                sessionsApi.configure(
                    accountId,
                    com.exponential.app.data.api.SessionConfigureInput(
                        sessionId = codingSessionId,
                        planMode = change.planMode,
                        effort = change.effort,
                        ultracode = change.ultracode,
                        computerUse = change.computerUse,
                        mcpServerIds = change.mcpServerIds,
                    ),
                )
                if (session.value?.dispatch == true) return@launch
                // The synced row now says it (or the wait ran out: the row is
                // the read either way, as on web).
                withTimeoutOrNull(CONFIGURE_SYNC_WAIT_MS) {
                    session.first { row -> row != null && change.rowPicks().dropSettledBy(row).hasRowPick().not() }
                }
                _configurePicks.update { it.dropSettled(change) }
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _configurePicks.update { current ->
                    current.dropSettled(change)
                        .let { if (change.mcpServerIds != null) it.copy(mcpServerIds = previousMcp) else it }
                }
                _configureError.value = trpcErrorMessage(t, "")
            }
        }
    }

    /** A failed [stopSession] call's message — FEED-79: raised as the error
     *  toast once, then [clearKillError]ed. */
    private val _killError = MutableStateFlow<String?>(null)
    val killError: StateFlow<String?> = _killError

    fun clearKillError() {
        _killError.value = null
    }

    /** Revive the connection on attach (EXP-625). A no-op when the socket is
     *  healthy (which, since EXP-621, is the common case on a return visit),
     *  but it redials a dial loop that died while the screen was away, which
     *  no phase-gated entry could. */
    fun ensureConnected() = connection.kick("screen-attach")

    fun setDraft(text: String) = connection.setDraft(text)

    /**
     * EXP-1291: send the composed message as `sessions.send({sessionId})` —
     * the server holds it until the device takes it (an idle session resumes,
     * an ended one continues). Picked files upload against the session first
     * and ride as `attachmentIds`. The draft and the thumbnails clear only
     * once the server took it.
     */
    fun sendDraft() {
        val text = draft.value
        val images = pendingImages.value
        if (text.isBlank() && images.isEmpty()) return
        if (_sending.value) return
        // A slash command (`/btw` included) is the WHOLE message: with
        // attachments around it the send refuses, like web — never an
        // ordinary message carrying the command text.
        if (images.isNotEmpty() && pendingSlashCommand() != null) {
            _sendRefusal.value = SlashCommands.COMMAND_WITH_ATTACHMENTS
            return
        }
        // EXP-1323: `/btw <question>` = a read-only aside child of THIS
        // session (its answer lands as a child card), never a message to it.
        val aside = SlashCommands.asideQuestion(text, SlashCommands.agentId(session.value?.cli, sessionConfig.value != null))
        if (aside != null) {
            viewModelScope.launch {
                val accountId = auth.activeAccountId.value ?: return@launch
                _sending.value = true
                try {
                    sessionsApi.send(accountId, SessionsSendInput.aside(codingSessionId, aside))
                    connection.setDraft("")
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    _sendError.value = trpcErrorMessage(t, "")
                } finally {
                    _sending.value = false
                }
            }
            return
        }
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _sending.value = true
            _uploadError.value = null
            try {
                val ids = mutableListOf<String>()
                for (image in images) {
                    val id = image.uploadedId ?: try {
                        issueImagesApi.uploadSessionImage(
                            accountId,
                            codingSessionId,
                            image.bytes,
                            image.filename,
                            image.contentType,
                        ).also { connection.stampPendingUpload(image, it.id, it.filename) }.id
                    } catch (t: Throwable) {
                        if (t is CancellationException) throw t
                        _uploadError.value = trpcErrorMessage(
                            t,
                            if (image.isImage) "The image could not be uploaded" else "The file could not be uploaded",
                        )
                        return@launch
                    }
                    ids += id
                }
                try {
                    sessionsApi.send(
                        accountId,
                        SessionsSendInput.message(codingSessionId, text, ids.takeIf { it.isNotEmpty() }),
                    )
                } catch (t: Throwable) {
                    if (t is CancellationException) throw t
                    _sendError.value = trpcErrorMessage(t, "")
                    return@launch
                }
                for (index in images.indices.reversed()) connection.removePendingImage(index)
                connection.setDraft("")
            } finally {
                _sending.value = false
            }
        }
    }

    /** EXP-893: send a command line (`/model opus`) as a plain message
     *  WITHOUT touching the draft — the composer footer's model picker rides
     *  this, so a half-typed message is never clobbered by a pick. */
    fun sendCommand(text: String): Boolean {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.send(accountId, SessionsSendInput.message(codingSessionId, text))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _sendError.value = trpcErrorMessage(t, "")
            }
        }
        return true
    }

    /** EXP-849: the profile a switch is in flight to (its row spins). */
    private val _switchingAccount = MutableStateFlow<String?>(null)
    val switchingAccount: StateFlow<String?> = _switchingAccount

    /** A failed switch — `Couldn't switch the account`, raised once by the screen. */
    private val _switchError = MutableStateFlow<String?>(null)
    val switchError: StateFlow<String?> = _switchError
    fun clearSwitchError() {
        _switchError.value = null
    }

    /**
     * EXP-849: continue this session on [option]'s login — ONE
     * `sessions.send({sessionId, account})`, no text. The host machine resumes
     * the same row on it between turns; the synced `account` re-marks the
     * current row, so success needs nothing else.
     */
    fun switchAccount(option: SessionAccountOption) {
        if (_switchingAccount.value != null) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _switchingAccount.value = option.profileId
            try {
                sessionsApi.send(
                    accountId,
                    SessionsSendInput.switchAccount(codingSessionId, SessionAccountSwitch.wireAccount(option)),
                )
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _switchError.value = trpcErrorMessage(t, "")
            } finally {
                _switchingAccount.value = null
            }
        }
    }

    /** A failed move — `Couldn't move the session`, raised once by the screen. */
    private val _moveError = MutableStateFlow<String?>(null)
    val moveError: StateFlow<String?> = _moveError
    fun clearMoveError() {
        _moveError.value = null
    }

    /**
     * EXP-1252: [picked] in the Device pin — another device = MOVE the session
     * there, the current device while a move is pending = CANCEL it; both ONE
     * `sessions.send({sessionId, deviceId})`. The synced `move_to_device_id`
     * relabels the pin, so success needs nothing else.
     */
    fun moveToDevice(picked: String) {
        val row = session.value ?: return
        SessionDeviceMove.pick(picked, row.deviceId, row.moveToDeviceId?.takeIf { it.isNotEmpty() }) ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.send(accountId, SessionsSendInput.moveToDevice(codingSessionId, picked))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _moveError.value = trpcErrorMessage(t, "")
            }
        }
    }

    /** EXP-1291: revoke one of the viewer's held messages (`sessions.unqueue`). */
    fun unqueueMessage(messageId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.unqueue(accountId, messageId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _sendError.value = trpcErrorMessage(t, "")
            }
        }
    }

    fun addPendingImage(uri: Uri, bytes: ByteArray, filename: String, mime: String) =
        connection.addPendingImage(uri, bytes, filename, mime)

    fun removePendingImage(index: Int) = connection.removePendingImage(index)

    fun refusePendingAttachment(message: String) = connection.refusePendingAttachment(message)

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

    /** EXP-820: reject a plan with feedback — the reject key answers the card,
     *  the text follows as the next message (see the connection). */
    fun answerThenSend(
        questionId: String,
        askId: String?,
        keys: List<String>,
        labels: List<String>,
        text: String,
    ) = connection.answerThenSend(questionId, askId, keys, labels, text)

    /** EXP-746: switch the run into one of its advertised modes.
     *  Fire-and-forget — the re-emitted `config_state` IS the confirmation, so
     *  nothing here waits, locks or times out. EXP-772 retired the option
     *  pickers beside it: model and effort are launch decisions now. */
    fun setMode(id: String) = connection.setMode(id)

    /** EXP-790: interrupt the turn in flight — the composer's Stop glyph.
     *  Fire-and-forget like [setMode]; the feed shows the agent stopping. */
    fun interrupt() {
        if (connected.value) connection.interrupt() else stopSession()
    }

    /** EXP-861: revoke a message the device is holding for the next turn —
     *  the queue bar's X. Fire-and-forget; the next `queue` event confirms. */
    fun unqueue(id: String) = connection.unqueue(id)

    /** EXP-783: whether the DEVICE still holds transcript below the oldest row
     *  this client has, and the ask that fetches the next page of it. */
    fun canLoadEarlier(): Boolean = connection.canLoadEarlier()

    fun loadEarlier() {
        connection.loadEarlier()
    }

    /**
     * EXP-1291: Stop = `sessions.stop` — interrupts the current turn; the
     * session stays live and goes idle (every member may stop a SHARED one).
     * A failure surfaces via [killError].
     */
    fun stopSession() = sessionCall("Couldn't stop the session") { accountId ->
        sessionsApi.stop(accountId, codingSessionId)
    }

    /** End the session for good (`sessions.end`; a root ends its tree). */
    fun endSession() = sessionCall("Couldn't end the session") { accountId ->
        sessionsApi.end(accountId, codingSessionId)
    }

    private fun sessionCall(fallback: String, call: suspend (String) -> Unit) {
        viewModelScope.launch {
            _killError.value = null
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _killError.value = trpcErrorMessage(t, fallback)
            }
        }
    }

    /** Detach from the connection — it keeps streaming for the next visit,
     *  and is dropped only once it has finished and nobody is watching. */
    /** EXP-1323: opening a session view reads it (`sessions.markRead`);
     *  background, so a failure stays silent. */
    fun markRead() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.markRead(accountId, codingSessionId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
            }
        }
    }

    override fun onCleared() {
        store.release(codingSessionId)
        super.onCleared()
    }
}

// ── EXP-1334: the configure picks over the synced row ───────────────────────

/** How long a configure waits (bounded) for its row to sync (web parity). */
internal const val CONFIGURE_SYNC_WAIT_MS = 3_000L

/** The picks with [change]'s set keys laid over them. */
internal fun AgentSessionViewModel.ConfigurePicks.overriddenBy(
    change: AgentSessionViewModel.ConfigurePicks,
) = AgentSessionViewModel.ConfigurePicks(
    planMode = change.planMode ?: planMode,
    effort = change.effort ?: effort,
    ultracode = change.ultracode ?: ultracode,
    computerUse = change.computerUse ?: computerUse,
    mcpServerIds = change.mcpServerIds ?: mcpServerIds,
)

/** The picks the synced row can confirm (everything but the MCP pick). */
internal fun AgentSessionViewModel.ConfigurePicks.rowPicks() = copy(mcpServerIds = null)

/** True while any row-backed pick is set. */
internal fun AgentSessionViewModel.ConfigurePicks.hasRowPick() =
    planMode != null || effort != null || ultracode != null || computerUse != null

/** Drops the row-backed overrides [picks] set, unless a later pick replaced
 *  them (web `dropSettled`). The MCP pick is never a row's: it stays. */
internal fun AgentSessionViewModel.ConfigurePicks.dropSettled(
    picks: AgentSessionViewModel.ConfigurePicks,
) = AgentSessionViewModel.ConfigurePicks(
    planMode = planMode?.takeUnless { it == picks.planMode },
    effort = effort?.takeUnless { it == picks.effort },
    ultracode = ultracode?.takeUnless { it == picks.ultracode },
    computerUse = computerUse?.takeUnless { it == picks.computerUse },
    mcpServerIds = mcpServerIds,
)

/** Drops every override the synced [row] already says. The row's null effort
 *  is the CLI default, which the picker sends as `""`. */
internal fun AgentSessionViewModel.ConfigurePicks.dropSettledBy(row: SessionEntity) = dropSettled(
    AgentSessionViewModel.ConfigurePicks(
        planMode = row.planMode,
        effort = row.effort.orEmpty(),
        ultracode = row.ultracode,
        computerUse = row.computerUse,
    ),
)
