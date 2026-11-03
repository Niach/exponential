package com.exponential.app.ui.issue

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens

// The session state dots shared by every session list and the Work screen's
// top-bar dot. EXP-1647: they outlived the issue page's PR card (web draws no
// PR row on the issue page, EXP-1293), so they live here on their own.

internal val LiveGreen = Color(0xFF34D399)

// EXP-194/EXP-214: the parked states render a STATIC dot + label instead of
// the running pulse, colored like the issue-status palette (StatusColors):
// review green, done blue; the desktop-reported "needs input" picker wait is
// amber.
internal val ReviewGreen = DesignTokens.Semantic.Green
internal val DoneBlue = DesignTokens.Semantic.Blue
internal val NeedsInputAmber = DesignTokens.Semantic.Yellow

// Pulsing live-session dot; internal so the Agents tab and the bottom bar's
// start circle reuse the exact "Coding now" pulse (same package).
@Composable
internal fun PulsingDot(size: androidx.compose.ui.unit.Dp = 8.dp) {
    val transition = rememberInfiniteTransition(label = "coding-now")
    val alpha = transition.animateFloat(
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
            .graphicsLayer { this.alpha = alpha.value }
            .clip(CircleShape)
            .background(LiveGreen),
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
