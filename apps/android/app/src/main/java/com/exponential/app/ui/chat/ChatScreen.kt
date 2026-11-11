package com.exponential.app.ui.chat

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
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.Chats
import com.exponential.app.domain.PlanPage
import com.exponential.app.ui.plan.LocalOpenPlanPage
import com.exponential.app.ui.plan.OpenPlanPage
import com.exponential.app.ui.plan.PlanPageContent
import com.exponential.app.ui.plan.PlanPageState
import com.exponential.app.domain.WaitingCards
import com.exponential.app.domain.Dock
import com.exponential.app.ui.components.DockSheetBody
import com.exponential.app.domain.ClosePr
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.InboxRoute
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.session.SessionLiveTail
import com.exponential.app.ui.components.PageViewModelStore
import com.exponential.app.domain.SessionScreen
import androidx.activity.compose.BackHandler
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.IssueScreen
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.IssueSearchSheet
import com.exponential.app.ui.markdown.LocalAppLinks
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.markdown.rememberAppLinkHandler
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.MergeStatusPill
import com.exponential.app.ui.work.NeedsApprovalConfirm
import com.exponential.app.ui.work.MergeNowConfirm
import com.exponential.app.ui.work.STACK_CONFIRM_CANCEL_LABEL
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.PullRequestStack
import com.exponential.app.ui.work.TitleCollapseState
import dev.chrisbanes.haze.rememberHazeState
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * EXP-1296 (Model v3): THE conversation screen for a chat root
 * OR an agent root (`chat/{sessionId}`), `session-chat.json`.
 *
 * #EXP-1732 (session-screen.json): THE session screen, phones = NARROW. The
 * header band (back, the lead disc, the title over `status · device ·
 * trigger`, the Merge pill, THE gear for an agent / the Assistant, `…`) over
 * THE segmented tabs Chat · Issues · Sessions · Results (a muted count each).
 * Layout `chat` = the conversation (the root's rows over THE Dock + THE
 * composer; plans and questions inline), `tab` = the tab's body replacing it
 * (Issues = THE issue rows led by `+`, Sessions = THE tree rows, Results =
 * the tree's results under its PR block), `item` = a pushed page over the
 * tab (an issue's [IssueScreen] embedded, a session's raw work + composer, a
 * plan's [PlanPageContent]) with its `‹` back. The gear pushes the agent's
 * settings page.
 */
@Composable
fun ChatScreen(
    onBack: () -> Unit,
    onOpenSession: (sessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenChat: (rootSessionId: String) -> Unit,
    /** The "+"'s New issue: the New issue page on [boardId], linked to this
     *  root on create. Null hides the row. */
    onNewIssue: ((boardId: String, rootSessionId: String) -> Unit)? = null,
    /** The agent menu's Agent settings (owners). */
    onOpenAgentSettings: ((agentId: String) -> Unit)? = null,
    viewModel: ChatViewModel = hiltViewModel(),
) {
    val view by viewModel.view.collectAsStateWithLifecycle()
    val root by viewModel.root.collectAsStateWithLifecycle()
    val isAgent by viewModel.isAgent.collectAsStateWithLifecycle()
    val messages by viewModel.messages.collectAsStateWithLifecycle()
    val teamUsers by viewModel.teamUsers.collectAsStateWithLifecycle()
    val me by viewModel.currentUserId.collectAsStateWithLifecycle()
    val statuses by viewModel.statuses.collectAsStateWithLifecycle()
    val issues by viewModel.issues.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val linkCandidates by viewModel.linkCandidates.collectAsStateWithLifecycle()
    val newIssueBoardId by viewModel.newIssueBoardId.collectAsStateWithLifecycle()
    val pullRequests by viewModel.pullRequests.collectAsStateWithLifecycle()
    val teamPullRequests by viewModel.teamPullRequests.collectAsStateWithLifecycle()
    val pullRequestIssues by viewModel.pullRequestIssues.collectAsStateWithLifecycle()
    val syncedIssues by viewModel.syncedIssues.collectAsStateWithLifecycle()
    val isMember by viewModel.isMember.collectAsStateWithLifecycle()
    val startDevices by viewModel.startDevices.collectAsStateWithLifecycle()
    val suggestionPending by viewModel.suggestionPending.collectAsStateWithLifecycle()
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

    // EXP-1323: opening the chat reads it; news landing while it is open
    // is read too (the row never turns bold under the reader).
    val newestAgentMessage = remember(messages) {
        messages.filter { it.authorKind == DomainContract.messageAuthorKindAgent }.maxOfOrNull { it.createdAt }
    }
    LaunchedEffect(viewModel.rootId, newestAgentMessage) { viewModel.markRead() }

    // #EXP-1732 (session-screen.json): the requested tab (`?tab=`, legacy
    // values map through `legacyTabs`) and the opened item (`open` + its id;
    // `?plan=` opens that plan).
    var requestedTab by rememberSaveable { mutableStateOf(viewModel.initialTab) }
    var openKind by rememberSaveable {
        mutableStateOf(if (viewModel.initialPlan != null) SessionScreen.Open.Plan.wire else null)
    }
    var openId by rememberSaveable { mutableStateOf(viewModel.initialPlan) }
    val openItem: (SessionScreen.Open, String) -> Unit = { kind, id ->
        openKind = kind.wire
        openId = id
    }
    val closeItem: () -> Unit = {
        openKind = null
        openId = null
    }
    var menuFor by remember { mutableStateOf<ChatTabMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var closePrConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }
    // #EXP-1710: the `…`'s Merge now asks the shared `merge-now-pr` alert.
    var mergeNowConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }
    val tree by viewModel.tree.collectAsStateWithLifecycle()

    val statusById = remember(issues, statuses) {
        issues.associate { it.id to IssueStatusResolver.resolve(it, statuses) }
    }
    val messagesById = remember(messages) { messages.associateBy { it.id } }
    val issueTitles = remember(syncedIssues) { syncedIssues.associate { it.id to it.title } }
    // #EXP-1662 / #EXP-1732: the plan a row or a way in opened (`open: plan`),
    // any plan of the tree.
    val openedPlan = remember(view?.rows, openKind, openId, messagesById) {
        if (openKind == SessionScreen.Open.Plan.wire) chatOpenedPlan(view?.rows.orEmpty(), openId, messagesById) else null
    }
    // A plan of this tree opens in place as the pushed item (a session
    // page's row too); anything else takes the host's way in.
    val outerOpenPlan = LocalOpenPlanPage.current
    val latestMessages by rememberUpdatedState(messagesById)
    val latestOpenItem by rememberUpdatedState(openItem)
    val openPlanHere = remember(outerOpenPlan) {
        OpenPlanPage { sessionId, messageId ->
            if (latestMessages.containsKey(messageId)) {
                latestOpenItem(SessionScreen.Open.Plan, messageId)
            } else {
                outerOpenPlan.open(sessionId, messageId)
            }
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
        ChatAgent(
            name = def?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS,
            icon = def?.icon,
            color = def?.color,
            assistant = def?.isAssistant == true,
        )
    } else {
        null
    }
    // EXP-1307: the Guide's ONE PR behind the header `…`.
    val guidePr = SessionTree.guidePullRequest(pullRequests)

    // Session 58cb32df (chats.json): THE chat header — the lead disc, the
    // subtitle, the Stop pill (= the menu's Stop: `sessions.end` on the
    // ROOT, the whole tree; shown while the root is not ended; an agent
    // chat has none) and the `…` (the chat's / the agent's entries).
    val canSteer = rootRow != null && (rootRow.shared || rootRow.userId == me)
    val chatLead = chatLeadOf(view, pullRequests)
    val headerLead: @Composable () -> Unit = {
        if (agent != null) {
            Box(
                modifier = Modifier
                    .size(com.exponential.app.domain.Chats.Shell.ChatHeader.DISC.dp)
                    .androidxClipCircle()
                    .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted, androidx.compose.foundation.shape.CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                // S-3: the viewer's Assistant wears the brand mark.
                if (agent.assistant) {
                    com.exponential.app.ui.components.ExponentialMark(size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC_GLYPH.dp, tint = MaterialTheme.colorScheme.onSurface)
                } else {
                    AgentGlyph(icon = agent.icon, color = agent.color, size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC_GLYPH.dp)
                }
            }
        } else {
            com.exponential.app.ui.components.ChatLeadDisc(lead = chatLead, agent = rootRow?.cli, size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC.dp)
        }
    }
    // #EXP-1732 (session-screen.json `subtitle`): `status · device · trigger`
    // (an agent's first enabled trigger's summary), blanks dropped.
    val trigger = remember(agentDefinition?.triggers, agent != null) {
        if (agent == null) {
            null
        } else {
            com.exponential.app.domain.parseAgentTriggers(agentDefinition?.triggers)
                .firstOrNull { it.enabled }
                ?.let { com.exponential.app.domain.AgentSettings.triggerSummary(it) }
        }
    }
    val stateWord = chatStateWord(viewModel.rootId, tree)
    val headerSubtitle = SessionScreen.subtitle(stateWord, deviceLabel, trigger)
    val headerSubtitleColor = chatStateTone(stateWord)
    val chatMenu: List<com.exponential.app.ui.components.ChatMenuItem> = if (agent != null) {
        com.exponential.app.ui.components.agentMenuItems(
            com.exponential.app.domain.Chats.agentMenuEntries(busy = rootRow?.busy == true, isOwner = isOwner)
                // chats.json `agentMenu` locks Settings; it opens exactly what
                // THE gear opens (#EXP-1732: the agent's settings page).
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
            com.exponential.app.domain.Chats.chatMenuEntries(rootRow.status, canSteer, rootRow.userId == me || isOwner),
            onDelete = { deleteConfirm = true },
        )
    } else {
        emptyList()
    }
    // #EXP-1611: the Guide PR's CI checks (the `…`'s Checks submenu, the
    // Merge pill's failing confirm); a 1s clock while one runs.
    val guideCheckRows by viewModel.guideChecks.collectAsStateWithLifecycle()
    val guideChecks = remember(guideCheckRows) { guideCheckRows.map(com.exponential.app.domain.PrChecks::of) }
    val checksNow = com.exponential.app.ui.components.rememberTickingNow(
        guideChecks.any { it.status == DomainContract.prCheckStatusInProgress },
    )
    val uriHandler = androidx.compose.ui.platform.LocalUriHandler.current
    // #EXP-1710 (merge-train.json MENU, `mergeMenuEntries`): checks, Merge
    // now (owners, the `merge-now-pr` alert), Hold | Resume auto-merge
    // (members), Close PR.
    val guideChecksItem = prChecksMenuItem(guideChecks, checksNow) { url -> runCatching { uriHandler.openUri(url) } }
    val guideMenuItems = guidePr?.let { pr ->
        com.exponential.app.domain.MergeTrain.mergeMenuEntries(
            state = pr.state,
            member = isMember,
            owner = isOwner,
            checks = guideChecks.size,
            mergeState = com.exponential.app.domain.MergeTrain.prMergeState(pr.state, pr.mergeState),
        ).mapNotNull { entry ->
            when (entry) {
                com.exponential.app.domain.MergeTrain.MenuEntry.Checks -> guideChecksItem
                com.exponential.app.domain.MergeTrain.MenuEntry.MergeNow -> com.exponential.app.ui.components.ChatMenuItem(
                    label = com.exponential.app.domain.MergeTrain.Copy.Menu.MERGE_NOW,
                    icon = ExpIcons.prMerged,
                    onClick = { mergeNowConfirm = pr },
                    testTag = com.exponential.app.domain.MergeTrain.TestIds.MERGE_NOW,
                )
                com.exponential.app.domain.MergeTrain.MenuEntry.Hold -> com.exponential.app.ui.components.ChatMenuItem(
                    label = com.exponential.app.domain.MergeTrain.Copy.Menu.HOLD,
                    icon = ExpIcons.prHold,
                    onClick = { viewModel.holdPullRequest(pr.id, hold = true) },
                    testTag = com.exponential.app.domain.MergeTrain.TestIds.HOLD,
                )
                com.exponential.app.domain.MergeTrain.MenuEntry.Resume -> com.exponential.app.ui.components.ChatMenuItem(
                    label = com.exponential.app.domain.MergeTrain.Copy.Menu.RESUME,
                    icon = ExpIcons.prResume,
                    onClick = { viewModel.holdPullRequest(pr.id, hold = false) },
                    testTag = com.exponential.app.domain.MergeTrain.TestIds.RESUME,
                )
                com.exponential.app.domain.MergeTrain.MenuEntry.ClosePr -> com.exponential.app.ui.components.ChatMenuItem(
                    label = DomainContract.diffUiClosePr,
                    icon = ExpIcons.prClosed,
                    onClick = { closePrConfirm = pr },
                    destructive = true,
                    testTag = "chat-guide-menu-close-pr",
                )
            }
        }
    }.orEmpty()

    // #EXP-1732 (session-screen.json): the Issues / Sessions tabs' rows and
    // THE screen (kind, tabs + counts, the active tab, the opened item).
    // Sessions = EVERY session under the root, off the raw tree (never the
    // view's visible subset); Issues = the view's tree of issues (an agent's
    // view carries none: its linked issues).
    val isAgentChat = agent != null
    val items = remember(view?.tabs, isAgentChat, tree, issues) {
        val sessions = sessionScreenTreeItems(viewModel.rootId, tree.map(SessionTree::session), emptyList()).sessions
        val treeIssues = if (isAgentChat) {
            issues.map { SessionScreenIssue(it.id, it.identifier, 0) }
        } else {
            sessionScreenItems(view?.tabs.orEmpty(), null).issues
        }
        SessionScreenItems(treeIssues, sessions)
    }
    val resultsCount = remember(tree, messages, viewModel.rootId) { chatResultsCount(viewModel.rootId, tree, messages) }
    val kind = SessionScreen.kind(
        agentId = if (agent != null) rootRow?.agentId else null,
        // An app's editor session renders as AppEditorScreen (its Preview /
        // Data / Versions bodies live there); a chat route reads it as its agent.
        appId = null,
        assistant = agent?.assistant == true,
    )
    val screen = SessionScreen.screen(
        kind = kind,
        wide = false,
        requested = requestedTab,
        open = openKind,
        counts = SessionScreen.Counts(issues = items.issues.size, sessions = items.sessions.size, results = resultsCount),
    )
    // A requested `settings` (the gear's item) is the agent's settings PAGE
    // on a phone: push it once and forget the request.
    LaunchedEffect(screen.open) {
        if (screen.open == SessionScreen.Open.Settings) {
            closeItem()
            if (requestedTab == SessionScreen.Open.Settings.wire) requestedTab = null
            rootRow?.agentId?.takeIf { it.isNotEmpty() }?.let { id -> onOpenAgentSettings?.invoke(id) }
        }
    }

    // EXP-760 ×4: a `#IDENTIFIER` (and, in the conversation, a BARE one) of a
    // synced same-team issue is a pill; one of this tree opens in place.
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    // EXP-1347: every issue ref inside the chat (its pages, nested issues and
    // embedded session pages included) takes this one handler.
    val openIssueHere: (String) -> Unit = { id ->
        if (statusById.containsKey(id) || items.issue(id) != null) openItem(SessionScreen.Open.Issue, id) else currentOnOpenIssue(id)
    }
    val issueRefHandler = remember(linkCandidates, statusById, items) {
        IssueRefHandler(linkCandidates, searchServer = viewModel::searchIssues) { target -> openIssueHere(target.issueId) }
    }

    // #EXP-1464 (chat-row-action.json `open`): a session opens where it
    // lives (inbox-route.json `sessionTarget`): my agent conversation root =
    // its channel, a chat root = its chat, a chat's child or anything under
    // an agent = its own session view (a Dock suggestion's started session).
    val openChatSession: (String) -> Unit = { id ->
        when (val target = if (id == viewModel.rootId) null else viewModel.sessionTarget(id)) {
            is InboxRoute.SessionTarget.Agent -> onOpenChat(id)
            is InboxRoute.SessionTarget.Chat ->
                target.sessionId?.let(onOpenSession) ?: onOpenChat(target.chatId)
            is InboxRoute.SessionTarget.Session -> onOpenSession(target.sessionId)
            null -> if (viewModel.isOtherRoot(id)) onOpenChat(id) else onOpenSession(id)
        }
    }
    // EXP-1323: the root's suggested tasks (THE Dock card while one is open:
    // Start, Start all, Do it here, Start on…, Start with…, File, Dismiss).
    val suggestionActions = ChatSuggestionActions(
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
    // #EXP-1545 (dock.json CHAT QUEUE): each Live child's Queue (its
    // per-session relay slots, LOCAL FIRST); the Dock shows ONE tasks card.
    val childDocks by viewModel.childDocks.collectAsStateWithLifecycle()
    val childQueues = HashMap<String, com.exponential.app.domain.TaskQueue?>()
    for ((id, child) in childDocks) {
        key(id) {
            childQueues[id] = com.exponential.app.ui.session.rememberSessionTaskQueue(
                child.activity.feed,
                child.activity.taskList,
                child.activity.backgroundTasks,
            ).takeIf { child.slots }
        }
    }
    val treeNames = remember(items) { items.sessions.associate { it.sessionId to it.name } }
    var childTodoOpen by rememberSaveable { mutableStateOf(true) }
    var childCompletedOpen by rememberSaveable { mutableStateOf(false) }
    val chatDock: (com.exponential.app.ui.session.SessionDock) -> com.exponential.app.ui.session.SessionDock = { sd ->
        val rootQueue = sd.sources.filterIsInstance<Dock.Source.Tasks>().firstOrNull()?.queue
        val candidates = tree.map { row ->
            val root = row.id == viewModel.rootId
            val queue = if (root) rootQueue else childQueues[row.id]
            Dock.ChatQueueCandidate(
                sessionId = row.id,
                name = treeNames[row.id] ?: row.title.orEmpty(),
                root = root,
                ended = row.isEnded,
                activeAt = (row.lastTurnAt ?: row.startedAt)?.let(com.exponential.app.domain.WireTimestamps::parseEpochMs),
                open = queue?.todo?.items?.isNotEmpty() == true,
            )
        }
        val chosen = Dock.chatDockQueue(candidates)
        when {
            chosen?.sessionId == viewModel.rootId -> sd
            chosen == null -> com.exponential.app.ui.session.SessionDock(
                sd.sources.filter { it !is Dock.Source.Tasks }, sd.sheet, sd.action,
            )
            else -> {
                val queue = childQueues[chosen.sessionId]
                com.exponential.app.ui.session.SessionDock(
                    sources = sd.sources.map { if (it is Dock.Source.Tasks) Dock.Source.Tasks(queue, chosen.sessionName) else it },
                    // The card's sheet opens THAT child's Trace.
                    sheet = { row, scope, close ->
                        if (row.kind == Dock.Kind.Tasks) {
                            DockSheetBody {
                                if (queue != null) {
                                    com.exponential.app.ui.components.TaskQueueView(
                                        queue = queue,
                                        todoOpen = childTodoOpen,
                                        onTodoOpenChange = { childTodoOpen = it },
                                        completedOpen = childCompletedOpen,
                                        onCompletedOpenChange = { childCompletedOpen = it },
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                }
                            }
                        } else {
                            sd.sheet(this, row, scope, close)
                        }
                    },
                    action = sd.action,
                )
            }
        }
    }
    // A viewer who cannot steer the chat reads the suggestions (no actions):
    // every card then sits inline, none on the Dock.
    val dockSuggestions = if (canSteer) suggestionActions else null
    // EXP-1323 / #EXP-1732: a suggestions card with an open item (and actions)
    // rides the Dock; every other one (settled: the started session's card,
    // `Sent here`, `Filed EXP-9`) sits inline at the END of the root's
    // transcript, or under its empty-feed placeholder.
    val inlineSuggestions = remember(view?.rows, dockSuggestions != null) {
        val rows = view?.rows.orEmpty()
        val docked = if (dockSuggestions != null) chatSuggestionSources(rows).map { it.id }.toSet() else emptySet()
        rows.filterIsInstance<SessionTree.SessionChatRow.Suggestions>().filter { it.id !in docked }
    }
    val rootFooter: (@Composable () -> Unit)? = if (inlineSuggestions.isEmpty()) {
        null
    } else {
        {
            Column(verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(8.dp)) {
                inlineSuggestions.forEach { row ->
                    key(row.id) { ChatSuggestionsCard(row = row, actions = dockSuggestions, onOpenSession = openChatSession) }
                }
            }
        }
    }
    // #EXP-1732: the Chat tab's Dock = the root's own (its Queue, else a Live
    // child's) plus the root's suggested tasks, over THE composer.
    val rootDock: @Composable (com.exponential.app.ui.session.SessionDock) -> Unit = { sd ->
        val sessionDock = chatDock(sd)
        ChatDock(
            rows = view?.rows.orEmpty(),
            sources = sessionDock.sources,
            sessionSheet = sessionDock.sheet,
            action = sessionDock.action,
            suggestions = dockSuggestions,
            onOpenSession = openChatSession,
        )
    }
    val onDecidePlan: ChatPlanDecision = { id, d, settled ->
        viewModel.decidePlan(id, d.approved, d.fresh, d.feedback, settled, d.answers, d.surfaceData)
    }

    // Inline `@email` mention pills in the chat rows: the team's synced
    // members (issue detail's resolver); unknown addresses stay plain text.
    val mentionResolver = remember(teamUsers) {
        MentionResolver(teamUsers.map { MentionMember(it.name ?: it.email, it.email) })
    }

    // #EXP-1674: links in a chat's prose open like everywhere else.
    val appLinkHandler = rememberAppLinkHandler(root?.teamId)
    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            // #EXP-1561 (waiting-cards.json DISMISS): every waiting card's ×.
            com.exponential.app.ui.session.LocalCardDismiss provides viewModel::dismissCard,
            LocalOpenPlanPage provides openPlanHere,
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
            LocalIssueRefs provides issueRefHandler,
            LocalMentions provides mentionResolver,
            LocalAppLinks provides appLinkHandler,
            com.exponential.app.ui.components.LocalAgentMarks provides agentMarks,
        ) {
            ChatContent(
                view = view,
                agent = agent,
                screen = screen,
                items = items,
                openId = openId,
                onSelectTab = { tab ->
                    requestedTab = tab.wire
                    closeItem()
                },
                onOpen = openItem,
                onCloseItem = closeItem,
                plan = openedPlan?.let { PlanPageState(it, messagesById[it.id]?.card) },
                issueStatus = { statusById[it] },
                rootId = viewModel.rootId,
                onDecidePlan = onDecidePlan,
                issueTitle = { id -> issueTitles[id] },
                onBack = onBack,
                onAddIssue = { menuFor = ChatTabMenuTarget(null) },
                // Only a LINKED issue (a root row) unlinks; a nested one has no link.
                onIssueMenu = { issue -> if (issue.depth == 0) menuFor = ChatTabMenuTarget(issue) },
                issuePage = { issueId, padding ->
                    ChatIssuePage(
                        issueId = issueId,
                        padding = padding,
                        onBack = onBack,
                        onOpenIssue = openIssueHere,
                        onOpenChat = { id -> if (id != viewModel.rootId) onOpenChat(id) },
                        onMessage = { text -> viewModel.send(text) },
                    )
                },
                // EXP-1343: a session chip's page = the session view's body
                // (raw work + its composer) under the chat's ONE header.
                sessionPage = { sessionId, padding ->
                    com.exponential.app.ui.session.SessionPage(
                        sessionId = sessionId,
                        padding = padding,
                        onOpenIssue = openIssueHere,
                        onOpenOrigin = { id, isRoot -> if (isRoot) onOpenChat(id) else onOpenSession(id) },
                        dock = if (sessionId == viewModel.rootId) rootDock else null,
                        feedFooter = if (sessionId == viewModel.rootId) rootFooter else null,
                    )
                },
                headerLead = headerLead,
                headerSubtitle = headerSubtitle,
                headerSubtitleColor = headerSubtitleColor,
                menu = chatMenu,
                menuGroups = listOf(guideMenuItems),
                // NEW-69: an agent's conversation carries the telephone.
                headerCall = agent?.let { a -> { com.exponential.app.ui.call.AgentCallButton(agentId = rootRow?.agentId, agentName = a.name, agentIcon = if (a.assistant) null else a.icon, agentColor = a.color) } },
                // S-3: the Merge rides the header, left of `…` (phones too).
                headerAction = if (agent == null) {
                    { ChatHeaderMerge(pullRequests = pullRequests, teamPullRequests = teamPullRequests, checksItem = guideChecksItem, member = isMember) }
                } else {
                    null
                },
                // #EXP-1732: THE gear pushes the agent's (the Assistant's) settings page.
                gear = if (screen.gear && onOpenAgentSettings != null) {
                    { rootRow?.agentId?.takeIf { it.isNotEmpty() }?.let { id -> onOpenAgentSettings?.invoke(id) } }
                } else {
                    null
                },
                initialCard = viewModel.initialCard,
                // #EXP-1732: every kind has Results (an agent's tree too).
                resultsPage = { padding ->
                    ChatResultsPage(
                        rootId = viewModel.rootId,
                        sessions = tree,
                        messages = messages,
                        onReply = viewModel::sendTo,
                        pullRequests = pullRequests,
                        teamPullRequests = teamPullRequests,
                        pullRequestIssues = pullRequestIssues,
                        issues = syncedIssues,
                        padding = padding,
                        onOpenChat = { id -> if (id != viewModel.rootId) onOpenChat(id) },
                    )
                },
            )
        }
    }

    menuFor?.let { target ->
        ChatTabMenuSheet(
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
                if (openKind == SessionScreen.Open.Issue.wire && openId == issue.issueId) closeItem()
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
    // #EXP-1710: Merge now asks first (merge-train.json `mergeNowPrompt`:
    // Merge destructive, Cancel focused; `titleStack` inside an open stack).
    mergeNowConfirm?.let { pr ->
        MergeNowConfirm(
            pr = pr,
            stackPool = (teamPullRequests + pullRequests).distinctBy { it.id },
            onDismiss = { mergeNowConfirm = null },
            onMerge = { viewModel.mergePullRequestNow(pr.id) },
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

private fun Modifier.androidxClipCircle(): Modifier = this.then(Modifier.clip(androidx.compose.foundation.shape.CircleShape))

/** The chat header's lead (chats.json `lead`, the list row's rule): needs
 *  you, working, else the root's PR glyph, else the bare mark. */
internal fun chatLeadOf(view: SessionTree.ChatView?, pullRequests: List<PullRequestEntity>): com.exponential.app.domain.Chats.ChatLead {
    val mark = SessionTree.chatRowMarkState(view?.needsYou == true, view?.busy == true)
    return when {
        mark == SessionTree.SessionState.NeedsYou -> com.exponential.app.domain.Chats.ChatLead.NeedsYou
        mark == SessionTree.SessionState.Working -> com.exponential.app.domain.Chats.ChatLead.Working
        pullRequests.any { it.state == DomainContract.prStateOpen } -> com.exponential.app.domain.Chats.ChatLead.Open
        pullRequests.any { it.state == DomainContract.prStateMerged } -> com.exponential.app.domain.Chats.ChatLead.Merged
        else -> com.exponential.app.domain.Chats.ChatLead.Waiting
    }
}

/** The chat header's state word ×4 (#EXP-1732 session-screen.json STATUS
 *  WORD, `statusCases`): THE TREE's lowercase word, never dropped. */
internal fun chatStateWord(rootId: String, tree: List<com.exponential.app.data.db.SessionEntity>): String =
    SessionScreen.status(
        tree.map {
            SessionScreen.StatusInput(
                status = it.status,
                busy = it.busy,
                needsInput = it.needsInput,
                root = it.id == rootId,
            )
        },
    )

/** session-screen.json TONE ×4: needs input = amber, working or queued =
 *  the foreground, else null (the muted default). */
@Composable
internal fun chatStateTone(word: String): Color? = when (word) {
    SessionScreen.Copy.Status.NEEDS_INPUT -> com.exponential.app.ui.issue.NeedsInputAmber
    SessionScreen.Copy.Status.WORKING, SessionScreen.Copy.Status.QUEUED -> MaterialTheme.colorScheme.onSurface
    else -> null
}

/** A wire timestamp as the local `HH:mm` a message row shows. */
internal fun clockTime(wire: String): String? {
    val ms = WireTimestamps.parseEpochMs(wire) ?: return null
    return Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).format(DateTimeFormatter.ofPattern("HH:mm"))
}

/**
 * #EXP-1732 THE session screen's whole layout, stateless (phones = NARROW,
 * session-screen.json `sessionScreen`'s `layout`). `chat` / `tab`: the header
 * band = THE chat header ([ChatHeader] in its edge chrome: `‹`, the lead
 * disc, the title over the subtitle, the Merge pill, THE gear, `…`) with
 * [SessionScreenTabs] under it, the body scrolling UNDER the band: Chat = the
 * ROOT's own session view ([sessionPage] of [rootId]: its raw work with the
 * traces, its cards, THE Dock and THE composer), always; Issues / Sessions =
 * THE rows (every session UNDER the root); Results =
 * [resultsPage]. `item` = a pushed page over the tab: `‹` + the item's name
 * over the opened issue / session / plan (system Back pops it).
 */
@Composable
fun ChatContent(
    view: SessionTree.ChatView?,
    agent: ChatAgent?,
    /** session-screen.json `sessionScreen` (narrow). */
    screen: SessionScreen.Screen,
    /** The Issues / Sessions tabs' rows ([sessionScreenItems]). */
    items: SessionScreenItems,
    /** The opened item's id (an issue, a session, a plan message). */
    openId: String?,
    onSelectTab: (SessionScreen.Tab) -> Unit,
    onOpen: (SessionScreen.Open, String) -> Unit,
    onCloseItem: () -> Unit,
    issueStatus: (String) -> ResolvedIssueStatus?,
    /** The root session: the Chat tab is ITS session view. */
    rootId: String,
    /** An opened plan's decision. */
    onDecidePlan: ChatPlanDecision,
    onBack: (() -> Unit)?,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** The Issues tab's `+` (THE Menu: Link an issue, New issue); null hides it. */
    onAddIssue: (() -> Unit)? = null,
    /** An issue row's long-press (its menu: Unlink). */
    onIssueMenu: ((SessionScreenIssue) -> Unit)? = null,
    /** EXP-1343: a session's own view = its raw work + its composer (the
     *  session view's body, no own header): the root's = the Chat tab, a
     *  descendant's = an opened item. */
    sessionPage: @Composable (sessionId: String, padding: PaddingValues) -> Unit = { _, _ -> },
    /** Session 58cb32df (chats.json): THE chat header — its lead disc, the
     *  subtitle and the `…` entries (the chat's menu, then the Guide's). */
    headerLead: @Composable () -> Unit = {},
    headerSubtitle: String? = null,
    headerSubtitleColor: Color? = null,
    menu: List<com.exponential.app.ui.components.ChatMenuItem> = emptyList(),
    menuGroups: List<List<com.exponential.app.ui.components.ChatMenuItem>> = emptyList(),
    /** An issue's title (the Issues rows). */
    issueTitle: (String) -> String? = { null },
    /** NEW-69: the agent chat's telephone in the header (null = none). */
    headerCall: (@Composable () -> Unit)? = null,
    /** S-3: the header's action slot left of `…` (the chat's Merge). */
    headerAction: (@Composable () -> Unit)? = null,
    /** #EXP-1732: THE gear (null = none: a chat). */
    gear: (() -> Unit)? = null,
    /** S-3: an inter-session link pushes that session (null = inert). */
    onOpenXref: ((sessionId: String) -> Unit)? = null,
    /** results-tab.json: the Results tab's body (null = an empty tab). */
    resultsPage: (@Composable (padding: PaddingValues) -> Unit)? = null,
    /** needs-you.json: a way in names a waiting card: the Conversation centres
     *  it once, when the chat is there. */
    initialCard: String? = null,
    /** #EXP-1662 (plan-page.json): the opened plan (`open: plan`). */
    plan: PlanPageState? = null,
) {
    // needs-you.json: a way in's waiting card is centred in the root's
    // session view (its feed honours [LocalCardScrollRequest]) and rung for `constants.highlightMs`.
    var cardScroll by remember { mutableStateOf<com.exponential.app.ui.session.CardScrollRequest?>(null) }
    LaunchedEffect(cardScroll) {
        if (cardScroll == null) return@LaunchedEffect
        kotlinx.coroutines.delay(WaitingCards.Constants.HIGHLIGHT_MS)
        cardScroll = null
    }
    val latestScreen by rememberUpdatedState(screen)
    val pickWaiting: (String) -> Unit = { id ->
        if (latestScreen.open != null) onCloseItem()
        if (latestScreen.active != SessionScreen.Tab.Chat) onSelectTab(SessionScreen.Tab.Chat)
        cardScroll = com.exponential.app.ui.session.CardScrollRequest(id, (cardScroll?.nonce ?: 0) + 1)
    }
    // #EXP-1785 (waiting-cards.json, phones = `button`): THE TREE's waiting
    // plans + questions (ask-dock `chatPendingAsks` over the chat rows: the
    // root's and every descendant's, Conversation order) behind ONE button
    // in the header's trailing slot; a pick goes to `waitingTarget`.
    val waitingChips = remember(view?.rows) {
        val rows = view?.rows.orEmpty()
        WaitingCards.waitingChips(WaitingCards.inputs(AskDock.chatPendingAsks(rows) { null }), phone = true).chips
    }
    var waitingSheet by remember { mutableStateOf(false) }
    val pickWaitingChip: (WaitingCards.Chip) -> Unit = { chip ->
        val target = WaitingCards.waitingTarget(chip.kind, chip.sessionId, rootId)
        when (target.open) {
            // THE plan page as the screen's opened item (any session's plan).
            WaitingCards.Open.Plan -> onOpen(SessionScreen.Open.Plan, chip.id)
            // The root's question: the Chat tab, centred on its card + the ring.
            WaitingCards.Open.Card -> pickWaiting(chip.id)
            // A descendant's question: that session opened in place, at its card.
            WaitingCards.Open.Session -> {
                onOpen(SessionScreen.Open.Session, target.sessionId)
                cardScroll = com.exponential.app.ui.session.CardScrollRequest(chip.id, (cardScroll?.nonce ?: 0) + 1)
            }
        }
    }
    var initialCardDone by rememberSaveable { mutableStateOf(false) }
    val chatLoaded = view != null
    LaunchedEffect(initialCard, chatLoaded) {
        if (initialCard != null && chatLoaded && !initialCardDone) {
            initialCardDone = true
            pickWaiting(initialCard)
        }
    }
    // plan-page.json (#FEED-191): a decision taken ON the opened plan returns
    // to the Conversation once it succeeds, so the agent is seen working on;
    // one from the Conversation stays there (its call never comes through here).
    val decideOnPlanItem: ChatPlanDecision = { id, decision, settled ->
        onDecidePlan(id, decision) { ok ->
            settled(ok)
            if (ok && latestScreen.open == SessionScreen.Open.Plan) onCloseItem()
        }
    }

    if (screen.layout == SessionScreen.Layout.Item) {
        // THE pushed page: `‹` + the item's name; system Back pops it.
        BackHandler(onBack = onCloseItem)
        val title = when (screen.open) {
            SessionScreen.Open.Issue -> openId?.let { items.issue(it)?.identifier }.orEmpty()
            SessionScreen.Open.Session -> openId?.let { items.session(it)?.name }.orEmpty()
            SessionScreen.Open.Plan -> PlanPage.Copy.TITLE
            SessionScreen.Open.Settings -> SessionScreen.Copy.SETTINGS
            null -> ""
        }
        Scaffold(
            modifier = modifier,
            containerColor = Color.Transparent,
            topBar = { ChatHeader(title = title, subtitle = null, lead = {}, onBack = onCloseItem) },
        ) { padding ->
            key(screen.open, openId) {
                // Each item owns its view models: they die with the item.
                // #EXP-1785: a descendant's waiting question opened here is
                // centred + rung like the Chat tab's.
                CompositionLocalProvider(
                    com.exponential.app.ui.session.LocalCardScrollRequest provides cardScroll,
                    com.exponential.app.ui.session.LocalCardHighlight provides cardScroll?.id,
                ) {
                PageViewModelStore {
                    val id = openId
                    when (screen.open) {
                        SessionScreen.Open.Issue -> if (id != null) issuePage(id, padding)
                        SessionScreen.Open.Session -> if (id != null) sessionPage(id, padding)
                        SessionScreen.Open.Plan -> {
                            if (plan != null) {
                                PlanPageContent(
                                    state = plan,
                                    onDecidePlan = decideOnPlanItem,
                                    topInset = padding.calculateTopPadding(),
                                    bottomInset = padding.calculateBottomPadding(),
                                )
                            } else {
                                ChatPageSkeleton(padding.calculateTopPadding())
                            }
                        }
                        else -> Unit
                    }
                }
                }
            }
        }
        return
    }

    if (waitingSheet && waitingChips.isNotEmpty()) {
        SessionWaitingSheet(
            chips = waitingChips,
            onPick = pickWaitingChip,
            onDismiss = { waitingSheet = false },
        )
    }
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
                action = headerAction,
                gear = gear,
                waiting = if (waitingChips.isNotEmpty()) {
                    { SessionWaitingButton(count = waitingChips.size, onClick = { waitingSheet = true }) }
                } else {
                    null
                },
                tabs = if (view != null) {
                    { SessionScreenTabs(tabs = screen.tabs, active = screen.active, onSelect = onSelectTab) }
                } else {
                    null
                },
            )
        },
    ) { padding ->
        if (view == null) {
            Box(Modifier.padding(padding).fillMaxSize()) {
                SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("chat-skeleton"))
            }
            return@Scaffold
        }
        val topInset = padding.calculateTopPadding()
        val bottomInset = padding.calculateBottomPadding()
        val listPadding = PaddingValues(top = topInset + 8.dp, bottom = bottomInset + 16.dp)
        CompositionLocalProvider(
            com.exponential.app.ui.session.LocalCardScrollRequest provides cardScroll,
            com.exponential.app.ui.session.LocalCardHighlight provides cardScroll?.id,
        ) {
            when (screen.active) {
                // #EXP-1732: the Chat tab = the ROOT's own session view (its
                // raw work + traces, its cards, THE Dock, THE composer).
                SessionScreen.Tab.Chat -> sessionPage(rootId, padding)
                SessionScreen.Tab.Issues -> SessionIssuesBody(
                    issues = items.issues,
                    issueStatus = issueStatus,
                    issueTitle = issueTitle,
                    onOpen = { id -> onOpen(SessionScreen.Open.Issue, id) },
                    onAdd = onAddIssue,
                    contentPadding = listPadding,
                    onMenu = onIssueMenu,
                )
                SessionScreen.Tab.Sessions -> SessionSessionsBody(
                    sessions = items.sessions,
                    onOpen = { id -> onOpen(SessionScreen.Open.Session, id) },
                    contentPadding = listPadding,
                )
                SessionScreen.Tab.Results -> resultsPage?.invoke(padding)
                else -> Unit
            }
        }
    }
}

/** #EXP-1387: an item while it loads: skeleton rows under the header band. */
@Composable
private fun ChatPageSkeleton(topInset: Dp) {
    Box(Modifier.fillMaxSize().padding(top = topInset)) {
        SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("chat-page-skeleton"))
    }
}

/**
 * #EXP-1662 / #EXP-1732 (plan-page.json): the OPENED plan (`open: plan`):
 * the root's plan row [id], else any plan message of the tree (a child
 * session's); null = not synced yet.
 */
internal fun chatOpenedPlan(
    rows: List<SessionTree.SessionChatRow>,
    id: String?,
    messages: Map<String, SessionTree.Message>,
): SessionTree.SessionChatRow.Plan? {
    if (id == null) return null
    return rows.filterIsInstance<SessionTree.SessionChatRow.Plan>().firstOrNull { it.id == id }
        ?: messages[id]?.let(SessionTree::askRow) as? SessionTree.SessionChatRow.Plan
}

/** What THE menu acts on: the "+" (null) or a linked issue's row. */
private data class ChatTabMenuTarget(val issue: SessionScreenIssue?)

/** The "+" / an issue row's menu as a phone sheet: Link an issue, New
 *  issue, and on an issue row Unlink <IDENT>. */
@Composable
private fun ChatTabMenuSheet(
    issue: SessionScreenIssue?,
    canFile: Boolean,
    onLink: () -> Unit,
    onNewIssue: () -> Unit,
    onUnlink: (SessionScreenIssue) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = issue?.identifier, onDismiss = onDismiss, modifier = Modifier.testTag("chat-tab-menu")) {
        val iconTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        GlassSheetRow(
            label = ChatUiCopy.LINK_ISSUE,
            onClick = onLink,
            leading = { Icon(ExpIcons.navSearch, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        GlassSheetRow(
            label = ChatUiCopy.NEW_ISSUE,
            onClick = onNewIssue,
            enabled = canFile,
            leading = { Icon(ExpIcons.navCreateIssue, contentDescription = null, tint = iconTint, modifier = Modifier.size(18.dp)) },
        )
        if (issue != null) {
            GlassSheetRow(
                label = ChatUiCopy.unlink(issue.identifier),
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
 *  top bar, no `Chats` row, no Start; a composer seed carrying text goes
 *  to the chat's root as a message ([onMessage]). */
@Composable
private fun ChatIssuePage(
    issueId: String,
    padding: PaddingValues,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenChat: (String) -> Unit,
    onMessage: (String) -> Unit,
) {
    IssueScreen(
        issueId = issueId,
        onBack = onBack,
        onOpenIssue = onOpenIssue,
        onOpenAgent = { seed -> seed.text?.takeIf { it.isNotBlank() }?.let(onMessage) },
        onOpenChat = onOpenChat,
        embedded = true,
        padding = padding,
    )
}

// ── the header Merge ────────────────────────────────────────────────────────

/**
 * #EXP-1710 (merge-train.json STATUS PILL): the chat's merge STATUS in the
 * header's action slot, left of `…` (phones too: Android's Merge rides the
 * header). It reads [SessionTree.guidePullRequest]'s synced `merge_*` columns
 * (`MergeTrain.statusPill`); a tap opens the Checks submenu ([checksItem]'s
 * rows; none = readonly). The ONE action is `Needs approval` (approval mode):
 * the plain or stack confirm as the Merge pill asked, then
 * `pullRequests.merge`, which QUEUES it. A refused call toasts with Retry.
 * Nothing while there is no PR, or it was closed.
 */
@Composable
private fun ChatHeaderMerge(
    pullRequests: List<PullRequestEntity>,
    teamPullRequests: List<PullRequestEntity>,
    /** #EXP-1611: the `…`'s Checks entry (its submenu = the pill's rows). */
    checksItem: com.exponential.app.ui.components.ChatMenuItem?,
    /** A viewer (not a member) sees Needs approval as a plain status. */
    member: Boolean,
) {
    val newest = SessionTree.guidePullRequest(pullRequests) ?: return
    if (newest.state == DomainContract.prStateClosed) return
    val stackPool = remember(pullRequests, teamPullRequests) {
        (teamPullRequests + pullRequests).distinctBy { it.id }
    }
    val stack = remember(newest, stackPool) {
        if (newest.state == DomainContract.prStateOpen) PullRequestStack.stackConfirm(newest, stackPool) else null
    }
    val checkRows = checksItem?.submenu.orEmpty()
    val view = remember(newest, stack, stackPool, checkRows.size, member) {
        com.exponential.app.domain.MergeTrain.statusPill(
            com.exponential.app.domain.MergeTrain.StatusPillInput(
                state = com.exponential.app.domain.MergeTrain.prMergeState(newest.state, newest.mergeState),
                wait = newest.mergeWait,
                position = newest.mergePosition,
                batchSize = com.exponential.app.domain.MergeTrain.batchSize(newest.mergeTrainId, stackPool.map { it.mergeTrainId }),
                holdReason = newest.mergeHold,
                stack = stack != null,
                checks = checkRows.size,
            ),
        ).let { com.exponential.app.domain.MergeTrain.forViewer(it, member, checkRows.size) }
    }
    var approveOpen by remember { mutableStateOf(false) }
    var checksOpen by remember { mutableStateOf(false) }
    Spacer(Modifier.width(com.exponential.app.domain.Chats.Shell.ChatHeader.GAP.dp))
    if (view.tap == "approve") {
        val merging = NeedsApprovalConfirm(
            newest = newest,
            stackPool = stackPool,
            confirmOpen = approveOpen,
            onDismiss = { approveOpen = false },
        )
        MergeStatusPill(view = view, onClick = { approveOpen = true }, loading = merging)
        return
    }
    Box {
        MergeStatusPill(view = view, onClick = if (view.tap == "checks") ({ checksOpen = true }) else null)
        com.exponential.app.ui.components.GlassDropdownMenu(expanded = checksOpen, onDismissRequest = { checksOpen = false }) {
            ChatMenuPanel(
                groups = listOf(checkRows),
                page = null,
                onPush = {},
                onBack = {},
                onPick = { item ->
                    checksOpen = false
                    item.onClick()
                },
            )
        }
    }
}
