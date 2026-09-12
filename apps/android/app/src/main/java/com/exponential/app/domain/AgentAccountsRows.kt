package com.exponential.app.domain

import com.exponential.app.data.api.AgentAccount
import com.exponential.app.data.api.AgentUsage
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceEntity

// EXP-829: the Devices page's Accounts section (EXP-818 folded the Usage page
// into Devices on web and the desktop; this is the Android third). One ROW
// per ACCOUNT — an agent plus the login the machines named — off the synced
// `devices` rows: own machines plus the servers teammates shared with the
// selected team. The machines holding the account are chips on the row, a
// chip wears a check where the account is the ACTIVE login there, and the
// numbers are the FRESHEST machine's report (they are the account's limits,
// so every machine reads the same ones).
//
// Hand-mirrored against web `lib/agent-usage.ts` (`agentProfileUsageRows` /
// `accountUsageGroups` / `sortAccountGroupsAttentionFirst` /
// `refreshAllowedAt`) and desktop `ui/src/usage_bar.rs`, same names, same
// tests (AgentAccountsRowsTest). Everything here is a pure function of the
// rows: online-ness and refresh eligibility are injected, never re-derived.

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
    /** The account is the machine's ACTIVE login (the ambient login always is). */
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

/** One account row of the section. */
data class AgentAccountUsageGroup(
    /**
     * `<agent>:<email>` for a named login; a row with no email (an agent that
     * names a provider, a signed-out row names nobody) can never be told apart from
     * another machine's, so it keeps its own `<agent>:<deviceId>:<profileId>`.
     */
    val key: String,
    val agent: String,
    val signedIn: Boolean,
    /**
     * EXP-849: the WORST health among the machines holding this account — the
     * badge the account row carries (one machine's expired credential is a
     * re-login, even if another machine's copy still works).
     */
    val health: AgentHealth,
    val email: String?,
    val plan: String?,
    /**
     * The machines (× profile) holding this account: online first, then by
     * label, then profile — a heartbeat cannot reshuffle the chips.
     */
    val rows: List<AgentProfileUsageRow>,
    /**
     * The FRESHEST member's numbers: newest `fetchedAt`, a non-stale report
     * winning a tie, a report with windows beating one without.
     */
    val usage: AgentUsage?,
    /** The newest probe stamp among the members — the "as of …" fallback. */
    val checkedAt: String?,
    /**
     * Where a refresh is queued: the eligible member (`canRefresh`) that
     * reported the freshest numbers, or null when no member may run one.
     */
    val refreshTarget: AgentProfileUsageRow?,
)

/** The rows of ONE agent, in the page's order. */
data class AgentAccountSection(
    val agent: String,
    val groups: List<AgentAccountUsageGroup>,
)

/**
 * EXP-849: one account a MACHINE holds, for the machine row's chips (web
 * `DeviceAccountChip`). The Devices surface is the SETUP/REPAIR surface, so
 * every chip names the agent, the login, whether it is that machine's ACTIVE
 * one and how healthy it is — the account-level view
 * ([AgentAccountsRows.agentProfileUsageRows]) groups ACROSS machines and is
 * the wrong shape for "what is wrong on this box".
 */
