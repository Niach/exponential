package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.PersonOff
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

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
    status: IssueStatus,
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
            .glassSection()
            // Box-level clickable first, so the chips' own clickables win on
            // the chips and the gaps fall through to Properties.
            .then(if (isModerator) Modifier.clickable(onClick = onOpenProperties) else Modifier)
            .padding(10.dp)
            .alpha(if (isModerator) 1f else 0.55f),
    ) {
        PropertyChip(enabled = isModerator, onClick = onOpenStatus) {
            StatusIcon(status, size = 14.dp)
            Spacer(Modifier.width(6.dp))
            ChipLabel(status.label)
        }
        PropertyChip(enabled = isModerator, onClick = onOpenPriority) {
            PriorityIcon(priority, size = 14.dp)
            Spacer(Modifier.width(6.dp))
            ChipLabel(priority.label)
        }
        if (!hideAssignee) {
            PropertyChip(enabled = isModerator, onClick = onOpenAssignee) {
                if (issue.assigneeId != null) {
                    val name = userDisplayName(assignee, issue.assigneeId)
                    UserAvatar(user = assignee, nameOrEmail = name, size = 18.dp)
                    Spacer(Modifier.width(6.dp))
                    ChipLabel(name)
                } else {
                    Icon(
                        Icons.Filled.PersonOff,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Spacer(Modifier.width(6.dp))
                    ChipLabel("Unassigned", muted = true)
                }
            }
        }
        if (issue.dueDate != null) {
            PropertyChip(enabled = isModerator, onClick = onOpenDueDate) {
                Icon(
                    Icons.Filled.CalendarMonth,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = dueDateColor(issue.dueDate),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    formatDueDate(issue.dueDate),
                    style = MaterialTheme.typography.labelMedium,
                    color = dueDateColor(issue.dueDate),
                    maxLines = 1,
                )
            }
        }
        issueLabels.forEach { label ->
            PropertyChip(enabled = isModerator, onClick = onOpenLabels) {
                Box(
                    modifier = Modifier
                        .size(8.dp)
                        .background(parseColor(label.color), CircleShape),
                )
                Spacer(Modifier.width(6.dp))
                ChipLabel(label.name)
            }
        }
        if (isModerator) {
            PropertyChip(enabled = true, onClick = onOpenProperties) {
                Icon(
                    Icons.Filled.Add,
                    contentDescription = "Edit properties",
                    modifier = Modifier.size(14.dp),
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
    }
}

@Composable
private fun PropertyChip(
    enabled: Boolean,
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier
            .glassButton()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 10.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content()
    }
}

@Composable
private fun ChipLabel(text: String, muted: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.labelMedium,
        color = Color.White.copy(alpha = if (muted) TextEmphasis.Tertiary else 0.9f),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}
