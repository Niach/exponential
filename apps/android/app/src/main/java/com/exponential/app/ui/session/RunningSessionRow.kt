package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.MergeFailure
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassCard

/**
 * EXP-874: ONE live coding-session row — state dot + identity line, the
 * device-written caption, the state/device byline, the usage wall, the merge /
 * fix-conflicts shortcut and the trailing issue-or-action circle. Shared by the
 * Agent page's Running band and the Automations tab's live automated runs, so
 * the two can't drift (the reference layout web/desktop/iOS copy).
 */
@Composable
internal fun RunningSessionRow(
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
                // Only a live row (web/desktop rule): a merged run never shows
                // what it "is doing", whatever the column still says.
                session.agentCaption?.takeIf { it.isNotBlank() && state != CodingSessionDisplayState.Done }?.let { caption ->
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
