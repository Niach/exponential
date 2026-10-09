package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.components.CodingSessionRow
import com.exponential.app.ui.components.SessionRowSize
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-825: the caller's OWN coding sessions — the RUNNING ones (live rows,
 * EXP-312: owner-only) under the Agent page composer (web parity, EXP-818). A
 * `LazyListScope` extension so the page hosts the composer and the rows in ONE
 * scroller.
 *
 * EXP-923: the finished runs live behind the top bar's history sheet
 * ([RecentRunsSheet]).
 *
 * EXP-1050: the nesting is the NODE tree (`sessionTree`, the EXP-996 contract)
 * — resumes collapse into one row and children nest under their parent.
 * EXP-1248: the rows are the BIG [CodingSessionRow] ×4: children ALWAYS show
 * (no fold, trees cap at 10 runs) and the list is gapless, so the connector
 * runs unbroken from a parent's mark to its children's.
 */
internal fun LazyListScope.agentSessionsList(
    rows: List<AgentRow>,
    steerEnabled: Boolean,
    onOpenSteer: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
    /**
     * EXP-1186: the member teams — the list is CROSS-TEAM, and with more than
     * one team each team's runs sit under its own band (avatar + name)
     * instead of the one "Running" band.
     */
    teams: List<TeamEntity> = emptyList(),
) {
    val bands = teamBands(rows, teams) { it.session.teamId }
    if (rows.isEmpty() || bands.all { it.team == null }) {
        item(key = "__running_header__") { SectionHeader("Running") }
    }
    if (rows.isEmpty()) {
        item(key = "__no_running__") {
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
    } else bands.forEach { band ->
        band.team?.let { team ->
            item(key = "__running_team_${team.id}__") { TeamSectionHeader(team) }
        }
        val tree = visibleSessionTreeRows(sessionTree(band.items.map { it.session }))
        val rowsBySessionId = band.items.associateBy { it.session.id }
        val guides = TreeGuides.compute(tree.map { it.depth })
        // ONE item per band: the page scroller spaces its items, and a gap
        // between rows would break the connector into a dashed ladder.
        item(key = "__running_rows_${band.team?.id.orEmpty()}__") {
            Column(modifier = Modifier.fillMaxWidth().testTag("agent-session-rows")) {
                tree.forEachIndexed { index, entry ->
                    when (val node = entry.node) {
                        is SessionTreeNode.Session -> {
                            val row = rowsBySessionId[node.session.id]
                            // EXP-893: a row only OPENS the run.
                            CodingSessionRow(
                                session = node.session,
                                issue = row?.issue,
                                device = row?.device ?: SessionDevicePresentation.Unknown,
                                size = SessionRowSize.Big,
                                // EXP-876: a batch names itself after its issues.
                                batchIssues = row?.batchIssues.orEmpty(),
                                depth = entry.depth,
                                guide = guides.getOrNull(index),
                                onClick = {
                                    // Every listed row is the caller's own
                                    // (EXP-312), so steer availability alone
                                    // decides the viewer.
                                    if (steerEnabled) {
                                        onOpenSteer(node.session.id)
                                    } else {
                                        node.session.issueId?.let(onOpenIssue)
                                    }
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}
