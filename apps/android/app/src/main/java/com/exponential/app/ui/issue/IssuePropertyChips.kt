package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.estimateShortLabel
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassCard

/**
 * What the chip box shows besides status + labels — an issue's fields, or the
 * New issue page's unsent ones (EXP-1170).
 */
data class IssuePropertySubject(
    val priority: IssuePriority,
    val assigneeId: String?,
    val dueDate: String?,
    val estimate: Int?,
) {
    companion object {
        fun fromIssue(issue: IssueEntity) = IssuePropertySubject(
            priority = IssuePriority.fromWire(issue.priority),
            assigneeId = issue.assigneeId,
            dueDate = issue.dueDate,
            estimate = issue.estimate,
        )
    }
}

/**
 * The top property chip box (EXP-240) — one glass box of wrapping capsule
 * chips replacing the stacked property/times cards + labels section: Status,
 * Priority, Assignee (hidden on solo teams, EXP-50), Due date (only when set),
 * Estimate (EXP-630: only while the team's scale is not `none`; reads
 * "Estimate" until set), one chip per assigned label, and a "+" chip. Chip taps open the per-property
 * sheets; the box background (FlowRow gaps included) and "+" open the combined
 * Properties sheet. Non-moderators see it dimmed and inert.
 *
 * EXP-1170: with no Properties sheet ([onOpenProperties] null, the New issue
 * page) there is no box tap and no "+"; the empty Labels + Due date chips show
 * instead, in the ×4 page order status · priority · assignee · labels · due
 * date · board.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IssuePropertyChips(
    subject: IssuePropertySubject,
    status: ResolvedIssueStatus,
    assignee: UserEntity?,
    issueLabels: List<LabelEntity>,
    isModerator: Boolean,
    hideAssignee: Boolean,
    /** The team's `estimation_type` (contract issueEstimation); `none`/null
     *  hides the estimate chip entirely (EXP-630). */
    estimationType: String?,
    onOpenStatus: () -> Unit,
    onOpenPriority: () -> Unit,
    onOpenAssignee: () -> Unit,
    onOpenDueDate: () -> Unit,
    onOpenEstimate: () -> Unit,
    onOpenLabels: () -> Unit,
    onOpenProperties: (() -> Unit)?,
    /** EXP-1170: the board chip, LAST in the box (the New issue page only). */
    board: String? = null,
    onOpenBoard: (() -> Unit)? = null,
) {
    val priority = subject.priority
    val pageMode = onOpenProperties == null
    val estimatesOn = estimationType != null && estimationType != DomainContract.issueEstimationNone
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier
            .fillMaxWidth()
            .glassCard()
            // Box-level clickable first, so the chips' own clickables win on
            // the chips and the gaps fall through to Properties.
            .then(
                if (isModerator && onOpenProperties != null) {
                    Modifier.clickable(onClick = onOpenProperties)
                } else {
                    Modifier
                },
            )
            // EXP-698: no outer .alpha() — every pill in here already dims
            // itself to quaternary when it is not enabled, and the two dims
            // stacked into an unreadable box for non-moderators.
            .padding(10.dp),
    ) {
        GlassPill(
            status.name,
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onOpenStatus,
            leading = { StatusIcon(status, size = GlassPillDefaults.SmGlyphSize) },
        )
        GlassPill(
            priority.label,
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onOpenPriority,
            leading = { PriorityIcon(priority, size = GlassPillDefaults.SmGlyphSize) },
        )
        if (!hideAssignee) {
            val assigneeName = subject.assigneeId?.let { userDisplayName(assignee, it) }
            GlassPill(
                assigneeName ?: "Unassigned",
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenAssignee,
                leading = if (assigneeName != null) {
                    {
                        // An avatar is a face, not a glyph: at the 12dp glyph
                        // rung its initials fell to ~5sp (EXP-698).
                        UserAvatar(
                            user = assignee,
                            nameOrEmail = assigneeName,
                            size = GlassPillDefaults.AvatarSize,
                        )
                    }
                } else {
                    null
                },
                icon = if (assigneeName == null) ExpIcons.uiUnassigned else null,
            )
        }
        if (pageMode) {
            LabelChips(issueLabels, isModerator, onOpenLabels, showEmpty = true)
            DueDateChip(subject.dueDate, isModerator, onOpenDueDate, showEmpty = true)
        } else {
            DueDateChip(subject.dueDate, isModerator, onOpenDueDate, showEmpty = false)
            if (estimatesOn) {
                GlassPill(
                    subject.estimate?.let { estimateShortLabel(it, estimationType!!) } ?: "Estimate",
                    size = PillSize.Sm,
                    enabled = isModerator,
                    onClick = onOpenEstimate,
                    icon = ExpIcons.uiEstimate,
                    maxLines = 1,
                )
            }
            LabelChips(issueLabels, isModerator, onOpenLabels, showEmpty = false)
        }
        if (board != null) {
            GlassPill(
                board,
                size = PillSize.Sm,
                enabled = onOpenBoard != null,
                onClick = onOpenBoard,
                icon = ExpIcons.navBoards,
                maxLines = 1,
                modifier = Modifier.testTag("issue-board-chip"),
            )
        }
        if (isModerator && onOpenProperties != null) {
            GlassPill(
                "",
                size = PillSize.Sm,
                onClick = onOpenProperties,
                icon = ExpIcons.uiAdd,
                // The glyph IS the label here, so it carries the name.
                contentDescription = "Edit properties",
            )
        }
    }
}

@Composable
private fun DueDateChip(dueDate: String?, isModerator: Boolean, onClick: () -> Unit, showEmpty: Boolean) {
    if (dueDate != null) {
        GlassPill(
            formatDueDate(dueDate),
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            icon = ExpIcons.uiDueDate,
            maxLines = 1,
            // Overdue/soon tints the whole pill, glyph and label alike.
            contentColor = dueDateColor(dueDate),
        )
    } else if (showEmpty) {
        GlassPill(
            "Due date",
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            icon = ExpIcons.uiDueDate,
            maxLines = 1,
        )
    }
}

@Composable
private fun LabelChips(labels: List<LabelEntity>, isModerator: Boolean, onClick: () -> Unit, showEmpty: Boolean) {
    labels.forEach { label ->
        GlassPill(
            label.name,
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            dot = parseColor(label.color),
        )
    }
    if (labels.isEmpty() && showEmpty) {
        GlassPill(
            "Labels",
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            icon = ExpIcons.settingsLabels,
        )
    }
}
