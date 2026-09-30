package com.exponential.app.ui.work

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueGraph
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.WorkFaceKind
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunIdentifier
import com.exponential.app.domain.resolveSessionDevice
import com.exponential.app.domain.runHasEnded
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IssueGraphList
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.RelationIssueRow
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.reviews.PrStackRow
import com.exponential.app.ui.reviews.PrStatePill
import com.exponential.app.ui.session.RunningSessionRow
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-897 part 4: ONE badge in the Work screen's top bar for everything this
// pull request is entangled with — the stack it sits in, the batch it spans —
// and ONE "Related work" view behind it whose LEADING section follows the
// face the reader is on. Same rows and the same words on all four clients
// (`components/pr-graph-badge.tsx`, `PrGraphBadge.swift`, `pr_graph.rs`).

/**
 * SLOP-16: the badge is a quiet ICON BUTTON beside the `…` — the same ghost
 * [CircleIconButton] the `…` wears — whose glyph names the SHAPE
 * ([PrGraph.badgeShape]: stack, batch, run family, blockers). The stacked
 * issue chip it replaced (EXP-1058/1097) only repeated the title. A small
 * muted `+N` beside it counts everything behind the subject
 * ([PrGraph.badgeChip]). One tap target that opens the overlay ([onOpen]).
 * Nothing at all when [PrGraph.badgeShape] is null.
 */
