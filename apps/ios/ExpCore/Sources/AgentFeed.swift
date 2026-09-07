import Foundation

// Steer protocol v2 (EXP-249) — the pure core of the "Agent session" activity
// feed: the wire's question / subagent / permission shapes, the render grouping
// over the flat feed, and the per-card answer lock. Foundation only (ExpCore
// rule) and free of SwiftUI, so ExpCoreTests can drive all of it. The view
// model (Exponential/UI/Session/AgentSessionModel.swift) owns only the socket.

/// One answer choice of a `question` activity event. `key` is what a steering
/// client submits — it rides inside the semantic `answer` frame and the desktop
/// maps it onto its own picker.
public struct AgentQuestionOption: Equatable, Sendable {
    public let label: String
    public let key: String
    /// Per-option help text (protocol v2, optional on the wire).
    public let description: String?
    /// EXP-513: claude's synthetic free-text row ("Type something.") —
    /// selecting it reveals an inline input and the typed reply rides the
    /// answer frame's `text`. Absent from older desktops.
    public let freeText: Bool

    public init(label: String, key: String, description: String? = nil, freeText: Bool = false) {
        self.label = label
        self.key = key
        self.description = description
        self.freeText = freeText
    }
}

/// One interactive question card — an AskUserQuestion step or a plan approval.
public struct AgentQuestion: Equatable, Sendable, Identifiable {
    /// Local monotonic feed id — the render identity. A re-emission of the same
    /// wire id keeps the id the card already had (see `AgentFeed.upsertQuestion`).
    public var id: Int
    /// Protocol-v2 stable question id — what every `answer`, `answer_ack` and
    /// `question_resolved` frame addresses. Required: a card with no id could
    /// not be answered, and no publisher emits one (EXP-613).
    public let wireId: String
    /// Groups the steps of ONE multi-question ask into a single stepper card.
    public let askId: String?
    /// 1-based step position; absent on the ask's final review/submit step.
    public let index: Int?
    public let total: Int?
    public let header: String?
    public let text: String
    public let options: [AgentQuestionOption]
    public let multiSelect: Bool
    /// An ExitPlanMode plan approval — `text` is the full plan markdown.
    public let planMode: Bool
    public var resolved: Bool
    public var answers: [String]
    public var dismissed: Bool

    public init(
        id: Int,
        wireId: String,
        askId: String? = nil,
        index: Int? = nil,
        total: Int? = nil,
        header: String? = nil,
        text: String,
        options: [AgentQuestionOption],
        multiSelect: Bool = false,
        planMode: Bool = false,
        resolved: Bool = false,
        answers: [String] = [],
        dismissed: Bool = false
    ) {
        self.id = id
        self.wireId = wireId
        self.askId = askId
        self.index = index
        self.total = total
        self.header = header
        self.text = text
        self.options = options
        self.multiSelect = multiSelect
        self.planMode = planMode
        self.resolved = resolved
        self.answers = answers
        self.dismissed = dismissed
    }

    /// The ask's final review/submit step: it belongs to an ask but carries no
    /// step position of its own.
    public var isSubmitStep: Bool { askId != nil && index == nil }

    /// Key of the per-card answer lock.
    public var lockKey: String { wireId }

    /// The chosen answer(s) once resolved, for display.
    public var answerSummary: String? {
        answers.isEmpty ? nil : answers.joined(separator: ", ")
    }
}

/// Lifecycle of a `subagent` activity event (protocol v2).
public enum AgentSubagentStatus: String, Sendable {
    case started
    case completed
}

/// EXP-724: a context compaction is in flight on the host agent. Opened by a
/// `compaction` activity event with `phase: "started"`, closed by `"ended"`.
/// The viewer draws an indeterminate strip while this is non-nil — there is no
/// progress to report, only "the agent is busy folding its context away", which
/// is why the wire carries no percentage.
public struct AgentCompaction: Equatable, Sendable {
    /// `manual` (a `/compact` someone ran) or `auto` (the agent hit its
    /// context ceiling). Absent on publishers that don't know which.
    public let trigger: String?

    public init(trigger: String? = nil) {
        self.trigger = trigger
    }
}

// MARK: - Live agent configuration (EXP-746)

