package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.auth.AuthRepository
import io.ktor.http.HttpStatusCode
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.MultiAccountBoardRepository
import com.exponential.app.data.db.ServerBoardGroup
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Powers the Issues tab root's board-switcher sheet (the old Boards home
// collapsed into a bottom sheet): the cross-server team/board tree,
// first-run team bootstrap, and the pick action that swaps the root
// list's board in place.

data class HomeState(
    // Server > Team > Board tree shown in the switcher sheet. Every
    // signed-in account contributes one block.
    val boardTree: List<ServerBoardGroup> = emptyList(),
    // True while the first team bootstrap is in flight and nothing has
    // synced yet — drives the root "Syncing…" state (parity with iOS).
    val isSyncing: Boolean = false,
    // True when the ACTIVE account has at least one board. Distinct from
    // "the tree is non-empty": the tree includes boardless teams (a
    // fresh account has exactly one), and a sibling account may hold boards
    // the active one doesn't — so the root screen must gate its spinner on
    // THIS, not on tree-non-emptiness, or a boardless active account spins
    // forever (its current board can never resolve).
    val activeAccountHasBoard: Boolean = false,
    // True once the ACTIVE account's boards shape has reached up-to-date at
    // least once (initial snapshot done, even at zero rows). Until then a
    // boardless-looking account is still syncing — the root shows "Syncing…"
    // rather than prematurely flashing "Create your first board".
    val activeAccountBoardsSynced: Boolean = false,
    // True when the ACTIVE account belongs to at least one team. Signups no
    // longer auto-create a team (EXP-188), so a settled false drives the
    // root's create-or-join empty state instead of the create-board one.
    val activeAccountHasTeam: Boolean = false,
)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val teamsApi: TeamsApi,
    private val holder: DatabaseHolder,
    private val selection: TeamSelection,
    multiAccountBoards: MultiAccountBoardRepository,
) : ViewModel() {

    private val _syncing = MutableStateFlow(false)

    // The active account's boards-shape "up-to-date seen" flag, re-scoped on
    // account switch. Null (no offset row yet) reads as not-yet-synced.
    private val activeBoardsSynced =
        accountDatabaseFlow(auth, holder)
            .scopedQuery<Boolean?>(null) { db -> db.electricOffsetDao().observeIsLive("boards") }
            .map { it == true }

    val state: StateFlow<HomeState> = combine(
        multiAccountBoards.serverGroups,
        _syncing,
        auth.activeAccountId,
        activeBoardsSynced,
    ) { boardTree, syncing, activeId, boardsSynced ->
        val activeGroup = boardTree.firstOrNull { it.accountId == activeId }
        val activeHasBoard = activeGroup?.teamBlocks?.any { it.boards.isNotEmpty() } == true
        HomeState(
            boardTree = boardTree,
            isSyncing = syncing,
            activeAccountHasBoard = activeHasBoard,
            activeAccountBoardsSynced = boardsSynced,
            // The tree drops accounts whose team list is empty, so a present
            // group ⇒ at least one team (blocks include boardless teams).
            activeAccountHasTeam = activeGroup != null,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    fun bootstrap() {
        viewModelScope.launch {
            _syncing.value = true
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                // Resolve-only (EXP-188): getDefault never creates — null just
                // means the account has no team yet, which the root renders as
                // the create-or-join empty state (not an error).
                val team = teamsApi.getDefault(accountId)
                if (team != null) {
                    // First-run head-start only: the upsert gives AppViewModel's
                    // default-team bootstrap a row to observe before Electric's
                    // first snapshot. Deliberately NO selection.select here —
                    // getDefault returns the oldest membership by server
                    // contract, and selecting it scoped Agents/Reviews to a
                    // team without repo boards or PRs (EXP-166/EXP-168).
                    holder.database(forAccountId = accountId).teamDao().upsert(team)
                }
                _error.value = null
            } catch (error: Throwable) {
                // A rejected session (expired/revoked token) can't be recovered by
                // retrying — clear it so the app routes the active account back to
                // login instead of looping on a 401'd home screen.
                if (error is TrpcException && error.status == HttpStatusCode.Unauthorized) {
                    auth.clearToken()
                }
                _error.value = error.message ?: "Failed to load team"
            } finally {
                // Always clear the spinner — the success path, the
                // null-accountId early `return@launch`, and the catch block
                // all funnel through here. Without this, an account that
                // bootstraps fine but has zero boards would be stuck on a
                // perpetual "Syncing…" state.
                _syncing.value = false
            }
        }
    }

    /// Root zero-team empty state's "Create team" (EXP-188): creator becomes
    /// owner. The upsert is the usual idempotent head-start so the empty state
    /// flips to create-board without waiting for the teams shape; selecting it
    /// points the settings/create-board flows at the new team.
    fun createTeam(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                val team = teamsApi.create(accountId, trimmed)
                runCatching { holder.database(forAccountId = accountId).teamDao().upsert(team) }
                selection.select(team.id)
            }.onFailure { _error.value = it.message ?: "Couldn't create the team" }
        }
    }

    /// Board pick from the switcher sheet. Makes the picked board's account
    /// active (a no-op for same-server picks) and records it as last-used —
    /// the Issues root's current-board resolution reacts to both, so the
    /// list swaps in place with no navigation.
    fun selectBoard(accountId: String, boardId: String) {
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
        selection.rememberLastBoard(accountId, boardId)
    }
}
