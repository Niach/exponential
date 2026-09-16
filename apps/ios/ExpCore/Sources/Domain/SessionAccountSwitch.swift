import Foundation

/// EXP-849 phase 3: the MID-SESSION account switch, as the rule the session
/// screen's account rows render.
///
/// A switch is a RESUME that names an account (`steer.startSession({
/// resumeSessionId, deviceId, account })`): the machine relaunches the run
/// under the other login in the same worktree and links the new
/// `coding_sessions` row by `resumed_from_id`, so the run reads as a
/// continuation rather than a second run. Claude only — codex stays one account
/// per session (EXP-849 §E) — and only between turns: a switch mid-turn would
/// abandon work the agent has in flight.
///
/// Everything here is pure. Hand-mirrored with Android
/// `domain/SessionAccountSwitch.kt`, web `components/session-account-switch.tsx`
/// and the desktop's `ResumeRunRequest.account` gate: same refusals, same
/// strings, same order.
public struct SessionAccountOption: Equatable, Sendable, Identifiable {
    /// What a switch sends as `account` (`system` = the machine's ambient
    /// login, named on the wire here unlike on a fresh start — see
    /// `wireAccount`).
    public let profileId: String
    /// The row label (`Default` for the ambient login when the device sent
    /// none).
    public let label: String
    public let email: String?
    public let plan: String?
    public let signedIn: Bool
    public let health: AgentAccountHealth
    /// The machine's CURRENT login for the agent.
    public let active: Bool
    /// This login's own rate-limit windows, when the machine reported them.
    public let usage: AgentUsage?
    /// EXP-909: the run is KNOWN to be on this login — its synced
    /// `coding_sessions.agent_account` names this profile. False on every row
    /// of a run whose account is unknown; never inferred from `active` (see
    /// `activeAccountIndex`, which does the inferring explicitly).
    public let current: Bool

    public var id: String { profileId }

    /// The identity line: the email, else the plan, else the profile label.
    public var caption: String { email ?? plan ?? label }

    public init(
        profileId: String,
        label: String,
        email: String?,
        plan: String?,
        signedIn: Bool,
        health: AgentAccountHealth,
        active: Bool,
        usage: AgentUsage?,
        current: Bool = false
    ) {
        self.profileId = profileId
        self.label = label
        self.email = email
        self.plan = plan
        self.signedIn = signedIn
        self.health = health
        self.active = active
        self.usage = usage
        self.current = current
    }
}

public enum SessionAccountSwitch {
    /// The only agent that can change account between messages (EXP-849 §E).
    public static let switchableAgent = "claude"

    /// The sheet's section title, byte-identical ×4.
    public static let sectionTitle = "Accounts"

    /// The primary control on an account row.
    public static let switchLabel = "Switch to this account"

    /// The rate-limit wall's PRIMARY button (EXP-849 interface D).
    public static let wallSwitchLabel = "Switch account"

    /// What a switch costs, said once where the switch is offered: the agent
    /// re-enters the recorded run under the other login, which re-reads the
    /// transcript — one extra context read, not a per-message surcharge.
    public static let costNote =
        "The run continues under the other account. Re-reading the transcript once costs tokens."

    /// The continuation byline a resumed run's screen carries.
    public static let continuationNote = "Continues an earlier run"

    /// What that continuation cost, said ONCE on the new run.
    public static let continuationCostNote =
        "The agent re-read the transcript once to pick it up — a one-time cost."

    // MARK: - Refusals

    // One sentence each, and the reason is always about the thing the person
    // can change. Shown on the DISABLED control rather than hiding it, so the
    // switch never silently disappears mid-run.

    public static let reasonAgent = "Only claude can switch accounts during a run."
    public static let reasonNotMine =
        "Only the person who started this run can switch its account."
    public static let reasonEnded = "This run has ended — resume it instead."
    public static let reasonOffline = "The machine is offline."
    public static let reasonNoCap = "Update the app on that machine to switch accounts."
    public static let reasonBusy =
        "The agent is working — switching waits for the turn to finish."
    public static let reasonSignedOut = "Sign in to this account on that machine first."
    public static let reasonNeedsRelogin = "This account needs a re-login on that machine."
    public static let reasonAlready = "This run is already on this account."

    /// Whether the run's agent can change account at all — what decides if the
    /// readout offers the control in the first place.
    public static func supports(agent: String?) -> Bool {
        agent == switchableAgent
    }

    /// The logins the session screen lists for the run's `agent` on its host
    /// machine: every profile the machine reported, or the single ambient
    /// account for a pre-profile machine. Empty when the machine said nothing
    /// about the agent — there is then nothing to switch between.
    public static func options(
        accounts: [String: AgentAccount]?,
        agent: String?,
        currentAccount: String? = nil
    ) -> [SessionAccountOption] {
        let current = nonEmpty(currentAccount)
        guard let agent, !agent.trimmingCharacters(in: .whitespaces).isEmpty,
              let account = accounts?[agent]
        else { return [] }
        let profiles = account.profiles ?? []
        if profiles.isEmpty {
            return [
                SessionAccountOption(
                    profileId: AgentAccountsRows.systemProfileId,
                    label: "Default",
                    email: nonEmpty(account.email),
                    plan: nonEmpty(account.plan),
                    signedIn: account.signedIn == true,
                    health: AgentAccountHealth.of(account),
                    active: true,
                    usage: nil,
                    current: current == AgentAccountsRows.systemProfileId
                )
            ]
        }
        return profiles.map { profile in
            SessionAccountOption(
                profileId: profile.id,
                label: nonEmpty(profile.label)
                    ?? (profile.id == AgentAccountsRows.systemProfileId ? "Default" : profile.id),
                email: nonEmpty(profile.email),
                plan: nonEmpty(profile.plan),
                signedIn: profile.signedIn == true,
                health: AgentAccountHealth.of(profile),
                active: profile.active == true,
                usage: profile.usage,
                current: current == profile.id
            )
        }
    }

