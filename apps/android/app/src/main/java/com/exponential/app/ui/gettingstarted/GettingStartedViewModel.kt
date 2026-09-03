package com.exponential.app.ui.gettingstarted

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.IntegrationsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * The getting-started checklist's signals for the SELECTED team (EXP-698 r5,
 * Mechanism B). Everything but the GitHub install is already on the device —
 * the mobile clients sync devices, boards, coding sessions, actions, members
 * and invites — so the whole checklist derives off Room and stays live while
 * the user works through it. The GitHub status is a one-shot tRPC query, since
 * installations are server-only (never a synced shape); it is re-read whenever
 * the account or the team changes.
 *
 * The state rules themselves live in `GettingStartedModel.kt`, ported from web
 * and unit tested there.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class GettingStartedViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val integrationsApi: IntegrationsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /**
     * The GitHub install status TAGGED with the team it was read for; null
     * until the first read lands. Tagged rather than reset from the fetch
     * coroutine: a team switch has to invalidate the previous team's answer in
     * the SAME emission that changes the team, or the checklist renders one
     * team's status against another team's rows for a frame or two.
     */
    private val _githubStatus = MutableStateFlow<Pair<String, Boolean>?>(null)

    /** Own machines by kind — the devices shape already carries only rows the
     *  caller may see, but `shared_team_id` puts teammates' servers in it too. */
    private val ownDevices = combine(
        dbFlow.scopedQuery(emptyList()) { it.deviceDao().observeAll() },
        auth.userId,
    ) { devices, userId ->
        if (userId == null) emptyList() else devices.filter { it.userId == userId }
    }

    private val boards = teamScoped(emptyList()) { db, teamId ->
        db.boardDao().observeByTeam(teamId)
    }
    private val codingSessions = teamScoped(emptyList()) { db, teamId ->
        db.codingSessionDao().observeByTeam(teamId)
    }
    private val actions = teamScoped(emptyList()) { db, teamId ->
        db.actionDao().observeByTeam(teamId)
    }
    private val members = teamScoped(emptyList()) { db, teamId ->
        db.teamMemberDao().observeByTeam(teamId)
    }
    private val invites = teamScoped(emptyList()) { db, teamId ->
        db.teamInviteDao().observeByTeam(teamId)
    }

    /**
     * The two owner-gated entries (invite, action) need the caller's role in
     * the SELECTED team — the same resolution `TeamPermissions` performs, off
     * the same member rows.
     */
    private val isOwner = combine(members, auth.userId) { rows, userId ->
        userId != null && rows.firstOrNull { it.userId == userId }?.role == "owner"
    }

    private val signals = combine(
        ownDevices,
        boards,
        codingSessions,
        actions,
        combine(members, invites) { memberRows, inviteRows ->
            memberRows.size > 1 || inviteRows.isNotEmpty()
        },
    ) { devices, boardRows, sessions, actionRows, invitedTeam ->
        GettingStartedSignals(
            hasDesktopDevice = devices.any { it.kind == "desktop" },
            hasServerDevice = devices.any { it.kind == "server" },
            hasInvitedTeam = invitedTeam,
            hasBoard = boardRows.isNotEmpty(),
            hasRepoBoard = boardRows.any { it.repositoryId != null },
            hasCodingSession = sessions.isNotEmpty(),
            hasAction = actionRows.isNotEmpty(),
        )
    }

    val state: StateFlow<GettingStartedState> = combine(
        signals,
        _githubStatus,
        isOwner,
        selection.selectedId,
    ) { base, status, owner, teamId ->
        // The status only counts for the team it was read for.
        val github = status?.takeIf { it.first == teamId }?.second
        // Until every signal has resolved the checklist renders NOTHING (web
        // renders its entries neutral for the same reason): deriving against a
        // not-yet-known GitHub status would show "Connect GitHub" with a live
        // pill and a wrong done/total, then flip both a moment later.
        if (teamId == null || github == null) {
            GettingStartedState(loading = true)
        } else {
            GettingStartedState(
                entries = deriveGettingStartedEntries(
                    base.copy(githubInstalled = github),
                    isOwner = owner,
                ),
                loading = false,
            )
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), GettingStartedState())

    init {
        viewModelScope.launch {
            combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
                accountId to teamId
            }.distinctUntilChanged().collectLatest { (accountId, teamId) ->
                if (accountId == null || teamId == null) return@collectLatest
                // A failed status read is not an error the checklist can act
                // on — it just means "not connected as far as we can tell".
                val installed = runCatching {
                    integrationsApi.githubStatus(accountId, teamId).installed
                }.getOrDefault(false)
                _githubStatus.value = teamId to installed
            }
        }
    }

    /** The `EXP_INSTANCE` the server install one-liner points at. */
    val instanceOrigin: StateFlow<String?> = auth.instanceUrl
        .map { it?.trimEnd('/') }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The active team, for the create-board action's target. */
    val selectedTeamId: StateFlow<String?> = selection.selectedId

    private fun <T> teamScoped(empty: T, query: (ExponentialDatabase, String) -> Flow<T>): Flow<T> =
        combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) flowOf(empty) else query(db, teamId)
            }
}
