package com.exponential.app.ui.thread

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.draw.clip
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
import androidx.compose.ui.unit.Dp
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
import com.exponential.app.domain.GuideSectionKey
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
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
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
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
    /** The "+"'s New issue: the New issue page on [boardId], linked to this
     *  root on create. Null hides the row. */
    onNewIssue: ((boardId: String, rootSessionId: String) -> Unit)? = null,
    /** The agent menu's Agent settings (owners). */
    onOpenAgentSettings: ((agentId: String) -> Unit)? = null,
    viewModel: ThreadViewModel = hiltViewModel(),
) {
    val view by viewModel.view.collectAsStateWithLifecycle()
    val root by viewModel.root.collectAsStateWithLifecycle()
    val isAgent by viewModel.isAgent.collectAsStateWithLifecycle()
    val messages by viewModel.messages.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val teamUsers by viewModel.teamUsers.collectAsStateWithLifecycle()
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
    val notice by viewModel.notice.collectAsStateWithLifecycle()
    val noticeToaster = LocalToaster.current
    LaunchedEffect(notice) {
        val text = notice ?: return@LaunchedEffect
        noticeToaster.warning(text)
        viewModel.clearNotice()
    }
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val deviceLabel by viewModel.rootDeviceLabel.collectAsStateWithLifecycle()
    var deleteConfirm by remember { mutableStateOf(false) }

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
    // #EXP-1380: one instance per change of its inputs, so the transcript's
    // rows skip recomposition while the screen's other state moves.
    val metaFor: (SessionTree.ThreadRow.MessageRow) -> ThreadRowMeta? = remember(users, messagesById) {
        { row ->
            val user = row.authorId?.let { users[it] }
            ThreadRowMeta(
                authorName = if (row.author == "user") userDisplayName(user, row.authorId) else "",
                time = messagesById[row.id]?.createdAt?.let(::clockTime).orEmpty(),
                user = user,
            )
        }
    }
    val rootRow = root
    // NEW-2: the agent's DEFINITION row names the header (its coloured icon +
    // name); NEW-8: the only source (the session's legacy facets are gone).
    val agentDefinition by viewModel.agentDefinition.collectAsStateWithLifecycle()
    // S-3: the agents' marks the inter-session links draw.
    val agentMarks by viewModel.agentMarks.collectAsStateWithLifecycle()
    val agent = if (isAgent && rootRow != null) {
        val def = agentDefinition
        ThreadAgent(
            name = def?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS,
            icon = def?.icon,
            color = def?.color,
            assistant = def?.isAssistant == true,
        )
    } else {
        null
    }
    val placeholder = agent?.let { ThreadUiCopy.messageAgent(it.name) }
        ?: view?.composer?.placeholder
        ?: SessionTree.ThreadCopy.PLACEHOLDER

    // EXP-1307: the Guide's ONE PR behind the header `…`.
    val guidePr = SessionTree.guidePullRequest(pullRequests)

    // Thread 58cb32df (chats.json): THE chat header — the lead disc, the
    // subtitle, the Stop pill (= the menu's Stop: `sessions.end` on the
    // ROOT, the whole tree; shown while the root is not ended; an agent
    // chat has none) and the `…` (the chat's / the agent's entries).
    val canSteer = rootRow != null && (rootRow.shared || rootRow.userId == me)
    val rootEnded = rootRow?.isEnded == true
    val chatLead = chatLeadOf(view, pullRequests)
    val headerLead: @Composable () -> Unit = {
        if (agent != null) {
            Box(
                modifier = Modifier
                    .size(44.dp)
                    .androidxClipCircle()
                    .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted, androidx.compose.foundation.shape.CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                // S-3: the viewer's Assistant wears the brand mark.
                if (agent.assistant) {
                    com.exponential.app.ui.components.ExponentialMark(size = 22.dp, tint = MaterialTheme.colorScheme.onSurface)
                } else {
                    AgentGlyph(icon = agent.icon, color = agent.color, size = 22.dp)
                }
            }
        } else {
            com.exponential.app.ui.components.ChatLeadDisc(lead = chatLead, agent = rootRow?.cli, size = 44.dp)
        }
    }
    val headerSubtitle = if (agent != null) {
        listOfNotNull(AGENT_SUBTITLE, rootRow?.model?.takeIf { it.isNotBlank() }?.let(::modelLabel)).joinToString(" · ")
    } else {
        listOfNotNull(chatStateWord(view, rootEnded), deviceLabel).joinToString(" · ")
    }
    val headerSubtitleColor = if (agent == null && view?.needsYou == true) com.exponential.app.ui.issue.NeedsInputAmber else null
    val chatMenu: List<com.exponential.app.ui.components.ChatMenuItem> = if (agent != null) {
        com.exponential.app.ui.components.agentMenuItems(
            com.exponential.app.domain.Chats.agentMenuEntries(busy = rootRow?.busy == true, isOwner = isOwner)
                .filter { it != com.exponential.app.domain.Chats.AgentMenuEntry.Settings || onOpenAgentSettings != null },
        ) { entry ->
            when (entry) {
                com.exponential.app.domain.Chats.AgentMenuEntry.Compact,
                com.exponential.app.domain.Chats.AgentMenuEntry.NewConversation ->
                    com.exponential.app.domain.Chats.agentMenuSend(entry)?.let { viewModel.send(it) }
                com.exponential.app.domain.Chats.AgentMenuEntry.Settings ->
                    rootRow?.agentId?.let { id -> onOpenAgentSettings?.invoke(id) }
            }
        }
    } else if (rootRow != null) {
        com.exponential.app.ui.components.chatMenuItems(
            com.exponential.app.domain.Chats.threadMenuEntries(rootRow.status, canSteer, rootRow.userId == me || isOwner),
            onDelete = { deleteConfirm = true },
        )
    } else {
        emptyList()
    }
    val guideMenuItems = guidePr?.let { pr ->
        SessionTree.guideMenuEntries(state = pr.state, member = isMember).map { entry ->
            when (entry) {
                SessionTree.GuideMenuEntry.ClosePr -> com.exponential.app.ui.components.ChatMenuItem(
                    label = DomainContract.diffUiClosePr,
                    icon = ExpIcons.prClosed,
                    onClick = { closePrConfirm = pr },
                    destructive = true,
                    testTag = "thread-guide-menu-close-pr",
                )
            }
        }
    }.orEmpty()

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

    // N-12: THE Dock (dock.json): every card waiting in the tree (any
    // session's plan with no decision, unanswered question) leaves the
    // Conversation and floats as a DockCard over the composer; the asking
    // session's name is the caption (the dispatcher's own = none).
    val sessionName: (String) -> String? = remember(view) { AskDock.threadSessionNames(view?.tabs.orEmpty()) }
    // The suggested-tasks rows with an open item ride the Dock too.
    val dockedIds = remember(view, sessionName) { threadDockedIds(view?.rows.orEmpty(), sessionName) }
    val openThreadSession: (String) -> Unit = { id -> if (viewModel.isOtherRoot(id)) onOpenThread(id) else onOpenSession(id) }
    val suggestionActions = ThreadSuggestionActions(
        onStart = { rowId, itemId, deviceId, model -> viewModel.startSuggestion(rowId, itemId, deviceId, model) },
        onStartAll = viewModel::startAllSuggestions,
        onHere = viewModel::suggestionHere,
        onDismiss = viewModel::dismissSuggestion,
        onFile = viewModel::fileSuggestion,
        devices = startDevices.map { SuggestionDevice(it.deviceId, it.deviceLabel, deviceIcon(it)) },
        // Contract `codingModel` (fable · opus · sonnet · haiku), never a Default row.
        models = DomainContract.codingModelValues.map { it to modelLabel(it) },
        pending = suggestionPending,
    )
    val onAnswer: ThreadAnswer = { id, answer, settled ->
        viewModel.answer(id, answer.answer, settled, answer.answers, answer.surfaceData)
    }
    val onDecidePlan: ThreadPlanDecision = { id, d, settled ->
        viewModel.decidePlan(id, d.approved, d.fresh, d.feedback, settled, d.answers, d.surfaceData)
    }

    // Inline `@email` mention pills in the chat rows: the team's synced
    // members (issue detail's resolver); unknown addresses stay plain text.
    val mentionResolver = remember(teamUsers) {
        MentionResolver(teamUsers.map { MentionMember(it.name ?: it.email, it.email) })
    }

    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
            LocalIssueRefs provides issueRefHandler,
            LocalMentions provides mentionResolver,
            com.exponential.app.ui.components.LocalAgentMarks provides agentMarks,
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
                dockedIds = dockedIds,
                issueTitle = { id -> issueTitles[id] },
                canUnqueue = { row -> row.authorId != null && row.authorId == me },
                onUnqueue = viewModel::unqueue,
                onBack = onBack,
                onAdd = { menuFor = ThreadMenuTarget(null) },
                onIssueMenu = { menuFor = ThreadMenuTarget(it) },
                // THE session composer (dispatcher roots): the session view's
                // box targeting the ROOT, fed by the root's live state.
                composer = { expanded, onExpandedChange ->
                    SessionComposer(
                        viewModel = rootSession,
                        expanded = expanded,
                        onExpandedChange = onExpandedChange,
                        placeholder = placeholder,
                        capsuleTestTag = "thread-composer-capsule",
                        // S-3: an agent chat's composer = surface `agent`
                        // (its + carries Model ›).
                        surface = if (agent != null) {
                            com.exponential.app.domain.ComposerMenuSurface.Agent
                        } else {
                            com.exponential.app.domain.ComposerMenuSurface.Session
                        },
                        // Queued rows already sit in the Conversation (EXP-1317).
                        showQueue = false,
                        onOpenUsage = { usageSheetOpen = true },
                        modifier = Modifier.testTag(SessionTree.ThreadCopy.TEST_ID_COMPOSER),
                        // N-12: THE Dock: the tree's waiting cards, then the
                        // root's Queue and banners (its queued rows sit in the
                        // Conversation), over the composer that never leaves.
                        dock = { sessionDock ->
                            ThreadDock(
                                rows = view?.rows.orEmpty(),
                                autoOpenReady = !view?.rows.isNullOrEmpty(),
                                sessionName = sessionName,
                                onAnswer = onAnswer,
                                onDecidePlan = onDecidePlan,
                                sources = sessionDock.sources,
                                sessionSheet = sessionDock.sheet,
                                action = sessionDock.action,
                                suggestions = suggestionActions,
                                onOpenSession = openThreadSession,
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
                        onOpenOrigin = { id, isRoot -> if (isRoot) onOpenThread(id) else onOpenSession(id) },
                    )
                },
                headerLead = headerLead,
                headerSubtitle = headerSubtitle,
                headerSubtitleColor = headerSubtitleColor,
                menu = chatMenu,
                menuGroups = listOf(guideMenuItems),
                // NEW-69: an agent's conversation carries the telephone.
                headerCall = agent?.let { a -> { com.exponential.app.ui.call.AgentCallButton(agentId = rootRow?.agentId, agentName = a.name) } },
                onSurfaceReply = { text -> viewModel.send(text) },
                guidePage = { padding ->
                    ThreadGuidePage(
                        guideRaw = guide,
                        pullRequests = pullRequests,
                        teamPullRequests = teamPullRequests,
                        pullRequestIssues = pullRequestIssues,
                        issues = syncedIssues,
                        padding = padding,
                        onOpenThread = { id -> if (id != viewModel.rootId) onOpenThread(id) },
                    )
                },
                onOpenSession = openThreadSession,
                // S-3: an inter-session link PUSHES that session (another
                // root as its thread, any other session as its view).
                onOpenXref = openThreadSession,
                suggestions = suggestionActions,
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
    if (deleteConfirm) {
        com.exponential.app.ui.components.ChatDeleteConfirm(
            onConfirm = {
                deleteConfirm = false
                // After a delete the chat's screen closes back to the list.
                viewModel.deleteChat(onDeleted = onBack)
            },
            onDismiss = { deleteConfirm = false },
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

/** The agent chat's subtitle lead (chats.json draft: `Agent · <model>`). */
private const val AGENT_SUBTITLE = "Agent"

private fun Modifier.androidxClipCircle(): Modifier = this.then(Modifier.clip(androidx.compose.foundation.shape.CircleShape))

/** The chat header's lead (chats.json `lead`, the list row's rule): needs
 *  you, working, else the root's PR glyph, else the bare mark. */
internal fun chatLeadOf(view: SessionTree.ThreadView?, pullRequests: List<PullRequestEntity>): com.exponential.app.domain.Chats.ChatLead {
    val mark = SessionTree.threadRowMarkState(view?.needsYou == true, view?.busy == true)
    return when {
        mark == SessionTree.SessionState.NeedsYou -> com.exponential.app.domain.Chats.ChatLead.NeedsYou
        mark == SessionTree.SessionState.Working -> com.exponential.app.domain.Chats.ChatLead.Working
        pullRequests.any { it.state == DomainContract.prStateOpen } -> com.exponential.app.domain.Chats.ChatLead.Open
        pullRequests.any { it.state == DomainContract.prStateMerged } -> com.exponential.app.domain.Chats.ChatLead.Merged
        else -> com.exponential.app.domain.Chats.ChatLead.Waiting
    }
}

/** The chat header's state word (S-3's four): done, needs you, working…,
 *  else idle. */
internal fun chatStateWord(view: SessionTree.ThreadView?, ended: Boolean): String? = when {
    view == null -> null
    ended -> SessionTree.CardCopy.STATES.getValue(SessionTree.SessionState.Done)
    view.needsYou -> SessionTree.CardCopy.STATES.getValue(SessionTree.SessionState.NeedsYou)
    view.busy -> com.exponential.app.domain.Chats.Copy.PREVIEW_WORKING
    else -> SessionTree.CardCopy.STATES.getValue(SessionTree.SessionState.Idle)
}

/** A wire timestamp as the local `HH:mm` a message row shows. */
internal fun clockTime(wire: String): String? {
    val ms = WireTimestamps.parseEpochMs(wire) ?: return null
    return Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).format(DateTimeFormatter.ofPattern("HH:mm"))
}

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
    /** THE floating reply box, folded or expanded. */
    composer: @Composable (expanded: Boolean, onExpandedChange: (Boolean) -> Unit) -> Unit,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    guidePage: @Composable (padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-1343: a session chip's page = that session's raw work + its
     *  composer (the session view's body, no own header). */
    sessionPage: @Composable (sessionId: String, padding: PaddingValues) -> Unit = { _, _ -> },
    /** Thread 58cb32df (chats.json): THE chat header — its lead disc, the
     *  subtitle (state · device; an agent: `Agent` · its model) and the `…`
     *  entries (the chat's menu, then the Guide's Close PR). S-3: no Stop. */
    headerLead: @Composable () -> Unit = {},
    headerSubtitle: String? = null,
    headerSubtitleColor: Color? = null,
    menu: List<com.exponential.app.ui.components.ChatMenuItem> = emptyList(),
    menuGroups: List<List<com.exponential.app.ui.components.ChatMenuItem>> = emptyList(),
    /** A surface card's `reply` → the person's next message. */
    onSurfaceReply: ((String) -> Unit)? = null,
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
    /** NEW-69: the agent chat's telephone in the header (null = none). */
    headerCall: (@Composable () -> Unit)? = null,
    /** S-3: an inter-session link pushes that session (null = inert). */
    onOpenXref: ((sessionId: String) -> Unit)? = null,
) {
    // An agent channel is the conversation only. EXP-1334: a folded
    // sub-issue is a page too (right after its chip's). EXP-1343: phones
    // page through EVERY strip tab in strip order, the session chips (and
    // their folded sessions) included.
    // NEW-6 (N-1): a thread with ONE session (the root alone) IS that
    // session. NEW-46: the strip always carries the Conversation tab; on a
    // single-session thread its page IS the root's session page (one page,
    // the Conversation tab active while it shows). An agent conversation
    // stays the Conversation.
    val mainSessionId = if (agent != null) null else view?.mainSessionId
    val pages = remember(agent != null, view?.tabs, mainSessionId) {
        threadMainPages(threadPages(if (agent != null) null else view?.tabs), mainSessionId)
    }
    // A tab that left the strip (an issue unlinked elsewhere) falls back to
    // the first page: the conversation, or the main session's page.
    // NEW-46: "conversation" on a single-session thread = the main page.
    val pageKey = if (mainSessionId != null && selectedKey == "conversation") "session:$mainSessionId" else selectedKey
    val selected = pages.firstOrNull { threadTabKey(it) == pageKey }
        ?: pages.firstOrNull()
        ?: SessionTree.ThreadTab.Conversation
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
    var composerExpanded by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        modifier = modifier,
        containerColor = Color.Transparent,
        topBar = {
            ChatHeader(
                title = agent?.name ?: view?.title.orEmpty(),
                subtitle = headerSubtitle,
                subtitleColor = headerSubtitleColor,
                lead = headerLead,
                onBack = onBack,
                menu = menu,
                menuGroups = menuGroups,
                call = headerCall,
                // An agent's chat is the conversation only: no strip.
                tabs = if (agent == null && view != null) {
                    {
                        ThreadTabStrip(
                            tabs = view.tabs,
                            // NEW-46: the main session's page shows the
                            // Conversation tab selected.
                            selectedKey = threadTabKey(selected).let {
                                if (mainSessionId != null && it == "session:$mainSessionId") "conversation" else it
                            },
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
            // EXP-1343: each page owns its view models: they die with the
            // page instead of piling up per swipe. #EXP-1387: the neighbours
            // stay composed, an issue or session page as cheap skeleton rows
            // until it settles (session-thread.json: only the visible page
            // holds live models), so a swipe's first frame never builds one.
            beyondViewportPageCount = 1,
            pageViewModels = true,
            neighbour = { tab ->
                when (tab) {
                    is SessionTree.ThreadTab.IssueTab, is SessionTree.ThreadTab.SessionTab -> {
                        { ThreadPageSkeleton(topInset) }
                    }
                    else -> null
                }
            },
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
                            onSurfaceReply = onSurfaceReply,
                            origin = view.origin,
                            onOpenXref = onOpenXref,
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

/** #EXP-1387: an issue or session page while it is a neighbour (the iOS
 *  thread pager's stand-in): skeleton rows under the header band. */
@Composable
private fun ThreadPageSkeleton(topInset: Dp) {
    Box(Modifier.fillMaxSize().padding(top = topInset)) {
        SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("thread-page-skeleton"))
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

/**
 * NEW-46: [pages] with the single-session thread's Conversation page REPLACED,
 * in place, by the root's own session page (one page, never a duplicate);
 * null [mainSessionId] = the pages as they are.
 */
internal fun threadMainPages(
    pages: List<SessionTree.ThreadTab>,
    mainSessionId: String?,
): List<SessionTree.ThreadTab> {
    if (mainSessionId == null) return pages
    val main = SessionTree.ThreadTab.SessionTab(mainSessionId, name = "", markState = SessionTree.SessionState.Idle)
    return pages.map { if (it == SessionTree.ThreadTab.Conversation) main else it }.distinctBy(::threadTabKey)
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
 *  top bar, no `Threads` row, no Start; a composer seed carrying text goes
 *  to the thread's root as a message ([onMessage]). */
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
 * A section's Changes row opens its diff page. A refused merge toasts (the
 * server posts it into the root, whose agent takes it over, EXP-1391) and
 * offers Retry.
 */
@Composable
private fun ThreadGuidePage(
    guideRaw: String?,
    pullRequests: List<PullRequestEntity>,
    teamPullRequests: List<PullRequestEntity>,
    pullRequestIssues: List<PullRequestIssueEntity>,
    issues: List<IssueEntity>,
    padding: PaddingValues,
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
        val vm = throughVm ?: return@LaunchedEffect
        if (vm !== changesVm) toaster.failedAction("merge", "the stack", message, retry = vm::retryMerge)
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
    LaunchedEffect(actionError) {
        val message = actionError ?: return@LaunchedEffect
        val vm = changesVm ?: return@LaunchedEffect
        toaster.failedAction("merge", "the PR", message, retry = vm::retryMerge)
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
