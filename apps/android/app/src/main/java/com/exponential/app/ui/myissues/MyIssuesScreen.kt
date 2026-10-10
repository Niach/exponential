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
import androidx.compose.foundation.lazy.itemsIndexed
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
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.IssueGraphSheet
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.IssueGroupBand
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.LongPressIssueRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * "My issues" (masterplan §5a): a cross-board list of everything assigned
 * to me on the active account, grouped by status. A fixed built-in view — no
 * filters, no saved views. Lives on as the My Issues segment of the "My Work"
 * tab (PersonalScreen, EXP-58); embedded there rather than routed to.
 */
@Composable
fun MyIssuesListContent(
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: MyIssuesViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var collapsed by remember { mutableStateOf(emptySet<String>()) }
    // EXP-980: the row whose blocks badge opened the mini-graph (null = none).
    var graphIssueId by remember { mutableStateOf<String?>(null) }

    graphIssueId?.let { subjectId ->
        val graphIssues by viewModel.allIssues.collectAsStateWithLifecycle()
        val graphRelations by viewModel.relations.collectAsStateWithLifecycle()
        IssueGraphSheet(
            subjectIds = listOf(subjectId),
            issues = graphIssues,
            relations = graphRelations,
            onOpenIssue = onOpenIssue,
            onDismiss = { graphIssueId = null },
        )
    }

    when {
        !state.loaded -> LoadingState(modifier = modifier)
        state.groups.isEmpty() -> EmptyState(
            message = "Nothing assigned to you",
            icon = ExpIcons.uiAssignee,
            modifier = modifier,
        )
        else -> LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            state.groups.forEach { group ->
                val isCollapsed = group.key in collapsed
                item(key = "header-${group.key}") {
                    IssueGroupBand(
                        glyph = { StatusIcon(group.status, size = 14.dp) },
                        name = group.status.name,
                        count = group.issues.size,
                        collapsed = isCollapsed,
                        onToggle = {
                            collapsed =
                                if (isCollapsed) collapsed - group.key else collapsed + group.key
                        },
                    )
                }
                if (!isCollapsed) {
                    // EXP-980/965: sub-issues hang off their parent with the
                    // shared elbow connector.
                    val guides = TreeGuides.compute(group.issues.map { it.depth })
                    itemsIndexed(group.issues, key = { _, it -> it.issue.id }) { index, entry ->
                        TreeGuidesRow(
                            depth = entry.depth,
                            guide = guides.getOrNull(index),
                            gap = RowGap,
                        ) {
                            // Rows span boards — the identifier's board
                            // prefix ({PREFIX}-{n}) disambiguates; the assignee
                            // avatar is omitted (it's always me).
                            LongPressIssueRow(
                                issue = entry.issue,
                                labels = entry.labels,
                                assignee = null,
                                canMutate = true,
                                blocks = entry.blocks,
                                onBlocksClick = { graphIssueId = entry.issue.id },
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
}

/** The list's own row spacing — what a nesting connector has to bridge. */
private val RowGap = 3.dp
