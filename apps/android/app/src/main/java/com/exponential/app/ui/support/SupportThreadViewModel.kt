package com.exponential.app.ui.support

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.HelpdeskApi
import com.exponential.app.data.api.MAX_SUPPORT_MESSAGE_CHARS
import com.exponential.app.data.api.SupportLinkedIssue
import com.exponential.app.data.api.SupportMessage
import com.exponential.app.data.api.SupportThreadDetail
import com.exponential.app.data.api.SupportThreadRow
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.BoardEntity
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
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

data class SupportThreadState(
    val thread: SupportThreadRow? = null,
    val messages: List<SupportMessage> = emptyList(),
    val linkedIssue: SupportLinkedIssue? = null,
    val loading: Boolean = true,
    val error: String? = null,
    val sending: Boolean = false,
    val transient: String? = null,
)

// One poll cycle's result — the mutable send/transient state rides beside it.
private data class DetailLoad(
    val detail: SupportThreadDetail? = null,
    val loading: Boolean = true,
    val error: String? = null,
)

/**
 * One support ticket's conversation (EXP-180). Server-only data → 15s tRPC
 * poll, subscription-tied like the inbox (WhileSubscribed cancels the loop
 * when the screen stops collecting). Mutations bump [reload], which restarts
 * the poll loop for an immediate refresh.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SupportThreadViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val helpdeskApi: HelpdeskApi,
    savedStateHandle: SavedStateHandle,
) : ViewModel() {

    val threadId: String = savedStateHandle["threadId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val reload = MutableStateFlow(0)
    private val _sending = MutableStateFlow(false)
    private val _transient = MutableStateFlow<String?>(null)

    private val detail: StateFlow<DetailLoad> =
        combine(auth.activeAccountId, reload) { accountId, _ -> accountId }
            .flatMapLatest { accountId ->
                flow {
                    if (accountId == null || threadId.isBlank()) {
                        emit(DetailLoad(loading = false))
                        return@flow
                    }
                    var current = DetailLoad(loading = true)
                    emit(current)
                    while (currentCoroutineContext().isActive) {
                        current = runCatching { helpdeskApi.getThread(accountId, threadId) }
                            .fold(
                                onSuccess = { DetailLoad(detail = it, loading = false, error = null) },
                                onFailure = {
                                    if (it is CancellationException) throw it
                                    // Keep the last good transcript on a failed
                                    // refresh; the error only fills the empty state.
                                    current.copy(
                                        loading = false,
                                        error = trpcErrorMessage(it, "Couldn't load the ticket"),
                                    )
                                },
                            )
                        emit(current)
                        delay(15_000)
                    }
                }
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), DetailLoad())

    val state: StateFlow<SupportThreadState> =
        combine(detail, _sending, _transient) { load, sending, transient ->
            SupportThreadState(
                thread = load.detail?.thread,
                messages = load.detail?.messages ?: emptyList(),
                linkedIssue = load.detail?.linkedIssue,
                loading = load.loading,
                error = load.error,
                sending = sending,
                transient = transient,
            )
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SupportThreadState())

    /**
     * Boards eligible for escalation: the ticket's own team (server-enforced),
     * falling back to the active team while the thread is still loading (a
     * push-tapped thread can belong to a non-selected team of this account).
     */
    val boards: StateFlow<List<BoardEntity>> =
        combine(
            dbFlow,
            detail.map { it.detail?.thread?.teamId }.distinctUntilChanged(),
            selection.selectedId,
        ) { db, threadTeamId, selectedId -> db to (threadTeamId ?: selectedId) }
            .flatMapLatest { (db, teamId) ->
                if (db == null || teamId == null) flowOf(emptyList())
                else db.boardDao().observeByTeam(teamId)
            }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun refresh() {
        reload.value += 1
    }

    /**
     * Send a public reply or an internal note. [onSent] fires only on success
     * so the composer clears without losing a failed draft.
     */
    fun sendMessage(body: String, internal: Boolean, onSent: () -> Unit) {
        val trimmed = body.trim().take(MAX_SUPPORT_MESSAGE_CHARS)
        if (trimmed.isEmpty() || _sending.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _sending.value = true
            runCatching {
                if (internal) helpdeskApi.note(accountId, threadId, trimmed)
                else helpdeskApi.reply(accountId, threadId, trimmed)
            }
                .onSuccess { onSent() }
                .onFailure {
                    if (it is CancellationException) throw it
                    _transient.value = trpcErrorMessage(
                        it,
                        if (internal) "Couldn't add the note" else "Couldn't send the reply",
                    )
                }
            _sending.value = false
            refresh()
        }
    }

    fun closeTicket() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { helpdeskApi.close(accountId, threadId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't close the ticket") }
        refresh()
    }

    fun reopenTicket() = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { helpdeskApi.reopen(accountId, threadId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't reopen the ticket") }
        refresh()
    }

    /** File a linked issue on [boardId] (server rejects a second escalation). */
    fun escalate(boardId: String) = viewModelScope.launch {
        val accountId = auth.activeAccountId.value ?: return@launch
        runCatching { helpdeskApi.escalate(accountId, threadId, boardId) }
            .onFailure { _transient.value = trpcErrorMessage(it, "Couldn't escalate the ticket") }
        refresh()
    }

    fun consumeTransient() {
        _transient.value = null
    }
}
