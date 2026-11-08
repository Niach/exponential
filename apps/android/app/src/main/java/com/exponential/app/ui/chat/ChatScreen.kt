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
import com.exponential.app.ui.components.TabPager
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
 * A chat: the title (EXP-1316: derived, never renamed), then ONE tab strip
 * = the phone pager — Conversation · one chip per linked issue · "+" (link /
 * file an issue) · the session chips (EXP-1343: each a page = its raw work + composer) ·
 * Results. #EXP-1662: a chat holding a plan adds the plan tab after
 * Conversation ([PlanPageContent] under the header + strip). Conversation = the
 * root's rows (THE session card per session, the person's bubbles, the agent's
 * prose, THE session view's question card answered in place, THE plan row,
 * subagent / picture / event rows) over THE composer box; an issue page = that issue's [IssueScreen] embedded; Results = the tree's results under its PR block.
 * The header carries the Merge capsule of its PR. An AGENT channel is the same screen with the
 * agent's icon + name (settings affordance) and the conversation only, its
 * messages as comment rows. A live session page puts Stop in the header.
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
    val users by viewModel.users.collectAsStateWithLifecycle()
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
    // EXP-1521: only WHETHER there is a live tail; the tail collects the
    // relay state itself, so the agent's `turn` ticks recompose the tail
    // alone, never the whole chat (transcript and composer).
    val hasLiveTail by viewModel.hasLiveTail.collectAsStateWithLifecycle()
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

    var selectedKey by rememberSaveable { mutableStateOf(chatInitialTabKey(viewModel.initialTab)) }
    // #EXP-1662 (plan-page.json): the plan a row or a way in opened (else
    // `planTabTarget`), any plan of the tree, kept while the tab is shown.
    var pinnedPlan by rememberSaveable { mutableStateOf(viewModel.initialPlan) }
    // #FEED-191 (plan-page.json `planTabTarget`'s `kept`): the plan the tab
    // showed during THIS visit of the chat, kept until the person leaves the
    // chat (saveable, so rotation keeps it; leaving pops the entry and a
    // decided plan has no chip on the way back); tab switches keep it.
    var keptPlan by rememberSaveable { mutableStateOf<String?>(null) }
    var menuFor by remember { mutableStateOf<ChatTabMenuTarget?>(null) }
    var linkPickerOpen by remember { mutableStateOf(false) }
    var closePrConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }
    // #EXP-1710: the `…`'s Merge now asks the shared `merge-now-pr` alert.
    var mergeNowConfirm by remember { mutableStateOf<PullRequestEntity?>(null) }
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
    val shownPlan = remember(view?.rows, pinnedPlan, messagesById, keptPlan) {
        chatShownPlan(view?.rows.orEmpty(), pinnedPlan, messagesById, keptPlan)
    }
    // `kept` follows the tab's target whenever it is non-null (a pressed row
    // pins its plan, so it lands here too).
    LaunchedEffect(shownPlan?.id) { shownPlan?.id?.let { keptPlan = it } }
    // A plan of this tree opens the tab in place (a session page's row);
    // anything else takes the host's way in.
    val outerOpenPlan = LocalOpenPlanPage.current
    val latestMessages by rememberUpdatedState(messagesById)
    val openPlanHere = remember(outerOpenPlan) {
        OpenPlanPage { sessionId, messageId ->
            if (latestMessages.containsKey(messageId)) {
                pinnedPlan = messageId
                selectedKey = PLAN_TAB_KEY
            } else {
                outerOpenPlan.open(sessionId, messageId)
            }
        }
    }
    // #EXP-1380: one instance per change of its inputs, so the transcript's
    // rows skip recomposition while the screen's other state moves.
    val metaFor: (SessionTree.SessionChatRow.MessageRow) -> ChatRowMeta? = remember(users, messagesById) {
        { row ->
            val user = row.authorId?.let { users[it] }
            ChatRowMeta(
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
        ChatAgent(
            name = def?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS,
            icon = def?.icon,
            color = def?.color,
            assistant = def?.isAssistant == true,
        )
    } else {
        null
    }
    // EXP-1649: an agent's conversation reads the same placeholder as web.
    val placeholder = view?.composer?.placeholder
        ?: SessionTree.ChatCopy.PLACEHOLDER

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
    val headerSubtitle = if (agent != null) {
        SessionTree.agentChatSubtitle(rootRow?.model?.takeIf { it.isNotBlank() }?.let(::modelLabel))
    } else {
        listOfNotNull(chatStateWord(view), deviceLabel).joinToString(" · ")
    }
    // #EXP-1688 (web `ChatHeaderTitle`): needs input = amber, working = the
    // foreground, otherwise the muted default.
    val headerSubtitleColor = when {
        agent != null -> null
        view?.needsYou == true -> com.exponential.app.ui.issue.NeedsInputAmber
        view?.busy == true -> MaterialTheme.colorScheme.onSurface
        else -> null
    }
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

    // EXP-760 ×4: a `#IDENTIFIER` (and, in the conversation, a BARE one) of a
    // synced same-team issue is a pill; one linked here opens its tab.
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    // EXP-1347: every issue ref inside the chat (its pages, nested issue
    // tabs and embedded session pages included) takes this one handler.
    val openIssueHere: (String) -> Unit = { id ->
        if (statusById.containsKey(id)) selectedKey = "issue:$id" else currentOnOpenIssue(id)
    }
    val issueRefHandler = remember(linkCandidates, statusById) {
        IssueRefHandler(linkCandidates, searchServer = viewModel::searchIssues) { target -> openIssueHere(target.issueId) }
    }

    // N-12: THE Dock (dock.json): the suggested-tasks rows with an open item
    // ride the Dock. S-3: a waiting plan or question stays inline.
    val dockedIds = remember(view) { chatDockedIds(view?.rows.orEmpty()) }
    // #EXP-1464 (chat-row-action.json `open`): a session opens where it
    // lives (inbox-route.json `sessionTarget`, [sessionTargetRoute]): my agent
    // conversation root = its channel, a chat root = its chat, a chat's
    // child or anything under an agent = its own session view. A session of
    // THIS chat's strip opened in place before this ([ChatContent]).
    val openChatSession: (String) -> Unit = { id ->
        when (val target = if (id == viewModel.rootId) null else viewModel.sessionTarget(id)) {
            is InboxRoute.SessionTarget.Agent -> onOpenChat(id)
            is InboxRoute.SessionTarget.Chat ->
                target.sessionId?.let(onOpenSession) ?: onOpenChat(target.chatId)
            is InboxRoute.SessionTarget.Session -> onOpenSession(target.sessionId)
            null -> if (viewModel.isOtherRoot(id)) onOpenChat(id) else onOpenSession(id)
        }
    }
    val rowActions = ChatRowActions(
        blocked = viewModel::isBlocked,
        waitingCard = { id ->
            remember(id) { viewModel.waitingCard(id) }.collectAsStateWithLifecycle(null).value
        },
    )
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
    val onAnswer: ChatAnswer = { id, answer, settled ->
        viewModel.answer(id, answer.answer, settled, answer.answers, answer.surfaceData)
    }
    val onDecidePlan: ChatPlanDecision = { id, d, settled ->
        viewModel.decidePlan(id, d.approved, d.fresh, d.feedback, settled, d.answers, d.surfaceData)
    }

    // Inline `@email` mention pills in the chat rows: the team's synced
    // members (issue detail's resolver); unknown addresses stay plain text.
    val mentionResolver = remember(teamUsers) {
        MentionResolver(teamUsers.map { MentionMember(it.name ?: it.email, it.email) })
    }

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
    val treeNames = remember(view) {
        buildMap {
            view?.tabs.orEmpty().filterIsInstance<SessionTree.ChatTab.SessionTab>().forEach { tab ->
                put(tab.sessionId, tab.name)
                tab.nested.forEach { put(it.sessionId, it.name) }
            }
        }
    }
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
                selectedKey = selectedKey,
                onSelectKey = { selectedKey = it },
                plan = shownPlan?.let { PlanPageState(it, messagesById[it.id]?.card) },
                onPlanLeft = { if (selectedKey != PLAN_TAB_KEY) pinnedPlan = null },
                issueStatus = { statusById[it] },
                meta = metaFor,
                // #EXP-1464: a session card opens where its session lives.
                onOpenCard = { card -> openChatSession(card.sessionId) },
                rowActions = rowActions,
                onAnswer = onAnswer,
                onDecidePlan = onDecidePlan,
                dockedIds = dockedIds,
                issueTitle = { id -> issueTitles[id] },
                canUnqueue = { row -> row.authorId != null && row.authorId == me },
                onUnqueue = viewModel::unqueue,
                onBack = onBack,
                onAdd = { menuFor = ChatTabMenuTarget(null) },
                onIssueMenu = { menuFor = ChatTabMenuTarget(it) },
                // THE session composer (dispatcher roots): the session view's
                // box targeting the ROOT, fed by the root's live state.
                composer = { expanded, onExpandedChange ->
                    SessionComposer(
                        viewModel = rootSession,
                        expanded = expanded,
                        onExpandedChange = onExpandedChange,
                        placeholder = placeholder,
                        capsuleTestTag = "chat-composer-capsule",
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
                        modifier = Modifier.testTag(SessionTree.ChatCopy.TEST_ID_COMPOSER),
                        // N-12: THE Dock: the tree's waiting cards, then the
                        // root's Queue and banners (its queued rows sit in the
                        // Conversation), over the composer that never leaves.
                        dock = { rootDock ->
                            // #EXP-1545: the root's Queue, else a Live child's.
                            val sessionDock = chatDock(rootDock)
                            ChatDock(
                                rows = view?.rows.orEmpty(),
                                sources = sessionDock.sources,
                                sessionSheet = sessionDock.sheet,
                                action = sessionDock.action,
                                suggestions = suggestionActions,
                                onOpenSession = openChatSession,
                            )
                        },
                    )
                },
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
                    )
                },
                headerLead = headerLead,
                headerSubtitle = headerSubtitle,
                headerSubtitleColor = headerSubtitleColor,
                menu = chatMenu,
                menuGroups = listOf(guideMenuItems),
                // NEW-69: an agent's conversation carries the telephone.
                headerCall = agent?.let { a -> { com.exponential.app.ui.call.AgentCallButton(agentId = rootRow?.agentId, agentName = a.name, agentIcon = if (a.assistant) null else a.icon, agentColor = a.color) } },
                onSurfaceReply = { text -> viewModel.send(text) },
                // S-3: the Merge rides the header, left of `…` (phones too).
                headerAction = if (agent == null) {
                    { ChatHeaderMerge(pullRequests = pullRequests, teamPullRequests = teamPullRequests, checksItem = guideChecksItem, member = isMember) }
                } else {
                    null
                },
                onOpenSession = openChatSession,
                // S-3: an inter-session link PUSHES that session (another
                // root as its chat, any other session as its view).
                onOpenXref = openChatSession,
                initialCard = viewModel.initialCard,
                resultsPage = if (agent == null) {
                    { padding ->
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
                    }
                } else {
                    null
                },
                suggestions = suggestionActions,
                tail = if (hasLiveTail) { { ChatLiveTail(viewModel, root) } } else null,
            )
        }
    }

    SessionUsageSheet(
        viewModel = rootSession,
        open = usageSheetOpen,
        onDismiss = { usageSheetOpen = false },
    )
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

