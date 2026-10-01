package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceEntity

// EXP-829/EXP-909: the per-login rows the Devices page renders UNDER each
// device (EXP-909 folded the cross-device "Accounts" section away: logins
// belong to the machine that holds them, and merging them by email across
// machines hid the only thing that mattered — which box needs the repair).
// One ROW per agent × profile a machine reports, off the synced `devices`
// rows: own machines plus the servers teammates shared with the selected team.
//
// Hand-mirrored against web `lib/agent-usage.ts` (`agentProfileUsageRows` /
// `deviceLoginRows` / `sortDeviceLogins` / `loginLabel` / `refreshAllowedAt`)
// and desktop `ui/src/usage_bar.rs`, same names, same tests
// (AgentAccountsRowsTest). Everything here is a pure function of the rows:
// online-ness and refresh eligibility are injected, never re-derived.

/**
 * One device × agent × profile — what a machine reports about ONE login of
 * an agent. A device that reports no `profiles` (an older build) falls back
 * to its top-level account + `agentUsage[agent]` as the single `system` row,
 * so the section never goes blank on a pre-profile machine.
 */
data class AgentProfileUsageRow(
    /** `<deviceId>:<agent>:<profileId>` — stable enough to key a list. */
    val key: String,
    val deviceId: String,
    val deviceLabel: String,
    /** One of the caller's own machines (a refresh is only ever queued on those). */
    val mine: Boolean,
    val online: Boolean,
    val agent: String,
    val profileId: String,
    /** The profile's label (`Default` for the system profile when the device sent none). */
    val profileLabel: String,
    /**
     * The machine's LAST USED login for this agent (wire `active`; the ambient
     * login of a profile-less agent always is). Only the device moves it.
     */
    val active: Boolean,
    val signedIn: Boolean,
    /**
     * EXP-849: the device's verdict on the credential (`AgentHealthRules.of`,
     * derived from [signedIn] on a pre-EXP-849 machine).
     */
    val health: AgentHealth,
    val email: String?,
    val plan: String?,
    val usage: AgentUsage?,
    /** The "as of …" fallback when the usage is stale or absent. */
    val checkedAt: String?,
)

object AgentAccountsRows {

    /**
     * A forced usage refresh (`agent_usage_refresh`) is refused while the
     * last fetch is younger than this: the device never hits the agent's
     * usage endpoint more often (its `RATE_LIMITED_FLOOR_SECS`), so the
     * button greys out and names the next allowed time instead of queueing
     * a no-op. Web `RATE_LIMITED_FLOOR_MS`.
     */
    const val RATE_LIMITED_FLOOR_MS = 5 * 60_000L

    /** The device cap a machine must advertise before a refresh is offered. */
    const val REFRESH_CAP = "agent-usage-refresh"

    /** The label for the system profile when the device sent none. */
    const val SYSTEM_PROFILE_LABEL = "Default"

    /**
     * The rows the page renders for [devices] — every login every machine
     * holds. [isOnline] is decided by the caller's clock the same way every
     * device list does (`DeviceLiveness.isOnline`); it is passed in so the
     * derivation stays a pure function of the rows.
     */
    fun agentProfileUsageRows(
        devices: List<DeviceEntity>,
        currentUserId: String,
        isOnline: (String?) -> Boolean,
    ): List<AgentProfileUsageRow> {
        val out = mutableListOf<AgentProfileUsageRow>()
        for (device in devices) {
            profileRows(
                out = out,
                deviceId = device.deviceId,
                deviceLabel = device.label,
                mine = device.userId == currentUserId,
                online = isOnline(device.lastSeenAt),
                accounts = parseAgentAccounts(device.agentAccounts).orEmpty(),
                usageMap = parseAgentUsage(device.agentUsage).orEmpty(),
                usageAt = device.agentUsageAt,
            )
        }
        return out
    }

    /**
     * EXP-909: the logins ONE machine holds, in the order its row lists them
     * ([sortDeviceLogins]) — the Devices page's per-device fold. The SAME core
     * as [agentProfileUsageRows]; only the source differs (a composed
     * [SteerDevice], which has already parsed and merged the jsonb), so a
     * login can never read differently depending on which entry point found
     * it. Web/iOS `deviceLoginRows`.
     */
    fun deviceLoginRows(device: SteerDevice): List<AgentProfileUsageRow> {
        val out = mutableListOf<AgentProfileUsageRow>()
        profileRows(
            out = out,
            deviceId = device.deviceId,
            deviceLabel = device.deviceLabel,
            mine = device.isMine,
            online = device.online,
            accounts = device.agentAccounts.orEmpty(),
            usageMap = device.agentUsage.orEmpty(),
            usageAt = device.agentUsageAt,
        )
        return sortDeviceLogins(out)
    }

