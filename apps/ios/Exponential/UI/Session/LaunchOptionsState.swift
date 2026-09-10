import ExpCore
import Foundation

/// EXP-825: the launch options a run carries — agent, model, effort, the two
/// toggles, the resume latch and the account profile — with the seed/clamp
/// rules that used to live inside the Start-coding sheet (EXP-437/EXP-201):
///
/// - The run lands on the selected MACHINE, so that machine's advertised
///   per-agent defaults (`launchDefaults` on the presence row) are the ONLY
///   seed source: they apply on open, on every device switch and on every
///   agent switch. Nothing is remembered locally.
/// - A desktop that advertises none (an older build) falls back to the
///   static contract defaults, and every advertised value is validated
///   against today's contract lists + the agent's capabilities before it can
///   be shown or sent (a desktop of another vintage must never push a value
///   the server rejects).
/// - Reselecting the SAME machine keeps the user's edits; only a different
///   one reseeds (`lastSeededDeviceId`).
///
/// Lifted into its own object because the Agent page composer holds it
/// alongside a subject that changes underneath it; the sheet could keep it
/// as `@State` because it was rebuilt on every open.
@MainActor @Observable
final class LaunchOptionsState {
    var agent = "claude"
    var model = ""
    var effort = LaunchVocabulary.cliDefault
    var ultracode = false
    var planMode = false
    /// EXP-481: the resume offer's latch — defaults ON, only the user flips
    /// it; eligibility (`resumeCandidate`) is the composer's to compute.
    var resume = true
    /// EXP-825: the picked login profile id, `""` = the machine's active
    /// login (never sent). Reset on every device or agent change — profiles
    /// are per machine and per agent.
    var account = ""

    /// The machine the options currently reflect (EXP-437).
    private(set) var lastSeededDeviceId: String?

    // MARK: - Agents

    /// The selected device's RUNNABLE agents, in contract order. An ABSENT
    /// advertisement is an old claude-only desktop, but an explicitly empty
    /// one means the machine can run nothing (EXP-409) — such devices never
    /// reach the candidate pool, so the claude fallback only covers the
    /// no-device case.
    func availableAgents(for device: SteerDevice?) -> [String] {
        let supported = device?.agentIds ?? ["claude"]
        let ordered = DomainContract.codingAgentValues.filter { supported.contains($0) }
        return ordered.isEmpty ? ["claude"] : ordered
    }

    /// Switch agent: model/effort/toggles reseed from the machine's defaults
    /// for the NEW agent (EXP-437 — they are per-agent), falling back to the
    /// agent's static defaults, then clamp to what it supports.
    func selectAgent(_ value: String, device: SteerDevice?) {
        guard value != agent else { return }
        agent = value
        applyAgentDefaults(for: value, device: device)
    }

    /// The newly resolved machine may not run the chosen agent.
    func clampAgent(to device: SteerDevice?) {
        let available = availableAgents(for: device)
        if !available.contains(agent) {
            selectAgent(available.first ?? "claude", device: device)
        }
    }

    /// After anything that can re-resolve the machine: clamp the agent, and
    /// when the pool settled on a DIFFERENT machine, reseed from its
    /// defaults exactly like an explicit pick would (EXP-437).
    func reconcile(device: SteerDevice?) {
        clampAgent(to: device)
        if device?.deviceId != lastSeededDeviceId { seed(from: device) }
    }

    // MARK: - Seeding

    /// Reseed agent + every option from the resolved machine's advertised
    /// defaults (EXP-437). Runs on open and on every device switch.
    func seed(from device: SteerDevice?) {
        let available = availableAgents(for: device)
        if let advertised = device?.defaultLaunchAgent {
            agent = advertised
        } else if !available.contains(agent) {
            // Nothing advertised: keep the current agent when the machine
            // can run it, else fall back exactly like `clampAgent`.
            agent = available.first ?? "claude"
        }
        applyAgentDefaults(for: agent, device: device)
        lastSeededDeviceId = device?.deviceId
    }

    /// The per-agent half: reset to [value]'s static defaults, then overlay
    /// what the machine advertises FOR THAT AGENT, and clamp the toggles to
    /// the agent's capabilities.
    private func applyAgentDefaults(for value: String, device: SteerDevice?) {
        let advertised = device?.agentDefaults(for: value)
        model = Self.seedModel(advertised?.model, for: value)
        effort = Self.seedEffort(advertised?.effort, for: value)
        ultracode = advertised?.ultracode ?? false
        planMode = advertised?.planMode ?? false
        account = ""
        clampToggles()
    }

    private func clampToggles() {
        if agent != "claude" {
            ultracode = false
        }
        if !LaunchVocabulary.supportsPlanMode(agent) {
            planMode = false
        }
    }

    /// An advertised model, validated against the agent's contract list.
    /// Blank is the desktop's "CLI default", which for codex/pi IS the static
    /// default and for claude (explicit-always) means its first model.
    static func seedModel(_ value: String?, for agent: String) -> String {
        guard let value, !value.isEmpty,
              LaunchVocabulary.modelValues(for: agent).contains(value)
        else {
            return LaunchVocabulary.defaultModel(for: agent)
        }
        return value
    }

    /// An advertised effort/reasoning/thinking value; blank or unknown = the
    /// "CLI default" row (omit the flag).
    static func seedEffort(_ value: String?, for agent: String) -> String {
        guard let value, LaunchVocabulary.effortValues(for: agent).contains(value) else {
            return LaunchVocabulary.cliDefault
        }
        return value
    }

    // MARK: - Accounts (EXP-825)

    /// The login profiles the picked machine reports for the picked agent —
    /// the Account picker renders only with two or more.
    func accountProfiles(on device: SteerDevice?) -> [AgentAccountProfile] {
        device?.agentAccounts?[agent]?.profiles ?? []
    }

    /// The web's `SYSTEM_PROFILE_ID`: the machine's ambient login, which a
    /// start never names explicitly.
    static let systemProfileId = "system"

    // MARK: - Wire

    /// The chosen options in wire form — shared by every launch subject.
    /// `resume` is the composer's call: single-issue starts only.
    func buildOptions(resume: Bool? = nil) -> SteerStartOptions {
        let isClaude = agent == "claude"
        let profile = account.isEmpty || account == Self.systemProfileId ? nil : account
        return SteerStartOptions(
            agent: agent,
            model: model == LaunchVocabulary.cliDefault ? "" : model,
            effort: effort == LaunchVocabulary.cliDefault ? "" : effort,
            // The toggles only exist for the agents that support them — never
            // send a stale value the launcher would reject or misread.
            ultracode: isClaude ? ultracode : nil,
            // A resume never re-enters plan mode (mirrors the desktop clamp).
            planMode: LaunchVocabulary.supportsPlanMode(agent)
                ? (resume == true ? false : planMode)
                : nil,
            resume: resume,
            account: profile
        )
    }
}
