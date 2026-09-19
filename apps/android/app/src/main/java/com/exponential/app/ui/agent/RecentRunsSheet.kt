package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunIdentifier
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-923: the caller's finished person-started runs (EXP-746), behind the
 * Agent page's history button instead of under its composer. History is
 * something the reader goes LOOKING for, so it gets a surface of its own —
 * and, unlike the band it replaces, it has no folded state to get in the way:
 * every run is on screen, the tree nested as everywhere else (EXP-965).
 *
 * An automated run belongs to the Automations tab's "Recent automated runs"
 * and never lists here.
 */
@Composable
fun RecentRunsSheet(
    pastRuns: List<PastRunRow>,
    onOpenRun: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = "Recent", onDismiss = onDismiss) {
        if (pastRuns.isEmpty()) {
            Text(
                "No recent runs",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
            return@GlassSheet
        }
        // EXP-897: a child run that finished under its parent reads as one
        // story here too — capped BEFORE nesting, so the cap keeps meaning
        // what it meant.
        val tree = remember(pastRuns) {
            SessionTree.nest(
                pastRuns,
                { it.session.id },
                { it.session.parentSessionId },
                { it.session.startedAt },
            )
        }
        val guides = remember(tree) { TreeGuides.compute(tree.map { it.depth }) }
        LazyColumn(
            modifier = Modifier.fillMaxWidth().testTag("recent-runs-sheet"),
            contentPadding = PaddingValues(horizontal = 16.dp),
        ) {
            itemsIndexed(tree, key = { _, entry -> entry.session.session.id }) { index, entry ->
                val row = entry.session
                TreeGuidesRow(depth = entry.depth, guide = guides.getOrNull(index)) {
                    EndedRunRow(
                        // The ×4 rule (domain `pastRunTitle`): the issue's
                        // title, a sync placeholder while it is missing, the
                        // action_name snapshot (a chat run's reads "Chat"),
                        // else the batch.
                        title = pastRunTitle(row.session, row.issue, row.batchIssues),
                        // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
                        identifier = pastRunIdentifier(row.session, row.issue, row.batchIssues),
                        timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                        byline = pastRunByline(
                            deviceLabel = row.device.displayLabel,
                            timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                        ),
                        onOpen = {
                            onDismiss()
                            onOpenRun(row.session.id)
                        },
                    )
                }
            }
        }
    }
}