@Composable
fun PrGraphBadge(
    graph: PrGraph.Graph,
    onOpen: () -> Unit,
) {
    val shape = PrGraph.badgeShape(graph) ?: return
    val spec = PrGraph.badgeChip(graph) ?: return
    Row(
        modifier = Modifier
            .clickable(role = Role.Button, onClick = onOpen)
            .testTag("pr-graph-badge"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CircleIconButton(
            prGraphBadgeIcon(shape),
            prGraphBadgeName(shape),
            onClick = onOpen,
            borderless = true,
        )
        if (spec.count > 0) {
            Text(
                "+${spec.count}",
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                modifier = Modifier.testTag("pr-graph-badge-count"),
            )
        }
    }
}

/** SLOP-16: the badge glyph per shape (web `PrGraphBadge`, iOS twin). */
fun prGraphBadgeIcon(shape: PrGraph.BadgeShape) = when (shape) {
    PrGraph.BadgeShape.STACK, PrGraph.BadgeShape.STACK_AND_BATCH -> ExpIcons.prStack
    PrGraph.BadgeShape.BATCH -> ExpIcons.prBatch
    PrGraph.BadgeShape.RUNS -> ExpIcons.sessionTree
    PrGraph.BadgeShape.BLOCKED -> ExpIcons.relationBlockedBy
}

/** SLOP-16: the badge's spoken name (iOS `PrGraphBadge.accessibilityName`). */
fun prGraphBadgeName(shape: PrGraph.BadgeShape) = when (shape) {
    PrGraph.BadgeShape.STACK -> "Pull request stack"
    PrGraph.BadgeShape.BATCH -> "Batch pull request"
    PrGraph.BadgeShape.STACK_AND_BATCH -> "Stack and batch"
    PrGraph.BadgeShape.BLOCKED -> IssueRelationsView.Copy.BLOCKED_BY
    PrGraph.BadgeShape.RUNS -> "Related runs"
}

/**
 * SLOP-16 r3: THE "Related work" view — one layout ×4 (web
 * `PrGraphOverlay`, iOS `PrGraphBadge.swift`, desktop `pr_graph.rs`). The
 * platform's standard modal ([GlassSheet], content-sized) over the sections
 * in [PrGraph.overlaySections] order, the face's own section FIRST. Each
 * section = the group band ([SectionHeader]) over FLAT rows the product
 * already draws, divided by the group hairline — nothing else:
 *  · issues = the relations card's [RelationIssueRow];
 *  · pull requests = the Reviews list's [PrStackRow], bottom-up, nested;
 *  · runs = the session tree's rows ([RunningSessionRow] live,
 *    [EndedRunRow] ended);
 *  · Blocked by = the COMPACT mini-graph.
 * A batch's issues are listed ONCE: in the batch band when it is drawn, else
 * folded under the batch pull request's row (web's rule).
 */
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
    /** What the relation rows resolve their glyph and avatar against. */
    statuses: List<ResolvedIssueStatus>,
    users: List<UserEntity>,
    /** The run rows' host machines. */
    devices: List<DeviceEntity>,
    onOpenIssue: (String) -> Unit,
    /** A pull request row: that entry's Changes face / review. */
    onOpenPr: (String) -> Unit,
    onOpenRun: (String) -> Unit,
    onMergeStack: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val runFace = face == WorkFaceKind.Run || face == WorkFaceKind.Results
    val sections = PrGraph.overlaySections(graph, face)
    val openIssue: (String) -> Unit = { id -> onDismiss(); onOpenIssue(id) }
    GlassSheet(
        title = PrGraph.OverlayCopy.RELATED_WORK_TITLE,
        onDismiss = onDismiss,
        // SLOP-16 r3: content-sized, never a mostly-empty full sheet.
        height = SheetHeight.Fitted,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                // The sheet's 16dp content gutter (styleguide `sheet`).
                .padding(horizontal = 16.dp)
                .padding(bottom = 8.dp)
                .testTag("pr-graph-sheet"),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // The Issue and Changes faces read the batch from the subject
            // issue, so the band lists its PARTNERS; on a run the covered set
            // IS the run's subject (EXP-930).
            val inBatch = graph.batch?.issues.orEmpty()
                .filter { runFace || it.id != graph.subjectIssueId }
            var drewAny = false
            sections.forEach { section ->
                when (section) {
                    // EXP-980: the transitive chain as THE mini-graph — the
                    // COMPACT one, scrolling sideways past the sheet.
                    PrGraph.OverlaySection.BLOCKED -> {
                        drewAny = true
                        Band(PrGraph.OverlayCopy.BLOCKED) {
                            IssueGraphList(
                                graph = blocksGraph,
                                issuesById = issuesById,
                                onOpenIssue = openIssue,
                                density = IssueGraph.Geometry.Density.COMPACT,
                            )
                        }
                    }

                    PrGraph.OverlaySection.BATCH -> if (inBatch.isNotEmpty()) {
                        drewAny = true
                        Band(PrGraph.OverlayCopy.batchBandTitle(face)) {
                            inBatch.forEachIndexed { index, issue ->
                                if (index > 0) GroupDivider()
                                RelationIssueRow(issue, statuses, users, onClick = { openIssue(issue.id) })
                            }
                        }
                    }

                    PrGraph.OverlaySection.RUNS -> {
                        drewAny = true
                        Band(PrGraph.OverlayCopy.RUNS) {
                            RunRows(graph, devices, nowMs) { id -> onDismiss(); onOpenRun(id) }
                        }
                    }

                    PrGraph.OverlaySection.STACK -> {
                        drewAny = true
                        Band(PrGraph.OverlayCopy.STACK) {
                            StackRows(
                                graph = graph,
                                // A batch's issues are listed ONCE: the band
                                // above holds them when it is drawn.
                                foldIssues = PrGraph.OverlaySection.BATCH !in sections,
                                merging = merging,
                                mergeError = mergeError,
                                statuses = statuses,
                                users = users,
                                onOpenIssue = openIssue,
                                onOpenPr = { id -> onDismiss(); onOpenPr(id) },
                                onMergeStack = onMergeStack,
                            )
                        }
                    }
                }
            }
            if (!drewAny) {
                Text(
                    PrGraph.OverlayCopy.EMPTY,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/** One section: the group band over its flat rows. */
@Composable
private fun Band(title: String, rows: @Composable () -> Unit) {
    Column(modifier = Modifier.fillMaxWidth()) {
        SectionHeader(title)
        rows()
    }
}

/** The run family, nested — the session tree's own rows (EXP-965 guides). */
@Composable
private fun RunRows(
    graph: PrGraph.Graph,
    devices: List<DeviceEntity>,
    nowMs: Long,
    onOpenRun: (String) -> Unit,
) {
    val guides = remember(graph.tree) { TreeGuides.compute(graph.tree.map { it.depth }) }
    graph.tree.forEachIndexed { index, row ->
        val session = row.session
        if (index > 0) GroupDivider()
        TreeGuidesRow(depth = row.depth, guide = guides.getOrNull(index), gap = GlassTokens.Hairline) {
            val device = resolveSessionDevice(session, devices, nowMs)
            if (runHasEnded(session)) {
                val timeLabel = relativeTime(session.endedAt ?: session.updatedAt)
                EndedRunRow(
                    title = row.title,
                    identifier = pastRunIdentifier(session, row.issue, row.batchIssues),
                    timeLabel = timeLabel,
                    byline = pastRunByline(deviceLabel = device.displayLabel, timeLabel = timeLabel),
                    onOpen = { onOpenRun(session.id) },
                )
            } else {
                RunningSessionRow(
                    session = session,
                    issue = row.issue,
                    device = device,
                    batchIssues = row.batchIssues,
                    // EXP-968: the name the graph resolved.
                    titleOverride = row.title,
                    onClick = { onOpenRun(session.id) },
                )
            }
        }
    }
}

/** A line of the pull-request band: a PR row, or a folded batch issue. */
private sealed interface StackLine {
    val depth: Int
    data class Pr(val member: PrGraph.StackEntry, val index: Int) : StackLine {
        override val depth get() = member.depth
    }
    data class Folded(val issue: IssueEntity, override val depth: Int) : StackLine
}

/** The pull requests, BOTTOM-UP — the foundation first, the way it merges. */
@Composable
private fun StackRows(
    graph: PrGraph.Graph,
    foldIssues: Boolean,
    merging: Boolean,
    mergeError: MergeFailure?,
    statuses: List<ResolvedIssueStatus>,
    users: List<UserEntity>,
    onOpenIssue: (String) -> Unit,
    onOpenPr: (String) -> Unit,
    onMergeStack: (String) -> Unit,
) {
    val lines = remember(graph.stack, foldIssues) {
        graph.stack.flatMapIndexed { index, member ->
            listOf<StackLine>(StackLine.Pr(member, index)) +
                if (foldIssues && member.entry.isBatch) {
                    member.entry.issues.map { StackLine.Folded(it, member.depth + 1) }
                } else {
                    emptyList()
                }
        }
    }
    val guides = remember(lines) { TreeGuides.compute(lines.map { it.depth }) }
    // Only the BOTTOM of a real stack takes the whole chain.
    val realStack = graph.stack.size > 1
    lines.forEachIndexed { lineIndex, line ->
        if (lineIndex > 0) GroupDivider()
        val guide = guides.getOrNull(lineIndex)
        when (line) {
            is StackLine.Folded -> TreeGuidesRow(depth = line.depth, guide = guide, gap = GlassTokens.Hairline) {
                RelationIssueRow(line.issue, statuses, users, onClick = { onOpenIssue(line.issue.id) })
            }
            is StackLine.Pr -> {
                val entry = line.member.entry
                val bottom = realStack && line.index == 0
                val below = graph.stack.getOrNull(line.index - 1)?.entry?.representative
                PrStackRow(
                    isBatch = entry.isBatch,
                    label = prLabel(entry.representative),
                    title = if (entry.isBatch) "${entry.issues.size} issues" else entry.representative.title,
                    depth = line.member.depth,
                    guide = guide,
                    gap = GlassTokens.Hairline,
                    onClick = { onOpenPr(entry.representative.id) },
                    details = {
                        // EXP-897: an upper member names its foundation, as on Reviews.
                        if (line.member.depth > 0 && below != null) {
                            Spacer(Modifier.height(2.dp))
                            Text(
                                "on top of ${prLabel(below)}",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    },
                    trailing = {
                        if (bottom) {
                            GlassPill(
                                PrGraph.OverlayCopy.MERGE_STACK,
                                onClick = { onMergeStack(entry.representative.id) },
                                icon = ExpIcons.prStack,
                                enabled = !merging,
                                loading = merging,
                                modifier = Modifier.testTag("graph-merge-stack"),
                            )
                        } else {
                            PrStatePill(entry.representative.prState)
                        }
                    },
                    footer = {
                        // A refused merge captions the row that offered it.
                        if (bottom && mergeError != null) {
                            Text(
                                mergeError.message,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                            )
                        }
                    },
                )
            }
        }
    }
}

/** A pull request's mono label: `#n`, else its representative's identifier. */
private fun prLabel(issue: IssueEntity): String = issue.prNumber?.let { "#$it" } ?: issue.identifier
