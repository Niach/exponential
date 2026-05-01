package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceDao
import com.exponential.app.data.db.WorkspaceEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class HomeState(
    val email: String? = null,
    val workspace: WorkspaceEntity? = null,
    val projects: List<ProjectEntity> = emptyList(),
    val error: String? = null,
)

@HiltViewModel
@kotlinx.coroutines.ExperimentalCoroutinesApi
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val workspacesApi: WorkspacesApi,
    private val workspaceDao: WorkspaceDao,
    private val projectDao: ProjectDao,
) : ViewModel() {

    private val workspaceFlow = MutableStateFlow<WorkspaceEntity?>(null)

    private val projectsFlow = workspaceFlow.flatMapLatest { ws ->
        if (ws == null) flowOf(emptyList()) else projectDao.observeByWorkspace(ws.id)
    }

    val state: StateFlow<HomeState> = combine(
        workspaceFlow,
        projectsFlow,
        auth.userEmail,
    ) { workspace, projects, email ->
        HomeState(email = email, workspace = workspace, projects = projects)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    fun bootstrap() {
        viewModelScope.launch {
            try {
                val email = authApi.fetchSession()
                if (email != null) auth.setToken(auth.token.value ?: return@launch, email)

                val workspace = workspacesApi.ensureDefault()
                workspaceDao.upsert(workspace)
                workspaceFlow.value = workspace
                _error.value = null
            } catch (error: Throwable) {
                _error.value = error.message ?: "Failed to load workspace"
            }
        }
    }
}
