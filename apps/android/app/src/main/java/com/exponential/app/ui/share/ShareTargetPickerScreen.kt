package com.exponential.app.ui.share

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.ProjectRow
import com.exponential.app.ui.components.WorkspaceAvatar
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

data class WorkspaceProjects(val workspace: WorkspaceEntity, val projects: List<ProjectEntity>)

data class ShareTargetState(
    val groups: List<WorkspaceProjects> = emptyList(),
    val recentProjectId: String? = null,
    val isLoading: Boolean = true,
)

@HiltViewModel
class ShareTargetPickerViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: WorkspaceSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ShareTargetState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.workspaceDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.projectDao().observeAll() },
        auth.activeAccountId,
    ) { workspaces, projects, accountId ->
        val byWorkspace = projects.groupBy { it.workspaceId }
        val groups = workspaces.mapNotNull { ws ->
            val ps = byWorkspace[ws.id].orEmpty()
            if (ps.isEmpty()) null else WorkspaceProjects(ws, ps)
        }
        ShareTargetState(
            groups = groups,
            recentProjectId = accountId?.let { selection.lastProject(it) },
            isLoading = false,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ShareTargetState())
}

/**
 * Project picker shown when content is shared into the app. Lists the active
 * account's workspaces → projects (pinning the most recently opened project at
 * the top), and hands the chosen `projectId` back so the create flow can open
 * pre-filled with the shared content.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareTargetPickerScreen(
    onPicked: (projectId: String) -> Unit,
    onCancel: () -> Unit,
    viewModel: ShareTargetPickerViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val recentProject = remember(state) {
        state.recentProjectId?.let { id ->
            state.groups.firstNotNullOfOrNull { g -> g.projects.firstOrNull { it.id == id } }
        }
    }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            TopAppBar(
                title = { Text("Add to project") },
                navigationIcon = {
                    IconButton(onClick = onCancel) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            when {
                state.isLoading -> LoadingState()
                state.groups.isEmpty() -> EmptyState(
                    message = "Open Exponential to create your first project, then try sharing again.",
                    icon = Icons.Filled.Inbox,
                )
                else -> LazyColumn(
                    contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (recentProject != null) {
                        item(key = "recent") {
                            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                SectionLabel("Recent")
                                ProjectRow(project = recentProject, onClick = { onPicked(recentProject.id) })
                            }
                        }
                    }
                    items(state.groups, key = { it.workspace.id }) { group ->
                        Column(
                            modifier = Modifier.fillMaxWidth(),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Row(
                                modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                WorkspaceAvatar(group.workspace, size = 18.dp)
                                Spacer(Modifier.width(8.dp))
                                Text(
                                    group.workspace.name,
                                    style = MaterialTheme.typography.labelMedium,
                                    color = MaterialTheme.colorScheme.onSurface,
                                )
                            }
                            group.projects.forEach { project ->
                                ProjectRow(project = project, onClick = { onPicked(project.id) })
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(horizontal = 4.dp),
    )
}
