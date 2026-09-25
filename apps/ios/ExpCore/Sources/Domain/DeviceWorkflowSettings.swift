import Foundation

/// EXP-1029/EXP-1042: the WORKFLOW model pair a machine seeds new workflows
/// from (`launch_defaults.workflow`) — the cheap `model` that runs leaf nodes
/// and the subagents inside them, and the `strongModel` that runs contract,
/// integration and risky nodes and every review.
///
/// The pair belongs to the machine's DEFAULT AGENT: a model id is one agent's
/// vocabulary, so a stored value counts only while that agent is the default
/// (switching the default account to the other agent falls the pair back to
/// the new agent's own defaults rather than sending it an id it cannot run).
/// One resolver ×4, so the device settings sheet and every later reader agree
/// on what an absent, stale or foreign value means.
public enum DeviceWorkflowSettings {
    /// The agent's contract model vocabulary — no "CLI default" blank: a
    /// workflow always names a concrete model on both rungs.
    public static func modelValues(for agent: String) -> [String] {
        switch agent {
        case "codex": return DomainContract.codexModelValues
        default: return DomainContract.codingModelValues
        }
    }

    /// The contract's own per-agent defaults. An agent this build has no
    /// workflow vocabulary for falls back to the device-defaults pair, which
    /// is claude's.
    public static func defaults(for agent: String) -> (model: String, strongModel: String) {
        switch agent {
        case "claude":
            return (
                DomainContract.workflowLaunchClaudeModel,
                DomainContract.workflowLaunchClaudeStrongModel
            )
        case "codex":
            return (
                DomainContract.workflowLaunchCodexModel,
                DomainContract.workflowLaunchCodexStrongModel
            )
        default:
            return (
                DomainContract.deviceAgentDefaultsWorkflowModel,
                DomainContract.deviceAgentDefaultsWorkflowStrongModel
            )
        }
    }

    /// The pair to render and to store: a stored value survives only when it
    /// is one of THAT agent's contract models; anything else (absent, a
    /// blank, the other agent's id, a model this build retired) reads as the
    /// agent's default.
    public static func resolve(
        agent: String,
        stored: DeviceWorkflowDefaults?
    ) -> (model: String, strongModel: String) {
        let fallback = defaults(for: agent)
        let values = modelValues(for: agent)
        func pick(_ value: String?, else fallbackValue: String) -> String {
            guard let value, values.contains(value) else { return fallbackValue }
            return value
        }
        return (
            pick(stored?.model, else: fallback.model),
            pick(stored?.strongModel, else: fallback.strongModel)
        )
    }
}
