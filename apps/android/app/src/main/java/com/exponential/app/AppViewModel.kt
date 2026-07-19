package com.exponential.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.UpdateGate
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.domain.CodingSessionLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.defaultTeamId
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
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
    // Non-null once the server has answered HTTP 426 (this build is below the
    // configured minimum, EXP-104) — drives the blocking "Update required" gate.
    val updateRequired: UpdateGate.UpgradeInfo? = null,
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
    private val databaseHolder: DatabaseHolder,
    private val teamSelection: TeamSelection,
    private val updateGate: UpdateGate,
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
                .collect { teamSelection.clearSelection() }
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
                            accountId to defaultTeamId(
                                teams,
                                boards,
                                teamSelection.lastBoard(accountId),
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
    }

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
        auth.activeAccountId,
        auth.accounts,
        updateGate.state,
    ) { url, token, activeId, accounts, updateRequired ->
        AppState(
            instanceUrl = url,
            token = token,
            activeAccountId = activeId,
            accounts = accounts,
            updateRequired = updateRequired,
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, AppState())

    // Unread notifications for the active account — drives the bottom bar's
    // inbox dot. Re-scopes reactively on account switch like the feature VMs.
    @OptIn(ExperimentalCoroutinesApi::class)
    val unreadCount: StateFlow<Int> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        auth.accounts,
    ) { db, activeId, accounts ->
        db to accounts.firstOrNull { it.id == activeId }?.userId
    }.flatMapLatest { (db, userId) ->
        if (db == null || userId == null) flowOf(0)
        else db.notificationDao().observeUnreadCount(userId)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    // Unread helpdesk activity in the selected team — drives the bottom bar's
    // Support dot (EXP-182): issue-less support_reply rows carry a synced
    // team_id, the same rule the inbox's per-team Support groups use.
    @OptIn(ExperimentalCoroutinesApi::class)
    val supportUnread: StateFlow<Boolean> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        auth.accounts,
        teamSelection.selectedId,
    ) { db, activeId, accounts, teamId ->
        Triple(db, accounts.firstOrNull { it.id == activeId }?.userId, teamId)
    }.flatMapLatest { (db, userId, teamId) ->
        if (db == null || userId == null || teamId == null) flowOf(false)
        else db.notificationDao()
            .observeUnreadSupportCount(
                userId,
                teamId,
                DomainContract.notificationTypeSupportReply,
            )
            .map { it > 0 }
    }.stateIn(viewModelScope, SharingStarted.Eagerly, false)

    // True while at least one coding session is live on the active account —
    // drives the bottom bar's Agents dot. A live session is `running` or the
    // `in_review` PR-open parking spot (EXP-194 — the dot counts in_review as
    // the "agent finished, look at it" signal). Heartbeat-stale rows count as
    // absent (EXP-153); the minute ticker clears the dot once the liveness
    // window elapses without any sync delta.
    @OptIn(ExperimentalCoroutinesApi::class)
    val agentsRunning: StateFlow<Boolean> = accountDatabaseFlow(auth, databaseHolder)
        .flatMapLatest { db ->
            if (db == null) flowOf(false)
            else combine(
                db.codingSessionDao()
                    .observeByStatuses(CodingSessionLiveness.liveStatuses),
                CodingSessionLiveness.minuteTicker(),
            ) { sessions, now -> sessions.any { CodingSessionLiveness.isLive(it, now) } }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, false)

    // The active team's synced `helpdesk_enabled` flag — gates the bottom
    // bar's Support tab (EXP-180). Room-observing only (the teams shape syncs
    // the column); the ticket poll starts when the Support screen mounts.
    @OptIn(ExperimentalCoroutinesApi::class)
    val helpdeskEnabled: StateFlow<Boolean> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        teamSelection.selectedId,
    ) { db, teamId -> db to teamId }
        .flatMapLatest { (db, teamId) ->
            if (db == null || teamId == null) flowOf(false)
            else db.teamDao().observeById(teamId).map { it?.helpdeskEnabled == true }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, false)

    // The Issues tab root's current board: last-used on the active account
    // (validated against the live Room table, so deleted/archived boards fall
    // through), else the first board of the first team, else none. The
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
            ) { boards, teams ->
                val lastUsed = teamSelection.lastBoard(accountId)
                boards.firstOrNull { it.id == lastUsed }
                    ?: teams.firstNotNullOfOrNull { ws ->
                        boards.firstOrNull { it.teamId == ws.id }
                    }
                    ?: boards.firstOrNull()
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
            // Awaited before the credentials drop — the unregister request
            // needs the bearer token that clearInstanceUrl removes.
            auth.activeAccountId.value?.let { pushTokenManager.unregisterToken(it) }
            syncManager.signOut()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            auth.activeAccountId.value?.let { pushTokenManager.unregisterToken(it) }
            syncManager.signOut()
            auth.clearToken()
        }
    }

    fun switchAccount(id: String) {
        viewModelScope.launch { auth.switchAccount(id) }
    }

    fun removeAccount(id: String) {
        viewModelScope.launch {
            pushTokenManager.unregisterToken(id)
            auth.removeAccount(id)
            databaseHolder.deleteFiles(id)
        }
    }

}
