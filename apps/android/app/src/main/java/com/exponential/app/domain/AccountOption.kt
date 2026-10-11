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
//    no profiles yields none — the ambient login is never run on);
//  - the label is ALWAYS the agent's brand mark + the email (a login has no
//    other name). A login the device reports without an address shows its
//    plan; with neither, "No email";
//  - the LAST USED login leads, marked by ORDER (it is first) and by a check,
//    not by a label: [AccountOption.isLastUsed] is true for exactly one
//    option — `defaultAgent`'s active login, else the first contract agent's
//    active login, else the first row. The rest follow in
//    [AgentAccountsRows.sortDeviceLogins] order;
//  - selecting an option IMPLIES the agent: there is no separate agent pick,
//    the agent rides the option and the launch takes both from it.
//
// Last used = per agent, the login a PERSON last started or switched a run
// on, on that device (`agent_accounts[agent].profiles[].active`); the last
// used agent = `launch_defaults.defaultAgent`. Triggered action runs,
// agent-started runs and auto-rotation never move it. A launch naming no
// account runs on it; a legacy stored `system` reads as naming none
// ([AccountOptions.pinnedAccount]).
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
    /**
     * The profile id (`agent_profiles`), what a launch passes as `account`;
     * "" = unpinned (the machine's last used login — the fallback options a
     * machine that reports no login offers).
     */
    val id: String,
    val agent: String,
    /** What the row SAYS beside the brand mark; see the header's fallbacks. */
    val email: String,
    /** Exactly one option per device is the last used one; it is also listed first. */
    val isLastUsed: Boolean,
    /**
     * EXP-849: the device's verdict on the credential — a run started on a
     * dead login dies on its first call, so the row badges `needs_relogin`.
     */
    val health: AgentHealth,
    val limits: AccountLimits? = null,
) {
    /**
     * `<agent>:<profileId>` — the ONE string a select-shaped picker can carry
     * for an option (the agent rides it, and an unpinned "" repeats across
     * agents). [AccountOptions.parseKey] reads it back.
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
     * The flattened logins of ONE machine's reporting, last used first.
     * Empty for a machine that reports no signed-in login at all — the caller
     * then decides whether it has a fallback (the composer offers one unpinned
     * option per runnable agent) or simply nothing to pick.
     */
    fun flatten(
        accounts: Map<String, AgentAccount>?,
        usage: Map<String, AgentUsage>?,
        launchDefaults: DeviceLaunchDefaults?,
    ): List<AccountOption> {
        // `loginRows` already knows the shape: one row per profile, retired
        // agents dropped, the active profile's numbers read off either slot.
        val rows = AgentAccountsRows
            .sortDeviceLogins(AgentAccountsRows.loginRows(accounts, usage))
            .filter { it.signedIn }
        if (rows.isEmpty()) return emptyList()

        // The LAST USED login leads: `defaultAgent`'s active login, else the
        // first contract agent's active login, else the first row — never none.
        val lastUsedRow = activeOf(rows, launchDefaults?.defaultAgent)
            ?: DomainContract.codingAgentValues.firstNotNullOfOrNull { activeOf(rows, it) }
            ?: rows.first()

        val ordered = listOf(lastUsedRow) + rows.filter { it !== lastUsedRow }
        return ordered.map { row ->
            AccountOption(
                id = row.profileId,
                agent = row.agent,
                email = optionEmail(row),
                isLastUsed = row === lastUsedRow,
                health = row.health,
                limits = optionLimits(row),
            )
        }
    }

    /** [flatten] for a composed machine row. */
    fun flatten(device: SteerDevice): List<AccountOption> =
        flatten(device.agentAccounts, device.agentUsage, device.launchDefaults)

    /**
     * The option a launch surface should START on: the last used login, or the
     * first option. Null for a device that reports no login at all.
     */
    fun lastUsed(options: List<AccountOption>): AccountOption? =
        options.firstOrNull { it.isLastUsed } ?: options.firstOrNull()

    /**
     * EXP-1278: the new device's option for the login picked on the previous
     * one — same agent, same email address (case-insensitive). Profile ids are
     * per machine, so the address is the identity; a row with no address (a
     * plan, "No email", the agent-named ambient fallback) never carries. Null
     * = the new device settles on its own last used login.
     */
    fun carried(options: List<AccountOption>, previous: AccountOption?): AccountOption? {
        if (previous == null || '@' !in previous.email) return null
        val email = previous.email.trim().lowercase()
        return options.firstOrNull {
            it.agent == previous.agent && it.email.trim().lowercase() == email
        }
    }

    /** An [AccountOption.key] back into its parts (an unpinned key has id ""); null on anything else. */
    fun parseKey(key: String): AccountOptionKey? {
        val at = key.indexOf(':')
        if (at <= 0) return null
        return AccountOptionKey(agent = key.substring(0, at), id = key.substring(at + 1))
    }

    /** The legacy stored id of the retired ambient login: read as unpinned. */
    private const val LEGACY_SYSTEM_ACCOUNT = "system"

    /**
     * A stored launch/trigger/run `account` as a pin: blank, null or the
     * retired `system` = UNPINNED (null — the machine's last used login).
     */
    fun pinnedAccount(raw: String?): String? =
        raw?.trim()?.takeIf { it.isNotEmpty() && it != LEGACY_SYSTEM_ACCOUNT }

    private fun activeOf(
        rows: List<AgentProfileUsageRow>,
        agent: String?,
    ): AgentProfileUsageRow? =
        agent?.let { wanted -> rows.firstOrNull { it.agent == wanted && it.active } }

    /** The email a row reads as — the header's fallback ladder. */
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
