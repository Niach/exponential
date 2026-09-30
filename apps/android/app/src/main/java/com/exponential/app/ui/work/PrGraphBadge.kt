package com.exponential.app.ui.work

import androidx.compose.foundation.ExperimentalFoundationApi
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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.IssueChipSize
import com.exponential.app.ui.components.IssueChipStack
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.markdown.MdStyle
import androidx.compose.ui.semantics.Role
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueGraphList
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.components.treeGuides
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.session.sessionDotTone
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-897 part 4: ONE badge in the Work screen's top bar for everything this
// pull request is entangled with — the stack it sits in, the batch it spans —
// and ONE overlay behind it whose SECTION follows the face the reader is on:
// the Issue face lists relations, the Run face the run tree, the Changes face
// the pull requests. Same rows and the same words on all four clients
// (`components/pr-graph-badge.tsx`, `PrGraphBadge.swift`, `pr_graph.rs`).

/**
 * EXP-1058: the badge is the STACKED issue chip — the front issue
 * ([PrGraph.badgeChip]) with ghost outlines saying there is more than one,
 * `+N` BESIDE the stack for everything behind it. EXP-1097: the SAME chip on
 * every face ([PrGraph.badgeShape] is face-independent: a stack/batch, a run
 * family, else open blockers) and COMPACT — SLOP-15/16: the chip's own small
 * mode ([IssueChipSize.Sm]: glyph + identifier, the title on long press). A
 * run with no issue (the run tree alone) fronts the same chip box with the
 * session-tree glyph and the run's own name ([runTitle]). The chip is inert:
 * the WHOLE thing is one tap target that opens the overlay ([onOpen]).
 * Nothing at all when [PrGraph.badgeChip] is null.
 */
