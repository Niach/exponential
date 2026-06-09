package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

// Web-parity active filter pills (apps/web/.../active-filter-pills.tsx): one
// removable capsule per active status / priority / label, shown below the tab
// bar. Each X removes just that value via the same (idempotent) toggle method.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ActiveFilterPills(
    filters: IssueFilters,
    labels: List<LabelEntity>,
    onToggleStatus: (IssueStatus) -> Unit,
    onTogglePriority: (IssuePriority) -> Unit,
    onToggleLabel: (String) -> Unit,
    onClear: () -> Unit,
) {
    if (filters.isEmpty) return
    val labelsById = remember(labels) { labels.associateBy { it.id } }
    FlowRow(
        modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        filters.statuses.forEach { status ->
            FilterPill(onRemove = { onToggleStatus(status) }) {
                StatusIcon(status, size = 13.dp)
                PillLabel(status.label)
            }
        }
        filters.priorities.forEach { priority ->
            FilterPill(onRemove = { onTogglePriority(priority) }) {
                PriorityIcon(priority, size = 13.dp)
                PillLabel(priority.label)
            }
        }
        filters.labelIds.forEach { labelId ->
            val label = labelsById[labelId] ?: return@forEach
            FilterPill(onRemove = { onToggleLabel(labelId) }) {
                LabelDot(remember(label.color) { parseColor(label.color) }, size = 8.dp)
                PillLabel(label.name)
            }
        }
        Text(
            "Clear all",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier
                .glassButton()
                .clickable(onClick = onClear)
                .padding(horizontal = 12.dp, vertical = 6.dp),
        )
    }
}

@Composable
private fun FilterPill(onRemove: () -> Unit, content: @Composable () -> Unit) {
    Row(
        modifier = Modifier
            .glassButton()
            .clickable(onClick = onRemove)
            .padding(start = 10.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        content()
        Icon(
            Icons.Filled.Close,
            contentDescription = "Remove filter",
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

@Composable
private fun PillLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Primary),
    )
}