    /**
     * EXP-872: the SAME rows off the bare maps, for a caller that holds the
     * reporting without a machine around it ([AccountOptions.flatten], which
     * only needs the logins and never the device's own identity). UNSORTED,
     * like [agentProfileUsageRows] — [sortDeviceLogins] owns the order.
     */
    fun loginRows(
        accounts: Map<String, AgentAccount>?,
        usageMap: Map<String, AgentUsage>?,
        usageAt: String? = null,
    ): List<AgentProfileUsageRow> {
        val out = mutableListOf<AgentProfileUsageRow>()
        profileRows(
            out = out,
            deviceId = "",
            deviceLabel = "",
            mine = true,
            online = true,
            accounts = accounts.orEmpty(),
            usageMap = usageMap.orEmpty(),
            usageAt = usageAt,
        )
        return out
    }

    /**
     * ONE machine's logins, appended to [out]. The union of "has an account"
     * and "reported usage": a machine that only managed one of the two still
     * gets its row.
     *
     * EXP-849: an agent this build has no name for (a retired `pi` still
     * beating off an old daemon) is never a row. The jsonb parse already drops
     * it; the set is filtered here too so a row built from a wire-decoded map
     * can never smuggle one in.
     */
    private fun profileRows(
        out: MutableList<AgentProfileUsageRow>,
        deviceId: String,
        deviceLabel: String,
        mine: Boolean,
        online: Boolean,
        accounts: Map<String, AgentAccount>,
        usageMap: Map<String, AgentUsage>,
        usageAt: String?,
    ) {
        val agents = LinkedHashSet<String>().apply {
            addAll(accounts.keys)
            addAll(usageMap.keys)
        }.filterTo(LinkedHashSet(), AgentUsagePresentation::isContractAgent)
        for (agent in agents) {
            val account = accounts[agent]
            val profiles = account?.profiles.orEmpty()
            if (profiles.isEmpty()) {
                out += AgentProfileUsageRow(
                    key = "$deviceId:$agent:$SYSTEM_PROFILE_ID",
                    deviceId = deviceId,
                    deviceLabel = deviceLabel,
                    mine = mine,
                    online = online,
                    agent = agent,
                    profileId = SYSTEM_PROFILE_ID,
                    profileLabel = SYSTEM_PROFILE_LABEL,
                    active = true,
                    signedIn = account?.signedIn == true,
                    health = AgentHealthRules.of(account),
                    email = nonEmpty(account?.email),
                    plan = nonEmpty(account?.plan),
                    usage = usageMap[agent],
                    // The account's own probe stamp, else the row's usage
                    // stamp; an empty string is nothing to say.
                    checkedAt = nonEmpty(account?.checkedAt) ?: nonEmpty(usageAt),
                )
                continue
            }
            for (profile in profiles) {
                // The active profile's numbers ride BOTH the profile entry and
                // the pre-profile `agentUsage[agent]` slot; prefer the
                // profile's own and fall back for a device that only populated
                // the old slot.
                val usage = profile.usage ?: usageMap[agent].takeIf { profile.active }
                out += AgentProfileUsageRow(
                    key = "$deviceId:$agent:${profile.id}",
                    deviceId = deviceId,
                    deviceLabel = deviceLabel,
                    mine = mine,
                    online = online,
                    agent = agent,
                    profileId = profile.id,
                    profileLabel = nonEmpty(profile.label)
                        ?: if (profile.id == SYSTEM_PROFILE_ID) SYSTEM_PROFILE_LABEL else profile.id,
                    active = profile.active,
                    signedIn = profile.signedIn,
                    health = AgentHealthRules.of(profile),
                    email = nonEmpty(profile.email),
                    plan = nonEmpty(profile.plan),
                    usage = usage,
                    checkedAt = nonEmpty(profile.checkedAt) ?: nonEmpty(account?.checkedAt),
                )
            }
        }
    }

