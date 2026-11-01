import Foundation

// EXP-850 — the two latest-wins slots the steer wire gained beside the feed
// (`background_tasks`, `workflow`), and the working caption every client
// derives from the `turn` slot. Foundation only (the ExpCore rule), so
// ExpCoreTests drives all of it.

// MARK: - background_tasks (§2)

/// One background task the agent is running (a `&`-launched shell command, a
/// workflow, a spawned agent). `kind` is a contract `backgroundTaskKind`
/// value; anything this build does not know folds onto `other`.
public struct AgentBackgroundTask: Equatable, Sendable, Identifiable {
    public let id: String
    public let kind: String
    public let description: String
    /// The launching tool_use_id, once the CLI names one — the first list
    /// frame for a task carries none.
    public let toolId: String?
    /// EXP-1324: epoch ms the device saw it start (nil on older publishers:
    /// the client's first sight of the id stands in).
    public let startedAt: Int64?

    public init(
        id: String, kind: String, description: String, toolId: String? = nil,
        startedAt: Int64? = nil
    ) {
        self.id = id
        self.kind = kind
        self.description = description
        self.toolId = toolId
        self.startedAt = startedAt
    }

    /// A wire `backgroundTaskKind`, folded onto `other` for anything this
    /// build does not know (never fatal: a newer engine may name a new kind).
    public static func kind(_ raw: Any?) -> String {
        guard let value = raw as? String,
              DomainContract.backgroundTaskKindValues.contains(value) else { return "other" }
        return value
    }
}

// MARK: - task_list (§2c)

/// Contract `taskListStatus` — where one entry of the agent's own task list
/// stands.
public enum AgentTaskListStatus: String, Sendable, CaseIterable {
    case pending
    case inProgress = "in_progress"
    case completed

    /// A wire status, defaulting to `pending` — an entry this build cannot
    /// read is still work the agent has not reported done (never fatal).
    public static func parse(_ raw: Any?) -> AgentTaskListStatus {
        guard let value = raw as? String, let status = AgentTaskListStatus(rawValue: value)
        else { return .pending }
        return status
    }
}

/// EXP-927 §2c: one entry of the agent's OWN task list (claude's TodoWrite and
/// its `Task*` hooks, codex's plan) — the FIRST block of the bottom strip.
public struct AgentTaskListEntry: Equatable, Sendable {
    public let content: String
    public let status: AgentTaskListStatus

    public init(content: String, status: AgentTaskListStatus) {
        self.content = content
        self.status = status
    }
}

/// EXP-927 §2c: what the COLLAPSED task-list line says — the current entry and
/// its `{completed}/{total}` count. Byte-identical ×4 (web `TaskListSummary`,
/// desktop `session_rows::task_list_summary`).
public struct AgentTaskListSummary: Equatable, Sendable {
    public let current: String
    public let completed: Int
    public let total: Int

    public init(current: String, completed: Int, total: Int) {
        self.current = current
        self.completed = completed
        self.total = total
    }
}

// MARK: - workflow (§3)

/// Contract `workflowStatus`.
public enum AgentWorkflowStatus: String, Sendable, CaseIterable {
    case running
    case completed
    case failed
    case stopped

    /// A wire status, defaulting to `running` — a card whose status this build
    /// cannot read still draws its agents.
    public static func parse(_ raw: Any?) -> AgentWorkflowStatus {
        guard let value = raw as? String, let status = AgentWorkflowStatus(rawValue: value)
        else { return .running }
        return status
    }

    public var isTerminal: Bool { self != .running }
}

/// Contract `workflowAgentState`.
public enum AgentWorkflowAgentState: String, Sendable, CaseIterable {
    case queued
    case running
    case done
    case error

    public static func parse(_ raw: Any?) -> AgentWorkflowAgentState {
        guard let value = raw as? String, let state = AgentWorkflowAgentState(rawValue: value)
        else { return .queued }
        return state
    }

    /// `done` and `error` both count as finished (the caption's `done/total`).
    public var isFinished: Bool { self == .done || self == .error }
}

/// One phase of a workflow run.
public struct AgentWorkflowPhase: Equatable, Sendable, Identifiable {
    public let index: Int
    public let title: String

    public var id: Int { index }

    public init(index: Int, title: String) {
        self.index = index
        self.title = title
    }
}

/// One agent of a workflow run, latest-wins per `index`.
public struct AgentWorkflowAgent: Equatable, Sendable, Identifiable {
    public let index: Int
    public let label: String
    public let phaseIndex: Int?
    /// The subagent id its `subagent` edges publish under — how the card finds
    /// this agent's nested events.
    public let agentId: String?
    public let model: String?
    public let state: AgentWorkflowAgentState
    public let tokens: Int?
    public let toolCalls: Int?
    public let durationMs: Int?
    public let lastTool: String?
    public let lastToolSummary: String?
    public let resultPreview: String?
    public let error: String?