    /// Why `option` cannot be switched to right now, or nil when it can.
    /// Display gating only — the server and the machine re-check everything.
    ///
    /// `currentAccount` is the profile this run is KNOWN to be on — since
    /// EXP-909 the synced `coding_sessions.agent_account`. Still nil for a run
    /// started by a client too old to stamp it, and the machine's own active
    /// login is never a safe stand-in for it.
    public static func refusal(
        option: SessionAccountOption,
        agent: String?,
        mine: Bool,
        sessionEnded: Bool,
        deviceOnline: Bool,
        canResume: Bool,
        canSwitchAccount: Bool,
        turnState: AgentTurnState,
        currentAccount: String? = nil
    ) -> String? {
        guard supports(agent: agent) else { return reasonAgent }
        guard mine else { return reasonNotMine }
        guard !sessionEnded else { return reasonEnded }
        guard deviceOnline else { return reasonOffline }
        // BOTH caps: `resume-run` carries the relaunch, `account-switch`
        // (desktop/CLI 0.14.38) is what makes the machine honour `account` on
        // a LIVE run. Without the second one the switch is a no-op there, so
        // it must read as "update that machine", never as a silent nothing.
        guard canResume, canSwitchAccount else { return reasonNoCap }
        // EXP-848's turn slot is the idle test: `ended` is the default, so a
        // viewer that has not seen a `turn` event yet reads as idle.
        guard turnState != .started else { return reasonBusy }
        if option.health == .needsRelogin { return reasonNeedsRelogin }
        if !option.signedIn || option.health == .signedOut { return reasonSignedOut }
        if let currentAccount, currentAccount == option.profileId { return reasonAlready }
        return nil
    }

    /// The refusals that are about the RUN, not about one login — when every
    /// listed account is refused for the same one of these, the overlay says
    /// it ONCE in its footer instead of repeating it under every row.
    static let globalSwitchReasons: [String] = [
        reasonAgent,
        reasonNotMine,
        reasonEnded,
        reasonOffline,
        reasonNoCap,
        reasonBusy,
    ]

    /// EXP-863: the ONE footer sentence when switching is refused for every
    /// listed account by the same run-level reason, else nil (the footer then
    /// carries `costNote`). Row-specific refusals (signed out, needs a
    /// re-login) never become the footer: they stay under their row. Web
    /// `globalSwitchBlocker`, same rule.
    public static func globalSwitchBlocker(_ refusals: [String?]) -> String? {
        guard let first = refusals.first ?? nil, globalSwitchReasons.contains(first) else {
            return nil
        }
        return refusals.allSatisfy { $0 == first } ? first : nil
    }

    /// EXP-909: WHICH listed login the run is on, as an index into `options`
    /// — nil when it cannot be known, which is NOT "the ambient one". The
    /// order is the desktop's `SwitchTarget.current` rule, mirrored from web
    /// `activeAccountIndex`, in the order the client can know it:
    ///   1. the synced `coding_sessions.agent_account` (`option.current`),
    ///   2. else the login whose email the machine REPORTS for this agent
    ///      (`agentAccounts[agent].email`, always the active login's),
    ///   3. else the machine's active login for the agent.
    ///
    /// Guessing here is the failure that matters: a header naming the wrong
    /// account draws the wrong limits beside a run that is spending someone
    /// else's.
    public static func activeAccountIndex(
        _ options: [SessionAccountOption],
        reportedEmail: String?
    ) -> Int? {
        if let known = options.firstIndex(where: { $0.current }) { return known }
        if let email = nonEmpty(reportedEmail),
           let byEmail = options.firstIndex(where: { $0.email == email }) {
            return byEmail
        }
        return options.firstIndex { $0.active }
    }

    /// `activeAccountIndex` as the option itself — what the usage overlay's
    /// header draws (and whose windows it shows).
    public static func currentOption(
        _ options: [SessionAccountOption],
        reportedEmail: String?
    ) -> SessionAccountOption? {
        activeAccountIndex(options, reportedEmail: reportedEmail).map { options[$0] }
    }

    /// What the switch carries as `account` — the picked profile VERBATIM,
    /// `system` included.
    ///
    /// A fresh start omits the ambient login (`system` is the absence of an
    /// account there), but a switch may not: the server reads the PRESENCE of
    /// `account` as "this resume is a switch" and that is the only thing that
    /// lets a resume ride a LIVE run, so an omitted field would come back as
    /// "That run is still live". `system` is accepted there explicitly and
    /// skips the profile-membership check (Android `wireAccount`, web
    /// `session-account-switch.tsx` send it verbatim too).
    public static func wireAccount(_ option: SessionAccountOption) -> String {
        option.profileId
    }

    private static func nonEmpty(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty
        else { return nil }
        return trimmed
    }
}
