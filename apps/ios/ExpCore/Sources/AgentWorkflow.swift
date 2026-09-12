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

    public init(id: String, kind: String, description: String, toolId: String? = nil) {
        self.id = id
        self.kind = kind
        self.description = description
        self.toolId = toolId
    }

    /// A wire `backgroundTaskKind`, folded onto `other` for anything this
    /// build does not know (never fatal: a newer engine may name a new kind).
    public static func kind(_ raw: Any?) -> String {
        guard let value = raw as? String,
              DomainContract.backgroundTaskKindValues.contains(value) else { return "other" }
        return value
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

/// How many agents of one phase sit in each state — the phase strip's counts.
public struct AgentWorkflowPhaseCounts: Equatable, Sendable {
    public let queued: Int
    public let running: Int
    public let done: Int
    public let error: Int

    public init(queued: Int = 0, running: Int = 0, done: Int = 0, error: Int = 0) {
        self.queued = queued
        self.running = running
        self.done = done
        self.error = error
    }

    public var total: Int { queued + running + done + error }
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

    /// The phase strip's per-state counts.
    public func counts(inPhase index: Int) -> AgentWorkflowPhaseCounts {
        let rows = agents(inPhase: index)
        return AgentWorkflowPhaseCounts(
            queued: rows.filter { $0.state == .queued }.count,
            running: rows.filter { $0.state == .running }.count,
            done: rows.filter { $0.state == .done }.count,
            error: rows.filter { $0.state == .error }.count
        )
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

    /// The strip above the composer: one line per background task, then one
    /// per UNSETTLED `wait` tool row, in feed order. Empty = no strip at all.
    ///
    /// A wait row whose publisher named no detail falls back to the tool's own
    /// name — `Waiting on ` alone says nothing.
    public static func stripLines(
        backgroundTasks: [AgentBackgroundTask], feed: [AgentFeedItem]
    ) -> [AgentStripLine] {
        var lines = backgroundTasks.map { task in
            AgentStripLine(
                id: "task:\(task.id)",
                kind: .backgroundTask,
                text: task.description
            )
        }
        var seen = Set(lines.map(\.text))
        for item in feed {
            guard case let .tool(id, name, detail, _, callId, kind, settled, _, _, _) = item,
                  kind == toolKindWait, !settled else { continue }
            let label = (detail?.isEmpty == false) ? detail! : name
            let text = "\(waitingOnPrefix)\(label)"
            guard !seen.contains(text) else { continue }
            seen.insert(text)
            lines.append(AgentStripLine(id: "wait:\(callId ?? "\(id)")", kind: .wait, text: text))
        }
        return lines
    }

    // MARK: - The workflow card's captions (§3)

    /// One phase chip's counts: `2 done · 1 running · 1 queued · 1 failed`.
    /// Zero segments are omitted and the order is fixed, so two phases always
    /// read the same way round.
    public static func workflowPhaseCaption(_ counts: AgentWorkflowPhaseCounts) -> String {
        var segments: [String] = []
        if counts.done > 0 { segments.append("\(counts.done) done") }
        if counts.running > 0 { segments.append("\(counts.running) running") }
        if counts.queued > 0 { segments.append("\(counts.queued) queued") }
        if counts.error > 0 { segments.append("\(counts.error) failed") }
        return segments.joined(separator: WorkflowCaption.separator)
    }

    /// One agent row's trailing telemetry: `12.4k tokens · 7 tools · 1m 05s`,
    /// with whatever the agent did not report left out. Nil = nothing to draw.
    public static func workflowAgentTelemetry(_ agent: AgentWorkflowAgent) -> String? {
        var segments: [String] = []
        if let tokens = agent.tokens, tokens > 0 {
            segments.append("\(workingTokens(tokens)) tokens")
        }
        if let calls = agent.toolCalls, calls > 0 {
            segments.append("\(calls) tool\(calls == 1 ? "" : "s")")
        }
        if let duration = agent.durationMs, duration > 0 {
            segments.append(workingDuration(ms: duration))
        }
        return segments.isEmpty ? nil : segments.joined(separator: WorkflowCaption.separator)
    }

    // MARK: - The working caption (§5)

    /// The bare label a run with no `turn.startedAt` shows — an older
    /// publisher sends no start stamp, and a verb needs one. Byte-identical ×4.
    public static let workingFallbackLabel = "Working…"

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
    public static func workingCaption(
        startedAt: Int?, tokens: Int?, now: Date, workflow: AgentWorkflow? = nil
    ) -> String {
        let head = workflow?.caption
        guard let startedAt else { return head ?? workingFallbackLabel }
        let lead = head ?? "\(workingVerb(startedAt: startedAt))…"
        let elapsed = Int(now.timeIntervalSince1970 * 1000) - startedAt
        var group = workingDuration(ms: elapsed)
        if let tokens, tokens > 0 {
            group += "\(WorkflowCaption.separator)↓ \(workingTokens(tokens)) tokens"
        }
        return "\(lead) (\(group))"
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
