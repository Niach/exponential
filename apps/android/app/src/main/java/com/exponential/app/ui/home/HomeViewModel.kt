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
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class HomeState(
    val email: String? = null,
    val workspaces: List<WorkspaceEntity> = emptyList(),
    val selectedWorkspace: WorkspaceEntity? = null,
    val projects: List<ProjectEntity> = emptyList(),
    val error: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val workspacesApi: WorkspacesApi,
    private val workspaceDao: WorkspaceDao,
    private val projectDao: ProjectDao,
) : ViewModel() {

    private val _selectedId = MutableStateFlow<String?>(null)

    private val workspacesFlow = workspaceDao.observeAll()

    private val projectsFlow = _selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else projectDao.observeByWorkspace(id)
    }

    val state: StateFlow<HomeState> = combine(
        workspacesFlow,
        _selectedId,
        projectsFlow,
        auth.userEmail,
    ) { workspaces, selectedId, projects, email ->
        val selected = workspaces.firstOrNull { it.id == selectedId }
            ?: workspaces.firstOrNull()
        HomeState(
            email = email,
            workspaces = workspaces,
            selectedWorkspace = selected,
            projects = projects,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    fun bootstrap() {
        viewModelScope.launch {
            try {
                val workspace = workspacesApi.ensureDefault()
                workspaceDao.upsert(workspace)
                if (_selectedId.value == null) _selectedId.value = workspace.id
                _error.value = null
            } catch (error: Throwable) {
                _error.value = error.message ?: "Failed to load workspace"
            }
        }
    }

    fun selectWorkspace(id: String) {
        _selectedId.value = id
    }
}
