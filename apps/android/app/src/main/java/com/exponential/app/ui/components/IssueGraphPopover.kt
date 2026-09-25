package com.exponential.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1057: THE blocks mini-graph, drawn as the SAME grid the web and the
 * desktop draw (`@exp/ui` `WaveGraph`, desktop `issue_graph`): columns =
 * waves, rows = lanes, one issue-chip box per node, cubic edges behind them —
 * grey, red on a blocking cycle. Subjects wear the primary ring, cycle members
 * the error one. Every number is [IssueGraph.Geometry] (locked by
 * `domain-contract/fixtures/issue-graph-geometry.json`); the grid carries its
 * own inset so no ring is ever clipped, and it scrolls both ways past
 * min(grid, available width) × min(grid, maxViewHeight). The two shared notes
 * sit underneath.
 */
@Composable
fun IssueGraphPopover(
    graph: IssueGraph.Graph,
    issuesById: Map<String, IssueEntity>,
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (graph.isEmpty) return
    val size = remember(graph) { IssueGraph.Geometry.size(graph) }
    val nodesById = remember(graph) { graph.nodes.associateBy { it.id } }
    // A node sits on a cycle when any red edge touches it.
    val onCycle = remember(graph) {
        graph.edges.filter { it.cycle }.flatMap { listOf(it.from, it.to) }.toSet()
    }
    val edgeColor = GlassTokens.StrokeStrong
    val cycleColor = MaterialTheme.colorScheme.error
    val primary = MaterialTheme.colorScheme.primary
    val nodeShape = remember { RoundedCornerShape(IssueGraph.Geometry.NODE_RADIUS.dp) }

    Column(
        modifier = modifier.testTag("issue-graph"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        BoxWithConstraints {
            val gridWidth = size.width.dp
            val gridHeight = size.height.dp
            val viewWidth = if (constraints.hasBoundedWidth) minOf(gridWidth, maxWidth) else gridWidth
            val viewHeight = minOf(gridHeight, size.viewHeight.dp)
            Box(
                modifier = Modifier
                    .size(viewWidth, viewHeight)
                    .horizontalScroll(rememberScrollState())
                    .verticalScroll(rememberScrollState()),
            ) {
                Box(modifier = Modifier.size(gridWidth, gridHeight)) {
                    Canvas(modifier = Modifier.fillMaxSize()) {
                        val px = density
                        val stroke = Stroke(width = IssueGraph.Geometry.EDGE_STROKE * px)
                        for (edge in graph.edges) {
                            val from = nodesById[edge.from] ?: continue
                            val to = nodesById[edge.to] ?: continue
                            if (issuesById[edge.from] == null || issuesById[edge.to] == null) continue
                            val curve = IssueGraph.Geometry.edge(from, to)
                            val path = Path().apply {
                                moveTo(curve.start.x * px, curve.start.y * px)
                                cubicTo(
                                    curve.control1.x * px, curve.control1.y * px,
                                    curve.control2.x * px, curve.control2.y * px,
                                    curve.end.x * px, curve.end.y * px,
                                )
                            }
                            drawPath(
                                path = path,
                                color = if (edge.cycle) cycleColor else edgeColor,
                                style = stroke,
                            )
                        }
                    }
                    for (node in graph.nodes) {
                        // A node whose issue has not synced draws nothing.
                        val issue = issuesById[node.id] ?: continue
                        val origin = IssueGraph.Geometry.origin(node.wave, node.lane)
                        val ring: Color? = when {
                            node.id in onCycle -> cycleColor
                            node.subject -> primary
                            else -> null
                        }
                        Box(
                            modifier = Modifier
                                .offset(origin.x.dp, origin.y.dp)
                                .size(
                                    IssueGraph.Geometry.NODE_WIDTH.dp,
                                    IssueGraph.Geometry.NODE_HEIGHT.dp,
                                )
                                .testTag("issue-graph-node-${issue.identifier}")
                                .then(
                                    if (ring != null) {
                                        Modifier.border(
                                            IssueGraph.Geometry.RING_WIDTH.dp,
                                            ring,
                                            nodeShape,
                                        )
                                    } else {
                                        Modifier
                                    },
                                )
                                .clip(nodeShape)
                                .clickable { onOpenIssue(issue.id) },
                        ) {
                            IssueChip(
                                identifier = issue.identifier,
                                title = issue.title,
                                status = null,
                                modifier = Modifier.fillMaxSize(),
                                leading = {
                                    StatusIcon(
                                        IssueStatus.fromWire(issue.status),
                                        size = MdStyle.chipIconSize,
                                    )
                                },
                            )
                        }
                    }
                }
            }
        }
        if (graph.hasCycle) {
            GraphPopoverNote(IssueGraph.CYCLE_NOTE, MaterialTheme.colorScheme.error)
        }
        if (graph.truncated) {
            GraphPopoverNote(
                IssueGraph.TRUNCATED_NOTE,
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

@Composable
private fun GraphPopoverNote(text: String, color: Color) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = color,
        modifier = Modifier.padding(horizontal = 4.dp),
    )
}
