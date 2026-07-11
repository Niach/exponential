package com.exponential.app.ui.releases

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ReleasesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ReleaseEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.releaseProgress
import com.exponential.app.domain.releaseProgressText
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Release detail (EXP-56): header (name, target date, shipped state, PR pill
// linking out), ship/unship, delete (confirmed), and the release's issues
// grouped by status like the project board. Rows navigate to the issue
// detail; the per-row X unbundles a row (setIssueRelease null — the issue
// survives). Mobile never launches coding — no run affordance here.

data class ReleaseDetailState(
    val release: ReleaseEntity? = null,
    val issues: List<IssueEntity> = emptyList(),
)

@HiltViewModel
class ReleaseDetailViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val releasesApi: ReleasesApi,
) : ViewModel() {

    val releaseId: String = savedStateHandle["releaseId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ReleaseDetailState> = combine(
        dbFlow.scopedQuery<ReleaseEntity?>(null) { it.releaseDao().observeById(releaseId) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeByRelease(releaseId) },
    ) { release, issues ->
        ReleaseDetailState(release = release, issues = issues)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReleaseDetailState())

    fun toggleShipped() {
        val release = state.value.release ?: return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                releasesApi.markShipped(accountId, release.id, shipped = release.shippedAt == null)
            }
        }
    }

    fun delete(onDeleted: () -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { releasesApi.delete(accountId, releaseId) }.onSuccess { onDeleted() }
        }
    }

    fun removeIssue(issueId: String) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching { releasesApi.setIssueRelease(accountId, issueId, null) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReleaseDetailScreen(
    onOpenIssue: (issueId: String) -> Unit,
    onBack: () -> Unit,
    viewModel: ReleaseDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val release = state.release
    var overflowOpen by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Release") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    if (release != null) {
                        TextButton(onClick = { viewModel.toggleShipped() }) {
                            Text(if (release.shippedAt == null) "Mark shipped" else "Unship")
                        }
                        IconButton(onClick = { overflowOpen = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "Release actions")
                        }
                        DropdownMenu(expanded = overflowOpen, onDismissRequest = { overflowOpen = false }) {
                            DropdownMenuItem(
                                text = { Text("Delete release", color = MaterialTheme.colorScheme.error) },
                                leadingIcon = {
                                    Icon(
                                        Icons.Filled.DeleteOutline,
                                        contentDescription = null,
                                        tint = MaterialTheme.colorScheme.error,
                                    )
                                },
                                onClick = {
                                    overflowOpen = false
                                    confirmDelete = true
                                },
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        if (release == null) {
            LoadingState(modifier = Modifier.padding(padding))
        } else {
            val progress = releaseProgress(state.issues.map { it.status })
            LazyColumn(
                modifier = Modifier.padding(padding).fillMaxSize(),
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                item(key = "header") {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .glassSection()
                            .padding(14.dp),
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                release.name,
                                style = MaterialTheme.typography.titleLarge,
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f, fill = false),
                            )
                            Spacer(Modifier.width(8.dp))
                            ReleaseStatePill(release = release, isComplete = progress.isComplete)
                        }
                        if (!release.description.isNullOrBlank()) {
                            Spacer(Modifier.height(6.dp))
                            Text(
                                release.description,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                        Spacer(Modifier.height(10.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (release.targetDate != null) {
                                Icon(
                                    Icons.Filled.CalendarMonth,
                                    contentDescription = null,
                                    modifier = Modifier.size(13.dp),
                                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                                Spacer(Modifier.width(4.dp))
                                Text(
                                    "Target ${formatDueDate(release.targetDate)}",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                                Spacer(Modifier.width(10.dp))
                            }
                            ReleasePrPill(release)
                        }
                        Spacer(Modifier.height(10.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            LinearProgressIndicator(
                                progress = { progress.fraction },
                                modifier = Modifier.weight(1f),
                                color = DesignTokens.Semantic.Green,
                                trackColor = Color.White.copy(alpha = 0.08f),
                            )
                            Spacer(Modifier.width(8.dp))
                            Text(
                                releaseProgressText(progress),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    }
                }

                item(key = "spacer-header") { Spacer(Modifier.height(12.dp)) }

                if (state.issues.isEmpty()) {
                    item(key = "empty") {
                        EmptyState(
                            message = "No issues in this release. Add issues from their detail page's Release picker.",
                            icon = Icons.Filled.RocketLaunch,
                            modifier = Modifier.fillMaxWidth().padding(top = 48.dp),
                        )
                    }
                } else {
                    // Status-grouped issues — the same grouping/order the
                    // project board list uses (issueStatusOrder, empty groups
                    // hidden).
                    issueStatusOrder.forEach { status ->
                        val statusIssues = state.issues
                            .filter { it.status == status.wire }
                            .sortedWith(compareBy({ it.sortOrder }, { it.createdAt }))
                        if (statusIssues.isNotEmpty()) {
                            item(key = "header-${status.wire}") {
                                StatusGroupHeader(status = status, count = statusIssues.size)
                            }
                            items(statusIssues, key = { it.id }) { issue ->
                                ReleaseIssueRow(
                                    issue = issue,
                                    onClick = { onOpenIssue(issue.id) },
                                    onRemove = { viewModel.removeIssue(issue.id) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    if (confirmDelete && release != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete release") },
            text = {
                Text(
                    "Delete ${release.name}? Its issues are kept — they just leave the release. " +
                        "This cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.delete(onBack)
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text("Cancel") }
            },
        )
    }
}

/** The release PR (integration branch → default), linking out when set. */
@Composable
private fun ReleasePrPill(release: ReleaseEntity) {
    val prUrl = release.prUrl ?: return
    val uriHandler = LocalUriHandler.current
    val merged = release.prState == "merged"
    val tint = if (merged) Color(0xFF8B5CF6) else DesignTokens.Semantic.Green
    Row(
        modifier = Modifier
            .clickable { runCatching { uriHandler.openUri(prUrl) } }
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Filled.RocketLaunch,
            contentDescription = null,
            modifier = Modifier.size(12.dp),
            tint = tint,
        )
        Spacer(Modifier.width(4.dp))
        Text(
            buildString {
                release.prNumber?.let { append("#$it") }
                release.prState?.let {
                    if (isNotEmpty()) append(" ")
                    append(it.replaceFirstChar(Char::uppercase))
                }
                if (isEmpty()) append("Pull request")
            },
            style = MaterialTheme.typography.labelSmall,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

@Composable
private fun StatusGroupHeader(status: IssueStatus, count: Int) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 4.dp, start = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StatusIcon(status, size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            status.label,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(6.dp))
        Text(
            "$count",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

@Composable
private fun ReleaseIssueRow(
    issue: IssueEntity,
    onClick: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PriorityIcon(IssuePriority.fromWire(issue.priority), size = 14.dp)
        Spacer(Modifier.width(10.dp))
        Text(
            issue.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            modifier = Modifier.widthIn(min = 56.dp),
        )
        Spacer(Modifier.width(8.dp))
        StatusIcon(IssueStatus.fromWire(issue.status), size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            issue.title,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        IconButton(onClick = onRemove) {
            Icon(
                Icons.Filled.Close,
                contentDescription = "Remove from release",
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}
