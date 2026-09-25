package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.domain.IssueGraph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-980: ONE view of the `blocks` graph ([IssueGraph]) per client, used in
// three places — the list row's badge overlay, the Work screen's stack/batch
// overlay on its Issue face, and the blocked-start dialog. EXP-1057: every
// one of them draws the grid ([IssueGraphPopover]), identical ×4.

/**
 * The list row's blocks badge: `⊘ 2` for open blockers (destructive), `⃠ 1`
 * for the issues this one blocks (muted), or both. Never rendered with two
 * zero counts — the caller passes null then. [onClick] opens the mini-graph;
 * without it the badge is a plain decoration.
 */
@Composable
fun BlocksBadge(
    counts: IssueGraph.Counts,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    if (counts.blockedBy <= 0 && counts.blocking <= 0) return
    val label = remember(counts) { IssueGraph.blocksBadgeLabel(counts) }
    val shape = remember { RoundedCornerShape(BadgeRadius) }
    Row(
        modifier = modifier
            .testTag("blocks-badge")
            .clip(shape)
            .background(GlassTokens.RowFillActive, shape)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 5.dp, vertical = 2.dp)
            .semantics { contentDescription = label },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (counts.blockedBy > 0) {
            BadgeSide(
                icon = ExpIcons.relationBlockedBy,
                count = counts.blockedBy,
                tint = MaterialTheme.colorScheme.error,
            )
        }
        if (counts.blocking > 0) {
            BadgeSide(
                icon = ExpIcons.relationBlocks,
                count = counts.blocking,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

@Composable
private fun BadgeSide(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    count: Int,
    tint: androidx.compose.ui.graphics.Color,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(11.dp), tint = tint)
        Text(count.toString(), style = MaterialTheme.typography.labelSmall, color = tint)
    }
}

/**
 * The graph itself. EXP-1057: phones draw the SAME grid the web and desktop
 * draw — [IssueGraphPopover] — so this is only the name the list badge, the
 * Work screen's overlay and the blocked-start dialog already call.
 */
@Composable
fun IssueGraphList(
    graph: IssueGraph.Graph,
    issuesById: Map<String, IssueEntity>,
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    IssueGraphPopover(
        graph = graph,
        issuesById = issuesById,
        onOpenIssue = onOpenIssue,
        modifier = modifier,
    )
}

@Composable
private fun GraphNote(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

/**
 * The badge's overlay: the graph of [subjectIds] in a sheet. Builds the graph
 * itself off the synced pools the host already observes, so no caller has to
 * know the rule.
 */
@Composable
fun IssueGraphSheet(
    subjectIds: List<String>,
    issues: List<IssueEntity>,
    relations: List<IssueRelationEntity>,
    onOpenIssue: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val graph = remember(subjectIds, issues, relations) {
        IssueGraph.blockGraph(subjectIds, relations, issues)
    }
    val issuesById = remember(issues) { issues.associateBy { it.id } }
    GlassSheet(title = "Blocking relations", onDismiss = onDismiss) {
        if (graph.isEmpty) {
            // Only reachable while the subject itself has not synced — a
            // synced subject is always a node of its own graph.
            GraphNote("This issue has no blocking relations.")
        } else {
            IssueGraphList(
                graph = graph,
                issuesById = issuesById,
                onOpenIssue = { id ->
                    onDismiss()
                    onOpenIssue(id)
                },
            )
        }
        Spacer(Modifier.size(8.dp))
    }
}

private val BadgeRadius = 6.dp
