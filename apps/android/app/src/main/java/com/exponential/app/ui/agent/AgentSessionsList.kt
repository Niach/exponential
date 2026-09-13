package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.session.RunningSessionRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-825: the caller's OWN coding sessions — Running (live rows, EXP-312:
 * owner-only), then Past (EXP-746: finished person-started runs) — under the
 * Agent page composer, moved here verbatim from the Devices tab, which keeps
 * machines only (web parity, EXP-818). A `LazyListScope` extension so the page
 * hosts the composer and the rows in ONE scroller.
 */
internal fun LazyListScope.agentSessionsList(
    rows: List<AgentRow>,
    pastRuns: List<PastRunRow>,
    /** EXP-862: the Past band is folded until the header is tapped. */
    pastExpanded: Boolean,
    onTogglePast: () -> Unit,
    steerEnabled: Boolean,
    merging: Set<String>,
    mergeErrors: Map<String, MergeFailure>,
    actions: List<ActionDto>,
    automations: List<AutomationEntity>,
    isTeamOwner: Boolean,
    onOpenSteer: (String) -> Unit,
    onOpenIssue: (String) -> Unit,
    onEditAction: (String) -> Unit,
    onEditAutomation: (AutomationEntity) -> Unit,
    onMerge: (AgentRow) -> Unit,
    onFixConflicts: (issueId: String) -> Unit,
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
                // EXP-535: batch rows merge (and fix conflicts) through their
                // resolved PR's representative issue — the server resolves a
                // batch PR to ALL linked issues (Reviews pattern).
                val mergeIssue = row.issue ?: row.batchPrIssue
                // EXP-734: only an issue target can be handed to the "Fix
                // merge conflicts" run (its input IS an issue-linked PR); a
                // run's own PR has no issue.
                val issueMergeTarget = row.mergeTarget as? MergeTarget.Issue
                // EXP-694 (S6): the trailing control names what the run is
                // about — an issue's identifier, or the action/automation's
                // own glyph. A chat or batch run has neither, and gets no
                // button at all.
                val rowAction = row.session.actionId?.let { id -> actions.firstOrNull { it.id == id } }
                val rowAutomation = row.session.automationId?.let { id ->
                    automations.firstOrNull { it.id == id }
                }
                // An automated run edits the AUTOMATION (owner-only, like the
                // Automations tab); everything else — an unresolved
                // automation, or a member — edits the action, whose sheet is
                // read-only for members anyway. Destination AND label come
                // off this one resolution (iOS AgentsView.editTarget), so the
                // button can never announce what it doesn't open.
                val editsAutomation = rowAutomation != null && isTeamOwner
                RunningSessionRow(
                    session = row.session,
                    issue = row.issue,
                    device = row.device,
                    mergeTarget = row.mergeTarget,
                    merging = row.mergeTarget?.key in merging,
                    failure = row.mergeTarget?.key?.let(mergeErrors::get),
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
                    // The pill shows only once the issue itself has synced —
                    // it IS the identifier.
                    issueIdentifier = row.issue?.identifier,
                    actionIcon = row.session.actionId?.let { actionGlyph(rowAction) },
                    actionLabel = if (editsAutomation) "Edit automation" else "Edit action",
                    onOpenIssue = { row.session.issueId?.let(onOpenIssue) },
                    onOpenAction = {
                        if (editsAutomation) {
                            rowAutomation?.let(onEditAutomation)
                        } else {
                            row.session.actionId?.let(onEditAction)
                        }
                    },
                    onMerge = { onMerge(row) },
                    // A REAL conflict only (EXP-533); the recovery run rebases
                    // the PR's branch, so it needs one recorded — the same gate
                    // as the Reviews rows (EXP-323), plus a reachable machine.
                    canFixConflicts = issueMergeTarget != null &&
                        canOfferFixConflicts(
                            mergeErrors[issueMergeTarget.key],
                            mergeIssue?.branch,
                            steerEnabled = steerEnabled,
                        ),
                    onFixConflicts = { issueMergeTarget?.issueId?.let(onFixConflicts) },
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
    // EXP-862: FOLDED by default — the band names the count and expands in
    // place, so a long history never pushes the composer off the page.
    if (pastRuns.isNotEmpty()) {
        item(key = "__past_header__") {
            SectionHeader(
                "Past",
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
                trailing = {
                    Text(
                        pastRuns.size.toString(),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
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
