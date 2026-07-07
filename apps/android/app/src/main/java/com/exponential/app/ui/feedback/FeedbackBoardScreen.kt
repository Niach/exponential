package com.exponential.app.ui.feedback

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.People
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.WorkspaceMembersApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

// The cloud bootstrap's public feedback board (apps/web bootstrap-cloud.ts).
private const val FEEDBACK_WORKSPACE_SLUG = "feedback"

/// Android port of the web join gate (`components/workspace/join-gate.tsx`):
/// public boards only sync once a user explicitly joins, so instead of the old
/// browser handoff the app resolves the board, offers the self-service join,
/// waits for the board's project to sync, and opens it in-app. Servers without
/// a public feedback board fall back to the existing external `/feedback` URL.
sealed interface FeedbackBoardState {
    data object Loading : FeedbackBoardState

    /** Signed-in non-member of an existing public board — offer the join. */
    data class Gate(
        val workspaceName: String,
        val joining: Boolean = false,
        val error: String? = null,
    ) : FeedbackBoardState

    /** Member (just joined or already) — waiting for the board to sync locally. */
    data object Syncing : FeedbackBoardState

    /** No public feedback board reachable — browser fallback only. */
    data object Unavailable : FeedbackBoardState
}

@HiltViewModel
class FeedbackBoardViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val membersApi: WorkspaceMembersApi,
    private val holder: DatabaseHolder,
) : ViewModel() {
    private val _state = MutableStateFlow<FeedbackBoardState>(FeedbackBoardState.Loading)
    val state: StateFlow<FeedbackBoardState> = _state.asStateFlow()

    // One-shot navigation signal: the board project's id once it exists in the
    // active account's Room DB (navigation replaces this screen, so no reset).
    private val _openProjectId = MutableStateFlow<String?>(null)
    val openProjectId: StateFlow<String?> = _openProjectId.asStateFlow()

    val instanceUrl: StateFlow<String?> = auth.instanceUrl

    private var workspaceId: String? = null

    fun load() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = FeedbackBoardState.Unavailable
                return@launch
            }
            _state.value = FeedbackBoardState.Loading
            // Public-aware lookup — works for non-members, who have nothing
            // synced yet. NOT_FOUND (self-hosted instances have no public
            // board) and network failures both land on the browser fallback.
            val preview = runCatching {
                workspacesApi.getBySlug(accountId, FEEDBACK_WORKSPACE_SLUG)
            }.getOrNull()
            if (preview == null || !preview.isPublic) {
                _state.value = FeedbackBoardState.Unavailable
                return@launch
            }
            workspaceId = preview.id
            if (preview.membership != null) {
                waitForBoard(accountId, preview.id)
            } else {
                _state.value = FeedbackBoardState.Gate(preview.name)
            }
        }
    }

    fun join() {
        val gate = _state.value as? FeedbackBoardState.Gate ?: return
        val id = workspaceId ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _state.value = gate.copy(joining = true, error = null)
            runCatching { membersApi.join(accountId, id) }
                .onSuccess { waitForBoard(accountId, id) }
                .onFailure {
                    _state.value = gate.copy(joining = false, error = it.message)
                }
        }
    }

    // After the join, the board arrives through the normal Electric pipelines:
    // the new membership rotates every shape's where clause, so each shape
    // 409-refetches on its next poll (worst case one live long-poll cycle).
    // Wait for the board's project row to land, then signal navigation.
    private suspend fun waitForBoard(accountId: String, workspaceId: String) {
        _state.value = FeedbackBoardState.Syncing
        val db = holder.database(forAccountId = accountId)
        val projects = db.projectDao().observeByWorkspace(workspaceId).first { it.isNotEmpty() }
        _openProjectId.value = projects.first().id
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FeedbackBoardScreen(
    onBack: () -> Unit,
    onOpenBoard: (projectId: String) -> Unit,
    viewModel: FeedbackBoardViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val openProjectId by viewModel.openProjectId.collectAsStateWithLifecycle()
    val instanceUrl by viewModel.instanceUrl.collectAsStateWithLifecycle()
    val context = LocalContext.current

    LaunchedEffect(Unit) { viewModel.load() }
    LaunchedEffect(openProjectId) { openProjectId?.let(onOpenBoard) }

    // The pre-join behavior, kept as the fallback when the in-app join isn't
    // possible (no public board on this server, resolution failed) or sync is
    // taking too long for the user's patience.
    val openInBrowser: () -> Unit = {
        instanceUrl?.let { base ->
            runCatching {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("$base/feedback"))
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(intent)
            }
        }
    }

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Feedback") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Box(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .padding(24.dp),
            contentAlignment = Alignment.Center,
        ) {
            when (val current = state) {
                FeedbackBoardState.Loading -> CircularProgressIndicator()

                is FeedbackBoardState.Gate -> Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassCard()
                        .padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(
                        Icons.Filled.People,
                        contentDescription = null,
                        modifier = Modifier.size(28.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Text(
                        "Join ${current.workspaceName}",
                        style = MaterialTheme.typography.titleMedium,
                        textAlign = TextAlign.Center,
                    )
                    Text(
                        "This is a public board. Join it to browse issues, follow " +
                            "discussions and share feedback. You can leave again anytime.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        textAlign = TextAlign.Center,
                    )
                    if (current.error != null) {
                        Text(
                            current.error,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            textAlign = TextAlign.Center,
                        )
                    }
                    Button(
                        onClick = { viewModel.join() },
                        enabled = !current.joining,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (current.joining) "Joining…" else "Join board")
                    }
                }

                FeedbackBoardState.Syncing -> Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    CircularProgressIndicator()
                    Text(
                        "Syncing the board…",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    TextButton(onClick = openInBrowser) {
                        Text("Open in browser instead")
                    }
                }

                FeedbackBoardState.Unavailable -> Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassCard()
                        .padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        "The public feedback board isn't available in the app for this server.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        textAlign = TextAlign.Center,
                    )
                    Button(onClick = openInBrowser, modifier = Modifier.fillMaxWidth()) {
                        Text("Open feedback in browser")
                    }
                }
            }
        }
    }
}
