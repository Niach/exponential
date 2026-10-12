package com.exponential.app.ui.thread

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
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.Diff
import com.exponential.app.domain.GuideSectionKey
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.RunRowState
import com.exponential.app.domain.ThreadItem
import com.exponential.app.domain.ThreadPage
import com.exponential.app.domain.ThreadView
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.guideSectionPage
import com.exponential.app.domain.lastToolLine
import com.exponential.app.domain.parseSessionResultGroups
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.runRowCaption
import com.exponential.app.domain.runRowState
import com.exponential.app.domain.sessionThread
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.TabPager
import com.exponential.app.ui.components.failedAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.CommentThreadViewModel
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueFace
import com.exponential.app.ui.issue.compactRelativeTime
import com.exponential.app.ui.issue.rememberIssueFaceController
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.IssueSearchSheet
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.session.RunStatusRow
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.ChangesLoadState
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.ChangesSource
import com.exponential.app.ui.work.ChangesViewModel
import com.exponential.app.ui.work.GuideFace
import com.exponential.app.ui.work.GuideSectionDiff
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.TitleCollapseState
import com.exponential.app.ui.work.guideSectionParam
import com.exponential.app.ui.work.parseGuideSectionParam
import dev.chrisbanes.haze.rememberHazeState
import kotlinx.coroutines.flow.MutableStateFlow

/**
 * EXP-1284: the phone THREAD screen (draft 3) — the unit of work holding n
 * issues and m runs. The header names it; the child strip IS the pager's
 * tabs (Conversation · each issue · Guide, the "+" trailing); the
 * Conversation is every run as a `Run N` block with the issues its agent
 * linked and its parallel children as one card, over the `Reply in this
 * thread` box; an issue tab is that issue's own Issue face; the Guide is the
 * THREAD's Guide with the Merge of its PR. The "+" and an issue tab's
 * long-press open THE menu: Link an issue (THE search list), New issue in
 * this thread, Start a session here, Unlink.
 */
