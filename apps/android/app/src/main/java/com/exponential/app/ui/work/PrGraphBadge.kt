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
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.IssueChip
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

/** The pill's own words — `2 of 3`, `3 issues`, or both. */
internal fun badgeLabel(graph: PrGraph.Graph): String? {
    val batchCount = graph.batch?.issues?.size
    val position = graph.subject
    val stacked = graph.stack.size > 1
    val head = if (stacked && position != null) "${position.depth + 1} of ${graph.stack.size}" else null
    val tail = batchCount?.let { "$it issues" }
    return when {
        head != null && tail != null -> "$head · $tail"
        head != null -> head
        else -> tail
    }
}

/**
 * The badge itself: nothing at all for a lone single-issue pull request, a
 * tappable pill otherwise. [onOpen] opens the overlay the host renders.
 */
@Composable
fun PrGraphBadge(graph: PrGraph.Graph, onOpen: () -> Unit) {
    val kind = PrGraph.badgeKind(graph) ?: return
    val label = badgeLabel(graph) ?: return
    GlassPill(
        label,
        onClick = onOpen,
        size = PillSize.Sm,
        icon = when (kind) {
            PrGraph.BadgeKind.BATCH -> ExpIcons.prBatch
            else -> ExpIcons.prStack
        },
        trailing = if (kind == PrGraph.BadgeKind.STACK_AND_BATCH) {
            {
                Icon(
                    ExpIcons.prBatch,
                    contentDescription = null,
                    modifier = Modifier.size(12.dp),
                )
            }
        } else {
            null
        },
        modifier = Modifier.testTag("pr-graph-badge"),
    )
}

/**
 * The overlay: a bottom sheet whose content is the section this [face] is
 * about — the reader never has to hunt for the part that matches what is on
 * screen, and the rows are the same primitives everywhere.
 */
@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
fun PrGraphSheet(
    graph: PrGraph.Graph,
    face: WorkFaceKind,
    nowMs: Long,
    merging: Boolean,
    mergeError: MergeFailure?,
    onOpenIssue: (String) -> Unit,
    onOpenRun: (String) -> Unit,
    onMergeStack: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(
        title = when (PrGraph.badgeKind(graph)) {
            PrGraph.BadgeKind.BATCH -> "Batch pull request"
            else -> "Stacked pull requests"
        },
        onDismiss = onDismiss,
    ) {
        when (face) {
            WorkFaceKind.Issue -> {
                if (graph.blockedBy.isNotEmpty()) {
                    SectionHeader("Blocked by")
                    IssueChipRow(graph.blockedBy) { onDismiss(); onOpenIssue(it) }
                }
                val siblings = graph.batch?.issues.orEmpty()
                    .filter { it.id != graph.subjectIssueId }
                if (siblings.isNotEmpty()) {
                    SectionHeader("In batch with")
                    IssueChipRow(siblings) { onDismiss(); onOpenIssue(it) }
                }
                if (graph.blockedBy.isEmpty() && siblings.isEmpty()) {
                    EmptyNote("Nothing else is linked to this issue.")
                }
            }

            // EXP-879: Results is a SUB-FACE of Run, so its overlay is the
            // run family too — there is no results-shaped graph.
            WorkFaceKind.Run, WorkFaceKind.Results -> {
                // EXP-930: the pill on a BATCH run says `3 issues`, so the
                // first thing behind it is those three issues — the run tree
                // alone answered a question nobody asked. The Issue face lists
                // the subject's siblings; this is the run's own subject, so
                // the WHOLE covered set is here, not "everything but me".
                val covered = graph.batch?.issues.orEmpty()
                if (covered.isNotEmpty()) {
                    SectionHeader("Issues")
                    IssueChipRow(covered) { onDismiss(); onOpenIssue(it) }
                }
                SectionHeader("Runs")
                if (graph.tree.isEmpty()) {
                    EmptyNote("No runs on this work yet.")
                }
                // EXP-968: the row's NAME rides the graph — one snapshot for
                // the tree and the issues it was joined against, so a label
                // can never disagree with the row it sits on.
                val runGuides = remember(graph.tree) {
                    TreeGuides.compute(graph.tree.map { it.depth })
                }
                graph.tree.forEachIndexed { index, row ->
                    val session = row.session
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

            WorkFaceKind.Changes -> {
                SectionHeader("Pull requests")
                // EXP-965: the chain's own connector, the same one the run
                // tree above draws.
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
        Spacer(Modifier.size(8.dp))
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
        issues.forEach { issue ->
            IssueChip(
                identifier = issue.identifier,
                title = issue.title,
                status = null,
                onClick = { onOpen(issue.id) },
            )
        }
    }
}

/** One pull request in the stack: its identifiers, its state, its batch. */
@Composable
private fun StackMemberRow(
    member: PrGraph.StackEntry,
    guide: TreeGuide?,
    isSubject: Boolean,
    mergeStackIssueId: String?,
    merging: Boolean,
    onMergeStack: (String) -> Unit,
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
        // A batch member folds its issues right underneath it.
        if (entry.isBatch) {
            Text(
                entry.identifiers.joinToString(", "),
                style = MaterialTheme.typography.labelSmall,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 2.dp),
            )
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
