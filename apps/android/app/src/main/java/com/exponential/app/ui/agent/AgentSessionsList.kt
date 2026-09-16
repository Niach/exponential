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
import com.exponential.app.domain.pastRunIdentifier
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

/** EXP-897: one nesting level is 14dp of indent, on every client and list. */
internal const val SESSION_TREE_INDENT_DP = 14

/**
 * EXP-825: the caller's OWN coding sessions — Running (live rows, EXP-312:
 * owner-only), then Recent (EXP-746: finished person-started runs) — under the
 * Agent page composer, moved here verbatim from the Devices tab, which keeps
 * machines only (web parity, EXP-818). A `LazyListScope` extension so the page
 * hosts the composer and the rows in ONE scroller.
 *
 * EXP-897: STATELESS — both bands nest their children (`SessionTree`) and the
 * fold state lives on the page, so a rebuilt list never loses it.
 */
internal fun LazyListScope.agentSessionsList(
    rows: List<AgentRow>,
    pastRuns: List<PastRunRow>,
    /** EXP-862: the Recent band is folded until the header is tapped. */
    pastExpanded: Boolean,
    onTogglePast: () -> Unit,
    /** The ids whose CHILD runs are folded away, per band. */
    collapsedRunning: Set<String>,
    onToggleRunning: (String) -> Unit,
    collapsedPast: Set<String>,
    onTogglePastRun: (String) -> Unit,
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
        // indented (`SessionTree`, the ×4 rule). EXP-897: a parent folds.
        val tree = SessionTree.visibleRows(
            SessionTree.nest(rows, { it.session.id }, { it.session.parentSessionId }, { it.session.startedAt }),
            collapsedRunning,
        ) { it.session.id }
        items(tree, key = { it.session.session.id }) { entry ->
            val row = entry.session
            Box(Modifier.padding(start = (entry.depth * SESSION_TREE_INDENT_DP).dp)) {
                // EXP-893: a row only OPENS the run — the Work screen it
                // lands on merges (Changes face) and reaches the issue or
                // the action from there; the trailing circles are gone.
                RunningSessionRow(
                    session = row.session,
                    issue = row.issue,
                    device = row.device,
                    // EXP-876: a batch names itself after its issues.
                    batchIssues = row.batchIssues,
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
                    expandable = entry.hasChildren,
                    expanded = row.session.id !in collapsedRunning,
                    onToggle = { onToggleRunning(row.session.id) },
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
        // EXP-897: Recent nests too — a child run that finished under its
        // parent reads as one story here as well, capped BEFORE nesting so
        // the cap keeps meaning what it meant.
        val tree = SessionTree.visibleRows(
            SessionTree.nest(
                pastRuns,
                { it.session.id },
                { it.session.parentSessionId },
                { it.session.startedAt },
            ),
            collapsedPast,
        ) { it.session.id }
        items(tree, key = { "past_${it.session.session.id}" }) { entry ->
            val row = entry.session
            Box(Modifier.padding(start = (entry.depth * SESSION_TREE_INDENT_DP).dp)) {
                EndedRunRow(
                    // The ×4 rule (domain `pastRunTitle`): the issue's title, a
                    // sync placeholder while it is missing, the action_name
                    // snapshot (a chat run's reads "Chat"), else the batch.
                    title = pastRunTitle(row.session, row.issue, row.batchIssues),
                    // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
                    identifier = pastRunIdentifier(row.session, row.issue, row.batchIssues),
                    timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                    byline = pastRunByline(
                        deviceLabel = row.device.displayLabel,
                        timeLabel = relativeTime(row.session.endedAt ?: row.session.updatedAt),
                    ),
                    onOpen = { onOpenSteer(row.session.id) },
                    expandable = entry.hasChildren,
                    expanded = row.session.id !in collapsedPast,
                    onToggle = { onTogglePastRun(row.session.id) },
                )
            }
        }
    }
}
