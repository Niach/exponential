import Foundation

/// EXP-829/EXP-909: the LOGINS under a Devices row — the rules behind them.
///
/// EXP-909 folded the cross-device "Accounts" section away on all four
/// clients: it merged by email logins that are per MACHINE, so one account
/// lived twice, with two orderings, two health badges and two menus. What is
/// left is the derivation that always mattered — one row per machine × agent ×
/// login profile, listed under the machine that holds it — hand-mirrored
/// against web `lib/agent-usage.ts` (`agentProfileUsageRows` /
/// `deviceLoginRows` / `sortDeviceLogins` / `loginLabel` / `refreshAllowedAt`
/// / the sign-in landing)
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
    /// The `agent-usage-refresh` device cap: the machine runs
    /// `agent_usage_refresh` (web `deviceCanRefreshUsage`).
    public static let refreshCap = "agent-usage-refresh"

    /// A forced usage refresh is refused while the last fetch is younger than
    /// this: the device never hits the agent's usage endpoint more often (its
    /// `RATE_LIMITED_FLOOR_SECS`). Locked ×4.
    public static let rateLimitedFloor: TimeInterval = 5 * 60

    // MARK: - Rows off the devices shape

    /// The rows the section renders for `devices` — one per machine × agent ×
    /// profile, and ONLY the profiles the machine reports: no ambient login is
    /// ever used, so nothing is synthesized off the top-level fields (no
    /// `system` / "Default" row). An agent with no profiles has no rows.
    /// `isOnline` is the caller's clock (`DeviceLiveness`), passed in so the
    /// derivation stays a pure function of the rows.
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
                usageMap: AgentUsagePresentation.parseMap(device.agentUsage) ?? [:]
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
            usageMap: device.agentUsage ?? [:]
        ))
    }

    /// The shared core both entry points run: one machine's reported accounts
    /// and usage → its rows, in contract agent order, then the order the
    /// machine sent its profiles in.
    private static func rows(
        deviceId: String,
        deviceLabel: String,
        mine: Bool,
        online: Bool,
        accounts: [String: AgentAccount],
        usageMap: [String: AgentUsage]
    ) -> [AgentProfileUsageRow] {
        var out: [AgentProfileUsageRow] = []
        // EXP-849: an agent this build has no name for (a retired `pi`
        // still beating off an old daemon) is not a row, not a tab and
        // not a chip. The row mapping already drops it; the set is
        // filtered here too, so a caller that parsed the jsonb itself
        // cannot smuggle one in.
        let agents = orderedAgents(
            Set(accounts.keys).filter(AgentUsagePresentation.isContractAgent)
        )
        for agent in agents {
            let account = accounts[agent]
            let profiles = account?.profiles ?? []
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
    /// signed-out or refused login leads the rest), then the order the
    /// machine SENT them in — a login has no name to sort by, and the device's
    /// order is stable across heartbeats.
    ///
    /// The active login leads DELIBERATELY, before attention: this list
    /// answers "what is this machine running right now", and the broken
    /// sibling below it still wears its badge. Locked ×4 by `device logins
    /// lead with the active login, in contract agent order`.
    public static func sortDeviceLogins(_ rows: [AgentProfileUsageRow]) -> [AgentProfileUsageRow] {
        rows.enumerated().sorted { lhs, rhs in
            let (a, b) = (lhs.element, rhs.element)
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
            return lhs.offset < rhs.offset
        }.map(\.element)
    }

    /// EXP-1013: what a login with no known address and no plan is called.
    /// Byte-identical ×4.
    public static let noEmailLabel = "No email"

    /// EXP-1013: the ONE name a login wears on every surface (pickers, rows,
    /// sheets; ×4 `accountName`): its EMAIL, signed in or not (the device
    /// keeps the last address a signed-out login answered with). An agent that
    /// reports no address (codex's API-key login) is named by its plan; a
    /// login nobody ever signed in to is "No email". A login has no other
    /// name: profiles carry none.
    public static func accountName(email: String?, plan: String?) -> String {
        nonEmpty(email) ?? nonEmpty(plan) ?? noEmailLabel
    }

    /// A login's title on its device row: who it IS (`accountName`). NEVER a
    /// status: the brand mark says which agent, the row above says which
    /// machine, and the health badge says whether it still works. Locked ×4 by
    /// `the login label is the identity, never the status`.
    public static func loginLabel(_ row: AgentProfileUsageRow) -> String {
        accountName(email: row.email, plan: row.plan)
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

    /// The `account-switch` device cap: the machine switches a live run onto
    /// another login (the mid-run switch). `SteerDevice.canSwitchAccount`
    /// reads the cap.
    public static let switchCap = "account-switch"

    /// EXP-862: the `account-remove` device cap — the machine runs
    /// `agent_profile_remove` and deletes its own copy of a login. Its own cap
    /// beside `agent-login`, because an older build would leave the queued row
    /// pending forever.
    public static let removeCap = "account-remove"

    /// EXP-1137: the `account-sign-out` device cap — the machine runs
    /// `agent_profile_sign_out`. `SteerDevice.canSignOutAccount` reads it.
    public static let signOutCap = "account-sign-out"

    /// The `agent-import` device cap: the machine takes `agent_login {agent,
    /// import: true}` and MOVES its ambient login into a profile.
    /// `SteerDevice.canImportAgent` reads it.
    public static let importCap = "agent-import"

    /// EXP-862: a chip whose one repair is a SIGN-IN — there is no login on
    /// that machine, or the agent refused the one there. Both read the same to
    /// a person: sign in. (Web `MachineAccountChip`, Android `chipSignsIn`.)
    public static func chipSignsIn(_ row: AgentProfileUsageRow) -> Bool {
        !row.signedIn || row.health == .needsRelogin
    }

    /// Byte-identical with the server's refusal (`lib/trpc/devices.ts`), so a
    /// requester that raced a downgrade reads the same sentence twice.
    public static let removeAccountOldApp =
        "That machine runs an older Exponential app that cannot remove agent accounts. Update it first."

    /// EXP-1137: the server's refusal for a sign-out on a build without the
    /// body. Byte-identical ×4.
    public static let signOutOldApp =
        "That machine runs an older Exponential app that cannot sign agent accounts out. Update it first."

    /// EXP-862: why "Remove account" is NOT offered for this login on this
    /// machine, or nil when it is — the web `removeAccountBlockReason` twin:
    /// the machine needs the removal body (`account-remove`: it deletes the
    /// profile dir).
    ///
    /// EXP-944: being signed OUT is not one of them. A dead profile is the
    /// thing people most want gone, the removal is a profile-dir delete that
    /// never touches the account (no `codex logout`, ever), and the server has
    /// always taken it — it gates on the caps and the reported profile, never
    /// on the credential's state. So a signed-out login offers "Sign in" AND
    /// "Remove account".
    public static func removeAccountBlockReason(
        _ row: AgentProfileUsageRow,
        canAgentLogin: Bool,
        canRemoveAccount: Bool
    ) -> String? {
        canAgentLogin && canRemoveAccount ? nil : removeAccountOldApp
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

    /// EXP-1137: why "Sign out" is NOT offered for this login on this machine,
    /// or nil when it is: nothing to sign out of, or a build without the
    /// command. A revoked credential (`needsRelogin`) still signs out — that
    /// is how the dead credential leaves the machine.
    public static func signOutBlockReason(
        _ row: AgentProfileUsageRow,
        canAgentLogin: Bool,
        canSignOutAccount: Bool
    ) -> String? {
        if !row.signedIn { return "That login is already signed out there." }
        if !canAgentLogin || !canSignOutAccount { return signOutOldApp }
        return nil
    }

    /// EXP-1137: whether the chip menu offers "Sign out" for this login.
    public static func chipSignsOut(
        _ row: AgentProfileUsageRow,
        canAgentLogin: Bool,
        canSignOutAccount: Bool
    ) -> Bool {
        signOutBlockReason(
            row, canAgentLogin: canAgentLogin, canSignOutAccount: canSignOutAccount
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

    /// EXP-1137: the sign-out confirm, pinned ×4: the login keeps its row for
    /// a later sign-in.
    public static func signOutConfirmCopy(
        account: String,
        device: String
    ) -> String {
        "Sign \(account) out on \(device)? The login stays listed so it can sign in again; the account itself is untouched."
    }

    // MARK: - Signing in (EXP-827/EXP-862, logins by email)

    /// A sign-in has no say in WHERE it lands: the machine signs in in a
    /// fresh staging dir and commits by EMAIL — the profile already holding
    /// that address is refreshed (and only it), a new address becomes a new
    /// profile. A queued `agent_login` names at most the INTENDED profile
    /// (the row whose Sign in was tapped; nil = "Add account"), which only
    /// decides whether the landing is a duplicate.
    ///
    /// The baseline a sign-in sheet captures when it queues the command:
    /// every reported profile id → its `lastLoginAt` ("" when it has none).
    /// A profile's presence matters as much as its stamp — see
    /// `loginLanding`.
    public static func loginBaseline(_ account: AgentAccount?) -> [String: String] {
        var out: [String: String] = [:]
        for profile in account?.profiles ?? [] where !profile.id.isEmpty {
            out[profile.id] = profile.lastLoginAt ?? ""
        }
        return out
    }

    /// Where a sign-in (or an import) landed, once it has.
    public struct LoginLanding: Equatable, Sendable {
        public let profileId: String
        public let email: String?
        /// The login landed on a profile that was ALREADY on the machine and
        /// was not the one asked for: the person signed in as somebody the
        /// machine already had. Say so (`alreadyAddedToast`).
        public let duplicate: Bool

        public init(profileId: String, email: String?, duplicate: Bool) {
            self.profileId = profileId
            self.email = email
            self.duplicate = duplicate
        }
    }

    /// Has the login a sign-in sheet drove ARRIVED, and where? The sheet
    /// closes on the TRANSITION into non-nil (the same rule as the web,
    /// Android and desktop sign-in dialogs): the first reported profile whose
    /// `lastLoginAt` is set and differs from `baseline` (a new id with one
    /// counts). It is a DUPLICATE
    /// when that profile existed in the baseline and either nothing was
    /// intended ("Add account", an import) or it is not the intended one.
    public static func loginLanding(
        account: AgentAccount?,
        baseline: [String: String],
        intendedProfileId: String?
    ) -> LoginLanding? {
        let intended = nonEmpty(intendedProfileId)
        for profile in account?.profiles ?? [] where !profile.id.isEmpty {
            guard let stamp = nonEmpty(profile.lastLoginAt) else { continue }
            let before = baseline[profile.id]
            if before == stamp { continue }
            let existed = before != nil
            return LoginLanding(
                profileId: profile.id,
                email: nonEmpty(profile.email),
                duplicate: existed && (intended == nil || intended != profile.id)
            )
        }
        return nil
    }

    /// The warning a duplicate landing raises, byte-identical ×4 (fixture
    /// `device-doctor.json` `copy.alreadyAdded`). `email` = the landed
    /// profile's.
    public static func alreadyAddedToast(_ landing: LoginLanding) -> String {
        "\(accountName(email: landing.email, plan: nil)) was already added. Refreshed it."
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
