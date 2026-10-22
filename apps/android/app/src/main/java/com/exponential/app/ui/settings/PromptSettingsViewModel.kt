package com.exponential.app.ui.settings

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.PromptFields
import com.exponential.app.data.api.PromptsApi
import com.exponential.app.data.api.toPromptDto
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// NEW-2/NEW-7: Settings → Prompts (prompts replaced actions). The list = the
// SELECTED team's synced, body-less `prompts` rows; a prompt's page fetches
// its ≤64KB body with `prompts.get` and writes with `prompts.update` /
// `prompts.delete`; `New prompt` creates with `prompts.create`. Writes are
// owner-only (the server enforces it; members read the same form disabled
// and see no Save / Delete / New prompt). Web `components/team/prompts-section.tsx`.

/** One prompt page's fetch + write state (the body is server-only). */
data class PromptPageState(
    /** The `prompts.get` round-trip is in flight (the body field parks). */
    val loading: Boolean = false,
    /** The fetched row, body included; null until it lands (or a new prompt). */
    val prompt: PromptDto? = null,
    val saving: Boolean = false,
    /** The fetch failed: the body shows its empty state + Retry (no toast on open). */
    val loadFailed: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class PromptSettingsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val prompts: PromptsApi,
) : ViewModel() {

    /** The page's prompt (`settings/prompt/{promptId}`); null = the list or `New prompt`. */
    val promptId: String? = savedStateHandle.get<String>("promptId")?.takeIf { it.isNotEmpty() }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The selected team's prompts, server order (sort_order, then name). */
    val list: StateFlow<List<PromptDto>?> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(null) else db.promptDao().observeByTeam(team).map { rows -> rows.map { it.toPromptDto() } }
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The page's LIVE synced row (null while loading, or once deleted). */
    val row: StateFlow<PromptDto?> = dbFlow.flatMapLatest { db ->
        val id = promptId
        if (db == null || id == null) flowOf(null) else db.promptDao().observeById(id).map { it?.toPromptDto() }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The team the page writes in: the row's own, else the selection. */
    private val teamId = combine(row, selection.selectedId) { r, selected -> r?.teamId ?: selected }
        .distinctUntilChanged()

    /** Whether the caller OWNS that team (every write is owner-only). */
    val isOwner: StateFlow<Boolean> = combine(dbFlow, teamId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(team)
        }
        .combine(auth.userId) { members, userId ->
            userId != null && members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    private val _state = MutableStateFlow(PromptPageState(loading = promptId != null))
    val state: StateFlow<PromptPageState> = _state

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** FEED-79: a refused write (or a failed Retry tap), raised once as a toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** Fetch the page's body; [retried] = the person tapped Retry (only then
     *  does a failure toast). */
    fun load(retried: Boolean = false) {
        val id = promptId ?: return
        _state.value = _state.value.copy(loading = true, loadFailed = false)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = _state.value.copy(loading = false)
                return@launch
            }
            try {
                _state.value = _state.value.copy(loading = false, prompt = prompts.get(accountId, id))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = _state.value.copy(loading = false, loadFailed = true)
                if (retried) _failure.value = FailedAction("load", "the prompt", trpcErrorMessage(t, "")) { load(retried = true) }
            }
        }
    }

    /** `New prompt` → `prompts.create` in the selected team; [onCreated] gets its id. */
    fun create(fields: PromptFields, onCreated: (String) -> Unit) {
        val team = selection.selectedId.value ?: return
        write("create", { create(fields, onCreated) }) { accountId ->
            onCreated(prompts.create(accountId, team, fields).id)
        }
    }

    /** Save the edited prompt (`prompts.update`). */
    fun save(fields: PromptFields) {
        val id = promptId ?: return
        write("save", { save(fields) }) { accountId ->
            val saved = prompts.update(accountId, id, fields)
            _state.value = _state.value.copy(prompt = saved)
        }
    }

    /** Owner-only, permanent; [onDeleted] leaves the page. */
    fun delete(onDeleted: () -> Unit) {
        val id = promptId ?: return
        write("delete", null) { accountId ->
            prompts.delete(accountId, id)
            onDeleted()
        }
    }

    private fun write(verb: String, retry: (() -> Unit)?, call: suspend (accountId: String) -> Unit) {
        if (_state.value.saving) return
        _state.value = _state.value.copy(saving = true)
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                call(accountId)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction(verb, "the prompt", trpcErrorMessage(t, ""), retry)
            } finally {
                _state.value = _state.value.copy(saving = false)
            }
        }
    }
}
