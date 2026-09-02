package com.exponential.app.ui.issue

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow

// The issue detail's agent surfaces (EXP-156). EXP-698 r4 split them in two,
// because they answer different questions and belong in different places:
// [CodingNowCard] is the live session, in the property-chip box's own chrome
// directly under it, and [AgentPrCard] is only the PR / branch row below the
// description, linking to the dedicated Changes page. The Start-coding
// launcher moved into the bottom bar's start circle (EXP-240), so neither of
// them starts anything; each renders only when it has something to say.

// The remote-start progress state — relocated here from the deleted SteerPanel.
// EXP-536: `Sent` is a pure WAITING state now (single and batch alike) — the
// surface jumps into the live session as soon as the desktop's row syncs in,
// so nothing points at the Agents tab any more.
sealed interface SteerStartState {
    data object Idle : SteerStartState
    data object Sending : SteerStartState
    data class Sent(val deviceLabel: String) : SteerStartState
    data class Failed(val message: String) : SteerStartState
}

internal val LiveGreen = Color(0xFF34D399)

// EXP-194/EXP-214: the parked states render a STATIC dot + label instead of
// the running pulse, colored like the issue-status palette (StatusColors):
// review green, done blue; the desktop-reported "needs input" picker wait is
// amber.
internal val ReviewGreen = DesignTokens.Semantic.Green
internal val DoneBlue = DesignTokens.Semantic.Blue
internal val NeedsInputAmber = DesignTokens.Semantic.Yellow

// PR-state tints (EXP-240): open green / merged blue (the done-status
// semantic, web/desktop parity — EXP-594 retired the indigo) / closed red.
private val PrOpenGreen = DesignTokens.Semantic.Green
private val PrMergedBlue = DesignTokens.Semantic.Blue
private val PrClosedRed = DesignTokens.Semantic.Red

@Composable
fun AgentPrCard(
    issue: IssueEntity,
    onOpenChanges: () -> Unit,
) {
    val hasPr = !issue.prUrl.isNullOrBlank()
    val hasBranch = !hasPr && !issue.branch.isNullOrBlank()
    if (!hasPr && !hasBranch) return

    // No card wrapper (EXP-246): the PR/branch chips render full width against
    // the screen background, matching iOS.
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (hasPr) {
            PrRow(prState = issue.prState, prNumber = issue.prNumber, onOpenChanges = onOpenChanges)
        } else {
            BranchRow(branch = issue.branch!!, onOpenChanges = onOpenChanges)
        }
    }
}

/**
 * The live session, in the SAME box the property chips sit in (EXP-698 r4) and
 * mounted right under them: a run in progress is a property of the issue you
 * are looking at, not a footnote below the description. The PR/branch rows
 * stay where they were, next to the code they link to.
 */
@Composable
fun CodingNowCard(
    session: CodingSessionEntity,
    prState: String?,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    /** EXP-312: live sessions are owner-only — Watch renders only on the
     *  caller's own session. */
    currentUserId: String?,
    onWatch: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    SessionRow(
        session = session,
        prState = prState,
        sessionOwner = sessionOwner,
        steerEnabled = steerEnabled,
        currentUserId = currentUserId,
        onWatch = onWatch,
        // The property-chip box's own chrome, to the pixel — the two boxes
        // stack, so a different fill or padding would read as a mistake.
        modifier = modifier
            .fillMaxWidth()
            .glassCard()
            .padding(10.dp),
    )
}

