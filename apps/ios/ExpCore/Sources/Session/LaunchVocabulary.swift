import Foundation

// EXP-615: the ONE launch vocabulary. Agent / model / effort labels, the
// per-agent option lists, the plan-mode capability and the picker captions
// used to live three times over — the launcher's statics (now the Agent
// page composer), a private copy inside DeviceSettingsSheet, and a third set
// of shims in the trigger form sheet. Every launch-shaped surface (the Agent
// page composer, the trigger form, device launch defaults) now reads
// them from here, so a contract change lands once.
public enum LaunchVocabulary {
    /// Sentinel for the blank "CLI default" EFFORT choice (omit --effort).
    /// Models have no such row: every model is explicit (2026-10-15).
    public static let cliDefault = "cli-default"

    /// The effort a picker's choice puts on the wire: the "CLI default" row
    /// sends a BLANK (the server folds `""` to NULL = omit --effort); the
    /// sentinel itself is not a contract value and the server refuses it.
    public static func wireEffort(_ choice: String) -> String {
        choice == cliDefault ? "" : choice
    }

    /// The picker row a stored/sent effort reads as: NULL or blank = the
    /// "CLI default" row, else the value itself.
    public static func effortChoice(_ stored: String?) -> String {
        guard let stored, !stored.isEmpty else { return cliDefault }
        return stored
    }

    // MARK: - Option lists

    /// THE model list of an agent, for EVERY model pick (session model,
    /// thread model, subagents, computer use): claude = contract
    /// `codingModel`, codex = `codexModel`. No default row.
    /// Parameterized because a seed validates an advertised value against the
    /// agent it belongs to, which isn't always the selected one yet (EXP-437).
    public static func modelValues(for agent: String) -> [String] {
        switch agent {
        case "codex": DomainContract.codexModelValues
        default: DomainContract.codingModelValues
        }
    }

    public static func effortValues(for agent: String) -> [String] {
        switch agent {
        case "codex": DomainContract.codexEffortValues
        default: DomainContract.codingEffortValues
        }
    }

    /// The session model a machine that never set one runs (contract
    /// `deviceAgentDefaults`).
    public static func defaultModel(for agent: String) -> String {
        agent == "codex"
            ? DomainContract.deviceAgentDefaultsCodexModel
            : DomainContract.deviceAgentDefaultsModel
    }

    /// The dispatcher root's model (threads + team agents) a machine that
    /// never set one runs.
    public static func defaultThreadModel(for agent: String) -> String {
        agent == "codex"
            ? DomainContract.deviceAgentDefaultsCodexThreadModel
            : DomainContract.deviceAgentDefaultsThreadModel
    }

    /// The model the session composer's pill shows (web `session-composer.tsx`
    /// `fallbackModel`): the live relay model, else the row's own pick, else
    /// the device's session `model` for the row's agent, else the contract
    /// defaults.
    public static func composerModel(
        live: String?,
        rowModel: String?,
        cli: String?,
        launchDefaults: DeviceLaunchDefaults?
    ) -> String {
        if let live, !live.isEmpty { return live }
        if let rowModel, !rowModel.isEmpty { return rowModel }
        let agent = cli ?? "claude"
        if let value = launchDefaults?.agents?[agent]?.model, !value.isEmpty { return value }
        return defaultModel(for: agent)
    }

    /// [value] when it is one of [agent]'s models, else [fallback] — absent,
    /// blank (a pre-2026-10-15 "CLI default"), or a model this build cannot
    /// name.
    public static func seedModel(_ value: String?, for agent: String, fallback: String) -> String {
        guard let value, modelValues(for: agent).contains(value) else { return fallback }
        return value
    }

    /// Subagents are claude's (EXP-981) — codex has no such option, so the
    /// picker hides exactly like Ultracode does.
    public static func supportsSubagentModel(_ agent: String) -> Bool {
        agent == "claude"
    }

    /// The stored `launchDefaults.computerUseModel` as the picker's draft: a
    /// claude alias as itself, anything else the contract default.
    public static func seedComputerUseModel(_ advertised: String?) -> String {
        seedModel(advertised, for: "claude", fallback: DomainContract.deviceComputerUseDefaultsModel)
    }

    /// EXP-1005: auto-rotating accounts is a claude-only device setting.
    public static func supportsAccountRotation(_ agent: String) -> Bool {
        agent == "claude"
    }

    /// Plan mode is claude's own (EXP-441); codex has no launch-into-plan
    /// mode. EXP-849 dropped the second agent that had one.
    public static func supportsPlanMode(_ agent: String) -> Bool {
        agent == "claude"
    }

    /// The agents [device] can actually RUN, in contract order.
    public static func agents(of device: SteerDevice?) -> [String] {
        let supported = device?.agentIds ?? []
        return DomainContract.codingAgentValues.filter { supported.contains($0) }
    }

    /// The agent a machine starts on (EXP-437): its LAST USED agent
    /// (EXP-1158), clamped to what it advertises, else the first agent it
    /// runs. EXP-615 retired the trigger editors' "Device default" segment,
    /// so a binding seeds its strip with this and stores a CONCRETE agent.
    public static func lastUsedAgent(of device: SteerDevice?) -> String {
        let available = agents(of: device)
        if let advertised = device?.defaultLaunchAgent, available.contains(advertised) {
            return advertised
        }
        return available.first ?? ""
    }

    // MARK: - Labels

    /// EXP-849: an id outside the contract (a historical row, a future agent)
    /// falls through as itself rather than rendering blank.
    public static func agentLabel(_ value: String) -> String {
        switch value {
        case "claude": "Claude Code"
        case "codex": "Codex"
        default: value
        }
    }

    public static func modelLabel(_ value: String) -> String {
        switch value {
        case "gpt-5.6-sol": "GPT-5.6 Sol"
        case "gpt-5.6-terra": "GPT-5.6 Terra"
        case "gpt-5.6-luna": "GPT-5.6 Luna"
        case "grok-4.5": "Grok 4.5"
        default: value.prefix(1).uppercased() + value.dropFirst()
        }
    }

    public static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }

    /// The effort picker's TITLE follows the agent's own vocabulary.
    public static func effortTitle(for agent: String) -> String {
        switch agent {
        case "codex": "Reasoning"
        default: "Effort"
        }
    }

    /// Picker caption for a machine. A teammate's shared server (EXP-432) is
    /// attributed to its owner — two people's boxes can wear the same label,
    /// and a run lands on somebody else's hardware, so the picker says whose.
    /// EXP-615: no "(offline)" suffix anywhere — the trigger form offers
    /// every trigger-capable machine plainly (a sleeping box still owns the
    /// binding), and the run pickers only ever list online ones.
    public static func deviceCaption(_ device: SteerDevice) -> String {
        let name = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        guard let owner = device.owner else { return name }
        return "\(name) — \(owner.name)"
    }

    /// The machine's plain name, no owner attribution — what a sentence about
    /// it reads best with.
    public static func deviceName(_ device: SteerDevice) -> String {
        device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
    }

    /// EXP-773: the note under the options when the picked agent is outside
    /// the machine's reported ACP set — with the PTY path gone it cannot start
    /// there at all, so the caller also disables the start button. Nil when
    /// the machine never reported its ACP agents (unknown = assume all) or
    /// drives this one. Never a filter: the agent stays pickable.
    public static func notReadyNote(device: SteerDevice?, agent: String) -> String? {
        guard let device, !agent.isEmpty, device.agentNotReady(agent) else { return nil }
        return "Not ready on \(deviceName(device)). Run the doctor there."
    }
}