    public var id: Int { index }

    public init(
        index: Int,
        label: String,
        phaseIndex: Int? = nil,
        agentId: String? = nil,
        model: String? = nil,
        state: AgentWorkflowAgentState = .queued,
        tokens: Int? = nil,
        toolCalls: Int? = nil,
        durationMs: Int? = nil,
        lastTool: String? = nil,
        lastToolSummary: String? = nil,
        resultPreview: String? = nil,
        error: String? = nil
    ) {
        self.index = index
        self.label = label
        self.phaseIndex = phaseIndex
        self.agentId = agentId
        self.model = model
        self.state = state
        self.tokens = tokens
        self.toolCalls = toolCalls
        self.durationMs = durationMs
        self.lastTool = lastTool
        self.lastToolSummary = lastToolSummary
        self.resultPreview = resultPreview
        self.error = error
    }
}

/// EXP-850 §3: a claude `Workflow` tool call's progress, published as a
/// latest-wins `workflow` event keyed by the call's own id — so the card
/// renders IN PLACE of that tool row, never as a second row.
public struct AgentWorkflow: Equatable, Sendable, Identifiable {
    /// The `Workflow` tool_use_id — the same id as its `tool` row.
    public let id: String
    public let name: String
    public let description: String?
    public let status: AgentWorkflowStatus
    public let phases: [AgentWorkflowPhase]
    public let agents: [AgentWorkflowAgent]
    /// The terminal notification's own summary, when it carried one.
    public let summary: String?

    public init(
        id: String,
        name: String,
        description: String? = nil,
        status: AgentWorkflowStatus = .running,
        phases: [AgentWorkflowPhase] = [],
        agents: [AgentWorkflowAgent] = [],
        summary: String? = nil
    ) {
        self.id = id
        self.name = name
        self.description = description
        self.status = status
        self.phases = phases
        self.agents = agents
        self.summary = summary
    }

    /// §7's caption for this card, through the ONE shared derivation.
    public var caption: String {
        WorkflowCaption.caption(
            name: name,
            status: status.rawValue,
            phases: phases.map { WorkflowCaptionPhase(index: $0.index, title: $0.title) },
            agents: agents.map {
                WorkflowCaptionAgent(
                    index: $0.index, state: $0.state.rawValue, phaseIndex: $0.phaseIndex
                )
            }
        )
    }

    /// The agents of one phase, in publish order.
    public func agents(inPhase index: Int) -> [AgentWorkflowAgent] {
        agents.filter { $0.phaseIndex == index }
    }

    // MARK: - The Work card (EXP-1240, EXP-1259 D12)

    /// The card's phases, as the shared model reads them.
    private var workCardPhases: [WorkflowCaptionPhase] {
        phases.map { WorkflowCaptionPhase(index: $0.index, title: $0.title) }
    }

    /// The card's agents, as the shared model reads them — no tokens, no
    /// report text (the card shows neither).
    private var workCardAgents: [WorkCardWorkflowAgent] {
        agents.map {
            WorkCardWorkflowAgent(
                index: $0.index, label: $0.label, phaseIndex: $0.phaseIndex,
                model: $0.model, state: $0.state.rawValue, durationMs: $0.durationMs
            )
        }
    }

    /// THE Work card this workflow draws. The wire carries no start, so the
    /// elapsed time is the sum of its phases' spans (web `workflowWorkCard`).
    public var workCard: WorkCard {
        WorkCards.card(.workflow(
            name: name,
            status: status.rawValue,
            phases: workCardPhases,
            agents: workCardAgents,
            elapsedMs: WorkCards.workflowElapsedMs(phases: workCardPhases, agents: workCardAgents)
        ))
    }

    /// The phases the card's leading folded row stands for, for its in-place
    /// expansion. Empty when nothing folded.
    public func workCardFoldedPhases(_ card: WorkCard) -> [WorkCardFoldedPhase] {
        WorkCards.foldedPhases(phases: workCardPhases, agents: workCardAgents, card: card)
    }

    /// The subagent id behind a lane the card named (phase POSITION, lane
    /// position) — the tab a lane tap opens. Nil when that agent published
    /// no id.
    public func workCardLaneAgentId(phase: Int, lane: Int) -> String? {
        guard let picked = WorkCards.laneAgent(
            phases: workCardPhases, agents: workCardAgents, phase: phase, lane: lane
        ) else { return nil }
        return agents.first { $0.index == picked.index }?.agentId
    }

