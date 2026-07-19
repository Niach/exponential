package com.exponential.app.ui.support

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.HelpdeskApi
import com.exponential.app.data.api.SupportThreadRow
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
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
 * Support tab mounting) and cancels it when the screen goes away. (The
 * helpdesk_enabled gate for the Support TAB itself lives in AppViewModel —
 * the bottom bar needs it whether or not this ViewModel exists yet.)
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SupportInboxViewModel @Inject constructor(
    private val auth: AuthRepository,
    selection: TeamSelection,
    private val helpdeskApi: HelpdeskApi,
) : ViewModel() {

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
