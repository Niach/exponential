package com.exponential.app.ui.support

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.HelpdeskApi
import com.exponential.app.data.api.SupportThreadRow
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive

enum class SupportFilter(val wire: String, val label: String) {
    Open("open", "Open"),
    Resolved("resolved", "Resolved"),
}

data class SupportInboxState(
    val threads: List<SupportThreadRow> = emptyList(),
    val filter: SupportFilter = SupportFilter.Open,
    val loading: Boolean = true,
    val error: String? = null,
)

/**
 * The Support inbox (EXP-180): support tickets of the active team. The
 * support tables are server-only (never an Electric shape), so the list is
 * polled over tRPC every 30s — the poll loop lives inside the state flow's
 * upstream, so WhileSubscribed starts it with the first collector (the
 * Support segment mounting) and cancels it when the segment goes away.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SupportInboxViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val helpdeskApi: HelpdeskApi,
) : ViewModel() {

    // Reactive account scoping, like every feature ViewModel.
    private val dbFlow = accountDatabaseFlow(auth, holder)

    /**
     * The active team's synced `helpdesk_enabled` flag — gates the "Support"
     * segment pill. A Room-observing flow (the teams shape syncs the column),
     * so collecting it never triggers HTTP polling.
     */
    val helpdeskEnabled: StateFlow<Boolean> =
        combine(dbFlow, selection.selectedId) { db, id -> db to id }
            .flatMapLatest { (db, id) ->
                if (db == null || id == null) flowOf(false)
                else db.teamDao().observeById(id).map { it?.helpdeskEnabled == true }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _filter = MutableStateFlow(SupportFilter.Open)

    val state: StateFlow<SupportInboxState> =
        combine(auth.activeAccountId, selection.selectedId, _filter) { accountId, teamId, filter ->
            Triple(accountId, teamId, filter)
        }.flatMapLatest { (accountId, teamId, filter) ->
            flow {
                if (accountId == null || teamId == null) {
                    emit(SupportInboxState(filter = filter, loading = false))
                    return@flow
                }
                var current = SupportInboxState(filter = filter, loading = true)
                emit(current)
                while (currentCoroutineContext().isActive) {
                    current = runCatching { helpdeskApi.listThreads(accountId, teamId, filter.wire) }
                        .fold(
                            onSuccess = { current.copy(threads = it, loading = false, error = null) },
                            onFailure = {
                                if (it is CancellationException) throw it
                                // Keep the last good list on a failed refresh;
                                // the error only fills the empty state.
                                current.copy(
                                    loading = false,
                                    error = trpcErrorMessage(it, "Couldn't load the support inbox"),
                                )
                            },
                        )
                    emit(current)
                    delay(30_000)
                }
            }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SupportInboxState())

    /** Switching Open/Resolved restarts the poll loop, reloading immediately. */
    fun setFilter(filter: SupportFilter) {
        _filter.value = filter
    }
}
