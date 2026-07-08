package com.exponential.app.ui.feedback

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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

/// Feedback entry point. Public boards are now read-only feedback-type projects
/// inside an otherwise-private workspace, and there is no self-service join: a
/// member syncs the board and opens it in-app, everyone else (non-members, or
/// servers without a public board) opens the public web board in the browser,
/// where the anonymous read-only view lives.
sealed interface FeedbackBoardState {
    data object Loading : FeedbackBoardState

    /** Member — waiting for the board's project to sync into local Room. */
    data object Syncing : FeedbackBoardState

    /** Not a member (or no public board here) — browser is the only path. */
    data object Browser : FeedbackBoardState
}

@HiltViewModel
class FeedbackBoardViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val holder: DatabaseHolder,
) : ViewModel() {
    private val _state = MutableStateFlow<FeedbackBoardState>(FeedbackBoardState.Loading)
    val state: StateFlow<FeedbackBoardState> = _state.asStateFlow()

    // One-shot navigation signal: the board project's id once it exists in the
    // active account's Room DB (navigation replaces this screen, so no reset).
    private val _openProjectId = MutableStateFlow<String?>(null)
    val openProjectId: StateFlow<String?> = _openProjectId.asStateFlow()

    val instanceUrl: StateFlow<String?> = auth.instanceUrl

    fun load() {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                _state.value = FeedbackBoardState.Browser
                return@launch
            }
            _state.value = FeedbackBoardState.Loading
            // Public-aware lookup — works for non-members too. NOT_FOUND
            // (self-hosted instances have no public board) and network failures
            // both land on the browser fallback.
            val preview = runCatching {
                workspacesApi.getBySlug(accountId, FEEDBACK_WORKSPACE_SLUG)
            }.getOrNull()
            if (preview == null || !preview.hasPublicBoard) {
                _state.value = FeedbackBoardState.Browser
                return@launch
            }
            // Only members sync the board locally; a non-member cannot render it
            // in-app (the mobile app syncs membership-scoped shapes only), so it
            // opens in the browser where the anonymous read-only view lives.
            if (preview.membership != null) {
                waitForBoard(accountId, preview.id)
            } else {
                _state.value = FeedbackBoardState.Browser
            }
        }
    }

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
            when (state) {
                FeedbackBoardState.Loading -> CircularProgressIndicator()

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
                }

                FeedbackBoardState.Browser -> Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassCard()
                        .padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        "The public feedback board opens in your browser.",
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
