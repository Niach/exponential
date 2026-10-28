package com.exponential.app.ui.agent

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.AgentBubbleDisc
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * S-3 (composer-home.json `agentRow`): THE AGENT ROW over the New-chat
 * composer card — the label line (`To <name>` / `Temporary session`), then
 * single-select circles: the chats agent BUBBLE disc at [ComposerAgentRow.SIZE]
 * (the Assistant's brand mark first, then the team agents), then the dotted
 * circle (concept `ui-icon-placeholder`) = Temporary session. The selected
 * circle wears the foreground ring, the others the glass hairline; no name
 * under a circle (it is the label and the accessibility label).
 */
@Composable
internal fun ComposerAgentRowView(
    state: ComposerAgentRow.State<SessionTree.AgentRow>,
    onPick: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val size = ComposerAgentRow.SIZE.dp
    Column(modifier = modifier.fillMaxWidth().testTag(ComposerAgentRow.TEST_ID_ROW)) {
        Text(
            state.label,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            modifier = Modifier.padding(start = 4.dp).testTag(ComposerAgentRow.TEST_ID_LABEL),
        )
        Spacer(Modifier.height(ComposerAgentRow.LABEL_TO_ROW.dp))
        Row(
            modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 2.dp),
            horizontalArrangement = Arrangement.spacedBy(ComposerAgentRow.GAP.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            state.agents.forEach { agent ->
                val picked = state.selected == agent.agentId
                OptionCircle(
                    id = agent.agentId,
                    name = agent.name,
                    picked = picked,
                    onPick = onPick,
                ) {
                    AgentBubbleDisc(
                        icon = agent.icon,
                        color = agent.color,
                        assistant = agent.assistant,
                        size = size,
                        ring = if (picked) MaterialTheme.colorScheme.onSurface else GlassTokens.StrokeCard,
                        ringWidth = if (picked) ComposerAgentRow.RING_WIDTH.dp else GlassTokens.Hairline,
                        ringGap = ComposerAgentRow.RING_GAP.dp,
                        glyph = 16.dp,
                    )
                }
            }
            val temporary = state.selected == ComposerAgentRow.TEMPORARY_PICK
            OptionCircle(
                id = ComposerAgentRow.TEMPORARY_PICK,
                name = ComposerAgentRow.COPY_TEMPORARY,
                picked = temporary,
                onPick = onPick,
            ) {
                Box(
                    modifier = Modifier
                        .size(size)
                        .border(
                            if (temporary) ComposerAgentRow.RING_WIDTH.dp else GlassTokens.Hairline,
                            if (temporary) MaterialTheme.colorScheme.onSurface else GlassTokens.StrokeCard,
                            CircleShape,
                        )
                        .padding(ComposerAgentRow.RING_GAP.dp)
                        .clip(CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        ExpIcons.uiIconPlaceholder,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.size(size - (ComposerAgentRow.RING_GAP * 2).dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun OptionCircle(
    id: String,
    name: String,
    picked: Boolean,
    onPick: (String) -> Unit,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = Modifier
            .clip(CircleShape)
            .clickable(role = Role.RadioButton) { onPick(id) }
            .semantics {
                contentDescription = name
                selected = picked
            }
            .testTag(ComposerAgentRow.optionTestId(id)),
    ) { content() }
}

/** The row's input: the launch data's [SessionTree.listAgents] rows (already
 *  filtered + Assistant first) in the domain shape. */
internal fun composerAgentRowState(
    agents: List<SessionTree.AgentRow>,
    me: String?,
    picked: String?,
): ComposerAgentRow.State<SessionTree.AgentRow> {
    val order = agents.withIndex().associate { (i, a) -> a.agentId to i }
    return ComposerAgentRow.composerAgentRow(agents, me, picked) { a ->
        ComposerAgentRow.Agent(
            id = a.agentId,
            role = if (a.assistant) com.exponential.app.domain.DomainContract.agentRoleAssistant else com.exponential.app.domain.DomainContract.agentRoleAgent,
            // listAgents already dropped other members' Assistants.
            ownerId = if (a.assistant) me else null,
            name = a.name,
            sortOrder = order.getValue(a.agentId),
        )
    }
}
