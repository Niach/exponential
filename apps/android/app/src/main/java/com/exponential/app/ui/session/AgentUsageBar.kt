package com.exponential.app.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.AgentUsageSeverity
import com.exponential.app.domain.UsageCard
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.delay

// EXP-484/EXP-688: the agent's rate-limit usage, rendered the same way on all
// four clients (web `components/agent-usage-bar.tsx`, iOS `AgentUsageBar.swift`,
// desktop `ui/src/usage_bar.rs`) — one card per reported window, grouped into
// Current session / the untitled weekly limits / Other. Every rule (grouping, titles,
// captions, the severity thresholds, the stale treatment) comes from the
// shared `AgentUsagePresentation.usageGroups`; nothing is decided here.
//
// EXP-688 deleted the collapsed hairline strip and the pinned-window radio
// rows with it: usage lives in the session's Usage sheet and in each agent's
// tab of Device settings, never as chrome over the feed.

/**
 * Every reported window as cards. Only a group that carries a title renders a
 * header (EXP-694 dropped the weekly one; the session group's single card
 * already says "Current session").
 * Stale numbers — the last good ones after a failed refresh — render at half
 * opacity with an "as of …" footer rather than vanishing.
 *
 * [compact] is the Device-settings rendering: the cards sit INSIDE that
 * agent's card there, so they drop the nested glass surface and tighten up.
 */
@Composable
internal fun AgentUsageCards(
    usage: AgentUsage,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val nowMs = rememberUsageClock()
    val groups = remember(usage, nowMs) { AgentUsagePresentation.usageGroups(usage, nowMs) }
    if (groups.isEmpty()) return

    Column(
        modifier = modifier.fillMaxWidth().alpha(if (usage.stale) 0.5f else 1f),
        verticalArrangement = Arrangement.spacedBy(if (compact) 12.dp else 16.dp),
    ) {
        groups.forEach { group ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                // A group renders a header only when it has one to render: the
                // weekly group's title is empty since EXP-694 (its windows are
                // plain rows) and the session group's single card is already
                // titled "Current session".
                if (group.title.isNotEmpty() &&
                    group.key != AgentUsagePresentation.GROUP_SESSION
                ) {
                    Text(
                        group.title,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                    )
                }
                group.cards.forEach { card -> UsageCardRow(card = card, compact = compact) }
            }
        }
        // Stale = a refresh failed and these are the last good numbers, so the
        // cards say when they were true instead of pretending to be live.
        if (usage.stale) {
            usage.fetchedAt?.let { fetchedAt ->
                Text(
                    "as of ${relativeTime(fetchedAt)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/** One window: title + `n% used`, its track, and the countdown under it. */
@Composable
private fun UsageCardRow(card: UsageCard, compact: Boolean) {
    // One announcement per card instead of three fragments — the title, the
    // percentage and the countdown only mean anything together.
    val description = buildString {
        append("${card.title} ${card.percent}% used")
        if (card.caption.isNotEmpty()) append(", ${card.caption}")
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (compact) Modifier else Modifier.glassRow())
            .padding(
                horizontal = if (compact) 0.dp else 12.dp,
                vertical = if (compact) 0.dp else 10.dp,
            )
            .semantics(mergeDescendants = true) { contentDescription = description },
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Text(
                card.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Spacer(Modifier.weight(1f))
            Text(
                "${card.percent}% used",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Spacer(Modifier.height(6.dp))
        UsageTrack(percent = card.percent.toDouble(), severity = card.severity, height = 6.dp)
        if (card.caption.isNotEmpty()) {
            Spacer(Modifier.height(4.dp))
            Text(
                card.caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
    }
}

/** The filled track every usage card renders. */
@Composable
internal fun UsageTrack(percent: Double, severity: AgentUsageSeverity, height: Dp) {
    val fraction = (percent / 100.0).coerceIn(0.0, 1.0).toFloat()
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(height / 2))
            .background(GlassTokens.StrokeStrong),
    ) {
        if (fraction > 0f) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(fraction)
                    .fillMaxHeight()
                    .background(severityColor(severity)),
            )
        }
    }
}

/** Normal / ≥75 warning / ≥95 danger — the shared thresholds, mobile's tones. */
private fun severityColor(severity: AgentUsageSeverity): Color = when (severity) {
    AgentUsageSeverity.Danger -> DesignTokens.Semantic.Red
    AgentUsageSeverity.Warning -> DesignTokens.Semantic.Yellow
    AgentUsageSeverity.Normal -> Color.White.copy(alpha = 0.35f)
}

/**
 * A wall clock the countdowns re-read on. The synced snapshot itself doesn't
 * change between the machine's refreshes, so without this "resets in 2h 10m"
 * would be frozen at whatever it said when the screen opened.
 */
@Composable
private fun rememberUsageClock(): Long {
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000L)
            nowMs = System.currentTimeMillis()
        }
    }
    return nowMs
}
