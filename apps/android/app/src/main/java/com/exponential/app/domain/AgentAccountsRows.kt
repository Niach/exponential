package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.AgentUsage
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
 * an agent. Only the reported `profiles` are rows: the top-level fields are
 * never turned into a login of their own (the ambient login is never run on;
 * the doctor offers to import it).
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
    /** The machine's LAST USED login for this agent (wire `active`). Only the device moves it. */
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
     * ONE machine's logins, appended to [out]: one row per reported
     * profile, nothing for an agent that reports none.
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
        for ((agent, account) in accounts) {
            if (!AgentUsagePresentation.isContractAgent(agent)) continue
            for (profile in account.profiles.orEmpty()) {
                if (profile.id.isBlank()) continue
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
                    active = profile.active,
                    signedIn = profile.signedIn,
                    health = AgentHealthRules.of(profile),
                    email = nonEmpty(profile.email),
                    plan = nonEmpty(profile.plan),
                    usage = usage,
                    checkedAt = nonEmpty(profile.checkedAt) ?: nonEmpty(account.checkedAt)
                        ?: nonEmpty(usageAt),
                )
            }
        }
    }

    /**
     * EXP-909: the order ONE device's logins list in — contract agent order
     * first (so claude's logins never interleave with codex's), then the
     * order the device SENDS its profiles in (a stable sort). A heartbeat can
     * move the numbers without reshuffling the rows. Byte-identical ×4.
     */
    fun sortDeviceLogins(rows: List<AgentProfileUsageRow>): List<AgentProfileUsageRow> {
        val order = DomainContract.codingAgentValues
        return rows.sortedWith(
            compareBy<AgentProfileUsageRow> {
                order.indexOf(it.agent).let { rank -> if (rank == -1) Int.MAX_VALUE else rank }
            }
                .thenBy { it.agent },
        )
    }

    /**
     * EXP-909: a login row's identity line — WHO the login is: its email, else
     * the bare plan an agent reports instead of an address ([accountName]).
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
     * nobody ever signed in to is "No email". A login has no other name.
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
     * EXP-1137: the server's refusal for a sign-out on a build without the
     * body. Byte-identical ×4.
     */
    const val SIGN_OUT_OLD_APP =
        "That machine runs an older Exponential app that cannot sign agent accounts out. Update it first."

    /**
     * EXP-862: what a login's chip menu offers, on a machine row or on an
     * account row — the SAME rule on every client, in this fixed order:
     *  - signed out, or a credential that expired here: a sign-in first;
     *  - EXP-1137, signed in on a build with the sign-out body: sign it out
     *    (the row stays);
     *  - remove: any login on a build with `account-remove`.
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
        canRemoveAccount: Boolean,
        canAgentLogin: Boolean,
        canSignOutAccount: Boolean = false,
    ): List<String> {
        val signsIn = !signedIn || health == AgentHealth.NeedsRelogin
        val out = mutableListOf<String>()
        if (signsIn) out += ACTION_SIGN_IN
        if (signedIn && canAgentLogin && canSignOutAccount) out += ACTION_SIGN_OUT
        if (canRemoveAccount && canAgentLogin) out += ACTION_REMOVE
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
     * EXP-1137: the sign-out confirm, pinned ×4 (web `signOutConfirmCopy`):
     * the login keeps its row.
     */
    fun signOutConfirm(accountLabel: String, deviceLabel: String): String =
        "Sign $accountLabel out on $deviceLabel? The login stays listed so it can sign in " +
            "again; the account itself is untouched."

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
     * What an open sign-in sheet compares against: profile id → its
     * `lastLoginAt`, captured when the `agent_login` command is queued.
     */
    fun loginBaseline(account: AgentAccount?): Map<String, String?> =
        account?.profiles.orEmpty()
            .filter { it.id.isNotBlank() }
            .associate { it.id to nonEmpty(it.lastLoginAt) }

    /**
     * A sign-in that committed on the device: the profile it landed on, its
     * email, and whether that profile was ALREADY there under another intent
     * (a duplicate — the device refreshed it, and the sheet says so).
     */
    data class LoginLanding(val profileId: String, val email: String?, val duplicate: Boolean)

    /**
     * Has a sign-in landed since [baseline] was taken? The ONE self-close rule
     * of the sign-in sheet (web `agentLoginLanding`, iOS `loginLanding`):
     * landed = a profile whose `lastLoginAt` is set and differs from the
     * baseline (a new id with one counts). A landing on a profile the baseline
     * already held is a DUPLICATE unless it is the very login the sign-in was
     * for ([intendedProfileId]; null = "Add account", where any existing
     * profile is a duplicate). Null while nothing has landed.
     */
    fun loginLanding(
        account: AgentAccount?,
        baseline: Map<String, String?>,
        intendedProfileId: String?,
    ): LoginLanding? {
        val landed = account?.profiles.orEmpty().firstOrNull { profile ->
            val stamp = nonEmpty(profile.lastLoginAt) ?: return@firstOrNull false
            profile.id.isNotBlank() && stamp != baseline[profile.id]
        } ?: return null
        val existed = landed.id in baseline
        val intended = intendedProfileId?.trim()?.takeIf { it.isNotEmpty() }
        return LoginLanding(
            profileId = landed.id,
            email = nonEmpty(landed.email),
            duplicate = existed && (intended == null || landed.id != intended),
        )
    }

    /** The duplicate sign-in's warning toast, byte-identical ×4 (device-doctor.json `alreadyAdded`). */
    fun alreadyAddedToast(email: String): String = "$email was already added. Refreshed it."

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
