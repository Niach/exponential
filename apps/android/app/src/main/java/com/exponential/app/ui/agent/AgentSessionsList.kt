package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SessionTreeContext
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TreeGuidesRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.session.RunningSessionRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/** EXP-965: the Agent page scroller's row spacing, which the connector spans. */
internal val AGENT_LIST_ROW_GAP = 6.dp

/**
 * EXP-825: the caller's OWN coding sessions — the RUNNING ones (live rows,
 * EXP-312: owner-only) under the Agent page composer, moved here verbatim from
 * the Devices tab, which keeps machines only (web parity, EXP-818). A
 * `LazyListScope` extension so the page hosts the composer and the rows in ONE
 * scroller.
 *
 * EXP-923: the finished runs left this page for the top bar's history sheet
 * ([RecentRunsSheet]) — the composer is what the page is for, and a band that
 * had to be unfolded to read was neither here nor there.
 *
 * EXP-897: STATELESS — the band nests its children and the fold state lives on
 * the page, so a rebuilt list never loses it.
 *
 * EXP-1050: the nesting is the NODE tree (`sessionTree`, the EXP-996 contract)
 * — resumes collapse into one row, children keep nesting under their parent,
 * and a workflow's or a stack's runs sit under one group row ([treeContext] is
 * what tells them apart). Every group folds by the same key.
 */
internal fun LazyListScope.agentSessionsList(
    rows: List<AgentRow>,
    /** The ids (and group keys) whose children are folded away. */
    collapsedRunning: Set<String>,
    onToggleRunning: (String) -> Unit,
    steerEnabled: Boolean,
    onOpenSteer: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
    onOpenWorkflow: (String) -> Unit,
    /** What the rows alone cannot say: the team's workflows and their nodes,
     *  plus the issues the stack edges live on. */
    treeContext: SessionTreeContext = SessionTreeContext(),
) {
    item(key = "__running_header__") { SectionHeader("Running") }
    if (rows.isEmpty()) {
        item(key = "__no_running__") {
            // iOS noAgentsRow: caption/tertiary text in a glass row.
            Text(
                "No agents running right now.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier
                    .fillMaxWidth()
                    .flatRow()
                    .padding(horizontal = 12.dp, vertical = 12.dp),
            )
        }
    } else {
        // EXP-818/EXP-996: a run started by another run nests under its parent,
        // a resume succession is ONE row, and a workflow's or a stack's runs
        // hang off a group row. EXP-897: everything with children folds.
        val tree = visibleSessionTreeRows(
            sessionTree(rows.map { it.session }, treeContext),
            collapsedRunning,
        )
        val rowsBySessionId = rows.associateBy { it.session.id }
        // EXP-965: the indent alone made a child read as a shifted stranger.
        val guides = TreeGuides.compute(tree.map { it.depth })
        itemsIndexed(tree, key = { _, entry -> entry.key }) { index, entry ->
            TreeGuidesRow(
                depth = entry.depth,
                guide = guides.getOrNull(index),
                // The Agent page's scroller spaces its rows; the branch
                // bridges that gap instead of breaking at every row.
                gap = AGENT_LIST_ROW_GAP,
            ) {
                when (val node = entry.node) {
                    is SessionTreeNode.Session -> {
                        val row = rowsBySessionId[node.session.id]
                        // EXP-893: a row only OPENS the run — the Work screen
                        // it lands on merges (Changes face) and reaches the
                        // issue or the action from there; the trailing circles
                        // are gone.
                        RunningSessionRow(
                            session = node.session,
                            issue = row?.issue,
                            device = row?.device ?: SessionDevicePresentation.Unknown,
                            // EXP-876: a batch names itself after its issues.
                            batchIssues = row?.batchIssues.orEmpty(),
                            onClick = {
                                // Every listed row is the caller's own
                                // (EXP-312), so steer availability alone
                                // decides the live viewer.
                                if (steerEnabled) {
                                    onOpenSteer(node.session.id)
                                } else {
                                    // Batch multi-issue sessions carry no issue.
                                    node.session.issueId?.let(onOpenIssue)
                                }
                            },
                            expandable = entry.hasChildren,
                            expanded = entry.key !in collapsedRunning,
                            onToggle = { onToggleRunning(entry.key) },
                        )
                    }
                    // EXP-978: the workflow the runs below belong to — the row
                    // LEADS to it, which is where the graph lives.
                    is SessionTreeNode.Workflow -> SessionTreeGroupRow(
                        icon = ExpIcons.navWorkflows,
                        label = node.name,
                        count = node.children.size,
                        nodeKey = entry.key,
                        expanded = entry.key !in collapsedRunning,
                        onToggle = { onToggleRunning(entry.key) },
                        onClick = { onOpenWorkflow(node.workflowId) },
                    )
                    // EXP-897: the stack, lowest first. There is no stack
                    // screen to open — its members are the rows below.
                    is SessionTreeNode.Stack -> SessionTreeGroupRow(
                        icon = ExpIcons.prStack,
                        label = SessionTree.STACK_GROUP_LABEL,
                        count = node.children.size,
                        nodeKey = entry.key,
                        expanded = entry.key !in collapsedRunning,
                        onToggle = { onToggleRunning(entry.key) },
                    )
                }
            }
        }
    }
}
