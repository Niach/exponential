package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.data.api.BoardsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// Shared engine for the create-board surfaces (onboarding page 2 + the
// empty-state sheets): loads the team's registry repos and performs
// `boards.create`. Plan caps no longer apply to board creation (v5 per-seat),
// but a server-side limit message is surfaced as a softer nudge, mirroring web.
@HiltViewModel
class CreateBoardViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val boardsApi: BoardsApi,
    private val repositoriesApi: RepositoriesApi,
    private val teamsApi: TeamsApi,
    private val holder: DatabaseHolder,
    private val selection: TeamSelection,
) : ViewModel() {

    data class UiState(
        val repos: List<TeamRepo> = emptyList(),
        val loadingRepos: Boolean = true,
        // Registry load failure — distinct from `error` (create/ensure failures)
        // so a failed load renders as a retriable error row instead of silently
        // looking like "no repos connected" (EXP-46).
        val reposError: String? = null,
        val submitting: Boolean = false,
        val error: String? = null,
        val limitError: String? = null,
    )

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    val accountId: StateFlow<String?> = auth.activeAccountId

    // The team the new board lands in. When a caller (team settings)
    // already knows it, it's used directly; the account-level empty states pass
    // null and resolve the default team here.
    private val _teamId = MutableStateFlow<String?>(null)
    val teamId: StateFlow<String?> = _teamId.asStateFlow()

    fun ensureTeam(explicit: String?) {
        if (explicit != null) {
            _teamId.value = explicit
            return
        }
        if (_teamId.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            // A retry (the sheet's error state re-calls this) starts clean.
            _state.value = _state.value.copy(error = null)
            runCatching {
                // Resolve-only (EXP-188): getDefault never creates. A null team
                // is an actionable error here — boards need a team to land in,
                // and the create-or-join flows live on the root empty state /
                // onboarding wizard, not in this sheet.
                val team = teamsApi.getDefault(accountId)
                    ?: error("Create a team first.")
                holder.database(forAccountId = accountId).teamDao().upsert(team)
                if (selection.selectedId.value == null) selection.select(team.id)
                team.id
            }.onSuccess { _teamId.value = it }
                .onFailure { _state.value = _state.value.copy(error = it.message ?: "Failed to prepare team") }
        }
    }

    /** Remember the freshly-created board as last-used so the Issues tab opens on it. */
    fun rememberCreated(boardId: String) {
        val accountId = auth.activeAccountId.value ?: return
        selection.rememberLastBoard(accountId, boardId)
    }

    fun loadRepos(teamId: String) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(loadingRepos = true, reposError = null)
            runCatching { repositoriesApi.list(accountId, teamId) }
                .onSuccess { repos ->
                    _state.value = _state.value.copy(repos = repos, loadingRepos = false)
                }
                .onFailure { err ->
                    // Don't let a failed registry load masquerade as "no repos
                    // connected" — surface it so the form can offer a retry.
                    _state.value = _state.value.copy(
                        repos = emptyList(),
                        loadingRepos = false,
                        reposError = trpcErrorMessage(err, "Couldn't load repositories"),
                    )
                }
        }
    }

    fun create(
        teamId: String,
        name: String,
        prefix: String,
        color: String,
        icon: String,
        repository: BoardRepositoryChoice?,
        onCreated: (boardId: String) -> Unit,
    ) {
        if (_state.value.submitting) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(submitting = true, error = null, limitError = null)
            runCatching {
                boardsApi.create(
                    accountId = accountId,
                    teamId = teamId,
                    name = name.trim(),
                    prefix = prefix.trim(),
                    color = color,
                    icon = icon,
                    repository = repository,
                )
            }.onSuccess { created ->
                // Mirror the new board into Room immediately instead of waiting
                // for the Electric boards shape's next long-poll — without this
                // a SUCCESSFUL create still shows the "Create your first board"
                // empty state (EXP-46), inviting duplicate-create retries. Exact
                // pattern of the issues upsertCreatedLocally head-start (EXP-19):
                // Electric re-delivers the same row idempotently (REPLACE), and a
                // local DB hiccup must not fail the already-committed create.
                created.entity?.let { entity ->
                    runCatching {
                        holder.database(forAccountId = accountId).boardDao().upsert(entity)
                    }
                }
                _state.value = _state.value.copy(submitting = false)
                onCreated(created.id)
            }.onFailure { err ->
                // Extract the server's human-readable message instead of the raw
                // "HTTP 403: {json}" blob — e.g. the no-grant FORBIDDEN's
                // "…reconnect GitHub in team settings → Repositories…" is
                // itself the actionable instruction.
                val message = trpcErrorMessage(err, err.message ?: "Failed to create board")
                val looksLikeLimit = message.contains("limit", ignoreCase = true) ||
                    message.contains("plan", ignoreCase = true) ||
                    message.contains("upgrade", ignoreCase = true)
                _state.value = _state.value.copy(
                    submitting = false,
                    error = if (looksLikeLimit) null else message,
                    limitError = if (looksLikeLimit) message else null,
                )
            }
        }
    }
}
