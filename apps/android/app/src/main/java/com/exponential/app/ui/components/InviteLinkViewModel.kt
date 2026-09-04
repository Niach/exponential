package com.exponential.app.ui.components

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.isPlanLimitError
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.WebLinks
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * What the invite-link creator renders (EXP-725).
 *
 * [hidden] is the store-compliance rule, not a styling choice: at the seat cap
 * the control is REMOVED — no hint, no "upgrade on the web" pointer, not even
 * the neutral plan-limit sentence (Apple/Google both read any of those as
 * steering the user to an outside purchase). It only hides once capacity is
 * KNOWN to be zero: an unresolved capacity keeps the control, and the server
 * is the real gate.
 */
data class InviteLinkState(
    val capacityKnown: Boolean = false,
    /** Free seats left; null = unlimited (self-hosted, paid or comp tier). */
    val remaining: Int? = null,
    val inviteUrl: String? = null,
    val generating: Boolean = false,
    /** NON-plan failures only — a cap never prints anything. */
    val error: String? = null,
) {
    val hidden: Boolean get() = capacityKnown && remaining == 0
}

/**
 * The invite-link creator's state, shared by the first-run wizard's invite step
 * and team settings → Members. Both host it keyed on the team
 * (`hiltViewModel(key = "invite-link:$teamId")`) and call [bind].
 *
 * Capacity is re-fetched whenever the team's member or pending-invite COUNT
 * changes — both are synced shapes, so a teammate accepting elsewhere hides
 * the control here without a poll. The counts are debounced: an Electric
 * snapshot lands its rows in a burst, which would otherwise fire one query per
 * row. A failed fetch leaves capacity "known, unlimited" rather than hiding
 * anything — the server still refuses over the cap, and the refusal itself
 * (a PRECONDITION_FAILED plan limit) hides the control for good.
 */
@OptIn(FlowPreview::class)
@HiltViewModel
class InviteLinkViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val teamsApi: TeamsApi,
    private val invitesApi: TeamInvitesApi,
    holder: DatabaseHolder,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val _state = MutableStateFlow(InviteLinkState())
    val state: StateFlow<InviteLinkState> = _state.asStateFlow()

    private var boundTeamId: String? = null

    /** Idempotent per team — a recomposition must not restart the watcher. */
    fun bind(teamId: String) {
        if (boundTeamId == teamId) return
        boundTeamId = teamId
        _state.value = InviteLinkState()
        viewModelScope.launch { refreshCapacity(teamId) }
        viewModelScope.launch {
            combine(
                dbFlow.scopedQuery(0) { db -> db.teamMemberDao().observeByTeam(teamId).map { it.size } },
                dbFlow.scopedQuery(0) { db -> db.teamInviteDao().observeByTeam(teamId).map { it.size } },
            ) { members, invites -> members to invites }
                .distinctUntilChanged()
                // The initial emission is what the bind fetch above already
                // covers; only CHANGES need another round trip.
                .drop(1)
                .debounce(300)
                .collect { refreshCapacity(teamId) }
        }
    }

    private suspend fun refreshCapacity(teamId: String) {
        val accountId = auth.activeAccountId.value ?: return
        runCatching { teamsApi.inviteCapacity(accountId, teamId) }
            .onSuccess { remaining ->
                _state.value = _state.value.copy(capacityKnown = true, remaining = remaining)
            }
            .onFailure {
                _state.value = _state.value.copy(capacityKnown = true, remaining = null)
            }
    }

    /**
     * Mint a link. A plan-limit refusal is the cap arriving late (someone took
     * the last seat between the capacity read and this call): the creator
     * hides itself and says nothing at all.
     */
    fun generate() {
        val teamId = boundTeamId ?: return
        if (_state.value.generating) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(generating = true, error = null)
            runCatching { invitesApi.create(accountId, teamId) }
                .onSuccess { token ->
                    _state.value = _state.value.copy(
                        generating = false,
                        inviteUrl = WebLinks.invite(auth.instanceUrl.value, token),
                    )
                }
                .onFailure { err ->
                    _state.value = if (isPlanLimitError(err)) {
                        _state.value.copy(
                            generating = false,
                            capacityKnown = true,
                            remaining = 0,
                            inviteUrl = null,
                            error = null,
                        )
                    } else {
                        _state.value.copy(
                            generating = false,
                            error = trpcErrorMessage(err, "Couldn't create the invite"),
                        )
                    }
                }
        }
    }
}