/** The chat header's state word ×4 (web `ChatHeaderTitle`, iOS
 *  `ChatStripTabs.subtitle`): `needs input` (chat-list.json
 *  `words.needsInput`) while it needs you, `working…` while busy, else none. */
internal fun chatStateWord(view: SessionTree.ChatView?): String? = when {
    view == null -> null
    view.needsYou -> SessionTree.ChatRowWord.NeedsInput.text
    view.busy -> com.exponential.app.domain.Chats.Copy.PREVIEW_WORKING
    else -> null
}

/** A wire timestamp as the local `HH:mm` a message row shows. */
internal fun clockTime(wire: String): String? {
    val ms = WireTimestamps.parseEpochMs(wire) ?: return null
    return Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).format(DateTimeFormatter.ofPattern("HH:mm"))
}

/**
 * The chat screen's whole layout, stateless: the header band and the
 * pager. EXP-1316: the header IS the detail chrome ([WorkTopBar] in
 * [HeaderEdgeChrome]): `‹ · title (one line, lock) · …` with the tab strip
 * inside the band, the pages scrolling UNDER it (the conversation registers
 * as the blur's source). The reply box floats over the conversation's tail
 * ([composer] = THE [FloatingComposer], in a [FloatingBarEdge]). An agent
 * channel wears the same band without a strip. The issue + Guide pages come
 * in as slots and take the Scaffold padding themselves.
 */
