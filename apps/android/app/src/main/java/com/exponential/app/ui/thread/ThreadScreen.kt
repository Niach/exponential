package com.exponential.app.ui.thread

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.GuideSectionKey
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.guideSectionPage
import com.exponential.app.domain.parseSessionResultGroups
import com.exponential.app.ui.agent.ComposerPick
import com.exponential.app.ui.agent.readComposerPick
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.failedAction
import com.exponential.app.ui.components.TabPager
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.IssueSearchSheet
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.ChangesLoadState
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.ChangesViewModel
import com.exponential.app.ui.work.GuideFace
import com.exponential.app.ui.work.GithubHeaderAction
import com.exponential.app.ui.work.GuideSectionDiff
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.PullRequestStack
import com.exponential.app.ui.work.TitleCollapseState
import dev.chrisbanes.haze.rememberHazeState
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.launch

/**
 * EXP-1296 Threads v2 (Model v3): THE conversation screen for a thread root
 * OR an agent root (`thread/{sessionId}`), `session-thread.json`.
 *
 * A thread: the title (long-press renames), then ONE tab strip = the phone
 * pager — Conversation · one chip per linked issue · "+" (link / file an
 * issue) · Guide. Conversation = the root's rows (THE session card per
 * session, the person's bubbles, the agent's prose, question / plan cards
 * answered in place, subagent / picture / event rows) over THE composer box;
 * an issue page = that issue's own Issue face; Guide = the root's Guide with
 * the Merge capsule of its PR. An AGENT channel is the same screen with the
 * agent's icon + name (settings affordance) and the conversation only, its
 * messages as comment rows. Stop / End live in the session view.
 */
