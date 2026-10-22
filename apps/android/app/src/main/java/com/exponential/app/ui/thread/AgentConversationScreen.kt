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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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

    /** The first message: `sessions.send({agentId, text})` creates my DM. */
    fun send(text: String, onSent: () -> Unit) {
        if (text.isBlank() || _sending.value) return
        _sending.value = true
        viewModelScope.launch {
            try {
                val result = sender.send(SessionsSendInput.toAgent(agentId, text))
                onSent()
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
    var draft by rememberSaveable { mutableStateOf("") }
    var expanded by rememberSaveable { mutableStateOf(false) }

    val row = agent
    val name = row?.name?.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS
    val placeholder = ThreadUiCopy.messageAgent(name)
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        Scaffold(
            containerColor = Color.Transparent,
            modifier = Modifier.testTag("agent-channel-page"),
            topBar = {
                HeaderEdgeChrome {
                    AgentChannelHeader(
                        name = if (row == null) "" else name,
                        icon = row?.icon,
                        color = row?.color,
                        onBack = onBack,
                        modifier = Modifier.statusBarsPadding(),
                    )
                }
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
                        FloatingComposer(
                            expanded = expanded,
                            onExpandedChange = { expanded = it },
                            placeholder = placeholder,
                            hasDraft = draft.isNotBlank(),
                            capsuleTestTag = "agent-composer-capsule",
                            modifier = Modifier.testTag("agent-composer"),
                        ) {
                            GlassComposer(
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                submit = {
                                    ComposerSubmitButton(
                                        ExpIcons.uiSubmit,
                                        contentDescription = ThreadUiCopy.SEND,
                                        onClick = { viewModel.send(draft) { draft = "" } },
                                        enabled = draft.isNotBlank(),
                                        sending = sending,
                                        modifier = Modifier.testTag("agent-composer-submit"),
                                    )
                                },
                            ) {
                                GlassTextField(
                                    value = draft,
                                    onValueChange = { draft = it },
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .testTag("agent-composer-field")
                                        .then(fieldModifier),
                                    placeholder = placeholder,
                                    minLines = 2,
                                    maxLines = 8,
                                    bordered = false,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}