/// One selectable value of an `AgentConfigOption` (ACP
/// `SessionConfigOption.values`).
public struct AgentConfigValue: Equatable, Sendable, Identifiable {
    public let id: String
    public let label: String

    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }
}

/// One live agent option — model, effort, thinking level, whatever the adapter
/// advertised. `values` ABSENT (empty here) means read-only on this run: draw
/// the value, offer no menu.
public struct AgentConfigOption: Equatable, Sendable, Identifiable {
    public let id: String
    public let label: String
    /// Grouping hint (`model`, `effort`, …); a client that doesn't know it
    /// still draws one chip per option.
    public let category: String?
    /// In force right now; blank/absent = the CLI's own default.
    public let value: String?
    public let values: [AgentConfigValue]

    public init(
        id: String, label: String, category: String? = nil,
        value: String? = nil, values: [AgentConfigValue] = []
    ) {
        self.id = id
        self.label = label
        self.category = category
        self.value = value
        self.values = values
    }
}

/// One permission/collaboration mode the agent offers (`set_mode` switches).
public struct AgentConfigMode: Equatable, Sendable, Identifiable {
    public let id: String
    public let label: String
    public let description: String?

    public init(id: String, label: String, description: String? = nil) {
        self.id = id
        self.label = label
        self.description = description
    }
}

/// One slash command the AGENT itself advertises (ACP
/// `available_commands_update`) — the `/` menu shows the contract catalog
/// UNION these, contract first.
public struct AgentConfigCommand: Equatable, Sendable {
    public let name: String
    public let description: String
    public let hint: String?

    public init(name: String, description: String, hint: String? = nil) {
        self.name = name
        self.description = description
        self.hint = hint
    }
}

/// EXP-746: the agent's live configuration behind the composer chips.
/// Latest-wins state beside the feed, never a row (the `AgentCompaction`
/// shape) — the relay replays the newest `config_state` after its log, so a
/// join and a reconnect both repaint from one frame.
public struct AgentSessionConfig: Equatable, Sendable {
    public let options: [AgentConfigOption]
    public let currentMode: String?
    public let modes: [AgentConfigMode]
    public let commands: [AgentConfigCommand]

    public init(
        options: [AgentConfigOption] = [],
        currentMode: String? = nil,
        modes: [AgentConfigMode] = [],
        commands: [AgentConfigCommand] = []
    ) {
        self.options = options
        self.currentMode = currentMode
        self.modes = modes
        self.commands = commands
    }
}

/// EXP-746: the run's context window and spend as the engine last measured it.
/// Deliberately a TOKEN count, not a percent: the device-reported rate-limit
/// windows already own the 0-100 vocabulary and this is a different quantity.
public struct AgentSessionUsage: Equatable, Sendable {
    public let contextUsed: Int
    public let contextSize: Int
    public let costUsd: Double?

    public init(contextUsed: Int, contextSize: Int, costUsd: Double? = nil) {
        self.contextUsed = contextUsed
        self.contextSize = contextSize
        self.costUsd = costUsd
    }

    /// Clamped 0-100; nil when the size is unknown.
    public var percent: Int? {
        guard contextSize > 0 else { return nil }
        let share = Double(contextUsed) * 100 / Double(contextSize)
        return min(100, max(0, Int(share.rounded(.down))))
    }
}

/// One composer chip: the mode chip (when the agent offers modes) followed by
/// the advertised options, in publisher order.
public struct AgentConfigChip: Equatable, Sendable, Identifiable {
    public enum Kind: Sendable, Equatable {
        case mode
        case option
    }

    public let kind: Kind
    /// What a pick addresses: the OPTION id (`set_config`), or the `mode`
    /// sentinel for the mode chip (whose picks carry the MODE id to
    /// `set_mode`).
    public let id: String
    public let label: String
    public let valueLabel: String
    public let values: [AgentConfigValue]

    public init(
        kind: Kind, id: String, label: String, valueLabel: String,
        values: [AgentConfigValue] = []
    ) {
        self.kind = kind
        self.id = id
        self.label = label
        self.valueLabel = valueLabel
        self.values = values
    }

    /// A chip with nothing to pick from is a read-only badge.
    public var isReadOnly: Bool { values.isEmpty }
}

