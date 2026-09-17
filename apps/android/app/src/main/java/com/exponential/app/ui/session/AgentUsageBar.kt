package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.UsageCard
import com.exponential.app.ui.components.UsageTrack
import com.exponential.app.ui.components.UsageTrackMiniHeight
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay

// EXP-484/EXP-688/EXP-909: the agent's rate-limit usage, rendered the same way
// on all four clients (web `components/agent-usage-bar.tsx` `UsageWindows` +
// `agent-usage-mini.tsx` `UsageMini`, iOS `UsageWindows`/`AgentUsageMini`,
// desktop `usage_bar::render_usage_windows` / `render_usage_mini`). Every rule
// (grouping, titles, captions, the severity thresholds, the "as of" line)
// comes from the shared `AgentUsagePresentation`; nothing is decided here.
//
// EXP-909 replaced the three-line glass CARDS with two forms and ONE bar
// primitive ([UsageTrack]): the FULL form (two lines per window) for the login
// the overlay is about, and the MINI form (three tiny meters abreast) for
// every OTHER login — on an account row in the overlay and on a device's
// login row on the Devices page.

/**
 * The FULL form: every reported window as two lines — the limit's name and its
 * countdown, then the meter and the percentage. Nothing is hidden for being
 * old: a report past its freshness window (or flagged stale by the machine)
 * DIMS and carries an "as of …" line, because the last good numbers still say
 * more than a blank.
 */
@Composable
internal fun UsageWindows(usage: AgentUsage, modifier: Modifier = Modifier) {
    val nowMs = rememberUsageClock()
    val groups = remember(usage, nowMs) { AgentUsagePresentation.usageGroups(usage, nowMs) }
    if (groups.isEmpty()) return
    val age = remember(usage, nowMs) { AgentUsagePresentation.usageAge(usage, nowMs) }

    Column(
        modifier = modifier.fillMaxWidth().alpha(if (age != null) 0.5f else 1f),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        groups.forEach { group ->
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                // EXP-909: no group headings ×4 — every window names itself
                // ("Current session" / "All models" / "<Label> only", or its
                // wire label), so the groups only order the rows.
                group.cards.forEach { card -> UsageWindowRow(card) }
            }
        }
        if (age != null) {
            Text(
                age,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** One window: title + countdown, then the meter + `NN%`. */
@Composable
private fun UsageWindowRow(card: UsageCard) {
    // One announcement per window instead of three fragments — the title, the
    // percentage and the countdown only mean anything together.
    val description = buildString {
        append("${card.title} ${card.percent}% used")
        if (card.caption.isNotEmpty()) append(", ${card.caption}")
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
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
            if (card.caption.isNotEmpty()) {
                Spacer(Modifier.weight(1f))
                Text(
                    card.caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                )
            }
        }
        Spacer(Modifier.height(6.dp))
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            UsageTrack(
                percent = card.percent.toDouble(),
                severity = card.severity,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            // EXP-909: "NN%", never "NN% used" — the meter beside it already
            // says what the number is a share of.
            Text(
                "${card.percent}%",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
            )
        }
    }
}

/**
 * EXP-909: the MINI form — up to three tiny meters in ONE line, each the
 * window's WIRE label, its bar and `NN%`. The other accounts in the usage
 * overlay and every login on the Devices page wear it, and EXP-872 will mount
 * the same piece as the account picker's hover preview, so it stays a
 * standalone composable with no surrounding chrome.
 *
 * Renders nothing when the login reported no windows — the caller then says
 * `Checking…` or `No usage reported`, which are different statements.
 */
@Composable
internal fun AgentUsageMini(
    usage: AgentUsage?,
    modifier: Modifier = Modifier,
    /**
     * EXP-944: a clock captions the 5h and Week bars with when they reset
     * (`resets in 2h 14m`, the wording the full form already uses). Null =
     * bars only, which is what the tight surfaces want — the usage overlay's
     * other logins and the account picker's preview.
     */
    nowMs: Long? = null,
) {
    val windows = remember(usage) { AgentUsagePresentation.miniWindows(usage) }
    if (windows.isEmpty()) return
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        windows.forEach { window ->
            val reset = nowMs?.let { AgentUsagePresentation.miniWindowReset(window, it) }
            Column(
                modifier = Modifier
                    .weight(1f)
                    .semantics(mergeDescendants = true) {
                        contentDescription = buildString {
                            append("${window.label} ${window.percent}% used")
                            reset?.let { append(", $it") }
                        }
                    },
                verticalArrangement = Arrangement.spacedBy(1.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        window.label,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    UsageTrack(
                        percent = window.percent.toDouble(),
                        severity = window.severity,
                        modifier = Modifier.weight(1f),
                        height = UsageTrackMiniHeight,
                    )
                    Text(
                        "${window.percent}%",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                    )
                }
                // EXP-944: under the bar it belongs to, centred — the device
                // list is where a limit is actually planned around.
                if (reset != null) {
                    Text(
                        reset,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}

/**
 * A wall clock the countdowns re-read on. The synced snapshot itself doesn't
 * change between the machine's refreshes, so without this "resets in 2h 10m"
 * would be frozen at whatever it said when the screen opened.
 */
@Composable
internal fun rememberUsageClock(): Long {
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000L)
            nowMs = System.currentTimeMillis()
        }
    }
    return nowMs
}
