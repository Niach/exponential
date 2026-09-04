package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LabelsPickerBlock
import com.exponential.app.ui.components.MetaRow
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor

/**
 * The combined Properties sheet (EXP-240): Status / Priority / Assignee / Due
 * date / Board rows, then the team's labels as toggle pills.
 *
 * EXP-698 r5 made it the create-issue screen's rows exactly — the same
 * [MetaRow] in the same [OptionGroup], with the same value glyphs and the same
 * [LabelsPickerBlock] under it. Editing a property after the fact and setting
 * it while creating are the same act, and they used to be two different
 * screens: this one drew a chevron per row and only the labels already on the
 * issue, so removing one was possible and adding one meant a second sheet.
 */
@Composable
fun PropertiesSheet(
    issue: IssueEntity,
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    assignee: UserEntity?,
    hideAssignee: Boolean,
    /** Every label in the team — the pills are a SELECT, not a read-out of
     *  what is already assigned. */
    teamLabels: List<LabelEntity>,
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
    // EXP-736: relations live ONLY here on mobile — the detail page keeps the
    // chip tray, so the sheet is where an edge is added or dropped.
    relations: List<RelationRow>,
    onOpenRelations: () -> Unit,
    onRemoveRelation: (RelationRow) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(title = "Properties", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            OptionGroup {
                MetaRow(label = "Status", enabled = true, onClick = onOpenStatus) {
                    StatusIcon(status, size = 14.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(
                        status.name,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
                GroupDivider()
                MetaRow(label = "Priority", enabled = true, onClick = onOpenPriority) {
                    PriorityIcon(priority, size = 14.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(
                        priority.label,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
                // EXP-50: hidden in a solo team (no one else to assign to).
                if (!hideAssignee) {
                    GroupDivider()
                    MetaRow(label = "Assignee", enabled = true, onClick = onOpenAssignee) {
                        Icon(
                            if (issue.assigneeId != null) ExpIcons.uiAssignee else ExpIcons.uiUnassigned,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            if (issue.assigneeId != null) {
                                userDisplayName(assignee, issue.assigneeId)
                            } else {
                                "Unassigned"
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                GroupDivider()
                MetaRow(label = "Due date", enabled = true, onClick = onOpenDueDate) {
                    Icon(
                        ExpIcons.uiDueDate,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = issue.dueDate?.let { dueDateColor(it) }
                            ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        // The create screen's wording: an EMPTY value says so
                        // in words, like "Unassigned" a row above.
                        issue.dueDate?.let { formatDueDate(it) } ?: "No date",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (issue.dueDate != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                        ),
                    )
                }
                // Board is the one row the create screen has no twin for (it
                // picks the board first) — hidden when there is nowhere to go.
                if (hasMoveTargets) {
                    GroupDivider()
                    MetaRow(label = "Board", enabled = true, onClick = onOpenMoveBoard) {
                        if (currentBoard != null) {
                            BoardIcon(currentBoard, size = 14.dp)
                        } else {
                            Icon(
                                ExpIcons.navBoards,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                        Spacer(Modifier.width(6.dp))
                        Text(
                            currentBoard?.name ?: "Move to board",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }

            Spacer(Modifier.height(16.dp))
            val assignedIds = remember(issueLabels) { issueLabels.map { it.id }.toSet() }
            LabelsPickerBlock(
                labels = teamLabels,
                selectedIds = assignedIds,
                onToggle = onToggleLabel,
                onOpenPicker = onOpenLabels,
                // 16dp group gutter + the group's own 4dp inset, so the
                // heading sits under the rows' label column.
                modifier = Modifier.padding(horizontal = 20.dp),
            )
            Spacer(Modifier.height(16.dp))
            RelationsSection(
                relations = relations,
                onOpenRelations = onOpenRelations,
                onRemoveRelation = onRemoveRelation,
            )
            Spacer(Modifier.height(8.dp))
        }
    }
}
