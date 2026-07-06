package com.exponential.app.ui.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.OnboardingApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

// First-run flow: a welcome page, then a create-first-project page (name → prefix
// → color → required repository, with inline GitHub connect). On load it resolves
// the user's default workspace (`workspaces.ensureDefault`) so the create form has
// a workspaceId. A successful create marks onboarding complete (server + local
// flag) and remembers the project as last-used so the Issues tab opens on it.
//
// The server also backfills onboardingCompletedAt on session reads for users who
// already have a project in a non-public workspace (lib/auth/onboarding.ts), so a
// returning account self-heals via reconcile() before this screen would show.
@HiltViewModel
class OnboardingViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val onboardingApi: OnboardingApi,
    private val workspacesApi: WorkspacesApi,
    private val holder: DatabaseHolder,
    private val selection: WorkspaceSelection,
) : ViewModel() {

    val instanceUrl: StateFlow<String?> = auth.instanceUrl
    val accountId: StateFlow<String?> = auth.activeAccountId

    private val _workspaceId = MutableStateFlow<String?>(null)
    val workspaceId: StateFlow<String?> = _workspaceId.asStateFlow()

    private val _preparing = MutableStateFlow(true)
    val preparing: StateFlow<Boolean> = _preparing.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _done = MutableStateFlow(false)
    val done: StateFlow<Boolean> = _done.asStateFlow()

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

    /** Resolve (or create) the default workspace so the create form has a target. */
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
                val workspace = workspacesApi.ensureDefault(accountId)
                holder.database(forAccountId = accountId).workspaceDao().upsert(workspace)
                if (selection.selectedId.value == null) selection.select(workspace.id)
                workspace.id
            }.onSuccess { _workspaceId.value = it }
                .onFailure { _error.value = it.message ?: "Failed to prepare workspace" }
            _preparing.value = false
        }
    }

    /** After the project is created: mark onboarding complete and remember it as last-used. */
    fun onProjectCreated(projectId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId != null) {
                runCatching { onboardingApi.complete(accountId) }
                selection.rememberLastProject(accountId, projectId)
            }
            auth.markOnboardingCompleted(java.time.Instant.now().toString())
            _done.value = true
        }
    }
}
