package com.exponential.app.ui.work

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.PrGraph
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.components.IssueRowContent
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.RelationBandFrame
import com.exponential.app.ui.issue.RelationIssueRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-897 part 4: ONE badge in the Work screen's top bar for everything this
// pull request is entangled with (the stack it sits in, the batch it spans,
// its open blockers) and ONE "Related work" sheet behind it (SLOP-16 r5: the
// relations card's bands, nothing else; SLOP-3: read-only, no merge). Same rows and the same words on all four clients
// (`components/pr-graph-badge.tsx`, `PrGraphBadge.swift`, `pr_graph.rs`).

/**
 * SLOP-16: the badge is a quiet ICON BUTTON beside the `…` (the same ghost
 * [CircleIconButton] the `…` wears) whose glyph names the SHAPE
 * ([PrGraph.badgeShape]: stack, batch, blockers). The stacked
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
    PrGraph.BadgeShape.BLOCKED -> ExpIcons.relationBlockedBy
}

/** SLOP-16: the badge's spoken name (iOS `PrGraphBadge.accessibilityName`). */
fun prGraphBadgeName(shape: PrGraph.BadgeShape) = when (shape) {
    PrGraph.BadgeShape.STACK -> "Pull request stack"
    PrGraph.BadgeShape.BATCH -> "Batch pull request"
    PrGraph.BadgeShape.STACK_AND_BATCH -> "Stack and batch"
    PrGraph.BadgeShape.BLOCKED -> IssueRelationsView.Copy.BLOCKED_BY
}

/**
 * SLOP-16 r5: THE "Related work" sheet: one layout ×4 (web
 * `PrGraphOverlay`, iOS `PrGraphBadge.swift`, desktop `pr_graph.rs`). The
 * platform's standard modal ([GlassSheet], content-sized) over EXACTLY the
 * relations card's bands ([RelationBandFrame]: foldable, counted, capped at
 * [IssueRelationsView.BAND_CAP] with "Show N more"), in
 * [PrGraph.overlaySections] order and nothing else:
 *  · Blocked by: the direct open blockers as [RelationIssueRow]s;
 *  · Same pull request: the batch partners as [RelationIssueRow]s;
 *  · Pull request stack: the OTHER pull requests, bottom-up ([PrRelationRow]).
 */
@Composable
fun PrGraphSheet(
    graph: PrGraph.Graph,
    /** What the relation rows resolve their glyph and avatar against. */
    statuses: List<ResolvedIssueStatus>,
    users: List<UserEntity>,
    onOpenIssue: (String) -> Unit,
    /** A pull request row: that entry's Changes face / review. */
    onOpenPr: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val sections = PrGraph.overlaySections(graph)
    val openIssue: (String) -> Unit = { id -> onDismiss(); onOpenIssue(id) }
    val openPr: (String) -> Unit = { id -> onDismiss(); onOpenPr(id) }
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
            // The relations card's band spacing.
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            sections.forEach { section ->
                when (section) {
                    PrGraph.OverlaySection.BLOCKED -> CappedBand(
                        title = PrGraph.OverlayCopy.BLOCKED,
                        icon = ExpIcons.relationBlockedBy,
                        tag = "blocked",
                        items = graph.blockedBy,
                    ) { issue ->
                        RelationIssueRow(issue, statuses, users, onClick = { openIssue(issue.id) })
                    }

                    PrGraph.OverlaySection.BATCH -> CappedBand(
                        title = PrGraph.OverlayCopy.BATCH,
                        icon = ExpIcons.prBatch,
                        tag = "batch",
                        items = PrGraph.batchPartners(graph),
                    ) { issue ->
                        RelationIssueRow(issue, statuses, users, onClick = { openIssue(issue.id) })
                    }

                    PrGraph.OverlaySection.STACK -> CappedBand(
                        title = PrGraph.OverlayCopy.STACK,
                        icon = ExpIcons.prStack,
                        tag = "stack",
                        items = PrGraph.otherStackEntries(graph),
                    ) { member ->
                        val pr = member.entry.representative
                        PrRelationRow(pr, onClick = { openPr(pr.id) })
                    }
                }
            }
            if (sections.isEmpty()) {
                Text(
                    PrGraph.OverlayCopy.EMPTY,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/**
 * One band: open by default, [IssueRelationsView.BAND_CAP] rows then "Show N
 * more" / "Show less": the relations card's rule, over any row.
 */
@Composable
private fun <T> CappedBand(
    title: String,
    icon: ImageVector,
    tag: String,
    items: List<T>,
    row: @Composable (T) -> Unit,
) {
    var expanded by rememberSaveable(tag) { mutableStateOf(true) }
    var everything by rememberSaveable(tag) { mutableStateOf(false) }
    val overflow = items.size > IssueRelationsView.BAND_CAP
    val shown = if (everything || !overflow) items else items.take(IssueRelationsView.BAND_CAP)
    val footer = when {
        !expanded || !overflow -> null
        everything -> IssueRelationsView.Copy.SHOW_LESS
        else -> IssueRelationsView.showMore(items.size - IssueRelationsView.BAND_CAP)
    }
    RelationBandFrame(
        title = title,
        icon = icon,
        count = items.size,
        expanded = expanded,
        footer = footer,
        onToggle = { expanded = !expanded },
        onFooter = { everything = !everything },
        modifier = Modifier.testTag("pr-graph-band-$tag"),
    ) {
        shown.forEachIndexed { index, item ->
            if (index > 0) GroupDivider()
            row(item)
        }
    }
}

/**
 * A pull request in the relation row's shape: the state's PR glyph · mono
 * `#n` (the identifier when there is no number) · the representative's title
 * · the state pill. Flat 48dp, like [RelationIssueRow].
 */
@Composable
private fun PrRelationRow(pr: IssueEntity, onClick: () -> Unit) {
    val (glyph, tint) = when (pr.prState) {
        DomainContract.prStateMerged -> ExpIcons.prMerged to DoneBlue
        DomainContract.prStateClosed ->
            ExpIcons.prClosed to MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        DomainContract.prStateDraft ->
            ExpIcons.prDraft to MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        else -> ExpIcons.prOpen to DesignTokens.Semantic.Green
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 48.dp)
            .padding(horizontal = 12.dp)
            .testTag("pr-relation-row-${pr.identifier}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IssueRowContent(
            title = pr.title,
            leading = { Icon(glyph, contentDescription = null, modifier = Modifier.size(16.dp), tint = tint) },
            identifier = pr.prNumber?.let { "#$it" } ?: pr.identifier,
            trailing = { PrStatePill(pr.prState) },
        )
    }
}

/** The PR's state as one coloured word: open green, merged blue, else muted. */
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
