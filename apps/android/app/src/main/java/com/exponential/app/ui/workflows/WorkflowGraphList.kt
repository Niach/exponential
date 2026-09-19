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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.captionNode
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.markdown.MdStyle
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
 * EXP-983: the colour an edge's style paints in, or null for the plain edge
 * that says nothing. Web and the desktop draw the real lines; a phone has no
 * drawn edges, so the `Blocked by` chips wear this instead.
 */
@Composable
internal fun workflowEdgeStyleColor(style: WorkflowView.EdgeStyle): Color? = when (style) {
    WorkflowView.EdgeStyle.Plain -> null
    WorkflowView.EdgeStyle.Cycle, WorkflowView.EdgeStyle.Stale ->
        MaterialTheme.colorScheme.error
    WorkflowView.EdgeStyle.Landed -> DesignTokens.Semantic.Green
    WorkflowView.EdgeStyle.Speculative ->
        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
}

/**
 * The chip's ring: solid for a landed or a red edge, DASHED for a speculative
 * one (the dependent started before its blocker landed, or the engine
 * serialized two colliding siblings). Drawn outside the chip's own hairline,
 * which is why the ring sits on a padded box rather than on the chip itself.
 */
@Composable
private fun Modifier.workflowEdgeRing(style: WorkflowView.EdgeStyle): Modifier {
    val color = workflowEdgeStyleColor(style) ?: return this.padding(2.dp)
    val dashed = style == WorkflowView.EdgeStyle.Speculative
    val radius = MdStyle.chipCornerRadius + 2.dp
    return this
        .drawBehind {
            drawRoundRect(
                color = color,
                cornerRadius = CornerRadius(radius.toPx()),
                style = Stroke(
                    width = 1.dp.toPx(),
                    pathEffect = if (dashed) {
                        PathEffect.dashPathEffect(floatArrayOf(6f, 6f))
                    } else {
                        null
                    },
                ),
            )
        }
        .padding(2.dp)
}

/**
 * EXP-984: the dashed row border a `proposed` node wears — a follow-up filed
 * during the run that nobody admitted yet. It is drawn like every other node
 * (same caption rule), but the dashes say it is not part of the run: no merge
 * train, and the final pull request does not wait for it.
 */
@Composable
private fun proposedRowBorder(): Modifier {
    val color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val radius = GlassTokens.RowRadius
    return Modifier.drawBehind {
        drawRoundRect(
            color = color,
            cornerRadius = CornerRadius(radius.toPx()),
            style = Stroke(
                width = 1.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)),
            ),
        )
    }
}

/**
 * EXP-982: the glyph a node state wears, or null for the states nobody has to
 * act on (proposed, blocked, ready, skipped, paused) — they read from their
 * caption alone. `running` is not here: it draws the session lists' live dot,
 * not an icon.
 */
private fun workflowStateIcon(state: String): ImageVector? = when (state) {
    DomainContract.wfNodeStateWaiting -> ExpIcons.uiWarning
    DomainContract.wfNodeStateLanded -> ExpIcons.notificationPrMerged
    DomainContract.wfNodeStateFailed -> ExpIcons.uiError
    DomainContract.wfNodeStateInReview, DomainContract.wfNodeStateUpdating -> ExpIcons.navReviews
    else -> null
}

/** Whether [WorkflowStateGlyph] draws anything — the caller's leading slot. */
internal fun workflowStateHasGlyph(state: String): Boolean =
    state == DomainContract.wfNodeStateRunning || workflowStateIcon(state) != null

/**
 * The node's state by SHAPE as well as by colour (EXP-982) — a caption tone
 * alone is invisible to a reader who cannot tell amber from green. Existing
 * icon concepts only.
 */
@Composable
internal fun WorkflowStateGlyph(state: String, modifier: Modifier = Modifier) {
    // The live dot every session list uses, steady: the node is live, and
    // EXP-848 keeps the PULSE for a mid-turn agent alone.
    if (state == DomainContract.wfNodeStateRunning) {
        Box(modifier = modifier.size(14.dp), contentAlignment = Alignment.Center) {
            LiveDot(busy = false)
        }
        return
    }
    val icon = workflowStateIcon(state) ?: return
    Icon(
        icon,
        contentDescription = WorkflowView.nodeStateLabel(state),
        modifier = modifier.size(14.dp).testTag("workflow-node-glyph"),
        tint = workflowToneColor(WorkflowView.nodeTone(state)),
    )
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
    /**
     * EXP-982 ([WorkflowView.finalPrCaption]): non-null once the integration
     * branch is ready to land on the default one. Web and the desktop draw it
     * as one extra node after the last wave; a phone has no grid, so it is the
     * list's LAST section.
     */
    finalPrCaption: String? = null,
    finalPrUrl: String? = null,
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
        finalPrCaption?.let { caption ->
            SectionHeader(WorkflowView.FINAL_PR_TITLE)
            FinalPrRow(caption = caption, url = finalPrUrl)
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

/**
 * The integration branch's one pull request onto the default branch: the whole
 * workflow's result. It opens on GitHub once the engine has actually opened it
 * — before that the row is the caption alone ("Opening the pull request").
 */
@Composable
private fun FinalPrRow(caption: String, url: String?) {
    val uriHandler = LocalUriHandler.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .then(if (url != null) Modifier.clickable { uriHandler.openUri(url) } else Modifier)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
            .testTag("workflow-final-pr"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            ExpIcons.navReviews,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(8.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                WorkflowView.FINAL_PR_TITLE,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (url != null) {
            Icon(
                ExpIcons.uiExternalLink,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
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
                    when {
                        node.onCycle ->
                            Modifier.border(1.dp, MaterialTheme.colorScheme.error, shape)
                        // EXP-984: a proposal, drawn but not yet part of the run.
                        node.state == DomainContract.wfNodeStateProposed -> proposedRowBorder()
                        else -> Modifier
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
            Row(
                modifier = Modifier.padding(top = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // EXP-982: a draft has no states yet, so its caption names the
                // plan and there is nothing for a glyph to say.
                if (workflowStatus != DomainContract.wfStatusDraft &&
                    workflowStateHasGlyph(node.state)
                ) {
                    WorkflowStateGlyph(node.state)
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = tone,
                    modifier = Modifier.testTag("workflow-node-caption"),
                )
            }
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
                        // EXP-983: no lines to draw here, so the chip carries
                        // the edge's style — red on a cycle or a stale
                        // upstream, green once the blocker landed, dashed
                        // while the work below it is speculative.
                        val style = WorkflowView.edgeStyle(edge, from.state, node.state)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (style == WorkflowView.EdgeStyle.Cycle ||
                                style == WorkflowView.EdgeStyle.Stale
                            ) {
                                Icon(
                                    ExpIcons.relationBlockedBy,
                                    contentDescription = null,
                                    modifier = Modifier.size(12.dp),
                                    tint = MaterialTheme.colorScheme.error,
                                )
                                Spacer(Modifier.width(4.dp))
                            }
                            Box(
                                modifier = Modifier
                                    .workflowEdgeRing(style)
                                    .testTag("workflow-blocker-${style.key}"),
                            ) {
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
}