@Composable
fun ThreadScreen(
    onBack: () -> Unit,
    /** Another issue's own Work screen (a chip outside the thread). */
    onOpenIssue: (String) -> Unit,
    /** A run's own screen (`steer/{id}`): a block header, a parallel row. */
    onOpenRun: (String) -> Unit,
    /** The composer with a seed (Start a session here, a reply that starts). */
    onOpenAgent: (AgentComposerSeed) -> Unit,
    /** The New issue page on [boardId], filed into the thread. */
    onNewIssue: (boardId: String, threadId: String) -> Unit,
    /** `thread/{id}?child=` — `guide` or an issue id; null = Conversation. */
    initialChild: String? = null,
    /** Another thread (an issue tab's `Threads` row). */
    onOpenThread: (String) -> Unit = {},
    /** The `thread-not-found` state's "Go to Agent". */
    onGoToAgent: () -> Unit = onBack,
    viewModel: ThreadScreenViewModel = hiltViewModel(),
) {
    val bundle by viewModel.bundle.collectAsStateWithLifecycle()
    val statuses by viewModel.statuses.collectAsStateWithLifecycle()
    val viewerId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val linkCandidates by viewModel.linkCandidates.collectAsStateWithLifecycle()
    val newIssueBoardId by viewModel.newIssueBoardId.collectAsStateWithLifecycle()
    val missing by viewModel.missing.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    val view = bundle?.view
    var selectedKey by rememberSaveable {
        mutableStateOf(
            when (initialChild) {
                null, "" -> "conversation"
                "guide" -> "guide"
                else -> "issue:$initialChild"
            },
        )
    }
    // A child that left the strip (unlinked elsewhere) falls back to the
    // conversation, like web `selectedThreadChild`.
    val pages = view?.strip?.filter { it != ThreadView.Child.Add } ?: listOf(ThreadView.Child.Conversation)
    val selected = pages.firstOrNull { threadChildKey(it) == selectedKey } ?: ThreadView.Child.Conversation

    // First-render stamps per issue item: a child linked moments before the
    // viewer first saw it fades in with a ring, once per mount.
    val firstSeen = remember { mutableStateMapOf<String, Long>() }
    val itemCreatedAt = bundle?.items?.filter { it.issueId != null }?.associate { it.issueId!! to it.createdAt }.orEmpty()
    itemCreatedAt.keys.forEach { id -> if (id !in firstSeen) firstSeen[id] = System.currentTimeMillis() }

    var menuFor by remember { mutableStateOf<ThreadMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var renameOpen by remember { mutableStateOf(false) }

    val statusById = remember(bundle, statuses) {
        bundle?.issues?.associate { it.id to com.exponential.app.domain.IssueStatusResolver.resolve(it, statuses) }.orEmpty()
    }
    val hazeState = rememberHazeState()

    ProvideMarkdownToolbar {
        CompositionLocalProvider(LocalDetailHaze provides hazeState) {
            Scaffold(
                containerColor = Color.Transparent,
                topBar = {
                    Column(modifier = Modifier.statusBarsPadding()) {
                        ThreadHeader(
                            title = view?.title.orEmpty(),
                            caption = view?.headerCaption.orEmpty(),
                            onBack = onBack,
                            onRename = { renameOpen = true },
                        )
                        if (view != null) {
                            ThreadChildTabs(
                                strip = view.strip,
                                selectedKey = threadChildKey(selected),
                                onSelect = { selectedKey = threadChildKey(it) },
                                onAdd = { menuFor = ThreadMenuTarget(null) },
                                issueStatus = { statusById[it] },
                                onIssueMenu = { menuFor = ThreadMenuTarget(it) },
                                fresh = { id ->
                                    ThreadPage.isFreshChild(itemCreatedAt[id], firstSeen[id] ?: System.currentTimeMillis())
                                },
                            )
                        }
                    }
                },
            ) { padding ->
                val current = bundle
                if (current == null) {
                    Box(Modifier.padding(padding).fillMaxSize(), contentAlignment = Alignment.Center) {
                        // EXP-1284: the ×4 `thread-not-found` page state, once
                        // the store answered without the thread.
                        if (!missing) {
                            // T14/T-N9: the skeleton while the store, a synced
                            // item or the one `threads.get` may still bring it.
                            SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("thread-skeleton"))
                        } else {
                            EmptyState(
                                EmptyStateCopy.threadNotFound,
                                modifier = Modifier.testTag("thread-not-found"),
                                action = {
                                    GlassPill(
                                        EmptyStateCopy.threadNotFound.action!!,
                                        size = PillSize.Md,
                                        primary = true,
                                        onClick = onGoToAgent,
                                    )
                                },
                            )
                        }
                    }
                    return@Scaffold
                }
                TabPager(
                    pages = pages,
                    selected = selected,
                    onSelect = { selectedKey = threadChildKey(it) },
                    key = { threadChildKey(it) },
                    modifier = Modifier.fillMaxSize(),
                ) { child ->
                    when (child) {
                        ThreadView.Child.Conversation -> ThreadConversationPage(
                            bundle = current,
                            padding = padding,
                            viewerId = viewerId,
                            canSteer = steerEnabled == true,
                            devices = devices,
                            statusOf = { statusById[it] },
                            onOpenIssueChild = { id -> selectedKey = "issue:$id" },
                            onOpenRun = onOpenRun,
                            onStart = { text -> onOpenAgent(AgentComposerSeed(text = text, threadId = current.thread.id)) },
                        )
                        is ThreadView.Child.IssueChild -> ThreadIssuePage(
                            issueId = child.issueId,
                            padding = padding,
                            onBack = onBack,
                            onOpenIssue = { id ->
                                if (id in current.view.issueIds) selectedKey = "issue:$id" else onOpenIssue(id)
                            },
                            onOpenGuide = { selectedKey = "guide" },
                            onOpenThread = { id -> if (id != current.thread.id) onOpenThread(id) },
                        )
                        is ThreadView.Child.Guide -> ThreadGuidePage(bundle = current, padding = padding)
                        ThreadView.Child.Add -> Unit
                    }
                }
            }
        }
    }

    menuFor?.let { target ->
        ThreadMenuSheet(
            issue = target.issue,
            canStart = steerEnabled == true,
            canFile = newIssueBoardId != null,
            onLink = {
                menuFor = null
                linkPickerOpen = true
            },
            onNewIssue = {
                menuFor = null
                newIssueBoardId?.let { onNewIssue(it, viewModel.threadId) }
            },
            onStartRun = {
                menuFor = null
                onOpenAgent(AgentComposerSeed(threadId = viewModel.threadId))
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
        val linked = bundle?.view?.issueIds?.toSet().orEmpty()
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
                viewModel.linkIssue(target.issueId)
            },
            onDismiss = { linkPickerOpen = false },
        )
    }
    if (renameOpen) {
        ThreadRenameSheet(
            initial = if (view?.titleDerived == false) view.title else "",
            placeholder = view?.title.orEmpty(),
            onSave = { title ->
                renameOpen = false
                viewModel.rename(title)
            },
            onDismiss = { renameOpen = false },
        )
    }
}

