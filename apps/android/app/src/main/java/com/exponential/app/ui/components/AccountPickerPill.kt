package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AccountLimits
import com.exponential.app.domain.AccountOption
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// EXP-872: THE account picker of this client (web `AccountPicker`, desktop
// `coding_selects::account_picker`, iOS `AccountPickerMenu`). It REPLACES the
// agent picker AND the account pill on every launch surface: the list is every
// signed-in login the machine reports, across both agents, and picking one
// IMPLIES its agent (`AccountOptions.flatten` owns the rules; this file only
// draws them).
//
// The chip and the row read the same way: the agent's brand mark + the login's
// EMAIL. Never the profile name, never the word "default" — the device default
// is simply the first row. A dead credential rides as a muted badge beside the
// email.
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
 * The pill AND its dropdown: the brand mark, the login's email, its health
 * badge when the credential is dead, and a chevron — a single login is a
 * statement, not a choice, so it renders as a plain capsule that opens
 * nothing. [enabled] false is the same statement for a LOCKED surface (a
 * workflow past draft, whose server refuses the write anyway): dimmed,
 * chevron-less, still readable.
 *
 * Renders NOTHING when [options] is empty: a machine with no login to pick has
 * no decision to offer, and the surface's own caption says what cannot start.
 *
 * EXP-1021 did NOT retire this: the shared `AccountPicker` sheet took the
 * launch composer, and this dropdown SURVIVES on the three surfaces the sweep
 * did not reach — `ui/agent/AgentOptionsRow` (EXP-1019 owns that file),
 * `ui/workflows/WorkflowDetailScreen` and `ui/session/DeviceSettingsSheet`.
 * Until those move, the app shows a login list two ways, and this file owes a
 * deletion once they land. [AccountPill] is the part that outlives it — it is
 * the picker's trigger.
 */
@Composable
internal fun AccountPickerPill(
    options: List<AccountOption>,
    selectedKey: String?,
    onSelect: (AccountOption) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentDescription: String = "Account",
) {
    val current = options.firstOrNull { it.key == selectedKey } ?: options.firstOrNull() ?: return
    val pickable = enabled && options.size > 1
    val byKey = remember(options) { options.associateBy { it.key } }
    // EXP-1030: the pill is only the TRIGGER — the list it opens is the shared
    // [AccountPicker] (a sheet of plain rows, the pick marked by the row's own
    // highlight like every other pick on the phone), whose row keeps the brand
    // mark, the badge and the EXP-992 limit preview.
    Box(modifier = modifier) {
        AccountPicker(
            options = options.map { it.toPickerAccount() },
            value = current.key,
            onChange = { key -> byKey[key]?.let(onSelect) },
            trigger = { open ->
                AccountPill(
                    option = current,
                    onClick = if (pickable) open else null,
                    enabled = enabled,
                    contentDescription = contentDescription,
                )
            },
        )
    }
}

/**
 * The picker's TRIGGER on its own: the brand mark, the login's email, its
 * health badge when the credential is dead, and a chevron only when [onClick]
 * opens something. EXP-1021 split it out of [AccountPickerPill] so a surface
 * can keep this exact capsule while its options move into the shared
 * `AccountPicker` sheet (the automation editor's pin).
 */
@Composable
internal fun AccountPill(
    option: AccountOption,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    contentDescription: String = "Account",
) {
    val badge = AgentHealthRules.badgeLabel(option.health)
    val pickable = enabled && onClick != null
    GlassPill(
        option.email,
        onClick = if (pickable) onClick else null,
        enabled = enabled,
        modifier = modifier,
        leading = {
            Icon(
                agentIconPainter(option.agent),
                contentDescription = null,
                modifier = Modifier.size(13.dp),
                tint = agentIconTint(option.agent),
            )
        },
        trailing = if (badge != null || pickable) {
            {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (badge != null) {
                        Text(
                            badge,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface
                                .copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                    }
                    if (pickable) {
                        Icon(
                            ExpIcons.uiChevronDown,
                            contentDescription = null,
                            modifier = Modifier.size(10.dp),
                            tint = MaterialTheme.colorScheme.onSurface
                                .copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
        } else {
            null
        },
        contentDescription = contentDescription,
    )
}


/**
 * EXP-872 (web `accountOptionsOf`): the machine's flattened logins, or — for a
 * machine that reports NONE at all (a build before profiles, a heartbeat that
 * has not landed) — one AMBIENT option per agent in [fallbackAgents], labelled
 * by the agent's own name, the machine's default agent first. The picker never
 * goes empty while a run could still start on that machine.
 */
internal fun accountOptionsFor(
    device: SteerDevice?,
    fallbackAgents: List<String>,
): List<AccountOption> {
    if (device == null) return emptyList()
    val flat = AccountOptions.flatten(device)
    if (flat.isNotEmpty()) return flat
    return ambientAccountOptions(
        fallbackAgents,
        device.launchDefaults?.defaultAgent?.takeIf { it in fallbackAgents },
    )
}

/**
 * EXP-1043: [accountOptionsFor] for the DEVICE SETTINGS sheet, where the
 * default-account row is the machine's "which agent do runs start on"
 * setting and must therefore always be changeable: every agent in [agents]
 * that reports no login of its own still contributes its AMBIENT one, so a
 * machine signed into claude alone can still be pointed at codex. The launch
 * surfaces keep [accountOptionsFor]'s stricter list — there, an agent with no
 * login on the machine is not something to start a run on.
 */
internal fun deviceAccountOptions(
    device: SteerDevice?,
    agents: List<String>,
): List<AccountOption> {
    val reported = accountOptionsFor(device, agents)
    val missing = agents.filter { agent -> reported.none { it.agent == agent } }
    if (missing.isEmpty()) return reported
    return reported + ambientAccountOptions(missing, preferred = null)
        .map { it.copy(isDeviceDefault = false) }
}

/**
 * The ambient fallback on its own — one `system` option per agent, the
 * [preferred] one first. Split out for the surface that must stay editable
 * with NO machine bound at all (a workflow's runner block, where the agent is
 * configured before a runner is picked).
 */
internal fun ambientAccountOptions(
    agents: List<String>,
    preferred: String?,
): List<AccountOption> {
    val first = preferred?.takeIf { it in agents } ?: agents.firstOrNull()
    return agents
        .map { agent ->
            AccountOption(
                id = SYSTEM_PROFILE_ID,
                agent = agent,
                email = agentLabel(agent),
                isDeviceDefault = agent == first,
                health = AgentHealth.Unknown,
            )
        }
        .sortedByDescending { it.isDeviceDefault }
}
