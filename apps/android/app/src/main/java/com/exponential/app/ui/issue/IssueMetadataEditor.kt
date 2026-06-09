package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// The stacked metadata/property cards of the issue detail screen: grouped
// Status/Priority/Assignee, Due date, Start/End times, Repeat, and the Labels
// chip section. Extracted from IssueDetailScreen.kt (pure move — no behavior
// change); the pickers/dialogs stay in the screen, driven via the callbacks.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IssueMetadataEditor(
    issue: IssueEntity,
    status: IssueStatus,
    priority: IssuePriority,
    assignee: UserEntity?,
    workspaceLabels: List<LabelEntity>,
    issueLabels: List<LabelEntity>,
    isModerator: Boolean,
    onStatusClick: () -> Unit,
    onPriorityClick: () -> Unit,
    onAssigneeClick: () -> Unit,
    onDueDateClick: () -> Unit,
    onClearDueDate: () -> Unit,
    onStartTimeClick: () -> Unit,
    onEndTimeClick: () -> Unit,
    onRepeatClick: () -> Unit,
    onToggleLabel: (labelId: String, assigned: Boolean) -> Unit,
    onAddLabel: () -> Unit,
) {
    val mutedTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    // Grouped Status / Priority / Assignee card
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassSection()
            .padding(vertical = 4.dp)
            .alpha(if (isModerator) 1f else 0.55f),
    ) {
        DetailRow(label = "Status", enabled = isModerator, onClick = onStatusClick) {
            StatusIcon(status, size = 14.dp)
            Spacer(Modifier.width(6.dp))
            Text(status.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
        }
        CardDivider()
        DetailRow(label = "Priority", enabled = isModerator, onClick = onPriorityClick) {
            PriorityIcon(priority, size = 14.dp)
            Spacer(Modifier.width(6.dp))
            Text(priority.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
        }
        CardDivider()
        DetailRow(label = "Assignee", enabled = isModerator, onClick = onAssigneeClick) {
            Text(
                assignee?.name ?: assignee?.email ?: "Unassigned",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (assignee != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                ),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }

    Spacer(Modifier.height(20.dp))
    // Due date card
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassSection()
            .padding(vertical = 4.dp)
            .alpha(if (isModerator) 1f else 0.55f),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .then(if (isModerator) Modifier.clickable(onClick = onDueDateClick) else Modifier)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Filled.CalendarMonth,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = if (issue.dueDate != null) dueDateColor(issue.dueDate) else mutedTint,
            )
            Spacer(Modifier.width(10.dp))
            Text("Due date", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
            Spacer(Modifier.weight(1f))
            if (issue.dueDate != null) {
                Text(
                    formatDueDate(issue.dueDate),
                    style = MaterialTheme.typography.bodyMedium,
                    color = dueDateColor(issue.dueDate),
                )
                Spacer(Modifier.width(8.dp))
                Icon(
                    Icons.Filled.Close,
                    contentDescription = "Clear due date",
                    modifier = Modifier
                        .size(18.dp)
                        .then(if (isModerator) Modifier.clickable(onClick = onClearDueDate) else Modifier),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            } else {
                Text(
                    "None",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }

    // Start / End time card (only when a due date is set)
    if (issue.dueDate != null) {
        Spacer(Modifier.height(20.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .glassSection()
                .padding(vertical = 4.dp)
                .alpha(if (isModerator) 1f else 0.55f),
        ) {
            DetailRow(label = "Start time", enabled = isModerator, onClick = onStartTimeClick) {
                TimeValue(issue.dueTime)
            }
            CardDivider()
            DetailRow(label = "End time", enabled = isModerator, onClick = onEndTimeClick) {
                TimeValue(issue.endTime)
            }
        }
    }

    Spacer(Modifier.height(20.dp))
    // Repeat card
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassSection()
            .padding(vertical = 4.dp)
            .alpha(if (isModerator) 1f else 0.55f),
    ) {
        DetailRow(label = "Repeat", enabled = isModerator, onClick = onRepeatClick) {
            Text(
                formatRecurrence(issue.recurrenceInterval, issue.recurrenceUnit),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (issue.recurrenceInterval == null) TextEmphasis.Tertiary else TextEmphasis.Primary,
                ),
            )
        }
    }

    Spacer(Modifier.height(20.dp))
    // Labels section (all workspace labels as colored-dot toggle chips)
    Text(
        "Labels",
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
    Spacer(Modifier.height(8.dp))
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val assignedIds = remember(issueLabels) { issueLabels.map { it.id }.toSet() }
        workspaceLabels.forEach { label ->
            val assigned = label.id in assignedIds
            Row(
                modifier = Modifier
                    .glassButton(active = assigned)
                    .then(if (isModerator) Modifier.clickable { onToggleLabel(label.id, assigned) } else Modifier)
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(modifier = Modifier.size(8.dp).background(parseColor(label.color), CircleShape))
                Spacer(Modifier.width(5.dp))
                Text(label.name, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurface)
            }
        }
        if (isModerator) {
            Row(
                modifier = Modifier
                    .glassButton()
                    .clickable(onClick = onAddLabel)
                    .padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(14.dp), tint = mutedTint)
                Spacer(Modifier.width(4.dp))
                Text("Label", style = MaterialTheme.typography.labelSmall, color = mutedTint)
            }
        }
    }
}

// One row of a grouped glass card: fixed-width label on the left, value pushed
// to the trailing edge (iOS detailRow). Tappable when [enabled].
@Composable
private fun DetailRow(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    value: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.width(84.dp),
        )
        Spacer(Modifier.weight(1f))
        value()
    }
}

@Composable
private fun TimeValue(time: String?) {
    Text(
        time ?: "—",
        style = MaterialTheme.typography.bodyMedium,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (time != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
        ),
    )
}

// Hairline divider between grouped-card rows (iOS Divider white@6%).
@Composable
private fun CardDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}
