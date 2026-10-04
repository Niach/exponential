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
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunIdentifier
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.TreeGuidesRow
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
 * child nests under its parent, every parent folds, top level newest
 * ACTIVITY first.
 *
 * A triggered run belongs to its action page's Runs and never lists here.
 */
@Composable
fun RecentRunsSheet(
    pastRuns: List<PastRunRow>,
    onOpenRun: (String) -> Unit,
    onDismiss: () -> Unit,
    /** EXP-1186: cross-team — one band per team once there are several. */
    teams: List<TeamEntity> = emptyList(),
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
        // off the cap is a top-level orphan.
        val rowsBySessionId = remember(pastRuns) { pastRuns.associateBy { it.session.id } }
        val bands = remember(pastRuns, teams, collapsed) {
            teamBands(pastRuns, teams) { it.session.teamId }.map { band ->
                val tree = visibleSessionTreeRows(sessionTree(band.items.map { it.session }), collapsed)
                Triple(band.team, tree, TreeGuides.compute(tree.map { it.depth }))
            }
        }
        val toggle = { key: String -> collapsed = if (key in collapsed) collapsed - key else collapsed + key }
        LazyColumn(
            modifier = Modifier.fillMaxWidth().testTag("recent-runs-sheet"),
            contentPadding = PaddingValues(horizontal = 16.dp),
            // EXP-818: the 6dp every converted list uses.
            verticalArrangement = Arrangement.spacedBy(AGENT_LIST_ROW_GAP),
        ) {
            bands.forEach { (team, tree, guides) ->
                team?.let {
                    item(key = "__recent_team_${it.id}__") { TeamSectionHeader(it) }
                }
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
                        }
                    }
                }
            }
        }
    }
}
