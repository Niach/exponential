package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.data.api.toActionDto
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.serialization.json.Json

// The unified Start-coding sheet's Actions-tab data (EXP-257): the selected
// team's actions LIVE from the synced actions shape (EXP-268 — the local Room
// flow, body-less by design; the virtual builtin "Create action" row is
// prepended client-side) plus the lookup
// sources the typed input fields render from — the team repo registry for
// `repo` inputs and the synced boards from the local DB for `board` inputs.
// Owned by a dedicated ViewModel so every host screen (Agents / issue list /
// issue detail / Actions) gets the Actions tab without fetching any of it
// itself; running stays with the HOST's ViewModel via the sheet's callback.

/** Actions-list progress: null [actions] with null [error] = still loading. */
data class SheetActionsState(
    val actions: List<ActionDto>? = null,
    val error: String? = null,
)

/** One pickable board for a `board`-typed action input. */
data class StartBoardOption(
    val id: String,
    val name: String,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class StartCodingSheetViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val repositoriesApi: RepositoriesApi,
    selection: TeamSelection,
    private val json: Json,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val scope = combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
        accountId to teamId
    }

    /** The selected team's actions (the virtual builtin row is prepended). */
    val actionsState: StateFlow<SheetActionsState> = combine(dbFlow, selection.selectedId) { db, teamId ->
        db to teamId
    }.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(SheetActionsState(actions = emptyList()))
        } else {
            db.actionDao().observeByTeam(teamId).map { rows ->
                SheetActionsState(
                    actions = listOf(builtinCreateAction(teamId)) +
                        rows.map { it.toActionDto(json) },
                )
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SheetActionsState())

    /** The team repo registry — options for `repo`-typed inputs (failure = empty). */
    val repos: StateFlow<List<TeamRepo>> = scope.flatMapLatest { (accountId, teamId) ->
        flow {
            emit(emptyList<TeamRepo>())
            if (accountId != null && teamId != null) {
                emit(
                    runCatching { repositoriesApi.list(accountId, teamId) }.fold(
                        onSuccess = { it },
                        onFailure = {
                            if (it is CancellationException) throw it
                            emptyList()
                        },
                    ),
                )
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** Live, team-scoped boards — options for `board`-typed inputs. */
    val boardOptions: StateFlow<List<StartBoardOption>> = combine(
        dbFlow.scopedQuery(emptyList()) { it.boardDao().observeAll() },
        selection.selectedId,
    ) { boards, teamId ->
        if (teamId == null) {
            emptyList()
        } else {
            boards
                .filter { it.teamId == teamId && it.archivedAt == null && it.deletedAt == null }
                .sortedBy { it.name.lowercase() }
                .map { StartBoardOption(id = it.id, name = it.name) }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())
}
