package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.ListItem
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.codingSessionIsWorking
import com.exponential.app.domain.pastRunIdentifier
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.domain.runHasEnded
import com.exponential.app.domain.sessionRowCaption
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.session.rememberUsageClock
import com.exponential.app.domain.SessionRowCaption
import com.exponential.app.domain.SessionStatusTone
import com.exponential.app.domain.TreeGuide
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/** EXP-1248: a session row's two sizes (`list-item.json` small 32 / big 52). */
enum class SessionRowSize { Small, Big }

/**
 * EXP-1248: THE session row, x4 (web `@exp/ui` SessionRow, desktop
 * `run_rows::run_row`, iOS `SessionRow`; fixture `list-item.json`). One
 * anatomy in two sizes: [tree guides][run mark at 12 + 14·depth][mono id ·
 * title][caption, big only][device glyph]. No fold chevron, no trailing
 * chevron, no buttons: any inline control before the text pushes a parent's
 * mark or title off its child's, which is the bug this row exists to end.
 * Gapless lists: the connector has nothing to bridge.
 */
@Composable
fun SessionRow(
    size: SessionRowSize,
    /** The run's `coding_sessions.agent`: whose brand mark leads. */
    agent: String?,
    /** The live mark's state ([runningRowMarkState]); null = the bare mark. */
    markState: CodingSessionDisplayState?,
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** A FINISHED run: the dimmed brand mark, no badge. */
    ended: Boolean = false,
    /** An issue run's identifier or a batch's `EXP-874 +2`; null otherwise. */
    identifier: String? = null,
    /** The big row's second line ([com.exponential.app.domain.sessionRowCaption]); ignored when small. */
    caption: SessionRowCaption? = null,
    /** Nesting depth under a parent run: 14dp per level. */
    depth: Int = 0,
    /** This row's connector ([com.exponential.app.domain.TreeGuides.compute]). */
    guide: TreeGuide? = null,
    /** The host device's glyph ([deviceIcon]); null draws none. */
    deviceIcon: ImageVector? = null,
    /** The host device's name: the glyph's description. */
    deviceName: String? = null,
    active: Boolean = false,
    /** An offline host: the row dims. */
    dimmed: Boolean = false,
    testTag: String = SESSION_ROW_TAG,
) {
    val big = size == SessionRowSize.Big
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height((if (big) ListItem.BIG_DP else ListItem.SMALL_DP).dp)
            .testTag(testTag)
            // Drawn before the clip, in the gutters the indent leaves.
            .treeGuides(guide)
            .flatRow(active)
            .clickable(onClick = onClick)
            .then(if (dimmed) Modifier.alpha(DIMMED_ALPHA) else Modifier)
            .padding(start = ListItem.leadX(depth).dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ListItem.GAP_DP.dp),
    ) {
        AgentRunMark(
            agent = agent,
            state = markState,
            size = SessionRowMarkSize,
            badgeSize = SessionRowBadgeSize,
            ended = ended,
        )
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (!identifier.isNullOrEmpty()) {
                    Text(
                        identifier,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    title,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (big && caption != null && caption.text.isNotEmpty()) {
                Text(
                    caption.text,
                    style = MaterialTheme.typography.bodySmall,
                    color = sessionRowToneColor(caption.tone),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.testTag("session-row-caption"),
                )
            }
        }
        if (deviceIcon != null) {
            Icon(
                deviceIcon,
                contentDescription = deviceName,
                modifier = Modifier.size(14.dp).testTag("session-row-device"),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/**
 * EXP-1248: a synced run drawn as a [SessionRow] — every session list's ONE
 * binding (the Agent page's Running band, its Recent sheet, an action's
 * Runs), live and ended alike. The mark, the caption and the dim follow the
 * x4 rules: [runHasEnded] → the dimmed mark + `Done · <device> · <when>`; an
 * offline host → the bare mark, `Paused · <device>`, the row dimmed; a usage
 * wall → its badge label in amber; else the live display state.
 */
@Composable
fun CodingSessionRow(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    device: SessionDevicePresentation,
    size: SessionRowSize,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** EXP-876: the issues a BATCH row names itself after (`EXP-874 +2`). */
    batchIssues: List<IssueEntity> = emptyList(),
    /** A caller's own title (an action's Runs: what started it). */
    titleOverride: String? = null,
    depth: Int = 0,
    guide: TreeGuide? = null,
    /** The host's glyph; the device kind default when the caller knows no pick. */
    deviceIcon: ImageVector? = ExpIcons.uiDevice,
) {
    val nowMs = rememberUsageClock()
    val ended = runHasEnded(session)
    // EXP-734: an issueless run carries its own PR state.
    val state = codingSessionDisplayState(session, issue?.prState ?: session.prState)
    val paused = !ended && device.isPaused(state, session.status)
    val working = !paused && codingSessionIsWorking(session.status, state)
    val blockedLabel = if (ended) {
        null
    } else {
        AgentUsagePresentation.blockedBadgeLabel(AgentUsagePresentation.parseBlocked(session.blocked), nowMs)
    }
    val caption = sessionRowCaption(
        ended = ended,
        paused = paused,
        state = state,
        device = device.displayLabel,
        startedAt = session.startedAt,
        updatedAt = session.updatedAt,
        endedAt = session.endedAt,
        blockedLabel = blockedLabel,
        nowMs = nowMs,
    )
    SessionRow(
        size = size,
        agent = session.agent,
        markState = if (ended) null else runningRowMarkState(state, paused, working),
        ended = ended,
        identifier = pastRunIdentifier(session, issue, batchIssues),
        title = titleOverride ?: pastRunTitle(session, issue, batchIssues),
        caption = caption,
        depth = depth,
        guide = guide,
        deviceIcon = deviceIcon,
        deviceName = device.displayLabel,
        dimmed = paused,
        onClick = onClick,
        modifier = modifier,
    )
}

/** The caption tones, `session-display.json` statusTone x4. */
@Composable
internal fun sessionRowToneColor(tone: SessionStatusTone): Color = when (tone) {
    SessionStatusTone.Muted -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    SessionStatusTone.Amber -> NeedsInputAmber
    SessionStatusTone.Emerald -> ReviewGreen
    SessionStatusTone.Sky -> DoneBlue
}

/** Every session row's test tag (live and ended alike, x4 `session-row`). */
const val SESSION_ROW_TAG = "session-row"

/** An offline host's row, dimmed (web `opacity-60`). */
private const val DIMMED_ALPHA = 0.6f
