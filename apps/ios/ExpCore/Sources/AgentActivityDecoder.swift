import Foundation

// EXP-850: the steer wire's `activity` payloads, decoded ONCE, here.
//
// The session view model used to read every field off the raw dictionary
// inside its socket handler, which meant the wire contract — new kinds, new
// optional fields, the tolerance rules for older publishers — was only ever
// exercised by a running app. This decoder is the whole of that reading,
// Foundation-only and pure, so ExpCoreTests drives every kind (including the
// EXP-850 additions) against the exact JSON the engine publishes.
//
// Contract: an unknown `kind`, a missing one, and an unusable payload all
// decode to `nil` and are SKIPPED by the caller — never fatal, because a newer
// desktop may publish events this build has no renderer for.

/// What one latest-wins frame does to its slot. Three outcomes, because the
/// wire has three: a readable payload SETS it, an explicit "nothing" CLEARS
/// it, and an unreadable frame must leave what the screen already shows alone.
public enum AgentSlotUpdate<Value: Equatable & Sendable>: Equatable, Sendable {
    /// Unreadable payload — keep the current value (never blank a slot on a
    /// malformed frame).
    case keep
    case clear
    case set(Value)

    /// Apply to a current value.
    public func applied(to current: Value?) -> Value? {
        switch self {
        case .keep: current
        case .clear: nil
        case let .set(value): value
        }
    }
}

/// A `question` event's fields — everything but the local feed id, which the
/// model stamps when it appends the card.
public struct AgentQuestionDraft: Equatable, Sendable {
    public let wireId: String
    public let askId: String?
    public let index: Int?
    public let total: Int?
    public let header: String?
    public let text: String
    public let options: [AgentQuestionOption]
    public let multiSelect: Bool
    public let planMode: Bool

    public init(
        wireId: String, askId: String? = nil, index: Int? = nil, total: Int? = nil,
        header: String? = nil, text: String, options: [AgentQuestionOption],
        multiSelect: Bool = false, planMode: Bool = false
    ) {
        self.wireId = wireId
        self.askId = askId
        self.index = index
        self.total = total
        self.header = header
        self.text = text
        self.options = options
        self.multiSelect = multiSelect
        self.planMode = planMode
    }

    public func question(id: Int) -> AgentQuestion {
        AgentQuestion(
            id: id, wireId: wireId, askId: askId, index: index, total: total,
            header: header, text: text, options: options,
            multiSelect: multiSelect, planMode: planMode
        )
    }
}

/// EXP-785/786/846: a `tool_update`'s payload — the settle, the per-call diff
/// and the Exponential-tool result preview, folded into the row with `id`.
public struct AgentToolUpdate: Equatable, Sendable {
    public let id: String
    /// `completed` / `failed` settle the call; anything else (a status-less
    /// diff-only update) leaves it open.
    public let status: String?
    public let diff: String?
    public let preview: AgentToolPreview?

    public init(
        id: String, status: String? = nil, diff: String? = nil,
        preview: AgentToolPreview? = nil
    ) {
        self.id = id
        self.status = status
        self.diff = diff
        self.preview = preview
    }

    public var settles: Bool { status == "completed" || status == "failed" }
    public var failed: Bool { status == "failed" }
}

/// EXP-724: a `compaction` edge, as read off the wire.
public struct AgentCompactionEdge: Equatable, Sendable {
    public let phase: String?
    public let trigger: String?
    /// The frame's own `at` stamp (ms), for the strip's backstop.
    public let at: Double?

    public init(phase: String? = nil, trigger: String? = nil, at: Double? = nil) {
        self.phase = phase
        self.trigger = trigger
        self.at = at
    }

    public var started: Bool { phase == "started" }
    public var ended: Bool { phase == "ended" }
}

/// EXP-848/850 §5: a `turn` edge. `state` is nil for a frame this build cannot
/// read — the slot then keeps whatever it had.
public struct AgentTurnEdge: Equatable, Sendable {
    public let state: AgentTurnState?
    public let startedAt: Int?
    public let tokens: Int?

    public init(state: AgentTurnState? = nil, startedAt: Int? = nil, tokens: Int? = nil) {
        self.state = state
        self.startedAt = startedAt
        self.tokens = tokens
    }
}

