package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.exponential.app.data.db.WorkflowEventEntity

/**
 * EXP-1082 STUB: a workflow's event log (the `workflow_events` shape, newest
 * first via `WorkflowEventDao.observeByWorkflow`). Renders an empty column
 * for now; EXP-1068 fills the event list.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
fun WorkflowEventList(events: List<WorkflowEventEntity>, modifier: Modifier = Modifier) {
    Column(modifier) {}
}
