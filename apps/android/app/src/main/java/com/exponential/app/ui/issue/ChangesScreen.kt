package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.PrFilesApi
import com.exponential.app.data.api.PullFile
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The dedicated diff page (EXP-34): summary header (branch, PR state, totals)
// + per-file expandable unified patches. Opened from ChangesSection's "View
// changes" on both the PR tier (issues.prFiles) and the pushed-branch tier
// (repositories.branchDiff). Horizontal scrolling stays inside each file's
// code block — the page itself never scrolls sideways.

sealed interface ChangesLoadState {
    data object Loading : ChangesLoadState
    data class Failed(val message: String) : ChangesLoadState
    data class Loaded(val files: List<PullFile>) : ChangesLoadState
}

@HiltViewModel
class ChangesViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val prFilesApi: PrFilesApi,
    private val repositoriesApi: RepositoriesApi,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val issue: StateFlow<IssueEntity?> =
        dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(issueId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val _load = MutableStateFlow<ChangesLoadState>(ChangesLoadState.Loading)
    val load: StateFlow<ChangesLoadState> = _load

    init {
        // Re-fetch when the diff source flips (a PR opens on a watched branch).
        viewModelScope.launch {
            issue.filterNotNull()
                .map { it.prUrl.isNullOrBlank() }
                .distinctUntilChanged()
                .collectLatest { refresh() }
        }
    }

    fun refresh() {
        viewModelScope.launch {
            _load.value = ChangesLoadState.Loading
            try {
                val accountId = auth.activeAccountId.value
                    ?: throw IllegalStateException("No active account")
                val hasPr = !issue.value?.prUrl.isNullOrBlank()
                val files = if (hasPr) {
                    prFilesApi.get(accountId, issueId).files
                } else {
                    repositoriesApi.branchDiff(accountId, issueId)?.files ?: emptyList()
                }
                _load.value = ChangesLoadState.Loaded(files)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _load.value = ChangesLoadState.Failed(t.message ?: "Failed to load changes")
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChangesScreen(
    onBack: () -> Unit,
    viewModel: ChangesViewModel = hiltViewModel(),
) {
    val issue by viewModel.issue.collectAsStateWithLifecycle()
    val load by viewModel.load.collectAsStateWithLifecycle()

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Changes") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

        when (val state = load) {
            ChangesLoadState.Loading -> Box(
                modifier = Modifier.padding(padding).fillMaxSize(),
                contentAlignment = Alignment.Center,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                    Text("Loading changes…", style = MaterialTheme.typography.bodySmall, color = secondary)
                }
            }
            is ChangesLoadState.Failed -> Box(
                modifier = Modifier.padding(padding).fillMaxSize().padding(20.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    "Couldn’t load changes: ${state.message}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            is ChangesLoadState.Loaded -> {
                val files = state.files
                // ≤3 files start expanded, more start collapsed.
                val expanded = remember(files) {
                    mutableStateMapOf<String, Boolean>().apply {
                        files.forEach { put(it.filename, files.size <= 3) }
                    }
                }
                LazyColumn(
                    modifier = Modifier.padding(padding).fillMaxSize(),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(
                        start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp,
                    ),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    item(key = "__summary__") {
                        ChangesSummaryHeader(issue = issue, files = files)
                    }
                    if (files.isEmpty()) {
                        item(key = "__empty__") {
                            Text(
                                "No changed files.",
                                style = MaterialTheme.typography.bodySmall,
                                color = tertiary,
                                modifier = Modifier.padding(vertical = 12.dp),
                            )
                        }
                    }
                    items(files, key = { it.filename }) { file ->
                        FileSection(
                            file = file,
                            expanded = expanded[file.filename] == true,
                            onToggle = {
                                expanded[file.filename] = expanded[file.filename] != true
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ChangesSummaryHeader(issue: IssueEntity?, files: List<PullFile>) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth().glassSection().padding(12.dp)) {
        val branch = issue?.branch
        if (!branch.isNullOrBlank()) {
            Text(
                branch,
                style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
                color = secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(8.dp))
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            val prState = issue?.prState
            if (!prState.isNullOrBlank()) {
                Text(
                    prState.replaceFirstChar { it.uppercase() },
                    style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier
                        .glassButton()
                        .padding(horizontal = 8.dp, vertical = 3.dp),
                )
            }
            Text(
                "${files.size} ${if (files.size == 1) "file" else "files"}",
                style = MaterialTheme.typography.labelMedium,
                color = secondary,
            )
            Text(
                "+${files.sumOf { it.additions }}",
                color = DiffAddColor,
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
            )
            Text(
                "−${files.sumOf { it.deletions }}",
                color = DiffDelColor,
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
            )
            Spacer(Modifier.weight(1f))
            val prUrl = issue?.prUrl
            if (!prUrl.isNullOrBlank()) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.clickable {
                        runCatching {
                            val intent = android.content.Intent(
                                android.content.Intent.ACTION_VIEW,
                                android.net.Uri.parse(prUrl),
                            )
                            intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(intent)
                        }
                    },
                ) {
                    Icon(
                        Icons.AutoMirrored.Filled.OpenInNew,
                        contentDescription = null,
                        modifier = Modifier.size(13.dp),
                        tint = CommentAccent,
                    )
                    Spacer(Modifier.width(5.dp))
                    Text(
                        "View PR",
                        style = MaterialTheme.typography.labelMedium,
                        color = CommentAccent,
                    )
                }
            }
        }
    }
}

// One changed file: a tappable header (status letter, filename, +/− counts)
// over a collapsible unified patch with the shared line coloring.
@Composable
private fun FileSection(file: PullFile, expanded: Boolean, onToggle: () -> Unit) {
    val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth().glassSection()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onToggle)
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                statusLetter(file.status),
                color = statusColor(file.status),
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                file.filename,
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            Text("+${file.additions}", color = DiffAddColor, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            Spacer(Modifier.width(4.dp))
            Text("−${file.deletions}", color = DiffDelColor, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
            Spacer(Modifier.width(6.dp))
            Icon(
                if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (expanded) {
            val patch = file.patch
            if (!patch.isNullOrEmpty()) {
                PatchLines(
                    lines = remember(patch) { patch.split("\n") },
                    contextColor = contextColor,
                    modifier = Modifier.padding(bottom = 8.dp),
                )
            } else {
                Text(
                    if (file.status == "renamed") "Renamed." else "No textual diff (binary or too large).",
                    style = MaterialTheme.typography.bodySmall,
                    color = contextColor,
                    modifier = Modifier.padding(horizontal = 12.dp).padding(bottom = 10.dp),
                )
            }
        }
    }
}

// GitHub file statuses: added / modified / removed / renamed / copied / changed.
private fun statusLetter(status: String): String = when (status) {
    "added" -> "A"
    "removed" -> "D"
    "renamed" -> "R"
    "copied" -> "C"
    else -> "M"
}

@Composable
private fun statusColor(status: String): Color = when (status) {
    "added" -> DiffAddColor
    "removed" -> DiffDelColor
    "renamed", "copied" -> DiffHunkColor
    else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
}