@Composable
fun ThreadScreen(
    onBack: () -> Unit,
    onOpenSession: (sessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenThread: (rootSessionId: String) -> Unit,
    onOpenAgentSettings: (agentSessionId: String) -> Unit,
    /** The "+"'s New issue: the New issue page on [boardId], linked to this
     *  root on create. Null hides the row. */
    onNewIssue: ((boardId: String, rootSessionId: String) -> Unit)? = null,
    viewModel: ThreadViewModel = hiltViewModel(),
) {
    val view by viewModel.view.collectAsStateWithLifecycle()
    val root by viewModel.root.collectAsStateWithLifecycle()
    val isAgent by viewModel.isAgent.collectAsStateWithLifecycle()
    val messages by viewModel.messages.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val me by viewModel.currentUserId.collectAsStateWithLifecycle()
    val statuses by viewModel.statuses.collectAsStateWithLifecycle()
    val issues by viewModel.issues.collectAsStateWithLifecycle()
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val images by viewModel.images.collectAsStateWithLifecycle()
    val imageError by viewModel.imageError.collectAsStateWithLifecycle()
    val linkCandidates by viewModel.linkCandidates.collectAsStateWithLifecycle()
    val newIssueBoardId by viewModel.newIssueBoardId.collectAsStateWithLifecycle()
    val pullRequests by viewModel.pullRequests.collectAsStateWithLifecycle()
    val guide by viewModel.guide.collectAsStateWithLifecycle()
    val teamPullRequests by viewModel.teamPullRequests.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    var selectedKey by rememberSaveable { mutableStateOf(viewModel.initialTab ?: "conversation") }
    var draft by rememberSaveable { mutableStateOf("") }
    var menuFor by remember { mutableStateOf<ThreadMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var renameOpen by remember { mutableStateOf(false) }

    // The composer's "+": the system document picker (any type), one pick at
    // a time through the VM so the steer caps apply (the Agent page's rule).
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris: List<Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            for (uri in uris) when (val picked = readComposerPick(context, uri)) {
                is ComposerPick.Read -> viewModel.addImage(picked.uri, picked.bytes, picked.filename, picked.mime)
                is ComposerPick.Refused -> viewModel.refuseAttachment(picked.message)
            }
        }
    }

    val statusById = remember(issues, statuses) {
        issues.associate { it.id to IssueStatusResolver.resolve(it, statuses) }
    }
    val messagesById = remember(messages) { messages.associateBy { it.id } }
    val metaFor: (SessionTree.ThreadRow.MessageRow) -> ThreadRowMeta? = { row ->
        val user = row.authorId?.let { users[it] }
        ThreadRowMeta(
            authorName = if (row.author == "user") userDisplayName(user, row.authorId) else "",
            time = messagesById[row.id]?.createdAt?.let(::clockTime).orEmpty(),
            user = user,
        )
    }
    val rootRow = root
    val agent = if (isAgent && rootRow != null) {
        ThreadAgent(
            name = rootRow.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS,
            icon = rootRow.icon,
            color = rootRow.color,
        )
    } else {
        null
    }
    val placeholder = agent?.let { ThreadUiCopy.messageAgent(it.name) }
        ?: view?.composer?.placeholder
        ?: SessionTree.ThreadCopy.PLACEHOLDER

    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
        ) {
            ThreadContent(
                view = view,
                agent = agent,
                selectedKey = selectedKey,
                onSelectKey = { selectedKey = it },
                issueStatus = { statusById[it] },
                meta = metaFor,
                onOpenCard = { card ->
                    // A `thread_started` card names ANOTHER root: open that
                    // thread; every other card opens its raw session view.
                    if (viewModel.isOtherRoot(card.sessionId)) onOpenThread(card.sessionId) else onOpenSession(card.sessionId)
                },
                onAnswer = viewModel::answer,
                onDecidePlan = { id, approved -> viewModel.decidePlan(id, approved) },
                canUnqueue = { row -> row.authorId != null && row.authorId == me },
                onUnqueue = viewModel::unqueue,
                onBack = onBack,
                onRename = { renameOpen = true },
                onAdd = { menuFor = ThreadMenuTarget(null) },
                onIssueMenu = { menuFor = ThreadMenuTarget(it) },
                onAgentSettings = { onOpenAgentSettings(viewModel.rootId) },
                composer = { modifier ->
                    ThreadComposerBox(
                        value = draft,
                        onValueChange = { draft = it },
                        placeholder = placeholder,
                        pendingImages = images,
                        imageError = imageError,
                        sending = sending,
                        onAdd = { filePicker.launch(arrayOf("*/*")) },
                        onRemoveImage = viewModel::removeImage,
                        onSend = { viewModel.sendDraft(draft) { draft = "" } },
                        modifier = modifier,
                    )
                },
                issuePage = { issueId, padding ->
                    ThreadIssuePage(
                        issueId = issueId,
                        padding = padding,
                        onBack = onBack,
                        onOpenIssue = { id ->
                            if (statusById.containsKey(id)) selectedKey = "issue:$id" else onOpenIssue(id)
                        },
                        onOpenThread = { id -> if (id != viewModel.rootId) onOpenThread(id) },
                    )
                },
                guidePage = { padding ->
                    ThreadGuidePage(
                        guideRaw = guide,
                        pullRequests = pullRequests,
                        teamPullRequests = teamPullRequests,
                        padding = padding,
                        onFixConflicts = { number -> viewModel.send(fixConflictsMessage(number)) },
                    )
                },
                guidePrUrl = guidePullRequest(pullRequests)?.url?.takeIf { it.isNotBlank() },
            )
        }
    }

    menuFor?.let { target ->
        ThreadMenuSheet(
            issue = target.issue,
            canFile = onNewIssue != null && newIssueBoardId != null,
            onLink = {
                menuFor = null
                linkPickerOpen = true
            },
            onNewIssue = {
                menuFor = null
                val board = newIssueBoardId
                if (board != null) onNewIssue?.invoke(board, viewModel.rootId)
            },
            onUnlink = { issue ->
                menuFor = null
                if (selectedKey == "issue:${issue.issueId}") selectedKey = "conversation"
                viewModel.unlinkIssue(issue.issueId)
            },
            onDismiss = { menuFor = null },
        )
    }
    if (linkPickerOpen) {
        val linked = issues.map { it.id }.toSet()
        val handler = remember(linkCandidates, linked) {
            IssueRefHandler(
                candidates = linkCandidates.filter { it.issueId !in linked },
                searchServer = viewModel::searchIssues,
            ) { }
        }
        IssueSearchSheet(
            handler = handler,
            initialQuery = "",
            onPick = { target ->
                linkPickerOpen = false
                viewModel.linkIssues(listOf(target.issueId))
            },
            onDismiss = { linkPickerOpen = false },
        )
    }
    if (renameOpen) {
        ThreadRenameSheet(
            initial = if (view?.titleDerived == false) view?.title.orEmpty() else "",
            placeholder = view?.title.orEmpty(),
            onSave = { title ->
                renameOpen = false
                viewModel.setTitle(title.trim().ifEmpty { null })
            },
            onDismiss = { renameOpen = false },
        )
    }
}

