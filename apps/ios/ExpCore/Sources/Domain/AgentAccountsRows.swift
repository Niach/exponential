import Foundation

/// EXP-829: the Devices page's **Accounts** section — the rules behind it.
///
/// EXP-818 folded the Usage page into Devices on web and desktop: ONE row per
/// agent ACCOUNT (an agent plus the login the machines named) off the synced
/// `devices` rows, the machines holding it as chips (a check where it is the
/// ACTIVE login), the FRESHEST machine's numbers as the bars. This is the
/// mobile mirror of that derivation, hand-mirrored against web
/// `lib/agent-usage.ts` (`agentProfileUsageRows` / `accountUsageGroups` /
/// `sortAccountGroupsAttentionFirst` / `refreshAllowedAt`) and desktop
/// `ui/src/usage_bar.rs` + `accounts_section.rs` — same names, same
/// fallbacks, same test names. Change a rule here, change it there.
///
/// Pure: nothing here reads the clock (every stamp is compared against a
/// `now` the caller passes) and nothing fetches — the device is the only
/// writer of `agentAccounts` / `agentUsage`.

/// One machine × agent × login profile, as the devices row reported it.
public struct AgentProfileUsageRow: Equatable, Sendable, Identifiable {
    /// `<deviceId>:<agent>:<profileId>` — stable enough to key a list.
    public let key: String
    public let deviceId: String
    public let deviceLabel: String
    /// One of the caller's own machines (a refresh is only ever queued on
    /// those, and only their chips open the settings sheet).
    public let mine: Bool
    public let online: Bool
    public let agent: String
    public let profileId: String
    /// The profile's label (`Default` for the ambient login when the device
    /// sent none).
    public let profileLabel: String
    /// Whether this profile is the machine's CURRENT login for the agent.
    public let active: Bool
    public let signedIn: Bool
    public let email: String?
    public let plan: String?
    public let usage: AgentUsage?
    /// The "as of …" fallback when the usage is stale or absent.
    public let checkedAt: String?
    /// EXP-849: how the machine's last probe of THIS login went, already
    /// derived (`AgentAccountHealth.resolve`) — a row never carries the raw
    /// wire token.
    public let health: AgentAccountHealth

    public var id: String { key }

    public init(
        key: String,
        deviceId: String,
        deviceLabel: String,
        mine: Bool,
        online: Bool,
        agent: String,
        profileId: String,
        profileLabel: String,
        active: Bool,
        signedIn: Bool,
        email: String?,
        plan: String?,
        usage: AgentUsage?,
        checkedAt: String?,
        health: AgentAccountHealth = .unknown
    ) {
        self.key = key
        self.deviceId = deviceId
        self.deviceLabel = deviceLabel
        self.mine = mine
        self.online = online
        self.agent = agent
        self.profileId = profileId
        self.profileLabel = profileLabel
        self.active = active
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.usage = usage
        self.checkedAt = checkedAt
        self.health = health
    }
}

/// One ACCOUNT: the rows above folded by login. EXP-817's rule ×4.
public struct AgentAccountUsageGroup: Equatable, Sendable, Identifiable {
    /// `<agent>:<email>` for a named login; a row with no email (an agent that
    /// names a provider, a signed-out row that names nobody) can never be told
    /// apart from another machine's, so it keeps its own
    /// `<agent>:<deviceId>:<profileId>`.
    public let key: String
    public let agent: String
    public let signedIn: Bool
    public let email: String?
    public var plan: String?
    /// The machines (× profile) holding this account: online first, then by
    /// label, then profile — a heartbeat cannot reshuffle the chips.
    public var rows: [AgentProfileUsageRow]
    /// The FRESHEST member's numbers: newest `fetchedAt`, a non-stale report
    /// winning a tie, a report with windows beating one without.
    public var usage: AgentUsage?
    /// The newest probe stamp among the members — the "as of …" fallback.
    public var checkedAt: String?
    /// Where a refresh is queued: the eligible member (`canRefresh`) that
    /// reported the freshest numbers, or nil when no member may run one.
    public var refreshTarget: AgentProfileUsageRow?
    /// EXP-849: the WORST health among the machines holding this account — a
    /// login that works on one machine and is expired on another reads as
    /// needing a re-login, because it does.
    public var health: AgentAccountHealth

    public var id: String { key }

