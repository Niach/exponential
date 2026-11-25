package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.data.api.BoardsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.TeamsApi
import com.exponential.app.data.api.isPlanLimitError
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
        val submitting: Boolean = false,
        // FEED-79: a refused create naming a field (a taken name or prefix,
        // an invalid prefix) = invalid input, under the form's fields.
        val fieldError: String? = null,
        // FEED-79: a failed repos load or create, raised once as a toast (a
        // failed load never reads as "no repos connected", EXP-46).
        val failure: FailedAction? = null,
        // The team resolve failed: the sheet shows its empty state + a toast.
        val setupFailure: FailedAction? = null,
        val setupFailed: Boolean = false,
        val limitError: String? = null,
    )

    fun clearFailure() {
        _state.value = _state.value.copy(failure = null)
    }

    fun clearSetupFailure() {
        _state.value = _state.value.copy(setupFailure = null)
    }

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    val accountId: StateFlow<String?> = auth.activeAccountId

    // The team the new board lands in. When a caller (team settings)
    // already knows it, it's used directly; the account-level empty states pass
    // null and resolve the default team here.
    private val _teamId = MutableStateFlow<String?>(null)
    val teamId: StateFlow<String?> = _teamId.asStateFlow()

    /** [retried] = the person tapped Retry: only then does a failure toast
     *  (FEED-79: the resolve on open fails quietly into the empty state). */
    fun ensureTeam(explicit: String?, retried: Boolean = false) {
        if (explicit != null) {
            _teamId.value = explicit
            return
        }
        if (_teamId.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            // A retry starts clean.
            _state.value = _state.value.copy(setupFailed = false)
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
                .onFailure {
                    _state.value = _state.value.copy(
                        setupFailed = true,
                        setupFailure = if (!retried) null else FailedAction(
                            "load",
                            "your team",
                            trpcErrorMessage(it, it.message ?: ""),
                        ) { ensureTeam(explicit, retried = true) },
                    )
                }
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
            _state.value = _state.value.copy(loadingRepos = true)
            runCatching { repositoriesApi.list(accountId, teamId) }
                .onSuccess { repos ->
                    _state.value = _state.value.copy(repos = repos, loadingRepos = false)
                }
                .onFailure {
                    // FEED-79: the list loads when the form opens, so a failed
                    // load is quiet (no repository to pick), never a toast.
                    _state.value = _state.value.copy(repos = emptyList(), loadingRepos = false)
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
        defaultBranch: String?,
        onCreated: (boardId: String) -> Unit,
    ) {
        if (_state.value.submitting) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(submitting = true, fieldError = null, limitError = null)
            runCatching {
                boardsApi.create(
                    accountId = accountId,
                    teamId = teamId,
                    name = name.trim(),
                    prefix = prefix.trim(),
                    color = color,
                    icon = icon,
                    repository = repository,
                    // EXP-712: only meaningful with a repository — the server
                    // drops it otherwise.
                    defaultBranch = defaultBranch?.takeIf { repository != null },
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
                val message = trpcErrorMessage(err, "")
                // EXP-725: the server's own classification (PRECONDITION_FAILED
                // + the shared plan-limit prefix, captured at the throw site),
                // not a substring sniff for "limit"/"plan"/"upgrade" on copy
                // the app already neutralised.
                val isLimit = isPlanLimitError(err)
                // The zod prefix refusal arrives as an issue list: read the raw text too.
                val field = if (isLimit) null else boardCreateFieldError(message) ?: err.message?.let(::boardCreateFieldError)
                _state.value = _state.value.copy(
                    submitting = false,
                    fieldError = field,
                    // A create can duplicate, so no Retry; the form stays.
                    failure = if (isLimit || field != null) null else FailedAction("create", "the board", message),
                    limitError = if (isLimit) message else null,
                )
            }
        }
    }
}

/** The prefix rule boards.create's zod regex enforces (web `PREFIX_RULE`). */
private const val PREFIX_RULE = "Prefix must be 1-4 letters or digits, starting with a letter"

/**
 * FEED-79 (web `boardCreateFieldError`): the field a refused create is about
 * (a taken name or prefix, an invalid prefix = invalid INPUT, the field's own
 * line), else null (a failed action, a toast).
 */
internal fun boardCreateFieldError(message: String): String? = when {
    message.contains("this name") || message.contains("this prefix") -> message
    message.contains(PREFIX_RULE) -> PREFIX_RULE
    else -> null
}
