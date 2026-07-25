package com.exponential.app.ui.issue

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.trpcErrorMessage
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
import kotlinx.coroutines.flow.stateIn

// The unified Start-coding sheet's Actions-tab data (EXP-257): the selected
// team's actions (tRPC — deliberately NOT an Electric shape) plus the lookup
// sources the typed input fields render from — the team repo registry for
// `repo` inputs and the synced boards from the local DB for `board` inputs.
// Owned by a dedicated ViewModel so every host screen (Agents / issue list /
// issue detail / Actions) gets the Actions tab without fetching any of it
// itself; running stays with the HOST's ViewModel via the sheet's callback.

/** `actions.list` progress: null [actions] with null [error] = still loading. */
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
    private val actionsApi: ActionsApi,
    private val repositoriesApi: RepositoriesApi,
    selection: TeamSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val scope = combine(auth.activeAccountId, selection.selectedId) { accountId, teamId ->
        accountId to teamId
    }

    /** The selected team's actions (the server appends the virtual builtin row). */
    val actionsState: StateFlow<SheetActionsState> = scope.flatMapLatest { (accountId, teamId) ->
        flow {
            if (accountId == null || teamId == null) {
                emit(SheetActionsState(actions = emptyList()))
                return@flow
            }
            emit(SheetActionsState())
            emit(
                runCatching { actionsApi.list(accountId, teamId) }.fold(
                    onSuccess = { SheetActionsState(actions = it) },
                    onFailure = {
                        if (it is CancellationException) throw it
                        SheetActionsState(error = trpcErrorMessage(it, "Couldn't load actions"))
                    },
                ),
            )
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
