package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-980: ONE view of the `blocks` graph ([IssueGraph]) per client, used in
// three places — the list row's badge overlay, the Work screen's stack/batch
// overlay on its Issue face, and the blocked-start dialog. A phone draws the
// graph as a LIST grouped by wave (the desktop and web draw the real grid):
// section `Wave 1`, `Wave 2`, … over rows of status glyph + issue chip, each
// with the chips of its own direct blockers underneath. Cycle edges are red,
// subjects are highlighted, and the two notes under the graph are the shared
// strings.

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
 * The graph itself: one section per wave, the nodes of that wave as rows, and
 * under each row the chips of its direct blockers INSIDE the graph. Red = an
 * edge on a cycle; a subject row is highlighted. [onOpenIssue] is the same
 * navigation the stack overlay's chips take.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IssueGraphList(
    graph: IssueGraph.Graph,
    issuesById: Map<String, IssueEntity>,
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (graph.isEmpty) return
    // Every blocker of a node that is drawn here, with its cycle flag.
    val incoming = remember(graph) { graph.edges.groupBy { it.to } }
    // The nodes are already ordered by (wave, lane), so grouping keeps it.
    val waves = remember(graph) { graph.nodes.groupBy { it.wave } }
    Column(modifier = modifier.fillMaxWidth().testTag("issue-graph")) {
        waves.forEach { (wave, nodes) ->
            SectionHeader("Wave ${wave + 1}")
            nodes.forEach { node ->
                val issue = issuesById[node.id] ?: return@forEach
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        // EXP-818: an overlay row is a LIST row.
                        .flatRow()
                        .then(
                            if (node.subject) {
                                Modifier.border(
                                    1.dp,
                                    MaterialTheme.colorScheme.primary.copy(alpha = 0.45f),
                                    RoundedCornerShape(GlassTokens.RowRadius),
                                )
                            } else {
                                Modifier
                            },
                        )
                        .clickable { onOpenIssue(issue.id) }
                        .padding(
                            horizontal = GlassTokens.RowPaddingH,
                            vertical = GlassTokens.RowPaddingV,
                        ),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        StatusIcon(IssueStatus.fromWire(issue.status), size = 14.dp)
                        Spacer(Modifier.width(8.dp))
                        // The chip names the issue; the title rides beside it
                        // so a node still reads as a sentence, not a code.
                        IssueChip(
                            identifier = issue.identifier,
                            title = null,
                            status = null,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            issue.title,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    val blockers = incoming[node.id].orEmpty()
                    if (blockers.isNotEmpty()) {
                        Text(
                            "Blocked by",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.padding(top = 4.dp),
                        )
                        FlowRow(
                            modifier = Modifier.fillMaxWidth().padding(top = 2.dp),
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            blockers.forEach { edge ->
                                val blocker = issuesById[edge.from] ?: return@forEach
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    if (edge.cycle) {
                                        Icon(
                                            ExpIcons.relationBlockedBy,
                                            contentDescription = null,
                                            modifier = Modifier.size(12.dp),
                                            tint = MaterialTheme.colorScheme.error,
                                        )
                                        Spacer(Modifier.width(4.dp))
                                    }
                                    IssueChip(
                                        identifier = blocker.identifier,
                                        title = blocker.title,
                                        status = null,
                                        onClick = { onOpenIssue(blocker.id) },
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
        if (graph.hasCycle) GraphNote(IssueGraph.CYCLE_NOTE)
        if (graph.truncated) GraphNote(IssueGraph.TRUNCATED_NOTE)
    }
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
