import ExpCore
import Foundation

// EXP-615: the ONE launch vocabulary. Agent / model / effort labels, the
// per-agent option lists, the plan-mode capability and the picker captions
// used to live three times over — StartCodingSheet's statics, a private copy
// inside DeviceSettingsSheet, and a third set of shims in AutomationFormSheet.
// Every launch-shaped surface (Start coding, Chat, Create action, the
// automation editor, device launch defaults) now reads them from here, so a
// contract change lands once.
enum LaunchVocabulary {
    /// Sentinel for the blank "CLI default" choice (omit --effort; for
    /// codex/pi also the omit-model default — claude is explicit-always).
    static let cliDefault = "cli-default"

    /// Sentinel for "use the device's launch defaults" in the automation
    /// variant (agent/model/effort travel to the server as null).
    static let deviceDefault = ""

    // MARK: - Option lists

    /// Claude's model is explicit-always; codex/pi offer a "CLI default"
    /// blank. Parameterized because a seed validates an advertised value
    /// against the agent it belongs to, which isn't always the selected one
    /// yet (EXP-437).
    static func modelValues(for agent: String) -> [String] {
        switch agent {
        case "codex": [cliDefault] + DomainContract.codexModelValues
        case "pi": [cliDefault] + DomainContract.piModelValues
        default: DomainContract.codingModelValues
        }
    }

    static func effortValues(for agent: String) -> [String] {
        switch agent {
        case "codex": DomainContract.codexEffortValues
        case "pi": DomainContract.piThinkingValues
        default: DomainContract.codingEffortValues
        }
    }

    /// The automation variant's model list: its own "Device default" row
    /// replaces the run pickers' "CLI default" sentinel, so drop that entry.
    static func automationModelValues(for agent: String) -> [String] {
        modelValues(for: agent).filter { $0 != cliDefault }
    }

    static func defaultModel(for agent: String) -> String {
        agent == "claude" ? (DomainContract.codingModelValues.first ?? "") : cliDefault
    }

    /// Plan mode is claude (native) + pi (via the launcher-injected
    /// extension, EXP-441); codex has no launch-into-plan mode.
    static func supportsPlanMode(_ agent: String) -> Bool {
        agent == "claude" || agent == "pi"
    }

    /// The agents [device] can actually RUN, in contract order.
    static func agents(of device: SteerDevice?) -> [String] {
        let supported = device?.agentIds ?? []
        return DomainContract.codingAgentValues.filter { supported.contains($0) }
    }

    // MARK: - Labels

    static func agentLabel(_ value: String) -> String {
        switch value {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "pi": "pi"
        default: value
        }
    }

    static func modelLabel(_ value: String) -> String {
        switch value {
        case cliDefault: "CLI default"
        case "gpt-5.6-sol": "GPT-5.6 Sol"
        case "gpt-5.6-terra": "GPT-5.6 Terra"
        case "gpt-5.6-luna": "GPT-5.6 Luna"
        case "grok-4.5": "Grok 4.5"
        default: value.prefix(1).uppercased() + value.dropFirst()
        }
    }

    static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }

    /// The effort picker's TITLE follows the agent's own vocabulary.
    static func effortTitle(for agent: String) -> String {
        switch agent {
        case "codex": "Reasoning"
        case "pi": "Thinking"
        default: "Effort"
        }
    }

    /// Picker caption for a machine. A teammate's shared server (EXP-432) is
    /// attributed to its owner — two people's boxes can wear the same label,
    /// and a run lands on somebody else's hardware, so the picker says whose.
    /// EXP-615: no "(offline)" suffix anywhere — the automation editor offers
    /// every automation-capable machine plainly (a sleeping box still owns the
    /// binding), and the run pickers only ever list online ones.
    static func deviceCaption(_ device: SteerDevice) -> String {
        let name = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        guard let owner = device.owner else { return name }
        return "\(name) — \(owner.name)"
    }
}
