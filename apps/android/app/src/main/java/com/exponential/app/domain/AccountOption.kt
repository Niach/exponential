package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.AgentUsageWindow
import com.exponential.app.data.api.DeviceLaunchDefaults
import com.exponential.app.data.api.SteerDevice

// EXP-988 contract: the ONE account option model every composer offers
// (owner: EXP-872). One flattened list REPLACES the agent picker + the account
// picker on every platform. The rules, which [AccountOptions.flatten]
// implements and AccountOptionTest locks:
//
//  - one option per signed-in login the device reports, across both contract
//    agents (`devices.agent_accounts[agent].profiles`; a device that reports
//    no profiles yields its ambient `system` login);
//  - the label is ALWAYS the agent's brand mark + the email — never the
//    profile name, never the word "default". A login the device reports
//    without an address shows its plan; with neither, its profile id (the
//    machine has nothing better);
//  - the DEVICE DEFAULT is marked by ORDER (it is first) and by a check, not
//    by a label: [AccountOption.isDeviceDefault] is true for exactly one
//    option — the `launchDefaults.defaultAccount` profile of `defaultAgent`
//    when the device stores one, else the active login of `defaultAgent`,
//    falling back to the first contract agent's active login. The rest follow
//    in [AgentAccountsRows.sortDeviceLogins] order;
//  - selecting an option IMPLIES the agent: there is no separate agent pick,
//    the agent rides the option and the launch takes both from it;
//  - "default agent" settings become "default account": the setting stores a
//    profile id, and the agent is derived from it.
//
// [AccountOption.limits] are FRACTIONS 0-1 off the usage windows EXP-909
// settled (`AgentUsageWindow.percent / 100`): `fiveHour` = the `session`
// window, `week` = the `weekly` window, `model` = the FIRST per-model window.
// A login with no usage report has no limits at all.
//
// Hand-mirrored ×4 against the same fixture and the same test names:
//   web      apps/web/src/lib/accounts/account-option.ts
//   desktop  apps/desktop/crates/coding/src/account_option.rs
//   iOS      apps/ios/ExpCore/Sources/Domain/AccountOption.swift

/** The FIRST per-model window of a login: its own name and its fraction. */
data class AccountModelLimit(val label: String, val used: Double)

/** The three usage fractions a login carries (0-1, never percentages). */
data class AccountLimits(
    val fiveHour: Double,
    val week: Double,
    val model: AccountModelLimit? = null,
)

/** ONE login a run can be started on — the agent rides it, so picking the
 *  option is the whole decision. */
data class AccountOption(
    /** The profile id (`agent_profiles`), what a launch passes as `account`. */
    val id: String,
    val agent: String,
    /** What the row SAYS beside the brand mark; see the header's fallbacks. */
    val email: String,
    /** Exactly one option per device is the default; it is also listed first. */
    val isDeviceDefault: Boolean,
    /**
     * EXP-849: the device's verdict on the credential — a run started on a
     * dead login dies on its first call, so the row badges `needs_relogin`.
     */
    val health: AgentHealth,
    val limits: AccountLimits? = null,
) {
    /**
     * `<agent>:<profileId>` — the ONE string a select-shaped picker can carry
     * for an option, since a profile id alone (`system`) repeats across
     * agents. [AccountOptions.parseKey] reads it back.
     */
    val key: String get() = "$agent:$id"
}

/** What [AccountOptions.parseKey] reads back out of an [AccountOption.key]. */
data class AccountOptionKey(val agent: String, val id: String)

object AccountOptions {

    // The window keys EXP-484 pinned; the presentation module keeps its own
    // copies private, and this derivation is a different question (a launch
    // decision, not a usage card).
    private const val WINDOW_SESSION = "session"
    private const val WINDOW_WEEKLY = "weekly"
    private const val MODEL_WINDOW_PREFIX = "model:"