    public init(
        key: String,
        agent: String,
        signedIn: Bool,
        email: String?,
        plan: String?,
        rows: [AgentProfileUsageRow],
        usage: AgentUsage?,
        checkedAt: String?,
        refreshTarget: AgentProfileUsageRow?,
        health: AgentAccountHealth = .unknown
    ) {
        self.key = key
        self.agent = agent
        self.signedIn = signedIn
        self.email = email
        self.plan = plan
        self.rows = rows
        self.usage = usage
        self.checkedAt = checkedAt
        self.refreshTarget = refreshTarget
        self.health = health
    }
}

/// The section's agent bands: contract order first, anything else after.
public struct AgentAccountSection: Equatable, Sendable, Identifiable {
    public let agent: String
    public let groups: [AgentAccountUsageGroup]

    public var id: String { agent }

    public init(agent: String, groups: [AgentAccountUsageGroup]) {
        self.agent = agent
        self.groups = groups
    }
}

public enum AgentAccountsRows {
    /// The ambient login's profile id — byte-identical with web
    /// `SYSTEM_PROFILE_ID` and the desktop's `agent_profiles::SYSTEM_PROFILE`.
    public static let systemProfileId = "system"

    /// The `agent-usage-refresh` device cap: the machine runs
    /// `agent_usage_refresh` (web `deviceCanRefreshUsage`).
    public static let refreshCap = "agent-usage-refresh"

    /// A forced usage refresh is refused while the last fetch is younger than
    /// this: the device never hits the agent's usage endpoint more often (its
    /// `RATE_LIMITED_FLOOR_SECS`). Locked ×4.
    public static let rateLimitedFloor: TimeInterval = 5 * 60

    // MARK: - Rows off the devices shape

    /// The rows the section renders for `devices` — one per machine × agent ×
    /// profile. A device that reports no profiles (an older build, or a
    /// single-login install) gets exactly one row per agent: the ambient
    /// `system` profile, labelled "Default", carrying the top-level account
    /// and the pre-profile `agentUsage` slot; an agent that reported ONLY
    /// usage still gets its row, signed out, dated by the row's
    /// `agent_usage_at`. `isOnline` is the caller's clock (`DeviceLiveness`),
    /// passed in so the derivation stays a pure function of the rows.
    public static func profileRows(
        devices: [DeviceEntity],
        currentUserId: String?,
        isOnline: (String?) -> Bool
    ) -> [AgentProfileUsageRow] {
        var out: [AgentProfileUsageRow] = []
        for device in devices {
            let accounts = AgentUsagePresentation.parseAccounts(device.agentAccounts) ?? [:]
            let usageMap = AgentUsagePresentation.parseMap(device.agentUsage) ?? [:]
            // EXP-849: an agent this build has no name for (a retired `pi`
            // still beating off an old daemon) is not a row, not a tab and
            // not a chip. The row mapping already drops it; the set is
            // filtered here too, so a caller that parsed the jsonb itself
            // cannot smuggle one in.
            let agents = orderedAgents(
                Set(accounts.keys).union(usageMap.keys)
                    .filter(AgentUsagePresentation.isContractAgent)
            )
            let mine = currentUserId != nil && device.userId == currentUserId
            let online = isOnline(device.lastSeenAt)
            for agent in agents {
                let account = accounts[agent]
                let profiles = account?.profiles ?? []
                if profiles.isEmpty {
                    out.append(AgentProfileUsageRow(
                        key: "\(device.deviceId):\(agent):\(systemProfileId)",
                        deviceId: device.deviceId,
                        deviceLabel: device.label,
                        mine: mine,
                        online: online,
                        agent: agent,
                        profileId: systemProfileId,
                        profileLabel: "Default",
                        active: true,
                        signedIn: account?.signedIn == true,
                        email: nonEmpty(account?.email),
                        plan: nonEmpty(account?.plan),
                        usage: usageMap[agent],
                        checkedAt: nonEmpty(account?.checkedAt) ?? nonEmpty(device.agentUsageAt),
                        // EXP-849: an agent the machine reported ONLY usage
                        // for has no account entry at all, which is `unknown`
                        // — `AgentAccountHealth.of(nil)` says exactly that.
                        health: AgentAccountHealth.of(account)
                    ))
                    continue
                }
                for profile in profiles {
                    // The active profile's numbers ride BOTH the profile entry
                    // and the pre-profile `agentUsage[agent]` slot; prefer the
                    // profile's own and fall back for a device that only
                    // populated the old slot.
                    let usage = profile.usage
                        ?? (profile.active == true ? usageMap[agent] : nil)
                    out.append(AgentProfileUsageRow(
                        key: "\(device.deviceId):\(agent):\(profile.id)",
                        deviceId: device.deviceId,
                        deviceLabel: device.label,
                        mine: mine,
                        online: online,
                        agent: agent,
                        profileId: profile.id,
                        profileLabel: nonEmpty(profile.label)
                            ?? (profile.id == systemProfileId ? "Default" : profile.id),
                        active: profile.active == true,
                        signedIn: profile.signedIn == true,
                        email: nonEmpty(profile.email),
                        plan: nonEmpty(profile.plan),
                        usage: usage,
                        checkedAt: nonEmpty(profile.checkedAt) ?? nonEmpty(account?.checkedAt),
                        // EXP-849: the profile's own probe outcome; a profile
                        // entry that reports none derives from its `signedIn`.
                        health: AgentAccountHealth.of(profile)
                    ))
                }
            }
        }
        return out
    }