    /**
     * EXP-909: the order ONE device's logins list in — contract agent order
     * first (so claude's logins never interleave with codex's), then the
     * machine's ACTIVE login for each agent, then the ones that need attention
     * (a signed-out or expired credential), then the label and the id for a
     * stable tail. A heartbeat can move the numbers without reshuffling the
     * rows. Byte-identical ×4.
     */
    fun sortDeviceLogins(rows: List<AgentProfileUsageRow>): List<AgentProfileUsageRow> {
        val order = DomainContract.codingAgentValues
        return rows.sortedWith(
            compareBy<AgentProfileUsageRow> {
                order.indexOf(it.agent).let { rank -> if (rank == -1) Int.MAX_VALUE else rank }
            }
                .thenBy { it.agent }
                .thenByDescending { it.active }
                .thenBy { attentionRank(it.signedIn, it.usage, it.health) }
                .thenBy { it.profileLabel }
                .thenBy { it.profileId },
        )
    }

    /**
     * EXP-909: a login row's identity line — WHO the login is: its email, else
     * the bare plan an agent reports instead of an address, else the profile's
     * own label.
     *
     * NEVER a status (EXP-862's rule, kept): the health badge beside it is the
     * single signed-out notice, and the brand mark says which agent. The device
     * is not in it either — the row it sits under already named the machine.
     */
    fun loginLabel(row: AgentProfileUsageRow): String = accountName(row.email, row.plan)

    /** EXP-1013: what a login with no known address and no plan is called. ×4. */
    const val NO_EMAIL_LABEL = "No email"

    /**
     * EXP-1013: the ONE name a login wears on every surface (pickers, rows,
     * sheets; ×4 `accountName`): its EMAIL, signed in or not (the device keeps
     * the last address a signed-out login answered with). An agent that
     * reports no address (codex's API-key login) is named by its plan; a login
     * nobody ever signed in to is "No email". NEVER the profile's internal
     * label ("Default", "Claude Code account 2").
     */
    fun accountName(email: String?, plan: String?): String =
        nonEmpty(email) ?: nonEmpty(plan) ?: NO_EMAIL_LABEL

    /**
     * EXP-849/EXP-909: a login row's health badge, or null when there is
     * nothing to say. A signed-OUT row wears "Signed out" here — the label no
     * longer carries any status, so the badge is the whole notice.
     */
    fun healthBadge(row: AgentProfileUsageRow): String? =
        AgentHealthRules.badgeLabel(row.health)

    /** The chip menu's three entries, byte-identical ×4. */
    const val ACTION_SIGN_IN = "Sign in"
    const val ACTION_SIGN_OUT = "Sign out"
    const val ACTION_REMOVE = "Remove account"

    /**
     * EXP-1137: the server's refusal for a sign-out (or an ambient removal,
     * which signs out first) on a build without the body. Byte-identical ×4.
     */
    const val SIGN_OUT_OLD_APP =
        "That machine runs an older Exponential app that cannot sign agent accounts out. Update it first."

    /** EXP-1137: the ambient login (`system`, or a blank id): the CLI's own config dir. */
    fun isAmbient(profileId: String): Boolean =
        profileId.isBlank() || profileId.trim() == SYSTEM_PROFILE_ID

    /**
     * EXP-862: what a login's chip menu offers, on a machine row or on an
     * account row — the SAME rule on every client, in this fixed order:
     *  - signed out, or a credential that expired here: a sign-in first;
     *  - EXP-1137, signed in on a build with the sign-out body: sign it out
     *    (the row stays);
     *  - remove: any NAMED login on a build with `account-remove`, and
     *    (EXP-1137) the AMBIENT login on a build with the sign-out body, which
     *    signs it out there and hides it until it signs in again.
     *
     * EXP-944: being signed OUT no longer ends the menu at its sign-in. A dead
     * profile is the thing people most want gone, the removal is a profile-dir
     * delete that never touches the account (no `codex logout`, ever), and the
     * server has always taken it — it gates on the caps and the reported
     * profile, never on the credential's state.
     *
     * An empty list means the chip is a statement, not a control (a machine
     * that is offline, a teammate's, or too old to take any of the commands).
     *
     * [canRemoveAccount] is the machine's `account-remove` cap and
     * [canSignOutAccount] its `account-sign-out` one: the server refuses each
     * command without its cap, so an older machine simply does not offer that
     * entry. [canAgentLogin] is its `agent-login` cap, which the server ALSO
     * requires for every one of them (web `devices.ts` `createCommand`).
     */
    fun chipActions(
        signedIn: Boolean,
        health: AgentHealth,
        profileId: String,
        canRemoveAccount: Boolean,
        canAgentLogin: Boolean,
        canSignOutAccount: Boolean = false,
    ): List<String> {
        val signsIn = !signedIn || health == AgentHealth.NeedsRelogin
        val ambient = isAmbient(profileId)
        val out = mutableListOf<String>()
        if (signsIn) out += ACTION_SIGN_IN
        if (signedIn && canAgentLogin && canSignOutAccount) out += ACTION_SIGN_OUT
        val removes = if (ambient) canSignOutAccount else canRemoveAccount
        if (removes && canAgentLogin) out += ACTION_REMOVE
        return out
    }

