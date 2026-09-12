package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.pastRunByline
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EndedRunRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.session.LostGray
import com.exponential.app.ui.session.PastRunRow
import com.exponential.app.ui.session.SessionRowTitle
import com.exponential.app.ui.session.rememberUsageClock
import com.exponential.app.ui.session.sessionRowIdentifier
import com.exponential.app.ui.session.sessionRowTitle
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
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
                AgentSessionRow(
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
    // ones. EXP-773: a plain link; the transcript, the close-out summary and
    // Resume live in the session view it opens. An automated run belongs to
    // the Automations tab's "Recent automated runs" and never lists here.
    // Nothing renders while there are none.
    if (pastRuns.isNotEmpty()) {
        item(key = "__past_header__") { SectionHeader("Past") }
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

@Composable
private fun AgentSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    // EXP-549/550: the host machine resolved against its live devices row —
    // the current label, and offline = the run is paused until it returns.
    device: SessionDevicePresentation,
    // EXP-734: what the merge shortcut acts on — the linked issue, a batch
    // row's client-resolved representative (EXP-535), or, for an action/chat
    // run that opened a PR of its own, the SESSION. Null = nothing to merge.
    mergeTarget: MergeTarget?,
    merging: Boolean,
    failure: MergeFailure?,
    onClick: () -> Unit,
    // EXP-694 (S6): the trailing control — an issue run wears the circle that
    // opens its issue, an action / automation run the circle with that
    // action's glyph; a chat or batch run gets neither.
    issueIdentifier: String?,
    actionIcon: ImageVector?,
    actionLabel: String,
    onOpenIssue: () -> Unit,
    onOpenAction: () -> Unit,
    onMerge: () -> Unit,
    canFixConflicts: Boolean,
    onFixConflicts: () -> Unit,
) {
    // EXP-734: an issueless run carries its own PR state, so "in review with
    // a merged PR" reads as Done there too.
    val state = codingSessionDisplayState(session, issue?.prState ?: session.prState)
    val canMerge = mergeTarget != null
    // EXP-550: the machine went away (lid closed) — the run is not lost and
    // not ended, it continues when the machine comes back. Grey, never a live
    // dot.
    val paused = device.isPaused(state)
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("agent-session-row")
                .flatRow()
                .clickable(onClick = onClick)
                .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                // EXP-688: the identity line is shared with the steering
                // screen's header (SessionRowTitle) so the two can't drift.
                SessionRowTitle(
                    identifier = sessionRowIdentifier(issue),
                    title = sessionRowTitle(session, issue),
                    dot = {
                        when {
                            paused -> StaticDot(LostGray)
                            else -> when (state) {
                                // EXP-848: the pulse means MID-TURN, off the
                                // synced agent_busy flag — a live run between
                                // turns is steady, not forever "working".
                                CodingSessionDisplayState.Running ->
                                    LiveDot(busy = session.agentBusy)
                                CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber)
                                CodingSessionDisplayState.Review -> StaticDot(ReviewGreen)
                                CodingSessionDisplayState.Done -> StaticDot(DoneBlue)
                            }
                        }
                    },
                )
                // EXP-850 (S8): what the run is DOING right now, written by
                // the device (today the running workflow's caption) — the
                // SECOND line, above the device byline. The server clears it
                // on every end path, so an ended row never keeps a stale one.
                session.agentCaption?.takeIf { it.isNotBlank() }?.let { caption ->
                    Text(
                        caption,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier
                            .padding(start = 20.dp)
                            .testTag("session-agent-caption"),
                    )
                }
                // EXP-549: the LIVE machine label, so a rename lands here
                // instead of the row keeping the original hostname forever.
                val deviceName = device.displayLabel
                Text(
                    when {
                        paused -> "Paused · $deviceName"
                        else -> when (state) {
                            CodingSessionDisplayState.NeedsInput -> "Needs input · $deviceName"
                            CodingSessionDisplayState.Review -> "Ready for review · $deviceName"
                            CodingSessionDisplayState.Done -> "Done · $deviceName"
                            CodingSessionDisplayState.Running ->
                                "$deviceName · started ${relativeTime(session.startedAt)}"
                        }
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = when {
                        paused -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        else -> when (state) {
                            CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
                            CodingSessionDisplayState.Review -> ReviewGreen
                            CodingSessionDisplayState.Done -> DoneBlue
                            CodingSessionDisplayState.Running ->
                                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                        }
                    },
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    // Aligned under the identifier: the dot (8dp) plus its
                    // 12dp gap live inside the identity line above.
                    modifier = Modifier.padding(start = 20.dp),
                )
                // EXP-804: the agent's usage wall, on its OWN line under the
                // state — a walled run is still `running` and both facts have
                // to survive. A run that is not blocked draws nothing.
                val blockedLabel = AgentUsagePresentation.blockedBadgeLabel(
                    AgentUsagePresentation.parseBlocked(session.blocked),
                    rememberUsageClock(),
                )
                if (blockedLabel != null) {
                    Text(
                        blockedLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = NeedsInputAmber,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(start = 20.dp),
                    )
                }
            }
            // EXP-498: merging always closes the session too — confirm-gated,
            // and only while the PR is actually open. EXP-706: a
            // conflict-refused merge REPLACES this control with the recovery
            // run instead of stacking a second button under the message.
            if (canMerge) {
                if (merging) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                } else {
                    CircleIconButton(
                        if (canFixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                        contentDescription = if (canFixConflicts) "Fix conflicts" else "Merge",
                        onClick = if (canFixConflicts) onFixConflicts else onMerge,
                    )
                }
            }
            when {
                // EXP-698: the identity line already prints the identifier,
                // so the trailing control is the 32dp circle in BOTH cases:
                // open the issue, or open the action.
                issueIdentifier != null -> CircleIconButton(
                    ExpIcons.uiIssue,
                    contentDescription = "Open $issueIdentifier",
                    onClick = onOpenIssue,
                    modifier = Modifier.padding(start = 8.dp),
                )
                actionIcon != null -> CircleIconButton(
                    actionIcon,
                    contentDescription = actionLabel,
                    onClick = onOpenAction,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
        }

        // A refused merge (conflicts, branch protection, GitHub App errors)
        // captions THIS row (EXP-323 pattern) — inside the list, so the reason
        // is always readable. EXP-706: the message ONLY; the recovery run took
        // the merge control's place in the row above.
        if (failure != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 3.dp)
                    .glassCard()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    failure.message,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}