// Live session: a status dot + label + who/where, tapping into the steer
// viewer when steering is available; an inert caption when it's off. `running`
// shows the pulsing green "Coding now"; the parked states show a static dot —
// review green / done blue / needs-input amber (EXP-194/EXP-214).
@Composable
private fun SessionRow(
    session: CodingSessionEntity,
    prState: String?,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    currentUserId: String?,
    onWatch: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    // EXP-312: only the session's own runner may open it live — teammates see
    // the status badge + byline, nothing tappable.
    val ownSession = currentUserId != null && session.userId == currentUserId
    val watchable = ownSession && steerEnabled == true
    val state = codingSessionDisplayState(session, prState)
    Column(modifier = modifier) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                // EXP-627: the store slide's pop-out rect is measured off this
                // row (`PopRects`), iOS parity.
                .testTag("coding-now-row"),
            // EXP-698 r4: the ROW is inert — the Watch pill is the only tap
            // (iOS does the same). A clickable row inside a rounded card drew
            // a rectangular ripple across its corners, and gave the same
            // action two hit targets with only one of them looking tappable.
            verticalAlignment = Alignment.CenterVertically,
        ) {
            when (state) {
                CodingSessionDisplayState.Running -> PulsingDot()
                CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber)
                CodingSessionDisplayState.Review -> StaticDot(ReviewGreen)
                CodingSessionDisplayState.Done -> StaticDot(DoneBlue)
            }
            Spacer(Modifier.width(8.dp))
            Text(
                when (state) {
                    CodingSessionDisplayState.Running -> "Coding now"
                    CodingSessionDisplayState.NeedsInput -> "Needs input"
                    CodingSessionDisplayState.Review -> "Ready for review"
                    CodingSessionDisplayState.Done -> "Done"
                },
                style = MaterialTheme.typography.labelLarge,
                color = when (state) {
                    CodingSessionDisplayState.Running -> LiveGreen
                    CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
                    CodingSessionDisplayState.Review -> ReviewGreen
                    CodingSessionDisplayState.Done -> DoneBlue
                },
            )
            Spacer(Modifier.width(8.dp))
            val who = userDisplayName(sessionOwner, session.userId)
            val device = session.deviceLabel?.takeIf { it.isNotBlank() }
            Text(
                "· $who" + if (device != null) " · $device" else "",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (watchable) {
                Spacer(Modifier.width(8.dp))
                // EXP-698 r4: the ONE emphatic pill on the issue screen —
                // watching your own run live is the action the card exists
                // for, and a 18dp chevron never said so.
                GlassPill(
                    "Watch",
                    size = PillSize.Sm,
                    primary = true,
                    icon = ExpIcons.navDevices,
                    onClick = { onWatch(session.id) },
                )
            }
        }
        if (ownSession && steerEnabled == false) {
            Spacer(Modifier.height(6.dp))
            Text(
                "Live steering is unavailable on this instance.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

// Linked PR as a full-width row (EXP-327, Linear parity): pull icon tinted by
// state (open green / merged indigo / closed red) + "PR #n" on the left, the
// state word on the right, tapping into Changes. It used to hug its content as
// a small capsule, which read as a stray chip rather than a link to the code.
@Composable
private fun PrRow(prState: String?, prNumber: Int?, onOpenChanges: () -> Unit) {
    val tint = when (prState) {
        DomainContract.prStateMerged -> PrMergedBlue
        DomainContract.prStateClosed -> PrClosedRed
        else -> PrOpenGreen
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges)
            .glassRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.prOpen,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = tint,
        )
        Text(
            prNumber?.let { "PR #$it" } ?: "Pull request",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (!prState.isNullOrBlank()) {
            Text(
                prState.replaceFirstChar { it.uppercase() },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

// Pushed branch, no PR yet: the same full-width row shape as [PrRow] (they
// occupy the same slot, so they must look alike) with the indigo branch icon +
// mono name, tapping into Changes.
@Composable
private fun BranchRow(branch: String, onOpenChanges: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges)
            .glassRow()
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.uiBranch,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Text(
            branch,
            style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

// Pulsing live-session dot; internal so the Agents tab and the bottom bar's
// start circle reuse the exact "Coding now" pulse (same package).
@Composable
internal fun PulsingDot(size: androidx.compose.ui.unit.Dp = 8.dp) {
    val transition = rememberInfiniteTransition(label = "coding-now")
    val alpha by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.25f,
        animationSpec = infiniteRepeatable(
            animation = tween(900, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "coding-now-alpha",
    )
    Box(
        modifier = Modifier
            .size(size)
            .clip(CircleShape)
            .background(LiveGreen.copy(alpha = alpha)),
    )
}

// Static (non-pulsing) status dot — the `in_review` "ready for review" signal
// (EXP-194). Internal so the Agents tab and the bottom bar reuse the exact glyph.
@Composable
internal fun StaticDot(color: Color, size: androidx.compose.ui.unit.Dp = 8.dp) {
    Box(
        modifier = Modifier
            .size(size)
            .clip(CircleShape)
            .background(color),
    )
}
