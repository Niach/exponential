package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
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
 * The top property chip box (EXP-240) — one glass box of wrapping capsule
 * chips replacing the stacked property/times cards + labels section: Status,
 * Priority, Assignee (hidden on solo teams, EXP-50), Due date (only when set),
 * one chip per assigned label, and a "+" chip. Chip taps open the per-property
 * sheets; the box background (FlowRow gaps included) and "+" open the combined
 * Properties sheet. Non-moderators see it dimmed and inert.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IssuePropertyChips(
    issue: IssueEntity,
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    assignee: UserEntity?,
    issueLabels: List<LabelEntity>,
    isModerator: Boolean,
    hideAssignee: Boolean,
    onOpenStatus: () -> Unit,
    onOpenPriority: () -> Unit,
    onOpenAssignee: () -> Unit,
    onOpenDueDate: () -> Unit,
    onOpenLabels: () -> Unit,
    onOpenProperties: () -> Unit,
) {
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier
            .fillMaxWidth()
            .glassCard()
            // Box-level clickable first, so the chips' own clickables win on
            // the chips and the gaps fall through to Properties.
            .then(if (isModerator) Modifier.clickable(onClick = onOpenProperties) else Modifier)
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
            val assigneeName = issue.assigneeId?.let { userDisplayName(assignee, it) }
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
        if (issue.dueDate != null) {
            GlassPill(
                formatDueDate(issue.dueDate),
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenDueDate,
                icon = ExpIcons.uiDueDate,
                maxLines = 1,
                // Overdue/soon tints the whole pill, glyph and label alike.
                contentColor = dueDateColor(issue.dueDate),
            )
        }
        issueLabels.forEach { label ->
            GlassPill(
                label.name,
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenLabels,
                dot = parseColor(label.color),
            )
        }
        if (isModerator) {
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