/** A wire timestamp as the local `HH:mm` a message row shows. */
internal fun clockTime(wire: String): String? {
    val ms = WireTimestamps.parseEpochMs(wire) ?: return null
    return Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).format(DateTimeFormatter.ofPattern("HH:mm"))
}

/** The message a refused (conflicting) merge offers to send to the root. */
internal fun fixConflictsMessage(number: Int?): String =
    "Rebase ${number?.let { "PR #$it" } ?: "the pull request"} onto its base, resolve the merge conflicts and push."

/**
 * The thread screen's whole layout, stateless: the header, the tab strip
 * (none on an agent channel) and the pager. The issue + Guide pages come in
 * as slots, so the styleguide captures draw this over fake data.
 */
@Composable
fun ThreadContent(
    view: SessionTree.ThreadView?,
    agent: ThreadAgent?,
    selectedKey: String,
    onSelectKey: (String) -> Unit,
    issueStatus: (String) -> ResolvedIssueStatus?,
    meta: (SessionTree.ThreadRow.MessageRow) -> ThreadRowMeta?,
    onOpenCard: (SessionTree.SessionCard) -> Unit,
    onAnswer: (messageId: String, answer: String) -> Unit,
    onDecidePlan: (messageId: String, approved: Boolean) -> Unit,
    canUnqueue: (SessionTree.ThreadRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    onBack: (() -> Unit)?,
    onRename: (() -> Unit)?,
    onAdd: (() -> Unit)?,
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)?,
    onAgentSettings: (() -> Unit)?,
    composer: @Composable (Modifier) -> Unit,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    guidePage: @Composable (padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** The thread PR's URL: Open on GitHub in the header while the Guide
     *  tab is selected (EXP-916/1302, iOS's header action). */
    guidePrUrl: String? = null,
) {
    // An agent channel is the conversation only.
    val pages = if (agent != null || view == null) {
        listOf<SessionTree.ThreadTab>(SessionTree.ThreadTab.Conversation)
    } else {
        view.tabs.filter { it != SessionTree.ThreadTab.Add }
    }
    // A tab that left the strip (an issue unlinked elsewhere) falls back to
    // the conversation.
    val selected = pages.firstOrNull { threadTabKey(it) == selectedKey } ?: SessionTree.ThreadTab.Conversation
    Scaffold(
        modifier = modifier,
        containerColor = Color.Transparent,
        topBar = {
            Column(modifier = Modifier.statusBarsPadding()) {
                if (agent != null) {
                    AgentChannelHeader(
                        name = agent.name,
                        icon = agent.icon,
                        color = agent.color,
                        onBack = onBack,
                        onSettings = onAgentSettings,
                    )
                } else {
                    ThreadHeader(
                        title = view?.title.orEmpty(),
                        private = view?.private == true,
                        onBack = onBack,
                        onRename = onRename,
                        action = guidePrUrl
                            ?.takeIf { selected is SessionTree.ThreadTab.Guide }
                            ?.let { url -> { GithubHeaderAction(prUrl = url) } },
                    )
                    if (view != null) {
                        ThreadTabStrip(
                            tabs = view.tabs,
                            selectedKey = threadTabKey(selected),
                            onSelect = { onSelectKey(threadTabKey(it)) },
                            onAdd = onAdd,
                            issueStatus = issueStatus,
                            onIssueMenu = onIssueMenu,
                        )
                    }
                }
            }
        },
    ) { padding ->
        if (view == null) {
            Box(Modifier.padding(padding).fillMaxSize()) {
                SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("thread-skeleton"))
            }
            return@Scaffold
        }
        TabPager(
            pages = pages,
            selected = selected,
            onSelect = { onSelectKey(threadTabKey(it)) },
            key = { threadTabKey(it) },
            modifier = Modifier.fillMaxSize(),
        ) { tab ->
            when (tab) {
                SessionTree.ThreadTab.Conversation -> Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(top = padding.calculateTopPadding()),
                ) {
                    ThreadConversation(
                        rows = view.rows,
                        rootId = view.rootId,
                        meta = meta,
                        onOpenCard = onOpenCard,
                        onAnswer = onAnswer,
                        onDecidePlan = onDecidePlan,
                        canUnqueue = canUnqueue,
                        onUnqueue = onUnqueue,
                        agent = agent,
                        modifier = Modifier.weight(1f).fillMaxWidth(),
                        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 12.dp),
                    )
                    composer(
                        Modifier
                            .navigationBarsPadding()
                            .imePadding()
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                }
                is SessionTree.ThreadTab.IssueTab -> issuePage(tab.issueId, padding)
                is SessionTree.ThreadTab.Guide -> guidePage(padding)
                SessionTree.ThreadTab.Add -> Unit
            }
        }
    }
}

