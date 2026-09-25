package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTreeContext
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunIdentifier
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-923: the caller's finished person-started runs (EXP-746), behind the
 * Agent page's history button instead of under its composer. History is
 * something the reader goes LOOKING for, so it gets a surface of its own.
 *
 * EXP-1061: the rows are the `sessionTree` SELECTOR drawn, exactly like the
 * Running band (and web's Recent panel): a resume succession is ONE row, a
 * child nests under its parent, a workflow's or a stack's runs sit under one
 * group row, every parent folds, top level newest ACTIVITY first (rule 5).
 *
 * An automated run belongs to the Automations tab's "Recent automated runs"
 * and never lists here.
 */
@Composable
fun RecentRunsSheet(
    pastRuns: List<PastRunRow>,
    onOpenRun: (String) -> Unit,
    onOpenWorkflow: (String) -> Unit,
    onDismiss: () -> Unit,
    /** The team's workflows and nodes (the Running band's context); the stack
     *  edges are swapped for the PAST rows' own issues here. */
    treeContext: SessionTreeContext = SessionTreeContext(),
) {
    var collapsed by remember { mutableStateOf(emptySet<String>()) }
    GlassSheet(title = "Recent", onDismiss = onDismiss) {
        if (pastRuns.isEmpty()) {
            Text(
                "No recent runs",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
            return@GlassSheet
        }
        // Capped BEFORE the tree (`PastRuns` cap), so a child whose parent fell
        // off the cap is a top-level orphan (rule 6).
        val context = remember(pastRuns, treeContext) {
            treeContext.copy(issues = pastRunIssues(pastRuns))
        }
        val rowsBySessionId = remember(pastRuns) { pastRuns.associateBy { it.session.id } }
        val tree = remember(pastRuns, context, collapsed) {
            visibleSessionTreeRows(sessionTree(pastRuns.map { it.session }, context), collapsed)
        }
        val guides = remember(tree) { TreeGuides.compute(tree.map { it.depth }) }
        val toggle = { key: String -> collapsed = if (key in collapsed) collapsed - key else collapsed + key }
        LazyColumn(
            modifier = Modifier.fillMaxWidth().testTag("recent-runs-sheet"),
            contentPadding = PaddingValues(horizontal = 16.dp),
            // EXP-818: the 6dp every converted list uses.
            verticalArrangement = Arrangement.spacedBy(AGENT_LIST_ROW_GAP),
        ) {
            itemsIndexed(tree, key = { _, entry -> entry.key }) { index, entry ->
                val expanded = entry.key !in collapsed
                TreeGuidesRow(
                    depth = entry.depth,
                    guide = guides.getOrNull(index),
                    gap = AGENT_LIST_ROW_GAP,
                ) {
                    when (val node = entry.node) {
                        is SessionTreeNode.Session -> {
                            val row = rowsBySessionId[node.session.id] ?: return@TreeGuidesRow
                            val timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt)
                            EndedRunRow(
                                // The ×4 rule (domain `pastRunTitle`): the
                                // issue's title, a sync placeholder while it is
                                // missing, the action_name snapshot (a chat
                                // run's reads "Chat"), else the batch.
                                title = pastRunTitle(row.session, row.issue, row.batchIssues),
                                // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
                                identifier = pastRunIdentifier(row.session, row.issue, row.batchIssues),
                                timeLabel = timeLabel,
                                byline = pastRunByline(
                                    deviceLabel = row.device.displayLabel,
                                    timeLabel = timeLabel,
                                ),
                                expandable = entry.hasChildren,
                                expanded = expanded,
                                onToggle = { toggle(entry.key) },
                                onOpen = {
                                    onDismiss()
                                    onOpenRun(row.session.id)
                                },
                            )
                        }
                        is SessionTreeNode.Workflow -> SessionTreeGroupRow(
                            icon = ExpIcons.navWorkflows,
                            label = node.name,
                            count = node.children.size,
                            nodeKey = entry.key,
                            expanded = expanded,
                            onToggle = { toggle(entry.key) },
                            onClick = {
                                onDismiss()
                                onOpenWorkflow(node.workflowId)
                            },
                        )
                        // A stack has no screen: its members are the rows below.
                        is SessionTreeNode.Stack -> SessionTreeGroupRow(
                            icon = ExpIcons.prStack,
                            label = SessionTree.STACK_GROUP_LABEL,
                            count = node.children.size,
                            nodeKey = entry.key,
                            expanded = expanded,
                            onToggle = { toggle(entry.key) },
                        )
                    }
                }
            }
        }
    }
}

/** The issues the past rows name (an issue run's own, a batch's covered set):
 *  the stack edges, id-ordered like the Running band's. */
private fun pastRunIssues(rows: List<PastRunRow>): List<IssueEntity> {
    val byId = LinkedHashMap<String, IssueEntity>()
    for (row in rows) {
        row.issue?.let { byId[it.id] = it }
        for (issue in row.batchIssues) byId[issue.id] = issue
    }
    return byId.values.toList()
}
