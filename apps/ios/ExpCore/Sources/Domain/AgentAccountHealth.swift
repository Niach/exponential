import Foundation

/// EXP-849: how an agent LOGIN is doing on a machine — the fourth thing a
/// device reports about an account, beside who it is (`email`), what it may
/// spend (`plan`) and how much of that is left (`agentUsage`).
///
/// The device derives it from its own usage probe and writes it on the
/// heartbeat (`devices.agent_accounts[agent].health`, and the same field on
/// every `profiles[]` entry): a probe that came back Unauthorized is
/// `needs_relogin`, a probe that answered is `ok`, a login that is not there
/// at all is `signed_out`, and a login nothing ever probed is `unknown`. The
/// CLI's `auth status` is identity only (email/plan) and never health — a
/// stale token still prints an email.
///
/// Why a type and not a raw string: the four values are a contract the server
/// clamps to, so a build that meets a FIFTH one must degrade to `unknown`
/// rather than render a wire token at a person.
///
/// Hand-mirrored ×4 against the same rules and the same two badge strings:
/// web `lib/agent-usage.ts` (`agentHealth` / `derivedAgentHealth` /
/// `healthBadgeLabel` / `healthRank` / `worstHealth`), Android
/// `domain/AgentHealth.kt` (`AgentHealth` + `AgentHealthRules`), desktop
/// `coding::agent_accounts::Health`. Change a rule here, change it there.
public enum AgentAccountHealth: String, Equatable, Sendable, CaseIterable {
    /// The last probe answered: the login works.
    case ok
    /// The login is THERE but the agent refused it — the credential expired
    /// or was revoked. The one state a person has to fix by signing in again,
    /// and deliberately distinct from `signedOut`.
    case needsRelogin = "needs_relogin"
    /// No login at all on that machine.
    case signedOut = "signed_out"
    /// Signed in, never probed (or a report this build has no name for).
    case unknown

    /// Attention order: the repairable state first, then the absent login,
    /// then "we don't know", then healthy. `worst` folds on this, so a device
    /// badge names the loudest thing about any of its logins.
    public var rank: Int {
        switch self {
        case .needsRelogin: return 0
        case .signedOut: return 1
        case .unknown: return 2
        case .ok: return 3
        }
    }

    /// Whether there is something to DO about this login — what badges a
    /// device row and sorts an account up.
    public var needsAttention: Bool {
        self == .needsRelogin || self == .signedOut
    }

    /// The badge text, or nil when there is nothing worth a badge (a healthy
    /// or never-probed login says nothing). Byte-locked ×4 — "Needs re-login"
    /// is NOT "Signed out": the first keeps its email and its usage history,
    /// the second has neither.
    public var badgeLabel: String? {
        switch self {
        case .needsRelogin: return "Needs re-login"
        case .signedOut: return "Signed out"
        case .ok, .unknown: return nil
        }
    }

    /// A REPORTED value, parsed. Nothing, blank, or a name this build does not
    /// know is `unknown` — "nothing claimed", never a guess and never a raw
    /// token on screen. Compared as sent (trimmed, NOT lowercased): the wire
    /// values are lowercase by contract, so a differently-cased token is a
    /// token from somewhere else. Web `agentHealth`'s union check / Android
    /// `parse`.
    public static func parse(_ raw: String?) -> AgentAccountHealth {
        guard let token = raw?.trimmingCharacters(in: .whitespacesAndNewlines),
              !token.isEmpty,
              let known = AgentAccountHealth(rawValue: token)
        else { return .unknown }
        return known
    }

    /// What a row with no `health` at all means (a pre-EXP-849 device): the
    /// only thing its payload says is whether the CLI was signed in. A nil
    /// `signedIn` is not a signed-in claim, so it reads the same as `false` —
    /// byte-identical with web `derivedAgentHealth` / Android `derived`.
    public static func derived(signedIn: Bool?) -> AgentAccountHealth {
        signedIn == true ? .ok : .signedOut
    }

    /// The device's own verdict when it sent one, else the derivation. A
    /// `signed_out` claim from a signed-IN report is KEPT: the device is the
    /// authority on its own credential.
    public static func resolve(_ raw: String?, signedIn: Bool?) -> AgentAccountHealth {
        let reported = raw?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let reported, !reported.isEmpty else { return derived(signedIn: signedIn) }
        return parse(reported)
    }

    /// One agent's top-level account entry (nil = the machine never mentioned
    /// the agent at all, which is not a signed-out login).
    public static func of(_ account: AgentAccount?) -> AgentAccountHealth {
        guard let account else { return .unknown }
        return resolve(account.health, signedIn: account.signedIn)
    }

    /// One login PROFILE entry.
    public static func of(_ profile: AgentAccountProfile) -> AgentAccountHealth {
        resolve(profile.health, signedIn: profile.signedIn)
    }

    /// The loudest health among several — a device row badges this across every
    /// login it reports. Nil for an EMPTY set (web `worstHealth`, Android
    /// `worst`): nothing was reported, so nothing is claimed. A caller that
    /// needs a value reads nil as `unknown` — neither wears a badge.
    public static func worst(_ values: [AgentAccountHealth]) -> AgentAccountHealth? {
        values.min { $0.rank < $1.rank }
    }
}