    /// The fullest window's percent, or 0 for a row with no usage at all.
    public static func peakPercent(_ usage: AgentUsage?) -> Double {
        (usage?.windows ?? []).reduce(0) { max($0, $1.percent ?? 0) }
    }

    /// Attention-first ordering: rows with something to DO lead (EXP-849: a
    /// signed-out login, or one the agent refused — `needs_relogin`), then
    /// rows at or over the danger threshold, then everything else. `health`
    /// defaults to `unknown` so a caller that has none keeps the pre-EXP-849
    /// ordering exactly.
    public static func attentionRank(
        signedIn: Bool,
        usage: AgentUsage?,
        health: AgentAccountHealth = .unknown
    ) -> Int {
        if !signedIn || health.needsAttention { return 0 }
        if AgentUsagePresentation.severity(peakPercent(usage)) == .danger { return 1 }
        return 2
    }

    /// `attentionRank`, then the fuller row, then device label, agent,
    /// profile — so a heartbeat cannot shuffle equal rows.
    public static func sortAttentionFirst(_ rows: [AgentProfileUsageRow]) -> [AgentProfileUsageRow] {
        rows.sorted { a, b in
            let rankA = attentionRank(signedIn: a.signedIn, usage: a.usage, health: a.health)
            let rankB = attentionRank(signedIn: b.signedIn, usage: b.usage, health: b.health)
            if rankA != rankB { return rankA < rankB }
            let peakA = peakPercent(a.usage)
            let peakB = peakPercent(b.usage)
            if peakA != peakB { return peakA > peakB }
            if let ordered = before(a.deviceLabel, b.deviceLabel) { return ordered }
            if let ordered = before(a.agent, b.agent) { return ordered }
            return before(a.profileId, b.profileId) ?? false
        }
    }

    /// When a forced refresh is next allowed for `usage`: nil = right now (no
    /// fetch on record, or the last one is older than the floor). A stamp in
    /// the future (the machine's clock runs ahead) is treated as "just
    /// fetched".
    public static func refreshAllowedAt(_ usage: AgentUsage?, now: Date) -> Date? {
        guard let fetchedAt = usage?.fetchedAt, let fetched = WireTimestamps.parse(fetchedAt) else {
            return nil
        }
        let next = fetched.addingTimeInterval(rateLimitedFloor)
        return next > now ? next : nil
    }

    // MARK: - One group per account (EXP-817)

    public static func accountGroupKey(_ row: AgentProfileUsageRow) -> String {
        let email = row.signedIn
            ? row.email?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            : nil
        if let email, !email.isEmpty { return "\(row.agent):\(email)" }
        return "\(row.agent):\(row.deviceId):\(row.profileId)"
    }