/// One rendered feed entry. Diffs never enter the feed — the latest one lives
/// behind the pinned "Latest changes" chip.
public enum AgentFeedItem: Equatable, Sendable, Identifiable {
    case narration(id: Int, text: String)
    /// `subagentId` (protocol v2) tags the tool as a subagent's work — such
    /// runs collapse under their subagent row.
    case tool(id: Int, name: String, detail: String?, subagentId: String?)
    /// A human turn: the initial prompt or a steered message.
    case userMessage(id: Int, text: String)
    case question(AgentQuestion)
    /// A subagent started or finished (protocol v2).
    ///
    /// EXP-748: `toolCalls` is the publisher's own count of this subagent's
    /// tool calls, stamped on the completed edge. The replay log evicts
    /// subagent tool events first, so the visible rows can undercount — the
    /// run renders `max(visible rows, toolCalls)`.
    case subagent(
        id: Int, subagentId: String, agentType: String,
        status: AgentSubagentStatus, detail: String?, toolCalls: Int? = nil
    )
    /// A permission prompt the agent hit (protocol v2) — INFORMATIONAL: the
    /// desktop's own TUI owns the approval, there is nothing to answer here.
    case permission(id: Int, tool: String, detail: String?)
    /// EXP-724: the quiet marker a finished compaction leaves behind, so the
    /// gap in the conversation above it is explained forever after the strip
    /// is gone. Carries no text — every client renders `compactedLabel`.
    case compaction(id: Int)

    public var id: Int {
        switch self {
        case let .narration(id, _): id
        case let .tool(id, _, _, _): id
        case let .userMessage(id, _): id
        case let .question(value): value.id
        case let .subagent(id, _, _, _, _, _): id
        case let .permission(id, _, _): id
        case let .compaction(id): id
        }
    }

    public var isTool: Bool {
        if case .tool = self { return true }
        return false
    }

    public var isQuestion: Bool {
        if case .question = self { return true }
        return false
    }

    public var question: AgentQuestion? {
        if case let .question(value) = self { return value }
        return nil
    }

    /// The subagent this item belongs to, if any — the grouping key of a
    /// subagent run.
    public var subagentKey: String? {
        switch self {
        case let .tool(_, _, _, subagentId): return subagentId
        case let .subagent(_, subagentId, _, _, _, _): return subagentId
        default: return nil
        }
    }
}

/// A subagent's run: its lifecycle markers plus every tool call published
/// under it, collapsed into one expandable render row.
public struct AgentSubagentRun: Equatable, Sendable, Identifiable {
    /// Lowest feed id in the group — the row key stays put while the run grows.
    public let anchorId: Int
    public let subagentId: String
    public let agentType: String
    public let detail: String?
    /// A `completed` marker arrived.
    public let done: Bool
    /// The tool calls published under this subagent.
    public let items: [AgentFeedItem]
    /// EXP-748: the highest count the subagent's own markers reported, if any.
    /// Replay evicts subagent tool events first, so `items` can undercount.
    public let reportedToolCalls: Int?

    public var id: Int { anchorId }
    /// The reported count wins whenever it is higher than what is visible.
    public var toolCount: Int { max(items.count, reportedToolCalls ?? 0) }
    /// Whether the row has anything behind its chevron — the detail is always
    /// visible collapsed, so only tool calls justify an expand affordance
    /// (EXP-350: a chevron on an empty group expanded to nothing).
    public var expandable: Bool { !items.isEmpty }

    public init(
        anchorId: Int,
        subagentId: String,
        agentType: String,
        detail: String?,
        done: Bool,
        items: [AgentFeedItem],
        reportedToolCalls: Int? = nil
    ) {
        self.anchorId = anchorId
        self.subagentId = subagentId
        self.agentType = agentType
        self.detail = detail
        self.done = done
        self.items = items
        self.reportedToolCalls = reportedToolCalls
    }
}

/// The steps of one multi-question ask (protocol v2), rendered as ONE stepper
/// card: the client walks the steps in order and the desktop's `answer_ack`
/// advances it.
public struct AgentAskGroup: Equatable, Sendable, Identifiable {
    public let askId: String
    /// Ordered steps: `index`-carrying questions first, the review/submit step
    /// last.
    public let questions: [AgentQuestion]

