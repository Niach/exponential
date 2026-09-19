package com.exponential.app.ui.workflows

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.captionNode
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-981: a workflow's graph as a PHONE draws it — the same wave-grouped list
// the blocks mini-graph uses (`ui/components/IssueGraphList.kt`, EXP-980), fed
// workflow nodes instead of issues. Web and the desktop draw the real grid with
// edges; here a section per wave over flat rows, each row carrying the chips of
// its own direct blockers, is what fits.
//
// Position is the server's: `wave` is the section, `lane` the order inside it
// (the rows arrive already sorted). Nothing here lays anything out.

/** The colour a node's caption paints in ([WorkflowView.nodeTone]). */
@Composable
internal fun workflowToneColor(tone: WorkflowView.Tone): Color = when (tone) {
    WorkflowView.Tone.Amber -> DesignTokens.Semantic.Yellow
    WorkflowView.Tone.Danger -> MaterialTheme.colorScheme.error
    WorkflowView.Tone.Success -> DesignTokens.Semantic.Green
    WorkflowView.Tone.Active -> MaterialTheme.colorScheme.onSurface
    WorkflowView.Tone.Muted ->
        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
}

/**
 * The graph: one section per wave, the wave's nodes as rows, and under each
 * row the chips of its direct blockers INSIDE the workflow. A compound node is
 * a STACKED card (a second card edge peeking out behind it); a node on a cycle
 * — and every cycle edge — is red. A node whose issue row has not synced
 * renders its caption alone and never crashes.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun WorkflowGraphList(
    graph: WorkflowGraph,
    /** The workflow's status — a draft's captions name the plan, not states. */
    workflowStatus: String,
    cycleNote: String?,
    onSelectNode: (WorkflowNodeEntity) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (graph.nodes.isEmpty()) return
    val incoming = remember(graph.edges) { graph.edges.groupBy { it.to } }
    val nodesById = remember(graph.nodes) { graph.nodes.associateBy { it.id } }
    // Already ordered by (wave, lane) — grouping keeps it.
    val waves = remember(graph.nodes) { graph.nodes.groupBy { it.wave ?: 0 } }
    Column(modifier = modifier.fillMaxWidth().testTag("workflow-graph")) {
        waves.forEach { (wave, nodes) ->
            SectionHeader("Wave ${wave + 1}")
            nodes.forEach { node ->
                WorkflowNodeRow(
                    node = node,
                    graph = graph,
                    workflowStatus = workflowStatus,
                    blockers = incoming[node.id].orEmpty(),
                    nodesById = nodesById,
                    onClick = { onSelectNode(node) },
                )
            }
        }
        cycleNote?.let { note ->
            Text(
                note,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier
                    .padding(horizontal = 12.dp, vertical = 8.dp)
                    .testTag("workflow-cycle-note"),
            )
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WorkflowNodeRow(
    node: WorkflowNodeEntity,
    graph: WorkflowGraph,
    workflowStatus: String,
    blockers: List<WorkflowView.Edge>,
    nodesById: Map<String, WorkflowNodeEntity>,
    onClick: () -> Unit,
) {
    val issue = graph.issuesById[node.issueId]
    val caption = remember(node, workflowStatus) {
        WorkflowView.nodeCaption(node.captionNode, workflowStatus)
    }
    val tone = workflowToneColor(WorkflowView.nodeTone(node.state))
    val shape = remember { RoundedCornerShape(GlassTokens.RowRadius) }
    val compound = node.memberIssueIds.isNotEmpty()
    Column(modifier = Modifier.fillMaxWidth()) {
        // The compound node's second card edge, peeking out behind the row:
        // one node, several issues, run as ONE batch on one branch.
        if (compound) {
            Box(
                modifier = Modifier
                    .padding(horizontal = 8.dp)
                    .fillMaxWidth()
                    .height(6.dp)
                    .background(GlassTokens.RowFillActive, shape)
                    .testTag("workflow-node-stack"),
            )
        }
        Column(
            modifier = Modifier
                .fillMaxWidth()
                // EXP-818: a graph row is a LIST row.
                .flatRow()
                .then(
                    if (node.onCycle) {
                        Modifier.border(1.dp, MaterialTheme.colorScheme.error, shape)
                    } else {
                        Modifier
                    },
                )
                .clickable(onClick = onClick)
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                .testTag("workflow-node-row"),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (issue != null) {
                    StatusIcon(IssueStatus.fromWire(issue.status), size = 14.dp)
                    Spacer(Modifier.width(8.dp))
                    IssueChip(
                        identifier = WorkflowView.nodeTitle(
                            issue.identifier,
                            node.memberIssueIds.size,
                        ),
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
                        modifier = Modifier.weight(1f, fill = false),
                    )
                }
                // A node whose issue has not synced (yet, or at all) draws its
                // CAPTION alone — never a crash, and never a blank row that
                // reads as a broken node.
            }
            Text(
                caption,
                style = MaterialTheme.typography.labelSmall,
                color = tone,
                modifier = Modifier.padding(top = 2.dp).testTag("workflow-node-caption"),
            )
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
                        val from = nodesById[edge.from] ?: return@forEach
                        val blocker = graph.issuesById[from.issueId] ?: return@forEach
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
                                identifier = WorkflowView.nodeTitle(
                                    blocker.identifier,
                                    from.memberIssueIds.size,
                                ),
                                title = blocker.title,
                                status = null,
                            )
                        }
                    }
                }
            }
        }
    }
}
