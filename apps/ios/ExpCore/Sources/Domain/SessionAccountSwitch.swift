import Foundation

/// EXP-849: switching a SESSION to another agent login, from its usage wall.
///
/// A switch = ONE `sessions.send({ sessionId, account })` with no text
/// (`SessionsSendInput.switchAccount`): the server writes the synced
/// `sessions.account` (the login the session should run on) and the host
/// device resumes the SAME row on that login between turns. CLAUDE ONLY
/// (codex's conversation lives in its login's own rollout store) and BETWEEN
/// TURNS only.
///
/// The options are THE composer's account list (`AccountOptions.flatten`)
/// narrowed to the session's agent. Every sentence is hand-mirrored ×4 (web
/// `lib/session-account-switch.ts`, desktop `crates/ui/src/account_switch.rs`,
/// Android `domain/SessionAccountSwitch.kt`).
public enum SessionAccountSwitch {
    /// The only agent that can change account between messages (EXP-849 §E).
    public static let switchableAgent = "claude"

    /// The rate-limit wall's control.
    public static let wallSwitchLabel = "Switch account"

    /// The primary control on an account row.
    public static let switchLabel = "Switch to this account"

    /// What a switch costs, said once where the switch is offered.
    public static let costNote =
        "The session continues under the other account. Re-reading the transcript once costs tokens."

    // MARK: Refusals — one sentence each, shown ON the disabled control

    public static let reasonAgent = "Only claude can switch accounts during a session."
    public static let reasonOffline = "The machine is offline."
    public static let reasonBusy = "The agent is working — switching waits for the turn to finish."
    public static let reasonSignedOut = "Sign in to this account on that machine first."
    public static let reasonNeedsRelogin = "This account needs a re-login on that machine."
    public static let reasonAlready = "This session is already on this account."

    /// Who may steer a session (Threads v2 visibility ×4): a session on a
    /// team-SHARED device = every member, any other = its starter only.
    public static func canSteer(shared: Bool, userId: String, currentUserId: String?) -> Bool {
        if shared { return true }
        guard let currentUserId, !currentUserId.isEmpty else { return false }
        return userId == currentUserId
    }

    /// The session's agent: the row's `cli`, else the device's default agent.
    public static func agent(cli: String?, defaultAgent: String?) -> String? {
        if let cli, !cli.isEmpty { return cli }
        if let defaultAgent, !defaultAgent.isEmpty { return defaultAgent }
        return nil
    }

    /// The run-level refusal, or nil. The ORDER is the ×4 order: the most
    /// fundamental refusal wins, so a codex session never reads "offline".
    public static func blockedReason(agent: String?, deviceOnline: Bool, busy: Bool) -> String? {
        if agent != switchableAgent { return reasonAgent }
        if !deviceOnline { return reasonOffline }
        if busy { return reasonBusy }
        return nil
    }

    /// One login's refusal, or nil = switchable.
    public static func optionBlockedReason(_ option: AccountOption, currentAccount: String?) -> String? {
        if option.health == .needsRelogin { return reasonNeedsRelogin }
        if option.health == .signedOut { return reasonSignedOut }
        if let currentAccount, !currentAccount.isEmpty, currentAccount == option.id {
            return reasonAlready
        }
        return nil
    }

    /// One login the session could continue under.
    public struct Option: Equatable, Sendable {
        public let account: AccountOption
        /// This session is on it (`sessions.account`).
        public let current: Bool
        public let blockedReason: String?

        public init(account: AccountOption, current: Bool, blockedReason: String?) {
            self.account = account
            self.current = current
            self.blockedReason = blockedReason
        }
    }

    /// The device's signed-in logins for the session's agent, in the
    /// composer's order, each with its refusal.
    public static func options(
        accounts: [String: AgentAccount]?,
        usage: [String: AgentUsage]?,
        launchDefaults: DeviceLaunchDefaults?,
        cli: String?,
        account: String?
    ) -> [Option] {
        guard let agent = agent(cli: cli, defaultAgent: launchDefaults?.defaultAgent) else { return [] }
        let named = account.flatMap { $0.isEmpty ? nil : $0 }
        return AccountOptions.flatten(accounts: accounts, usage: usage, launchDefaults: launchDefaults)
            .filter { $0.agent == agent }
            .map { option in
                Option(
                    account: option,
                    current: named != nil && option.id == named,
                    blockedReason: optionBlockedReason(option, currentAccount: named)
                )
            }
    }

    /// The wire form of a switch onto `option` — its profile id VERBATIM.
    public static func sendInput(sessionId: String, option: AccountOption) -> SessionsSendInput {
        .switchAccount(sessionId: sessionId, account: option.id)
    }
}