    /// NOT `questions.first`: a late-arriving low-index step re-sorts to the
    /// head, and the row key must not move with it.
    public var id: Int { questions.map(\.id).min() ?? -1 }
    /// Steps excluding the ask's final review/submit step.
    public var stepCount: Int { questions.filter { !$0.isSubmitStep }.count }

    public init(askId: String, questions: [AgentQuestion]) {
        self.askId = askId
        self.questions = questions
    }
}

/// One render row over the flat feed: a single item, a run of ≥2 CONSECUTIVE
/// tool calls (EXP-97), a subagent's run, or a multi-question ask.
public enum AgentFeedRow: Equatable, Sendable, Identifiable {
    case single(AgentFeedItem)
    case toolRun([AgentFeedItem])
    case subagentRun(AgentSubagentRun)
    case ask(AgentAskGroup)

    public var id: Int {
        switch self {
        case let .single(item): item.id
        case let .toolRun(items): items.first?.id ?? -1
        case let .subagentRun(run): run.id
        case let .ask(group): group.id
        }
    }
}

/// Per-card answer lock (protocol v2): a tap locks its card IMMEDIATELY so a
/// double tap can never send twice. The desktop's `answer_ack` makes the lock
/// permanent (and advances a stepper); nothing at all coming back expires the
/// optimistic lock — the card re-surfaces flagged `failed` so the steerer sees
/// WHY it rolled back and can pick again (web parity, EXP-334).
public struct AgentAnswerTracker: Equatable, Sendable {
    /// Optimistically locked cards → when their answer frame went out.
    public private(set) var pending: [String: Date] = [:]
    /// Cards the desktop confirmed injecting (`answer_ack`).
    public private(set) var acked: Set<String> = []
    /// Cards whose optimistic lock expired with no confirmation — answerable
    /// again, rendered with a retry hint (EXP-334).
    public private(set) var failed: Set<String> = []
    /// What was picked, per card, in the steerer's own words — the option
    /// labels (and a typed free-text reply) of the answer that went out. The
    /// desktop only fills a question's `answers` on `question_resolved`, which
    /// for a multi-question ask lands after the WHOLE ask submits, so the
    /// stepper's answered steps would otherwise read "Answered" ×N until then
    /// (EXP-588, web parity: `AnswerState.labels`). Cleared on expiry — a
    /// rolled-back step has no answer to show.
    public private(set) var labels: [String: [String]] = [:]

    public init() {}

    public mutating func markSent(_ key: String, labels: [String] = [], at: Date = Date()) {
        pending[key] = at
        failed.remove(key)
        self.labels[key] = labels.isEmpty ? nil : labels
    }

    public mutating func acknowledge(_ key: String) {
        pending[key] = nil
        failed.remove(key)
        acked.insert(key)
    }

    /// A card the desktop retired (`question_resolved`) — the resolved card
    /// renders its answer and is never answerable again, so the optimistic
    /// lock has nothing left to guard.
    public mutating func resolve(_ key: String) {
        pending[key] = nil
        failed.remove(key)
    }

    /// Expire ONE card's still-unconfirmed lock — the per-card timer's target.
    /// A shared timeout sweep used to drop EVERY pending lock at once, rolling
    /// a stepper back past steps that were answered moments ago (EXP-334).
    /// An acked card stays locked.
    public mutating func expire(_ key: String) {
        guard pending[key] != nil else { return }
        pending[key] = nil
        failed.insert(key)
        labels[key] = nil
    }

    /// Drop optimistic locks older than `timeout`. Acked cards stay locked.
    /// Returns whether anything was dropped.
    @discardableResult
    public mutating func expire(now: Date = Date(), timeout: TimeInterval) -> Bool {
        let stale = pending.filter { now.timeIntervalSince($0.value) >= timeout }.map(\.key)
        for key in stale {
            pending[key] = nil
            failed.insert(key)
            labels[key] = nil
        }
        return !stale.isEmpty
    }

    public mutating func reset() {
        pending = [:]
        acked = []
        failed = []
        labels = [:]
    }

    /// The locally picked labels of a locked card, joined for display; nil
    /// when nothing went out (or the lock rolled back).
    public func answerSummary(_ key: String) -> String? {
        guard isLocked(key), let picked = labels[key], !picked.isEmpty else { return nil }
        return picked.joined(separator: ", ")
    }

