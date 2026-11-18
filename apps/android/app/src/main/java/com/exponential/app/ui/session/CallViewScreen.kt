package com.exponential.app.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.chat.AgentGlyph
import com.exponential.app.ui.chat.ChatAgent
import com.exponential.app.ui.chat.ChatConversation
import com.exponential.app.ui.chat.ChatHeader
import com.exponential.app.ui.chat.ChatRowMeta
import com.exponential.app.ui.chat.clockTime
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.markdown.MarkdownView
import kotlinx.coroutines.delay

/**
 * #EXP-1943 (session-card.json `call`, `callViewCases`): THE call view, a
 * call session's own screen: the chat header (title = [SessionTree.CallView.title],
 * subtitle = when · agent), the summary under its section band, then the
 * spoken turns as the agent chat's bubbles. Opened from the call's card.
 */
@Composable
internal fun CallViewPage(viewModel: AgentSessionViewModel, onBack: () -> Unit) {
    val row by viewModel.session.collectAsStateWithLifecycle()
    val messages by viewModel.callMessages.collectAsStateWithLifecycle()
    val agentDef by viewModel.callAgent.collectAsStateWithLifecycle()
    val header by viewModel.header.collectAsStateWithLifecycle()
    val viewerId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val session = remember(row) { row?.let(SessionTree::session) }
    val running = session?.let(SessionTree::callRunning) == true
    // The title's timer ticks every second while the call runs.
    val now by produceState(SessionTree.nowIso(), running) {
        value = SessionTree.nowIso()
        while (running) {
            delay(1_000)
            value = SessionTree.nowIso()
        }
    }
    val agent = agentDef?.let { def ->
        ChatAgent(
            name = def.name.takeIf { it.isNotBlank() } ?: SessionTree.ListCopy.AGENTS,
            icon = def.icon,
            color = def.color,
            assistant = def.isAssistant,
        )
    }
    val agentName = agent?.name ?: header?.chatTitle.orEmpty()
    val view = session?.let {
        SessionTree.callView(it, messages, agentName, now, Chats.deviceUtcOffsetMinutes())
    }
    val times = remember(messages) { messages.associate { it.id to it.createdAt } }
    CallViewContent(
        view = view,
        agent = agent,
        agentId = agentDef?.id,
        viewerId = viewerId,
        time = { id -> times[id]?.let(::clockTime) },
        onBack = onBack,
    )
}

/** The call view, stateless; [view] null = not synced yet. */
@Composable
internal fun CallViewContent(
    view: SessionTree.CallView?,
    agent: ChatAgent?,
    agentId: String?,
    viewerId: String?,
    time: (messageId: String) -> String?,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Scaffold(
        modifier = modifier,
        containerColor = Color.Transparent,
        topBar = {
            ChatHeader(
                title = view?.title.orEmpty(),
                subtitle = view?.subtitle,
                lead = { if (agent != null) CallAgentDisc(agent) },
                onBack = onBack,
            )
        },
    ) { padding ->
        if (view == null) return@Scaffold
        CallViewBody(
            view = view,
            heading = false,
            agent = agent,
            agentId = agentId,
            viewerId = viewerId,
            time = time,
            modifier = Modifier.fillMaxSize().padding(padding),
            transcriptModifier = { Modifier.weight(1f) },
        )
    }
}

/**
 * The call view's body (web `CallViewBody`): [heading] = the title over the
 * subtitle on top (a specimen; the screen's header has them), the summary
 * under its section band with the hairline under it, then the spoken turns
 * as the agent chat's bubbles in [transcriptModifier]'s space.
 */
@Composable
internal fun CallViewBody(
    view: SessionTree.CallView,
    heading: Boolean,
    agent: ChatAgent?,
    agentId: String?,
    viewerId: String?,
    time: (messageId: String) -> String?,
    modifier: Modifier = Modifier,
    transcriptModifier: ColumnScope.() -> Modifier = { Modifier.weight(1f) },
    /** A specimen: the transcript opens at its head. */
    startAtTop: Boolean = false,
) {
    Column(modifier.testTag("call-view")) {
        if (heading) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp).testTag("call-view-heading")) {
                Text(
                    view.title,
                    style = MaterialTheme.typography.titleSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    view.subtitle,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        view.summary?.let { summary ->
            Column(Modifier.fillMaxWidth().testTag("call-view-summary")) {
                SectionHeader(view.summaryLabel, modifier = Modifier.padding(top = 8.dp))
                Box(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(max = 220.dp)
                        .verticalScroll(rememberScrollState())
                        .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 12.dp),
                ) {
                    MarkdownView(summary, softBreaksAsNewlines = true)
                }
                HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            }
        }
        ChatConversation(
            rows = view.rows,
            rootId = view.sessionId,
            meta = { row ->
                ChatRowMeta(
                    authorName = if (row.author == "agent") agent?.name.orEmpty() else "",
                    time = time(row.id).orEmpty(),
                )
            },
            onOpenCard = {},
            onAnswer = { _, _, _ -> },
            onDecidePlan = { _, _, _ -> },
            canUnqueue = { false },
            onUnqueue = {},
            modifier = Modifier.fillMaxWidth().then(transcriptModifier()),
            agent = agent,
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 24.dp),
            chat = TranscriptGrouping.Chat(
                sessionId = view.sessionId,
                title = agent?.name.orEmpty(),
                agentId = agentId,
                originSessionId = null,
            ),
            viewerId = viewerId,
            startAtTop = startAtTop,
        )
    }
}

/** The chat header's lead disc of the call's agent (the agent chat's own). */
@Composable
private fun CallAgentDisc(agent: ChatAgent) {
    Box(
        modifier = Modifier
            .size(Chats.Shell.ChatHeader.DISC.dp)
            .clip(CircleShape)
            .background(com.exponential.app.ui.theme.DesignTokens.Palette.Muted, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (agent.assistant) {
            com.exponential.app.ui.components.ExponentialMark(
                size = Chats.Shell.ChatHeader.DISC_GLYPH.dp,
                tint = MaterialTheme.colorScheme.onSurface,
            )
        } else {
            AgentGlyph(icon = agent.icon, color = agent.color, size = Chats.Shell.ChatHeader.DISC_GLYPH.dp)
        }
    }
}
