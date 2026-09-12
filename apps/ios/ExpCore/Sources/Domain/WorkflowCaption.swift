import Foundation

/// One phase of a workflow as the caption sees it (`workflow.phases`).
public struct WorkflowCaptionPhase: Equatable, Sendable {
    public let index: Int
    public let title: String

    public init(index: Int, title: String) {
        self.index = index
        self.title = title
    }
}

/// One agent of a workflow as the caption sees it (`workflow.agents`).
public struct WorkflowCaptionAgent: Equatable, Sendable {
    public let index: Int
    /// A contract `workflowAgentState` value (`queued`/`running`/`done`/`error`).
    public let state: String
    public let phaseIndex: Int?

    public init(index: Int, state: String, phaseIndex: Int? = nil) {
        self.index = index
        self.state = state
        self.phaseIndex = phaseIndex
    }
}

/// EXP-850 §7: the ONE caption a running (or finished) workflow renders — the
/// session-list second line (`coding_sessions.agent_caption`) and the working
/// caption inside the steer view.
///
/// Hand-mirrored ×4 (TS `packages/domain-contract/src/workflow-caption.ts`,
/// desktop `steer::workflow_caption`, Android `domain/WorkflowCaption.kt`) and
/// byte-locked by `packages/domain-contract/fixtures/workflow-caption.json`,
/// which every client's test replays.
///
/// Rules:
/// - `running` with no agents yet → `Workflow {name} · starting`
/// - `running` → `Workflow {name} · {done}/{total} agents done · {phase}` with
///   done = agents in `done` or `error`, total = agents.count, and phase the
///   title of the RUNNING agent with the highest index (else of the
///   highest-index agent); the ` · {phase}` segment is omitted when that agent
///   names no phase title.
/// - `completed` → `Workflow {name} · done · {total} agents`
/// - `failed` → `Workflow {name} · failed`
/// - `stopped` (and any unknown status) → `Workflow {name} · stopped`
public enum WorkflowCaption {
    /// The segment separator: space, MIDDLE DOT (U+00B7), space.
    public static let separator = " · "

    public static func caption(
        name: String,
        status: String,
        phases: [WorkflowCaptionPhase] = [],
        agents: [WorkflowCaptionAgent] = []
    ) -> String {
        let head = "Workflow \(name.trimmingCharacters(in: .whitespacesAndNewlines))"
        let total = agents.count
        switch status {
        case "completed":
            return "\(head)\(separator)done\(separator)\(total) \(total == 1 ? "agent" : "agents")"
        case "failed":
            return "\(head)\(separator)failed"
        case "running":
            guard total > 0 else { return "\(head)\(separator)starting" }
            let done = agents.filter { $0.state == "done" || $0.state == "error" }.count
            let caption = "\(head)\(separator)\(done)/\(total) agents done"
            guard let phase = phaseTitle(leadAgent(agents), phases: phases) else { return caption }
            return "\(caption)\(separator)\(phase)"
        default:
            // `stopped`, and anything a newer contract adds: a run that is over
            // and did not say it succeeded reads as stopped, never as running.
            return "\(head)\(separator)stopped"
        }
    }

    /// The agent whose phase names the caption: the running one with the
    /// highest index, else the highest-index agent of any state.
    private static func leadAgent(_ agents: [WorkflowCaptionAgent]) -> WorkflowCaptionAgent? {
        var running: WorkflowCaptionAgent?
        var latest: WorkflowCaptionAgent?
        for agent in agents {
            if latest == nil || agent.index >= latest!.index { latest = agent }
            if agent.state == "running", running == nil || agent.index >= running!.index {
                running = agent
            }
        }
        return running ?? latest
    }

    private static func phaseTitle(
        _ agent: WorkflowCaptionAgent?, phases: [WorkflowCaptionPhase]
    ) -> String? {
        guard let agent, let phaseIndex = agent.phaseIndex,
              let phase = phases.first(where: { $0.index == phaseIndex })
        else { return nil }
        let title = phase.title.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? nil : title
    }
}