    /// Fold the rows into account groups, in first-seen order. `canRefresh`
    /// decides which members may run `agent_usage_refresh` (mine + online +
    /// the cap) — injected so the derivation stays pure. Nothing is sorted
    /// across groups here: `sortGroupsAttentionFirst` owns that.
    public static func accountGroups(
        _ rows: [AgentProfileUsageRow],
        canRefresh: (AgentProfileUsageRow) -> Bool
    ) -> [AgentAccountUsageGroup] {
        var order: [String] = []
        var byKey: [String: AgentAccountUsageGroup] = [:]
        for row in rows {
            let key = accountGroupKey(row)
            var group: AgentAccountUsageGroup
            if let existing = byKey[key] {
                group = existing
            } else {
                order.append(key)
                group = AgentAccountUsageGroup(
                    key: key,
                    agent: row.agent,
                    signedIn: row.signedIn,
                    email: row.email,
                    plan: row.plan,
                    rows: [],
                    usage: nil,
                    checkedAt: nil,
                    refreshTarget: nil,
                    // Folded below as every member joins: the worst wins.
                    health: row.health
                )
            }
            group.rows.append(row)
            if group.plan == nil, let plan = row.plan { group.plan = plan }
            group.health = AgentAccountHealth.worst([group.health, row.health]) ?? group.health
            if fresherUsage(row.usage, than: group.usage) { group.usage = row.usage }
            if stamp(row.checkedAt) > stamp(group.checkedAt) { group.checkedAt = row.checkedAt }
            if canRefresh(row),
               group.refreshTarget == nil || fresherUsage(row.usage, than: group.refreshTarget?.usage) {
                group.refreshTarget = row
            }
            byKey[key] = group
        }
        return order.compactMap { key in
            guard var group = byKey[key] else { return nil }
            group.rows.sort { a, b in
                if a.online != b.online { return a.online }
                if let ordered = before(a.deviceLabel, b.deviceLabel) { return ordered }
                return before(a.profileId, b.profileId) ?? false
            }
            return group
        }
    }

    /// `attentionRank` over groups, then the fuller group, then agent, then
    /// the key (email or device) — the section order.
    public static func sortGroupsAttentionFirst(_ groups: [AgentAccountUsageGroup]) -> [AgentAccountUsageGroup] {
        groups.sorted { a, b in
            let rankA = attentionRank(signedIn: a.signedIn, usage: a.usage, health: a.health)
            let rankB = attentionRank(signedIn: b.signedIn, usage: b.usage, health: b.health)
            if rankA != rankB { return rankA < rankB }
            let peakA = peakPercent(a.usage)
            let peakB = peakPercent(b.usage)
            if peakA != peakB { return peakA > peakB }
            if let ordered = before(a.agent, b.agent) { return ordered }
            return before(a.key, b.key) ?? false
        }
    }

    /// The agent bands: contract `codingAgent` order first, an agent this
    /// build has no name for after the known ones, alphabetically. A band
    /// only exists when a machine reported the agent. Group order within a
    /// band is preserved (sort the groups first).
    public static func sections(_ groups: [AgentAccountUsageGroup]) -> [AgentAccountSection] {
        var order: [String] = []
        var byAgent: [String: [AgentAccountUsageGroup]] = [:]
        for group in groups {
            if byAgent[group.agent] == nil { order.append(group.agent) }
            byAgent[group.agent, default: []].append(group)
        }
        return orderedAgents(Set(order)).map { agent in
            AgentAccountSection(agent: agent, groups: byAgent[agent] ?? [])
        }
    }

    // MARK: - Per-device health (EXP-849)

    /// The badge a MACHINE row wears: the worst health among the logins that
    /// machine reported (web `deviceWorstHealth`, Android `deviceWorst`).
    /// `unknown` when it reported none — web's nil, and the same outcome: a
    /// device row badges only `needsAttention` states, so an unprobed machine
    /// stays quiet either way.
    public static func deviceHealth(
        _ rows: [AgentProfileUsageRow],
        deviceId: String
    ) -> AgentAccountHealth {
        AgentAccountHealth.worst(rows.filter { $0.deviceId == deviceId }.map(\.health))
            ?? .unknown
    }

    /// The machine's logins, as the chips a Devices row draws: attention
    /// first, then the same order the account rows use for their chips
    /// (online, label, profile — and here agent before profile, since one
    /// machine holds several agents).
    public static func deviceRows(
        _ rows: [AgentProfileUsageRow],
        deviceId: String
    ) -> [AgentProfileUsageRow] {
        rows.filter { $0.deviceId == deviceId }.sorted { a, b in
            let rankA = attentionRank(signedIn: a.signedIn, usage: a.usage, health: a.health)
            let rankB = attentionRank(signedIn: b.signedIn, usage: b.usage, health: b.health)
            if rankA != rankB { return rankA < rankB }
            if let ordered = before(a.agent, b.agent) { return ordered }
            if a.active != b.active { return a.active }
            return before(a.profileLabel, b.profileLabel) ?? false
        }
    }

    // MARK: - Row copy

    /// The `Studio · Personal` chip text: the machine, plus the profile when
    /// it is not the ambient login.
    public static func chipLabel(_ row: AgentProfileUsageRow) -> String {
        let device = row.deviceLabel.isEmpty ? row.deviceId : row.deviceLabel
        return row.profileId == systemProfileId ? device : "\(device) · \(row.profileLabel)"
    }

