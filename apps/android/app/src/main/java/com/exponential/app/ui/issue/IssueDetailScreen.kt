package com.exponential.app.ui.issue

import android.content.Intent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.material.icons.filled.Feedback
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.NotificationsOff
import androidx.compose.material.icons.filled.Refresh
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.markdown.stripDraftImages
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection
import kotlinx.coroutines.launch

// The per-property/combined sheets the detail screen can present (EXP-240).
// One nullable slot: children opened from the Properties sheet stack over it
// (propertiesOpen stays true beneath).
private enum class IssueSheet { Status, Priority, Assignee, Labels, DueDate, Duplicate, MoveBoard, StartCoding }

// Linear-mobile-style issue detail (EXP-240): centered "Issue" nav title,
// identifier chip + overflow header row, large editable title, the property
// chip box, the description editor, the agent/PR card, and the activity
// timeline — with a floating three-element bottom bar (properties circle,
// expanding comment pill, start-coding circle).
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueDetailScreen(
    issueId: String,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit = {},
    onOpenSteer: (String) -> Unit = {},
    onOpenChanges: () -> Unit = {},
    viewModel: IssueDetailViewModel = hiltViewModel(),
    commentViewModel: CommentThreadViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val isSubscribed by viewModel.isSubscribed.collectAsStateWithLifecycle()
    val runningSession by viewModel.runningSession.collectAsStateWithLifecycle()
    val repoName by viewModel.repoName.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val steerDevices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val duplicateOf by viewModel.duplicateOf.collectAsStateWithLifecycle()
    val duplicateCandidates by viewModel.duplicateCandidates.collectAsStateWithLifecycle()
    val shareUrl by viewModel.shareUrl.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    // EXP-50: solo teams (one human member) hide the assignee chip/row.
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    // EXP-57: same-team boards the issue can move to.
    val moveTargets by viewModel.moveTargets.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val issue = state.issue
    // Remote-reconciled title/description: seed on first load, live-apply a remote
    // edit while clean, stash + banner while dirty (field-level last-write-wins).
    // remember(issue?.id) gives the per-issue reset the old seed effect provided.
    val titleSync = remember(issue?.id) { RemoteSyncedText(normalizeForEcho = { it.trim() }) }
    val descriptionSync = remember(issue?.id) { RemoteSyncedText(normalizeForEcho = ::stripDraftImages) }
    var propertiesOpen by remember { mutableStateOf(false) }
    var activeSheet by remember { mutableStateOf<IssueSheet?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }
    var overflowOpen by remember { mutableStateOf(false) }
    // The picked target board, pending the move confirmation (EXP-57).
    var moveTarget by remember { mutableStateOf<com.exponential.app.data.db.BoardEntity?>(null) }
    // The docked comment composer (bottom bar) expansion.
    var composerExpanded by remember { mutableStateOf(false) }

    // The bar's comment half shares the thread's screen-scoped VM (hoisted
    // draft) — bind before either consumer renders.
    LaunchedEffect(issueId) { commentViewModel.bind(issueId) }
    val commentDraft by commentViewModel.draft.collectAsStateWithLifecycle()
    val commentSending by commentViewModel.sending.collectAsStateWithLifecycle()

    LaunchedEffect(titleSync, issue?.title) {
        issue?.title?.let { titleSync.syncRemote(it) }
    }
    val remoteDescription = issue?.let { extractDescriptionMarkdown(it.description) }
    LaunchedEffect(descriptionSync, remoteDescription) {
        remoteDescription?.let { descriptionSync.syncRemote(it) }
        // A clean live-apply supersedes any not-yet-flushed local input: without
        // this, the dispose-time flush would re-save text the user no longer sees.
        if (remoteDescription != null && !descriptionSync.isDirty) viewModel.discardPendingDescription()
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

    // Remote-start feedback (EXP-240 — the inline captions left with the card's
    // start strip): failures surface as a snackbar; a batch send points at the
    // Agents tab (the single-issue send keeps spinning in the start circle
    // until the session row syncs in).
    LaunchedEffect(startState) {
        when (val s = startState) {
            is SteerStartState.Failed -> snackbarHostState.showSnackbar(s.message)
            is SteerStartState.Sent ->
                if (s.isBatch) {
                    snackbarHostState.showSnackbar(
                        "Batch start sent to ${s.deviceLabel} — follow it in the Agents tab.",
                    )
                }
            else -> Unit
        }
    }

    // Inline `#IDENTIFIER` pills + editor #-autocomplete (masterplan §5e):
    // resolve against this team's synced issues; a tap navigates to the
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
                                    // Move to another board in the same team
                                    // (EXP-57) — hidden when this is the team's
                                    // only board (web parity: 2+ boards).
                                    if (moveTargets.isNotEmpty()) {
                                        DropdownMenuItem(
                                            leadingIcon = { Icon(Icons.AutoMirrored.Filled.DriveFileMove, contentDescription = null) },
                                            text = { Text("Move to board") },
                                            onClick = {
                                                overflowOpen = false
                                                activeSheet = IssueSheet.MoveBoard
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
        val mentionMembers = remember(state.users) {
            state.users
                .map { MentionMember(it.name ?: it.email, it.email) }
        }

        // Start-circle gating + content (EXP-240): hidden without steer /
        // membership / a repo-backed board, and until the device list has
        // loaded (null = myDevices in flight — no premature dimmed circle); a
        // live session shows its state dot; an in-flight send spins; otherwise
        // the play glyph (dimmed while no desktop is online — tapping then
        // explains via snackbar).
        val session = runningSession
        val devices = steerDevices
        val startAllowed = steerEnabled == true && permissions.isMember && state.board?.repositoryId != null
        val startUi: StartButtonUi? = when {
            !startAllowed -> null
            session != null -> StartButtonUi.Session(codingSessionDisplayState(session, issue.prState))
            startState is SteerStartState.Sending || startState is SteerStartState.Sent -> StartButtonUi.Sending
            devices == null -> null
            else -> StartButtonUi.Start(enabled = devices.isNotEmpty())
        }

        // The bar yields to the title/description keyboard (the markdown
        // toolbar owns that space); its own composer keeps it visible.
        val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
        val barVisible = composerExpanded || !imeVisible

        Box(
            modifier = Modifier
                .padding(padding)
                // Shrink the scrollport above the keyboard: with edge-to-edge,
                // adjustResize alone never resizes the window, so without this
                // the focused editor line stays hidden behind the IME (EXP-135).
                // consumeWindowInsets keeps imePadding from re-adding the
                // nav-bar inset already applied by the Scaffold padding.
                .consumeWindowInsets(padding)
                .fillMaxSize(),
        ) {
            Column(
                modifier = Modifier
                    .imePadding()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp, vertical = 8.dp)
                    .fillMaxWidth(),
            ) {
            SyncBannerRow(syncBanner)
            if (syncBanner != SyncBanner.None) Spacer(Modifier.height(8.dp))
            // Header: identifier chip (actions live in the nav bar). The repo
            // chip renders once, above the agent/PR card (EXP-170).
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    issue.identifier,
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .glassButton()
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
                // Origin chip: issues filed via the feedback widget carry
                // source == "widget" (no user creator). Read-only indicator.
                if (issue.source == DomainContract.issueSourceWidget) {
                    FeedbackWidgetChip()
                }
            }

            // Conflict affordance: a remote edit to the title or description
            // arrived while that field was dirty/focused, so it was stashed rather
            // than clobbering the local edit. Tapping discards local text for the
            // remote value (until then it's last-write-wins — the local save
            // still overwrites the remote, matching iOS).
            if (titleSync.pendingRemote != null || descriptionSync.pendingRemote != null) {
                Spacer(Modifier.height(8.dp))
                RemoteEditBanner(onReload = {
                    titleSync.reloadPending()
                    if (descriptionSync.reloadPending()) viewModel.discardPendingDescription()
                })
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
                value = titleSync.text,
                onValueChange = { titleSync.onUserEdit(it) },
                readOnly = !isModerator,
                textStyle = MaterialTheme.typography.headlineSmall.copy(
                    color = MaterialTheme.colorScheme.onSurface,
                ),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                modifier = Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        titleSync.setFocused(focus.isFocused)
                        // Dirty is measured against the seed BASELINE, not the live
                        // row: a remote rename the user never touched leaves the
                        // field clean, so blur fires no save and the rename stands.
                        if (isModerator && !focus.isFocused && titleSync.text.isNotBlank() && titleSync.isDirty) {
                            viewModel.updateTitle(titleSync.text)
                        }
                    },
                decorationBox = { inner ->
                    if (titleSync.text.isEmpty()) {
                        Text(
                            "Title",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    inner()
                },
            )

            Spacer(Modifier.height(12.dp))
            // The top property chip box (EXP-240) — replaces the stacked
            // property/times cards + labels section.
            IssuePropertyChips(
                issue = issue,
                status = status,
                priority = priority,
                assignee = state.assignee,
                issueLabels = state.issueLabels,
                isModerator = isModerator,
                hideAssignee = soloMemberId != null,
                onOpenStatus = { activeSheet = IssueSheet.Status },
                onOpenPriority = { activeSheet = IssueSheet.Priority },
                onOpenAssignee = { activeSheet = IssueSheet.Assignee },
                onOpenDueDate = { activeSheet = IssueSheet.DueDate },
                onOpenLabels = { activeSheet = IssueSheet.Labels },
                onOpenProperties = { propertiesOpen = true },
            )

            Spacer(Modifier.height(16.dp))
            MarkdownEditor(
                markdown = descriptionSync.text,
                editable = isModerator,
                onChange = {
                    descriptionSync.onUserEdit(it)
                    viewModel.updateDescription(it)
                },
                onUploadImage = if (isModerator) { uri -> viewModel.uploadImage(uri) } else null,
                imageUploadEnabled = isModerator,
                mentionMembers = mentionMembers,
                onFocusChanged = { descriptionSync.setFocused(it) },
            )
            DisposableEffect(Unit) {
                onDispose { viewModel.flushDescription() }
            }

            // The agent/PR card (EXP-156): a live "Coding now" session and the
            // PR/branch chips linking to the dedicated Changes page. Start
            // moved to the bottom bar (EXP-240), so this renders only with a
            // session, a PR, or a pushed branch.
            val cardVisible = session != null ||
                !issue.prUrl.isNullOrBlank() ||
                !issue.branch.isNullOrBlank()
            if (cardVisible) {
                Spacer(Modifier.height(20.dp))
                repoName?.let { name ->
                    Row(modifier = Modifier.padding(bottom = 8.dp)) { RepoChip(name) }
                }
                AgentPrCard(
                    issue = issue,
                    session = session,
                    sessionOwner = session?.let { s -> state.users.firstOrNull { it.id == s.userId } },
                    steerEnabled = steerEnabled,
                    isMember = permissions.isMember,
                    onWatch = onOpenSteer,
                    onOpenChanges = onOpenChanges,
                )
            }

            Spacer(Modifier.height(20.dp))
            CommentThread(issueId = issue.id, viewModel = commentViewModel)

            // Clearance so the last timeline row scrolls out from under the
            // floating bar (kept in sync with the nav pill inset, EXP-36).
            Spacer(Modifier.height(BottomBarInset))
            }

            // The floating bottom bar / docked composer. Lives INSIDE the
            // ProvideMarkdownToolbar content (which bottom-insets by the
            // toolbar height), with a single imePadding — so the stack is
            // IME → markdown toolbar → composer.
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .imePadding(),
            ) {
                AnimatedVisibility(
                    visible = barVisible,
                    enter = fadeIn(tween(180)),
                    exit = fadeOut(tween(180)),
                ) {
                    IssueDetailBottomBar(
                        expanded = composerExpanded,
                        onExpandedChange = { composerExpanded = it },
                        showProperties = isModerator,
                        onOpenProperties = { propertiesOpen = true },
                        startButton = startUi,
                        onStartClick = {
                            when {
                                session != null -> onOpenSteer(session.id)
                                startState is SteerStartState.Sending || startState is SteerStartState.Sent -> Unit
                                steerDevices.isNullOrEmpty() -> scope.launch {
                                    snackbarHostState.showSnackbar(
                                        "No desktop online — open the Exponential desktop app to run here.",
                                    )
                                }
                                else -> activeSheet = IssueSheet.StartCoding
                            }
                        },
                        draft = commentDraft,
                        onDraftChange = commentViewModel::updateDraft,
                        sending = commentSending,
                        onSend = { commentViewModel.send { composerExpanded = false } },
                        onUploadImage = { uri -> commentViewModel.uploadImage(uri) },
                        mentionMembers = mentionMembers,
                    )
                }
            }
        }
    }
    }
    }

    // ── Sheets ────────────────────────────────────────────────────────────────

    if (propertiesOpen && issue != null && isModerator) {
        PropertiesSheet(
            issue = issue,
            status = IssueStatus.fromWire(issue.status),
            priority = IssuePriority.fromWire(issue.priority),
            assignee = state.assignee,
            hideAssignee = soloMemberId != null,
            issueLabels = state.issueLabels,
            currentBoardName = state.board?.name,
            hasMoveTargets = moveTargets.isNotEmpty(),
            onOpenStatus = { activeSheet = IssueSheet.Status },
            onOpenPriority = { activeSheet = IssueSheet.Priority },
            onOpenAssignee = { activeSheet = IssueSheet.Assignee },
            onOpenDueDate = { activeSheet = IssueSheet.DueDate },
            onOpenLabels = { activeSheet = IssueSheet.Labels },
            onOpenMoveBoard = { activeSheet = IssueSheet.MoveBoard },
            onToggleLabel = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onDismiss = { propertiesOpen = false },
        )
    }

    if (activeSheet == IssueSheet.Status && issue != null && isModerator) {
        val currentStatus = IssueStatus.fromWire(issue.status)
        IssuePickerSheet(
            title = "Status",
            items = issueStatusOrder,
            selected = currentStatus,
            labelOf = { it.label },
            leadingContent = { StatusIcon(it, size = 16.dp) },
            onSelect = {
                // Duplicate = status interception (L27): picking `duplicate`
                // opens the canonical-issue picker instead of writing the status
                // directly; markDuplicate sets duplicateOfId + status='duplicate'
                // atomically. Cancelling the picker leaves the status untouched.
                if (it == IssueStatus.Duplicate) {
                    activeSheet = IssueSheet.Duplicate
                } else {
                    viewModel.updateStatus(it)
                }
            },
            onDismiss = { if (activeSheet == IssueSheet.Status) activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Priority && issue != null && isModerator) {
        val currentPriority = IssuePriority.fromWire(issue.priority)
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = currentPriority,
            labelOf = { it.label },
            leadingContent = { PriorityIcon(it, size = 16.dp) },
            onSelect = { viewModel.updatePriority(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Assignee && issue != null && isModerator) {
        AssigneePickerSheet(
            users = state.users,
            selectedUserId = issue.assigneeId,
            onSelect = { viewModel.updateAssignee(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.DueDate && issue != null && isModerator) {
        DueDateSheet(
            dueDate = issue.dueDate,
            dueTime = issue.dueTime,
            endTime = issue.endTime,
            onSetDate = { viewModel.updateDueDate(it) },
            onSetDueTime = { viewModel.updateDueTime(it) },
            onSetEndTime = { viewModel.updateEndTime(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Labels && issue != null && isModerator) {
        LabelPickerSheet(
            teamLabels = state.teamLabels,
            selectedLabelIds = state.issueLabels.map { it.id }.toSet(),
            onToggle = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onCreate = { name, color -> viewModel.createAndAssignLabel(name, color) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Duplicate && issue != null && isModerator) {
        DuplicatePickerSheet(
            candidates = duplicateCandidates,
            onPick = { viewModel.markDuplicate(it.id) },
            onDismiss = { activeSheet = null },
        )
    }

    // Move to board (EXP-57): pick a same-team target, then confirm —
    // the move renumbers the issue (new identifier), so it's consequential.
    if (activeSheet == IssueSheet.MoveBoard && issue != null && isModerator) {
        IssuePickerSheet(
            title = "Move to board",
            items = moveTargets,
            selected = null,
            keyOf = { it.id },
            labelOf = { it.name },
            iconOf = { Icons.Filled.Folder },
            onSelect = { moveTarget = it },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.StartCoding && issue != null) {
        StartCodingSheet(
            devices = steerDevices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = setOf(issue.id),
            onStart = viewModel::startOnDesktop,
            onDismiss = { activeSheet = null },
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
                        "The issue will get a new identifier in that board.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    moveTarget = null
                    viewModel.moveToBoard(pendingMoveTarget.id)
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

// Non-blocking conflict banner: a teammate changed the title or description
// while this field was being edited, so the remote value was stashed. Tapping
// discards the local edit and loads the remote value. Matches the SyncBannerRow
// glass-row idiom.
@Composable
private fun RemoteEditBanner(onReload: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable { onReload() }
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            Icons.Filled.Refresh,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Text(
            "Updated by someone else — tap to reload",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

// Origin pill for widget-filed issues (source == "widget"): a muted, read-only
// "Feedback widget" indicator matching the RepoChip glass idiom.
@Composable
private fun FeedbackWidgetChip() {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .glassButton()
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Icon(
            Icons.Filled.Feedback,
            contentDescription = null,
            modifier = Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(5.dp))
        Text(
            "Feedback widget",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

// The backing repository's name (owner/name), resolved via the repositories API
// and cached in the ViewModel. A board is a repository now (masterplan v4 §6).
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