    /// Every agent id this card owns — the subagent runs it nests (and the
    /// tabs it must never offer).
    public var agentIds: Set<String> {
        Set(agents.compactMap(\.agentId))
    }
}

/// EXP-850 §1/§2: one line of the compact strip above the composer — a
/// background task, or a `wait` tool row that has not settled.
public struct AgentStripLine: Equatable, Sendable, Identifiable {
    public enum Kind: Equatable, Sendable {
        case backgroundTask
        case wait
    }

    public let id: String
    public let kind: Kind
    /// The line VERBATIM (`↻ {description}` / `Waiting on {detail}`), locked
    /// ×4 — nothing per client re-words it.
    public let text: String

    public init(id: String, kind: Kind, text: String) {
        self.id = id
        self.kind = kind
        self.text = text
    }
}

extension AgentFeed {
    // MARK: - Latest-wins workflow map (§3)

    /// Fold one `workflow` event into the card list: latest-wins per id, in
    /// FIRST-APPEARANCE order (desktop `SteerFeed::workflows`), so the cards
    /// never reshuffle under the reader.
    public static func applyWorkflow(
        _ current: [AgentWorkflow], workflow: AgentWorkflow
    ) -> [AgentWorkflow] {
        var next = current
        if let at = next.firstIndex(where: { $0.id == workflow.id }) {
            next[at] = workflow
        } else {
            next.append(workflow)
        }
        return next
    }

    /// The NEWEST still-running card — what the working caption and the
    /// synced `agent_caption` speak for (desktop `running_workflow`).
    public static func runningWorkflow(_ workflows: [AgentWorkflow]) -> AgentWorkflow? {
        workflows.last { $0.status == .running }
    }

    // MARK: - The bottom strip (§1/§2)

    /// EXP-850 §1: the contract `toolKind` a `TaskOutput`/`Monitor` call
    /// wears — the wait rows the strip reports. (The generated contract ships
    /// the LIST; this names the one value the strip keys on.)
    public static let toolKindWait = "wait"

    // A background-task line is the bare description; the repeat glyph is
    // drawn by the view (the same concept every client draws), never text.
    /// The lead of an open wait row's line. Byte-identical ×4.
    public static let waitingOnPrefix = "Waiting on "

    /// EXP-927: the `backgroundTaskKind` of a SUBAGENT. A subagent is a
    /// conversation TAB (Main + one per running lane), so the strip never
    /// lists it.
    public static let backgroundTaskKindAgent = "agent"

    /// The relay's `TASK_LIST_MAX`, re-applied because the wire is a device's
    /// word, not ours.
    public static let taskListMax = 50

    /// EXP-927 §2c: the collapsed task-list line, or nil when the block is
    /// hidden — no entries at all, or every one completed. `current` = the
    /// first `in_progress` entry, else the first `pending` one.
    public static func taskListSummary(
        _ entries: [AgentTaskListEntry]
    ) -> AgentTaskListSummary? {
        let open = entries.filter { $0.status != .completed }
        guard let first = open.first else { return nil }
        let current = open.first { $0.status == .inProgress } ?? first
        return AgentTaskListSummary(
            current: current.content,
            completed: entries.count - open.count,
            total: entries.count
        )
    }

    /// The strip above the composer: one line per background task, then one
    /// per UNSETTLED `wait` tool row, in feed order. Empty = no strip at all.
    ///
    /// A wait row whose publisher named no detail falls back to the tool's own
    /// name — `Waiting on ` alone says nothing.
    ///
    /// EXP-927: tasks of kind `agent` are left out — those are the
    /// conversation tabs.
    public static func stripLines(
        backgroundTasks: [AgentBackgroundTask], feed: [AgentFeedItem]
    ) -> [AgentStripLine] {
        var lines = backgroundTasks.filter { $0.kind != backgroundTaskKindAgent }.map { task in
            AgentStripLine(
                id: "task:\(task.id)",
                kind: .backgroundTask,
                text: task.description
            )
        }
        var seen = Set(lines.map(\.text))
        for item in feed {
            guard case let .tool(id, name, detail, _, callId, kind, settled, _, _, _, _, _, _) = item,
                  kind == toolKindWait, !settled else { continue }
            let label = (detail?.isEmpty == false) ? detail! : name
            let text = "\(waitingOnPrefix)\(label)"
            guard !seen.contains(text) else { continue }
            seen.insert(text)
            lines.append(AgentStripLine(id: "wait:\(callId ?? "\(id)")", kind: .wait, text: text))
        }
        return lines
    }

    // MARK: - Subagents as Work cards (EXP-1240)

