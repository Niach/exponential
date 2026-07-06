package com.exponential.app.ui.myissues

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.issue.LongPressIssueRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * "Assigned to you" (the old My Issues tab, masterplan §5a): a cross-project
 * list of everything assigned to me on the active account, grouped by status.
 * A fixed built-in view — no filters, no saved views. Lives on as the Search
 * tab's empty-query state; embedded there rather than routed to.
 */
@Composable
fun MyIssuesListContent(
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: MyIssuesViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var collapsed by remember { mutableStateOf(emptySet<IssueStatus>()) }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.groups.isEmpty() -> EmptyState(
            message = "Nothing assigned to you",
            icon = Icons.Filled.Person,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 96.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            state.groups.forEach { group ->
                val isCollapsed = group.status in collapsed
                item(key = "header-${group.status.wire}") {
                    GroupHeader(
                        status = group.status,
                        count = group.issues.size,
                        collapsed = isCollapsed,
                        onToggle = {
                            collapsed =
                                if (isCollapsed) collapsed - group.status else collapsed + group.status
                        },
                    )
                }
                if (!isCollapsed) {
                    items(group.issues, key = { it.issue.id }) { entry ->
                        // Rows span projects — the identifier's project
                        // prefix ({PREFIX}-{n}) disambiguates; the assignee
                        // avatar is omitted (it's always me).
                        LongPressIssueRow(
                            issue = entry.issue,
                            labels = entry.labels,
                            assignee = null,
                            canMutate = true,
                            onMarkDone = {
                                viewModel.updateIssueStatus(entry.issue.id, IssueStatus.Done)
                            },
                            onMoveToBacklog = {
                                viewModel.updateIssueStatus(entry.issue.id, IssueStatus.Backlog)
                            },
                            onClick = { onOpenIssue(entry.issue.id) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun GroupHeader(
    status: IssueStatus,
    count: Int,
    collapsed: Boolean,
    onToggle: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (collapsed) Icons.AutoMirrored.Filled.KeyboardArrowRight else Icons.Filled.KeyboardArrowDown,
            contentDescription = if (collapsed) "Expand" else "Collapse",
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Spacer(Modifier.width(6.dp))
        StatusIcon(status, size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            status.label,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.width(8.dp))
        Text(
            count.toString(),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
