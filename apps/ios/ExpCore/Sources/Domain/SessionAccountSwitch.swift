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
        usage: AgentUsage?
    ) {
        self.profileId = profileId
        self.label = label
        self.email = email
        self.plan = plan
        self.signedIn = signedIn
        self.health = health
        self.active = active
        self.usage = usage
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
        "Switching continues this run under the other account. The agent re-reads the "
        + "transcript once, which costs tokens."

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
        agent: String?
    ) -> [SessionAccountOption] {
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
                    usage: nil
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
                usage: profile.usage
            )
        }
    }

    /// Why `option` cannot be switched to right now, or nil when it can.
    /// Display gating only — the server and the machine re-check everything.
    ///
    /// `currentAccount` is the profile this run is KNOWN to be on, when the
    /// client knows it (`coding_sessions` carries no account column, so it is
    /// usually nil and the machine's own active login is not a safe stand-in).
    public static func refusal(
        option: SessionAccountOption,
        agent: String?,
        mine: Bool,
        sessionEnded: Bool,
        deviceOnline: Bool,
        canResume: Bool,
        turnState: AgentTurnState,
        currentAccount: String? = nil
    ) -> String? {
        guard supports(agent: agent) else { return reasonAgent }
        guard mine else { return reasonNotMine }
        guard !sessionEnded else { return reasonEnded }
        guard deviceOnline else { return reasonOffline }
        guard canResume else { return reasonNoCap }
        // EXP-848's turn slot is the idle test: `ended` is the default, so a
        // viewer that has not seen a `turn` event yet reads as idle.
        guard turnState != .started else { return reasonBusy }
        if option.health == .needsRelogin { return reasonNeedsRelogin }
        if !option.signedIn || option.health == .signedOut { return reasonSignedOut }
        if let currentAccount, currentAccount == option.profileId { return reasonAlready }
        return nil
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