/** What THE menu acts on: the "+" (null) or an issue tab. */
private data class ThreadMenuTarget(val issue: ThreadView.Child.IssueChild?)

/**
 * THE menu, as a phone sheet: Link an issue, New issue in this thread, Start
 * a session here, and on an issue tab Unlink <IDENT>.
 */
@Composable
internal fun ThreadMenuSheet(
    issue: ThreadView.Child.IssueChild?,
    canStart: Boolean,
    canFile: Boolean,
    onLink: () -> Unit,
    onNewIssue: () -> Unit,
    onStartRun: () -> Unit,
    onUnlink: (ThreadView.Child.IssueChild) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = issue?.identifier, onDismiss = onDismiss, modifier = Modifier.testTag("thread-menu")) {
        val iconTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        GlassSheetRow(
            label = ThreadCopy.LINK_ISSUE,
            onClick = onLink,
            leading = { Icon(ExpIcons.navSearch, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        GlassSheetRow(
            label = ThreadCopy.NEW_ISSUE,
            onClick = onNewIssue,
            enabled = canFile,
            leading = { Icon(ExpIcons.uiAdd, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        GlassSheetRow(
            label = ThreadCopy.START_RUN,
            onClick = onStartRun,
            enabled = canStart,
            leading = { Icon(ExpIcons.actionRun, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        if (issue != null) {
            GlassSheetRow(
                label = ThreadCopy.unlink(issue.identifier),
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
    GlassSheet(title = "Rename thread", onDismiss = onDismiss) {
        GlassTextField(
            value = value,
            onValueChange = { value = it },
            placeholder = placeholder,
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("thread-rename-field"),
        )
        GlassSubmitButton(
            label = "Save",
            onClick = { onSave(value) },
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
        )
    }
}

// ── Conversation ────────────────────────────────────────────────────────────

@Composable
private fun ThreadConversationPage(
    bundle: ThreadPage.Bundle,
    padding: PaddingValues,
    viewerId: String?,
    canSteer: Boolean,
    devices: List<DeviceEntity>,
    statusOf: (String) -> com.exponential.app.domain.ResolvedIssueStatus?,
    onOpenIssueChild: (String) -> Unit,
    onOpenRun: (String) -> Unit,
    onStart: (String) -> Unit,
) {
    val runsById = remember(bundle.runs) { bundle.runs.associateBy { it.id } }
    val issuesById = remember(bundle.issues) { bundle.issues.associateBy { it.id } }
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            kotlinx.coroutines.delay(30_000)
            nowMs = System.currentTimeMillis()
        }
    }
    val namesById = remember(bundle.view) {
        bundle.view.conversation.flatMap { block ->
            when (block) {
                is ThreadView.ParallelBlock -> block.runs.map { it.runId to it.name }
                is ThreadView.RunBlock -> emptyList()
            }
        }.toMap()
    }
    // The model's own names (the run identity) for the folded line.
    val modelRuns = remember(bundle, nowMs) {
        bundle.runs.map { run -> ThreadPage.toRun(run, "", null, nowMs) }
    }
    val replyTarget = ThreadPage.replyTarget(modelRuns, viewerId, canSteer)
    var reply by rememberSaveable { mutableStateOf("") }
    // A reply waiting for its run: a resume in flight, or a connection that
    // is not up yet. Sent once the target run can take it.
    var pending by rememberSaveable { mutableStateOf<String?>(null) }
    // Whether the held reply waits on a RESUME (else a connection).
    var pendingResume by rememberSaveable { mutableStateOf(false) }

    Box(modifier = Modifier.fillMaxSize()) {
        ThreadConversation(
            view = bundle.view,
            runMeta = { id ->
                runsById[id]?.let { run ->
                    ThreadRunMeta(
                        agent = run.agent?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
                        busy = run.agentBusy,
                        name = namesById[id] ?: com.exponential.app.domain.pastRunTitle(
                            run,
                            run.issueId?.let(issuesById::get),
                        ),
                    )
                }
            },
            issueStatus = statusOf,
            onOpenIssue = onOpenIssueChild,
            onOpenRun = onOpenRun,
            parallelCaption = { row ->
                listOfNotNull(
                    row.deviceLabel?.takeIf { it.isNotBlank() },
                    compactRelativeTime(row.startedAt, nowMs).takeIf { it.isNotEmpty() },
                ).joinToString(" · ")
            },
            issueTitle = { id -> issuesById[id]?.let { it.identifier to it.title } },
            runLead = { block ->
                val run = runsById[block.runId]
                if (run != null && run.userId == viewerId && block.state != ThreadView.RunState.Ended) {
                    OwnRunLead(run)
                }
            },
            runBody = { block, expanded ->
                runsById[block.runId]?.let { run ->
                    ThreadRunBody(
                        run = run,
                        // T-N3: the block shows only what THIS session wrote.
                        guide = ThreadPage.sessionGuide(run, bundle.thread, bundle.view.runIds.firstOrNull()),
                        state = block.state,
                        own = run.userId == viewerId,
                        expanded = expanded,
                        devices = devices,
                        nowMs = nowMs,
                    )
                }
            },
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = padding.calculateTopPadding() + 12.dp,
                bottom = padding.calculateBottomPadding() + 96.dp,
            ),
        )
        if (replyTarget != null) {
            // The send/resume target's connection, kept for the reply.
            val targetRunId = when (replyTarget) {
                is ThreadPage.ReplyTarget.Send -> replyTarget.runId
                is ThreadPage.ReplyTarget.Resume -> replyTarget.runId
                ThreadPage.ReplyTarget.Start -> null
            }
            val runVm: AgentSessionViewModel? = targetRunId?.let { id ->
                hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(key = "session:$id") { it.create(id) }
            }
            val connected by (runVm?.connected ?: remember { MutableStateFlow(false) }).collectAsStateWithLifecycle()
            val resumeTarget by (runVm?.resumeTarget ?: remember { MutableStateFlow(null) }).collectAsStateWithLifecycle()
            LaunchedEffect(runVm, replyTarget is ThreadPage.ReplyTarget.Send) {
                if (replyTarget is ThreadPage.ReplyTarget.Send) runVm?.ensureConnected()
            }
            // The held reply goes out the moment a live session of mine can take it.
            LaunchedEffect(pending, connected, replyTarget) {
                val text = pending ?: return@LaunchedEffect
                if (replyTarget is ThreadPage.ReplyTarget.Send && connected && runVm?.sendCommand(text) == true) {
                    pending = null
                }
            }
            val toaster = LocalToaster.current
            // T9/T-N4: the reply's ONE path. A live session of mine takes it;
            // my latest (heartbeat-dead) session is resumed when its machine
            // can; otherwise the thread gets a NEW session — a reply never
            // dead-ends.
            lateinit var sendReply: (String) -> Unit
            sendReply = { text ->
                when (val target = replyTarget) {
                    is ThreadPage.ReplyTarget.Send -> {
                        reply = ""
                        if (runVm?.sendCommand(text) != true) {
                            pending = text
                            pendingResume = false
                            runVm?.ensureConnected()
                        }
                    }
                    is ThreadPage.ReplyTarget.Resume -> {
                        val resume = resumeTarget
                        reply = ""
                        if (resume == null) {
                            // Its machine is gone or unknown: say so, and start
                            // a new session in this thread with the text.
                            toaster.info(THREAD_SESSION_ENDED)
                            onStart(text)
                        } else {
                            pending = text
                            pendingResume = true
                            runVm?.resumeRun(resume)
                        }
                    }
                    ThreadPage.ReplyTarget.Start -> {
                        reply = ""
                        onStart(text)
                    }
                    null -> Unit
                }
            }
            // T-N4: a held reply times out like the composer's start: the
            // text comes back and `Couldn't resume the session` offers Retry.
            LaunchedEffect(pending) {
                val text = pending ?: return@LaunchedEffect
                kotlinx.coroutines.delay(FeedbackStates.STARTING_TIMEOUT_MS)
                if (pending == text) {
                    pending = null
                    if (reply.isBlank()) reply = text
                    toaster.error(
                        if (pendingResume) {
                            FeedbackStates.errorTitle("resume", "the session")
                        } else {
                            FeedbackStates.errorTitle("send", "the message")
                        },
                        action = ToastAction(FeedbackStates.RETRY) { sendReply(reply.ifBlank { text }.trim()) },
                    )
                }
            }
            ThreadReplyBar(
                value = reply,
                onValueChange = { reply = it },
                sending = pending != null,
                onSend = {
                    val text = reply.trim()
                    if (text.isNotEmpty()) sendReply(text)
                },
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = padding.calculateBottomPadding())
                    .navigationBarsPadding()
                    .imePadding(),
            )
        }
    }
}

/** The owner's opening message of a live run (its feed's first message). */
@Composable
private fun OwnRunLead(run: CodingSessionEntity) {
    val vm = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(key = "session:${run.id}") {
        it.create(run.id)
    }
    LaunchedEffect(vm) { vm.ensureConnected() }
    val activity by vm.activity.collectAsStateWithLifecycle()
    val first = remember(activity.feed) {
        activity.feed.firstNotNullOfOrNull { (it as? AgentFeedItem.UserMessage)?.text?.takeIf { text -> text.isNotBlank() } }
    }
    first?.let { ThreadMessageBubble(it) }
}

/**
 * A run block's body: what the run published (its thread items and the
 * Summary as the reply) and, while it runs, the Run face's status row. A
 * LIVE run's content stays the owner's (EXP-312): teammates see the row.
 */
@Composable
private fun ThreadRunBody(
    run: CodingSessionEntity,
    /** This session's own slice of the thread Guide (`sessionGuideEntries`). */
    guide: String?,
    state: ThreadView.RunState,
    own: Boolean,
    expanded: Boolean,
    devices: List<DeviceEntity>,
    nowMs: Long,
) {
    val live = state != ThreadView.RunState.Ended
    val thread = remember(guide) { sessionThread(guide) }
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (own || !live || expanded) {
            thread.items.forEach { item ->
                if (item is ThreadItem.Text) {
                    Text(
                        item.topic,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    MarkdownView(markdown = item.text, softBreaksAsNewlines = true)
                }
            }
            thread.reply?.let { MarkdownView(markdown = it, softBreaksAsNewlines = true) }
        }
        if (live) {
            val device = remember(run, devices, nowMs) { resolveSessionDevice(run, devices, nowMs) }
            val display = codingSessionDisplayState(run, run.prState)
            val rowState = runRowState(
                paused = device.isPaused(display, run.status),
                ended = false,
                awaitingInput = false,
                working = run.agentBusy,
                display = display,
            )
            val caption = runRowCaption(rowState, device.displayLabel, run.startedAt, run.endedAt ?: run.updatedAt, nowMs)
            val toolLine = if (own) ownToolLine(run.id) else null
            RunStatusRow(
                agent = run.agent?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
                markState = when (rowState) {
                    RunRowState.Paused, RunRowState.Ended -> null
                    RunRowState.NeedsInput -> CodingSessionDisplayState.NeedsInput
                    RunRowState.Working -> CodingSessionDisplayState.Working
                    RunRowState.Review -> CodingSessionDisplayState.Review
                    RunRowState.Done -> CodingSessionDisplayState.Done
                },
                ended = false,
                caption = caption.text,
                tone = caption.tone,
                toolLine = toolLine,
                showWork = false,
                onToggle = null,
            )
        }
    }
}

/** The owner's newest tool line, off the run's relay feed. */
@Composable
private fun ownToolLine(runId: String): String? {
    val vm = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(key = "session:$runId") {
        it.create(runId)
    }
    val activity by vm.activity.collectAsStateWithLifecycle()
    return remember(activity.feed) { lastToolLine(activity.feed) }
}

// ── Issue child ─────────────────────────────────────────────────────────────

/** An issue child = that issue's own Issue face, embedded. */
@Composable
private fun ThreadIssuePage(
    issueId: String,
    padding: PaddingValues,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenGuide: () -> Unit,
    onOpenThread: (String) -> Unit,
) {
    val issueVm = hiltViewModel<IssueDetailViewModel, IssueDetailViewModel.Factory>(key = "issue:$issueId") {
        it.create(issueId)
    }
    val commentVm: CommentThreadViewModel = hiltViewModel(key = "comments:$issueId")
    val controller = rememberIssueFaceController()
    val collapse = remember { TitleCollapseState() }
    CompositionLocalProvider(LocalTitleCollapse provides collapse) {
        IssueFace(
            viewModel = issueVm,
            commentViewModel = commentVm,
            controller = controller,
            padding = padding,
            onBack = onBack,
            onOpenIssue = onOpenIssue,
            onOpenChanges = onOpenGuide,
            trailingBarSlot = { Spacer(Modifier.size(0.dp)) },
            onOpenThread = onOpenThread,
        )
    }
}

// ── Guide child ─────────────────────────────────────────────────────────────

/**
 * The THREAD's Guide (`threads.results`, else its newest run's) with the
 * Merge of the thread's PR — its newest run's, else the URL its issues share.
 */
@Composable
private fun ThreadGuidePage(bundle: ThreadPage.Bundle, padding: PaddingValues) {
    val newestRun = bundle.runs.lastOrNull()
    val raw = ThreadPage.guideResults(newestRun?.results, bundle.thread.results)
    val groups = remember(raw) { parseSessionResultGroups(raw) }
    val prUrl = ThreadPage.prUrl(bundle.runs, bundle.issues)
    val prIssue = prUrl?.let { url -> bundle.issues.firstOrNull { it.prUrl == url } }
    val prRun = prUrl?.let { url -> bundle.runs.lastOrNull { it.prUrl == url } }
    val source: ChangesSource? = when {
        prIssue != null -> ChangesSource.Issue(prIssue.id)
        prRun != null -> prRun.issueId?.let { ChangesSource.Issue(it) } ?: ChangesSource.Session(prRun.id)
        else -> null
    }
    val changesVm: ChangesViewModel? = source?.let { src ->
        val key = when (src) {
            is ChangesSource.Issue -> "thread-changes:issue:${src.id}"
            is ChangesSource.Session -> "thread-changes:run:${src.id}"
        }
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = key) { it.create(src) }
    }
    val load by (changesVm?.load ?: remember { MutableStateFlow<ChangesLoadState?>(null) }).collectAsStateWithLifecycle()
    val merging by (changesVm?.merging ?: remember { MutableStateFlow(false) }).collectAsStateWithLifecycle()
    val actionError by (changesVm?.actionError ?: remember { MutableStateFlow(null) }).collectAsStateWithLifecycle()
    val files: List<Diff.File>? = remember(load) {
        (load as? ChangesLoadState.Loaded)?.files?.map { it.toDiffFile() }
    }
    val prOpen = (prIssue?.prState ?: prRun?.prState) == DomainContract.prStateOpen
    val merge = if (changesVm != null && prOpen) {
        ChangesMergeControl(
            label = DomainContract.diffUiMergePr,
            loading = merging,
            error = actionError,
            confirmPrompt = if (source is ChangesSource.Session) {
                Prompts.MergeRunPr.prompt(prRun?.prNumber)
            } else {
                Prompts.MergeIssuePr.prompt(prIssue?.prNumber, bundle.issues.count { it.prUrl == prUrl }.coerceAtLeast(1))
            },
            onConfirm = { changesVm.mergePr() },
        )
    } else {
        null
    }
    val toaster = LocalToaster.current
    LaunchedEffect(actionError) {
        actionError?.let { toaster.failedAction("merge", "the PR", it) }
    }
    var sectionName by rememberSaveable { mutableStateOf<String?>(null) }
    val section = parseGuideSectionParam(sectionName)
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
                { key: GuideSectionKey -> sectionName = guideSectionParam(key) }
            } else {
                null
            },
        )
    }
}

/** T9: the reply's note when the session it would resume cannot come back. */
internal const val THREAD_SESSION_ENDED = "That session ended. Your reply starts a new session in this thread."
