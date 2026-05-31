package com.exponential.app.ui.admin

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Workspaces
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.AdminApi
import com.exponential.app.data.api.AdminWorkspace
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class AdminWorkspacesState(
    val workspaces: List<AdminWorkspace> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@HiltViewModel
class AdminWorkspacesViewModel @Inject constructor(
    private val adminApi: AdminApi,
    private val auth: com.exponential.app.data.auth.AuthRepository,
) : ViewModel() {
    private val _state = MutableStateFlow(AdminWorkspacesState())
    val state: StateFlow<AdminWorkspacesState> = _state.asStateFlow()

    fun refresh() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = _state.value.copy(loading = true, error = null)
            runCatching { adminApi.listWorkspaces(accountId) }
                .onSuccess { _state.value = _state.value.copy(workspaces = it, loading = false) }
                .onFailure { _state.value = _state.value.copy(loading = false, error = it.message) }
        }
    }

    fun delete(workspaceId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { adminApi.deleteWorkspace(accountId, workspaceId) }
                .onSuccess { refresh() }
                .onFailure { _state.value = _state.value.copy(error = it.message) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AdminWorkspacesScreen(
    onBack: () -> Unit,
    viewModel: AdminWorkspacesViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var pendingDelete by remember { mutableStateOf<AdminWorkspace?>(null) }
    var searchQuery by remember { mutableStateOf("") }

    val filteredWorkspaces = remember(state.workspaces, searchQuery) {
        if (searchQuery.isBlank()) state.workspaces
        else {
            val q = searchQuery.lowercase()
            state.workspaces.filter { ws ->
                ws.name.lowercase().contains(q) || ws.slug.lowercase().contains(q)
            }
        }
    }

    LaunchedEffect(Unit) { viewModel.refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Workspaces") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { viewModel.refresh() }) {
                        Icon(Icons.Filled.Refresh, contentDescription = "Refresh")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = androidx.compose.ui.graphics.Color.Transparent,
                ),
            )
        },
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            when {
                state.loading -> Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator()
                }
                state.error != null -> Box(modifier = Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                    Text(state.error!!, color = MaterialTheme.colorScheme.error)
                }
                else -> Column(modifier = Modifier.fillMaxSize()) {
                    OutlinedTextField(
                        value = searchQuery,
                        onValueChange = { searchQuery = it },
                        placeholder = { Text("Search by name or slug…") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    )
                    LazyColumn(contentPadding = PaddingValues(vertical = 8.dp)) {
                    items(filteredWorkspaces, key = { it.id }) { workspace ->
                        ListItem(
                            headlineContent = { Text(workspace.name) },
                            supportingContent = {
                                val planLabel = workspace.plan.replaceFirstChar { it.uppercase() }
                                Text(
                                    "$planLabel · ${workspace.memberCount} member${if (workspace.memberCount == 1) "" else "s"} · " +
                                        "${workspace.projectCount} project${if (workspace.projectCount == 1) "" else "s"}" +
                                        if (workspace.owners.isNotEmpty()) " · " + workspace.owners.joinToString { it.name ?: it.email } else "",
                                    maxLines = 1,
                                )
                            },
                            leadingContent = { Icon(Icons.Filled.Workspaces, contentDescription = null) },
                            trailingContent = {
                                IconButton(onClick = { pendingDelete = workspace }) {
                                    Icon(Icons.Filled.DeleteOutline, contentDescription = "Delete workspace")
                                }
                            },
                            colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                        )
                        HorizontalDivider()
                    }
                    }
                }
            }
        }
    }

    pendingDelete?.let { ws ->
        AlertDialog(
            onDismissRequest = { pendingDelete = null },
            title = { Text("Delete \"${ws.name}\"?") },
            text = { Text("All projects, issues, labels, and invites in this workspace will be deleted. This cannot be undone.") },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.delete(ws.id)
                    pendingDelete = null
                }) { Text("Delete") }
            },
            dismissButton = {
                TextButton(onClick = { pendingDelete = null }) { Text("Cancel") }
            },
        )
    }
}