/// One decoded activity event.
public enum AgentActivityEvent: Equatable, Sendable {
    case narration(text: String, messageId: String?, subagentId: String?, beforeQuestionId: String?)
    case tool(
        name: String, detail: String?, subagentId: String?, callId: String?, toolKind: String?
    )
    case toolUpdate(AgentToolUpdate)
    /// The worktree diff behind the "Latest changes" chip; nil clears it.
    case diff(String?)
    case userMessage(text: String, subagentId: String?)
    case question(AgentQuestionDraft)
    case questionResolved(id: String?, askId: String?, answers: [String], dismissed: Bool)
    case answerAck(id: String)
    case subagent(
        id: String, agentType: String, status: AgentSubagentStatus, detail: String?,
        toolCalls: Int?, title: String?, workflowId: String?
    )
    case permission(tool: String, detail: String?)
    case configState(AgentSlotUpdate<AgentSessionConfig>)
    case usage(AgentSlotUpdate<AgentSessionUsage>)
    case rateLimit(AgentSlotUpdate<AgentSessionRateLimit>)
    case turn(AgentTurnEdge)
    case compaction(AgentCompactionEdge)
    /// EXP-850 §2: the FULL current list — an empty array closes the strip.
    case backgroundTasks([AgentBackgroundTask])
    /// EXP-861: the FULL current queue, latest-wins — an empty array clears
    /// the strip.
    case queue([QueuedMessage])
    /// EXP-850 §3: latest-wins per `id`.
    case workflow(AgentWorkflow)
}

public enum AgentActivityDecoder {
    /// The ONE entry point: a raw `activity` payload → a typed event, or nil
    /// for anything this build cannot use (an unknown kind included).
    public static func decode(_ event: [String: Any]?) -> AgentActivityEvent? {
        guard let event, let kind = event["kind"] as? String else { return nil }
        switch kind {
        case "narration":
            guard let text = event["text"] as? String, !blank(text) else { return nil }
            return .narration(
                text: text,
                messageId: string(event["messageId"]),
                subagentId: string(event["subagentId"]),
                beforeQuestionId: string(event["beforeQuestionId"])
            )
        case "tool":
            guard let name = event["name"] as? String else { return nil }
            return .tool(
                name: name,
                detail: string(event["detail"]),
                subagentId: string(event["subagentId"]),
                callId: string(event["id"]),
                toolKind: AgentFeed.toolKind(event["toolKind"])
            )
        case "tool_update":
            guard let update = toolUpdate(event) else { return nil }
            return .toolUpdate(update)
        case "diff":
            let diff = event["diff"] as? String
            return .diff((diff?.isEmpty == false) ? diff : nil)
        case "user_message":
            guard let text = event["text"] as? String, !blank(text) else { return nil }
            return .userMessage(text: text, subagentId: string(event["subagentId"]))
        case "question":
            guard let draft = question(event) else { return nil }
            return .question(draft)
        case "question_resolved":
            return .questionResolved(
                id: string(event["id"]),
                askId: string(event["askId"]),
                answers: (event["answers"] as? [String]) ?? [],
                dismissed: (event["dismissed"] as? Bool) ?? false
            )
        case "answer_ack":
            guard let id = string(event["id"]) else { return nil }
            return .answerAck(id: id)
        case "subagent":
            guard let id = string(event["id"]),
                  let raw = event["status"] as? String,
                  let status = AgentSubagentStatus(rawValue: raw) else { return nil }
            return .subagent(
                id: id,
                agentType: string(event["agentType"]) ?? AgentFeed.subagentFallbackType,
                status: status,
                detail: string(event["detail"]),
                toolCalls: int(event["toolCalls"]),
                title: string(event["title"]),
                // EXP-850 §4: set on every edge of a workflow's agent.
                workflowId: string(event["workflowId"])
            )
        case "permission":
            guard let tool = string(event["tool"]) else { return nil }
            return .permission(tool: tool, detail: string(event["detail"]))
        case "config_state":
            return .configState(configState(event))
        case "usage":
            return .usage(usage(event))
        case "rate_limit":
            return .rateLimit(rateLimit(event))
        case "turn":
            return .turn(turn(event))
        case "compaction":
            return .compaction(compaction(event))
        case "background_tasks":
            return .backgroundTasks(backgroundTasks(event))
        case "queue":
            return .queue(queue(event))
        case "workflow":
            guard let workflow = workflow(event) else { return nil }
            return .workflow(workflow)
        default:
            // A newer publisher's kind — skipped, never fatal.
            return nil
        }
    }

