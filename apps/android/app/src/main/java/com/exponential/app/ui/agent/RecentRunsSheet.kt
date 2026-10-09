package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.SessionTreeNode
import com.exponential.app.domain.TreeGuides
import com.exponential.app.domain.sessionTree
import com.exponential.app.domain.visibleSessionTreeRows
import com.exponential.app.ui.components.CodingSessionRow
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SessionRowSize
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-923: the caller's finished person-started runs (EXP-746), behind the
 * Agent page's history button instead of under its composer.
 *
 * EXP-1061: the rows are the `sessionTree` SELECTOR drawn, exactly like the
 * Running band (and web's Recent panel): a resume succession is ONE row, a
 * child nests under its parent, top level newest ACTIVITY first.
 * EXP-1248: the BIG [CodingSessionRow] ×4 — `Done · <device> · <when>`, the
 * device glyph trailing, children always shown, the band gapless.
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
        val bands = remember(pastRuns, teams) {
            teamBands(pastRuns, teams) { it.session.teamId }.map { band ->
                val tree = visibleSessionTreeRows(sessionTree(band.items.map { it.session }))
                Triple(band.team, tree, TreeGuides.compute(tree.map { it.depth }))
            }
        }
        LazyColumn(
            modifier = Modifier.fillMaxWidth().testTag("recent-runs-sheet"),
            contentPadding = PaddingValues(horizontal = 16.dp),
        ) {
            bands.forEach { (team, tree, guides) ->
                team?.let {
                    item(key = "__recent_team_${it.id}__") { TeamSectionHeader(it) }
                }
                // ONE item per band, so the rows stay gapless under the band.
                item(key = "__recent_rows_${team?.id.orEmpty()}__") {
                    Column(modifier = Modifier.fillMaxWidth()) {
                        tree.forEachIndexed { index, entry ->
                            when (val node = entry.node) {
                                is SessionTreeNode.Session -> {
                                    val row = rowsBySessionId[node.session.id] ?: return@forEachIndexed
                                    CodingSessionRow(
                                        session = row.session,
                                        issue = row.issue,
                                        device = row.device,
                                        size = SessionRowSize.Big,
                                        batchIssues = row.batchIssues,
                                        depth = entry.depth,
                                        guide = guides.getOrNull(index),
                                        onClick = {
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
}