data class DeviceAccountChip(
    /** `<agent>:<profileId>` — stable within one device row. */
    val key: String,
    val agent: String,
    val profileId: String,
    /** The profile's label (`Default` for the ambient login). */
    val profileLabel: String,
    val signedIn: Boolean,
    val active: Boolean,
    val email: String?,
    val plan: String?,
    val health: AgentHealth,
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

    /** The section's empty state, byte-identical with web and the desktop. */
    const val EMPTY_STATE = "No machine has reported an agent account yet."

    /** The label for the system profile when the device sent none. */
    const val SYSTEM_PROFILE_LABEL = "Default"

    /**
     * Which synced rows the section reads (web `AgentAccountsSection`): the
     * caller's own machines plus the SERVERS shared with the team being
     * looked at — a teammate's shared server belongs to the pages of the
     * teams it is shared with (FEED-33: possibly several), not to every team
     * the caller is a member of. Signed out lists nothing.
     */
    fun sectionDevices(
        rows: List<DeviceEntity>,
        currentUserId: String?,
        teamId: String?,
    ): List<DeviceEntity> {
        if (currentUserId == null) return emptyList()
        return rows.filter {
            it.userId == currentUserId ||
                (teamId != null && it.sharedTeamIds.contains(teamId) && it.kind == SteerDevice.KIND_SERVER)
        }
    }

    /**
     * The rows the section renders for [devices], grouped by account later.
     * [isOnline] is decided by the caller's clock the same way every device
     * list does (`DeviceLiveness.isOnline`); it is passed in so the
     * derivation stays a pure function of the rows.
     */
    fun agentProfileUsageRows(
        devices: List<DeviceEntity>,
        currentUserId: String,
        isOnline: (String?) -> Boolean,
    ): List<AgentProfileUsageRow> {
        val out = mutableListOf<AgentProfileUsageRow>()
        for (device in devices) {
            val accounts = parseAgentAccounts(device.agentAccounts).orEmpty()
            val usageMap = parseAgentUsage(device.agentUsage).orEmpty()
            // The union of "has an account" and "reported usage": a machine
            // that only managed one of the two still gets its row.
            // EXP-849: an agent this build has no name for (a retired `pi`
            // still beating off an old daemon) is not a row, not a tab and
            // not a chip. The jsonb parse already drops it; the set is
            // filtered here too so a row built from a wire-decoded map can
            // never smuggle one in.
            val agents = LinkedHashSet<String>().apply {
                addAll(accounts.keys)
                addAll(usageMap.keys)
            }.filterTo(LinkedHashSet(), AgentUsagePresentation::isContractAgent)
            val mine = device.userId == currentUserId
            val online = isOnline(device.lastSeenAt)
            for (agent in agents) {
                val account = accounts[agent]
                val profiles = account?.profiles.orEmpty()
                if (profiles.isEmpty()) {
                    out += AgentProfileUsageRow(
                        key = "${device.deviceId}:$agent:$SYSTEM_PROFILE_ID",
                        deviceId = device.deviceId,
                        deviceLabel = device.label,
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
                        checkedAt = nonEmpty(account?.checkedAt) ?: nonEmpty(device.agentUsageAt),
                    )
                    continue
                }
                for (profile in profiles) {
                    // The active profile's numbers ride BOTH the profile entry
                    // and the pre-profile `agentUsage[agent]` slot; prefer the
                    // profile's own and fall back for a device that only
                    // populated the old slot.
                    val usage = profile.usage ?: usageMap[agent].takeIf { profile.active }
                    out += AgentProfileUsageRow(
                        key = "${device.deviceId}:$agent:${profile.id}",
                        deviceId = device.deviceId,
                        deviceLabel = device.label,
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
        return out
    }

    /**
     * EXP-849: every account ONE machine holds, agent by agent in contract
     * order, the ACTIVE login of each agent first — the chips a machine row
     * draws in "My machines" (web `deviceAccountChips`). A machine that
     * reported no profiles for an agent yields its single ambient account.
     * An agent this build has no name for is dropped, never chipped.
     */
    fun deviceAccountChips(accounts: Map<String, AgentAccount>?): List<DeviceAccountChip> {
        if (accounts.isNullOrEmpty()) return emptyList()
        val out = mutableListOf<DeviceAccountChip>()
        for (agent in DomainContract.codingAgentValues) {
            val account = accounts[agent] ?: continue
            val profiles = account.profiles.orEmpty().filter { it.id.isNotBlank() }
            if (profiles.isEmpty()) {
                out += DeviceAccountChip(
                    key = "$agent:$SYSTEM_PROFILE_ID",
                    agent = agent,
                    profileId = SYSTEM_PROFILE_ID,
                    profileLabel = SYSTEM_PROFILE_LABEL,
                    signedIn = account.signedIn,
                    active = true,
                    email = nonEmpty(account.email),
                    plan = nonEmpty(account.plan),
                    health = AgentHealthRules.of(account),
                )
                continue
            }
            out += profiles.map { profile ->
                DeviceAccountChip(
                    key = "$agent:${profile.id}",
                    agent = agent,
                    profileId = profile.id,
                    profileLabel = nonEmpty(profile.label)
                        ?: if (profile.id == SYSTEM_PROFILE_ID) SYSTEM_PROFILE_LABEL else profile.id,
                    signedIn = profile.signedIn,
                    active = profile.active,
                    email = nonEmpty(profile.email),
                    plan = nonEmpty(profile.plan),
                    health = AgentHealthRules.of(profile),
                )
            }.sortedByDescending { it.active }
        }
        return out
    }

    /**
     * `Claude · dev@acme.test` — the chip's text on a MACHINE row: the agent,
     * then the login (its email, else the bare plan an agent reports instead
     * of an address, else the profile's label). Web `machineChipLabel`, iOS
     * `DeviceAccountChips.caption`.
     */
    fun machineChipLabel(chip: DeviceAccountChip, agentLabel: (String) -> String): String {
        val who = chip.email
            ?: (if (chip.signedIn) (chip.plan ?: "signed in") else chip.profileLabel)
        return "${agentLabel(chip.agent)} · $who"
    }

    /**
     * EXP-849: the ONE repair a machine owes a login, as the chip menu's lead
     * entry — a healthy login the machine is not using simply BECOMES its
     * login (`agent_profile_use`, no credential touched), everything else is a
     * sign-in. Byte-identical with web `MachineAccountChip`.
     */
    fun chipAction(chip: DeviceAccountChip): String = when {
        !chip.signedIn -> "Sign in"
        chip.health == AgentHealth.NeedsRelogin -> "Re-login"
        chip.active -> "Sign in again"
        else -> "Use this account here"
    }

    /** Whether [chipAction] is the non-destructive active-login pick. */
    fun chipSwitchesTo(chip: DeviceAccountChip): Boolean =
        chip.signedIn && !chip.active && chip.health != AgentHealth.NeedsRelogin

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

    fun accountGroupKey(row: AgentProfileUsageRow): String {
        val email = if (row.signedIn) row.email?.trim()?.lowercase()?.takeIf { it.isNotEmpty() } else null
        return if (email != null) "${row.agent}:$email" else "${row.agent}:${row.deviceId}:${row.profileId}"
    }

    /**
     * Fold the page rows into account groups. [canRefresh] decides which
     * members may run `agent_usage_refresh` (mine + online + the cap) —
     * injected so the derivation stays a pure function of the rows. Nothing
     * is sorted across groups here: [sortAccountGroupsAttentionFirst] owns
     * that.
     */
    fun accountUsageGroups(
        rows: List<AgentProfileUsageRow>,
        canRefresh: (AgentProfileUsageRow) -> Boolean,
    ): List<AgentAccountUsageGroup> {
        val byKey = LinkedHashMap<String, AgentAccountUsageGroup>()
        for (row in rows) {
            val key = accountGroupKey(row)
            val group = byKey[key] ?: AgentAccountUsageGroup(
                key = key,
                agent = row.agent,
                signedIn = row.signedIn,
                health = row.health,
                email = row.email,
                plan = row.plan,
                rows = emptyList(),
                usage = null,
                checkedAt = null,
                refreshTarget = null,
            )
            byKey[key] = group.copy(
                rows = group.rows + row,
                // The worst health across the machines — one dead copy of the
                // credential is a re-login even if another machine's still works.
                health = if (AgentHealthRules.rank(row.health) < AgentHealthRules.rank(group.health)) {
                    row.health
                } else {
                    group.health
                },
                plan = group.plan ?: row.plan,
                usage = if (fresherUsage(row.usage, group.usage)) row.usage else group.usage,
                checkedAt = if (stampMs(row.checkedAt) > stampMs(group.checkedAt)) row.checkedAt else group.checkedAt,
                refreshTarget = if (
                    canRefresh(row) &&
                    (group.refreshTarget == null || fresherUsage(row.usage, group.refreshTarget.usage))
                ) {
                    row
                } else {
                    group.refreshTarget
                },
            )
        }
        return byKey.values.map { group ->
            group.copy(
                rows = group.rows.sortedWith(
                    compareByDescending<AgentProfileUsageRow> { it.online }
                        .thenBy { it.deviceLabel }
                        .thenBy { it.profileId },
                ),
            )
        }
    }

    /**
     * [attentionRank] over groups, then the fuller group, then agent, then
     * the key (email or device) — the page order.
     */
    fun sortAccountGroupsAttentionFirst(groups: List<AgentAccountUsageGroup>): List<AgentAccountUsageGroup> =
        groups.sortedWith(
            compareBy<AgentAccountUsageGroup> { attentionRank(it.signedIn, it.usage, it.health) }
                .thenByDescending { peakPercent(it.usage) }
                .thenBy { it.agent }
                .thenBy { it.key },
        )

    /**
     * Agent sections in CONTRACT order (`codingAgent` values), anything else
     * after it alphabetically — a section only exists once a machine has
     * reported the agent. The groups keep their attention-first order.
     */
    fun sections(groups: List<AgentAccountUsageGroup>): List<AgentAccountSection> {
        val order = DomainContract.codingAgentValues
        val byAgent = LinkedHashMap<String, MutableList<AgentAccountUsageGroup>>()
        for (group in groups) byAgent.getOrPut(group.agent) { mutableListOf() } += group
        val rank = { agent: String -> order.indexOf(agent).let { if (it == -1) Int.MAX_VALUE else it } }
        return byAgent.keys
            .sortedWith(compareBy<String> { rank(it) }.thenBy { it })
            .map { agent -> AgentAccountSection(agent, byAgent.getValue(agent)) }
    }

    /**
     * The `Studio · Personal` chip text: the machine, plus the profile when
     * it is not the ambient login. A label-less machine falls back to its id.
     */
    fun chipLabel(row: AgentProfileUsageRow): String {
        val device = row.deviceLabel.ifBlank { row.deviceId }
        return if (row.profileId == SYSTEM_PROFILE_ID) device else "$device · ${row.profileLabel}"
    }

    /**
     * The row's identity line: `Not signed in`, else the email, else the
     * plan (an agent may report a provider, never an address), else `signed in`.
     */
    fun caption(group: AgentAccountUsageGroup): String = when {
        !group.signedIn -> "Not signed in"
        else -> group.email ?: group.plan ?: "signed in"
    }

    /**
     * EXP-849: the account row's health badge, or null when there is nothing
     * to say. A signed-OUT row already says so in its [caption], so the badge
     * there would only repeat it — the one badge an account row wears is the
     * expired credential the caption cannot express.
     */
    fun healthBadge(group: AgentAccountUsageGroup): String? =
        if (!group.signedIn) null else AgentHealthRules.badgeLabel(group.health)

    private fun nonEmpty(value: String?): String? = value?.trim()?.takeIf { it.isNotEmpty() }

    private fun stampMs(stamp: String?): Long =
        stamp?.let(WireTimestamps::parseEpochMs) ?: Long.MIN_VALUE

    /** Whether [candidate] is a fresher report than [current]. */
    private fun fresherUsage(candidate: AgentUsage?, current: AgentUsage?): Boolean {
        if (candidate == null) return false
        if (current == null) return true
        val byStamp = stampMs(candidate.fetchedAt).compareTo(stampMs(current.fetchedAt))
        if (byStamp != 0) return byStamp > 0
        if (candidate.stale != current.stale) return current.stale
        return candidate.windows.isNotEmpty() && current.windows.isEmpty()
    }
}
