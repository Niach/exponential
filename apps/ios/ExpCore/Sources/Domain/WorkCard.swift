import Foundation

// EXP-1240 (EXP-1259 D12): THE Work card — ONE card for a claude workflow, a
// task list and a subagent, replacing the workflow card, the task-list block
// and the subagent group row. This is the pure MODEL the card draws; ExpUI
// `WorkCardView` paints it. Hand-mirrored ×4 (TS
// `packages/domain-contract/src/work-card.ts`, desktop `steer::work_card`,
// Android `domain/WorkCard.kt`) and byte-locked by
// `packages/domain-contract/fixtures/work-card.json`, which
// `WorkCardTests` replays (whole objects, `fill` within 1e-9).
//
// The card: a HEADER (kind icon, name, kind word, progress, elapsed, a spinner
// while running), a SEGMENT BAR (one segment per phase or task, never for a
// subagent) and ROWS. Finished leading phases FOLD into one row; the running
// (or failed) phase lists its lanes one line each; a done phase after it is
// one collapsed row, a pending phase a bare title. No report text, no token
// counts, no JSON. The session-list caption (`WorkflowCaption`) is a
// different surface and keeps its own fixture.

/// One agent of a workflow as the card reads it.
public struct WorkCardWorkflowAgent: Equatable, Sendable {
    public let index: Int
    public let label: String?
    public let phaseIndex: Int?
    public let model: String?
    /// Contract `workflowAgentState`: queued | running | done | error.
    public let state: String
    public let durationMs: Int?

    public init(
        index: Int, label: String? = nil, phaseIndex: Int? = nil, model: String? = nil,
        state: String, durationMs: Int? = nil
    ) {
        self.index = index
        self.label = label
        self.phaseIndex = phaseIndex
        self.model = model
        self.state = state
        self.durationMs = durationMs
    }
}

/// One entry of a task list as the card reads it.
public struct WorkCardTask: Equatable, Sendable {
    public let content: String
    /// pending | in_progress | completed.
    public let status: String

    public init(content: String, status: String) {
        self.content = content
        self.status = status
    }
}

/// What a Work card is drawn from.
public enum WorkCardInput: Equatable, Sendable {
    case workflow(
        name: String, status: String, phases: [WorkflowCaptionPhase],
        agents: [WorkCardWorkflowAgent], elapsedMs: Int?
    )
    case tasks([WorkCardTask])
    case subagent(title: String?, state: String, toolCalls: Int?, elapsedMs: Int?)
}

/// A phase's, a task's or a row's state.
public enum WorkState: String, Equatable, Sendable {
    case done
    case running
    case pending
    case failed
}

/// One capsule of the segment bar.
public struct WorkCardSegment: Equatable, Sendable {
    public let state: WorkState
    /// 0...1 — how much of the segment is filled.
    public let fill: Double

    public init(state: WorkState, fill: Double) {
        self.state = state
        self.fill = fill
    }
}

/// One lane line under a running or failed phase.
public struct WorkCardLane: Equatable, Sendable {
    public let label: String
    public let model: String?
    /// queued | running | done | error.
    public let state: String
    public let duration: String?

    public init(label: String, model: String?, state: String, duration: String?) {
        self.label = label
        self.model = model
        self.state = state
        self.duration = duration
    }
}

/// One row under the header.
public enum WorkCardRow: Equatable, Sendable {
    case folded(label: String, lanes: String, duration: String?)
    case phase(
        title: String, state: WorkState, detail: String?, duration: String?,
        lanes: [WorkCardLane]
    )
    case task(label: String, state: WorkState)
    case status(label: String)
}

/// The kind of thing a card stands for.
public enum WorkCardKind: String, Equatable, Sendable {
    case workflow
    case tasks
    case subagent
}

public struct WorkCard: Equatable, Sendable {
    public let kind: WorkCardKind
    /// A concept icon (packages/icons/icons.json).
    public let icon: String
    public let name: String
    public let kindWord: String?
    public let progress: String?
    public let elapsed: String?
    public let running: Bool
    public let segments: [WorkCardSegment]
    public let rows: [WorkCardRow]

    public init(
        kind: WorkCardKind, icon: String, name: String, kindWord: String?,
        progress: String?, elapsed: String?, running: Bool,
        segments: [WorkCardSegment], rows: [WorkCardRow]
    ) {
        self.kind = kind
        self.icon = icon
        self.name = name
        self.kindWord = kindWord
        self.progress = progress
        self.elapsed = elapsed
        self.running = running
        self.segments = segments
        self.rows = rows
    }

