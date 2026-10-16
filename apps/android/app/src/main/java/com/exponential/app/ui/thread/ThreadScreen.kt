package com.exponential.app.ui.thread

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.components.HeaderEdgeChrome
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.work.WorkTopBar
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.ui.session.AgentSessionViewModel
import com.exponential.app.ui.session.SessionComposer
import com.exponential.app.ui.session.SessionUsageSheet
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.domain.ClosePr
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
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.failedAction
import com.exponential.app.ui.components.TabPager
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.IssueScreen
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.IssueSearchSheet
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.ChangesLoadState
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.ChangesViewModel
import com.exponential.app.ui.work.GuideFace
import com.exponential.app.ui.work.GuideStack
import com.exponential.app.ui.work.STACK_CONFIRM_CANCEL_LABEL
import com.exponential.app.ui.work.StackMergeDialog
import com.exponential.app.ui.work.GuideSectionDiff
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.PullRequestStack
import com.exponential.app.ui.work.TitleCollapseState
import dev.chrisbanes.haze.rememberHazeState
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * EXP-1296 Threads v2 (Model v3): THE conversation screen for a thread root
 * OR an agent root (`thread/{sessionId}`), `session-thread.json`.
 *
 * A thread: the title (EXP-1316: derived, never renamed), then ONE tab strip
 * = the phone pager — Conversation · one chip per linked issue · "+" (link /
 * file an issue) · the session chips (a tap pushes that session's view) ·
 * Guide. Conversation = the root's rows (THE session card per session, the
 * person's bubbles, the agent's prose, THE session view's question / plan
 * card answered in place, subagent / picture / event rows) over THE composer
 * box; an issue page = that issue's [IssueScreen] embedded; Guide = the root's Guide with
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
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val linkCandidates by viewModel.linkCandidates.collectAsStateWithLifecycle()
    val newIssueBoardId by viewModel.newIssueBoardId.collectAsStateWithLifecycle()
    val pullRequests by viewModel.pullRequests.collectAsStateWithLifecycle()
    val guide by viewModel.guide.collectAsStateWithLifecycle()
    val teamPullRequests by viewModel.teamPullRequests.collectAsStateWithLifecycle()
    val pullRequestIssues by viewModel.pullRequestIssues.collectAsStateWithLifecycle()
    val syncedIssues by viewModel.syncedIssues.collectAsStateWithLifecycle()
    val isMember by viewModel.isMember.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    var selectedKey by rememberSaveable { mutableStateOf(viewModel.initialTab ?: "conversation") }
    var menuFor by remember { mutableStateOf<ThreadMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var closePrConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }

    // THE session composer's state = the ROOT session's view model (the same
    // live relay state the session view reads: config, usage, queue, turn).
    val rootSession = hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:${viewModel.rootId}",
    ) { factory -> factory.create(viewModel.rootId) }
    LaunchedEffect(rootSession) { rootSession.ensureConnected() }
    // The ring opens THE session view's Usage sheet (account switch included).
    var usageSheetOpen by remember { mutableStateOf(false) }

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

    // EXP-1307: the Guide's ONE PR behind the header `…`.
    val guidePr = SessionTree.guidePullRequest(pullRequests)

    // EXP-760 ×4: a `#IDENTIFIER` (and, in the conversation, a BARE one) of a
    // synced same-team issue is a pill; one linked here opens its tab.
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    val issueRefHandler = remember(linkCandidates, statusById) {
        IssueRefHandler(linkCandidates, searchServer = viewModel::searchIssues) { target ->
            if (statusById.containsKey(target.issueId)) selectedKey = "issue:${target.issueId}"
            else currentOnOpenIssue(target.issueId)
        }
    }

    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
            LocalIssueRefs provides issueRefHandler,
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
                onAnswer = { id, answer, settled -> viewModel.answer(id, answer, settled) },
                onDecidePlan = { id, approved, feedback, settled -> viewModel.decidePlan(id, approved, feedback, settled) },
                onOpenSessionTab = { tab -> onOpenSession(tab.sessionId) },
                canUnqueue = { row -> row.authorId != null && row.authorId == me },
                onUnqueue = viewModel::unqueue,
                onBack = onBack,
                onAdd = { menuFor = ThreadMenuTarget(null) },
                onIssueMenu = { menuFor = ThreadMenuTarget(it) },
                onAgentSettings = { onOpenAgentSettings(viewModel.rootId) },
                // THE session composer (dispatcher roots): the session view's
                // box targeting the ROOT, fed by the root's live state.
                composer = { expanded, onExpandedChange ->
                    SessionComposer(
                        viewModel = rootSession,
                        expanded = expanded,
                        onExpandedChange = onExpandedChange,
                        placeholder = placeholder,
                        capsuleTestTag = "thread-composer-capsule",
                        // Queued rows already sit in the Conversation (EXP-1317).
                        showQueue = false,
                        onOpenUsage = { usageSheetOpen = true },
                        modifier = Modifier.testTag(SessionTree.ThreadCopy.TEST_ID_COMPOSER),
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
                        onMessage = { text -> viewModel.send(text) },
                    )
                },
                guidePage = { padding ->
                    ThreadGuidePage(
                        guideRaw = guide,
                        pullRequests = pullRequests,
                        teamPullRequests = teamPullRequests,
                        pullRequestIssues = pullRequestIssues,
                        issues = syncedIssues,
                        padding = padding,
                        onFixConflicts = { number -> viewModel.send(fixConflictsMessage(number)) },
                        onOpenThread = { id -> if (id != viewModel.rootId) onOpenThread(id) },
                    )
                },
                guideMenu = { guidePr?.let { pr -> GuideMenu(pr, isMember) { closePrConfirm = pr } } },
            )
        }
    }

    SessionUsageSheet(
        viewModel = rootSession,
        open = usageSheetOpen,
        onDismiss = { usageSheetOpen = false },
    )
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
    // EXP-1307: the Guide `…`'s Close PR asks first (close-pr.json, the
    // issue screen's confirm; batchLine when the PR links other issues).
    closePrConfirm?.let { pr ->
        val others = (pullRequestIssues.count { it.pullRequestId == pr.id } - 1).coerceAtLeast(0)
        GlassAlert(
            title = ClosePr.TITLE,
            body = ClosePr.body(others),
            onDismiss = { closePrConfirm = null },
            trailing = listOf(
                GlassAlertAction(STACK_CONFIRM_CANCEL_LABEL, onClick = { closePrConfirm = null }),
                GlassAlertAction(
                    ClosePr.CONFIRM,
                    destructive = true,
                    testTag = "close-pr-confirm",
                    onClick = {
                        closePrConfirm = null
                        viewModel.closePullRequest(pr.id)
                    },
                ),
            ),
            defaultAction = 0,
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
 * The thread screen's whole layout, stateless: the header band and the
 * pager. EXP-1316: the header IS the detail chrome ([WorkTopBar] in
 * [HeaderEdgeChrome]): `‹ · title (one line, lock) · …` with the tab strip
 * inside the band, the pages scrolling UNDER it (the conversation registers
 * as the blur's source). The reply box floats over the conversation's tail
 * ([composer] = THE [FloatingComposer], in a [FloatingBarEdge]). An agent
 * channel wears the same band without a strip. The issue + Guide pages come
 * in as slots and take the Scaffold padding themselves.
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
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    canUnqueue: (SessionTree.ThreadRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    onBack: (() -> Unit)?,
    /** A session chip of the strip (phones push that session's view). */
    onOpenSessionTab: (SessionTree.ThreadTab.SessionTab) -> Unit,
    onAdd: (() -> Unit)?,
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)?,
    onAgentSettings: (() -> Unit)?,
    /** THE floating reply box, folded or expanded. */
    composer: @Composable (expanded: Boolean, onExpandedChange: (Boolean) -> Unit) -> Unit,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    guidePage: @Composable (padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-1307: the header `…` (Open on GitHub · Close PR) while the Guide
     *  tab is selected; the slot draws nothing when the Guide has no PR. */
    guideMenu: (@Composable () -> Unit)? = null,
) {
    // An agent channel is the conversation only.
    val pages = if (agent != null || view == null) {
        listOf<SessionTree.ThreadTab>(SessionTree.ThreadTab.Conversation)
    } else {
        view.tabs.filter { it != SessionTree.ThreadTab.Add && it !is SessionTree.ThreadTab.SessionTab }
    }
    // A tab that left the strip (an issue unlinked elsewhere) falls back to
    // the conversation.
    val selected = pages.firstOrNull { threadTabKey(it) == selectedKey } ?: SessionTree.ThreadTab.Conversation
    var composerExpanded by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        modifier = modifier,
        containerColor = Color.Transparent,
        topBar = {
            if (agent != null) {
                HeaderEdgeChrome {
                    AgentChannelHeader(
                        name = agent.name,
                        icon = agent.icon,
                        color = agent.color,
                        onBack = onBack,
                        onSettings = onAgentSettings,
                        modifier = Modifier.statusBarsPadding(),
                    )
                }
            } else {
                WorkTopBar(
                    title = view?.title.orEmpty(),
                    titleContent = { ThreadTitle(title = view?.title.orEmpty(), private = view?.private == true) },
                    onBack = { onBack?.invoke() },
                    verb = null,
                    verbEnabled = false,
                    onVerb = {},
                    // EXP-1316: the `…` rides every tab ×4 while the Guide has a PR.
                    menu = guideMenu,
                    tabs = if (view != null) {
                        {
                            ThreadTabStrip(
                                tabs = view.tabs,
                                selectedKey = threadTabKey(selected),
                                onSelect = { onSelectKey(threadTabKey(it)) },
                                onAdd = onAdd,
                                issueStatus = issueStatus,
                                onIssueMenu = onIssueMenu,
                                onOpenSession = onOpenSessionTab,
                            )
                        }
                    } else {
                        null
                    },
                )
            }
        },
    ) { padding ->
        if (view == null) {
            Box(Modifier.padding(padding).fillMaxSize()) {
                SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("thread-skeleton"))
            }
            return@Scaffold
        }
        val topInset = padding.calculateTopPadding()
        val bottomInset = padding.calculateBottomPadding()
        TabPager(
            pages = pages,
            selected = selected,
            onSelect = { onSelectKey(threadTabKey(it)) },
            key = { threadTabKey(it) },
            modifier = Modifier.fillMaxSize(),
        ) { tab ->
            when (tab) {
                SessionTree.ThreadTab.Conversation -> {
                    var bandHeightPx by remember { mutableIntStateOf(0) }
                    val bandClearance = with(LocalDensity.current) { bandHeightPx.toDp() } + bottomInset
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            // The band sits on the nav bar (its own padding
                            // below); consuming it keeps imePadding from
                            // re-adding it (the session view's rule).
                            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
                            .imePadding(),
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
                            modifier = Modifier.fillMaxSize().detailHazeSource(),
                            contentPadding = PaddingValues(
                                start = 16.dp,
                                end = 16.dp,
                                top = topInset + 16.dp,
                                bottom = bandClearance + 12.dp,
                            ),
                            jumpBottomInset = bandClearance,
                        )
                        FloatingBarEdge(
                            bottomInset = bottomInset,
                            // An expanded composer card is its own surface.
                            visible = !composerExpanded,
                            modifier = Modifier
                                .align(Alignment.BottomCenter)
                                .fillMaxWidth()
                                .padding(bottom = bottomInset),
                        ) {
                            Box(
                                Modifier
                                    .fillMaxWidth()
                                    .onSizeChanged { if (it.height != bandHeightPx) bandHeightPx = it.height },
                            ) {
                                composer(composerExpanded) { composerExpanded = it }
                            }
                        }
                    }
                }
                is SessionTree.ThreadTab.IssueTab -> issuePage(tab.issueId, padding)
                is SessionTree.ThreadTab.Guide -> guidePage(padding)
                SessionTree.ThreadTab.Add, is SessionTree.ThreadTab.SessionTab -> Unit
            }
        }
    }
}

