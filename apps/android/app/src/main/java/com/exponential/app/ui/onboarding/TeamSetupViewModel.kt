package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.WebLinks
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** One-shot outcome the sheet turns into its callbacks + a dismiss. */
sealed interface TeamSetupEvent {
    data class Created(val teamId: String) : TeamSetupEvent
    object Joined : TeamSetupEvent
}

/**
 * The create-or-join engine behind [TeamSetupSheet] (EXP-188): `teams.create`
 * for the first card, `teamInvites.accept` (paste-tolerant via
 * [WebLinks.extractInviteToken]) for the second. Both head-start the local
 * teams table with the returned row — Electric re-delivers the same row, so
 * the REPLACE is idempotent — and point the app-wide team selection at it, so
 * the screen behind the sheet swaps in place.
 *
 * Deliberately does NOT touch the local onboarding flag: this sheet is a
 * post-onboarding surface, and `teamInvites.accept` stamps
 * onboardingCompletedAt server-side anyway. The wizard's own team step keeps
 * its OnboardingViewModel path, which does have to flip the flag to exit.
 */
@HiltViewModel
class TeamSetupViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val teamsApi: TeamsApi,
    private val invitesApi: TeamInvitesApi,
    private val holder: DatabaseHolder,
    private val selection: TeamSelection,
) : ViewModel() {

    data class UiState(
        // ONE in-flight flag: both submits are disabled while either runs.
        val busy: Boolean = false,
        val createError: String? = null,
        val joinError: String? = null,
    )

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    private val _events = MutableSharedFlow<TeamSetupEvent>(extraBufferCapacity = 1)
    val events: SharedFlow<TeamSetupEvent> = _events.asSharedFlow()

    /** A fresh opening must not show the previous attempt's errors. */
    fun reset() {
        if (!_state.value.busy) _state.value = UiState()
    }

    /** First card: the creator becomes owner (open to every authed user). */
    fun createTeam(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || _state.value.busy) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(busy = true, createError = null)
            runCatching {
                val team = teamsApi.create(accountId, trimmed)
                runCatching { holder.database(forAccountId = accountId).teamDao().upsert(team) }
                selection.select(team.id)
                team.id
            }.onSuccess {
                _events.emit(TeamSetupEvent.Created(it))
                // The view model is scoped to the home nav entry, not to the
                // sheet, so it must not carry `busy` into the next opening.
                _state.value = UiState()
            }.onFailure {
                _state.value = _state.value.copy(
                    busy = false,
                    createError = trpcErrorMessage(it, "Couldn't create the team"),
                )
            }
        }
    }

    /** Second card: a pasted invite link, a `/invite/<token>` path, or the
     *  bare token — all three reduce to the token `teamInvites.accept` wants. */
    fun joinTeam(input: String) {
        if (_state.value.busy) return
        val token = WebLinks.extractInviteToken(input)
        if (token == null) {
            _state.value = _state.value.copy(
                joinError = "That doesn't look like an invite link or token.",
            )
            return
        }
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(busy = true, joinError = null)
            runCatching {
                val result = invitesApi.accept(accountId, token)
                runCatching {
                    holder.database(forAccountId = accountId).teamDao().upsert(result.team)
                }
                selection.select(result.team.id)
            }.onSuccess {
                _events.emit(TeamSetupEvent.Joined)
                _state.value = UiState()
            }.onFailure {
                _state.value = _state.value.copy(
                    busy = false,
                    joinError = trpcErrorMessage(it, "Couldn't join the team"),
                )
            }
        }
    }
}