    /// The phase POSITION each row stands for: a leading folded row covers the
    /// first `segments - (rows - 1)` phases, every other row is one phase.
    /// (Web `rowPhases`.) Empty for anything but a workflow.
    public var rowPhases: [Int] {
        guard kind == .workflow else { return [] }
        let folded: Bool
        if case .folded = rows.first { folded = true } else { folded = false }
        let foldedCount = folded ? segments.count - (rows.count - 1) : 0
        return rows.indices.map { index in
            folded ? (index == 0 ? 0 : foldedCount + index - 1) : index
        }
    }

    /// How many phases a leading folded row stands for (0 = none folded).
    public var foldedPhaseCount: Int {
        guard case .folded = rows.first else { return 0 }
        return segments.count - (rows.count - 1)
    }
}

/// One folded phase spelled out for the folded row's in-place expansion (the
/// model folds them into one row and drops their lanes).
public struct WorkCardFoldedPhase: Equatable, Sendable {
    public let title: String
    public let duration: String?
    public let lanes: [WorkCardLane]

    public init(title: String, duration: String?, lanes: [WorkCardLane]) {
        self.title = title
        self.duration = duration
        self.lanes = lanes
    }
}

public enum WorkCards {
    public static let separator = " · "

    /// Fixture `copy`.
    public enum Copy {
        public static let workflow = "Workflow"
        public static let tasks = "Tasks"
        public static let subagent = "Subagent"
        public static let phase = "Phase"
        public static let loosePhase = "Agents"
        public static let agent = "Agent"
        public static let lane = "lane"
        public static let lanes = "lanes"
        public static let call = "call"
        public static let calls = "calls"
        public static let running = "running"
        public static let done = "done"
        public static let failed = "failed"
    }

    /// Fixture `icons` (concept names).
    public enum Icons {
        public static let workflow = "session-tree"
        public static let tasks = "ui-checklist"
        public static let subagent = "coding-subagent"
    }

    /// Fixture `geometry` (points).
    public enum Geometry {
        public static let header: Double = 40
        public static let row: Double = 32
        public static let lane: Double = 28
        public static let laneIndent: Double = 28
        public static let segmentHeight: Double = 3
        public static let segmentGap: Double = 4
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let card = "work-card"
        public static let lane = "work-card-lane"
        public static let folded = "work-card-folded"
    }