/** EXP-1307: the Guide `…` for [pr] — [SessionTree.guideMenuEntries] over
 *  its state and the viewer's membership. */
@Composable
private fun GuideMenu(pr: PullRequestEntity, member: Boolean, onClosePr: () -> Unit) {
    ThreadGuideMenuButton(
        prUrl = pr.url,
        entries = SessionTree.guideMenuEntries(state = pr.state, member = member),
        onClosePr = onClosePr,
    )
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

// ── Issue page ──────────────────────────────────────────────────────────────

/** An issue tab = that issue's [IssueScreen] embedded (EXP-1316): no own
 *  top bar, no `Threads` row, no Start; a merge refused by a conflict asks
 *  the thread's root to fix it ([onMessage]). */
@Composable
private fun ThreadIssuePage(
    issueId: String,
    padding: PaddingValues,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenThread: (String) -> Unit,
    onMessage: (String) -> Unit,
) {
    IssueScreen(
        issueId = issueId,
        onBack = onBack,
        onOpenIssue = onOpenIssue,
        onOpenAgent = { seed -> seed.text?.takeIf { it.isNotBlank() }?.let(onMessage) },
        onOpenThread = onOpenThread,
        embedded = true,
        padding = padding,
    )
}

// ── Guide page ──────────────────────────────────────────────────────────────

/**
 * The root's Guide (`sessions.guide`) with the Merge capsule of its PR —
 * [SessionTree.guidePullRequest]: the newest OPEN one, else the newest merged
 * one (no capsule then). EXP-1306: when that PR sits in an open stack the
 * Guide leads with the `Stack` band over THE [StackRail] (top first, the PR
 * washed, a tap opens that PR's thread, a long-press merges through it) and
 * the capsule is THE `Merge stack` control (the issue screen's
 * [PullRequestStack] rule): it targets the stack's top and lands the whole
 * chain with `stack = true`, since the server refuses a plain merge of any
 * open-stack member.
 * A section's Changes row opens its diff page. A merge refused by a conflict
 * offers to message the root to fix it.
 */
@Composable
private fun ThreadGuidePage(
    guideRaw: String?,
    pullRequests: List<PullRequestEntity>,
    teamPullRequests: List<PullRequestEntity>,
    pullRequestIssues: List<PullRequestIssueEntity>,
    issues: List<IssueEntity>,
    padding: PaddingValues,
    onFixConflicts: (prNumber: Int?) -> Unit,
    onOpenThread: (rootSessionId: String) -> Unit,
) {
    val groups = remember(guideRaw) { parseSessionResultGroups(guideRaw) }
    val newest = SessionTree.guidePullRequest(pullRequests)
    val stackPool = remember(pullRequests, teamPullRequests) {
        (teamPullRequests + pullRequests).distinctBy { it.id }
    }
    val stack = remember(newest, stackPool) { newest?.let { PullRequestStack.stackConfirm(it, stackPool) } }
    val stackView = remember(newest, stackPool, pullRequestIssues, issues) {
        newest?.let { PullRequestStack.stackView(it, stackPool, pullRequestIssues, issues) }
    }
    // The rail's long-press: Merge through that member, its ONE confirm first.
    // The member stays picked after the confirm so its merge VM (and refusal
    // toast) outlives the dialog.
    var throughId by remember { mutableStateOf<String?>(null) }
    var throughConfirmOpen by remember { mutableStateOf(false) }
    val throughPr = throughId?.let { id -> stackPool.firstOrNull { it.id == id } }
    val throughVm: ChangesViewModel? = throughPr?.let { row ->
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${row.id}") { it.create(row.id) }
    }
    val throughError by (throughVm?.actionError ?: remember { kotlinx.coroutines.flow.MutableStateFlow<String?>(null) })
        .collectAsStateWithLifecycle()
    val guideStack = stackView?.let { view ->
        GuideStack(
            view = view,
            onOpen = { row -> row.rootSessionId?.let(onOpenThread) },
            onMergeThrough = { row ->
                throughId = row.pullRequestId
                throughConfirmOpen = true
            },
        )
    }
    val pr = stack?.throughId?.let { id -> stackPool.firstOrNull { it.id == id } } ?: newest
    val changesVm: ChangesViewModel? = pr?.let { row ->
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${row.id}") { it.create(row.id) }
    }
    // EXP-1303: the Guide counts the THREAD's PR files (web/iOS parity); only
    // the merge goes through the stack's top.
    val filesVm: ChangesViewModel? = if (newest != null && newest.id != pr?.id) {
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${newest.id}") { it.create(newest.id) }
    } else {
        changesVm
    }
    val load by (filesVm?.load ?: remember { kotlinx.coroutines.flow.MutableStateFlow<ChangesLoadState?>(null) })
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
    LaunchedEffect(throughError) {
        val message = throughError ?: return@LaunchedEffect
        // The top member's VM IS the capsule's: its refusal toasts below.
        if (throughVm !== changesVm) toaster.failedAction("merge", "the stack", message)
    }
    if (throughConfirmOpen && newest != null && throughPr != null && throughVm != null) {
        PullRequestStack.stackConfirm(newest, stackPool, through = throughPr)?.let { confirm ->
            StackMergeDialog(
                confirm = confirm,
                onConfirm = { throughVm.mergePr(stack = true) },
                onDismiss = { throughConfirmOpen = false },
            )
        }
    }
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
            onRetry = filesVm?.let { vm -> vm::refresh },
        )
    } else {
        GuideFace(
            padding = padding,
            groups = groups,
            files = files,
            numbered = true,
            stack = guideStack,
            merge = merge,
            onOpenSection = if (filesVm != null) {
                { key: GuideSectionKey -> sectionName = sectionKeyName(key) }
            } else {
                null
            },
        )
    }
}

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
