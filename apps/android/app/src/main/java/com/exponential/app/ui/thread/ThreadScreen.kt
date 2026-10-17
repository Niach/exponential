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
import com.exponential.app.domain.AskDock
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
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.session.SessionLiveTail
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
 * file an issue) · the session chips (EXP-1343: each a page = its raw work + composer) ·
 * Guide. Conversation = the root's rows (THE session card per session, the
 * person's bubbles, the agent's prose, THE session view's question / plan
 * card answered in place, subagent / picture / event rows) over THE composer
 * box; an issue page = that issue's [IssueScreen] embedded; Guide = the root's Guide with
 * the Merge capsule of its PR. An AGENT channel is the same screen with the
 * agent's icon + name (settings affordance) and the conversation only, its
 * messages as comment rows. A live session page puts Stop in the header.
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
    val startDevices by viewModel.startDevices.collectAsStateWithLifecycle()
    val suggestionPending by viewModel.suggestionPending.collectAsStateWithLifecycle()
    val liveActivity by viewModel.liveActivity.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    // EXP-1323: opening the thread reads it; news landing while it is open
    // is read too (the row never turns bold under the reader).
    val newestAgentMessage = remember(messages) {
        messages.filter { it.authorKind == DomainContract.messageAuthorKindAgent }.maxOfOrNull { it.createdAt }
    }
    LaunchedEffect(viewModel.rootId, newestAgentMessage) { viewModel.markRead() }

    var selectedKey by rememberSaveable { mutableStateOf(viewModel.initialTab ?: "conversation") }
    var menuFor by remember { mutableStateOf<ThreadMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var closePrConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }
    var stopConfirmFor by remember { mutableStateOf<String?>(null) }
    val tree by viewModel.tree.collectAsStateWithLifecycle()

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
    val issueTitles = remember(syncedIssues) { syncedIssues.associate { it.id to it.title } }
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
    // EXP-1347: every issue ref inside the thread (its pages, nested issue
    // tabs and embedded session pages included) takes this one handler.
    val openIssueHere: (String) -> Unit = { id ->
        if (statusById.containsKey(id)) selectedKey = "issue:$id" else currentOnOpenIssue(id)
    }
    val issueRefHandler = remember(linkCandidates, statusById) {
        IssueRefHandler(linkCandidates, searchServer = viewModel::searchIssues) { target -> openIssueHere(target.issueId) }
    }

    // THE ask dock (ask-dock.json): every card waiting in the tree (any
    // session's plan with no decision, unanswered question) leaves the
    // Conversation and takes the composer's slot; the asking session's name
    // is the caption (the dispatcher's own = none).
    val sessionName: (String) -> String? = remember(view) { AskDock.threadSessionNames(view?.tabs.orEmpty()) }
    val askPending = remember(view, sessionName) {
        view?.let { v -> AskDock.threadPendingAsks(v.rows, sessionName) }.orEmpty()
    }
    val onAnswer: ThreadAnswer = { id, answer, settled -> viewModel.answer(id, answer, settled) }
    val onDecidePlan: ThreadPlanDecision = { id, approved, fresh, feedback, settled ->
        viewModel.decidePlan(id, approved, fresh, feedback, settled)
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
                onAnswer = onAnswer,
                onDecidePlan = onDecidePlan,
                dockedIds = remember(askPending) { askPending.mapTo(mutableSetOf()) { it.id } },
                issueTitle = { id -> issueTitles[id] },
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
                        // The composer yields to ANY waiting card of the tree.
                        askPending = askPending.isNotEmpty(),
                        dock = {
                            ThreadAskDock(
                                rows = view?.rows.orEmpty(),
                                sessionName = sessionName,
                                onAnswer = onAnswer,
                                onDecidePlan = onDecidePlan,
                            )
                        },
                    )
                },
                issuePage = { issueId, padding ->
                    ThreadIssuePage(
                        issueId = issueId,
                        padding = padding,
                        onBack = onBack,
                        onOpenIssue = openIssueHere,
                        onOpenThread = { id -> if (id != viewModel.rootId) onOpenThread(id) },
                        onMessage = { text -> viewModel.send(text) },
                    )
                },
                // EXP-1343: a session chip's page = the session view's body
                // (raw work + its composer) under the thread's ONE header.
                sessionPage = { sessionId, padding ->
                    com.exponential.app.ui.session.SessionPage(
                        sessionId = sessionId,
                        padding = padding,
                        onOpenIssue = openIssueHere,
                    )
                },
                sessionStop = { sessionId ->
                    // A shared session lets every member Stop; a private one
                    // only its starter (the server re-checks).
                    tree.firstOrNull { it.id == sessionId }
                        ?.takeIf { !it.isEnded && (it.shared || it.userId == me) }
                        ?.let { { stopConfirmFor = sessionId } }
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
                onOpenSession = { id -> if (viewModel.isOtherRoot(id)) onOpenThread(id) else onOpenSession(id) },
                suggestions = ThreadSuggestionActions(
                    onStart = { rowId, itemId, deviceId, model -> viewModel.startSuggestion(rowId, itemId, deviceId, model) },
                    onStartAll = viewModel::startAllSuggestions,
                    onHere = viewModel::suggestionHere,
                    onDismiss = viewModel::dismissSuggestion,
                    onFile = viewModel::fileSuggestion,
                    devices = startDevices.map { SuggestionDevice(it.deviceId, it.deviceLabel, deviceIcon(it)) },
                    // Contract `codingModel` (fable · opus · sonnet · haiku), never a Default row.
                    models = DomainContract.codingModelValues.map { it to modelLabel(it) },
                    pending = suggestionPending,
                ),
                tail = liveActivity?.let { activity -> { SessionLiveTail(activity, root) } },
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
    stopConfirmFor?.let { sessionId ->
        com.exponential.app.ui.components.PromptAlert(
            prompt = Prompts.StopRun.prompt(),
            onDismiss = { stopConfirmFor = null },
            handlers = mapOf(
                "stop" to {
                    stopConfirmFor = null
                    viewModel.stop(sessionId)
                },
            ),
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
    onAdd: (() -> Unit)?,
    onIssueMenu: ((SessionTree.ThreadTab.IssueTab) -> Unit)?,
    onAgentSettings: (() -> Unit)?,
    /** THE floating reply box, folded or expanded. */
    composer: @Composable (expanded: Boolean, onExpandedChange: (Boolean) -> Unit) -> Unit,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    guidePage: @Composable (padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-1343: a session chip's page = that session's raw work + its
     *  composer (the session view's body, no own header). */
    sessionPage: @Composable (sessionId: String, padding: PaddingValues) -> Unit = { _, _ -> },
    /** EXP-1343: the header's Stop while the shown session page is live and
     *  the viewer may stop it; null = no Stop for that session. */
    sessionStop: (sessionId: String) -> (() -> Unit)? = { null },
    /** EXP-1307: the header `…` (Close PR, EXP-1338) while the Guide tab is
     *  selected; the slot draws nothing without an entry. */
    guideMenu: (@Composable () -> Unit)? = null,
    /** EXP-1323: a report line / a started suggestion opens that session. */
    onOpenSession: ((sessionId: String) -> Unit)? = null,
    /** EXP-1323: the suggested tasks cards' actions (null = read-only). */
    suggestions: ThreadSuggestionActions? = null,
    /** EXP-1323: the live tail above the composer (Tasks card + status line). */
    tail: (@Composable () -> Unit)? = null,
    /** EXP-1334: an issue's title (the strip's full tier). */
    issueTitle: (String) -> String? = { null },
    /** THE ask dock: the waiting cards' ids (drawn in the composer's slot). */
    dockedIds: Set<String> = emptySet(),
) {
    // An agent channel is the conversation only. EXP-1334: a folded
    // sub-issue is a page too (right after its chip's). EXP-1343: phones
    // page through EVERY strip tab in strip order, the session chips (and
    // their folded sessions) included.
    val pages = remember(agent != null, view?.tabs) { threadPages(if (agent != null) null else view?.tabs) }
    // A tab that left the strip (an issue unlinked elsewhere) falls back to
    // the conversation.
    val selected = pages.firstOrNull { threadTabKey(it) == selectedKey } ?: SessionTree.ThreadTab.Conversation
    val pageKeys = remember(pages) { pages.mapTo(mutableSetOf()) { threadTabKey(it) } }
    // EXP-1343: a session of this thread with a page opens AS that page; any
    // other (another root's card) keeps its own route.
    val openSessionHere: (String) -> Boolean = { id ->
        val key = "session:$id"
        if (key in pageKeys) {
            onSelectKey(key)
            true
        } else {
            false
        }
    }
    val shownStop = (selected as? SessionTree.ThreadTab.SessionTab)?.let { sessionStop(it.sessionId) }
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
                    // EXP-1343: ONE header for every page; a live session
                    // page puts its Stop in the trailing slot.
                    verb = if (shownStop != null) com.exponential.app.ui.work.WorkBarVerb.Stop else null,
                    verbEnabled = shownStop != null,
                    onVerb = { shownStop?.invoke() },
                    // EXP-1316: the `…` rides every tab ×4 while the Guide has a PR.
                    menu = guideMenu,
                    tabs = if (view != null) {
                        {
                            ThreadTabStrip(
                                tabs = view.tabs,
                                selectedKey = threadTabKey(selected),
                                onSelect = onSelectKey,
                                onAdd = onAdd,
                                issueStatus = issueStatus,
                                issueTitle = issueTitle,
                                onIssueMenu = onIssueMenu,
                                onOpenSession = { id -> openSessionHere(id) },
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
            // EXP-1343: only the shown page is composed at rest (a drag
            // composes its neighbour), and each page owns its view models:
            // they die with the page instead of piling up per swipe.
            beyondViewportPageCount = 0,
            pageViewModels = true,
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
                            onOpenCard = { card -> if (!openSessionHere(card.sessionId)) onOpenCard(card) },
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
                            onOpenSession = onOpenSession?.let { open -> { id -> if (!openSessionHere(id)) open(id) } },
                            suggestions = suggestions,
                            tail = tail,
                            dockedIds = dockedIds,
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
                is SessionTree.ThreadTab.SessionTab -> sessionPage(tab.sessionId, padding)
                SessionTree.ThreadTab.Add -> Unit
            }
        }
    }
}

/**
 * EXP-1343: the phone pager's pages = the strip's tabs in strip order, each
 * folded issue / session right after its chip; null (an agent channel, or
 * nothing loaded) = the conversation alone.
 */
internal fun threadPages(tabs: List<SessionTree.ThreadTab>?): List<SessionTree.ThreadTab> {
    if (tabs == null) return listOf(SessionTree.ThreadTab.Conversation)
    return tabs.flatMap { tab ->
        when (tab) {
            SessionTree.ThreadTab.Add -> emptyList()
            is SessionTree.ThreadTab.IssueTab ->
                listOf(tab) + tab.nested.map { SessionTree.ThreadTab.IssueTab(it.issueId, it.identifier) }
            is SessionTree.ThreadTab.SessionTab ->
                listOf(tab) + tab.nested.map { SessionTree.ThreadTab.SessionTab(it.sessionId, it.name, it.markState) }
            else -> listOf(tab)
        }
    }.distinctBy(::threadTabKey)
}

/** EXP-1307: the Guide `…` for [pr] — [SessionTree.guideMenuEntries] over
 *  its state and the viewer's membership. */
@Composable
private fun GuideMenu(pr: PullRequestEntity, member: Boolean, onClosePr: () -> Unit) {
    ThreadGuideMenuButton(
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
