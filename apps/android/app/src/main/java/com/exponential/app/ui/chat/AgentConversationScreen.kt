package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.background
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.hilt.navigation.compose.hiltViewModel
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.uploadedImageIds
import com.exponential.app.domain.uploadedSteerFiles
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FloatingComposer
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.HeaderEdgeChrome
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.steer.SessionSender
import dagger.hilt.android.lifecycle.HiltViewModel
import dev.chrisbanes.haze.rememberHazeState
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOn

/**
 * NEW-7 (web `routes/t/$teamSlug/agents/$agentId.tsx`): a team agent's page
 * = the CALLER's own conversation with it, the root session with that
 * `agent_id` and `user_id` = me. Once it exists the page IS the chat
 * screen's agent channel ([onOpenConversation] swaps it in); until then
 * this is the same screen empty (the agent's coloured icon + name, the
 * composer `Message <name>`), whose first send creates the conversation
 * with `sessions.send({agentId, text})`. #EXP-1732: it wears THE session
 * screen's header band (the tabs, THE gear) from the first frame; before the
 * conversation exists its Issues / Sessions / Results are empty.
 */
@HiltViewModel
class AgentConversationViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sender: SessionSender,
    private val imagesApi: com.exponential.app.data.api.IssueImagesApi,
    private val promptsApi: com.exponential.app.data.api.PromptsApi,
    private val store: com.exponential.app.data.steer.SteerConnectionStore,
    selection: com.exponential.app.data.TeamSelection,
    private val mcpServersApi: com.exponential.app.data.api.McpServersApi,
) : ViewModel() {

    val agentId: String = savedStateHandle["agentId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced definition row (name, icon, colour); null while loading or gone. */
    val agent: StateFlow<AgentDefinitionEntity?> =
        dbFlow.scopedQuery<AgentDefinitionEntity?>(null) { it.agentDefinitionDao().observeById(agentId) }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // ── #EXP-1715 (composer-menu.json `teamsPick`): the Assistant's Teams pick
    private val _teamsPick = MutableStateFlow<List<String>?>(null)

    fun setTeamsPick(next: List<String>?) {
        _teamsPick.value = next
    }

    /** The pick's rows + value while the agent is the viewer's Assistant;
     *  null for any other agent. No conversation yet = every team. */
    val teamsPick: StateFlow<com.exponential.app.ui.session.AgentSessionViewModel.TeamsPickState?> = combine(
        agent,
        auth.userId,
        dbFlow.scopedQuery(emptyList<com.exponential.app.data.db.TeamEntity>()) { it.teamDao().observeAll() },
        _teamsPick,
    ) { row, me, teams, picked ->
        if (row != null && row.isAssistant && row.ownerId == me) {
            com.exponential.app.ui.session.AgentSessionViewModel.TeamsPickState(com.exponential.app.ui.agent.pickTeams(teams), picked)
        } else {
            null
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    // ── #EXP-1793 (composer-menu.json `mcpPick`): the agent chat's MCP servers ›

    private val _mcpServers = MutableStateFlow<List<com.exponential.app.domain.McpServerOption>>(emptyList())
    /** The agent's team's MCP servers (`mcpServers.list`, once per team). */
    val mcpServers: StateFlow<List<com.exponential.app.domain.McpServerOption>> = _mcpServers

    init {
        viewModelScope.launch {
            agent.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } }.distinctUntilChanged().collect { team ->
                _mcpServers.value = emptyList()
                val accountId = auth.activeAccountId.value
                if (team == null || accountId == null) return@collect
                runCatching { mcpServersApi.list(accountId, team) }
                    .onFailure { if (it is CancellationException) throw it }
                    .getOrNull()
                    ?.let { _mcpServers.value = it }
            }
        }
    }

    /** The person's pick; null = untouched (the seed: no conversation yet,
     *  so the agent's `mcpServerIds`). */
    private val _mcpPick = MutableStateFlow<List<String>?>(null)

    /** The pick as the row shows it, ids naming a deleted server dropped. */
    val mcpServerIds: StateFlow<List<String>> = combine(_mcpPick, agent, _mcpServers) { picked, row, servers ->
        val known = servers.mapTo(HashSet()) { it.id }
        (picked ?: row?.mcpServerIdList.orEmpty()).filter { it in known }
    }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    fun toggleMcpServer(id: String) {
        val current = mcpServerIds.value
        _mcpPick.value = if (id in current) current - id else current + id
    }

    /** The send's `mcpServerIds`: the pick while it differs from the seed
     *  (an empty pick clears), else omitted. */
    private fun mcpSendPick(): List<String>? {
        if (_mcpPick.value == null) return null
        val current = mcpServerIds.value
        return current.takeIf { it.toSet() != agent.value?.mcpServerIdList.orEmpty().toSet() }
    }

    /** My conversation session with the agent, once it exists. */
    val conversationId: StateFlow<String?> = combine(
        dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }.map { rows -> rows.map(SessionTree::session) },
        auth.userId,
    ) { sessions, me -> SessionTree.conversations(sessions, me)[agentId] }
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** #EXP-1434 (composer-menu.json `prompts`, surface `agent`): the "+"
     *  menu's Prompts › rows = the agent's team's prompts in the `/` menu's
     *  order (the agent session composer's source). */
    @OptIn(ExperimentalCoroutinesApi::class)
    val promptRows: StateFlow<List<com.exponential.app.domain.SlashPrompt>> =
        // #EXP-1714: the global Assistant's personal team never syncs: the
        // active team's prompts then (uploads keep the agent's own team).
        com.exponential.app.data.browseTeamIdFlow(
            dbFlow,
            agent.map { it?.teamId?.takeIf { id -> id.isNotEmpty() } },
            selection.selectedId,
        )
            .flatMapLatest { team ->
                if (team == null) {
                    flowOf(emptyList())
                } else {
                    dbFlow.scopedQuery(emptyList<com.exponential.app.data.db.PromptEntity>()) {
                        it.promptDao().observeByTeam(team)
                    }
                }
            }
            .map { rows -> rows.map { com.exponential.app.domain.SlashPrompt(it.id, it.name, it.icon) } }
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    private val _promptFailure = MutableStateFlow<FailedAction?>(null)
    /** A picked prompt whose body could not load, raised once. */
    val promptFailure: StateFlow<FailedAction?> = _promptFailure

    fun clearPromptFailure() {
        _promptFailure.value = null
    }

    /** #EXP-1434: a Prompts › pick = the `/` menu's pick: loads the
     *  prompt's body and hands it to [onBody] (the screen inserts it at the
     *  caret). */
    fun loadPrompt(promptId: String, onBody: (String) -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                onBody(promptsApi.get(accountId, promptId).body)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _promptFailure.value = FailedAction("load", "the prompt", trpcErrorMessage(t, "")) {
                    loadPrompt(promptId, onBody)
                }
            }
        }
    }

    private val _sending = MutableStateFlow(false)
    val sending: StateFlow<Boolean> = _sending

    private val _sent = MutableStateFlow<String?>(null)
    /** The conversation the first send created (consumed once by the screen). */
    val sent: StateFlow<String?> = _sent

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** S-3: the composer's pending attachments (THE chat composer's +
     *  Photo or file), uploaded against the agent's team on send. */
    private val _images = MutableStateFlow<List<com.exponential.app.domain.PendingAttachment>>(emptyList())
    val images: StateFlow<List<com.exponential.app.domain.PendingAttachment>> = _images

    private val _imageError = MutableStateFlow<String?>(null)
    val imageError: StateFlow<String?> = _imageError

    fun addImage(uri: android.net.Uri, bytes: ByteArray, filename: String, contentType: String) {
        _imageError.value = null
        val canonical = com.exponential.app.domain.canonicalContentType(contentType)
        val isImage = com.exponential.app.domain.isInlineImage(canonical)
        val current = _images.value
        val refusal = com.exponential.app.domain.steerAttachmentRefusal(
            imageCount = current.count { it.isImage },
            fileCount = current.count { !it.isImage },
            isImage = isImage,
            sizeBytes = bytes.size.toLong(),
        )
        if (refusal != null) {
            _imageError.value = refusal
            return
        }
        _images.value = current + com.exponential.app.domain.PendingAttachment(
            uri = uri, bytes = bytes, filename = filename, contentType = canonical, isImage = isImage,
        )
    }

    fun refuseAttachment(message: String) {
        _imageError.value = message
    }

    fun removeImage(index: Int) {
        _images.value = _images.value.filterIndexed { i, _ -> i != index }
    }

    /** The first message: `sessions.send({agentId, text, attachmentIds})`
     *  creates my DM (the pending files upload against the team first).
     *  [text] = the field when Send was pressed; [onSent] gets it back once
     *  the server took it (EXP-1537: the screen clears the field only if it
     *  still holds that text). */
    fun send(text: String, onSent: (sentText: String) -> Unit) {
        val images = _images.value
        if ((text.isBlank() && images.isEmpty()) || _sending.value) return
        val accountId = auth.activeAccountId.value ?: return
        _sending.value = true
        // EXP-1537: on the store's scope, not the ViewModel's: backing out
        // right after Send must not cancel the uploads or the send.
        store.launchDetached {
            try {
                val teamId = agent.value?.teamId
                var sent = images
                if (teamId != null) {
                    sent = images.map { image ->
                        if (image.uploadedId != null) return@map image
                        val uploaded = imagesApi.uploadTeamSessionImage(accountId, teamId, image.bytes, image.filename, image.contentType)
                        val stamped = image.copy(uploadedId = uploaded.id, uploadedName = uploaded.filename)
                        // By key: a pick added or removed meanwhile is left alone.
                        _images.value = _images.value.map { if (it.key == image.key) stamped else it }
                        stamped
                    }
                }
                val body = com.exponential.app.domain.AgentComposerPrompt.build(
                    text,
                    sent.uploadedImageIds(),
                    sent.uploadedSteerFiles(),
                ).orEmpty()
                // #EXP-1715: the viewer's Assistant gets the turn's teams.
                val assistant = agent.value?.let { it.isAssistant && it.ownerId == auth.userId.value } == true
                val result = sender.send(
                    SessionsSendInput.toAgent(
                        agentId,
                        body,
                        sent.mapNotNull { it.uploadedId },
                        scopeTeamIds = if (assistant) SessionsSendInput.scope(_teamsPick.value) else null,
                        // #EXP-1793: the conversation starts with the pick.
                        mcpServerIds = mcpSendPick(),
                    ),
                )
                onSent(text)
                // Exactly the files that went out leave (composer-send.json).
                _images.value = com.exponential.app.domain.ComposerSend.settle(
                    com.exponential.app.domain.ComposerSend.Outcome.Sent,
                    text,
                    sent,
                    text,
                    _images.value,
                ).attachments
                _sent.value = result.sessionId
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("send", "the message", trpcErrorMessage(t, "")) { send(text, onSent) }
            } finally {
                _sending.value = false
            }
        }
    }
}

