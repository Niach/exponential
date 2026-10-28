package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/** S-3: the synced agent definitions by id, the marks a [SessionXref] draws
 *  (the host screen provides them; empty = every agent the default mark). */
val LocalAgentMarks = staticCompositionLocalOf<Map<String, AgentDefinitionEntity>> { emptyMap() }

/**
 * S-3 inter-session links ×4 (`session-thread.json` "S-3 INTER-SESSION
 * LINKS", `copy.xref`, `testIds.xref`; styleguide entry `session-xref`): THE
 * compact header naming ANOTHER session in ONE xs line = the muted lead
 * (`Message from` · `Report from` · `Started by` · `Reported to`), the
 * agent's mark ([AgentAvatar] at 16dp keyed by `agentId` through
 * [LocalAgentMarks], else the session glyph), the session's name (foreground,
 * medium, one line) and a muted chevron, 4dp apart. The whole line is the
 * tap target: [onOpen] pushes that session on the back stack (null = inert).
 */
@Composable
fun SessionXref(
    xref: SessionTree.SessionXref,
    onOpen: ((sessionId: String) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    val lead = SessionTree.ThreadCopy.XREF[xref.kind].orEmpty()
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val style = MaterialTheme.typography.bodySmall
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(6.dp))
            .then(
                if (onOpen != null) {
                    Modifier.clickable(role = Role.Button) { onOpen(xref.sessionId) }
                } else {
                    Modifier
                },
            )
            .padding(vertical = 2.dp)
            .semantics(mergeDescendants = true) { contentDescription = "$lead ${xref.name}" }
            .testTag(SessionTree.ThreadCopy.TEST_ID_XREF),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(lead, style = style, color = muted, maxLines = 1)
        val agentId = xref.agentId
        if (agentId != null) {
            val agent = LocalAgentMarks.current[agentId]
            AgentAvatar(icon = agent?.icon, color = agent?.color, size = 16.dp)
        } else {
            Icon(ExpIcons.codingAssistant, contentDescription = null, tint = muted, modifier = Modifier.size(16.dp))
        }
        Text(
            xref.name,
            style = style.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Icon(ExpIcons.uiChevronRight, contentDescription = null, tint = muted, modifier = Modifier.size(14.dp))
    }
}
