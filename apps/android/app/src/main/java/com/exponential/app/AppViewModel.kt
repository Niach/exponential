package com.exponential.app

import com.exponential.app.domain.FeedbackStates
import com.exponential.app.ui.components.Blocking
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.TeamSwitch
import com.exponential.app.data.applyTeamSwitch
import com.exponential.app.data.planTeamSwitch
import com.exponential.app.domain.currentBoardFor
import com.exponential.app.ui.inbox.activeTeamUnread
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.UpdateGate
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.electric.SyncHealth
import com.exponential.app.data.electric.SyncHealthTracker
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.data.steer.SteerConnectionStore
import com.exponential.app.domain.defaultTeamId
import com.exponential.app.domain.TeamLiveRuns
import com.exponential.app.domain.liveRunsByTeam
import com.exponential.app.domain.lightsLiveDot
import com.exponential.app.domain.activeTeamDrafts
import com.exponential.app.domain.resolveDraftRows
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class AppState(
    val instanceUrl: String? = null,
    val token: String? = null,
    val activeAccountId: String? = null,
    val accounts: List<ServerAccount> = emptyList(),
    // Non-null once the ACTIVE account's server has answered HTTP 426 (this
    // build is below that server's configured minimum, EXP-104) — drives the
    // blocking "Update required" gate. Keyed per instance (REV2-18): a
    // background account's 426 never blocks the app, it only stops that
    // account's sync and raises [gatedOtherServers].
    val updateRequired: UpdateGate.UpgradeInfo? = null,
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
    private val enetNodeManager: EnetNodeManager,
    private val databaseHolder: DatabaseHolder,
    private val teamSelection: TeamSelection,
    private val updateGate: UpdateGate,
    private val authApi: AuthApi,
    private val steerConnectionStore: SteerConnectionStore,
    private val syncHealthTracker: SyncHealthTracker,
) : ViewModel() {

    init {
        // Team selection is a global StateFlow; when the active account
        // changes (switch, or login re-keying a pending id to a per-user id) the
        // old selected team id belongs to a different DB. Clear it so the
        // new account resolves its own default (drop(1) skips the initial value).
        viewModelScope.launch {
            // activeAccountId is a StateFlow (already conflated/distinct); drop(1)
            // skips the initial value so we only clear on an actual switch.
            auth.activeAccountId
                .drop(1)
                .collect {
                    teamSelection.clearSelection()
                    // The app-held steer sockets (EXP-621) carry the OLD
                    // account's ticket and their feeds are its data — an
                    // account switch has to end them, not hand them over.
                    steerConnectionStore.closeAll()
                }
        }
        // EXP-166/EXP-168: default-team bootstrap. selectedId starts null
        // (and re-nulls on account switch / team deletion) while Agents +
        // Reviews gate on it — so resolve a default HERE (the app shell always
        // runs) instead of relying on the Issues tab having mounted. Priority:
        // the team of the last-opened board (what the Issues root
        // shows), else the first synced team (iOS AppNavigator parity).
        // Writes only while the selection is null, so explicit switches
        // (Settings → Teams) and the onboarding/create-board selects are
        // never overridden.
        @OptIn(ExperimentalCoroutinesApi::class)
        viewModelScope.launch {
            combine(
                auth.activeAccountId,
                teamSelection.selectedId,
                teamSelection.lastBoardVersion, // re-resolve after switcher picks
            ) { accountId, selected, _ -> accountId to selected }
                .flatMapLatest { (accountId, selected) ->
                    if (accountId == null || selected != null) {
                        flowOf<Pair<String, String?>?>(null)
                    } else {
                        // The db derives from the SAME accountId emission —
                        // combining accountDatabaseFlow separately could pair a
                        // stale db with a newer account mid-switch and select a
                        // team from the previous account's database.
                        val db = databaseHolder.database(forAccountId = accountId)
                        combine(
                            db.teamDao().observeAll(),
                            db.boardDao().observeAll(),
                        ) { teams, boards ->
                            // A cross-server team switch parked its team for
                            // this account (TeamSelection.applyTeamSwitch).
                            val pending = teamSelection.pendingTeam(accountId)
                                ?.takeIf { id -> teams.any { it.id == id } }
                            accountId to (
                                pending ?: defaultTeamId(
                                    teams,
                                    boards,
                                    teamSelection.lastBoard(accountId),
                                )
                                )
                        }
                    }
                }
                .collect { resolved ->
                    val (accountId, defaultId) = resolved ?: return@collect
                    // The account guard closes the tail of the switch race: a
                    // resolve computed for an account that is no longer active
                    // must never write.
                    if (defaultId != null && auth.activeAccountId.value == accountId) {
                        teamSelection.selectIfNull(defaultId)
                        teamSelection.clearPendingTeam(accountId)
                    }
                }
        }
        // Stale-selection guard (EXP-43 hardening): a deleted team leaves
        // the global selection pointing at a row that no longer exists in Room
        // (Electric removes it), which future consumers of selectedId would
        // trip over. Clear it once the id is confirmed gone. The delay absorbs
        // legitimate transients — e.g. the cross-server Settings tap selects
        // the target team BEFORE its account switch lands, so the id is
        // briefly absent from the still-active DB; collectLatest cancels the
        // pending clear as soon as the id resolves (or db/selection change).
        @OptIn(ExperimentalCoroutinesApi::class)
        viewModelScope.launch {
            combine(
                accountDatabaseFlow(auth, databaseHolder),
                teamSelection.selectedId,
            ) { db, id -> db to id }
                .flatMapLatest { (db, id) ->
                    if (db == null || id == null) flowOf(false)
                    else db.teamDao().observeById(id).map { it == null }
                }
                .collectLatest { stale ->
                    if (!stale) return@collectLatest
                    delay(2_000)
                    teamSelection.clearSelection()
                }
        }
        // REV2-18: a server that 426s rejects every request from this build, so
        // keep its 16 shape loops from polling forever — for the active account
        // (whose screen is the blocking gate) and, crucially, for background
        // accounts, which used to keep re-triggering the process-global latch.
        viewModelScope.launch {
            combine(updateGate.gated, auth.accounts) { gated, accounts ->
                accounts.filter { it.token != null && it.isGated(gated) }.map { it.id }
            }.collectLatest { ids ->
                if (ids.isEmpty()) return@collectLatest
                // Re-assert on a backing-off schedule rather than stopping once:
                // SyncManager's reconcile consumes the SAME auth.accounts
                // emission on its own dispatcher, so on an account-set change a
                // stop issued here can land BEFORE the relaunch it is meant to
                // undo — and nothing re-emits afterwards to fix it up (the latch
                // is first-wins, so later 426s yield an equal map the StateFlow
                // dedupes), leaving that account's 16 loops polling a server
                // that answers nothing but 426. Cancelling an already-cancelled
                // pipeline is a map lookup against an equal stats map, so the
                // steady-state cost is a no-op tick a minute, and collectLatest
                // drops the whole schedule the moment a newer gated/accounts
                // emission supersedes it.
                var backoffMs = GATED_STOP_REASSERT_MIN_MS
                while (true) {
                    ids.forEach { syncManager.signOut(it) }
                    delay(backoffMs)
                    backoffMs = (backoffMs * 2).coerceAtMost(GATED_STOP_REASSERT_MAX_MS)
                }
            }
        }
    }

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
        auth.activeAccountId,
        auth.accounts,
        updateGate.gated,
    ) { url, token, activeId, accounts, gated ->
        AppState(
            instanceUrl = url,
            token = token,
            activeAccountId = activeId,
            accounts = accounts,
            updateRequired = url?.let { UpdateGate.originKey(it) }?.let { gated[it] },
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, AppState())

    // Signed-in servers OTHER than the active one that answered 426 (REV2-18).
    // Their pipelines are stopped below; this is the banner that says so, since
    // silently frozen background sync reads as a broken app.
    val gatedOtherServers: StateFlow<List<String>> = combine(
        updateGate.gated,
        auth.accounts,
        auth.activeAccountId,
    ) { gated, accounts, activeId ->
        accounts
            .filter { it.id != activeId && it.token != null && it.isGated(gated) }
            .map { it.displayName }
            .distinct()
    }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    // EXP-533: is the ACTIVE account's sync reaching its server? Drives the
    // floating offline banner. WhileSubscribed, because the model re-evaluates
    // on a 2s timer while a failure streak is open and nothing off-screen
    // needs that.
    val syncHealth: StateFlow<SyncHealth> =
        syncHealthTracker.activeHealth(auth.activeAccountId)
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SyncHealth.Ok)

    // Unread notifications of the ACTIVE team on the active account — drives
    // the inbox dot. Counted exactly like the inbox page (chats.json: the
    // page lists the active team's rows only, `activeTeamUnread`), so the
    // dot never lights for another team's rows. Re-scopes reactively on
    // account and team switch.
    @OptIn(ExperimentalCoroutinesApi::class)
    val unreadCount: StateFlow<Int> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        auth.accounts,
    ) { db, activeId, accounts ->
        db to accounts.firstOrNull { it.id == activeId }?.userId
    }.flatMapLatest { (db, userId) ->
        if (db == null || userId == null) flowOf(0)
        else combine(
            db.notificationDao().observeByUser(userId),
            db.issueDao().observeAll(),
            db.boardDao().observeAll(),
            teamSelection.selectedId,
        ) { notifications, issues, boards, teamId ->
            activeTeamUnread(notifications, issues, boards, teamId)
        }
    }.distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    // EXP-1272: the ACTIVE team's drafts (web `useDraftCount(team.id)`, iOS
    // `draftTeamIds` against the active team), resolved exactly like the
    // Inbox's Drafts section (`DraftsViewModel`, `resolveDraftRows`: a draft
    // whose board no longer resolves drops out), so the Inbox tab's dot and
    // that section always agree, and a team switch moves the dot at once.
    // Statuses do not change the count.
    @OptIn(ExperimentalCoroutinesApi::class)
    val draftCount: StateFlow<Int> = accountDatabaseFlow(auth, databaseHolder)
        .flatMapLatest { db ->
            if (db == null) flowOf(0)
            else combine(
                db.issueDraftDao().observeAll(),
                db.boardDao().observeAll(),
                teamSelection.selectedId,
            ) { drafts, boards, teamId ->
                resolveDraftRows(activeTeamDrafts(drafts, teamId), boards, emptyMap()).size
            }
        }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    // EXP-1291: true while at least one of MY sessions is live in ANY member
    // team (`lightsLiveDot`: not ended; an agent only while busy / waiting) —
    // drives the board switcher's per-team dots. Own sessions only: a
    // teammate's may never light a dot over an empty screen.
    @OptIn(ExperimentalCoroutinesApi::class)
    private val myLiveSessions = accountDatabaseFlow(auth, databaseHolder)
        .flatMapLatest { db ->
            if (db == null) flowOf(emptyList())
            else combine(db.sessionDao().observeLive(), auth.userId) { sessions, me ->
                if (me == null) emptyList() else sessions.filter { it.userId == me && lightsLiveDot(it) }
            }
        }

    // EXP-1075: the caller's LIVE sessions grouped by team (per-team live
    // dots; the old board switcher sheet's were the only reader on phones).
    val liveRunsByTeam: StateFlow<Map<String, TeamLiveRuns>> = combine(myLiveSessions, auth.userId) { sessions, me ->
        liveRunsByTeam(sessions, me)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyMap())

    // The Board tab root's current board (`currentBoardFor`): the ACTIVE
    // team's last-used board, else its first board, else none (chats.json:
    // one active team drives every phone screen, so a team switch moves the
    // board too); before a team resolves, the last-used board. The
    // lastBoardVersion counter re-runs the resolve after every last-used
    // write — that's what swaps the root list in place after a switcher pick.
    @OptIn(ExperimentalCoroutinesApi::class)
    private val currentBoard: StateFlow<BoardEntity?> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        teamSelection.lastBoardVersion,
    ) { db, accountId, _ -> db to accountId }
        .flatMapLatest { (db, accountId) ->
            if (db == null || accountId == null) flowOf(null)
            else combine(
                db.boardDao().observeAll(),
                db.teamDao().observeAll(),
                teamSelection.selectedId,
            ) { boards, teams, selectedTeamId ->
                currentBoardFor(teams, boards, teamSelection.lastBoard(accountId), selectedTeamId)
            }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    val currentBoardId: StateFlow<String?> = currentBoard
        .map { it?.id }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    fun setInstanceUrl(url: String) {
        viewModelScope.launch { auth.setInstanceUrl(url) }
    }

    fun clearInstance() {
        viewModelScope.launch {
            // Capture credentials BEFORE clearInstanceUrl drops the row.
            val active = activeAccount()
            // Awaited before the credentials drop — the unregister request
            // needs the bearer token that clearInstanceUrl removes.
            auth.activeAccountId.value?.let {
                pushTokenManager.unregisterToken(it)
                enetNodeManager.signOut(it, revoke = true)
            }
            // Revoke the server session AFTER the unregister (which needs it
            // live) and BEFORE the token drops locally (REV2-15).
            revokeSession(active)
            syncManager.signOut()
            steerConnectionStore.closeAll()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            // EXP-1275 (D8.5): signing out runs behind the blocking overlay.
            Blocking.whileBlocking(FeedbackStates.BLOCKING_SIGN_OUT) {
                val active = activeAccount()
                auth.activeAccountId.value?.let {
                    pushTokenManager.unregisterToken(it)
                    enetNodeManager.signOut(it, revoke = true)
                }
                revokeSession(active)
                syncManager.signOut()
                steerConnectionStore.closeAll()
                auth.clearToken()
            }
        }
    }

    /**
     * The offline banner's Retry: poll every shape of every account NOW.
     * Forced, so the kick debounce can never make a user-initiated retry
     * silently do nothing.
     */
    fun retrySync() {
        syncManager.kick("offline-banner", force = true)
    }

    fun switchAccount(id: String) {
        viewModelScope.launch { auth.switchAccount(id) }
    }

    /** A notification / deep link naming a team (chats.json): plan the switch
     *  of the ONE active team on [accountId] (null = the active account);
     *  null when the team is unknown. Apply it with [applyTeamSwitch]. */
    suspend fun planTeamSwitch(accountId: String?, teamId: String?): TeamSwitch? {
        val account = accountId ?: auth.activeAccountId.value ?: return null
        if (teamId == null) return null
        return teamSelection.planTeamSwitch(databaseHolder, account, teamId)
    }

    fun applyTeamSwitch(switch: TeamSwitch) {
        teamSelection.applyTeamSwitch(switch, auth)
    }

    fun removeAccount(id: String) {
        viewModelScope.launch { removeAccountAwaiting(id) }
    }

    // The body of [removeAccount], as a suspend function: callers that must
    // sequence work AFTER the removal has fully settled (signOutOfGatedServer)
    // have to await it instead of racing the coroutine removeAccount() fires
    // and forgets.
    private suspend fun removeAccountAwaiting(id: String) {
        val account = auth.accounts.value.firstOrNull { it.id == id }
        pushTokenManager.unregisterToken(id)
        enetNodeManager.signOut(id, revoke = true)
        revokeSession(account)
        steerConnectionStore.closeAll()
        auth.removeAccount(id)
        databaseHolder.deleteFiles(id)
    }

    // The blocking 426 gate's escape hatch (REV2-18): remove the offending
    // account outright. Clearing just its token would leave that server's
    // account signed out but still present, so the gate would re-latch off its
    // own login round-trips instead of falling through to another signed-in
    // server (AccountStore.remove re-activates the most recent one) or the
    // instance picker.
    fun signOutOfGatedServer() {
        val account = activeAccount() ?: return
        viewModelScope.launch {
            // AWAIT the removal before clearing the latch. The removal's own
            // push unregister is a tRPC call to the very server that is 426ing,
            // and that response re-latches the origin through
            // HttpClientProvider's validator — a clear() issued before it lands
            // is silently undone, and the stale latch then blocks re-adding the
            // server for the rest of the app run (exactly what clear() exists
            // to prevent).
            removeAccountAwaiting(account.id)
            // Keep the latch while another account still lives on that server —
            // the same 426 gates it too.
            val origin = UpdateGate.originKey(account.instanceUrl)
            val othersOnServer = auth.accounts.value.any {
                it.id != account.id && UpdateGate.originKey(it.instanceUrl) == origin
            }
            if (!othersOnServer) updateGate.clear(account.instanceUrl)
        }
    }

    private fun activeAccount(): ServerAccount? =
        auth.activeAccountId.value?.let { id -> auth.accounts.value.firstOrNull { it.id == id } }

    // Best-effort server-side session revocation — sign-out must actually end
    // the Better Auth session, not just drop the local token copy (REV2-15).
    private suspend fun revokeSession(account: ServerAccount?) {
        val token = account?.token ?: return
        authApi.signOut(account.instanceUrl, token)
    }

}

// Re-assert cadence for the gated-account pipeline stop (REV2-18). Starts fast
// enough that the reconcile-relaunch race closes within a blink of the account
// change that opened it, then backs off to a once-a-minute insurance tick for
// as long as a signed-in server stays gated.
private const val GATED_STOP_REASSERT_MIN_MS = 200L
private const val GATED_STOP_REASSERT_MAX_MS = 60_000L

// Does this account's server sit behind a 426 update gate? Matching is by
// instance origin, the same key HttpClientProvider latches responses under.
private fun ServerAccount.isGated(gated: Map<String, UpdateGate.UpgradeInfo>): Boolean {
    val key = UpdateGate.originKey(instanceUrl) ?: return false
    return gated.containsKey(key)
}