    /** [chipActions] for an account row's machine chip. */
    fun chipActions(
        row: AgentProfileUsageRow,
        canRemoveAccount: Boolean,
        canAgentLogin: Boolean,
        canSignOutAccount: Boolean = false,
    ): List<String> = chipActions(
        signedIn = row.signedIn,
        health = row.health,
        profileId = row.profileId,
        canRemoveAccount = canRemoveAccount,
        canAgentLogin = canAgentLogin,
        canSignOutAccount = canSignOutAccount,
    )

    /**
     * The confirm "Remove account" asks, pinned ×4 (web
     * `removeAccountConfirmCopy`): it names the login and the machine, and says
     * in the same breath that the ACCOUNT survives — only this machine's copy
     * of the login goes.
     */
    fun removeAccountConfirm(accountLabel: String, deviceLabel: String): String =
        "Delete $accountLabel on $deviceLabel? The login is removed from this device " +
            "only; the account itself is untouched."

    /**
     * EXP-1137: the AMBIENT login's remove confirm, pinned ×4 (web
     * `removeAmbientAccountConfirmCopy`). [agentLabel] is the agent's display
     * name (`Claude` / `Codex`): the sentence says that the CLI in the person's
     * own terminal signs out along with it.
     */
    fun removeAmbientAccountConfirm(
        accountLabel: String,
        deviceLabel: String,
        agentLabel: String,
    ): String =
        "Remove $accountLabel from $deviceLabel? The machine's own $agentLabel login is " +
            "signed out there, including for the $agentLabel CLI in the terminal, and hidden " +
            "here until it signs in again; the account itself is untouched."

    /**
     * EXP-1137: the sign-out confirm, pinned ×4 (web `signOutConfirmCopy`).
     * [ambientAgentLabel] names the agent when the login is the machine's own
     * (the terminal CLI signs out too); a named profile keeps its row.
     */
    fun signOutConfirm(
        accountLabel: String,
        deviceLabel: String,
        ambientAgentLabel: String? = null,
    ): String = if (ambientAgentLabel != null) {
        "Sign $accountLabel out on $deviceLabel? That is the machine's own $ambientAgentLabel " +
            "login, so the $ambientAgentLabel CLI there is signed out too; the account itself " +
            "is untouched."
    } else {
        "Sign $accountLabel out on $deviceLabel? The login stays listed so it can sign in " +
            "again; the account itself is untouched."
    }

    /**
     * The agents an "Add account" flow may sign in on the machine: every
     * INSTALLED one, runnable or signed out (EXP-849: all remaining agents have
     * a device-code flow), in contract order.
     */
    fun addableAgents(device: SteerDevice): List<String> {
        val installed = buildSet {
            addAll(device.agents.orEmpty())
            addAll(device.unauthedAgents)
        }
        return DomainContract.codingAgentValues.filter { it in installed }
    }

    /**
     * Where a new login lands on a machine (web `addAccountLoginTarget`): the
     * AMBIENT login while it is still signed out — nothing to keep beside it —
     * otherwise a new profile carrying [label], which the machine creates.
     * Exactly one of the two is ever set.
     */
    data class LoginTarget(val profileId: String?, val newProfileLabel: String?)

    fun addAccountLoginTarget(device: SteerDevice, agent: String, label: String): LoginTarget {
        val account = device.agentAccounts?.get(agent)
        val ambient = account?.profiles.orEmpty().firstOrNull { it.id == SYSTEM_PROFILE_ID }
        val ambientSignedIn = ambient?.signedIn ?: (account?.signedIn == true)
        return if (ambientSignedIn) {
            LoginTarget(profileId = null, newProfileLabel = label)
        } else {
            LoginTarget(profileId = SYSTEM_PROFILE_ID, newProfileLabel = null)
        }
    }

