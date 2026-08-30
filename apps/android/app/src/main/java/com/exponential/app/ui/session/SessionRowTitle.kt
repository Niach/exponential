package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.theme.TextEmphasis

// EXP-688: a coding session's IDENTITY line — status dot, mono identifier,
// what the run is about. The Agents list row and the steering screen's header
// render the SAME composable so the two can't drift: the header used to say
// only "Live · macbook", which never named the issue being worked on.

/**
 * One session's identity line. [dot] is the caller's status dot — the list
 * derives it from the synced row's display state, the steering header from the
 * live phase — everything else is shared.
 */
@Composable
internal fun SessionRowTitle(
    identifier: String,
    title: String,
    modifier: Modifier = Modifier,
    dot: @Composable () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = modifier) {
        dot()
        Spacer(Modifier.width(12.dp))
        Text(
            identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
        )
        Spacer(Modifier.width(8.dp))
        Text(
            title,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
    }
}

/** The identifier column — an issue-scoped run whose issue hasn't synced (and
 *  a batch/action run, which has none) still needs something in the slot. */
internal fun sessionRowIdentifier(issue: IssueEntity?): String = issue?.identifier ?: "…"

/**
 * What the run is about: the issue's title, an action run's `action_name`
 * snapshot (EXP-253), else a batch run — never "not synced", except for an
 * issue-scoped session whose issue genuinely hasn't landed yet.
 */
internal fun sessionRowTitle(session: CodingSessionEntity, issue: IssueEntity?): String = when {
    issue != null -> issue.title.ifBlank { "Untitled issue" }
    session.issueId == null -> session.actionName?.takeIf { it.isNotBlank() } ?: "Batch run"
    else -> "Issue not synced yet"
}