    public func isLocked(_ key: String) -> Bool { acked.contains(key) || pending[key] != nil }
    public func isPending(_ key: String) -> Bool { pending[key] != nil }
    public func isAcked(_ key: String) -> Bool { acked.contains(key) }
    public func isFailed(_ key: String) -> Bool { failed.contains(key) }

    /// Every locked key — sent-and-unconfirmed plus acknowledged. What a
    /// stepper advances on (web parity: a step counts as answered the moment
    /// its answer goes out; an expired optimistic lock re-surfaces the step).
    public var lockedKeys: Set<String> { acked.union(pending.keys) }
}

/// The feed's pure logic: which cards are still answerable, how wire events
/// fold into the feed, and how the flat feed projects into render rows.
public enum AgentFeed {
    /// Client-side feed cap — old events fall off the top. Matches the relay's
    /// ACTIVITY_LOG_CAP so a full replay never truncates.
    public static let feedCap = 2000
    /// `subagent.agentType` when the desktop's hook payload carried none — old
    /// desktop builds also stamp it onto the COMPLETED edge, so it is a
    /// sentinel the label selection skips past, never a type to prefer
    /// (EXP-350).
    public static let subagentFallbackType = "agent"
    /// EXP-724, byte-identical ×4 (web `agent-feed.ts`, Android `AgentFeed.kt`,
    /// desktop `feed.rs`): the indeterminate strip's label while a compaction
    /// runs. The ellipsis is ONE character (U+2026), not three dots.
    public static let compactingLabel = "Compacting context…"
    /// The persistent marker row a finished compaction appends.
    public static let compactedLabel = "Context compacted"
    /// Backstop for a `started` whose `ended` never arrives (a publisher that
    /// died mid-compaction, a dropped frame): the strip clears itself after
    /// this long rather than sticking forever. Same 180s ×4.
    public static let compactionTimeoutSeconds: TimeInterval = 180

    // MARK: - Quiet live runs (FEED-26)

    /// FEED-26: how long a LIVE run's feed may stay unchanged before the
    /// header stops reading as a healthy "Live". Byte-identical ×4 (web
    /// `stale-activity.ts` STALE_ACTIVITY_AFTER_MS, Android
    /// `AgentSessionScreen.kt`, desktop `steer_viewer.rs`).
    public static let staleActivityAfter: TimeInterval = 10 * 60

    /// Whole minutes the feed has been quiet, or nil while inside the
    /// threshold (and with no known last activity).
    ///
    /// `since` is when the feed last CHANGED while live, falling back to the
    /// moment the phase went live — never an event's own `at`, which is
    /// optional on the wire, so the clock is the viewer's own. The caller
    /// gates the states that already explain the silence: a paused host, a
    /// trailing question or plan card, and a running compaction.
    public static func staleActivityMinutes(since: Date?, now: Date) -> Int? {
        guard let since else { return nil }
        let silent = now.timeIntervalSince(since)
        guard silent >= staleActivityAfter else { return nil }
        return Int(silent / 60)
    }

    /// The caption: `No activity for 27 min` / `No activity for 27 min ·
    /// macbook` — the same ` · <device>` suffix "Live" and "Needs your input"
    /// carry.
    public static func staleActivityLabel(minutes: Int, deviceLabel: String?) -> String {
        let head = "No activity for \(minutes) min"
        guard let deviceLabel, !deviceLabel.isEmpty else { return head }
        return "\(head) · \(deviceLabel)"
    }

