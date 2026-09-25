package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.components.FoldChevron
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.LiveDot
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-874: ONE live coding-session row — state dot + identity line, the
 * device-written caption, the state/device byline and the usage wall. Shared
 * by the Agent page's Running band and the Automations tab's live automated
 * runs, so the two can't drift (the reference layout web/desktop/iOS copy).
 * EXP-893: the trailing merge / fix-conflicts and open-issue / open-action
 * circles are gone — a row only OPENS the run (the Work screen), where the
 * Changes face merges and the Issue face is a switcher tap away.
 */
@Composable
internal fun RunningSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    // EXP-549/550: the host machine resolved against its live devices row —
    // the current label, and offline = the run is paused until it returns.
    device: SessionDevicePresentation,
    onClick: () -> Unit,
    // EXP-876: the issues a BATCH row names itself after (`EXP-874 +2`).
    // Empty on every other subject, and on a batch whose issues are unknown —
    // that row reads "Batch run" as it always did.
    batchIssues: List<IssueEntity> = emptyList(),
    // EXP-897: this row has children nested under it (the session TREE) — a
    // 12dp fold chevron leads the row, on Running and Recent alike (×4).
    expandable: Boolean = false,
    expanded: Boolean = true,
    onToggle: () -> Unit = {},
    // EXP-1068: a workflow REVIEW row's own title (`Review r2 · approved`);
    // null = the ordinary subject title.
    titleOverride: String? = null,
    // EXP-1068: glyphs drawn right after the state dot (the "needs you" red
    // dot, the duplicate-live warning).
    dotAccessory: (@Composable RowScope.() -> Unit)? = null,
    // EXP-1068: the run's account when it is not the machine's default for
    // its agent — the byline then ends `· account <label>`.
    accountLabel: String? = null,
) {
    // EXP-734: an issueless run carries its own PR state, so "in review with
    // a merged PR" reads as Done there too.
    val state = codingSessionDisplayState(session, issue?.prState ?: session.prState)
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
            if (expandable) {
                FoldChevron(expanded = expanded, onToggle = onToggle)
                Spacer(Modifier.width(4.dp))
            }
            Column(modifier = Modifier.weight(1f)) {
                // EXP-688: the identity line is shared with the steering
                // screen's header (SessionRowTitle) so the two can't drift.
                SessionRowTitle(
                    identifier = sessionRowIdentifier(issue, session, batchIssues),
                    title = titleOverride ?: sessionRowTitle(session, issue, batchIssues),
                    dot = { Row(verticalAlignment = Alignment.CenterVertically) {
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
                        dotAccessory?.invoke(this)
                    } },
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
                    } + (accountLabel?.let { " · account $it" } ?: ""),
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
        }
    }
}
