package com.exponential.app.ui.workflows

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.runtime.remember
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.WorkflowView
import com.exponential.app.domain.captionNode
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueChipStack
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.session.LostGray
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.work.SessionToneDot
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-981: a workflow's graph as a PHONE draws it — the same wave-grouped list
// the blocks mini-graph uses (`ui/components/IssueGraphList.kt`, EXP-980), fed
// workflow nodes instead of issues. Web and the desktop draw the real grid with
// edges; here a section per wave over chip rows, each row carrying the chips of
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
 * EXP-982: the caption colour a LIVE run lends its node — the ×4 session-dot
 * palette ([SessionToneDot]'s own), so a running node and a running run read
 * alike wherever they sit beside each other.
 */
@Composable
internal fun sessionToneColor(tone: SessionDotTone): Color = when (tone) {
    SessionDotTone.Running -> LiveGreen
    SessionDotTone.Review -> ReviewGreen
    SessionDotTone.NeedsInput -> NeedsInputAmber
    SessionDotTone.Done -> DoneBlue
    SessionDotTone.Muted -> LostGray
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
 * EXP-1014: the ring a node's CHIP wears, or nothing at all for the ordinary
 * node. A node on a blocking cycle takes the destructive ring; the one whose
 * sheet is open takes the accent one; a `proposed` node (EXP-984: a follow-up
 * filed during the run that nobody admitted yet) is dashed — it is drawn, but
 * it is not part of the run.
 */
@Composable
private fun Modifier.workflowNodeRing(node: WorkflowNodeEntity, selected: Boolean): Modifier {
    val proposed = node.state == DomainContract.wfNodeStateProposed
    val color = when {
        node.onCycle -> MaterialTheme.colorScheme.error
        selected -> MaterialTheme.colorScheme.primary
        proposed -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        else -> return this.padding(2.dp)
    }
    val dashed = proposed && !node.onCycle && !selected
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
 * row the chips of its direct blockers INSIDE the workflow. EXP-1014: a node
 * row IS the app's issue chip — its state glyph (or its live run's dot) in the
 * glyph slot, the identifier, the title, and the state caption trailing it in
 * the state's tone. A compound node is a STACKED chip ([IssueChipStack]); a
 * node on a cycle — and every cycle edge — is red. A node whose issue row has
 * not synced renders its caption alone and never crashes.
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
    /** EXP-982: steer the node's run — the Running strip's tap. */
    onOpenRun: (String) -> Unit = {},
    /**
     * EXP-982 ([WorkflowView.finalPrCaption]): non-null once the integration
     * branch is ready to land on the default one. Web and the desktop draw it
     * as one extra node after the last wave; a phone has no grid, so it is the
     * list's LAST section.
     */
    finalPrCaption: String? = null,
    finalPrUrl: String? = null,
    /**
     * EXP-1033: the final PR is OPEN — merging it is the run's one human
     * review, so the control sits on the chip that IS the pull request. The
     * confirmation and the mutation are the screen's ([onMergeFinalPr]).
     */
    finalPrMergeable: Boolean = false,
    onMergeFinalPr: () -> Unit = {},
    /** A mutation is in flight: the Merge control dims rather than re-firing. */
    busy: Boolean = false,
    /** EXP-1014: the node whose sheet is open wears the accent ring. */
    selectedNodeId: String? = null,
) {
    if (graph.nodes.isEmpty()) return
    val incoming = remember(graph.edges) { graph.edges.groupBy { it.to } }
    val nodesById = remember(graph.nodes) { graph.nodes.associateBy { it.id } }
    // Already ordered by (wave, lane) — grouping keeps it.
    val waves = remember(graph.nodes) { graph.nodes.groupBy { it.wave ?: 0 } }
    Column(modifier = modifier.fillMaxWidth().testTag("workflow-graph")) {
        RunningStrip(graph = graph, onOpenRun = onOpenRun)
        waves.forEach { (wave, nodes) ->
            SectionHeader("Wave ${wave + 1}")
            nodes.forEach { node ->
                WorkflowNodeRow(
                    node = node,
                    graph = graph,
                    workflowStatus = workflowStatus,
                    blockers = incoming[node.id].orEmpty(),
                    nodesById = nodesById,
                    // EXP-982: a node that is UP reads off the session itself,
                    // not off the node state alone.
                    run = graph.runsByNodeId[node.id]?.takeIf { it.live },
                    selected = node.id == selectedNodeId,
                    onClick = { onSelectNode(node) },
                )
            }
        }
        finalPrCaption?.let { caption ->
            FinalPrRow(
                caption = caption,
                url = finalPrUrl,
                mergeable = finalPrMergeable,
                mergeEnabled = !busy,
                onMerge = onMergeFinalPr,
            )
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
 * EXP-982 — the runs that are up right now, one tap away: a node's row says
 * THAT it runs, this strip is the way in. A wide run set scrolls sideways
 * rather than wrapping the header off a phone.
 */
@Composable
private fun RunningStrip(graph: WorkflowGraph, onOpenRun: (String) -> Unit) {
    val live = graph.liveRuns
    if (live.isEmpty()) return
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = 8.dp)
            .testTag("workflow-running-strip"),
    ) {
        Text(
            WorkflowView.RUNNING_NOW_LABEL,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(top = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            live.forEach { (node, run) ->
                val issue = graph.issuesById[node.issueId]
                GlassPill(
                    // The same stand-in the node chip uses while the issue
                    // row has not synced: the first 8 characters of its id.
                    label = WorkflowView.nodeTitle(
                        issue?.identifier ?: node.issueId.take(8),
                        node.memberIssueIds.size,
                    ),
                    onClick = { onOpenRun(run.sessionId) },
                    leading = { SessionToneDot(run.tone, busy = run.busy) },
                    modifier = Modifier.testTag("workflow-running-${node.id}"),
                )
            }
        }
    }
}

/**
 * The integration branch's one pull request onto the default branch: the whole
 * workflow's result. EXP-1014: one more CHIP after the last wave, wearing the
 * merged-PR glyph, with its caption trailing — it opens on GitHub once the
 * engine has actually opened it, and before that the caption is all there is
 * to say ("Opening the pull request").
 *
 * EXP-1033: while that PR is OPEN the row carries the run's ONE human review —
 * a small Merge control; the screen confirms it and calls the mutation.
 */
@Composable
private fun FinalPrRow(
    caption: String,
    url: String?,
    mergeable: Boolean,
    mergeEnabled: Boolean,
    onMerge: () -> Unit,
) {
    val uriHandler = LocalUriHandler.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (url != null) Modifier.clickable { uriHandler.openUri(url) } else Modifier)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
            .testTag("workflow-final-pr"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IssueChip(
            identifier = "",
            title = WorkflowView.FINAL_PR_TITLE,
            status = null,
            leading = {
                Icon(
                    ExpIcons.notificationPrMerged,
                    contentDescription = null,
                    modifier = Modifier.size(MdStyle.chipIconSize),
                    tint = MdStyle.ChipToken,
                )
            },
            modifier = Modifier.weight(1f, fill = false),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            caption,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
        )
        if (mergeable) {
            Spacer(Modifier.width(8.dp))
            GlassPill(
                WorkflowView.MERGE_FINAL_PR_LABEL,
                size = PillSize.Sm,
                enabled = mergeEnabled,
                onClick = onMerge,
                modifier = Modifier.testTag("workflow-final-pr-merge"),
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
    run: WorkflowNodeRun?,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val issue = graph.issuesById[node.issueId]
    val caption = remember(node, workflowStatus) {
        WorkflowView.nodeCaption(node.captionNode, workflowStatus)
    }
    // A live run paints the caption in its SESSION's tone, not the node's.
    val tone = run?.let { sessionToneColor(it.tone) }
        ?: workflowToneColor(WorkflowView.nodeTone(node.state))
    val compound = node.memberIssueIds.isNotEmpty()
    // EXP-1014/EXP-1035: the chip's glyph slot follows ONE rule ×4 — a live
    // run's dot while the node is up; else, in a STARTED workflow, the state's
    // own glyph when it has one; else the ISSUE's own status glyph, so an
    // unstarted node (a draft, or `blocked`/`ready`/`proposed`/`skipped`) reads
    // exactly like the same issue anywhere else. An unsynced issue: no glyph.
    val glyph: (@Composable () -> Unit)? = when {
        run != null -> {
            {
                Box(
                    modifier = Modifier.size(14.dp).testTag("workflow-node-run-dot"),
                    contentAlignment = Alignment.Center,
                ) {
                    SessionToneDot(run.tone, busy = run.busy)
                }
            }
        }
        workflowStatus != DomainContract.wfStatusDraft && workflowStateHasGlyph(node.state) -> {
            { WorkflowStateGlyph(node.state) }
        }
        else -> null
    }
    // Only when no glyph applies; [IssueChip] never draws both.
    val chipStatus = if (glyph == null) graph.statusByIssueId[node.issueId] else null
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV)
                .testTag("workflow-node-row"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val chip: @Composable () -> Unit = {
                IssueChip(
                    // EXP-1035: a node whose issue has not synced (yet, or at
                    // all) still draws — the first 8 characters of the issue id
                    // stand in for the identifier (a compound one still reads
                    // `abcd1234 +2`), never a crash and never a blank row.
                    identifier = WorkflowView.nodeTitle(
                        issue?.identifier ?: node.issueId.take(8),
                        node.memberIssueIds.size,
                    ),
                    title = issue?.title ?: WorkflowView.NODE_UNSYNCED_TITLE,
                    status = chipStatus,
                    leading = glyph,
                    onClick = onClick,
                )
            }
            Box(
                modifier = Modifier
                    .weight(1f, fill = false)
                    .workflowNodeRing(node, selected),
            ) {
                // One node, several issues, run as ONE batch on one branch:
                // the chip is STACKED rather than carrying a second row.
                if (compound) {
                    IssueChipStack(
                        modifier = Modifier.testTag("workflow-node-stack"),
                        chip = chip,
                    )
                } else {
                    chip()
                }
            }
            if (caption.isNotEmpty()) {
                Spacer(Modifier.width(8.dp))
                Text(
                    caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = tone,
                    maxLines = 1,
                    modifier = Modifier.testTag("workflow-node-caption"),
                )
            }
        }
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = GlassTokens.RowPaddingH)) {
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
