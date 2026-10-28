package com.exponential.app.ui.thread

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
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
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.uploadedImageIds
import com.exponential.app.domain.uploadedSteerFiles
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FloatingBarEdge
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
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * NEW-7 (web `routes/t/$teamSlug/agents/$agentId.tsx`): a team agent's page
 * = the CALLER's own conversation with it, the root session with that
 * `agent_id` and `user_id` = me. Once it exists the page IS the thread
 * screen's agent channel ([onOpenConversation] swaps it in); until then
 * this is the same screen empty (the agent's coloured icon + name, the
 * composer `Message <name>`), whose first send creates the conversation
 * with `sessions.send({agentId, text})`.
 */
@HiltViewModel
class AgentConversationViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val sender: SessionSender,
    private val imagesApi: com.exponential.app.data.api.IssueImagesApi,
) : ViewModel() {

    val agentId: String = savedStateHandle["agentId"] ?: ""

    private val dbFlow = accountDatabaseFlow(auth, holder)

    /** The synced definition row (name, icon, colour); null while loading or gone. */
    val agent: StateFlow<AgentDefinitionEntity?> =
        dbFlow.scopedQuery<AgentDefinitionEntity?>(null) { it.agentDefinitionDao().observeById(agentId) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** My conversation session with the agent, once it exists. */
    val conversationId: StateFlow<String?> = combine(
        dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }.map { rows -> rows.map(SessionTree::session) },
        auth.userId,
    ) { sessions, me -> SessionTree.conversations(sessions, me)[agentId] }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

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

    /** S-3: the composer's pending attachments (THE thread composer's +
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
     *  creates my DM (the pending files upload against the team first). */
    fun send(text: String, onSent: () -> Unit) {
        if ((text.isBlank() && _images.value.isEmpty()) || _sending.value) return
        val accountId = auth.activeAccountId.value ?: return
        _sending.value = true
        viewModelScope.launch {
            try {
                val teamId = agent.value?.teamId
                var current = _images.value
                if (teamId != null) {
                    for ((index, image) in current.withIndex()) {
                        if (image.uploadedId != null) continue
                        val uploaded = imagesApi.uploadTeamSessionImage(accountId, teamId, image.bytes, image.filename, image.contentType)
                        current = current.mapIndexed { i, entry ->
                            if (i == index) entry.copy(uploadedId = uploaded.id, uploadedName = uploaded.filename) else entry
                        }
                        _images.value = current
                    }
                }
                val body = com.exponential.app.domain.AgentComposerPrompt.build(
                    text,
                    current.uploadedImageIds(),
                    current.uploadedSteerFiles(),
                ).orEmpty()
                val result = sender.send(
                    SessionsSendInput.toAgent(agentId, body, current.mapNotNull { it.uploadedId }),
                )
                onSent()
                _images.value = emptyList()
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
    viewModel: AgentConversationViewModel = hiltViewModel(),
) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    val conversationId by viewModel.conversationId.collectAsStateWithLifecycle()
    val sent by viewModel.sent.collectAsStateWithLifecycle()
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

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
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val filePicker = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris: List<android.net.Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            for (uri in uris) when (val picked = com.exponential.app.ui.agent.readComposerPick(context, uri)) {
                is com.exponential.app.ui.agent.ComposerPick.Read -> viewModel.addImage(picked.uri, picked.bytes, picked.filename, picked.mime)
                is com.exponential.app.ui.agent.ComposerPick.Refused -> viewModel.refuseAttachment(picked.message)
            }
        }
    }

    val row = agent
    val name = row?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS
    val placeholder = ThreadUiCopy.messageAgent(name)
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        Scaffold(
            containerColor = Color.Transparent,
            modifier = Modifier.testTag("agent-channel-page"),
            topBar = {
                // Thread 58cb32df: THE chat header (back · the agent's disc ·
                // its name over `Agent`); no menu before the conversation
                // exists, the telephone already (NEW-69).
                ChatHeader(
                    title = if (row == null) "" else name,
                    subtitle = if (row == null) null else "Agent",
                    lead = {
                        androidx.compose.foundation.layout.Box(
                            modifier = Modifier
                                .size(44.dp)
                                .clip(androidx.compose.foundation.shape.CircleShape)
                                .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted),
                            contentAlignment = Alignment.Center,
                        ) { AgentGlyph(icon = row?.icon, color = row?.color, size = 22.dp) }
                    },
                    onBack = onBack,
                    call = if (row == null) null else {
                        { com.exponential.app.ui.call.AgentCallButton(agentId = viewModel.agentId, agentName = name) }
                    },
                )
            },
        ) { padding ->
            val bottomInset = padding.calculateBottomPadding()
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
                    FloatingBarEdge(
                        bottomInset = bottomInset,
                        visible = !expanded,
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .fillMaxWidth()
                            .padding(bottom = bottomInset),
                    ) {
                        // S-3: THE thread composer (its card, the "+" with
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
                            onPickImages = { filePicker.launch(arrayOf("*/*")) },
                            menu = com.exponential.app.ui.session.SessionComposerMenu(
                                agent = row.cli?.takeIf { it.isNotBlank() } ?: com.exponential.app.ui.components.DEFAULT_AGENT,
                                effort = "",
                                ultracode = false,
                                computerUse = false,
                                canComputerUse = false,
                                mcpServers = emptyList(),
                                mcpServerIds = emptyList(),
                                issues = emptyList(),
                                readOnly = false,
                                onConfigure = {},
                                onImplement = {},
                                surface = com.exponential.app.domain.ComposerMenuSurface.Agent,
                            ),
                            onRemoveImage = viewModel::removeImage,
                            onSend = { viewModel.send(field.text) { field = TextFieldValue("") } },
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