    /// A subagent's run as THE Work card (web `subagentWorkCard`): its label,
    /// the calls it made (EXP-748: the reported count when it is higher),
    /// done or running.
    public static func subagentWorkCard(_ run: AgentSubagentRun) -> WorkCard {
        WorkCards.card(.subagent(
            title: run.label,
            state: run.done ? "done" : "running",
            toolCalls: run.toolCount,
            elapsedMs: nil
        ))
    }

    // MARK: - The working caption (§5)

    /// The bare label a run with no `turn.startedAt` shows — an older
    /// publisher sends no start stamp, and a verb needs one. Byte-identical ×4.
    public static let workingFallbackLabel = "Working…"

    /// EXP-1501: the head while a tool call's input STREAMS (`turn.writing`),
    /// `{tool}` = `writingToolLabel`; the blank-name variant beside it.
    /// Byte-identical ×4 (`fixtures/working-caption.json` `copy`).
    public static let workingWritingLabel = "Writing {tool}…"
    public static let workingWritingBlankLabel = "Writing a tool call…"

    /// EXP-1501: the tool name the writing caption shows — whitespace
    /// collapsed, an `mcp__<server>__` prefix dropped (everything after the
    /// second `__`); nil = blank.
    public static func writingToolLabel(_ tool: String) -> String? {
        let clean = tool.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        var label = clean
        if clean.hasPrefix("mcp__"), let split = clean.dropFirst(5).range(of: "__") {
            label = String(clean[split.upperBound...])
        }
        return label.isEmpty ? nil : label
    }

    /// The verb this turn wears: `verbs[startedAt % verbs.count]`, so every
    /// client picks the SAME word for the same turn and it never flickers.
    public static func workingVerb(startedAt: Int) -> String {
        let verbs = DomainContract.steerWorkingVerbs
        guard !verbs.isEmpty else { return "Working" }
        let index = abs(startedAt % verbs.count)
        return verbs[index]
    }

    /// `37s` / `2m 04s` / `1h 03m` — the ×4 duration format. A negative span
    /// (clock skew) reads as `0s`.
    public static func workingDuration(ms: Int) -> String {
        let seconds = max(0, ms / 1000)
        if seconds < 60 { return "\(seconds)s" }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes)m \(pad2(seconds % 60))s" }
        return "\(minutes / 60)h \(pad2(minutes % 60))m"
    }

    /// `812` / `2.0k` / `1.2M` — the ×4 token format. Truncated, never
    /// rounded up, so a count can never read `1000.0k`.
    public static func workingTokens(_ tokens: Int) -> String {
        let count = max(0, tokens)
        if count < 1000 { return "\(count)" }
        if count < 1_000_000 { return "\(tenths(count, per: 100))k" }
        return "\(tenths(count, per: 100_000))M"
    }

    /// `{verb}… ({duration} · ↓ {tokens} tokens)` — the trailing working row's
    /// ONE caption (§5). While a workflow RUNS the head is its §7 caption
    /// instead of a verb, with the same suffix.
    ///
    /// No `startedAt` (an older publisher) falls back to `Working…` with no
    /// suffix at all; a known start with no token count keeps the duration and
    /// drops the ` · ↓ N tokens` half.
    ///
    /// EXP-1501: while a tool call's input streams (`writing`) the head reads
    /// `Writing {tool}…` and the duration runs from the STREAM's start, not the
    /// turn's (`fixtures/working-caption.json`).
    public static func workingCaption(
        startedAt: Int?, tokens: Int?, now: Date, workflow: AgentWorkflow? = nil,
        writing: WorkingWriting? = nil
    ) -> String {
        let head: String
        if let caption = workflow?.caption {
            head = caption
        } else if let writing {
            head = writingToolLabel(writing.tool).map {
                workingWritingLabel.replacingOccurrences(of: "{tool}", with: $0)
            } ?? workingWritingBlankLabel
        } else if let startedAt {
            head = "\(workingVerb(startedAt: startedAt))…"
        } else {
            return workingFallbackLabel
        }
        guard let clockStart = writing?.startedAt ?? startedAt else { return head }
        let elapsed = Int(now.timeIntervalSince1970 * 1000) - clockStart
        var group = workingDuration(ms: elapsed)
        if let tokens, tokens > 0 {
            group += "\(WorkflowCaption.separator)↓ \(workingTokens(tokens)) tokens"
        }
        return "\(head) (\(group))"
    }

    private static func pad2(_ value: Int) -> String {
        value < 10 ? "0\(value)" : "\(value)"
    }

    /// One decimal place, TRUNCATED: 2000/100 = 20 → `2.0`, 1_234_567/100_000
    /// = 12 → `1.2`.
    private static func tenths(_ value: Int, per: Int) -> String {
        let scaled = value / per
        return "\(scaled / 10).\(scaled % 10)"
    }
}
