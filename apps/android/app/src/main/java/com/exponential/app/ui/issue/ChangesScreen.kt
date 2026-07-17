package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.automirrored.filled.CallMerge
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.AlertDialog
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.PrFilesApi
import com.exponential.app.data.api.PullFile
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.WorkspacePermissions
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// The dedicated diff + PR-review page (EXP-34, EXP-156): summary header
// (branch, PR state, totals, and the member-only Merge / Close PR controls)
// + per-file expandable unified patches. Opened from the issue detail's
// AgentPrCard on both the PR tier (issues.prFiles) and the pushed-branch tier
// (repositories.branchDiff). Horizontal scrolling stays inside each file's
// code block — the page itself never scrolls sideways.

sealed interface ChangesLoadState {
    data object Loading : ChangesLoadState
    data class Failed(val message: String) : ChangesLoadState
    data class Loaded(val files: List<PullFile>) : ChangesLoadState
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ChangesViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    holder: DatabaseHolder,
    private val auth: AuthRepository,
    private val prFilesApi: PrFilesApi,
    private val repositoriesApi: RepositoriesApi,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    val issueId: String = savedStateHandle["issueId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val issue: StateFlow<IssueEntity?> =
        dbFlow.scopedQuery<IssueEntity?>(null) { it.issueDao().observeById(issueId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // Membership resolution for the merge/close controls (mirrors
    // IssueDetailViewModel): issue → project → workspace + members + auth.
    private val projectFlow = combine(dbFlow, issue) { db, iss -> db to iss }
        .flatMapLatest { (db, iss) ->
            if (db == null || iss == null) flowOf(null)
            else db.projectDao().observeAll().map { projects -> projects.firstOrNull { it.id == iss.projectId } }
        }
    private val workspaceForProject = combine(dbFlow, projectFlow) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(null)
            else db.workspaceDao().observeById(project.workspaceId)
        }
    private val membersForWorkspace = combine(dbFlow, projectFlow) { db, project -> db to project }
        .flatMapLatest { (db, project) ->
            if (db == null || project == null) flowOf(emptyList())
            else db.workspaceMemberDao().observeByWorkspace(project.workspaceId)
        }
    val permissions: StateFlow<WorkspacePermissions> = combine(
        workspaceForProject,
        membersForWorkspace,
        auth.userId,
        auth.isAdmin,
    ) { workspace, members, userId, isAdmin ->
        WorkspacePermissions.resolve(
            workspace = workspace,
            currentUserId = userId,
            isAdmin = isAdmin,
            isMember = userId != null && members.any { it.userId == userId },
            memberRole = members.firstOrNull { it.userId == userId }?.role,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), WorkspacePermissions.Denied)

    private val _load = MutableStateFlow<ChangesLoadState>(ChangesLoadState.Loading)
    val load: StateFlow<ChangesLoadState> = _load

    // PR review actions (EXP-156 — merge/close moved here off the issue detail).
    // No local writes: the Electric echo flips prState and the controls vanish.
    private val _merging = MutableStateFlow(false)
    val merging: StateFlow<Boolean> = _merging
    private val _closing = MutableStateFlow(false)
    val closing: StateFlow<Boolean> = _closing
    private val _actionError = MutableStateFlow<String?>(null)
    val actionError: StateFlow<String?> = _actionError

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

    /** Squash-merge the issue's open PR via the GitHub App (batch PRs complete all linked issues). */
    fun mergePr() {
        if (_merging.value || _closing.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _merging.value = true
            _actionError.value = null
            runCatching { issuesApi.mergePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _actionError.value = trpcErrorMessage(t, "The pull request could not be merged")
                }
            _merging.value = false
        }
    }

    /** Close the issue's open PR WITHOUT merging (EXP-100 reject path). */
    fun closePr() {
        if (_closing.value || _merging.value) return
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            _closing.value = true
            _actionError.value = null
            runCatching { issuesApi.closePr(accountId, issueId) }
                .onFailure { t ->
                    if (t is CancellationException) throw t
                    _actionError.value = trpcErrorMessage(t, "The pull request could not be closed")
                }
            _closing.value = false
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
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val closing by viewModel.closing.collectAsStateWithLifecycle()
    val actionError by viewModel.actionError.collectAsStateWithLifecycle()

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

        // The summary header (branch, PR state, and the member-only Merge/Close
        // controls) renders regardless of the diff fetch — a prFiles/branchDiff
        // failure must NOT strand a member with no PR actions. The per-file diff
        // renders below it, per load state.
        val loadedFiles = (load as? ChangesLoadState.Loaded)?.files
        val expanded = remember(loadedFiles) {
            mutableStateMapOf<String, Boolean>().apply {
                loadedFiles?.forEach { put(it.filename, loadedFiles.size <= 3) }
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
                ChangesSummaryHeader(
                    issue = issue,
                    files = loadedFiles,
                    isMember = permissions.isMember,
                    merging = merging,
                    closing = closing,
                    actionError = actionError,
                    onMerge = viewModel::mergePr,
                    onClosePr = viewModel::closePr,
                )
            }
            when (val state = load) {
                ChangesLoadState.Loading -> item(key = "__loading__") {
                    Row(
                        modifier = Modifier.padding(vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                        Text("Loading changes…", style = MaterialTheme.typography.bodySmall, color = secondary)
                    }
                }
                is ChangesLoadState.Failed -> item(key = "__failed__") {
                    Text(
                        "Couldn’t load changes: ${state.message}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(vertical = 12.dp),
                    )
                }
                is ChangesLoadState.Loaded -> {
                    val files = state.files
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
private fun ChangesSummaryHeader(
    issue: IssueEntity?,
    // Null while the diff fetch is still loading/failed — the file totals hide,
    // but the branch/PR-state/actions (which come from the synced issue, not the
    // fetch) always render.
    files: List<PullFile>?,
    isMember: Boolean,
    merging: Boolean,
    closing: Boolean,
    actionError: String?,
    onMerge: () -> Unit,
    onClosePr: () -> Unit,
) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    var closeConfirmOpen by remember { mutableStateOf(false) }
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
            if (files != null) {
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
            }
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

        // Merge / Close controls (EXP-156) — members only, on an OPEN PR. Merge
        // is the primary; Close is a deliberately subtle reject. No local write:
        // the Electric echo flips prState and these controls disappear with it.
        val canReview = isMember &&
            !issue?.prUrl.isNullOrBlank() &&
            issue?.prState == DomainContract.prStateOpen
        if (canReview) {
            Spacer(Modifier.height(12.dp))
            val busy = merging || closing
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Button(
                    onClick = { mergeConfirmOpen = true },
                    enabled = !busy,
                ) {
                    if (merging) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(16.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                    } else {
                        Icon(
                            Icons.AutoMirrored.Filled.CallMerge,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text("Merge")
                    }
                }
                if (closing) {
                    CircularProgressIndicator(modifier = Modifier.size(14.dp), strokeWidth = 2.dp)
                } else {
                    Text(
                        "Close PR",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier
                            .clickable(enabled = !busy) { closeConfirmOpen = true }
                            .padding(horizontal = 4.dp, vertical = 4.dp),
                    )
                }
            }
            if (actionError != null) {
                Spacer(Modifier.height(6.dp))
                Text(
                    actionError,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }

    if (mergeConfirmOpen) {
        AlertDialog(
            onDismissRequest = { mergeConfirmOpen = false },
            title = { Text("Merge pull request?") },
            text = {
                Text("Squash-merges PR #${issue?.prNumber ?: ""} via the GitHub App.")
            },
            confirmButton = {
                TextButton(onClick = {
                    mergeConfirmOpen = false
                    onMerge()
                }) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeConfirmOpen = false }) { Text("Cancel") }
            },
        )
    }

    if (closeConfirmOpen) {
        AlertDialog(
            onDismissRequest = { closeConfirmOpen = false },
            title = { Text("Close pull request?") },
            text = {
                Text(
                    "Closes the pull request on GitHub WITHOUT merging — use this " +
                        "when the issue was dropped even though the work exists. " +
                        "The branch is kept and the PR can be reopened on GitHub.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    closeConfirmOpen = false
                    onClosePr()
                }) {
                    Text("Close PR", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { closeConfirmOpen = false }) { Text("Cancel") }
            },
        )
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
