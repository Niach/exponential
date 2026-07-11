package com.exponential.app.ui.releases

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.CreateReleaseInput
import com.exponential.app.data.api.ReleasesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ReleaseEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.releaseComparator
import com.exponential.app.domain.releaseProgress
import com.exponential.app.domain.releaseProgressText
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Workspace Releases list (EXP-56): every release in the workspace, unshipped
// first (by target date), then shipped (most recent first) — the shared
// releaseComparator contract. Progress is pure client work over the synced
// issues shape (issues.release_id). Pushed from the Issues screen's top-bar
// rocket action (the bottom bar is full). Mobile never launches coding.

data class ReleasesListState(
    val releases: List<ReleaseEntity> = emptyList(),
    val issuesByRelease: Map<String, List<IssueEntity>> = emptyMap(),
)

@HiltViewModel
class ReleasesListViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val releasesApi: ReleasesApi,
) : ViewModel() {

    val workspaceId: String = savedStateHandle["workspaceId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ReleasesListState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.releaseDao().observeByWorkspace(workspaceId) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAllInReleases() },
    ) { releases, releaseIssues ->
        ReleasesListState(
            releases = releases.sortedWith(releaseComparator),
            // Cross-workspace bleed is impossible: an issue's release always
            // lives in the issue's own workspace.
            issuesByRelease = releaseIssues
                .filter { it.releaseId != null }
                .groupBy { it.releaseId!! },
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ReleasesListState())

    fun createRelease(name: String, description: String?) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            runCatching {
                releasesApi.create(
                    accountId,
                    CreateReleaseInput(workspaceId = workspaceId, name = name, description = description),
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReleasesListScreen(
    onOpenRelease: (releaseId: String) -> Unit,
    onBack: () -> Unit,
    viewModel: ReleasesListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var createOpen by remember { mutableStateOf(false) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Releases") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { createOpen = true }) {
                        Icon(Icons.Filled.Add, contentDescription = "New release")
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            if (state.releases.isEmpty()) {
                EmptyState(
                    message = "No releases yet. Bundle issues into a release to track what ships together and when.",
                    icon = Icons.Filled.RocketLaunch,
                    action = {
                        Button(onClick = { createOpen = true }) { Text("New release") }
                    },
                )
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    items(state.releases, key = { it.id }) { release ->
                        ReleaseRow(
                            release = release,
                            issues = state.issuesByRelease[release.id].orEmpty(),
                            onClick = { onOpenRelease(release.id) },
                        )
                    }
                }
            }
        }
    }

    if (createOpen) {
        CreateReleaseSheet(
            onCreate = { name, description ->
                viewModel.createRelease(name, description)
                createOpen = false
            },
            onDismiss = { createOpen = false },
        )
    }
}

@Composable
private fun ReleaseRow(
    release: ReleaseEntity,
    issues: List<IssueEntity>,
    onClick: () -> Unit,
) {
    val progress = releaseProgress(issues.map { it.status })
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Filled.RocketLaunch,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    release.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.width(8.dp))
                ReleaseStatePill(release = release, isComplete = progress.isComplete)
            }
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (release.targetDate != null) {
                    Icon(
                        Icons.Filled.CalendarMonth,
                        contentDescription = null,
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Spacer(Modifier.width(4.dp))
                    Text(
                        "${formatDueDate(release.targetDate)} · ",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                Text(
                    releaseProgressText(progress),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        LinearProgressIndicator(
            progress = { progress.fraction },
            modifier = Modifier.width(88.dp),
            color = DesignTokens.Semantic.Green,
            trackColor = Color.White.copy(alpha = 0.08f),
        )
    }
}

/**
 * "Shipped <date>" (filled emerald) when shipped_at is set; "Ready" (outline
 * emerald) when all non-dropped issues are done and the release is unshipped;
 * nothing otherwise. Mirrors the web's ReleaseStatePill.
 */
@Composable
internal fun ReleaseStatePill(release: ReleaseEntity, isComplete: Boolean) {
    val green = DesignTokens.Semantic.Green
    when {
        release.shippedAt != null -> Pill(
            text = "Shipped ${formatShippedDate(release.shippedAt)}".trim(),
            color = green,
            filled = true,
        )
        isComplete -> Pill(text = "Ready", color = green, filled = false)
    }
}

@Composable
private fun Pill(text: String, color: Color, filled: Boolean) {
    val shape = RoundedCornerShape(50)
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = color,
        maxLines = 1,
        modifier = Modifier
            .clip(shape)
            .background(if (filled) color.copy(alpha = 0.12f) else Color.Transparent, shape)
            .border(1.dp, color.copy(alpha = 0.4f), shape)
            .padding(horizontal = 6.dp, vertical = 1.dp),
    )
}

/** "MMM d" from a synced ISO-8601 shipped_at timestamp; empty when unparseable. */
internal fun formatShippedDate(shippedAt: String?): String {
    if (shippedAt.isNullOrBlank()) return ""
    return runCatching {
        val instant = java.time.Instant.parse(shippedAt)
        java.time.format.DateTimeFormatter.ofPattern("MMM d")
            .withZone(java.time.ZoneId.systemDefault())
            .format(instant)
    }.getOrElse {
        // Postgres-style "yyyy-MM-dd HH:mm:ss+00" — take the date prefix.
        runCatching {
            java.time.LocalDate.parse(shippedAt.take(10))
                .format(java.time.format.DateTimeFormatter.ofPattern("MMM d"))
        }.getOrDefault("")
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun CreateReleaseSheet(
    onCreate: (name: String, description: String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var name by remember { mutableStateOf("") }
    var description by remember { mutableStateOf("") }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Text("New release", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                placeholder = { Text("Release name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                value = description,
                onValueChange = { description = it },
                placeholder = { Text("Description (optional)") },
                minLines = 2,
                maxLines = 4,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(16.dp))
            Button(
                onClick = {
                    val trimmed = name.trim()
                    if (trimmed.isNotEmpty()) {
                        onCreate(trimmed, description.trim().ifEmpty { null })
                    }
                },
                enabled = name.trim().isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Create release")
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}
