package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.CreateProjectInput
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.OnboardingApi
import com.exponential.app.data.api.ProjectsApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// Drives the 2-step onboarding wizard (create first project -> create first
// issue), mirroring the web flow. ensureDefault resolves the workspace the
// project goes into; on finish/skip it calls onboarding.complete and flips the
// local account flag so the nav gate stops showing the wizard.
@HiltViewModel
class OnboardingViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val projectsApi: ProjectsApi,
    private val issuesApi: IssuesApi,
    private val onboardingApi: OnboardingApi,
) : ViewModel() {

    data class State(
        val step: Int = 0, // 0 = project, 1 = first issue
        val workspaceId: String? = null,
        val projectId: String? = null,
        val busy: Boolean = false,
        val error: String? = null,
    )

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    private val _done = MutableStateFlow(false)
    val done: StateFlow<Boolean> = _done.asStateFlow()

    fun ensureWorkspace() {
        if (_state.value.workspaceId != null) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { workspacesApi.ensureDefault(accountId) }
                .onSuccess { _state.value = _state.value.copy(workspaceId = it.id) }
                .onFailure { _state.value = _state.value.copy(error = it.message ?: "Couldn't load your workspace") }
        }
    }

    fun createProject(name: String, prefix: String, color: String) {
        if (_state.value.busy) return
        val workspaceId = _state.value.workspaceId ?: return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(busy = true, error = null)
            runCatching {
                projectsApi.create(
                    accountId,
                    CreateProjectInput(
                        workspaceId = workspaceId,
                        name = name.trim(),
                        prefix = prefix.trim().uppercase(),
                        color = color,
                    ),
                )
            }.onSuccess {
                _state.value = _state.value.copy(busy = false, projectId = it.id, step = 1)
            }.onFailure {
                _state.value = _state.value.copy(busy = false, error = it.message ?: "Couldn't create the project")
            }
        }
    }

    fun createIssue(title: String) {
        if (_state.value.busy) return
        val projectId = _state.value.projectId ?: return
        val accountId = auth.activeAccountId.value ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(busy = true, error = null)
            runCatching {
                issuesApi.create(accountId, CreateIssueInput(projectId = projectId, title = title.trim()))
            }.onSuccess {
                finish()
            }.onFailure {
                _state.value = _state.value.copy(busy = false, error = it.message ?: "Couldn't create the issue")
            }
        }
    }

    fun back() {
        if (_state.value.step > 0) _state.value = _state.value.copy(step = _state.value.step - 1)
    }

    /** Skip the rest of setup (still marks onboarding complete, like web). */
    fun skip() {
        if (_state.value.busy) return
        viewModelScope.launch { finish() }
    }

    private suspend fun finish() {
        val accountId = auth.activeAccountId.value
        if (accountId != null) runCatching { onboardingApi.complete(accountId) }
        auth.markOnboardingCompleted(java.time.Instant.now().toString())
        _done.value = true
    }
}
