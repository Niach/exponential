package com.exponential.app.ui.session

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.AgentUsageSeverity
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay

// EXP-484: the agent's rate-limit usage, rendered the same way on all four
// clients (web `components/agent-usage-bar.tsx`, iOS `AgentUsageBar.swift`,
// desktop `ui/src/usage_bar.rs`) — a hairline over the session feed that
// expands into one row per reported window. Every rule it renders (which
// window is selected, the severity thresholds, the countdown wording, the
// stale treatment) comes from the shared `AgentUsagePresentation`; nothing is
// decided here.

/** The collapsed hairline's height — a comfortable tap target for a 2 dp line. */
private val STRIP_HEIGHT = 14.dp

/**
 * The collapsed usage bar: a full-width 2 dp track filled to the selected
 * window's percentage, tapped to reveal every window. Stale numbers (the last
 * good ones after a failed refresh) render at half opacity rather than
 * vanishing.
 */
@Composable
internal fun AgentUsageStrip(
    agent: String,
    usage: AgentUsage,
    preferredKey: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val nowMs = rememberUsageClock()
    val window = AgentUsagePresentation.selectWindow(usage, preferredKey) ?: return
    var expanded by remember { mutableStateOf(false) }

    val percent = window.percent.toInt()
    val countdown = AgentUsagePresentation.resetCountdown(window.resetsAt, nowMs)
    val description = buildString {
        append("${agentLabel(agent)} usage: ${window.label} $percent%")
        countdown?.let { append(", $it") }
    }

    Column(modifier = modifier.fillMaxWidth().alpha(if (usage.stale) 0.5f else 1f)) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .height(STRIP_HEIGHT)
                .clickable { expanded = !expanded }
                .semantics { contentDescription = description },
            verticalArrangement = Arrangement.Center,
        ) {
            UsageTrack(percent = window.percent, height = 2.dp)
        }
        AnimatedVisibility(visible = expanded) {
            AgentUsageWindowRows(
                agent = agent,
                usage = usage,
                selectedKey = preferredKey,
                onSelect = { key ->
                    onSelect(key)
                    expanded = false
                },
            )
        }
    }
}

/**
 * Every reported window, one tappable row each — the expanded strip and the
 * device editor's Agents section render exactly the same list. Picking a row
 * is what the collapsed bar then shows for this agent.
 */
@Composable
internal fun AgentUsageWindowRows(
    agent: String,
    usage: AgentUsage,
    selectedKey: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (usage.windows.isEmpty()) return
    val nowMs = rememberUsageClock()
    // The marker follows what the collapsed bar actually shows: with no stored
    // preference that is the busiest window, not "nothing selected".
    val effectiveKey = AgentUsagePresentation.selectWindow(usage, selectedKey)?.key

    Column(modifier = modifier.fillMaxWidth().alpha(if (usage.stale) 0.5f else 1f)) {
        usage.windows.forEach { window ->
            val countdown = AgentUsagePresentation.resetCountdown(window.resetsAt, nowMs)
            // One announcement per row instead of five fragments — the label,
            // the percentage and the countdown only mean anything together.
            val rowDescription = buildString {
                append("${agentLabel(agent)} ${window.label} ${window.percent.toInt()}%")
                countdown?.let { append(", $it") }
                if (window.key == effectiveKey) append(", selected")
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onSelect(window.key) }
                    .padding(vertical = 4.dp)
                    .semantics(mergeDescendants = true) { contentDescription = rowDescription },
            ) {
                Icon(
                    if (window.key == effectiveKey) ExpIcons.uiSelected else ExpIcons.uiUnselected,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = if (window.key == effectiveKey) {
                            TextEmphasis.Secondary
                        } else {
                            TextEmphasis.Quaternary
                        },
                    ),
                )
                Spacer(Modifier.width(8.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            window.label,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        Spacer(Modifier.weight(1f))
                        Text(
                            "${window.percent.toInt()}%",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    Spacer(Modifier.height(4.dp))
                    UsageTrack(percent = window.percent, height = 6.dp)
                    countdown?.let {
                        Spacer(Modifier.height(2.dp))
                        Text(
                            it,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Secondary,
                            ),
                        )
                    }
                }
            }
        }
        // Stale = a refresh failed and these are the last good numbers, so the
        // rows say when they were true instead of pretending to be live.
        if (usage.stale) {
            usage.fetchedAt?.let { fetchedAt ->
                Text(
                    "as of ${relativeTime(fetchedAt)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
    }
}

/** The filled track both the collapsed strip and one expanded row render. */
@Composable
private fun UsageTrack(percent: Double, height: Dp) {
    val fraction = (percent / 100.0).coerceIn(0.0, 1.0).toFloat()
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(height / 2))
            .background(Color.White.copy(alpha = 0.10f)),
    ) {
        if (fraction > 0f) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(fraction)
                    .fillMaxHeight()
                    .background(severityColor(percent)),
            )
        }
    }
}

/** Normal / ≥75 warning / ≥95 danger — the shared thresholds, mobile's tones. */
private fun severityColor(percent: Double): Color =
    when (AgentUsagePresentation.severity(percent)) {
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
