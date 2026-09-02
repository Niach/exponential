package com.exponential.app.ui.issue

import androidx.compose.foundation.background
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor

/**
 * The combined Properties sheet (EXP-240): Status / Priority / Assignee / Due
 * date rows (current value + chevron, each handing off to its per-property
 * sheet — Android stacks the child sheet over this one), an inline Labels
 * section (assigned chips toggle off; "+" opens the label sheet), and a Board
 * row into the existing move-board picker (hidden when there is nowhere to
 * move).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PropertiesSheet(
    issue: IssueEntity,
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    assignee: UserEntity?,
    hideAssignee: Boolean,
    issueLabels: List<LabelEntity>,
    currentBoard: BoardEntity?,
    hasMoveTargets: Boolean,
    onOpenStatus: () -> Unit,
    onOpenPriority: () -> Unit,
    onOpenAssignee: () -> Unit,
    onOpenDueDate: () -> Unit,
    onOpenLabels: () -> Unit,
    onOpenMoveBoard: () -> Unit,
    onToggleLabel: (labelId: String, assigned: Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = "Properties", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            GlassSheetRow(
                label = "Status",
                leading = { StatusIcon(status, size = 16.dp) },
                trailing = { ValueWithChevron(status.name) },
                onClick = onOpenStatus,
            )
            GlassSheetRow(
                label = "Priority",
                leading = { PriorityIcon(priority, size = 16.dp) },
                trailing = { ValueWithChevron(priority.label) },
                onClick = onOpenPriority,
            )
            if (!hideAssignee) {
                GlassSheetRow(
                    label = "Assignee",
                    leading = {
                        if (issue.assigneeId != null) {
                            UserAvatar(
                                user = assignee,
                                nameOrEmail = userDisplayName(assignee, issue.assigneeId),
                                size = 20.dp,
                            )
                        } else {
                            Icon(
                                ExpIcons.uiUnassigned,
                                contentDescription = null,
                                modifier = Modifier.size(16.dp),
                                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    },
                    trailing = {
                        ValueWithChevron(
                            if (issue.assigneeId != null) userDisplayName(assignee, issue.assigneeId)
                            else "Unassigned",
                        )
                    },
                    onClick = onOpenAssignee,
                )
            }
            GlassSheetRow(
                label = "Due date",
                leading = {
                    Icon(
                        ExpIcons.uiDueDate,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = if (issue.dueDate != null) dueDateColor(issue.dueDate)
                        else Color.White.copy(alpha = TextEmphasis.Secondary),
                    )
                },
                trailing = {
                    ValueWithChevron(issue.dueDate?.let { formatDueDate(it) } ?: "None")
                },
                onClick = onOpenDueDate,
            )

            Spacer(Modifier.height(8.dp))
            // EXP-698: Labels is a property like the four rows above it, so it
            // gets their leading glyph in the same 30dp column — it was the one
            // heading in the sheet whose label started at the row TEXT's x.
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = GlassSheetDefaults.HorizontalPadding),
            ) {
                Box(modifier = Modifier.width(30.dp), contentAlignment = Alignment.CenterStart) {
                    Icon(
                        ExpIcons.settingsLabels,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                Text(
                    "Labels",
                    style = MaterialTheme.typography.labelMedium,
                    color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                )
            }
            Spacer(Modifier.height(8.dp))
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = GlassSheetDefaults.HorizontalPadding),
            ) {
                val assignedIds = remember(issueLabels) { issueLabels.map { it.id }.toSet() }
                issueLabels.forEach { label ->
                    GlassPill(
                        label.name,
                        size = PillSize.Sm,
                        mode = PillMode.Select,
                        selected = true,
                        dot = parseColor(label.color),
                        onClick = { onToggleLabel(label.id, label.id in assignedIds) },
                    )
                }
                GlassPill(
                    "Label",
                    size = PillSize.Sm,
                    icon = ExpIcons.uiAdd,
                    onClick = onOpenLabels,
                )
            }
            Spacer(Modifier.height(12.dp))

            if (hasMoveTargets) {
                GlassSheetRow(
                    label = "Board",
                    leading = {
                        if (currentBoard != null) {
                            BoardIcon(currentBoard)
                        } else {
                            Icon(
                                ExpIcons.navBoards,
                                contentDescription = null,
                                modifier = Modifier.size(16.dp),
                                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    },
                    trailing = { ValueWithChevron(currentBoard?.name ?: "Move to board") },
                    onClick = onOpenMoveBoard,
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun ValueWithChevron(value: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(2.dp))
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