@Composable
fun PrGraphBadge(
    graph: PrGraph.Graph,
    /** The front chip's issue resolved against its team ([PrGraphViewModel.chipStatus]). */
    chipStatus: ResolvedIssueStatus?,
    runTitle: String?,
    onOpen: () -> Unit,
) {
    val spec = PrGraph.badgeChip(graph) ?: return
    val front = spec.issue
    Row(
        modifier = Modifier
            .clickable(role = Role.Button, onClick = onOpen)
            .testTag("pr-graph-badge"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IssueChipStack {
            if (front != null) {
                IssueChip(
                    identifier = front.identifier,
                    title = front.title,
                    status = chipStatus,
                    size = IssueChipSize.Sm,
                )
            } else {
                IssueChip(
                    identifier = "",
                    title = runTitle ?: "Run",
                    status = null,
                    leading = {
                        Icon(
                            ExpIcons.sessionTree,
                            contentDescription = null,
                            modifier = Modifier.size(MdStyle.chipIconSize),
                            tint = MdStyle.ChipToken,
                        )
                    },
                )
            }
        }
        if (spec.count > 0) {
            // Beside the stack, clear of the ghost that peeks out on the right
            // (web `IssueChipStack`'s count slot).
            Text(
                "+${spec.count}",
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                modifier = Modifier.padding(start = 6.dp).testTag("pr-graph-badge-count"),
            )
        }
    }
}

/**
 * The overlay: a bottom sheet listing every relation the subject HAS, the
 * section this [face] is about FIRST ([PrGraph.overlaySections]) — the reader
 * never has to hunt for the part that matches what is on screen, and the
 * rows are the same primitives everywhere.
 */
@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
fun PrGraphSheet(
    graph: PrGraph.Graph,
    face: WorkFaceKind,
    nowMs: Long,
    merging: Boolean,
    mergeError: MergeFailure?,
    // EXP-980: the subject's blocks graph and the pool its nodes resolve
    // against — "Blocked by" draws the CHAIN instead of a flat chip row.
    blocksGraph: IssueGraph.Graph,
    issuesById: Map<String, IssueEntity>,
    onOpenIssue: (String) -> Unit,
    onOpenRun: (String) -> Unit,
    onMergeStack: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val runFace = face == WorkFaceKind.Run || face == WorkFaceKind.Results
    GlassSheet(
        title = when (PrGraph.badgeShape(graph)) {
            PrGraph.BadgeShape.BATCH -> "Batch pull request"
            PrGraph.BadgeShape.STACK, PrGraph.BadgeShape.STACK_AND_BATCH -> "Stacked pull requests"
            PrGraph.BadgeShape.BLOCKED -> IssueRelationsView.Copy.BLOCKED_BY
            // EXP-1058: the run tree alone earns the chip.
            PrGraph.BadgeShape.RUNS, null -> "Runs"
        },
        onDismiss = onDismiss,
    ) {
        val sections = PrGraph.overlaySections(graph, face)
        var drewAny = false
        sections.forEach { section ->
            when (section) {
                // EXP-980: the flat "Blocked by" chip row became the MINI-GRAPH
                // — the same one the list badges and the blocked-start dialog
                // draw, so a chain of blockers reads as a chain.
                PrGraph.OverlaySection.BLOCKED -> {
                    drewAny = true
                    SectionHeader(IssueRelationsView.Copy.BLOCKED_BY)
                    IssueGraphList(
                        graph = blocksGraph,
                        issuesById = issuesById,
                        onOpenIssue = { onDismiss(); onOpenIssue(it) },
                        // SLOP-16: the web's `density="compact"` blocked-by graph.
                        density = IssueGraph.Geometry.Density.COMPACT,
                    )
                }

                // EXP-930: on a run the batch is the run's own subject, so the
                // WHOLE covered set is listed ("Issues"); elsewhere it reads
                // from the subject's side ("In batch with" = everything but
                // me). A batch has no order to show, so it stays a chip row.
                PrGraph.OverlaySection.BATCH -> {
                    val covered = graph.batch?.issues.orEmpty()
                    val rows = if (runFace) covered else covered.filter { it.id != graph.subjectIssueId }
                    if (rows.isNotEmpty()) {
                        drewAny = true
                        SectionHeader(if (runFace) "Issues" else "In batch with")
                        IssueChipRow(rows) { onDismiss(); onOpenIssue(it) }
                    }
                }

                PrGraph.OverlaySection.RUNS -> {
                    drewAny = true
                    RunsSection(graph, nowMs, onDismiss, onOpenRun)
                }

                PrGraph.OverlaySection.STACK -> {
                    drewAny = true
                    SectionHeader("Pull requests")
                    // EXP-965: the chain's own connector, the same one the run
                    // tree draws.
                    val stackGuides = remember(graph.stack) {
                        TreeGuides.compute(graph.stack.map { it.depth })
                    }
                    // BOTTOM-UP: the foundation first, the way it merges.
                    graph.stack.forEachIndexed { index, member ->
                        StackMemberRow(
                            member = member,
                            guide = stackGuides.getOrNull(index),
                            isSubject = member.entry.issues.any { it.id == graph.subjectIssueId },
                            // Only the BOTTOM of a real stack takes the whole chain.
                            mergeStackIssueId = member.entry.representative.id
                                .takeIf { member.depth == 0 && graph.stack.size > 1 },
                            merging = merging,
                            onMergeStack = onMergeStack,
                            onOpenIssue = onOpenIssue,
                            onDismiss = onDismiss,
                        )
                    }
                    mergeError?.let { failure ->
                        Text(
                            failure.message,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                        )
                    }
                }
            }
        }
        if (!drewAny) {
            EmptyNote(
                if (runFace) "No runs on this work yet." else "Nothing else is linked to this issue.",
            )
        }
        Spacer(Modifier.size(8.dp))
    }
}

/** The run family, nested — its rows NAMED by the graph (EXP-968). */
@Composable
private fun RunsSection(
    graph: PrGraph.Graph,
    nowMs: Long,
    onDismiss: () -> Unit,
    onOpenRun: (String) -> Unit,
) {
    SectionHeader("Runs")
    val runGuides = remember(graph.tree) {
        TreeGuides.compute(graph.tree.map { it.depth })
    }
    graph.tree.forEachIndexed { index, row ->
        val session = row.session
        // The sheet's column stacks its rows flush, so the connector needs no
        // gap to bridge (EXP-965).
        TreeGuidesRow(depth = row.depth, guide = runGuides.getOrNull(index)) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    // EXP-818: an overlay row is a LIST row.
                    .flatRow()
                    .clickable {
                        onDismiss()
                        onOpenRun(session.id)
                    }
                    .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val tone = sessionDotTone(session, null, nowMs, awaitingInput = false)
                    ?: SessionDotTone.Muted
                SessionToneDot(tone, busy = session.agentBusy)
                Spacer(Modifier.width(10.dp))
                Text(
                    row.title,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun IssueChipRow(issues: List<IssueEntity>, onOpen: (String) -> Unit) {
    FlowRow(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        issues.forEach { issue -> SmallIssueChip(issue) { onOpen(issue.id) } }
    }
}

/**
 * SLOP-15/16: every overlay chip is the SMALL chip (web `size="sm"`) wearing
 * its own status glyph — the recipe `IssueGraphPopover` draws its nodes with.
 */
@Composable
private fun SmallIssueChip(issue: IssueEntity, onClick: () -> Unit) {
    IssueChip(
        identifier = issue.identifier,
        title = issue.title,
        status = null,
        size = IssueChipSize.Sm,
        onClick = onClick,
        leading = {
            StatusIcon(IssueStatus.fromWire(issue.status), size = MdStyle.chipIconSize)
        },
    )
}

/** One pull request in the stack: its identifiers, its state, its batch. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun StackMemberRow(
    member: PrGraph.StackEntry,
    guide: TreeGuide?,
    isSubject: Boolean,
    mergeStackIssueId: String?,
    merging: Boolean,
    onMergeStack: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val entry = member.entry
    Column(
        modifier = Modifier
            .fillMaxWidth()
            // EXP-965: the connector draws in the gutter the indent leaves.
            .treeGuides(guide)
            .padding(start = (TreeGuides.INDENT_DP * member.depth).dp)
            // EXP-818: an overlay row is a LIST row.
            .flatRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (entry.isBatch) {
                Icon(
                    ExpIcons.prBatch,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Spacer(Modifier.width(6.dp))
            }
            Text(
                entry.representative.identifier,
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (isSubject) TextEmphasis.Primary else TextEmphasis.Tertiary,
                ),
                maxLines = 1,
            )
            Spacer(Modifier.width(8.dp))
            PrStatePill(entry.representative.prState)
            Spacer(Modifier.width(8.dp))
            if (mergeStackIssueId != null) {
                GlassPill(
                    PrStack.MERGE_STACK_LABEL,
                    onClick = { onMergeStack(mergeStackIssueId) },
                    size = PillSize.Sm,
                    icon = ExpIcons.prStack,
                    enabled = !merging,
                    loading = merging,
                    modifier = Modifier.testTag("graph-merge-stack"),
                )
            }
        }
        // A batch member folds its issues right underneath it — SLOP-16: a
        // wrapped row of small chips, indented one level (web parity).
        if (entry.isBatch) {
            FlowRow(
                modifier = Modifier.padding(start = TreeGuides.INDENT_DP.dp, top = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                entry.issues.forEach { issue ->
                    SmallIssueChip(issue) { onDismiss(); onOpenIssue(issue.id) }
                }
            }
        }
    }
}

/** The PR's state as one coloured word — open green, merged blue, else muted. */
@Composable
private fun PrStatePill(state: String?) {
    val label = when (state) {
        DomainContract.prStateOpen -> "Open"
        DomainContract.prStateMerged -> "Merged"
        DomainContract.prStateClosed -> "Closed"
        DomainContract.prStateDraft -> "Draft"
        else -> return
    }
    val tint: Color = when (state) {
        DomainContract.prStateOpen -> DesignTokens.Semantic.Green
        DomainContract.prStateMerged -> DoneBlue
        else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    }
    Text(
        label,
        style = MaterialTheme.typography.labelSmall,
        color = tint,
        maxLines = 1,
    )
}

@Composable
private fun EmptyNote(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
    )
}
