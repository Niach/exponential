package com.exponential.app.ui.settings

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.SkillDto
import com.exponential.app.data.api.SkillsApi
import com.exponential.app.data.api.toSkillOption
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.SkillOption
import com.exponential.app.domain.sortSkills
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// #EXP-1661: Settings › Skills on a phone, READ ONLY (skills.json
// `permissions.phone`): the list = the SELECTED team's synced, body-less
// `skills` rows sorted by name; a skill's page fetches its SKILL.md body
// with `skills.get`. No create, edit or delete here (web + desktop own them).

/** One skill page's fetch state (the body is server-only). */
data class SkillPageState(
    val loading: Boolean = false,
    val skill: SkillDto? = null,
    /** The fetch failed: the body shows its empty state + Retry. */
    val loadFailed: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SkillsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val skillsApi: SkillsApi,
) : ViewModel() {

    /** The page's skill (`settings/skill/{skillId}`); null = the list. */
    val skillId: String? = savedStateHandle.get<String>("skillId")?.takeIf { it.isNotEmpty() }

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The selected team's skills by name; null = still loading. */
    val list: StateFlow<List<SkillOption>?> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) {
                flowOf(null)
            } else {
                db.skillDao().observeByTeam(team).map { rows -> sortSkills(rows.map { it.toSkillOption() }) }
            }
        }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** The page's LIVE synced row (name, description, Default). */
    val row: StateFlow<SkillOption?> = dbFlow.flatMapLatest { db ->
        val id = skillId
        if (db == null || id == null) flowOf(null) else db.skillDao().observeById(id).map { it?.toSkillOption() }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _state = MutableStateFlow(SkillPageState(loading = skillId != null))
    val state: StateFlow<SkillPageState> = _state

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** Fetch the body; [retried] = the person tapped Retry (only then a toast). */
    fun load(retried: Boolean = false) {
        val id = skillId ?: return
        _state.value = _state.value.copy(loading = true, loadFailed = false)
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = _state.value.copy(loading = false)
                return@launch
            }
            try {
                _state.value = _state.value.copy(loading = false, skill = skillsApi.get(accountId, id))
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _state.value = _state.value.copy(loading = false, loadFailed = true)
                if (retried) _failure.value = FailedAction("load", "the skill", trpcErrorMessage(t, "")) { load(retried = true) }
            }
        }
    }
}
