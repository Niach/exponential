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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
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
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.glassRow

// The issue detail's PR surface (EXP-156): [AgentPrCard] is the PR / branch
// row below the description, linking to the Changes face (EXP-893) or page.
// EXP-893 retired the Coding-now / Watch row that sat under the property
// chips — the run is a face of the Work screen now, and the top bar's dot
// says its state. The Start-coding launcher lives in the bottom bar (EXP-240),
// so nothing here starts anything; the row renders only with a PR or branch.
// The dot glyphs at the bottom are shared by every session list and the
// Work screen's switcher badge.

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
