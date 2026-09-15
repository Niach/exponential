package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.session.RunningSessionRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-825: the caller's OWN coding sessions — Running (live rows, EXP-312:
 * owner-only), then Recent (EXP-746: finished person-started runs) — under the
 * Agent page composer, moved here verbatim from the Devices tab, which keeps
 * machines only (web parity, EXP-818). A `LazyListScope` extension so the page
 * hosts the composer and the rows in ONE scroller.
 */
internal fun LazyListScope.agentSessionsList(
    rows: List<AgentRow>,
    pastRuns: List<PastRunRow>,
    /** EXP-862: the Recent band is folded until the header is tapped. */
    pastExpanded: Boolean,
    onTogglePast: () -> Unit,
    steerEnabled: Boolean,
    onOpenSteer: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
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
        // EXP-818: a run started by another run nests under its parent,
        // indented (`SessionTree`, the ×4 rule).
        val tree = SessionTree.nest(
            rows,
            { it.session.id },
            { it.session.parentSessionId },
            { it.session.startedAt },
        )
        items(tree, key = { it.session.session.id }) { entry ->
            val row = entry.session
            Box(Modifier.padding(start = (entry.depth * 16).dp)) {
                // EXP-893: a row only OPENS the run — the Work screen it
                // lands on merges (Changes face) and reaches the issue or
                // the action from there; the trailing circles are gone.
                RunningSessionRow(
                    session = row.session,
                    issue = row.issue,
                    device = row.device,
                    onClick = {
                        // Every listed row is the caller's own (EXP-312), so
                        // steer availability alone decides the live viewer.
                        if (steerEnabled) {
                            onOpenSteer(row.session.id)
                        } else {
                            // Batch multi-issue sessions carry no issue.
                            row.session.issueId?.let(onOpenIssue)
                        }
                    },
                )
            }
        }
    }

    // EXP-746: the runs that finished — the caller's own person-started
    // ones. EXP-773: a plain link; the transcript and Resume live in the
    // session view it opens. An automated run belongs to the Automations tab's
    // "Recent automated runs" and never lists here. Nothing renders while
    // there are none.
    //
    // EXP-862: FOLDED by default — the band expands in place, so a long
    // history never pushes the composer off the page. EXP-886: renamed
    // "Recent" and shows no count (the cap made the number meaningless).
    if (pastRuns.isNotEmpty()) {
        item(key = "__past_header__") {
            SectionHeader(
                "Recent",
                modifier = Modifier
                    .clickable(onClick = onTogglePast)
                    .testTag("past-runs-header"),
                leading = {
                    Icon(
                        if (pastExpanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
            )
        }
    }
    if (pastRuns.isNotEmpty() && pastExpanded) {
        items(pastRuns, key = { "past_${it.session.id}" }) { row ->
            EndedRunRow(
                // The ×4 rule (domain `pastRunTitle`): the issue's title, a
                // sync placeholder while it is missing, the action_name
                // snapshot (a chat run's reads "Chat"), else the batch.
                title = pastRunTitle(row.session, row.issue),
                identifier = row.issue?.identifier,
                timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                byline = pastRunByline(
                    deviceLabel = row.device.displayLabel,
                    timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                ),
                onOpen = { onOpenSteer(row.session.id) },
            )
        }
    }
}
