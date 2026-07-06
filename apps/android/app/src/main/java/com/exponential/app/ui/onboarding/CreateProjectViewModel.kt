package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.ProjectRepositoryChoice
import com.exponential.app.data.api.ProjectsApi
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
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
        val submitting: Boolean = false,
        val error: String? = null,
        val limitError: String? = null,
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
            runCatching {
                val workspace = workspacesApi.ensureDefault(accountId)
                holder.database(forAccountId = accountId).workspaceDao().upsert(workspace)
                if (selection.selectedId.value == null) selection.select(workspace.id)
                workspace.id
            }.onSuccess { _workspaceId.value = it }
                .onFailure { _state.value = _state.value.copy(error = it.message ?: "Failed to prepare workspace") }
        }
    }

    /** Remember the freshly-created project as last-used so the Issues tab opens on it. */
    fun rememberCreated(projectId: String) {
        val accountId = auth.activeAccountId.value ?: return
        selection.rememberLastProject(accountId, projectId)
    }

    fun loadRepos(workspaceId: String) {
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(loadingRepos = true)
            val repos = runCatching { repositoriesApi.list(accountId, workspaceId) }.getOrNull()
            _state.value = _state.value.copy(
                repos = repos ?: emptyList(),
                loadingRepos = false,
            )
        }
    }

    fun create(
        workspaceId: String,
        name: String,
        prefix: String,
        color: String,
        repository: ProjectRepositoryChoice,
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
                    repository = repository,
                )
            }.onSuccess { projectId ->
                _state.value = _state.value.copy(submitting = false)
                onCreated(projectId)
            }.onFailure { err ->
                val message = err.message ?: "Failed to create project"
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
