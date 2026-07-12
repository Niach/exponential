package com.exponential.app.ui.issue

import android.content.Intent
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.DriveFileMove
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.NotificationsOff
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.PersonOff
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// iOS-parity issue detail: a centered "Issue" nav title, an identifier chip +
// overflow header row, a large editable title, the description editor, then the
// metadata/property cards (IssueMetadataEditor), attachments and comments.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueDetailScreen(
    issueId: String,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit = {},
    onOpenSteer: (String) -> Unit = {},
    onOpenChanges: () -> Unit = {},
    viewModel: IssueDetailViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val isSubscribed by viewModel.isSubscribed.collectAsStateWithLifecycle()
    val runningSession by viewModel.runningSession.collectAsStateWithLifecycle()
    val repoName by viewModel.repoName.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val steerDevices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val duplicateOf by viewModel.duplicateOf.collectAsStateWithLifecycle()
    val duplicateCandidates by viewModel.duplicateCandidates.collectAsStateWithLifecycle()
    val shareUrl by viewModel.shareUrl.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    // EXP-50: solo workspaces (one human member) hide the assignee row.
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    // EXP-56: the workspace's releases + this issue's current one.
    val workspaceReleases by viewModel.workspaceReleases.collectAsStateWithLifecycle()
    val currentRelease by viewModel.currentRelease.collectAsStateWithLifecycle()
    // EXP-57: same-workspace projects the issue can move to.
    val moveTargets by viewModel.moveTargets.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val issue = state.issue
    var titleField by remember { mutableStateOf("") }
    var descriptionField by remember { mutableStateOf("") }
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var assigneeMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }
    var dueTimePickerOpen by remember { mutableStateOf(false) }
    var endTimePickerOpen by remember { mutableStateOf(false) }
    var recurrenceSheetOpen by remember { mutableStateOf(false) }
    var labelsOpen by remember { mutableStateOf(false) }
    var releaseMenuOpen by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var duplicatePickerOpen by remember { mutableStateOf(false) }
    var overflowOpen by remember { mutableStateOf(false) }
    var movePickerOpen by remember { mutableStateOf(false) }
    // The picked target project, pending the move confirmation (EXP-57).
    var moveTarget by remember { mutableStateOf<com.exponential.app.data.db.ProjectEntity?>(null) }

    LaunchedEffect(issue?.id) {
        if (issue != null) {
            titleField = issue.title
            descriptionField = extractDescriptionMarkdown(issue.description)
        }
    }

    // Surface failed description saves (retries exhausted) — the draft is
    // retained in the ViewModel, so the user knows to stay/retry instead of
    // believing the edit persisted.
    val descriptionSaveError by viewModel.descriptionSaveError.collectAsStateWithLifecycle()
    val snackbarHostState = remember { SnackbarHostState() }
    LaunchedEffect(descriptionSaveError) {
        descriptionSaveError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeDescriptionSaveError()
        }
    }

    // Surface a failed move (EXP-57) — otherwise the issue silently stays put.
    val moveError by viewModel.moveError.collectAsStateWithLifecycle()
    LaunchedEffect(moveError) {
        moveError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeMoveError()
        }
    }

    // Inline `#IDENTIFIER` pills + editor #-autocomplete (masterplan §5e):
    // resolve against this workspace's synced issues; a tap navigates to the
    // referenced issue. The CompositionLocal reaches every MarkdownView below
    // (description read view + comment thread) and every embedded editor
    // (description, comment composer, comment edit).
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    val issueRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(issueRefCandidates) { target -> currentOnOpenIssue(target.issueId) }
    }

    CompositionLocalProvider(LocalIssueRefs provides issueRefHandler) {
    ProvideMarkdownToolbar {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Issue") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    if (issue != null) {
                        val url = shareUrl
                        if (url != null) {
                            IconButton(onClick = {
                                val send = Intent(Intent.ACTION_SEND).apply {
                                    type = "text/plain"
                                    putExtra(
                                        Intent.EXTRA_TEXT,
                                        "${issue.identifier}: ${issue.title}\n$url",
                                    )
                                }
                                runCatching {
                                    context.startActivity(
                                        Intent.createChooser(send, "Share issue"),
                                    )
                                }
                            }) {
                                Icon(Icons.Filled.Share, contentDescription = "Share issue")
                            }
                        }
                        IconButton(onClick = { viewModel.toggleSubscribe() }) {
                            Icon(
                                if (isSubscribed) Icons.Filled.Notifications else Icons.Filled.NotificationsOff,
                                contentDescription = if (isSubscribed) "Unsubscribe" else "Subscribe",
                                tint = if (isSubscribed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        // Overflow lives in the nav bar (parity with iOS); the
                        // content header below carries only the identifier + repo
                        // chips. Moderator-gated, matching the pickers' guards.
                        if (isModerator) {
                            Box {
                                IconButton(onClick = { overflowOpen = true }) {
                                    Icon(Icons.Filled.MoreVert, contentDescription = "Issue actions")
                                }
                                DropdownMenu(expanded = overflowOpen, onDismissRequest = { overflowOpen = false }) {
                                    // Duplicate = status interception (L27): marking a
                                    // duplicate happens by picking the `duplicate` status,
                                    // which opens the canonical-issue picker. Only the
                                    // unmark action lives here.
                                    if (issue.duplicateOfId != null) {
                                        DropdownMenuItem(
                                            leadingIcon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
                                            text = { Text("Unmark duplicate") },
                                            onClick = {
                                                overflowOpen = false
                                                viewModel.unmarkDuplicate()
                                            },
                                        )
                                    }
                                    // Move to another project in the same workspace
                                    // (EXP-57) — hidden when this is the workspace's
                                    // only project (web parity: 2+ projects).
                                    if (moveTargets.isNotEmpty()) {
                                        DropdownMenuItem(
                                            leadingIcon = { Icon(Icons.AutoMirrored.Filled.DriveFileMove, contentDescription = null) },
                                            text = { Text("Move to project") },
                                            onClick = {
                                                overflowOpen = false
                                                movePickerOpen = true
                                            },
                                        )
                                    }
                                    DropdownMenuItem(
                                        leadingIcon = { Icon(Icons.Filled.DeleteOutline, contentDescription = null) },
                                        text = { Text("Delete issue") },
                                        onClick = {
                                            overflowOpen = false
                                            confirmDelete = true
                                        },
                                    )
                                }
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = Color.Transparent,
    ) { padding ->
        if (issue == null) {
            Column(
                modifier = Modifier.padding(padding).fillMaxSize(),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text("Loading…", color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary))
            }
            return@Scaffold
        }

        val status = IssueStatus.fromWire(issue.status)
        val priority = IssuePriority.fromWire(issue.priority)

        Column(
            modifier = Modifier
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 8.dp)
                .fillMaxWidth(),
        ) {
            SyncBannerRow(syncBanner)
            if (syncBanner != SyncBanner.None) Spacer(Modifier.height(8.dp))
            // Header: identifier chip + repo chip (actions live in the nav bar).
            Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    issue.identifier,
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .glassButton()
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
                // The backing repo (name resolved via the repositories API, cached)
                // — a project is a repository now (masterplan v4 §6).
                repoName?.let { name ->
                    Spacer(Modifier.width(6.dp))
                    RepoChip(name)
                }
            }

            // Canonical-issue banner (masterplan §5e): "Duplicate of {IDENTIFIER}"
            // with a clickable pill through to the canonical issue + Unmark.
            if (issue.duplicateOfId != null) {
                Spacer(Modifier.height(8.dp))
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        Icons.Filled.ContentCopy,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        "Duplicate of",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Spacer(Modifier.width(6.dp))
                    val canonical = duplicateOf
                    if (canonical != null) {
                        Text(
                            canonical.identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier
                                .glassButton()
                                .clickable { onOpenIssue(canonical.id) }
                                .padding(horizontal = 8.dp, vertical = 4.dp),
                        )
                    } else {
                        Text(
                            "another issue",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    Spacer(Modifier.weight(1f))
                    if (isModerator) {
                        Text(
                            "Unmark",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier
                                .glassButton()
                                .clickable { viewModel.unmarkDuplicate() }
                                .padding(horizontal = 10.dp, vertical = 4.dp),
                        )
                    }
                }
            }

            Spacer(Modifier.height(8.dp))
            // Large title (borderless, save on focus-loss)
            BasicTextField(
                value = titleField,
                onValueChange = { titleField = it },
                readOnly = !isModerator,
                textStyle = MaterialTheme.typography.headlineSmall.copy(
                    color = MaterialTheme.colorScheme.onSurface,
                ),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                modifier = Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        if (isModerator && !focus.isFocused && titleField.isNotBlank() && titleField != issue.title) {
                            viewModel.updateTitle(titleField)
                        }
                    },
                decorationBox = { inner ->
                    if (titleField.isEmpty()) {
                        Text(
                            "Title",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    inner()
                },
            )

            Spacer(Modifier.height(16.dp))
            val mentionMembers = remember(state.users) {
                state.users
                    .filter { !it.isAgent }
                    .map { MentionMember(it.name ?: it.email, it.email) }
            }
            MarkdownEditor(
                markdown = descriptionField,
                editable = isModerator,
                onChange = {
                    descriptionField = it
                    viewModel.updateDescription(it)
                },
                onUploadImage = if (isModerator) { uri -> viewModel.uploadImage(uri) } else null,
                imageUploadEnabled = isModerator,
                mentionMembers = mentionMembers,
            )
            DisposableEffect(Unit) {
                onDispose { viewModel.flushDescription() }
            }

            Spacer(Modifier.height(20.dp))
            // Metadata/property cards + labels (extracted to IssueMetadataEditor).
            IssueMetadataEditor(
                issue = issue,
                status = status,
                priority = priority,
                assignee = state.assignee,
                workspaceLabels = state.workspaceLabels,
                issueLabels = state.issueLabels,
                isModerator = isModerator,
                hideAssignee = soloMemberId != null,
                releaseName = currentRelease?.name,
                onStatusClick = { statusMenuOpen = true },
                onPriorityClick = { priorityMenuOpen = true },
                onAssigneeClick = { assigneeMenuOpen = true },
                onReleaseClick = { releaseMenuOpen = true },
                onDueDateClick = { datePickerOpen = true },
                onClearDueDate = { viewModel.updateDueDate(null) },
                onStartTimeClick = { dueTimePickerOpen = true },
                onEndTimeClick = { endTimePickerOpen = true },
                onRepeatClick = { recurrenceSheetOpen = true },
                onToggleLabel = { id, assigned -> viewModel.toggleLabel(id, assigned) },
                onAddLabel = { labelsOpen = true },
            )

            // Steer panel (masterplan §5b/§5c): live "Coding now" badge + Watch
            // live when a session is running; "Start on my desktop" otherwise.
            if (runningSession != null || (steerEnabled == true && permissions.isMember && !steerDevices.isNullOrEmpty())) {
                Spacer(Modifier.height(20.dp))
                repoName?.let { name ->
                    Row(modifier = Modifier.padding(bottom = 8.dp)) { RepoChip(name) }
                }
                SteerPanel(
                    session = runningSession,
                    sessionOwner = runningSession?.let { s -> state.users.firstOrNull { it.id == s.userId } },
                    steerEnabled = steerEnabled,
                    isMember = permissions.isMember,
                    devices = steerDevices,
                    startState = startState,
                    onStart = viewModel::startOnDesktop,
                    onWatch = onOpenSteer,
                )
            }

            // Changes before attachments (iOS order — masterplan §4.8, mobile
            // tiers 2–4): PR/branch summary linking to the dedicated diff page
            // → "being coded on <device>" opening the native agent viewer.
            if (!issue.prUrl.isNullOrBlank() || !issue.branch.isNullOrBlank() || runningSession != null) {
                Spacer(Modifier.height(20.dp))
                ChangesSection(
                    prUrl = issue.prUrl,
                    branch = issue.branch,
                    runningSessionId = runningSession?.id,
                    runningSessionDeviceLabel = runningSession?.deviceLabel,
                    steerEnabled = steerEnabled == true,
                    isMember = permissions.isMember,
                    loadBranchDiff = { viewModel.loadBranchDiff() },
                    onOpenChanges = onOpenChanges,
                    onWatch = onOpenSteer,
                )
            }

            Spacer(Modifier.height(20.dp))
            AttachmentList(issueId = issue.id)

            Spacer(Modifier.height(8.dp))
            CommentThread(issueId = issue.id)
        }
    }
    }
    }

    if (statusMenuOpen && issue != null && isModerator) {
        val currentStatus = IssueStatus.fromWire(issue.status)
        IssuePickerSheet(
            title = "Status",
            items = issueStatusOrder,
            selected = currentStatus,
            labelOf = { it.label },
            iconOf = { statusIcon(it) },
            onSelect = {
                // Duplicate = status interception (L27): picking `duplicate`
                // opens the canonical-issue picker instead of writing the status
                // directly; markDuplicate sets duplicateOfId + status='duplicate'
                // atomically. Cancelling the picker leaves the status untouched.
                if (it == IssueStatus.Duplicate) {
                    duplicatePickerOpen = true
                } else {
                    viewModel.updateStatus(it)
                }
            },
            onDismiss = { statusMenuOpen = false },
        )
    }

    if (priorityMenuOpen && issue != null && isModerator) {
        val currentPriority = IssuePriority.fromWire(issue.priority)
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = currentPriority,
            labelOf = { it.label },
            iconOf = { priorityIcon(it) },
            onSelect = { viewModel.updatePriority(it) },
            onDismiss = { priorityMenuOpen = false },
        )
    }

    if (assigneeMenuOpen && isModerator) {
        // Only real people are assignable; the widget helpdesk bot (isAgent) is excluded.
        val people = state.users.filter { !it.isAgent }
        val assigneeItems: List<com.exponential.app.data.db.UserEntity?> =
            listOf<com.exponential.app.data.db.UserEntity?>(null) + people
        IssuePickerSheet(
            title = "Assignee",
            items = assigneeItems,
            selected = assigneeItems.firstOrNull { it?.id == state.assignee?.id },
            keyOf = { it?.id ?: "__unassigned__" },
            labelOf = { user -> user?.let { it.name ?: it.email } ?: "Unassigned" },
            iconOf = { user -> if (user == null) Icons.Filled.PersonOff else Icons.Filled.Person },
            onSelect = { viewModel.updateAssignee(it?.id) },
            onDismiss = { assigneeMenuOpen = false },
        )
    }

    // Plain membership, not isModerator: v7 collapses moderation to membership
    // (matches web's isModerator = isMember and iOS's ungated picker).
    if (releaseMenuOpen && permissions.isMember) {
        // Single-select release picker (EXP-56): "No release" clears, picking
        // the current one is a no-op server-side.
        val releaseItems: List<com.exponential.app.data.db.ReleaseEntity?> =
            listOf<com.exponential.app.data.db.ReleaseEntity?>(null) + workspaceReleases
        IssuePickerSheet(
            title = "Release",
            items = releaseItems,
            selected = releaseItems.firstOrNull { it?.id == currentRelease?.id },
            keyOf = { it?.id ?: "__no_release__" },
            labelOf = { release ->
                when {
                    release == null -> "No release"
                    release.shippedAt != null -> "${release.name} (shipped)"
                    else -> release.name
                }
            },
            iconOf = { release -> if (release == null) Icons.Filled.Close else Icons.Filled.RocketLaunch },
            onSelect = { viewModel.setRelease(it?.id) },
            onDismiss = { releaseMenuOpen = false },
        )
    }

    if (datePickerOpen) {
        IssueDatePickerDialog(
            initialDate = issue?.dueDate,
            onConfirm = { viewModel.updateDueDate(it); datePickerOpen = false },
            onDismiss = { datePickerOpen = false },
        )
    }

    if (labelsOpen) {
        LabelPickerSheet(
            workspaceLabels = state.workspaceLabels,
            selectedLabelIds = state.issueLabels.map { it.id }.toSet(),
            onToggle = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onCreate = { name, color -> viewModel.createAndAssignLabel(name, color) },
            onDismiss = { labelsOpen = false },
        )
    }

    if (dueTimePickerOpen && issue != null) {
        IssueTimePickerDialog(
            initialTime = issue.dueTime,
            title = "Start time",
            onConfirm = { viewModel.updateDueTime(it); dueTimePickerOpen = false },
            onClear = { viewModel.updateDueTime(null); dueTimePickerOpen = false },
            onDismiss = { dueTimePickerOpen = false },
        )
    }

    if (endTimePickerOpen && issue != null) {
        IssueTimePickerDialog(
            initialTime = issue.endTime,
            title = "End time",
            onConfirm = { viewModel.updateEndTime(it); endTimePickerOpen = false },
            onClear = { viewModel.updateEndTime(null); endTimePickerOpen = false },
            onDismiss = { endTimePickerOpen = false },
        )
    }

    if (recurrenceSheetOpen && issue != null) {
        RecurrenceSheet(
            interval = issue.recurrenceInterval,
            unit = issue.recurrenceUnit,
            onApply = { i, u -> viewModel.updateRecurrence(i, u); recurrenceSheetOpen = false },
            onDismiss = { recurrenceSheetOpen = false },
        )
    }

    if (duplicatePickerOpen && issue != null && isModerator) {
        DuplicatePickerSheet(
            candidates = duplicateCandidates,
            onPick = { viewModel.markDuplicate(it.id) },
            onDismiss = { duplicatePickerOpen = false },
        )
    }

    // Move to project (EXP-57): pick a same-workspace target, then confirm —
    // the move renumbers the issue (new identifier), so it's consequential.
    if (movePickerOpen && issue != null && isModerator) {
        IssuePickerSheet(
            title = "Move to project",
            items = moveTargets,
            selected = null,
            keyOf = { it.id },
            labelOf = { it.name },
            iconOf = { Icons.Filled.Folder },
            onSelect = { moveTarget = it },
            onDismiss = { movePickerOpen = false },
        )
    }

    val pendingMoveTarget = moveTarget
    if (pendingMoveTarget != null && issue != null) {
        AlertDialog(
            onDismissRequest = { moveTarget = null },
            title = { Text("Move issue") },
            text = {
                Text(
                    "Move ${issue.identifier} to \"${pendingMoveTarget.name}\"? " +
                        "The issue will get a new identifier in that project.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    moveTarget = null
                    viewModel.moveToProject(pendingMoveTarget.id)
                }) {
                    Text("Move")
                }
            },
            dismissButton = {
                TextButton(onClick = { moveTarget = null }) { Text("Cancel") }
            },
        )
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete issue") },
            text = { Text("This action cannot be undone.") },
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

// The backing repository's name (owner/name), resolved via the repositories API
// and cached in the ViewModel. A project is a repository now (masterplan v4 §6).
@Composable
private fun RepoChip(fullName: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .glassButton()
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Icon(
            Icons.Filled.Code,
            contentDescription = null,
            modifier = Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(5.dp))
        Text(
            fullName,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}
