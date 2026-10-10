package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AccountLimits
import com.exponential.app.domain.AccountOption
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.ui.theme.TextEmphasis

// EXP-872: the account options every launch surface picks from (web
// `accountOptionsOf`) and the EXP-992 limit preview the shared
// `picker/AccountPicker` rows draw. The list is every signed-in login the
// machine reports, across both agents, and picking one IMPLIES its agent
// (`AccountOptions.flatten` owns the rules). The pill and the sheet are the
// Picker's (`AccountPicker` + `AccountPickerPillTrigger`).
//
// EXP-992: on a TOUCH platform the three tiny bars sit INLINE under the email
// in every menu row (web hovers them out to the side on a pointer): `5h` /
// `week` / `<model lowercased>`, off the option's `limits` (fractions 0-1).

/** The bar labels, byte-identical ×4; the model bar wears the window's own
 *  name, lower-cased (`fable`). */
private const val LIMIT_LABEL_FIVE_HOUR = "5h"
private const val LIMIT_LABEL_WEEK = "week"

/** The label column and the block's width in a menu row — a dropdown wraps its
 *  content, so the bars have to bring a width of their own. The block width is
 *  shared with the picker sheet's login row (EXP-1021), where a full-width bar
 *  would read as a progress meter rather than as a preview. */
private val LimitLabelWidth: Dp = 30.dp
internal val AccountLimitBarsWidth: Dp = 136.dp

/** One drawn bar: what it is called and how full it is (0-1). */
internal data class AccountLimitBar(val label: String, val used: Double)

/** The bars in order: 5h, week, then the model window when there is one. */
internal fun accountLimitBars(limits: AccountLimits): List<AccountLimitBar> = buildList {
    add(AccountLimitBar(LIMIT_LABEL_FIVE_HOUR, limits.fiveHour))
    add(AccountLimitBar(LIMIT_LABEL_WEEK, limits.week))
    limits.model?.let { add(AccountLimitBar(it.label.lowercase(), it.used)) }
}

/**
 * The compact three-bar block: a tiny label column and [UsageTrack] at its
 * MINI height per row — the same primitive every other meter in the app draws,
 * so a percentage can never read in two tones.
 */
@Composable
internal fun AccountLimitBars(
    limits: AccountLimits,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        accountLimitBars(limits).forEach { bar ->
            val percent = (bar.used * 100.0).coerceIn(0.0, 100.0)
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    bar.label,
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontSize = 10.sp,
                        lineHeight = 12.sp,
                    ),
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.width(LimitLabelWidth),
                )
                UsageTrack(
                    percent = percent,
                    severity = AgentUsagePresentation.severity(percent),
                    modifier = Modifier.weight(1f),
                    height = UsageTrackMiniHeight,
                )
            }
        }
    }
}

/**
 * EXP-872 (web `accountOptionsOf`): the machine's flattened logins, or — for a
 * machine that reports NONE at all (a build before profiles, a heartbeat that
 * has not landed) — one UNPINNED option per agent in [fallbackAgents] (the
 * machine picks its last used login), labelled by the agent's own name, the
 * machine's last used agent first. The picker never goes empty while a run
 * could still start on that machine.
 */
internal fun accountOptionsFor(
    device: SteerDevice?,
    fallbackAgents: List<String>,
): List<AccountOption> {
    if (device == null) return emptyList()
    val flat = AccountOptions.flatten(device)
    if (flat.isNotEmpty()) return flat
    return unpinnedAccountOptions(
        fallbackAgents,
        device.launchDefaults?.defaultAgent?.takeIf { it in fallbackAgents },
    )
}

/**
 * The unpinned fallback on its own — one option per agent (id ""), the
 * [preferred] one first. Split out for the surface that must stay editable
 * with NO machine bound at all (a workflow's runner block, where the agent is
 * configured before a runner is picked).
 */
internal fun unpinnedAccountOptions(
    agents: List<String>,
    preferred: String?,
): List<AccountOption> {
    val first = preferred?.takeIf { it in agents } ?: agents.firstOrNull()
    return agents
        .map { agent ->
            AccountOption(
                id = "",
                agent = agent,
                email = agentLabel(agent),
                isLastUsed = agent == first,
                health = AgentHealth.Unknown,
            )
        }
        .sortedByDescending { it.isLastUsed }
}
