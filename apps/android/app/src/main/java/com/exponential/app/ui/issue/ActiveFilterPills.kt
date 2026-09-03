package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.IssueFilters
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.isStatusSelected
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor

// Web-parity active filter pills (apps/web/.../active-filter-pills.tsx): one
// removable capsule per active status / priority / label, shown below the tab
// bar. Each X removes just that value via the same (idempotent) toggle method.
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ActiveFilterPills(
    filters: IssueFilters,
    labels: List<LabelEntity>,
    statuses: List<ResolvedIssueStatus>,
    onToggleStatus: (ResolvedIssueStatus) -> Unit,
    onTogglePriority: (IssuePriority) -> Unit,
    onToggleLabel: (String) -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (filters.isEmpty) return
    val labelsById = remember(labels) { labels.associateBy { it.id } }
    FlowRow(
        modifier = modifier.fillMaxWidth().padding(bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // Pills read in the team's status order (REV2-85 / EXP-314) — the same
        // order the filter sheet lists them in, not the order they were ticked.
        // A stale id (its row was deleted) renders nothing and clears itself on
        // the next toggle.
        statuses.filter { filters.isStatusSelected(it) }.forEach { status ->
            FilterPill(
                label = status.name,
                onRemove = { onToggleStatus(status) },
                leading = { StatusIcon(status, size = GlassPillDefaults.SmGlyphSize) },
            )
        }
        issuePriorityOrder.filter { it in filters.priorities }.forEach { priority ->
            FilterPill(
                label = priority.label,
                onRemove = { onTogglePriority(priority) },
                leading = { PriorityIcon(priority, size = GlassPillDefaults.SmGlyphSize) },
            )
        }
        filters.labelIds.forEach { labelId ->
            val label = labelsById[labelId] ?: return@forEach
            FilterPill(
                label = label.name,
                onRemove = { onToggleLabel(labelId) },
                dot = remember(label.color) { parseColor(label.color) },
            )
        }
        GlassPill("Clear all", size = PillSize.Sm, onClick = onClear)
    }
}

/** One removable filter value: the shared small pill with a dismiss `x`. */
@Composable
private fun FilterPill(
    label: String,
    onRemove: () -> Unit,
    leading: (@Composable () -> Unit)? = null,
    dot: Color? = null,
) {
    GlassPill(
        label,
        size = PillSize.Sm,
        onClick = onRemove,
        leading = leading,
        dot = dot,
        trailing = {
            Icon(
                ExpIcons.uiClose,
                contentDescription = "Remove filter",
                modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
            )
        },
    )
}
