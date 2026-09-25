package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueGraph

/**
 * EXP-1082 STUB: THE mini-graph as a popover — one drawing for the blocks
 * badge/rail and the workflow detail. Takes the same inputs [IssueGraphList]
 * draws from today and renders nothing yet. EXP-1057 fills it by COPYING
 * (not moving) the drawing out of `IssueGraphList.kt` /
 * `ui/workflows/WorkflowGraphList.kt`; the old files keep working meanwhile.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun IssueGraphPopover(
    graph: IssueGraph.Graph,
    issuesById: Map<String, IssueEntity>,
    onOpenIssue: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier)
}