/** How long a missing agent waits for its row before "Not found" (web `SYNC_GRACE_MS`). */
private const val SYNC_GRACE_MS = 4_000L
private const val NOT_FOUND = "Not found"
private const val NOT_FOUND_DETAIL = "It may still be syncing, or it was deleted."

@Composable
fun AgentConversationScreen(
    onBack: () -> Unit,
    /** The conversation exists (synced, or my first send created it): open it in place. */
    onOpenConversation: (sessionId: String) -> Unit,
    /** #EXP-1732: THE gear pushes the agent's (the Assistant's) settings. */
    onOpenAgentSettings: ((agentId: String) -> Unit)? = null,
    viewModel: AgentConversationViewModel = hiltViewModel(),
) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    val conversationId by viewModel.conversationId.collectAsStateWithLifecycle()
    val sent by viewModel.sent.collectAsStateWithLifecycle()
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val promptRows by viewModel.promptRows.collectAsStateWithLifecycle()
    val teamsPickState by viewModel.teamsPick.collectAsStateWithLifecycle()
    val mcpServers by viewModel.mcpServers.collectAsStateWithLifecycle()
    val mcpServerIds by viewModel.mcpServerIds.collectAsStateWithLifecycle()
    val promptFailure by viewModel.promptFailure.collectAsStateWithLifecycle()
    FailedActionToast(promptFailure, onShown = viewModel::clearPromptFailure)

    LaunchedEffect(conversationId, sent) {
        (sent ?: conversationId)?.let(onOpenConversation)
    }
    var graceOver by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(viewModel.agentId) {
        delay(SYNC_GRACE_MS)
        graceOver = true
    }
    var field by remember { mutableStateOf(TextFieldValue("")) }
    var expanded by rememberSaveable { mutableStateOf(false) }
    val images by viewModel.images.collectAsStateWithLifecycle()
    val imageError by viewModel.imageError.collectAsStateWithLifecycle()
    imageError?.let { message ->
        val toaster = com.exponential.app.ui.components.LocalToaster.current
        LaunchedEffect(message) { toaster.error(message) }
    }
    val attach = com.exponential.app.ui.agent.rememberComposerAttachPicker { picked ->
        when (picked) {
            is com.exponential.app.ui.agent.ComposerPick.Read -> viewModel.addImage(picked.uri, picked.bytes, picked.filename, picked.mime)
            is com.exponential.app.ui.agent.ComposerPick.Refused -> viewModel.refuseAttachment(picked.message)
        }
    }

    val row = agent
    val name = row?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS
    // EXP-1686 (chats.json `agentChat.emptyPlaceholder`): this page = no
    // conversation yet; session-chat.json copy.placeholder once one exists.
    val placeholder = if (row == null) SessionTree.ChatCopy.PLACEHOLDER else Chats.AgentChat.emptyPlaceholder(name)
    // #EXP-1732 (session-screen.json): THE screen of an agent (the Assistant)
    // with nothing under it yet: no counts, the Chat tab by default.
    var requestedTab by rememberSaveable { mutableStateOf<String?>(null) }
    val screen = SessionScreen.screen(
        kind = SessionScreen.kind(agentId = viewModel.agentId, appId = null, assistant = row?.isAssistant == true),
        wide = false,
        requested = requestedTab,
        open = null,
        counts = SessionScreen.Counts(),
    )
    val trigger = remember(row?.triggers) {
        com.exponential.app.domain.parseAgentTriggers(row?.triggers)
            .firstOrNull { it.enabled }
            ?.let { com.exponential.app.domain.AgentSettings.triggerSummary(it) }
    }
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        Scaffold(
            containerColor = Color.Transparent,
            modifier = Modifier.testTag("agent-channel-page"),
            topBar = {
                // Session 58cb32df: THE chat header (back · the agent's disc ·
                // its name over its trigger); no menu before the conversation
                // exists, the telephone and THE gear already (NEW-69, #EXP-1732).
                ChatHeader(
                    title = if (row == null) "" else name,
                    subtitle = if (row == null) null else SessionScreen.subtitle(SessionScreen.status(emptyList()), null, trigger),
                    lead = {
                        androidx.compose.foundation.layout.Box(
                            modifier = Modifier
                                .size(com.exponential.app.domain.Chats.Shell.ChatHeader.DISC.dp)
                                .clip(androidx.compose.foundation.shape.CircleShape)
                                .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted),
                            contentAlignment = Alignment.Center,
                        ) {
                            // #EXP-1448: the viewer's Assistant wears the brand
                            // mark (chats.json), never its stored icon.
                            if (row?.isAssistant == true) {
                                com.exponential.app.ui.components.ExponentialMark(size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC_GLYPH.dp, tint = MaterialTheme.colorScheme.onSurface)
                            } else {
                                AgentGlyph(icon = row?.icon, color = row?.color, size = com.exponential.app.domain.Chats.Shell.ChatHeader.DISC_GLYPH.dp)
                            }
                        }
                    },
                    onBack = onBack,
                    call = if (row == null) null else {
                        // A null icon = the call surface's brand mark (the Assistant's).
                        val callIcon = if (row.isAssistant) null else row.icon
                        { com.exponential.app.ui.call.AgentCallButton(agentId = viewModel.agentId, agentName = name, agentIcon = callIcon, agentColor = row.color) }
                    },
                    gear = if (row != null && screen.gear && onOpenAgentSettings != null) {
                        { onOpenAgentSettings?.invoke(viewModel.agentId) }
                    } else {
                        null
                    },
                    tabs = if (row != null) {
                        { SessionScreenTabs(tabs = screen.tabs, active = screen.active, onSelect = { requestedTab = it.wire }) }
                    } else {
                        null
                    },
                )
            },
        ) { padding ->
            val bottomInset = padding.calculateBottomPadding()
            val listPadding = PaddingValues(top = padding.calculateTopPadding() + 8.dp, bottom = bottomInset + 16.dp)
            if (row != null && screen.active != SessionScreen.Tab.Chat) {
                // Nothing under the agent yet: the tabs' bodies are empty.
                when (screen.active) {
                    SessionScreen.Tab.Issues -> SessionIssuesBody(
                        issues = emptyList(),
                        issueStatus = { null },
                        issueTitle = { null },
                        onOpen = {},
                        onAdd = null,
                        contentPadding = listPadding,
                    )
                    SessionScreen.Tab.Results -> ChatResultsPage(
                        rootId = "",
                        results = emptyList(),
                        cards = emptyList(),
                        onReply = { _, _ -> },
                        pullRequests = emptyList(),
                        teamPullRequests = emptyList(),
                        pullRequestIssues = emptyList(),
                        issues = emptyList(),
                        padding = padding,
                        onOpenChat = {},
                    )
                    else -> SessionSessionsBody(sessions = emptyList(), onOpen = {}, contentPadding = listPadding)
                }
                return@Scaffold
            }
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .consumeWindowInsets(PaddingValues(bottom = bottomInset))
                    .imePadding(),
            ) {
                when {
                    row == null && graceOver -> EmptyState(
                        title = NOT_FOUND,
                        description = NOT_FOUND_DETAIL,
                        icon = ExpIcons.settingsAgents,
                        modifier = Modifier.padding(padding),
                    )
                    row == null -> SkeletonRows(modifier = Modifier.padding(padding).padding(16.dp).align(Alignment.TopCenter))
                    // No conversation yet: the Conversation is empty.
                    else -> EmptyState(
                        title = placeholder,
                        icon = ExpIcons.byName(row.icon.orEmpty()) ?: ExpIcons.settingsAgents,
                        size = EmptyStateSize.List,
                        modifier = Modifier.padding(padding).align(Alignment.Center),
                    )
                }
                if (row != null) {
                    // #EXP-1782 (dock.json): the band sits under the body,
                    // no edge fade, no gradient, no backdrop strip.
                    Box(
                        contentAlignment = Alignment.TopCenter,
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .fillMaxWidth()
                            .padding(bottom = bottomInset),
                    ) {
                        // S-3: THE chat composer (its card, the "+" with
                        // Photo or file and Issue ›, surface `agent`); the
                        // first send creates the conversation.
                        com.exponential.app.ui.session.SteerComposer(
                            value = field,
                            onValueChange = { field = it },
                            onValueRewrite = { field = it },
                            pendingImages = images,
                            sending = sending,
                            live = true,
                            working = false,
                            onInterrupt = {},
                            expanded = expanded,
                            onExpandedChange = { expanded = it },
                            attach = attach,
                            menu = com.exponential.app.ui.session.SessionComposerMenu(
                                agent = row.runnerDefaults.cli ?: com.exponential.app.ui.components.DEFAULT_AGENT,
                                effort = "",
                                ultracode = false,
                                computerUse = false,
                                canComputerUse = false,
                                // #EXP-1793: MCP servers › (`mcpPick`).
                                mcpServers = mcpServers,
                                mcpServerIds = mcpServerIds,
                                issues = emptyList(),
                                readOnly = false,
                                onConfigure = {},
                                surface = com.exponential.app.domain.ComposerMenuSurface.Agent,
                                onToggleMcp = viewModel::toggleMcpServer,
                                // #EXP-1434: Prompts › on every composer; a
                                // pick inserts the body at the caret.
                                prompts = promptRows,
                                onPickPrompt = { prompt ->
                                    viewModel.loadPrompt(prompt.id) { body -> field = insertAtCaret(field, body) }
                                },
                                teams = teamsPickState?.let { state ->
                                    com.exponential.app.ui.agent.TeamsPickBinding(state.teams, state.picked, viewModel::setTeamsPick)
                                },
                            ),
                            onRemoveImage = viewModel::removeImage,
                            // EXP-1537: the field clears only if it still
                            // holds what went out (composer-send.json).
                            onSend = {
                                viewModel.send(field.text) { sentText ->
                                    val next = com.exponential.app.domain.ComposerSend.textAfterSend(sentText, field.text)
                                    if (next != field.text) field = TextFieldValue(next)
                                }
                            },
                            // The model is the agent's until the conversation runs.
                            model = null,
                            modelPickable = false,
                            onPickModel = {},
                            contextPercent = null,
                            hasUsage = false,
                            onOpenUsage = {},
                            trailing = null,
                            placeholder = placeholder,
                            capsuleTestTag = "agent-composer-capsule",
                        )
                    }
                }
            }
        }
    }
}

/** #EXP-1434: [text] replaces the selection (or lands at the caret), the
 *  caret after it. */
internal fun insertAtCaret(value: TextFieldValue, text: String): TextFieldValue {
    val start = minOf(value.selection.start, value.selection.end).coerceIn(0, value.text.length)
    val end = maxOf(value.selection.start, value.selection.end).coerceIn(0, value.text.length)
    val next = value.text.substring(0, start) + text + value.text.substring(end)
    return TextFieldValue(next, androidx.compose.ui.text.TextRange(start + text.length))
}
