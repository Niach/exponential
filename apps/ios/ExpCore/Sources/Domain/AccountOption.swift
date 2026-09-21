import Foundation

/// EXP-872/EXP-988: the ONE account option model every launch surface offers.
///
/// One flattened list REPLACES the agent picker + the account picker on every
/// platform. Hand-mirrored against web `lib/accounts/account-option.ts`
/// (`flattenAccounts`), desktop `coding/src/account_option.rs` and Android
/// `domain/AccountOption.kt` — same fixture, same test names. The rules:
///
///  - one option per SIGNED-IN login the device reports, across both contract
///    agents (`devices.agent_accounts[agent].profiles`; an agent that reports
///    no profiles yields its ambient `system` login);
///  - the label is ALWAYS the agent's brand mark + the login's EMAIL — never
///    the profile name (`label`), never the word "default". A login reported
///    without an address shows its plan; with neither, its profile id (the
///    machine has nothing better to say);
///  - the DEVICE DEFAULT is marked by ORDER (it is first) and by a check, not
///    by a label: `isDeviceDefault` is true for exactly one option — the
///    `launchDefaults.defaultAccount` profile of `defaultAgent` when the
///    device stores one, else that agent's active login, falling back to the
///    first contract agent's active login, then to the first row. The rest
///    follow in `AgentAccountsRows.sortDeviceLogins` order;
///  - selecting an option IMPLIES the agent: there is no separate agent pick,
///    `agent` rides the option and the launch takes both from it;
///  - "default agent" settings became "default account": the setting stores a
///    profile id, and the agent derives from it.
///
/// `limits` are FRACTIONS 0..1 off the usage windows (`percent / 100`):
/// `fiveHour` = the `session` window, `week` = the `weekly` window, `model` =
/// the FIRST per-model window. A login with no usage report has no `limits`
/// at all.

/// The three usage fractions a login carries, 0..1.
public struct AccountLimits: Equatable, Sendable {
    /// The FIRST per-model window: what the agent calls it, and how full it is.
    public struct ModelLimit: Equatable, Sendable {
        public let label: String
        public let used: Double

        public init(label: String, used: Double) {
            self.label = label
            self.used = used
        }
    }

    public let fiveHour: Double
    public let week: Double
    public let model: ModelLimit?

    public init(fiveHour: Double, week: Double, model: ModelLimit? = nil) {
        self.fiveHour = fiveHour
        self.week = week
        self.model = model
    }
}

/// ONE login a machine reports, as a launch surface offers it.
public struct AccountOption: Equatable, Sendable {
    /// The profile id (`agent_profiles`), what a launch passes as `account`.
    public let id: String
    /// The contract `codingAgent` the login belongs to — picking the option
    /// picks this agent.
    public let agent: String
    /// What the row SAYS beside the brand mark (see the header's fallbacks).
    public let email: String
    /// Exactly one option per device is the default; it is also listed first.
    public let isDeviceDefault: Bool
    /// EXP-849: the device's verdict on the credential — a run started on a
    /// dead login dies on its first call, so the row badges `needs_relogin`.
    public let health: AgentAccountHealth
    public let limits: AccountLimits?

    public init(
        id: String,
        agent: String,
        email: String,
        isDeviceDefault: Bool,
        health: AgentAccountHealth = .unknown,
        limits: AccountLimits? = nil
    ) {
        self.id = id
        self.agent = agent
        self.email = email
        self.isDeviceDefault = isDeviceDefault
        self.health = health
        self.limits = limits
    }

    /// `<agent>:<profileId>` — the ONE string a picker keys a row by, since a
    /// profile id alone (`system`) repeats across agents.
    public var key: String { "\(agent):\(id)" }
}

public enum AccountOptions {
    /// Every signed-in login ONE machine reports, default first. Takes the
    /// three pieces the row carries so both a `SteerDevice` and a synced
    /// devices row can feed it.
    public static func flatten(
        accounts: [String: AgentAccount]?,
        usage: [String: AgentUsage]?,
        launchDefaults: DeviceLaunchDefaults?
    ) -> [AccountOption] {
        // `deviceLoginRows` already knows the shape: one row per profile (or
        // the ambient `system` login for a profile-less agent), retired agents
        // dropped, the active profile's numbers read off either slot, sorted.
        let rows = AgentAccountsRows.deviceLoginRows(
            SteerDevice(
                deviceId: "",
                deviceLabel: "",
                agentAccounts: accounts,
                agentUsage: usage
            )
        ).filter(\.signedIn)
        guard let first = rows.first else { return [] }

        // The default: the stored default account of the configured default
        // agent, else that agent's active login, else the first contract
        // agent's active login, else the first row — never none.
        let configured = launchDefaults?.defaultAgent
        let configuredAccount = launchDefaults?.defaultAccount
        func activeOf(_ agent: String?) -> AgentProfileUsageRow? {
            guard let agent, !agent.isEmpty else { return nil }
            return rows.first { $0.agent == agent && $0.active }
        }
        let stored: AgentProfileUsageRow? = {
            guard let configured, let configuredAccount, !configuredAccount.isEmpty
            else { return nil }
            return rows.first {
                $0.agent == configured && $0.profileId == configuredAccount
            }
        }()
        let defaultRow = stored
            ?? activeOf(configured)
            ?? DomainContract.codingAgentValues.compactMap { activeOf($0) }.first
            ?? first

        let ordered = [defaultRow] + rows.filter { $0.key != defaultRow.key }
        return ordered.map { row in
            AccountOption(
                id: row.profileId,
                agent: row.agent,
                email: optionEmail(row),
                isDeviceDefault: row.key == defaultRow.key,
                health: row.health,
                limits: optionLimits(row)
            )
        }
    }

    /// The option a launch surface should START on: the device default, else
    /// the first option. Nil for a device that reports no login at all.
    public static func defaultOption(_ options: [AccountOption]) -> AccountOption? {
        options.first(where: \.isDeviceDefault) ?? options.first
    }

    // MARK: - Internals

    /// The email a row reads as — see the header's fallback ladder. The last
    /// rung is the profile ID, not its label: a label is a name somebody
    /// typed, and this list never shows one.
    private static func optionEmail(_ row: AgentProfileUsageRow) -> String {
        AgentAccountsRows.accountName(email: row.email, plan: row.plan)
    }

    private static func fraction(_ percent: Double?) -> Double {
        guard let percent, percent.isFinite else { return 0 }
        return min(1, max(0, percent / 100))
    }

    private static func optionLimits(_ row: AgentProfileUsageRow) -> AccountLimits? {
        let windows = row.usage?.windows ?? []
        if windows.isEmpty { return nil }
        let session = windows.first { $0.key == AgentUsagePresentation.sessionWindowKey }
        let weekly = windows.first { $0.key == AgentUsagePresentation.weeklyWindowKey }
        let model = windows.first {
            $0.key.hasPrefix(AgentUsagePresentation.modelWindowPrefix)
        }
        return AccountLimits(
            fiveHour: fraction(session?.percent),
            week: fraction(weekly?.percent),
            model: model.map {
                AccountLimits.ModelLimit(label: $0.label, used: fraction($0.percent))
            }
        )
    }
}
