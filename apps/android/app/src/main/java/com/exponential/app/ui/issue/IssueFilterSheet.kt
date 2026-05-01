package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.domain.statusIcon

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueFilterSheet(
    filters: IssueFilters,
    labels: List<LabelEntity>,
    onToggleStatus: (IssueStatus) -> Unit,
    onTogglePriority: (IssuePriority) -> Unit,
    onToggleLabel: (String) -> Unit,
    onClear: () -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Filters", style = MaterialTheme.typography.titleMedium)
                if (!filters.isEmpty) {
                    TextButton(onClick = onClear) { Text("Clear") }
                }
            }
            Spacer(Modifier.height(8.dp))

            SectionLabel("Status")
            issueStatusOrder.forEach { status ->
                FilterCheckRow(
                    selected = status in filters.statuses,
                    onClick = { onToggleStatus(status) },
                ) {
                    Icon(statusIcon(status), null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(status.label)
                }
            }

            Spacer(Modifier.height(8.dp))
            SectionLabel("Priority")
            issuePriorityOrder.forEach { priority ->
                FilterCheckRow(
                    selected = priority in filters.priorities,
                    onClick = { onTogglePriority(priority) },
                ) {
                    Icon(priorityIcon(priority), null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(priority.label)
                }
            }

            if (labels.isNotEmpty()) {
                Spacer(Modifier.height(8.dp))
                SectionLabel("Labels")
                labels.forEach { label ->
                    FilterCheckRow(
                        selected = label.id in filters.labelIds,
                        onClick = { onToggleLabel(label.id) },
                    ) {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(parseColor(label.color), CircleShape),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(label.name)
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(vertical = 4.dp),
    )
}

@Composable
private fun FilterCheckRow(
    selected: Boolean,
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) { content() }
        }
        if (selected) {
            Icon(
                Icons.Filled.Check,
                contentDescription = "Selected",
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

internal fun parseColor(hex: String): Color {
    val cleaned = hex.removePrefix("#")
    return runCatching {
        Color(android.graphics.Color.parseColor(if (cleaned.length == 6) "#$cleaned" else "#FF$cleaned"))
    }.getOrElse { Color(0xFF6366F1) }
}
