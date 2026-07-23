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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.AltRoute
import androidx.compose.material.icons.automirrored.filled.CallMerge
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection

// The compact agent/PR card on the issue detail (EXP-156): a live "Coding now"
// session row and the PR / branch summary as GitHub-style chips linking to the
// dedicated Changes page. The Start-coding launcher moved into the bottom
// bar's start circle (EXP-240) — this card renders only when there is a
// session, a PR, or a pushed branch.

// The remote-start progress state — relocated here from the deleted SteerPanel.
// `isBatch` drives the "follow it in the Agents tab" caption for a 2+ run.
sealed interface SteerStartState {
    data object Idle : SteerStartState
    data object Sending : SteerStartState
    data class Sent(val deviceLabel: String, val isBatch: Boolean) : SteerStartState
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

// GitHub PR-state tints (EXP-240): open green / merged indigo / closed red.
private val PrOpenGreen = DesignTokens.Semantic.Green
private val PrMergedIndigo = Color(0xFF818CF8)
private val PrClosedRed = DesignTokens.Semantic.Red

@Composable
fun AgentPrCard(
    issue: IssueEntity,
    session: CodingSessionEntity?,
    sessionOwner: UserEntity?,
    steerEnabled: Boolean?,
    isMember: Boolean,
    onWatch: (String) -> Unit,
    onOpenChanges: () -> Unit,
) {
    val hasPr = !issue.prUrl.isNullOrBlank()
    val hasBranch = !hasPr && !issue.branch.isNullOrBlank()
    val hasContent = session != null || hasPr || hasBranch
    if (!hasContent) return

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassSection()
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (session != null) {
            SessionRow(
                session = session,
                prState = issue.prState,
                sessionOwner = sessionOwner,
                steerEnabled = steerEnabled,
                isMember = isMember,
                onWatch = onWatch,
            )
        }
        if (hasPr) {
            PrRow(prState = issue.prState, prNumber = issue.prNumber, onOpenChanges = onOpenChanges)
        } else if (hasBranch) {
            BranchRow(branch = issue.branch!!, onOpenChanges = onOpenChanges)
        }
    }
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
    isMember: Boolean,
    onWatch: (String) -> Unit,
) {
    val watchable = isMember && steerEnabled == true
    val state = codingSessionDisplayState(session, prState)
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .let { if (watchable) it.clickable { onWatch(session.id) } else it },
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
                Icon(
                    Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = "Watch live",
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        if (isMember && steerEnabled == false) {
            Spacer(Modifier.height(6.dp))
            Text(
                "Live steering is unavailable on this instance.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

// Linked PR as a GitHub-style capsule chip: pull icon tinted by state
// (open green / merged indigo / closed red) + "PR #n" + the state word inside
// the capsule (iOS parity), tapping into Changes.
@Composable
private fun PrRow(prState: String?, prNumber: Int?, onOpenChanges: () -> Unit) {
    val tint = when (prState) {
        DomainContract.prStateMerged -> PrMergedIndigo
        DomainContract.prStateClosed -> PrClosedRed
        else -> PrOpenGreen
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .glassButton()
                .padding(horizontal = 10.dp, vertical = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Filled.CallMerge,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = tint,
            )
            Text(
                prNumber?.let { "PR #$it" } ?: "Pull request",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (!prState.isNullOrBlank()) {
                Text(
                    prState.replaceFirstChar { it.uppercase() },
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
    }
}

// Pushed branch, no PR yet: a capsule chip with the indigo branch icon + mono
// name, tapping into Changes.
@Composable
private fun BranchRow(branch: String, onOpenChanges: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpenChanges),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f, fill = false)
                .glassButton()
                .padding(horizontal = 10.dp, vertical = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Filled.AltRoute,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = AccentIndigo,
            )
            Text(
                branch,
                style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
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