    /// Fold one `compaction` activity event into the current state: `started`
    /// opens a fresh window (a re-emitted `started` just re-stamps the
    /// trigger), `ended` closes it, and anything else — an unknown phase, a
    /// missing one — leaves it exactly as it was. A publisher that only ever
    /// emits `ended` (codex has no start marker for auto-compaction) is
    /// handled by the caller, which appends the marker row regardless.
    public static func applyCompaction(
        _ current: AgentCompaction?, event: [String: Any]
    ) -> AgentCompaction? {
        switch event["phase"] as? String {
        case "started":
            let trigger = (event["trigger"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            return AgentCompaction(trigger: trigger)
        case "ended":
            return nil
        default:
            return current
        }
    }

    // MARK: - Live agent configuration (EXP-746)

    /// The value label a chip shows when the option carries no value at all —
    /// the CLI's own default, which is a real choice, not an absence.
    /// Byte-identical ×4 (web `steer-commands.ts` `CONFIG_DEFAULT_VALUE_LABEL`,
    /// Android `AgentFeed.kt`, desktop `ui/src/slash_commands.rs`).
    public static let configDefaultValueLabel = "CLI default"
    /// The mode chip's leading label. Every OTHER chip label arrives live on
    /// `config_state.options[].label`, so only this one has to be mirrored.
    public static let configModeLabel = "Mode"

    /// Fold a `config_state` activity event. Nil = an unusable payload, and
    /// the caller then KEEPS `current`: a malformed frame must never blank the
    /// chips (the `applyCompaction` contract, one level stricter because this
    /// carries arrays).
    public static func applyConfigState(
        _ current: AgentSessionConfig?, event: [String: Any]
    ) -> AgentSessionConfig? {
        // `options` is required on the wire (possibly empty) — its absence
        // means this is not a config_state we can read.
        guard let rawOptions = event["options"] as? [[String: Any]] else { return current }
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
        return AgentSessionConfig(
            options: options,
            currentMode: string(event["currentMode"]),
            modes: modes,
            commands: commands
        )
    }

    /// Fold a `usage` activity event. Nil CLEARS the slot — a run whose engine
    /// reports a zero context size knows nothing worth drawing — while an
    /// unreadable payload keeps `current` standing.
    public static func applyUsage(
        _ current: AgentSessionUsage?, event: [String: Any]
    ) -> AgentSessionUsage? {
        guard let used = (event["contextUsed"] as? NSNumber)?.intValue,
              let size = (event["contextSize"] as? NSNumber)?.intValue
        else { return current }
        guard size > 0 else { return nil }
        let cost = (event["costUsd"] as? NSNumber)?.doubleValue
        return AgentSessionUsage(
            contextUsed: max(0, used),
            contextSize: size,
            costUsd: (cost ?? -1) >= 0 ? cost : nil
        )
    }

    /// The composer's chips: the mode chip FIRST (only when the agent offers
    /// modes), then every advertised option in publisher order. Locked ×4 by
    /// the test `configChips puts the mode chip first`.
    public static func configChips(_ config: AgentSessionConfig?) -> [AgentConfigChip] {
        guard let config else { return [] }
        var chips: [AgentConfigChip] = []
        if !config.modes.isEmpty {
            let current = config.modes.first { $0.id == config.currentMode }
            chips.append(AgentConfigChip(
                kind: .mode,
                id: "mode",
                label: configModeLabel,
                valueLabel: current?.label ?? config.currentMode ?? configDefaultValueLabel,
                values: config.modes.map { AgentConfigValue(id: $0.id, label: $0.label) }
            ))
        }
        for option in config.options {
            let picked = option.values.first { $0.id == option.value }
            let value = option.value ?? ""
            chips.append(AgentConfigChip(
                kind: .option,
                id: option.id,
                label: option.label,
                valueLabel: picked?.label ?? (value.isEmpty ? configDefaultValueLabel : value),
                values: option.values
            ))
        }
        return chips
    }

    private static func configValues(_ raw: Any?) -> [AgentConfigValue] {
        guard let rows = raw as? [[String: Any]] else { return [] }
        return rows.compactMap { row in
            guard let id = string(row["id"]) else { return nil }
            return AgentConfigValue(id: id, label: string(row["label"]) ?? id)
        }
    }

    /// A wire string field, nil unless it carries something (the model's
    /// `trimmedField`, lifted here so the folds are testable without a view).
    private static func string(_ value: Any?) -> String? {
        guard let text = value as? String,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return nil }
        return text
    }

    /// Ids of the question items still answerable: every card the desktop has
    /// not retired with `question_resolved` (EXP-249). No screen-scraping
    /// heuristics — an unresolved card stays active however far back it is.
    /// Mirrors the Android `activeQuestionIds`.
    public static func activeQuestionIds(_ feed: [AgentFeedItem]) -> Set<Int> {
        var ids = Set<Int>()
        for item in feed {
            guard let question = item.question, !question.resolved else { continue }
            ids.insert(question.id)
        }
        return ids
    }

    /// Apply a `question_resolved` event: retire the card with `id`, else every
    /// card of `askId`, else (neither given) every card still unresolved.
    /// Answers land positionally on the answer-CONSUMING cards (an ask's submit
    /// step consumes none); a by-id resolution folds all of them into that one
    /// card, and a dismissal carries none at all. nil when nothing matched.
    public static func applyQuestionResolved(
        _ feed: [AgentFeedItem],
        id: String?,
        askId: String?,
        answers: [String] = [],
        dismissed: Bool = false
    ) -> [AgentFeedItem]? {
        var out = feed
        var matched = false
        var cursor = 0
        for i in out.indices {
            guard let question = out[i].question else { continue }
            let hit: Bool
            if let id {
                hit = question.wireId == id
            } else if let askId {
                hit = question.askId == askId
            } else {
                hit = !question.resolved
            }
            guard hit else { continue }
            matched = true
            var updated = question
            updated.resolved = true
            if dismissed { updated.dismissed = true }
            if !dismissed, !question.isSubmitStep {
                if id != nil {
                    if !answers.isEmpty { updated.answers = answers }
                } else if cursor < answers.count {
                    updated.answers = [answers[cursor]]
                    cursor += 1
                }
            }
            out[i] = .question(updated)
        }
        return matched ? out : nil
    }

    /// Append a question, or REPLACE the card that already carries its wire id
    /// (protocol v2 re-emits a question when its options are augmented — e.g. a
    /// synthetic "Type something" the desktop discovers in the TUI grid later).
    /// The replacement keeps the card's render id and whatever resolution it
    /// already had.
    public static func upsertQuestion(
        _ feed: [AgentFeedItem], question: AgentQuestion
    ) -> [AgentFeedItem] {
        guard let index = feed.firstIndex(where: { $0.question?.wireId == question.wireId }),
              let existing = feed[index].question
        else { return feed + [.question(question)] }
        var merged = question
        merged.id = existing.id
        merged.resolved = existing.resolved
        merged.answers = existing.answers
        merged.dismissed = existing.dismissed
        var out = feed
        out[index] = .question(merged)
        return out
    }

    /// Insert `item` immediately BEFORE the first question card matching
    /// `anchor` (its ask id or wire id) — EXP-483: claude withholds the
    /// transcript entry carrying an ask/plan tool_use, prose included, until
    /// the picker resolves, so that prose arrives AFTER the already-published
    /// card and tags itself with `beforeQuestionId` to be spliced back above
    /// it. Matches resolved cards too (the twin normally flushes post-answer).
    /// nil when no card matches (evicted) — the caller
    /// appends.
    public static func spliceBeforeQuestion(
        _ feed: [AgentFeedItem], anchor: String, item: AgentFeedItem
    ) -> [AgentFeedItem]? {
        guard let index = feed.firstIndex(where: { entry in
            guard let question = entry.question else { return false }
            return question.askId == anchor || question.wireId == anchor
        }) else { return nil }
        var out = feed
        out.insert(item, at: index)
        return out
    }

    /// Render rows over the flat feed — a projection only, the feed stays the
    /// state (and `activeQuestionIds` keeps operating on it): every card of one
    /// ask collapses into a stepper row, a subagent's markers and calls into
    /// its own group, and runs of ≥2 consecutive plain tool calls into a "N
    /// tool calls" row (EXP-97). Grouped items are pulled OUT of their in-place
    /// position into the row their group opened, so a late-arriving step (or a
    /// subagent call that lands behind an unrelated one) still joins its group.
    public static func rows(_ feed: [AgentFeedItem]) -> [AgentFeedRow] {
        var builders: [RowBuilder] = []
        var askAt: [String: Int] = [:]
        var subagentAt: [String: Int] = [:]
        var i = 0
        while i < feed.count {
            let item = feed[i]

            if let askId = item.question?.askId {
                if let at = askAt[askId] {
                    builders[at].items.append(item)
                } else {
                    askAt[askId] = builders.count
                    builders.append(RowBuilder(kind: .ask(askId), items: [item]))
                }
                i += 1
                continue
            }

            if let subagentId = item.subagentKey {
                if let at = subagentAt[subagentId] {
                    builders[at].items.append(item)
                } else {
                    subagentAt[subagentId] = builders.count
                    builders.append(RowBuilder(kind: .subagent(subagentId), items: [item]))
                }
                i += 1
                continue
            }

            if item.isTool {
                var end = i + 1
                // A tool tagged with a subagent belongs to that group, never to
                // a main-thread run.
                while end < feed.count, feed[end].isTool, feed[end].subagentKey == nil {
                    end += 1
                }
                if end - i >= 2 {
                    builders.append(RowBuilder(kind: .toolRun, items: Array(feed[i..<end])))
                    i = end
                    continue
                }
            }

            builders.append(RowBuilder(kind: .single, items: [item]))
            i += 1
        }
        return builders.compactMap(makeRow)
    }

    /// The stepper's current step: the first question of the ask that is
    /// neither resolved nor already answered on this client. nil once every
    /// step is done — the card then renders the whole ask with its answers.
    /// `done` holds the lock keys of steps the desktop acknowledged.
    public static func currentStepIndex(of group: AgentAskGroup, done: Set<String>) -> Int? {
        group.questions.firstIndex { !$0.resolved && !done.contains($0.lockKey) }
    }

    /// Every subagent seen in the feed, in first-appearance order (EXP-356) —
    /// the session view renders one conversation tab per run, labeled and
    /// summarized exactly like its group row.
    public static func subagents(_ feed: [AgentFeedItem]) -> [AgentSubagentRun] {
        rows(feed).compactMap { row in
            if case let .subagentRun(run) = row { return run }
            return nil
        }
    }

    /// The tabs the strip actually shows (EXP-387): running subagents, plus
    /// the focused one even when done — a completion never yanks the user out
    /// of a conversation they are reading; the tab disappears once they click
    /// away. Completed runs stay readable via their inline group row in Main.
    public static func visibleSubagentTabs(
        _ agents: [AgentSubagentRun], selected: String?
    ) -> [AgentSubagentRun] {
        agents.filter { !$0.done || $0.subagentId == selected }
    }

    /// Mutable accumulator behind `rows` — the row cases carry immutable
    /// payloads, but a group keeps collecting items as the feed is walked.
    private struct RowBuilder {
        enum Kind {
            case single
            case toolRun
            case ask(String)
            case subagent(String)
        }

        let kind: Kind
        var items: [AgentFeedItem]
    }

    private static func makeRow(_ builder: RowBuilder) -> AgentFeedRow? {
        switch builder.kind {
        case .single:
            guard let item = builder.items.first else { return nil }
            return .single(item)
        case .toolRun:
            return .toolRun(builder.items)
        case let .ask(askId):
            // Step order, submit step last; the local id breaks ties so the
            // order never depends on the sort's stability.
            let questions = builder.items.compactMap(\.question).sorted { lhs, rhs in
                let left = lhs.index ?? Int.max
                let right = rhs.index ?? Int.max
                return left == right ? lhs.id < rhs.id : left < right
            }
            guard !questions.isEmpty else { return nil }
            return .ask(AgentAskGroup(askId: askId, questions: questions))
        case let .subagent(subagentId):
            guard let anchorId = builder.items.map(\.id).min() else { return nil }
            var types: [String] = []
            var detail: String?
            var done = false
            // EXP-748: the highest count any marker reported — a re-emitted
            // edge must never shrink the row's "N tool calls".
            var reported: Int?
            for item in builder.items {
                guard case let .subagent(_, _, type, status, markerDetail, toolCalls) = item
                else { continue }
                if !type.isEmpty { types.append(type) }
                if status == .completed { done = true }
                if let markerDetail { detail = markerDetail }
                if let toolCalls { reported = max(reported ?? 0, toolCalls) }
            }
            // First marker with a REAL type wins — "agent" is the desktop's
            // fallback sentinel, and old builds stamp it onto the completed
            // edge, so last-wins degraded every finished row's label (EXP-350).
            let agentType = types.first { $0 != Self.subagentFallbackType }
                ?? types.first
                ?? Self.subagentFallbackType
            return .subagentRun(AgentSubagentRun(
                anchorId: anchorId,
                subagentId: subagentId,
                agentType: agentType,
                detail: detail,
                done: done,
                items: builder.items.filter(\.isTool),
                reportedToolCalls: reported
            ))
        }
    }
}