@Composable
fun ChatContent(
    view: SessionTree.ChatView?,
    agent: ChatAgent?,
    selectedKey: String,
    onSelectKey: (String) -> Unit,
    issueStatus: (String) -> ResolvedIssueStatus?,
    meta: (SessionTree.SessionChatRow.MessageRow) -> ChatRowMeta?,
    onOpenCard: (SessionTree.SessionCard) -> Unit,
    onAnswer: ChatAnswer,
    onDecidePlan: ChatPlanDecision,
    canUnqueue: (SessionTree.SessionChatRow.MessageRow) -> Boolean,
    onUnqueue: (messageId: String) -> Unit,
    onBack: (() -> Unit)?,
    onAdd: (() -> Unit)?,
    onIssueMenu: ((SessionTree.ChatTab.IssueTab) -> Unit)?,
    /** THE floating reply box, folded or expanded. */
    composer: @Composable (expanded: Boolean, onExpandedChange: (Boolean) -> Unit) -> Unit,
    issuePage: @Composable (issueId: String, padding: PaddingValues) -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-1343: a session chip's page = that session's raw work + its
     *  composer (the session view's body, no own header). */
    sessionPage: @Composable (sessionId: String, padding: PaddingValues) -> Unit = { _, _ -> },
    /** Session 58cb32df (chats.json): THE chat header — its lead disc, the
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
    suggestions: ChatSuggestionActions? = null,
    /** EXP-1323: the live tail above the composer (Tasks card + status line). */
    tail: (@Composable () -> Unit)? = null,
    /** EXP-1334: an issue's title (the strip's full tier). */
    issueTitle: (String) -> String? = { null },
    /** THE ask dock: the waiting cards' ids (drawn in the composer's slot). */
    dockedIds: Set<String> = emptySet(),
    /** NEW-69: the agent chat's telephone in the header (null = none). */
    headerCall: (@Composable () -> Unit)? = null,
    /** S-3: the header's action slot left of `…` (the chat's Merge). */
    headerAction: (@Composable () -> Unit)? = null,
    /** S-3: an inter-session link pushes that session (null = inert). */
    onOpenXref: ((sessionId: String) -> Unit)? = null,
    /** results-tab.json: the strip's last tab's page (null = no Results tab,
     *  an agent channel). */
    resultsPage: (@Composable (padding: PaddingValues) -> Unit)? = null,
    /** needs-you.json: a way in names a waiting card: the Conversation centres
     *  it once, when the chat is there (THE waiting chip's pick). */
    initialCard: String? = null,
    /** #EXP-1464: what an expanding session row needs (null = rows only open). */
    rowActions: ChatRowActions? = null,
    /** #EXP-1662 (plan-page.json): the plan the plan tab shows (null = no tab). */
    plan: PlanPageState? = null,
    /** The pager rested off the plan tab: an opened plan is forgotten. */
    onPlanLeft: () -> Unit = {},
) {
    // results-tab.json: the host appends the Results tab after the
    // last chip (never part of session-chat.json `tabs`): the phones'
    // pager's last page.
    // #EXP-1662: the plan tab right after Conversation. An agent channel has no
    // strip but for this: its two pages while it holds a plan.
    val hasPlan = plan != null
    val stripTabs = remember(view?.tabs, agent != null, resultsPage != null, hasPlan) {
        view?.tabs?.let { tabs ->
            when {
                agent != null -> Chats.AgentChat.tabs(hasPlan).map { if (it == "plan") SessionTree.ChatTab.Plan else SessionTree.ChatTab.Conversation }
                else -> chatTabsWithPlan(if (agent == null && resultsPage != null) tabs + SessionTree.ChatTab.Results else tabs, hasPlan)
            }
        }
    }
    val hasStrip = agent == null || hasPlan
    // An agent channel is the conversation only. NEW-6 (N-1) / NEW-46: a
    // chat with ONE session (the root alone) IS that session: the
    // Conversation tab's page is the root's session page (one page).
    val mainSessionId = if (agent != null) null else view?.mainSessionId
    // #EXP-1433: the strip's issues + sessions (chips = the one issue, group
    // = everything behind ONE tab); phones page Conversation · (the one
    // issue | the item opened from the group) · Results.
    val stripItems = remember(stripTabs, mainSessionId) { chatStripItems(stripTabs.orEmpty(), mainSessionId) }
    // NEW-46: "conversation" on a single-session chat = the main page.
    val pageKey = if (mainSessionId != null && selectedKey == "conversation") "session:$mainSessionId" else selectedKey
    // session-chat.json STRIP: the item opened from the group is a page
    // only while it is open (web `chatPages`: the shown page). It stays
    // through a drag away (no mid-drag jump) and goes once the pager settles
    // on another page ([chatGroupOpenAfterSettle]).
    var groupOpen by rememberSaveable { mutableStateOf<String?>(null) }
    val groupMode = stripItems.single == null
    val openNow = pageKey.takeIf { groupMode && stripItems.tabOf(it) != null }
    LaunchedEffect(openNow) { if (openNow != null) groupOpen = openNow }
    val open = openNow ?: groupOpen?.takeIf { groupMode && stripItems.tabOf(it) != null }
    val pages = remember(hasStrip, stripTabs, mainSessionId, open) {
        chatMainPages(chatPages(if (hasStrip) stripTabs else null, open, mainSessionId), mainSessionId)
    }
    // A tab that left the strip (an issue unlinked elsewhere) falls back to
    // the first page: the conversation, or the main session's page.
    val selected = pages.firstOrNull { chatTabKey(it) == pageKey }
        ?: pages.firstOrNull()
        ?: SessionTree.ChatTab.Conversation
    // EXP-1343 / #EXP-1433: a session of this chat's strip opens IN PLACE
    // (its page); any other (another root's card) keeps its own route.
    val openSessionHere: (String) -> Boolean = { id ->
        val key = "session:$id"
        if (stripItems.tabOf(key) != null || pages.any { chatTabKey(it) == key }) {
            onSelectKey(key)
            true
        } else {
            false
        }
    }
    var composerExpanded by rememberSaveable { mutableStateOf(false) }
    // #EXP-1562 (waiting-cards.json CHIPS): the conversation's WAITING cards
    // (`chatPendingAsks`, Conversation order) as the phones' ONE button; a
    // pick shows the Conversation, centres that card and rings it for
    // `constants.highlightMs`.
    val waitingChips = remember(view?.rows, agent != null) {
        if (agent != null || view == null) {
            emptyList()
        } else {
            // #EXP-1662: a plan's chip is the plan tab; these are the questions.
            val questions = AskDock.chatPendingAsks(view.rows) { null }.filter { it.kind != AskDock.Kind.Plan }
            WaitingCards.waitingChips(WaitingCards.inputs(questions), phone = true).chips
        }
    }
    var cardScroll by remember { mutableStateOf<com.exponential.app.ui.session.CardScrollRequest?>(null) }
    LaunchedEffect(cardScroll) {
        if (cardScroll == null) return@LaunchedEffect
        kotlinx.coroutines.delay(WaitingCards.Constants.HIGHLIGHT_MS)
        cardScroll = null
    }
    val pickWaiting: (String) -> Unit = { id ->
        if (pageKey != "conversation" && pageKey != "session:$mainSessionId") onSelectKey("conversation")
        cardScroll = com.exponential.app.ui.session.CardScrollRequest(id, (cardScroll?.nonce ?: 0) + 1)
    }
    // needs-you.json: a row's `card`: once, when the chat is there.
    var initialCardDone by rememberSaveable { mutableStateOf(false) }
    val chatLoaded = view != null
    LaunchedEffect(initialCard, chatLoaded) {
        if (initialCard != null && chatLoaded && !initialCardDone) {
            initialCardDone = true
            pickWaiting(initialCard)
        }
    }
    // plan-page.json (#FEED-191): a decision taken ON the Plan tab returns to
    // the Conversation once it succeeds, so the agent is seen working on; one
    // from the Conversation stays there (its call never comes through here).
    val latestPageKey by rememberUpdatedState(pageKey)
    val decideOnPlanTab: ChatPlanDecision = { id, decision, settled ->
        onDecidePlan(id, decision) { ok ->
            settled(ok)
            if (ok && latestPageKey == PLAN_TAB_KEY) onSelectKey("conversation")
        }
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
                // An agent's chat is the conversation only: no strip, but for
                // the plan tab while it holds a plan (#EXP-1662).
                tabs = if (hasStrip && view != null) {
                    {
                        ChatTabStrip(
                            tabs = stripTabs.orEmpty(),
                            // NEW-46: the main session's page shows the
                            // Conversation tab selected.
                            selectedKey = chatTabKey(selected).let {
                                if (mainSessionId != null && it == "session:$mainSessionId") "conversation" else it
                            },
                            onSelect = onSelectKey,
                            onAdd = onAdd,
                            issueStatus = issueStatus,
                            issueTitle = issueTitle,
                            onIssueMenu = onIssueMenu,
                            onOpenSession = { id -> openSessionHere(id) },
                            mainSessionId = mainSessionId,
                            waiting = waitingChips,
                            onPickWaiting = pickWaiting,
                            planWaiting = plan?.row?.decision == null && hasPlan,
                            planDecision = plan?.row?.decision,
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
                SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("chat-skeleton"))
            }
            return@Scaffold
        }
        val topInset = padding.calculateTopPadding()
        val bottomInset = padding.calculateBottomPadding()
        CompositionLocalProvider(
            com.exponential.app.ui.session.LocalCardScrollRequest provides cardScroll,
            com.exponential.app.ui.session.LocalCardHighlight provides cardScroll?.id,
        ) {
        TabPager(
            pages = pages,
            selected = selected,
            onSelect = { onSelectKey(chatTabKey(it)) },
            key = { chatTabKey(it) },
            modifier = Modifier.fillMaxSize(),
            // EXP-1343: each page owns its view models: they die with the
            // page instead of piling up per swipe. #EXP-1387: the neighbours
            // stay composed, an issue or session page as cheap skeleton rows
            // until it settles (session-chat.json: only the visible page
            // holds live models), so a swipe's first frame never builds one.
            beyondViewportPageCount = 1,
            pageViewModels = true,
            onSettled = { tab ->
                groupOpen = chatGroupOpenAfterSettle(groupOpen, chatTabKey(tab))
                if (tab != SessionTree.ChatTab.Plan) onPlanLeft()
            },
            neighbour = { tab ->
                when (tab) {
                    is SessionTree.ChatTab.IssueTab, is SessionTree.ChatTab.SessionTab -> {
                        { ChatPageSkeleton(topInset) }
                    }
                    else -> null
                }
            },
        ) { tab ->
            when (tab) {
                SessionTree.ChatTab.Conversation -> {
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
                        ChatConversation(
                            bubbles = agent != null,
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
                            // #EXP-1464: a session of this chat opens in place.
                            onOpenXref = onOpenXref?.let { open -> { id -> if (!openSessionHere(id)) open(id) } },
                            rowActions = rowActions,
                        )
                        FloatingBarEdge(
                            bottomInset = bottomInset,
                            // An expanded composer card is its own surface.
                            visible = !composerExpanded,
                            // EXP-1676: the transcript never shows under the band.
                            ground = true,
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
                is SessionTree.ChatTab.IssueTab -> issuePage(tab.issueId, padding)
                is SessionTree.ChatTab.SessionTab -> sessionPage(tab.sessionId, padding)
                SessionTree.ChatTab.Results -> resultsPage?.invoke(padding)
                SessionTree.ChatTab.Plan -> plan?.let { shown ->
                    key(shown.row.id) {
                        PlanPageContent(
                            state = shown,
                            onDecidePlan = decideOnPlanTab,
                            topInset = topInset,
                            bottomInset = bottomInset,
                        )
                    }
                }
                SessionTree.ChatTab.Add -> Unit
            }
        }
        }
    }
}

/** #EXP-1387: an issue or session page while it is a neighbour (the iOS
 *  chat pager's stand-in): skeleton rows under the header band. */
@Composable
private fun ChatPageSkeleton(topInset: Dp) {
    Box(Modifier.fillMaxSize().padding(top = topInset)) {
        SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp).testTag("chat-page-skeleton"))
    }
}

/**
 * EXP-1343 / #EXP-1433 (session-chat.json STRIP): the phone pager's pages
 * = Conversation · (#EXP-1662: the plan) · (chips: the ONE issue | group: the item [open] from the
 * group's sheet, only while it is open) · Results (the last page). The "+"
 * and the group tab are never pages. null [tabs] (an agent channel, or
 * nothing loaded) = the conversation alone.
 */
internal fun chatPages(
    tabs: List<SessionTree.ChatTab>?,
    open: String? = null,
    mainSessionId: String? = null,
): List<SessionTree.ChatTab> {
    if (tabs == null) return listOf(SessionTree.ChatTab.Conversation)
    val items = chatStripItems(tabs, mainSessionId)
    val middle = items.single?.let { SessionTree.ChatTab.IssueTab(it.issueId, it.identifier) }
        ?: open?.let(items::tabOf)
    return buildList {
        if (SessionTree.ChatTab.Conversation in tabs) add(SessionTree.ChatTab.Conversation)
        if (SessionTree.ChatTab.Plan in tabs) add(SessionTree.ChatTab.Plan)
        middle?.let(::add)
        if (SessionTree.ChatTab.Results in tabs) add(SessionTree.ChatTab.Results)
    }
}

/** #EXP-1662: [tabs] with the plan tab right after Conversation. */
internal fun chatTabsWithPlan(tabs: List<SessionTree.ChatTab>, hasPlan: Boolean): List<SessionTree.ChatTab> =
    if (!hasPlan) {
        tabs
    } else {
        buildList {
            for (tab in tabs) {
                add(tab)
                if (tab == SessionTree.ChatTab.Conversation) add(SessionTree.ChatTab.Plan)
            }
        }
    }

/**
 * #EXP-1662 (plan-page.json): the plan the tab shows: the plan a row or a way
 * in opened ([pinned], the root's rows or any message of the tree), else
 * `planTabTarget` over the Conversation's plan rows with this visit's [kept]
 * (#FEED-191: a decided plan leaves the strip once the person left); null =
 * no tab.
 */
internal fun chatShownPlan(
    rows: List<SessionTree.SessionChatRow>,
    pinned: String?,
    messages: Map<String, SessionTree.Message>,
    kept: String? = null,
): SessionTree.SessionChatRow.Plan? {
    val plans = rows.filterIsInstance<SessionTree.SessionChatRow.Plan>()
    pinned?.let { id ->
        val row = plans.firstOrNull { it.id == id } ?: messages[id]?.let(SessionTree::askRow) as? SessionTree.SessionChatRow.Plan
        if (row != null) return row
    }
    val target = PlanPage.planTabTarget(plans.map { PlanPage.TabPlan(it.id, it.decision) }, kept)
    return plans.firstOrNull { it.id == target }
}

/**
 * #EXP-1433 (session-chat.json STRIP, web `chatPages`): the group's open
 * item once the pager RESTS on [settledKey]: kept while it is the shown page,
 * dropped the moment another page is (Conversation, Results).
 */
internal fun chatGroupOpenAfterSettle(open: String?, settledKey: String): String? =
    open?.takeIf { it == settledKey }

/**
 * NEW-46: [pages] with the single-session chat's Conversation page REPLACED,
 * in place, by the root's own session page (one page, never a duplicate);
 * null [mainSessionId] = the pages as they are.
 */
internal fun chatMainPages(
    pages: List<SessionTree.ChatTab>,
    mainSessionId: String?,
): List<SessionTree.ChatTab> {
    if (mainSessionId == null) return pages
    val main = SessionTree.ChatTab.SessionTab(mainSessionId, name = "", markState = SessionTree.SessionState.Idle)
    return pages.map { if (it == SessionTree.ChatTab.Conversation) main else it }.distinctBy(::chatTabKey)
}

/** What THE menu acts on: the "+" (null) or an issue tab. */
private data class ChatTabMenuTarget(val issue: SessionTree.ChatTab.IssueTab?)

/** The "+" / an issue tab's menu as a phone sheet: Link an issue, New
 *  issue, and on an issue tab Unlink <IDENT>. */
@Composable
private fun ChatTabMenuSheet(
    issue: SessionTree.ChatTab.IssueTab?,
    canFile: Boolean,
    onLink: () -> Unit,
    onNewIssue: () -> Unit,
    onUnlink: (SessionTree.ChatTab.IssueTab) -> Unit,
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

/** EXP-1521: the chat's live tail, the ONE place that reads the relay state. */
@Composable
private fun ChatLiveTail(viewModel: ChatViewModel, root: com.exponential.app.data.db.SessionEntity?) {
    val activity by viewModel.liveActivity.collectAsStateWithLifecycle()
    activity?.let { SessionLiveTail(it, root) }
}
