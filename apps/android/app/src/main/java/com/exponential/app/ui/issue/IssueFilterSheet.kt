package com.exponential.app.ui.issue

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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.issueStatusOrder
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis

private enum class FilterView { Categories, Status, Priority, Labels }

// Web-parity drill-down filter sheet (apps/web/.../issue-filter-popover.tsx): a
// category list (Status / Priority / Labels, each with its active count) drills
// into a dedicated sub-view; the Labels sub-view adds a search field. All toggles
// reuse the shared IssueFilters model + the ViewModel toggle methods.
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
    var view by remember { mutableStateOf(FilterView.Categories) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp).fillMaxWidth()) {
            when (view) {
                FilterView.Categories -> {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Filters", style = MaterialTheme.typography.titleMedium)
                        if (!filters.isEmpty) {
                            TextButton(onClick = onClear) { Text("Clear all") }
                        }
                    }
                    Spacer(Modifier.height(4.dp))
                    CategoryRow("Status", filters.statuses.size) { view = FilterView.Status }
                    CategoryRow("Priority", filters.priorities.size) { view = FilterView.Priority }
                    CategoryRow("Labels", filters.labelIds.size) { view = FilterView.Labels }
                }

                FilterView.Status -> SubViewHeader("Status", onBack = { view = FilterView.Categories }) {
                    issueStatusOrder.forEach { status ->
                        FilterCheckRow(selected = status in filters.statuses, onClick = { onToggleStatus(status) }) {
                            StatusIcon(status, size = 16.dp)
                            Spacer(Modifier.width(8.dp))
                            Text(status.label)
                        }
                    }
                }

                FilterView.Priority -> SubViewHeader("Priority", onBack = { view = FilterView.Categories }) {
                    issuePriorityOrder.forEach { priority ->
                        FilterCheckRow(selected = priority in filters.priorities, onClick = { onTogglePriority(priority) }) {
                            PriorityIcon(priority, size = 16.dp)
                            Spacer(Modifier.width(8.dp))
                            Text(priority.label)
                        }
                    }
                }

                FilterView.Labels -> LabelsSubView(
                    labels = labels,
                    selectedIds = filters.labelIds,
                    onToggle = onToggleLabel,
                    onBack = { view = FilterView.Categories },
                )
            }
        }
    }
}

@Composable
private fun CategoryRow(label: String, count: Int, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        if (count > 0) {
            Text(
                count.toString(),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Spacer(Modifier.width(8.dp))
        }
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

@Composable
private fun SubViewHeader(title: String, onBack: () -> Unit, content: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
        IconButton(onClick = onBack) {
            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
        }
        Spacer(Modifier.width(4.dp))
        Text(title, style = MaterialTheme.typography.titleMedium)
    }
    Spacer(Modifier.height(4.dp))
    content()
}

@Composable
private fun LabelsSubView(
    labels: List<LabelEntity>,
    selectedIds: Set<String>,
    onToggle: (String) -> Unit,
    onBack: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    SubViewHeader("Labels", onBack = onBack) {
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text("Filter labels…") },
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null, modifier = Modifier.size(18.dp)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(4.dp))
        val filtered = labels.filter { it.name.contains(query.trim(), ignoreCase = true) }
        if (filtered.isEmpty()) {
            Text(
                if (labels.isEmpty()) "No labels yet" else "No labels match",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.padding(vertical = 12.dp),
            )
        }
        filtered.forEach { label ->
            FilterCheckRow(selected = label.id in selectedIds, onClick = { onToggle(label.id) }) {
                LabelDot(remember(label.color) { parseColor(label.color) }, size = 10.dp)
                Spacer(Modifier.width(8.dp))
                Text(label.name)
            }
        }
    }
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