    /// Coarse: `42s`, `48m`, `1h 12m`, `2h`. Header + folded rows.
    public static func durationShort(ms: Int) -> String {
        let seconds = max(0, ms / 1000)
        if seconds < 60 { return "\(seconds)s" }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes)m" }
        let hours = minutes / 60
        let rest = minutes % 60
        return rest == 0 ? "\(hours)h" : "\(hours)h \(rest)m"
    }

    /// Fine: `42s`, `8m 29s`, `1h 12m`. Lane rows.
    public static func durationLong(ms: Int) -> String {
        let seconds = max(0, ms / 1000)
        if seconds < 60 { return "\(seconds)s" }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes)m \(seconds % 60)s" }
        return "\(minutes / 60)h \(minutes % 60)m"
    }

    /// `claude-opus-5-5` → `opus`; anything else as given; blank → nil.
    public static func modelLabel(_ model: String?) -> String? {
        let trimmed = (model ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let prefix = "claude-"
        guard trimmed.hasPrefix(prefix) else { return trimmed }
        let family = trimmed.dropFirst(prefix.count).prefix { ("a"..."z").contains($0) }
        return family.isEmpty ? trimmed : String(family)
    }

    public static func card(_ input: WorkCardInput) -> WorkCard {
        switch input {
        case let .workflow(name, status, phases, agents, elapsedMs):
            return workflowCard(
                name: name, status: status, phases: phases, agents: agents, elapsedMs: elapsedMs
            )
        case let .tasks(tasks):
            return tasksCard(tasks)
        case let .subagent(title, state, toolCalls, elapsedMs):
            return subagentCard(title: title, state: state, toolCalls: toolCalls, elapsedMs: elapsedMs)
        }
    }

    // MARK: - Workflow phases

    /// A workflow's agents grouped the way the card groups them: the declared
    /// phases by index, each one's agents by index, then the agents outside
    /// every declared phase as a trailing `Agents` phase. Position N here is
    /// segment N on the card (web `workflowPhaseGroups`).
    public static func phaseGroups(
        phases: [WorkflowCaptionPhase], agents: [WorkCardWorkflowAgent]
    ) -> [(title: String, agents: [WorkCardWorkflowAgent])] {
        let sortedAgents = agents.sorted { $0.index < $1.index }
        let declared = phases.sorted { $0.index < $1.index }
        var groups: [(title: String, agents: [WorkCardWorkflowAgent])] = declared.map { phase in
            let title = phase.title.trimmingCharacters(in: .whitespacesAndNewlines)
            return (
                title: title.isEmpty ? "\(Copy.phase) \(phase.index)" : title,
                agents: sortedAgents.filter { $0.phaseIndex == phase.index }
            )
        }
        let known = Set(declared.map(\.index))
        let loose = sortedAgents.filter { agent in
            guard let phaseIndex = agent.phaseIndex else { return true }
            return !known.contains(phaseIndex)
        }
        if !loose.isEmpty { groups.append((title: Copy.loosePhase, agents: loose)) }
        return groups
    }

    /// The longest settled lane of a phase: its wall time.
    public static func phaseSpanMs(_ agents: [WorkCardWorkflowAgent]) -> Int? {
        var span: Int?
        for agent in agents {
            if let duration = agent.durationMs { span = max(span ?? 0, duration) }
        }
        return span
    }

    /// The workflow's elapsed time. The wire carries no start, so it is the
    /// sum of its phases' spans (they run in sequence); nil before any lane
    /// settled (web `workflowElapsedMs`).
    public static func workflowElapsedMs(
        phases: [WorkflowCaptionPhase], agents: [WorkCardWorkflowAgent]
    ) -> Int? {
        var total: Int?
        for group in phaseGroups(phases: phases, agents: agents) {
            if let span = phaseSpanMs(group.agents) { total = (total ?? 0) + span }
        }
        return total
    }

    /// The phases a card's leading folded row stands for, spelled out for its
    /// in-place expansion. Empty when nothing folded.
    public static func foldedPhases(
        phases: [WorkflowCaptionPhase], agents: [WorkCardWorkflowAgent], card: WorkCard
    ) -> [WorkCardFoldedPhase] {
        let count = card.foldedPhaseCount
        guard count > 0 else { return [] }
        return phaseGroups(phases: phases, agents: agents).prefix(count).map { group in
            WorkCardFoldedPhase(
                title: group.title,
                duration: phaseSpanMs(group.agents).map { durationShort(ms: $0) },
                lanes: group.agents.map(lane)
            )
        }
    }

    /// The agent behind a lane the card named: `phase` = the phase POSITION,
    /// `lane` = the lane's position inside it.
    public static func laneAgent(
        phases: [WorkflowCaptionPhase], agents: [WorkCardWorkflowAgent], phase: Int, lane: Int
    ) -> WorkCardWorkflowAgent? {
        let groups = phaseGroups(phases: phases, agents: agents)
        guard groups.indices.contains(phase), groups[phase].agents.indices.contains(lane)
        else { return nil }
        return groups[phase].agents[lane]
    }

    private static func plural(_ count: Int, _ one: String, _ many: String) -> String {
        "\(count) \(count == 1 ? one : many)"
    }

    private static func settled(_ state: String) -> Bool { state == "done" || state == "error" }

    private static func phaseState(_ agents: [WorkCardWorkflowAgent]) -> WorkState {
        if agents.isEmpty { return .pending }
        if agents.allSatisfy({ settled($0.state) }) {
            return agents.contains { $0.state == "error" } ? .failed : .done
        }
        return .running
    }

    private static func phaseDetail(_ agents: [WorkCardWorkflowAgent]) -> String {
        func count(_ state: String) -> Int { agents.filter { $0.state == state }.count }
        var parts: [String] = []
        let done = count("done")
        let failed = count("error")
        let running = count("running")
        let queued = count("queued")
        if done > 0 { parts.append("\(done) done") }
        if failed > 0 { parts.append("\(failed) failed") }
        if running > 0 { parts.append("\(running) running") }
        if queued > 0 { parts.append("\(queued) queued") }
        return parts.joined(separator: separator)
    }

    private static func lane(_ agent: WorkCardWorkflowAgent) -> WorkCardLane {
        let label = (agent.label ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return WorkCardLane(
            label: label.isEmpty ? "\(Copy.agent) \(agent.index)" : label,
            model: modelLabel(agent.model),
            state: agent.state,
            duration: settled(agent.state) ? agent.durationMs.map { durationLong(ms: $0) } : nil
        )
    }

    private static func workflowCard(
        name: String, status: String, phases declared: [WorkflowCaptionPhase],
        agents: [WorkCardWorkflowAgent], elapsedMs: Int?
    ) -> WorkCard {
        let groups = phaseGroups(phases: declared, agents: agents)
        let states = groups.map { phaseState($0.agents) }
        let segments = groups.enumerated().map { i, group -> WorkCardSegment in
            let done = group.agents.filter { settled($0.state) }.count
            return WorkCardSegment(
                state: states[i],
                fill: group.agents.isEmpty ? 0 : Double(done) / Double(group.agents.count)
            )
        }

        var rows: [WorkCardRow] = []
        var cursor = 0
        while cursor < groups.count, states[cursor] == .done { cursor += 1 }
        if cursor > 0 {
            let folded = groups[0..<cursor]
            let spans = folded.compactMap { phaseSpanMs($0.agents) }
            rows.append(.folded(
                label: folded.map(\.title).joined(separator: separator),
                lanes: plural(folded.reduce(0) { $0 + $1.agents.count }, Copy.lane, Copy.lanes),
                duration: spans.isEmpty ? nil : durationShort(ms: spans.reduce(0, +))
            ))
        }
        for i in cursor..<groups.count {
            let group = groups[i]
            let state = states[i]
            let span = phaseSpanMs(group.agents)
            switch state {
            case .pending:
                rows.append(.phase(title: group.title, state: state, detail: nil, duration: nil, lanes: []))
            case .done:
                rows.append(.phase(
                    title: group.title,
                    state: state,
                    detail: plural(group.agents.count, Copy.lane, Copy.lanes),
                    duration: span.map { durationShort(ms: $0) },
                    lanes: []
                ))
            case .running, .failed:
                rows.append(.phase(
                    title: group.title,
                    state: state,
                    detail: phaseDetail(group.agents),
                    duration: state == .failed ? span.map { durationShort(ms: $0) } : nil,
                    lanes: group.agents.map(lane)
                ))
            }
        }

        let settledCount = agents.filter { settled($0.state) }.count
        return WorkCard(
            kind: .workflow,
            icon: Icons.workflow,
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            kindWord: Copy.workflow,
            progress: agents.isEmpty ? nil : "\(settledCount) / \(agents.count)",
            elapsed: elapsedMs.map { durationShort(ms: $0) },
            running: status == "running",
            segments: segments,
            rows: rows
        )
    }

    private static func tasksCard(_ tasks: [WorkCardTask]) -> WorkCard {
        func stateOf(_ task: WorkCardTask) -> WorkState {
            switch task.status {
            case "completed": .done
            case "in_progress": .running
            default: .pending
            }
        }
        let active = tasks.filter { $0.status == "in_progress" }
        let shown = active.isEmpty ? Array(tasks.filter { stateOf($0) == .pending }.prefix(1)) : active
        let completed = tasks.filter { $0.status == "completed" }.count
        return WorkCard(
            kind: .tasks,
            icon: Icons.tasks,
            name: Copy.tasks,
            kindWord: nil,
            progress: tasks.isEmpty ? nil : "\(completed) / \(tasks.count)",
            elapsed: nil,
            running: !active.isEmpty,
            segments: tasks.map { task in
                let state = stateOf(task)
                return WorkCardSegment(
                    state: state, fill: state == .done ? 1 : state == .running ? 0.5 : 0
                )
            },
            rows: shown.map { task in
                .task(
                    label: task.content.trimmingCharacters(in: .whitespacesAndNewlines),
                    state: stateOf(task)
                )
            }
        )
    }

    private static func subagentCard(
        title: String?, state: String, toolCalls: Int?, elapsedMs: Int?
    ) -> WorkCard {
        let word = state == "running" ? Copy.running : state == "error" ? Copy.failed : Copy.done
        let name = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return WorkCard(
            kind: .subagent,
            icon: Icons.subagent,
            name: name.isEmpty ? Copy.subagent : name,
            kindWord: Copy.subagent,
            progress: plural(toolCalls ?? 0, Copy.call, Copy.calls),
            elapsed: elapsedMs.map { durationShort(ms: $0) },
            running: state == "running",
            segments: [],
            rows: [.status(label: "\(Copy.subagent)\(separator)\(word)")]
        )
    }
}