    /**
     * The flattened logins of ONE machine's reporting, device default first.
     * Empty for a machine that reports no signed-in login at all — the caller
     * then decides whether it has a fallback (the composer offers one ambient
     * option per runnable agent) or simply nothing to pick.
     */
    fun flatten(
        accounts: Map<String, AgentAccount>?,
        usage: Map<String, AgentUsage>?,
        launchDefaults: DeviceLaunchDefaults?,
    ): List<AccountOption> {
        // `loginRows` already knows the shape: one row per profile (or the
        // ambient `system` login for a profile-less agent), retired agents
        // dropped, the active profile's numbers read off either slot.
        val rows = AgentAccountsRows
            .sortDeviceLogins(AgentAccountsRows.loginRows(accounts, usage))
            .filter { it.signedIn }
        if (rows.isEmpty()) return emptyList()

        // The default: the stored default account of the configured default
        // agent, else that agent's active login, else the first contract
        // agent's active login, else the first row — never none.
        val configured = launchDefaults?.defaultAgent
        val configuredAccount = launchDefaults?.defaultAccount
        val stored = if (configured != null && configuredAccount != null) {
            rows.firstOrNull { it.agent == configured && it.profileId == configuredAccount }
        } else {
            null
        }
        val defaultRow = stored
            ?: activeOf(rows, configured)
            ?: DomainContract.codingAgentValues.firstNotNullOfOrNull { activeOf(rows, it) }
            ?: rows.first()

        val ordered = listOf(defaultRow) + rows.filter { it !== defaultRow }
        return ordered.map { row ->
            AccountOption(
                id = row.profileId,
                agent = row.agent,
                email = optionEmail(row),
                isDeviceDefault = row === defaultRow,
                health = row.health,
                limits = optionLimits(row),
            )
        }
    }

    /** [flatten] for a composed machine row. */
    fun flatten(device: SteerDevice): List<AccountOption> =
        flatten(device.agentAccounts, device.agentUsage, device.launchDefaults)

    /**
     * The option a launch surface should START on: the device default, or the
     * first option. Null for a device that reports no login at all.
     */
    fun default(options: List<AccountOption>): AccountOption? =
        options.firstOrNull { it.isDeviceDefault } ?: options.firstOrNull()

    /** An [AccountOption.key] back into its parts; null on anything else. */
    fun parseKey(key: String): AccountOptionKey? {
        val at = key.indexOf(':')
        if (at <= 0 || at == key.length - 1) return null
        return AccountOptionKey(agent = key.substring(0, at), id = key.substring(at + 1))
    }

    private fun activeOf(
        rows: List<AgentProfileUsageRow>,
        agent: String?,
    ): AgentProfileUsageRow? =
        agent?.let { wanted -> rows.firstOrNull { it.agent == wanted && it.active } }

    /** The email a row reads as — the header's fallback ladder. NOTE: the
     *  profile's own LABEL is never it; the id is the last resort. */
    private fun optionEmail(row: AgentProfileUsageRow): String =
        AgentAccountsRows.accountName(row.email, row.plan)

    private fun optionLimits(row: AgentProfileUsageRow): AccountLimits? {
        val windows = row.usage?.windows.orEmpty()
        if (windows.isEmpty()) return null
        val model = windows.firstOrNull { it.key.startsWith(MODEL_WINDOW_PREFIX) }
        return AccountLimits(
            fiveHour = fraction(windows.firstOrNull { it.key == WINDOW_SESSION }),
            week = fraction(windows.firstOrNull { it.key == WINDOW_WEEKLY }),
            model = model?.let { AccountModelLimit(label = it.label, used = fraction(it)) },
        )
    }

    /** An absent window is 0, never "unknown": the bar is a fraction of a
     *  plan, and a plan with no reading on it has nothing spent. */
    private fun fraction(window: AgentUsageWindow?): Double =
        ((window?.percent ?: 0.0) / 100.0).coerceIn(0.0, 1.0)
}
