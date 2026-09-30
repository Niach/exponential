package com.exponential.app.ui.issue

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.components.ContextRingSize
import com.exponential.app.ui.components.ContextRingStroke
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassSectionBand
import com.exponential.app.ui.theme.resolvedStatusColor
import com.exponential.app.ui.theme.statusColor

// EXP-1097 (direction A): the issue's relations as the detail DRAWS them, off
// the ONE fixture-locked model (`IssueRelationsView`, ×4):
//   · [SubIssueOfLine]     — "Sub-issue of [chip]" above the title;
//   · [SubIssuesSection]   — completion ring · "Sub-issues" · `done/total` ·
//                            only a `+`, over flat 48dp hairline rows;
//   · [RelationsSection]   — inside the properties sheet: "Relations" + Add,
//                            then ONE foldable band per remaining side.
// Every row is THE relation row ×4: status glyph · identifier · title · avatar.

/** The row's status glyph, resolved against the subject's team statuses. */
private fun rowStatus(
    issue: IssueEntity?,
    row: IssueRelationsView.Row,
    statuses: List<ResolvedIssueStatus>,
): ResolvedIssueStatus =
    if (issue != null) {
        IssueStatusResolver.resolve(issue, statuses)
    } else {
        IssueStatusResolver.resolve(null, row.status, statuses)
    }

/**
 * "Sub-issue of [chip]" — the parent line above the title. A tap opens the
 * parent; a long press (members who edit) offers to drop the relation.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun SubIssueOfLine(
    parent: IssueRelationsView.Row,
    parentIssue: IssueEntity?,
    statuses: List<ResolvedIssueStatus>,
    onOpen: () -> Unit,
    onRemove: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("issue-parent-line"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        Icon(
            ExpIcons.relationSubIssue,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = muted,
        )
        Text(
            IssueRelationsView.Copy.SUB_ISSUE_OF,
            style = MaterialTheme.typography.labelMedium,
            color = muted,
            maxLines = 1,
        )
        IssueChip(
            identifier = parent.identifier,
            title = parent.title,
            status = rowStatus(parentIssue, parent, statuses),
            modifier = Modifier
                .weight(1f, fill = false)
                .combinedClickable(
                    onClick = onOpen,
                    onLongClick = onRemove?.let { { menuOpen = true } },
                ),
        )
    }
    if (menuOpen && onRemove != null) {
        RemoveRelationSheet(parent.identifier, onRemove) { menuOpen = false }
    }
}

/**
 * The Sub-issues section: a band (completion ring · "Sub-issues" · `2/5` ·
 * `+`) over flat hairline rows; with none yet, ONE "Add sub-issues" band.
 * [onAdd] null (a reader who cannot create) hides the `+` and the empty band.
 */
