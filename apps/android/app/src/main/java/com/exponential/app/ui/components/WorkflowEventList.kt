package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.WorkflowEventEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * EXP-1082/EXP-1068: a workflow's event log (the `workflow_events` shape).
 * Newest first (`at` desc, then id desc); each flat row = the kind's concept
 * glyph, the node's identifier as a mono lead-in ([nodeLabel]), the message on
 * one line and the local `HH:mm`. A row naming a run opens it
 * ([onOpenSession]). An empty log renders nothing.
 */
@Composable
fun WorkflowEventList(
    events: List<WorkflowEventEntity>,
    modifier: Modifier = Modifier,
    /** A node id → its identifier (`EXP-14`); null = no lead-in. */
    nodeLabel: (String) -> String? = { null },
    /** Opens the run an event names; null = rows are not clickable. */
    onOpenSession: ((String) -> Unit)? = null,
) {
    if (events.isEmpty()) return
    val ordered = events.sortedWith(
        compareByDescending<WorkflowEventEntity> { WireTimestamps.parseEpochMs(it.at) ?: 0L }
            .thenByDescending { it.id },
    )
    Column(modifier) {
        for (event in ordered) {
            val sessionId = event.sessionId
            val open = if (sessionId != null && onOpenSession != null) {
                Modifier.clickable { onOpenSession(sessionId) }
            } else {
                Modifier
            }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("workflow-event-row")
                    .flatRow()
                    .then(open)
                    .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    workflowEventIcon(event.kind),
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Spacer(Modifier.width(10.dp))
                event.nodeId?.let(nodeLabel)?.let { label ->
                    Text(
                        label,
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                }
                Text(
                    event.message,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    workflowEventTime(event.at),
                    style = MaterialTheme.typography.labelSmall,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/** The concept glyph of a contract `wfEventKind`; unknown kinds read as info. */
internal fun workflowEventIcon(kind: String): ImageVector = when (kind) {
    DomainContract.wfEventKindNodeStarted -> ExpIcons.actionRun
    DomainContract.wfEventKindRetrying -> ExpIcons.runResume
    DomainContract.wfEventKindReviewStarted,
    DomainContract.wfEventKindReviewVerdict,
    DomainContract.wfEventKindReviewNoVerdict,
    -> ExpIcons.codingInReview
    DomainContract.wfEventKindLanded,
    DomainContract.wfEventKindCompleted,
    -> ExpIcons.uiCheck
    DomainContract.wfEventKindFinalPrOpened,
    DomainContract.wfEventKindFinalPrReopened,
    -> ExpIcons.prOpen
    DomainContract.wfEventKindFailed,
    DomainContract.wfEventKindGaveUp,
    -> ExpIcons.uiWarning
    DomainContract.wfEventKindAccountPicked,
    DomainContract.wfEventKindAccountSwitched,
    DomainContract.wfEventKindWaitingReset,
    -> ExpIcons.navAccount
    DomainContract.wfEventKindQuestionAsked,
    DomainContract.wfEventKindQuestionAnswered,
    -> ExpIcons.uiHelp
    DomainContract.wfEventKindCancelled,
    DomainContract.wfEventKindSkipped,
    -> ExpIcons.uiClose
    else -> ExpIcons.uiInfo
}

private val EVENT_TIME: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")

/** The event's local `HH:mm`; empty for an unreadable stamp. */
private fun workflowEventTime(at: String): String {
    val ms = WireTimestamps.parseEpochMs(at) ?: return ""
    return EVENT_TIME.format(Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()))
}
