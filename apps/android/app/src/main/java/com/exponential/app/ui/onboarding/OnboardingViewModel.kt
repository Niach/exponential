package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.OnboardingApi
import com.exponential.app.data.api.TeamInvitesApi
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.WebLinks
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// First-run flow (shared iOS/Android spec, EXP-188): welcome → team
// (create-or-join — signups get NO auto-created team anymore) →
// create-first-board (name + optional repository, with inline GitHub connect)
// → done. On load it RESOLVES the user's default team (`teams.getDefault`,
// which never creates); null routes to the create-or-join choice. Creating a
// team advances to the board step; joining via a pasted invite link completes
// onboarding immediately (the server stamps onboardingCompletedAt in accept)
// and skips the board step. A successful board create marks onboarding
// complete server-side and remembers the board as last-used so the Issues tab
// opens on it; the local flag lands in finish() — the done step's single
// action — which drops into the app.
//
// The server also backfills onboardingCompletedAt on session reads for users who
// already have a board in a non-public team (lib/auth/onboarding.ts), so a
// returning account self-heals via reconcile() before this screen would show.
@HiltViewModel
class OnboardingViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val onboardingApi: OnboardingApi,
    private val teamsApi: TeamsApi,
    private val invitesApi: TeamInvitesApi,
    private val holder: DatabaseHolder,
    private val selection: TeamSelection,
) : ViewModel() {

    val instanceUrl: StateFlow<String?> = auth.instanceUrl
    val accountId: StateFlow<String?> = auth.activeAccountId

    private val _teamId = MutableStateFlow<String?>(null)
    val teamId: StateFlow<String?> = _teamId.asStateFlow()

    private val _preparing = MutableStateFlow(true)
    val preparing: StateFlow<Boolean> = _preparing.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _done = MutableStateFlow(false)
    val done: StateFlow<Boolean> = _done.asStateFlow()

    // True when getDefault resolved to NO team — the team step shows the
    // create-or-join choice instead of advancing to the board step.
    private val _needsTeamChoice = MutableStateFlow(false)
    val needsTeamChoice: StateFlow<Boolean> = _needsTeamChoice.asStateFlow()

    private val _teamSubmitting = MutableStateFlow(false)
    val teamSubmitting: StateFlow<Boolean> = _teamSubmitting.asStateFlow()

    private val _teamError = MutableStateFlow<String?>(null)
    val teamError: StateFlow<String?> = _teamError.asStateFlow()

    private var reconciled = false

    /** Re-read the session on appear so an account whose onboardingCompletedAt was
     * still null at login self-heals here instead of showing this screen again. */
    fun reconcile() {
        if (reconciled) return
        reconciled = true
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            val completedAt = runCatching { authApi.fetchSession(accountId)?.onboardingCompletedAt }
                .getOrNull()
            if (completedAt != null) {
                auth.markOnboardingCompleted(completedAt)
                _done.value = true
            }
        }
    }

    /** Resolve the default team (never creates — EXP-188): an existing team
     * skips the choice, null shows the create-or-join step. */
    fun prepare() {
        viewModelScope.launch {
            _preparing.value = true
            _error.value = null
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _preparing.value = false
                return@launch
            }
            runCatching {
                val team = teamsApi.getDefault(accountId)
                if (team != null) {
                    holder.database(forAccountId = accountId).teamDao().upsert(team)
                    if (selection.selectedId.value == null) selection.select(team.id)
                }
                team?.id
            }.onSuccess { teamId ->
                if (teamId != null) _teamId.value = teamId else _needsTeamChoice.value = true
            }.onFailure { _error.value = it.message ?: "Failed to prepare team" }
            _preparing.value = false
        }
    }

    /** Choice-step "Create team": creator becomes owner; advances to the board step. */
    fun createTeam(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || _teamSubmitting.value) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _teamSubmitting.value = true
            _teamError.value = null
            runCatching {
                val team = teamsApi.create(accountId, trimmed)
                // Local head-start (idempotent REPLACE — Electric re-delivers
                // the same row) so downstream screens see the team immediately.
                runCatching { holder.database(forAccountId = accountId).teamDao().upsert(team) }
                selection.select(team.id)
                team.id
            }.onSuccess { _teamId.value = it }
                .onFailure { _teamError.value = trpcErrorMessage(it, "Couldn't create the team") }
            _teamSubmitting.value = false
        }
    }

    /** Choice-step "Join team": paste tolerance via extractInviteToken. Accepting
     * completes onboarding (the server stamps onboardingCompletedAt in-tx), so
     * the LOCAL flag must flip too — without it AppNavHost's needsOnboarding
     * gate bounces straight back into this wizard — and _done exits, skipping
     * the board step (the joined team already has its owner's boards). */
    fun joinTeam(input: String) {
        if (_teamSubmitting.value) return
        val token = WebLinks.extractInviteToken(input)
        if (token == null) {
            _teamError.value = "Paste an invite link or code."
            return
        }
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _teamSubmitting.value = true
            _teamError.value = null
            runCatching {
                val result = invitesApi.accept(accountId, token)
                runCatching {
                    holder.database(forAccountId = accountId).teamDao().upsert(result.team)
                }
                selection.select(result.team.id)
            }.onSuccess {
                auth.markOnboardingCompleted(java.time.Instant.now().toString())
                _done.value = true
            }.onFailure { _teamError.value = trpcErrorMessage(it, "Couldn't join the team") }
            _teamSubmitting.value = false
        }
    }

    /**
     * After the board is created: persist completion server-side and remember
     * the board as last-used. The LOCAL flag is deliberately deferred to
     * [finish] (the done step's button): flipping it changes the authenticated
     * nav graph's startDestination, which resets the back stack straight to home
     * and would skip the done step. If the app dies on the done step the next
     * launch self-heals — the server flag is already set, so reconcile()'s
     * session read reports completedAt and exits the wizard.
     */
    fun onBoardCreated(boardId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId != null) {
                runCatching { onboardingApi.complete(accountId) }
                selection.rememberLastBoard(accountId, boardId)
            }
        }
    }

    /** Done-step action: set the local onboarding flag; the screen navigates home. */
    fun finish() {
        auth.markOnboardingCompleted(java.time.Instant.now().toString())
    }
}