    // MARK: - Per-kind readers (the AgentFeed folds share these)

    static func question(_ event: [String: Any]) -> AgentQuestionDraft? {
        // The wire id is required (EXP-613): it addresses the `answer` frame
        // and every resolution event, so an id-less card would be unanswerable
        // and never retire. No publisher emits one.
        guard let wireId = string(event["id"]),
              let text = event["text"] as? String, !text.isEmpty,
              let rawOptions = event["options"] as? [[String: Any]] else { return nil }
        let options: [AgentQuestionOption] = rawOptions.compactMap { option in
            guard let label = option["label"] as? String, let key = option["key"] as? String,
                  !key.isEmpty else { return nil }
            return AgentQuestionOption(
                label: label, key: key, description: string(option["description"]),
                freeText: option["freeText"] as? Bool ?? false
            )
        }
        guard !options.isEmpty else { return nil }
        return AgentQuestionDraft(
            wireId: wireId,
            askId: string(event["askId"]),
            index: positiveInt(event["index"]),
            total: positiveInt(event["total"]),
            header: string(event["header"]),
            text: text,
            options: options,
            multiSelect: (event["multiSelect"] as? Bool) ?? false,
            planMode: (event["planMode"] as? Bool) ?? false
        )
    }

    static func toolUpdate(_ event: [String: Any]) -> AgentToolUpdate? {
        guard let id = string(event["id"]) else { return nil }
        return AgentToolUpdate(
            id: id,
            status: event["status"] as? String,
            diff: string(event["diff"]),
            preview: AgentFeed.toolPreview(event["preview"])
        )
    }

    static func configState(_ event: [String: Any]) -> AgentSlotUpdate<AgentSessionConfig> {
        // `options` is required on the wire (possibly empty) — its absence
        // means this is not a config_state we can read.
        guard let rawOptions = event["options"] as? [[String: Any]] else { return .keep }
        let options: [AgentConfigOption] = rawOptions.compactMap { raw in
            guard let id = string(raw["id"]) else { return nil }
            return AgentConfigOption(
                id: id,
                label: string(raw["label"]) ?? id,
                category: string(raw["category"]),
                value: raw["value"] as? String,
                values: configValues(raw["values"])
            )
        }
        let modes: [AgentConfigMode] = (event["modes"] as? [[String: Any]] ?? []).compactMap { raw in
            guard let id = string(raw["id"]) else { return nil }
            return AgentConfigMode(
                id: id,
                label: string(raw["label"]) ?? id,
                description: string(raw["description"])
            )
        }
        let commands: [AgentConfigCommand] = (event["commands"] as? [[String: Any]] ?? [])
            .compactMap { raw in
                guard let name = string(raw["name"]) else { return nil }
                return AgentConfigCommand(
                    name: name,
                    description: (raw["description"] as? String) ?? "",
                    hint: string(raw["hint"])
                )
            }
        return .set(AgentSessionConfig(
            options: options,
            currentMode: string(event["currentMode"]),
            modes: modes,
            commands: commands
        ))
    }

    static func usage(_ event: [String: Any]) -> AgentSlotUpdate<AgentSessionUsage> {
        guard let used = int(event["contextUsed"]), let size = int(event["contextSize"])
        else { return .keep }
        // A run whose engine reports a zero context size knows nothing worth
        // drawing.
        guard size > 0 else { return .clear }
        let cost = (event["costUsd"] as? NSNumber)?.doubleValue
        return .set(AgentSessionUsage(
            contextUsed: max(0, used),
            contextSize: size,
            costUsd: (cost ?? -1) >= 0 ? cost : nil
        ))
    }

    static func rateLimit(_ event: [String: Any]) -> AgentSlotUpdate<AgentSessionRateLimit> {
        // An unreadable payload CLEARS here (unlike every other slot): a stale
        // "rate limited" banner beside a live run is the worse error.
        guard let status = event["status"] as? String, !AgentFeed.rateLimitClears(status)
        else { return .clear }
        let resetsAt = int(event["resetsAt"])
        return .set(AgentSessionRateLimit(
            status: status.trimmingCharacters(in: .whitespacesAndNewlines),
            resetsAt: (resetsAt ?? -1) >= 0 ? resetsAt : nil,
            message: string(event["message"])?.trimmingCharacters(in: .whitespacesAndNewlines)
        ))
    }

