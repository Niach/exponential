package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.ProjectRepositoryChoice
import com.exponential.app.data.api.ProjectsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// Shared engine for the create-project surfaces (onboarding page 2 + the
// empty-state sheets): loads the workspace's registry repos and performs
// `projects.create`. Plan caps no longer apply to project creation (v5 per-seat),
// but a server-side limit message is surfaced as a softer nudge, mirroring web.
@HiltViewModel
class CreateProjectViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val projectsApi: ProjectsApi,
    private val repositoriesApi: RepositoriesApi,
    private val workspacesApi: WorkspacesApi,
    private val holder: DatabaseHolder,
    private val selection: WorkspaceSelection,
) : ViewModel() {

    data class UiState(
        val repos: List<WorkspaceRepo> = emptyList(),
        val loadingRepos: Boolean = true,
        // Registry load failure — distinct from `error` (create/ensure failures)
        // so a failed load renders as a retriable error row instead of silently
        // looking like "no repos connected" (EXP-46).
        val reposError: String? = null,
        val submitting: Boolean = false,
        val error: String? = null,
        val limitError: String? = null,
        // Whether the viewer owns the target workspace — the public-board
        // option is owner-only on the server (projects.create), so the form
        // disables it with a hint for non-owners. Biased false while the
        // membership shape loads (disabled-while-loading).
        val isOwner: Boolean = false,
    )

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    val accountId: StateFlow<String?> = auth.activeAccountId

    // The workspace the new project lands in. When a caller (workspace settings)
    // already knows it, it's used directly; the account-level empty states pass
    // null and resolve the default workspace here.
    private val _workspaceId = MutableStateFlow<String?>(null)
    val workspaceId: StateFlow<String?> = _workspaceId.asStateFlow()

    fun ensureWorkspace(explicit: String?) {
        if (explicit != null) {
            _workspaceId.value = explicit
            return
        }
        if (_workspaceId.value != null) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            // A retry (the sheet's error state re-calls this) starts clean.
            _state.value = _state.value.copy(error = null)
            runCatching {
                val workspace = workspacesApi.ensureDefault(accountId)
                holder.database(forAccountId = accountId).workspaceDao().upsert(workspace)
                if (selection.selectedId.value == null) selection.select(workspace.id)
                workspace.id
            }.onSuccess { _workspaceId.value = it }
                .onFailure { _state.value = _state.value.copy(error = it.message ?: "Failed to prepare team") }
        }
    }

    /** Remember the freshly-created project as last-used so the Issues tab opens on it. */
    fun rememberCreated(projectId: String) {
        val accountId = auth.activeAccountId.value ?: return
        selection.rememberLastProject(accountId, projectId)
    }

    private var ownerJob: Job? = null

    /** Track whether the viewer owns [workspaceId] (same derivation as
     * WorkspaceSettingsViewModel.isOwner) — drives the owner-only public
     * option. Job-managed so a workspace switch drops the old collection. */
    fun observeIsOwner(workspaceId: String) {
        ownerJob?.cancel()
        val accountId = auth.activeAccountId.value ?: return
        ownerJob = viewModelScope.launch {
            holder.database(forAccountId = accountId).workspaceMemberDao()
                .observeByWorkspace(workspaceId)
                .collect { members ->
                    val userId = auth.userId.value
                    _state.value = _state.value.copy(
                        isOwner = userId != null && members.any {
                            it.userId == userId && it.role == DomainContract.workspaceRoleOwner
                        },
                    )
                }
        }
    }

    fun loadRepos(workspaceId: String) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(loadingRepos = true, reposError = null)
            runCatching { repositoriesApi.list(accountId, workspaceId) }
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
        workspaceId: String,
        name: String,
        prefix: String,
        color: String,
        isPublic: Boolean,
        icon: String,
        repository: ProjectRepositoryChoice?,
        onCreated: (projectId: String) -> Unit,
    ) {
        if (_state.value.submitting) return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(submitting = true, error = null, limitError = null)
            runCatching {
                projectsApi.create(
                    accountId = accountId,
                    workspaceId = workspaceId,
                    name = name.trim(),
                    prefix = prefix.trim(),
                    color = color,
                    // Clamped — the server rejects a non-owner's isPublic.
                    isPublic = isPublic && _state.value.isOwner,
                    icon = icon,
                    repository = repository,
                )
            }.onSuccess { created ->
                // Mirror the new project into Room immediately instead of waiting
                // for the Electric projects shape's next long-poll — without this
                // a SUCCESSFUL create still shows the "Create your first project"
                // empty state (EXP-46), inviting duplicate-create retries. Exact
                // pattern of the issues upsertCreatedLocally head-start (EXP-19):
                // Electric re-delivers the same row idempotently (REPLACE), and a
                // local DB hiccup must not fail the already-committed create.
                created.entity?.let { entity ->
                    runCatching {
                        holder.database(forAccountId = accountId).projectDao().upsert(entity)
                    }
                }
                _state.value = _state.value.copy(submitting = false)
                onCreated(created.id)
            }.onFailure { err ->
                // Extract the server's human-readable message instead of the raw
                // "HTTP 403: {json}" blob — e.g. the no-grant FORBIDDEN's
                // "…reconnect GitHub in workspace settings → Repositories…" is
                // itself the actionable instruction.
                val message = trpcErrorMessage(err, err.message ?: "Failed to create project")
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