@Composable
fun SubIssuesSection(
    subIssues: IssueRelationsView.SubIssues,
    issuesById: Map<String, IssueEntity>,
    users: List<UserEntity>,
    statuses: List<ResolvedIssueStatus>,
    onAdd: (() -> Unit)?,
    onOpenIssue: (String) -> Unit,
    onRemove: ((String) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    if (subIssues.rows.isEmpty()) {
        if (onAdd == null) return
        Row(
            modifier = modifier
                .fillMaxWidth()
                .glassSectionBand()
                .clickable(onClick = onAdd)
                .heightIn(min = 44.dp)
                .padding(horizontal = 12.dp)
                .testTag("add-sub-issues"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            Icon(ExpIcons.uiAdd, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
            Spacer(Modifier.width(10.dp))
            Text(
                IssueRelationsView.Copy.ADD_SUB_ISSUES,
                style = MaterialTheme.typography.bodyMedium,
                color = muted,
            )
        }
        return
    }
    Column(modifier = modifier.fillMaxWidth().testTag("sub-issues")) {
        SectionHeader(
            IssueRelationsView.Copy.SUB_ISSUES,
            leading = {
                SubIssueProgressRing(
                    done = subIssues.done,
                    total = subIssues.total,
                    color = doneColor(statuses),
                )
            },
            trailing = {
                subIssues.progress?.let { progress ->
                    Text(
                        progress,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 1,
                    )
                }
                if (onAdd != null) {
                    // The band's ONE control; the 24dp box keeps the band at
                    // its title's height (M3's 48dp minimum suppressed).
                    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
                        IconButton(
                            onClick = onAdd,
                            modifier = Modifier.size(24.dp).testTag("sub-issues-add"),
                        ) {
                            Icon(
                                ExpIcons.uiAdd,
                                contentDescription = "Add sub-issue",
                                modifier = Modifier.size(16.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                }
            },
        )
        RelationRows(
            rows = subIssues.rows,
            issuesById = issuesById,
            users = users,
            statuses = statuses,
            onOpenIssue = onOpenIssue,
            onRemove = onRemove,
        )
    }
}

/**
 * The relations inside the properties sheet: the "Relations" heading with an
 * Add pill, then ONE foldable band per side (chevron · icon · title · count),
 * its rows under it, "Show N more" / "Show less" past the cap. Fold state is
 * the caller's view state ([onToggle] / [onShowAll]).
 */
@Composable
fun RelationsSection(
    bands: List<IssueRelationsView.Band>,
    issuesById: Map<String, IssueEntity>,
    users: List<UserEntity>,
    statuses: List<ResolvedIssueStatus>,
    onAdd: () -> Unit,
    onToggle: (IssueRelationsView.BandKey) -> Unit,
    onShowAll: (IssueRelationsView.BandKey, Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onRemove: (IssueRelationsView.BandKey, String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth().testTag("relations-section")) {
        Row(
            // 16dp group gutter + the group's own 4dp inset, so the heading
            // sits over the rows the way the labels heading does.
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                IssueRelationsView.Copy.RELATIONS,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.weight(1f),
            )
            GlassPill(
                IssueRelationsView.Copy.ADD,
                onClick = onAdd,
                size = PillSize.Sm,
                icon = ExpIcons.uiAdd,
                modifier = Modifier.testTag("relations-add"),
            )
        }
        Spacer(Modifier.height(8.dp))
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            bands.forEach { band ->
                RelationBand(
                    band = band,
                    issuesById = issuesById,
                    users = users,
                    statuses = statuses,
                    onToggle = { onToggle(band.key) },
                    onShowAll = { all -> onShowAll(band.key, all) },
                    onOpenIssue = onOpenIssue,
                    onRemove = { id -> onRemove(band.key, id) },
                )
            }
        }
    }
}

@Composable
private fun RelationBand(
    band: IssueRelationsView.Band,
    issuesById: Map<String, IssueEntity>,
    users: List<UserEntity>,
    statuses: List<ResolvedIssueStatus>,
    onToggle: () -> Unit,
    onShowAll: (Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onRemove: (String) -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth().testTag("relation-band-${band.key.wire}")) {
        SectionHeader(
            band.title,
            modifier = Modifier
                .clickable(onClick = onToggle)
                .semantics {
                    contentDescription =
                        "${band.title}, ${band.count}, ${if (band.expanded) "expanded" else "collapsed"}"
                },
            leading = {
                Icon(
                    if (band.expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = muted,
                )
                Icon(
                    bandIcon(band.key),
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = muted,
                )
            },
            trailing = {
                Text(
                    band.count.toString(),
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = FontFamily.Monospace,
                    color = muted,
                    maxLines = 1,
                )
            },
        )
        if (band.rows.isNotEmpty()) {
            RelationRows(
                rows = band.rows,
                issuesById = issuesById,
                users = users,
                statuses = statuses,
                onOpenIssue = onOpenIssue,
                onRemove = onRemove,
            )
        }
        val footer = band.more ?: band.less
        if (footer != null) {
            Text(
                footer,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier
                    .fillMaxWidth()
                    .flatRow()
                    .clickable { onShowAll(band.more != null) }
                    .heightIn(min = 44.dp)
                    .padding(horizontal = 12.dp, vertical = 12.dp),
            )
        }
    }
}

/** The glyph a band's side reads as — the relation picker's, per side. */
private fun bandIcon(key: IssueRelationsView.BandKey): ImageVector = when (key) {
    IssueRelationsView.BandKey.BlockedBy -> ExpIcons.relationBlockedBy
    IssueRelationsView.BandKey.Blocking -> ExpIcons.relationBlocks
    IssueRelationsView.BandKey.DuplicateOf,
    IssueRelationsView.BandKey.DuplicatedBy -> ExpIcons.relationDuplicate
    IssueRelationsView.BandKey.Related -> ExpIcons.relationRelated
}

/** Flat 48dp rows divided by the group hairline. */
@Composable
private fun RelationRows(
    rows: List<IssueRelationsView.Row>,
    issuesById: Map<String, IssueEntity>,
    users: List<UserEntity>,
    statuses: List<ResolvedIssueStatus>,
    onOpenIssue: (String) -> Unit,
    onRemove: ((String) -> Unit)?,
) {
    rows.forEachIndexed { index, row ->
        if (index > 0) GroupDivider()
        val issue = issuesById[row.id]
        RelationIssueRow(
            row = row,
            status = rowStatus(issue, row, statuses),
            assignee = issue?.assigneeId?.let { id -> users.firstOrNull { it.id == id } },
            assigneeId = issue?.assigneeId,
            onClick = { onOpenIssue(row.id) },
            onRemove = onRemove?.let { remove -> { remove(row.id) } },
        )
    }
}

/**
 * SLOP-16 r3: THE relation row for a plain synced [issue] — the "Related
 * work" view and the Reviews batch sheet list issues with it, so an issue
 * reads the same wherever it is listed. No long-press: nothing to remove.
 */
@Composable
internal fun RelationIssueRow(
    issue: IssueEntity,
    statuses: List<ResolvedIssueStatus>,
    users: List<UserEntity>,
    onClick: () -> Unit,
) {
    RelationIssueRow(
        row = IssueRelationsView.Row(
            id = issue.id,
            identifier = issue.identifier,
            title = issue.title,
            status = issue.status,
            open = IssueRelationsView.isOpenAnchor(issue.status),
        ),
        status = IssueStatusResolver.resolve(issue, statuses),
        assignee = issue.assigneeId?.let { id -> users.firstOrNull { it.id == id } },
        assigneeId = issue.assigneeId,
        onClick = onClick,
        onRemove = null,
    )
}

/** THE relation row ×4: status glyph · identifier · title · assignee. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RelationIssueRow(
    row: IssueRelationsView.Row,
    status: ResolvedIssueStatus,
    assignee: UserEntity?,
    assigneeId: String?,
    onClick: () -> Unit,
    onRemove: (() -> Unit)?,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .combinedClickable(
                onClick = onClick,
                onLongClick = onRemove?.let { { menuOpen = true } },
            )
            .heightIn(min = 48.dp)
            .padding(horizontal = 12.dp)
            .testTag("relation-row-${row.identifier}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StatusIcon(status, size = 16.dp)
        Spacer(Modifier.width(12.dp))
        Text(
            row.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
        )
        Spacer(Modifier.width(12.dp))
        Text(
            row.title,
            style = MaterialTheme.typography.bodyMedium,
            // A closed row reads a step back (web `text-foreground/60`).
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (row.open) 1f else 0.6f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(12.dp))
        if (assigneeId != null) {
            UserAvatar(
                user = assignee,
                nameOrEmail = assignee?.let { it.name ?: it.email },
                size = 24.dp,
                userId = assigneeId,
            )
        } else {
            Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
                Icon(
                    ExpIcons.uiAssignee,
                    contentDescription = "Unassigned",
                    modifier = Modifier.size(20.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                )
            }
        }
    }
    if (menuOpen && onRemove != null) {
        RemoveRelationSheet(row.identifier, onRemove) { menuOpen = false }
    }
}

/** A long press's ONE action — the relation goes, the issue stays. */
@Composable
private fun RemoveRelationSheet(identifier: String, onRemove: () -> Unit, onDismiss: () -> Unit) {
    GlassSheet(title = identifier, onDismiss = onDismiss) {
        GlassSheetRow(
            label = "Remove relation",
            onClick = {
                onRemove()
                onDismiss()
            },
            leading = {
                Icon(ExpIcons.eventRelationRemoved, contentDescription = null, modifier = Modifier.size(18.dp))
            },
        )
    }
}

/** The completed status's colour: the team's builtin Done row, else the anchor's. */
private fun doneColor(statuses: List<ResolvedIssueStatus>): Color =
    statuses.firstOrNull { it.builtinKey == IssueStatus.Done }?.let { resolvedStatusColor(it) }
        ?: statusColor(IssueStatus.Done)

/**
 * The sub-issue completion ring: the context ring's 16dp box, 2dp stroke and
 * track, the arc in the COMPLETED status colour, clockwise from twelve.
 */
@Composable
fun SubIssueProgressRing(done: Int, total: Int, color: Color, modifier: Modifier = Modifier) {
    val fraction = if (total <= 0) 0f else (done.toFloat() / total).coerceIn(0f, 1f)
    Canvas(
        modifier = modifier
            .size(ContextRingSize)
            .testTag("sub-issues-ring")
            .semantics { contentDescription = "$done of $total sub-issues done" },
    ) {
        val strokePx = ContextRingStroke.toPx()
        val inset = strokePx / 2f
        val arcSize = Size(size.width - strokePx, size.height - strokePx)
        val topLeft = Offset(inset, inset)
        drawArc(
            color = color.copy(alpha = 0.22f),
            startAngle = -90f,
            sweepAngle = 360f,
            useCenter = false,
            topLeft = topLeft,
            size = arcSize,
            style = Stroke(width = strokePx),
        )
        if (fraction > 0f) {
            drawArc(
                color = color,
                startAngle = -90f,
                sweepAngle = 360f * fraction,
                useCenter = false,
                topLeft = topLeft,
                size = arcSize,
                style = Stroke(width = strokePx, cap = StrokeCap.Round),
            )
        }
    }
}