/** What THE menu acts on: the "+" (null) or an issue tab. */
private data class ThreadMenuTarget(val issue: SessionTree.ThreadTab.IssueTab?)

/** The "+" / an issue tab's menu as a phone sheet: Link an issue, New
 *  issue, and on an issue tab Unlink <IDENT>. */
@Composable
private fun ThreadMenuSheet(
    issue: SessionTree.ThreadTab.IssueTab?,
    canFile: Boolean,
    onLink: () -> Unit,
    onNewIssue: () -> Unit,
    onUnlink: (SessionTree.ThreadTab.IssueTab) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = issue?.identifier, onDismiss = onDismiss, modifier = Modifier.testTag("thread-menu")) {
        val iconTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        GlassSheetRow(
            label = ThreadUiCopy.LINK_ISSUE,
            onClick = onLink,
            leading = { Icon(ExpIcons.navSearch, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        GlassSheetRow(
            label = ThreadUiCopy.NEW_ISSUE,
            onClick = onNewIssue,
            enabled = canFile,
            leading = { Icon(ExpIcons.navCreateIssue, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        if (issue != null) {
            GlassSheetRow(
                label = ThreadUiCopy.unlink(issue.identifier),
                onClick = { onUnlink(issue) },
                labelColor = MaterialTheme.colorScheme.error,
                leading = {
                    Icon(ExpIcons.uiClose, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp))
                },
            )
        }
        Spacer(Modifier.height(8.dp))
    }
}

@Composable
private fun ThreadRenameSheet(
    initial: String,
    placeholder: String,
    onSave: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var value by remember { mutableStateOf(initial) }
    GlassSheet(title = ThreadUiCopy.RENAME, onDismiss = onDismiss) {
        GlassTextField(
            value = value,
            onValueChange = { value = it.take(SessionTree.ThreadCopy.TITLE_MAX) },
            placeholder = placeholder,
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("thread-rename-field"),
        )
        GlassSubmitButton(
            label = ThreadUiCopy.SAVE,
            onClick = { onSave(value) },
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
        )
    }
}

// ── Issue page ──────────────────────────────────────────────────────────────

/** An issue tab = that issue's own Issue face, embedded (no Start circle:
 *  starts live in the composer). */
@Composable
private fun ThreadIssuePage(
    issueId: String,
    padding: PaddingValues,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenThread: (String) -> Unit,
) {
    val issueVm = hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(key = "issue:$issueId") {
        it.create(issueId)
    }
    val commentVm: CommentThreadViewModel = hiltViewModel(key = "comments:$issueId")
    val controller = rememberIssueFaceController()
    IssueFace(
        viewModel = issueVm,
        commentViewModel = commentVm,
        controller = controller,
        padding = padding,
        onBack = onBack,
        onOpenIssue = onOpenIssue,
        trailingBarSlot = { Spacer(Modifier.size(0.dp)) },
        onOpenThread = onOpenThread,
    )
}

// ── Guide page ──────────────────────────────────────────────────────────────

/**
 * The root's Guide (`sessions.guide`) with the Merge capsule of its PR — the
 * newest OPEN one, else none. When that PR sits in an open stack the capsule
 * is THE `Merge stack` control (the issue screen's [PullRequestStack] rule):
 * it targets the stack's top and lands the whole chain with `stack = true`,
 * since the server refuses a plain merge of any open-stack member.
 * A section's Changes row opens its diff page. A merge refused by a conflict
 * offers to message the root to fix it.
 */
@Composable
private fun ThreadGuidePage(
    guideRaw: String?,
    pullRequests: List<PullRequestEntity>,
    teamPullRequests: List<PullRequestEntity>,
    padding: PaddingValues,
    onFixConflicts: (prNumber: Int?) -> Unit,
) {
    val groups = remember(guideRaw) { parseSessionResultGroups(guideRaw) }
    val newest = guidePullRequest(pullRequests)
    val stackPool = remember(pullRequests, teamPullRequests) {
        (teamPullRequests + pullRequests).distinctBy { it.id }
    }
    val stack = remember(newest, stackPool) { newest?.let { PullRequestStack.stackConfirm(it, stackPool) } }
    val pr = stack?.throughId?.let { id -> stackPool.firstOrNull { it.id == id } } ?: newest
    val changesVm: ChangesViewModel? = pr?.let { row ->
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${row.id}") { it.create(row.id) }
    }
    val load by (changesVm?.load ?: remember { kotlinx.coroutines.flow.MutableStateFlow<ChangesLoadState?>(null) })
        .collectAsStateWithLifecycle()
    val merging by (changesVm?.merging ?: remember { kotlinx.coroutines.flow.MutableStateFlow(false) })
        .collectAsStateWithLifecycle()
    val actionError by (changesVm?.actionError ?: remember { kotlinx.coroutines.flow.MutableStateFlow<String?>(null) })
        .collectAsStateWithLifecycle()
    val conflict by (changesVm?.actionErrorIsConflict ?: remember { kotlinx.coroutines.flow.MutableStateFlow(false) })
        .collectAsStateWithLifecycle()
    val linkedCount by (changesVm?.linkedIssueCount ?: remember { kotlinx.coroutines.flow.MutableStateFlow(0) })
        .collectAsStateWithLifecycle()
    val files = remember(load) { (load as? ChangesLoadState.Loaded)?.files?.map { it.toDiffFile() } }
    val merge = if (changesVm != null && pr.state == DomainContract.prStateOpen) {
        ChangesMergeControl(
            label = if (stack != null) DomainContract.diffUiMergeStack else DomainContract.diffUiMergePr,
            loading = merging,
            error = actionError,
            confirmPrompt = if (linkedCount > 0) {
                Prompts.MergeIssuePr.prompt(pr.number, linkedCount)
            } else {
                Prompts.MergeRunPr.prompt(pr.number)
            },
            onConfirm = { changesVm.mergePr() },
            stackConfirm = stack,
            onMergeStack = { changesVm.mergePr(stack = true) },
        )
    } else {
        null
    }
    val toaster = LocalToaster.current
    LaunchedEffect(actionError, conflict) {
        val message = actionError ?: return@LaunchedEffect
        if (conflict) {
            toaster.error(
                FeedbackStates.errorTitle("merge", "the PR"),
                description = message,
                action = ToastAction(FIX_CONFLICTS) { onFixConflicts(pr?.number) },
            )
        } else {
            toaster.failedAction("merge", "the PR", message)
        }
    }
    var sectionName by rememberSaveable { mutableStateOf<String?>(null) }
    val section = sectionName?.let(::parseSectionKey)
    if (section != null) {
        GuideSectionDiff(
            padding = padding,
            page = remember(groups, files, section) { guideSectionPage(groups, files, section, numbered = true) },
            load = load,
            merge = merge,
            onBack = { sectionName = null },
            onRetry = changesVm?.let { vm -> vm::refresh },
        )
    } else {
        GuideFace(
            padding = padding,
            groups = groups,
            files = files,
            numbered = true,
            merge = merge,
            onOpenSection = if (changesVm != null) {
                { key: GuideSectionKey -> sectionName = sectionKeyName(key) }
            } else {
                null
            },
        )
    }
}

/** The thread's Guide PR: the newest OPEN one, else the newest. */
private fun guidePullRequest(pullRequests: List<PullRequestEntity>): PullRequestEntity? =
    pullRequests.lastOrNull { it.state == DomainContract.prStateOpen } ?: pullRequests.lastOrNull()

/** The merge-conflict toast's action (contract `diffUi` has no word). */
private const val FIX_CONFLICTS = "Fix conflicts"

private fun sectionKeyName(key: GuideSectionKey): String = when (key) {
    is GuideSectionKey.Numbered -> key.index.toString()
    GuideSectionKey.Lead -> "lead"
    GuideSectionKey.Other -> "other"
    GuideSectionKey.All -> "all"
}

private fun parseSectionKey(name: String): GuideSectionKey? = when (name) {
    "lead" -> GuideSectionKey.Lead
    "other" -> GuideSectionKey.Other
    "all" -> GuideSectionKey.All
    else -> name.toIntOrNull()?.let { GuideSectionKey.Numbered(it) }
}