    /**
     * EXP-862: has the login a sign-in was FOR landed on the device yet? The
     * ONE self-close rule of the sign-in sheet (web `agentLoginLanded`, iOS
     * `loginLanded`), evaluated on the TARGETED login:
     *  - a NEW profile (the "+ Add account" path): a profile carrying the
     *    asked-for label is now usable; the ambient flag is useless here, it
     *    is already true, which is why a new profile was asked for;
     *  - an existing profile: that profile's own state;
     *  - the ambient login: its `system` entry, else the top-level fields.
     * "Usable" is signed in AND not `needs_relogin`.
     */
    fun loginLanded(account: AgentAccount?, profileId: String?, newProfileLabel: String?): Boolean {
        if (account == null) return false
        val profiles = account.profiles.orEmpty().filter { it.id.isNotEmpty() }
        if (newProfileLabel != null) {
            val wanted = newProfileLabel.trim()
            if (wanted.isEmpty()) return false
            return profiles.any { it.label.orEmpty().trim() == wanted && usableLogin(it) }
        }
        if (profileId != null && profileId != SYSTEM_PROFILE_ID) {
            return profiles.any { it.id == profileId && usableLogin(it) }
        }
        profiles.firstOrNull { it.id == SYSTEM_PROFILE_ID }?.let { return usableLogin(it) }
        return account.signedIn && AgentHealthRules.of(account) != AgentHealth.NeedsRelogin
    }

    private fun usableLogin(profile: AgentAccountProfile): Boolean =
        profile.signedIn && AgentHealthRules.of(profile) != AgentHealth.NeedsRelogin

    /** The server's clamp on a profile label (web `MAX_PROFILE_LABEL`). */
    const val MAX_PROFILE_LABEL = 64

    /**
     * Trimmed and cut to the server's limit, so the label the machine names
     * its new config dir with is the one that was asked for. Web/iOS
     * `clampProfileLabel`.
     */
    fun clampProfileLabel(label: String): String = label.trim().take(MAX_PROFILE_LABEL)

    /**
     * `Claude Code account 2` — the smallest N >= 2 whose label is not already
     * one of the machine's logins for the agent (exact match), so a removed
     * "account 2" is reused rather than colliding with a surviving "account 3".
     * Web/iOS `nextProfileLabel`, same rule.
     */
    fun nextProfileLabel(device: SteerDevice, agent: String, agentLabel: String): String {
        val taken = device.agentAccounts?.get(agent)?.profiles.orEmpty()
            .map { it.label.orEmpty() }
            .toSet()
        var n = 2
        while ("$agentLabel account $n" in taken) n += 1
        return clampProfileLabel("$agentLabel account $n")
    }

    /** The fullest window's percent, or 0 for a row with no usage at all. */
    fun peakPercent(usage: AgentUsage?): Int =
        usage?.windows?.maxOfOrNull { it.percent.toInt() } ?: 0

    /**
     * Attention-first bucket: signed-out rows lead (there is something to
     * do), then rows at or over the danger threshold, then everything else.
     *
     * EXP-849: an EXPIRED credential is the same kind of "do something" as a
     * missing one — it leads too, even though the CLI still reports signed in.
     */
    fun attentionRank(
        signedIn: Boolean,
        usage: AgentUsage?,
        health: AgentHealth = AgentHealth.Unknown,
    ): Int = when {
        !signedIn -> 0
        health == AgentHealth.NeedsRelogin -> 0
        AgentUsagePresentation.severity(peakPercent(usage).toDouble()) == AgentUsageSeverity.Danger -> 1
        else -> 2
    }

    /**
     * When a forced refresh is next allowed for [usage], as epoch millis:
     * null = right now (no fetch on record, an unreadable stamp, or a last
     * fetch older than the floor). A stamp in the future (the machine's
     * clock runs ahead) is treated as "just fetched".
     */
    fun refreshAllowedAt(usage: AgentUsage?, nowMs: Long): Long? {
        val fetched = usage?.fetchedAt?.let(WireTimestamps::parseEpochMs) ?: return null
        val next = fetched + RATE_LIMITED_FLOOR_MS
        return next.takeIf { it > nowMs }
    }

    /**
     * Whether a row's machine may run the refresh at all: one of MY
     * machines, online, on a build that advertises the cap (web
     * `deviceCanRefreshUsage`). [caps] is the machine's parsed cap list.
     */
    fun canRefresh(row: AgentProfileUsageRow, caps: List<String>?): Boolean =
        row.mine && row.online && caps?.contains(REFRESH_CAP) == true

    private fun nonEmpty(value: String?): String? = value?.trim()?.takeIf { it.isNotEmpty() }
}
