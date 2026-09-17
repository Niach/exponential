import Foundation

/// EXP-829/EXP-909: the LOGINS under a Devices row — the rules behind them.
///
/// EXP-909 folded the cross-device "Accounts" section away on all four
/// clients: it merged by email logins that are per MACHINE, so one account
/// lived twice, with two orderings, two health badges and two menus. What is
/// left is the derivation that always mattered — one row per machine × agent ×
/// login profile, listed under the machine that holds it — hand-mirrored
/// against web `lib/agent-usage.ts` (`agentProfileUsageRows` /
/// `deviceLoginRows` / `sortDeviceLogins` / `loginLabel` / `refreshAllowedAt`)
/// and desktop `ui/src/usage_bar.rs`: same names, same fallbacks, same test
/// names. Change a rule here, change it there.
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

public enum AgentAccountsRows {
    /// The ambient login's profile id — byte-identical with web
    /// `SYSTEM_PROFILE_ID` and the desktop's `agent_profiles::SYSTEM_PROFILE`.
    public static let systemProfileId = "system"

    /// What the ambient login is CALLED when the device sent no label —
    /// Android's `SYSTEM_PROFILE_LABEL`, and the last fallback an account row's
    /// title takes.
    public static let systemProfileLabel = "Default"

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
        devices.flatMap { device in
            rows(
                deviceId: device.deviceId,
                deviceLabel: device.label,
                mine: currentUserId != nil && device.userId == currentUserId,
                online: isOnline(device.lastSeenAt),
                accounts: AgentUsagePresentation.parseAccounts(device.agentAccounts) ?? [:],
                usageMap: AgentUsagePresentation.parseMap(device.agentUsage) ?? [:],
                agentUsageAt: device.agentUsageAt
            )
        }
    }

    /// EXP-909: the logins ONE machine reported, as its Devices-page sub-rows
    /// — the same derivation `profileRows` runs, entered per device off the
    /// COMPOSED row (which already parsed the jsonb), then ordered by
    /// `sortDeviceLogins`. No new type and no second rule: the cross-device
    /// Accounts section is gone, so this is the only caller shape left beside
    /// the flat one. Web/Android `deviceLoginRows`, desktop
    /// `agent_profile_usage_rows(slice::from_ref(row), …)`.
    public static func deviceLoginRows(_ device: SteerDevice) -> [AgentProfileUsageRow] {
        sortDeviceLogins(rows(
            deviceId: device.deviceId,
            deviceLabel: device.deviceLabel,
            mine: device.isMine,
            online: device.isOnline,
            accounts: device.agentAccounts ?? [:],
            usageMap: device.agentUsage ?? [:],
            agentUsageAt: device.agentUsageAt
        ))
    }

    /// The shared core both entry points run: one machine's reported accounts
    /// and usage → its rows, in contract agent order, unsorted otherwise.
    private static func rows(
        deviceId: String,
        deviceLabel: String,
        mine: Bool,
        online: Bool,
        accounts: [String: AgentAccount],
        usageMap: [String: AgentUsage],
        agentUsageAt: String?
    ) -> [AgentProfileUsageRow] {
        var out: [AgentProfileUsageRow] = []
        // EXP-849: an agent this build has no name for (a retired `pi`
        // still beating off an old daemon) is not a row, not a tab and
        // not a chip. The row mapping already drops it; the set is
        // filtered here too, so a caller that parsed the jsonb itself
        // cannot smuggle one in.
        let agents = orderedAgents(
            Set(accounts.keys).union(usageMap.keys)
                .filter(AgentUsagePresentation.isContractAgent)
        )
        for agent in agents {
            let account = accounts[agent]
            let profiles = account?.profiles ?? []
            if profiles.isEmpty {
                out.append(AgentProfileUsageRow(
                    key: "\(deviceId):\(agent):\(systemProfileId)",
                    deviceId: deviceId,
                    deviceLabel: deviceLabel,
                    mine: mine,
                    online: online,
                    agent: agent,
                    profileId: systemProfileId,
                    profileLabel: systemProfileLabel,
                    active: true,
                    signedIn: account?.signedIn == true,
                    email: nonEmpty(account?.email),
                    plan: nonEmpty(account?.plan),
                    usage: usageMap[agent],
                    checkedAt: nonEmpty(account?.checkedAt) ?? nonEmpty(agentUsageAt),
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
                    key: "\(deviceId):\(agent):\(profile.id)",
                    deviceId: deviceId,
                    deviceLabel: deviceLabel,
                    mine: mine,
                    online: online,
                    agent: agent,
                    profileId: profile.id,
                    profileLabel: nonEmpty(profile.label)
                        ?? (profile.id == systemProfileId ? systemProfileLabel : profile.id),
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

    // MARK: - One device's logins (EXP-909)

    /// The order a machine's logins read in under its Devices row: contract
    /// agent order first (claude before codex, an unknown agent last), then
    /// the machine's ACTIVE login for that agent, then `attentionRank` (a
    /// signed-out or refused login leads the rest), then the profile label and
    /// its id — so a heartbeat can never reshuffle equal rows.
    ///
    /// The active login leads DELIBERATELY, before attention: this list
    /// answers "what is this machine running right now", and the broken
    /// sibling below it still wears its badge. Locked ×4 by `device logins
    /// lead with the active login, in contract agent order`.
    public static func sortDeviceLogins(_ rows: [AgentProfileUsageRow]) -> [AgentProfileUsageRow] {
        rows.sorted { a, b in
            if a.agent != b.agent {
                let rankA = agentRank(a.agent)
                let rankB = agentRank(b.agent)
                if rankA != rankB { return rankA < rankB }
                return before(a.agent, b.agent) ?? false
            }
            if a.active != b.active { return a.active }
            let attentionA = attentionRank(signedIn: a.signedIn, usage: a.usage, health: a.health)
            let attentionB = attentionRank(signedIn: b.signedIn, usage: b.usage, health: b.health)
            if attentionA != attentionB { return attentionA < attentionB }
            if let ordered = before(a.profileLabel, b.profileLabel) { return ordered }
            return before(a.profileId, b.profileId) ?? false
        }
    }

    /// A login's title on its device row: who it IS — the email, else the bare
    /// plan (an agent that names a provider instead of an address), else the
    /// profile's own label. NEVER a status: the brand mark says which agent,
    /// the row above says which machine, and the health badge says whether it
    /// still works. Locked ×4 by `the login label is the identity, never the
    /// status`.
    public static func loginLabel(_ row: AgentProfileUsageRow) -> String {
        row.email ?? row.plan ?? row.profileLabel
    }

    /// The badge a LOGIN row wears, or nil when there is nothing to say —
    /// `Needs re-login` / `Signed out`, the same two states the account rows
    /// used to badge.
    public static func healthBadge(_ row: AgentProfileUsageRow) -> String? {
        row.health.badgeLabel
    }

    /// Contract `codingAgent` position; an agent this build has no name for
    /// sorts after every known one (the caller then falls through to the name).
    private static func agentRank(_ agent: String) -> Int {
        DomainContract.codingAgentValues.firstIndex(of: agent)
            ?? DomainContract.codingAgentValues.count
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

    /// The machine's logins, in the order its Devices row lists them
    /// (`sortDeviceLogins`: contract agent order, its active login first).
    public static func deviceRows(
        _ rows: [AgentProfileUsageRow],
        deviceId: String
    ) -> [AgentProfileUsageRow] {
        sortDeviceLogins(rows.filter { $0.deviceId == deviceId })
    }

    // MARK: - Row copy

    /// The `account-switch` device cap: the machine handles
    /// `agent_profile_use`. Shipped in desktop/CLI 0.14.38 — the server
    /// REFUSES the command (`PRECONDITION_FAILED`) for a machine that does not
    /// advertise it, so offering the switch there would only produce a
    /// refusal. `SteerDevice.canSwitchAccount` reads the cap.
    public static let switchCap = "account-switch"

    /// EXP-862: the `account-remove` device cap — the machine runs
    /// `agent_profile_remove` and deletes its own copy of a login. Its own cap
    /// beside `agent-login`, because an older build would leave the queued row
    /// pending forever.
    public static let removeCap = "account-remove"

    /// EXP-862: a chip whose one repair is a SIGN-IN — there is no login on
    /// that machine, or the agent refused the one there. Both read the same to
    /// a person: sign in. (Web `MachineAccountChip`, Android `chipSignsIn`.)
    public static func chipSignsIn(_ row: AgentProfileUsageRow) -> Bool {
        !row.signedIn || row.health == .needsRelogin
    }

    /// EXP-862: whether the chip menu offers "Set as default" — a HEALTHY login
    /// the machine is not currently using simply BECOMES its login
    /// (`agent_profile_use`: no login flow, no logout, no credential touched).
    /// An expired one is never switched to (it would not work: it signs in
    /// instead), and neither is a login on a machine whose build has no
    /// `agent_profile_use` — the server refuses that before it reaches the
    /// machine.
    public static func chipSetsDefault(
        _ row: AgentProfileUsageRow,
        canSwitchAccount: Bool
    ) -> Bool {
        canSwitchAccount && !chipSignsIn(row) && !row.active
    }

    /// Byte-identical with the server's refusal (`lib/trpc/devices.ts`), so a
    /// requester that raced a downgrade reads the same sentence twice.
    public static let removeAccountOldApp =
        "That machine runs an older Exponential app that cannot remove agent accounts. Update it first."

    /// EXP-862: why "Remove account" is NOT offered for this login on this
    /// machine, or nil when it is — the web `removeAccountBlockReason` twin,
    /// TWO refusals in the order a person would hit them: the ambient login
    /// (the agent CLI's own config dir, which Exponential never created and
    /// must not delete), and a machine whose build cannot run the command.
    ///
    /// EXP-944: being signed OUT is no longer one of them. A dead profile is
    /// the thing people most want gone, the removal is a profile-dir delete
    /// that never touches the account (no `codex logout`, ever), and the
    /// server has always taken it — it gates on the ambient id, the caps and
    /// the reported profile, never on the credential's state. So a signed-out
    /// named login offers "Sign in" AND "Remove account".
    public static func removeAccountBlockReason(
        _ row: AgentProfileUsageRow,
        canAgentLogin: Bool,
        canRemoveAccount: Bool
    ) -> String? {
        if row.profileId.isEmpty || row.profileId == systemProfileId {
            return "That is the machine's own agent login, not one Exponential can remove."
        }
        if !canAgentLogin || !canRemoveAccount { return removeAccountOldApp }
        return nil
    }

    /// Whether the chip menu offers "Remove account" for this login.
    public static func canRemoveAccount(
        _ row: AgentProfileUsageRow,
        canAgentLogin: Bool,
        canRemoveAccount: Bool
    ) -> Bool {
        removeAccountBlockReason(
            row, canAgentLogin: canAgentLogin, canRemoveAccount: canRemoveAccount
        ) == nil
    }

    /// The confirm the destructive entry asks, pinned ×4: it names the login
    /// and the machine, and says in the same breath that the account survives.
    public static func removeAccountConfirmCopy(
        account: String,
        device: String
    ) -> String {
        "Delete \(account) on \(device)? The login is removed from this device only; the account itself is untouched."
    }

    // MARK: - Adding a login (EXP-827/EXP-862)

    /// The server's clamp on a profile label (web `MAX_PROFILE_LABEL`).
    public static let maxProfileLabel = 64

    /// Trimmed and cut to the server's limit, so the label the machine names
    /// its new config dir with is the one that was asked for.
    public static func clampProfileLabel(_ label: String) -> String {
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.count > maxProfileLabel
            ? String(trimmed.prefix(maxProfileLabel))
            : trimmed
    }

    /// WHERE a queued `agent_login` lands on the machine. Exactly one of the
    /// two is ever set: the machine reads neither as its AMBIENT config dir
    /// (`LoginTarget::System` in `coding/src/agent_login.rs`), which would
    /// sign a second account in ON TOP of the login already there.
    public struct AddLoginTarget: Equatable, Sendable {
        public let profileId: String?
        public let newProfileLabel: String?

        public init(profileId: String?, newProfileLabel: String?) {
            self.profileId = profileId
            self.newProfileLabel = newProfileLabel
        }
    }

    /// Whether the machine's AMBIENT login for an agent is taken — its own
    /// `system` profile when it reports profiles, the top-level flag for a
    /// machine that reports none.
    public static func ambientSignedIn(_ account: AgentAccount?) -> Bool {
        guard let account else { return false }
        guard let ambient = (account.profiles ?? []).first(where: {
            $0.id == systemProfileId
        }) else {
            return account.signedIn == true
        }
        return ambient.signedIn == true
    }

    /// Where a new login lands on a machine (web `addAccountLoginTarget`,
    /// Android `addAccountLoginTarget`): the ambient login while it is still
    /// signed out — nothing to keep beside it — otherwise a NEW profile
    /// carrying `label`, which the machine creates first (EXP-792's per-agent
    /// config dirs).
    public static func addAccountLoginTarget(
        _ account: AgentAccount?,
        label: String
    ) -> AddLoginTarget {
        ambientSignedIn(account)
            ? AddLoginTarget(profileId: nil, newProfileLabel: clampProfileLabel(label))
            : AddLoginTarget(profileId: systemProfileId, newProfileLabel: nil)
    }

    /// `Claude Code account 2` — the smallest N ≥ 2 whose `<agent> account N`
    /// is not already the label of a profile the machine reports for the
    /// agent (exact, case-sensitive). Counting profiles instead re-issued a
    /// label that still existed after an earlier one was removed
    /// (`[system, "account 3"]` → "account 3"), and `loginLanded` then saw
    /// the sign-in as already landed. Web/Android `nextProfileLabel`, same
    /// rule; `agentLabel` is resolved by the caller, since the agent's
    /// display name lives in the app target.
    public static func nextProfileLabel(
        _ account: AgentAccount?,
        agentLabel: String
    ) -> String {
        let taken = Set((account?.profiles ?? []).map { $0.label ?? "" })
        var n = 2
        while taken.contains("\(agentLabel) account \(n)") { n += 1 }
        return clampProfileLabel("\(agentLabel) account \(n)")
    }

    /// EXP-862: has the login the sign-in sheet drove ARRIVED? The sheet
    /// closes on the TRANSITION into this, so it must be false while the flow
    /// runs — web `agentLoginLanded`, rule for rule:
    ///   - a NEW profile (the "+ Add account" path): a profile carrying the
    ///     asked-for label is now usable. The ambient flag is useless here —
    ///     it is already true, which is precisely WHY a new profile was asked
    ///     for.
    ///   - an existing profile: that profile's own state.
    ///   - the ambient login: its `system` entry, else the top-level fields.
    ///
    /// "Usable" is signed in AND not `needs_relogin`: a login the agent
    /// refused the moment it was made has not landed.
    public static func loginLanded(
        account: AgentAccount?,
        profileId: String?,
        newProfileLabel: String?
    ) -> Bool {
        guard let account else { return false }
        let profiles = (account.profiles ?? []).filter { !$0.id.isEmpty }
        if let newProfileLabel {
            let wanted = newProfileLabel.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !wanted.isEmpty else { return false }
            return profiles.contains { profile in
                (profile.label ?? "").trimmingCharacters(in: .whitespacesAndNewlines) == wanted
                    && usableLogin(profile)
            }
        }
        if let profileId, profileId != systemProfileId {
            return profiles.contains { $0.id == profileId && usableLogin($0) }
        }
        // The ambient login: a machine that reports profiles carries it as the
        // `system` row, and its top-level fields are the ACTIVE profile's.
        if let ambient = profiles.first(where: { $0.id == systemProfileId }) {
            return usableLogin(ambient)
        }
        return account.signedIn == true && AgentAccountHealth.of(account) != .needsRelogin
    }

    private static func usableLogin(_ profile: AgentAccountProfile) -> Bool {
        profile.signedIn == true && AgentAccountHealth.of(profile) != .needsRelogin
    }

    // MARK: - Internals

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
