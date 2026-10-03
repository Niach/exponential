package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.NO_ESTIMATE
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.estimateLabel
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
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
 * The combined Properties sheet (EXP-240): Status / Priority / Assignee /
 * Labels / Due date / Estimate (EXP-630, only while the team's scale is not
 * `none`) / Board rows in one [OptionGroup], then the relations.
 *
 * EXP-1170: Labels is a [MetaRow] like the rest (the assigned names, or
 * "None"); tapping it opens the shared multi picker sheet (`onOpenLabels`),
 * which toggles labels itself and stays open. No inline chip cloud.
 */
@Composable
fun PropertiesSheet(
    issue: IssueEntity,
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    assignee: UserEntity?,
    hideAssignee: Boolean,
    /** Every label in the team; orders the Labels row's value. */
    teamLabels: List<LabelEntity>,
    issueLabels: List<LabelEntity>,
    currentBoard: BoardEntity?,
    hasMoveTargets: Boolean,
    /** The team's `estimation_type`; `none`/null hides the Estimate row. */
    estimationType: String?,
    onOpenStatus: () -> Unit,
    onOpenPriority: () -> Unit,
    onOpenAssignee: () -> Unit,
    onOpenDueDate: () -> Unit,
    onOpenEstimate: () -> Unit,
    onOpenLabels: () -> Unit,
    onOpenMoveBoard: () -> Unit,
    // EXP-736/EXP-1097: the relations OTHER than parent / sub-issues (those
    // live on the detail page) as foldable bands — where an edge is added or
    // dropped (long press) on mobile.
    relationBands: List<IssueRelationsView.Band>,
    relationIssues: Map<String, IssueEntity>,
    users: List<UserEntity>,
    teamStatuses: List<ResolvedIssueStatus>,
    onOpenRelations: () -> Unit,
    onToggleRelationBand: (IssueRelationsView.BandKey) -> Unit,
    onShowAllRelations: (IssueRelationsView.BandKey, Boolean) -> Unit,
    onOpenIssue: (String) -> Unit,
    onRemoveRelation: (IssueRelationsView.BandKey, String) -> Unit,
    onDismiss: () -> Unit,
) {
    // The assigned labels' names in the TEAM's label order.
    val assignedLabelNames = remember(teamLabels, issueLabels) {
        val assignedIds = issueLabels.map { it.id }.toSet()
        teamLabels.filter { it.id in assignedIds }.map { it.name }
    }
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
                // EXP-1170: labels are a ROW like every other property; the
                // tap opens the shared multi picker (LabelPickerSheet), never
                // an inline cloud of toggle chips.
                GroupDivider()
                MetaRow(label = "Labels", enabled = true, onClick = onOpenLabels) {
                    val hasLabels = assignedLabelNames.isNotEmpty()
                    Icon(
                        ExpIcons.settingsLabels,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (hasLabels) TextEmphasis.Secondary else TextEmphasis.Tertiary,
                        ),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        if (hasLabels) assignedLabelNames.joinToString(", ") else "None",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = if (hasLabels) TextEmphasis.Primary else TextEmphasis.Tertiary,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
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
                // EXP-630: the estimate on the team's scale — the web's phone
                // sheet row after Due date; absent while estimates are off.
                if (estimationType != null && estimationType != DomainContract.issueEstimationNone) {
                    GroupDivider()
                    MetaRow(label = "Estimate", enabled = true, onClick = onOpenEstimate) {
                        Icon(
                            ExpIcons.uiEstimate,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = if (issue.estimate != null) TextEmphasis.Secondary else TextEmphasis.Tertiary,
                            ),
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            issue.estimate?.let { estimateLabel(it, estimationType) } ?: NO_ESTIMATE,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = if (issue.estimate != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                            ),
                        )
                    }
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
            RelationsSection(
                bands = relationBands,
                issuesById = relationIssues,
                users = users,
                statuses = teamStatuses,
                onAdd = onOpenRelations,
                onToggle = onToggleRelationBand,
                onShowAll = onShowAllRelations,
                onOpenIssue = onOpenIssue,
                onRemove = onRemoveRelation,
            )
            Spacer(Modifier.height(8.dp))
        }
    }
}