    static func turn(_ event: [String: Any]) -> AgentTurnEdge {
        AgentTurnEdge(
            state: (event["state"] as? String).flatMap { AgentTurnState(rawValue: $0) },
            startedAt: int(event["startedAt"]).flatMap { $0 > 0 ? $0 : nil },
            tokens: int(event["tokens"]).flatMap { $0 >= 0 ? $0 : nil }
        )
    }

    static func compaction(_ event: [String: Any]) -> AgentCompactionEdge {
        AgentCompactionEdge(
            phase: event["phase"] as? String,
            trigger: string(event["trigger"]),
            at: (event["at"] as? NSNumber)?.doubleValue
        )
    }

    static func backgroundTasks(_ event: [String: Any]) -> [AgentBackgroundTask] {
        guard let rows = event["tasks"] as? [[String: Any]] else { return [] }
        return rows.compactMap { row in
            guard let id = string(row["id"]),
                  let description = string(row["description"]) else { return nil }
            return AgentBackgroundTask(
                id: id,
                kind: AgentBackgroundTask.kind(row["kind"]),
                description: description,
                toolId: string(row["toolId"])
            )
        }
    }

    /// EXP-861: the whole queue, oldest first. A row without an id or a text
    /// cannot be revoked or drawn, so it is skipped; a frame without a
    /// readable `messages` array reads as empty (the device's "nothing
    /// queued"), never as "keep".
    static func queue(_ event: [String: Any]) -> [QueuedMessage] {
        guard let rows = event["messages"] as? [[String: Any]] else { return [] }
        return rows.compactMap { row in
            guard let id = string(row["id"]), let text = row["text"] as? String else { return nil }
            return QueuedMessage(id: id, text: text)
        }
    }

    static func workflow(_ event: [String: Any]) -> AgentWorkflow? {
        guard let id = string(event["id"]), let name = string(event["name"]) else { return nil }
        let phases: [AgentWorkflowPhase] = (event["phases"] as? [[String: Any]] ?? [])
            .compactMap { raw in
                guard let index = int(raw["index"]) else { return nil }
                return AgentWorkflowPhase(index: index, title: string(raw["title"]) ?? "")
            }
        let agents: [AgentWorkflowAgent] = (event["agents"] as? [[String: Any]] ?? [])
            .compactMap { raw in
                guard let index = int(raw["index"]) else { return nil }
                return AgentWorkflowAgent(
                    index: index,
                    label: string(raw["label"]) ?? "Agent \(index)",
                    phaseIndex: int(raw["phaseIndex"]),
                    agentId: string(raw["agentId"]),
                    model: string(raw["model"]),
                    state: AgentWorkflowAgentState.parse(raw["state"]),
                    tokens: int(raw["tokens"]),
                    toolCalls: int(raw["toolCalls"]),
                    durationMs: int(raw["durationMs"]),
                    lastTool: string(raw["lastTool"]),
                    lastToolSummary: string(raw["lastToolSummary"]),
                    resultPreview: string(raw["resultPreview"]),
                    error: string(raw["error"])
                )
            }
        return AgentWorkflow(
            id: id,
            name: name,
            description: string(event["description"]),
            status: AgentWorkflowStatus.parse(event["status"]),
            phases: phases,
            agents: agents,
            summary: string(event["summary"])
        )
    }

    // MARK: - Field readers

    private static func configValues(_ raw: Any?) -> [AgentConfigValue] {
        guard let rows = raw as? [[String: Any]] else { return [] }
        return rows.compactMap { row in
            guard let id = string(row["id"]) else { return nil }
            return AgentConfigValue(id: id, label: string(row["label"]) ?? id)
        }
    }

    /// A wire string field, nil unless it carries something.
    private static func string(_ value: Any?) -> String? {
        guard let text = value as? String, !blank(text) else { return nil }
        return text
    }

    private static func blank(_ text: String) -> Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// JSON numbers arrive as `NSNumber` through JSONSerialization; a
    /// stringified one (some publishers' jsonb round-trips) is read too.
    private static func int(_ value: Any?) -> Int? {
        if let number = value as? NSNumber { return number.intValue }
        if let text = value as? String { return Int(text) }
        return nil
    }

    private static func positiveInt(_ value: Any?) -> Int? {
        guard let number = int(value), number >= 1 else { return nil }
        return number
    }
}
