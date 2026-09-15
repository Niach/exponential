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
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.glassRow

// The issue detail's agent surfaces (EXP-156). EXP-698 r4 split them in two,
// because they answer different questions and belong in different places:
// [CodingNowCard] is the live session, one chrome-less line directly under the
// property chips (EXP-818), and [AgentPrCard] is only the PR / branch row below the
// description, linking to the dedicated Changes page. The Start-coding
// launcher moved into the bottom bar's start circle (EXP-240), so neither of
// them starts anything; each renders only when it has something to say.

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
 * The live session on the issue screen. EXP-818 took its CARD away: the run is
 * one line under the property chips now — the caller's own run as a primary
 * "Watch" pill straight into it, a teammate's as a muted `● Coding now · name`
 * caption (web `issue-coding-rows.tsx`'s `start` slot, IDE `coding_now_slot`).
 * A second bordered box under the chips said the same thing the tray's start
 * circle already says, and the state it showed is the only part worth a line.
 * The PR/branch rows stay where they were, next to the code they link to.
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
        // No chrome of its own any more (EXP-818) — a caption and a pill sit
        // straight on the screen, aligned with the chips above them.
        modifier = modifier.fillMaxWidth(),
    )
}

// EXP-818: the live session as ONE line — the caller's own run is the primary
// "Watch" pill and nothing else (the state is what the session screen it opens
// is for), a teammate's is the muted `dot + state (+ · who)` caption, read-only
// (EXP-312 keeps live sessions owner-only). `running` pulses the dot off the
// synced `agent_busy` flag; the parked states are static tones — review green /
// done blue / needs-input amber (EXP-194/EXP-214).
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
            if (watchable) {
                // The ONE emphatic pill on the issue screen (EXP-698 r4):
                // watching your own run live is the whole affordance, and the
                // run's state is the first thing the screen it opens says.
                GlassPill(
                    "Watch",
                    size = PillSize.Sm,
                    primary = true,
                    icon = ExpIcons.navDevices,
                    onClick = { onWatch(session.id) },
                )
            } else {
                // A teammate's run (or the caller's own with steering off):
                // read-only, so it is a caption — the state, and whose run it
                // is when it is not the reader's.
                val tone = when (state) {
                    CodingSessionDisplayState.Running -> LiveGreen
                    CodingSessionDisplayState.NeedsInput -> NeedsInputAmber
                    CodingSessionDisplayState.Review -> ReviewGreen
                    CodingSessionDisplayState.Done -> DoneBlue
                }
                // EXP-848: the pulse is the MID-TURN cue (synced `agent_busy`)
                // — a live run between turns keeps a static disc.
                val running = state == CodingSessionDisplayState.Running && session.agentBusy
                if (running) {
                    PulsingDot(size = GlassPillDefaults.DotSize)
                } else {
                    StaticDot(tone, size = GlassPillDefaults.DotSize)
                }
                Spacer(Modifier.width(6.dp))
                val who = userDisplayName(sessionOwner, session.userId)
                Text(
                    // Web parity (`verb` + ` · name` only when it is someone
                    // else's): the machine name left with the card.
                    when (state) {
                        CodingSessionDisplayState.Running -> "Coding now"
                        CodingSessionDisplayState.NeedsInput -> "Needs input"
                        CodingSessionDisplayState.Review -> "Ready for review"
                        CodingSessionDisplayState.Done -> "Done"
                    } + if (ownSession) "" else " · $who",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
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

/** EXP-848: a LIVE run's dot — it pulses only while the agent is actually
 *  mid-turn (the synced `coding_sessions.agent_busy` flag), and sits steady
 *  green between turns. Every session list goes through this, so the rule
 *  cannot drift row to row (web/iOS/desktop parity). */
@Composable
internal fun LiveDot(busy: Boolean, size: androidx.compose.ui.unit.Dp = 8.dp) {
    if (busy) PulsingDot(size) else StaticDot(LiveGreen, size)
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