    /// The row's title: `Not signed in`, else the email, else the bare plan
    /// (an agent that reports a provider, never an address), else `signed in`.
    public static func groupCaption(_ group: AgentAccountUsageGroup) -> String {
        guard group.signedIn else { return "Not signed in" }
        return group.email ?? group.plan ?? "signed in"
    }

    /// The `account-switch` device cap: the machine handles
    /// `agent_profile_use`. Shipped in desktop/CLI 0.14.38 — the server
    /// REFUSES the command (`PRECONDITION_FAILED`) for a machine that does not
    /// advertise it, so offering the switch there would only produce a
    /// refusal. `SteerDevice.canSwitchAccount` reads the cap.
    public static let switchCap = "account-switch"

    /// EXP-849: the ONE repair a MACHINE owes a login, as its chip menu's lead
    /// entry — a healthy login the machine is not using simply BECOMES its
    /// login (`agent_profile_use`: no login flow, no logout, no credential
    /// touched), everything else is a sign-in. Byte-identical with web
    /// `MachineAccountChip` and Android `chipAction`.
    ///
    /// `canSwitchAccount` is the machine's `account-switch` cap: without it the
    /// switch falls through to a sign-in, because the server would refuse the
    /// command.
    public static func chipAction(
        _ row: AgentProfileUsageRow,
        canSwitchAccount: Bool
    ) -> String {
        if !row.signedIn { return "Sign in" }
        if row.health == .needsRelogin { return "Re-login" }
        return chipSwitchesTo(row, canSwitchAccount: canSwitchAccount)
            ? "Use this account here"
            : "Sign in again"
    }

    /// Whether `chipAction` is the non-destructive active-login pick rather
    /// than a sign-in. An EXPIRED credential is never switched to: it would
    /// not work — it gets re-signed-in instead. Neither is a login on a
    /// machine whose build has no `agent_profile_use`: the server refuses that
    /// one before it ever reaches the machine.
    public static func chipSwitchesTo(
        _ row: AgentProfileUsageRow,
        canSwitchAccount: Bool
    ) -> Bool {
        canSwitchAccount && row.signedIn && !row.active && row.health != .needsRelogin
    }

    /// EXP-849: the account row's health badge, or nil when there is nothing to
    /// say. A signed-OUT row already says so in its `groupCaption`, so a badge
    /// there would only repeat it — what a row wears is the expired credential
    /// the caption cannot express (Android `healthBadge`, web
    /// `accountHealthBadge`).
    public static func healthBadge(_ group: AgentAccountUsageGroup) -> String? {
        guard group.signedIn else { return nil }
        return group.health.badgeLabel
    }

    // MARK: - Internals

    /// Whether `candidate` is a fresher report than `current`.
    static func fresherUsage(_ candidate: AgentUsage?, than current: AgentUsage?) -> Bool {
        guard let candidate else { return false }
        guard let current else { return true }
        let byStamp = stamp(candidate.fetchedAt) - stamp(current.fetchedAt)
        if byStamp != 0 { return byStamp > 0 }
        if candidate.stale != current.stale { return current.stale == true }
        return !(candidate.windows ?? []).isEmpty && (current.windows ?? []).isEmpty
    }

    /// A stamp as seconds since the epoch; nothing / unreadable = -∞, so it
    /// never beats a real one. Two unreadable stamps differ by NaN, which
    /// `fresherUsage` reads exactly like the web mirror does: neither is
    /// fresher, and the tie-breaks below it do not run.
    private static func stamp(_ value: String?) -> Double {
        guard let value, let date = WireTimestamps.parse(value) else { return -.infinity }
        return date.timeIntervalSince1970
    }

    private static func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    /// Contract agents in contract order, then anything else alphabetically.
    private static func orderedAgents(_ agents: Set<String>) -> [String] {
        let known = DomainContract.codingAgentValues.filter { agents.contains($0) }
        let rest = agents.subtracting(known).sorted { before($0, $1) ?? false }
        return known + rest
    }

    /// `localeCompare` as a strict-before test: nil when the two are equal so
    /// the caller falls through to its next key.
    private static func before(_ a: String, _ b: String) -> Bool? {
        switch a.localizedStandardCompare(b) {
        case .orderedAscending: return true
        case .orderedDescending: return false
        case .orderedSame: return nil
        }
    }
}
