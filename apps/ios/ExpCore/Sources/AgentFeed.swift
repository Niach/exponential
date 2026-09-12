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

/// EXP-848: whether the agent is inside a TURN right now — the engine's own
/// edges (`turn` activity events), not a guess off the feed's shape.
///
/// A latest-wins slot like `config_state`/`usage`/`rate_limit`, never a feed
/// row: the relay replays the newest one after its log, so a join and a
/// reconnect both land on the truth. Before any `turn` event arrives a client
/// assumes `.ended` — an idle run must never pulse by default. Raw values are
/// the contract's `turnState` list.
public enum AgentTurnState: String, Sendable, CaseIterable {
    case started
    case ended
}

/// EXP-846: the result preview a `tool_update` carries for an Exponential MCP
/// tool call — the engine reads it off the tool's JSON result and publishes
/// only for `exponential_*` calls. Every field is optional (a tool reports
/// what it has) and the publisher caps every string at 200 chars.
///
/// Phase 1 only PLUMBS this: the reducer stores it on the tool row so the
/// custom rendering (an issue pill, a PR link, `N results`) can land later
/// without a protocol change.
public struct AgentToolPreview: Equatable, Sendable {
    public let id: String?
    /// The subject's human identifier (`EXP-849`) when it has one.
    public let identifier: String?
    public let title: String?
    public let url: String?
    /// A list tool's row count.
    public let count: Int?
    public let status: String?

    public init(
        id: String? = nil,
        identifier: String? = nil,
        title: String? = nil,
        url: String? = nil,
        count: Int? = nil,
        status: String? = nil
    ) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.url = url
        self.count = count
        self.status = status
    }

    /// Nothing to draw — every field came back empty.
    public var isEmpty: Bool {
        id == nil && identifier == nil && title == nil && url == nil
            && count == nil && status == nil
    }
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

/// EXP-784: the agent's rate-limit window as it last reported it — the fourth
/// latest-wins slot beside `AgentSessionUsage`. `status` is the agent's own
/// word (`allowed_warning`, `rejected`, …); the slot is CLEARED by an
/// empty/`ok` status (`AgentFeed.rateLimitClears`), never filled by one.
public struct AgentSessionRateLimit: Equatable, Sendable {
    public let status: String
    /// Unix ms when the window resets, when the agent named one.
    public let resetsAt: Int?
    public let message: String?

    public init(status: String, resetsAt: Int? = nil, message: String? = nil) {
        self.status = status
        self.resetsAt = resetsAt
        self.message = message
    }
}

/// EXP-772: the composer's ONE steering control — the agent's mode. Model,
/// effort and every other option picker is gone from a running session: those
/// are launch decisions, and a mid-run swap only ever confused a transcript.
/// The engine publishes an empty `config_state.options` to match.
public struct AgentModeChip: Equatable, Sendable {
    /// The mode in force, as the publisher labels it.
    public let valueLabel: String
    /// Every advertised mode, in publisher order.
    public let values: [AgentConfigValue]
    /// Set when the run advertises exactly `plan` plus one other mode — the
    /// pair claude offers. The chip then draws as a compact "Plan" switch
    /// rather than a two-entry dropdown, because that is the only thing it
    /// could ever say.
    public let planToggle: PlanToggle?

    /// The plan switch's two sides.
    public struct PlanToggle: Equatable, Sendable {
        /// The run is in plan mode right now.
        public let on: Bool
        public let planId: String
        /// Where flipping the switch OFF goes.
        public let otherId: String

        public init(on: Bool, planId: String, otherId: String) {
            self.on = on
            self.planId = planId
            self.otherId = otherId
        }
    }

    public init(
        valueLabel: String, values: [AgentConfigValue] = [], planToggle: PlanToggle? = nil
    ) {
        self.valueLabel = valueLabel
        self.values = values
        self.planToggle = planToggle
    }

    /// A chip with nothing to pick from is a read-only badge.
    public var isReadOnly: Bool { values.isEmpty }
}

/// One rendered feed entry. Diffs never enter the feed — the latest one lives
/// behind the pinned "Latest changes" chip.
public enum AgentFeedItem: Equatable, Sendable, Identifiable {
    /// `messageId` (EXP-772) is the ACP id of the assistant message this prose
    /// came out of — the engine flushes a message in several events, and
    /// consecutive flushes of the SAME message merge into one bubble
    /// (`mergeNarration`). `subagentId` (EXP-773) tags prose a subagent wrote:
    /// it renders inside that subagent's run, never in the main feed.
    case narration(id: Int, text: String, messageId: String? = nil, subagentId: String? = nil)
    /// `subagentId` (protocol v2) tags the tool as a subagent's work — such
    /// runs collapse under their subagent row.
    ///
    /// EXP-785: `callId` is the ACP tool-call id a later `tool_update` folds
    /// into this row by (nil on rows from a pre-EXP-785 publisher, which then
    /// never settle); `toolKind` is ACP's kind bucket (a contract `toolKind`
    /// value); `settled` = a status landed (the call ENDED), `failed` = that
    /// status was `failed` (a later `completed` clears it). EXP-786: `diff`
    /// is the per-call unified diff an `edit` published, already cut to the
    /// contract's caps by the publisher. EXP-846: `preview` is the tool's own
    /// result, folded in by a later `tool_update` and only ever present on an
    /// Exponential MCP call.
    case tool(
        id: Int, name: String, detail: String?, subagentId: String?,
        callId: String? = nil, toolKind: String? = nil,
        settled: Bool = false, failed: Bool = false, diff: String? = nil,
        preview: AgentToolPreview? = nil
    )
    /// A human turn: the initial prompt or a steered message. `subagentId`
    /// (EXP-773) tags a turn addressed to a subagent — same scoping rule as
    /// narration.
    case userMessage(id: Int, text: String, subagentId: String? = nil)
    case question(AgentQuestion)
    /// A subagent started or finished (protocol v2).
    ///
    /// EXP-748: `toolCalls` is the publisher's own count of this subagent's
    /// tool calls, stamped on the completed edge. The replay log evicts
    /// subagent tool events first, so the visible rows can undercount — the
    /// run renders `max(visible rows, toolCalls)`.
    ///
    /// EXP-847: `title` is the spawning Agent tool call's own `description`
    /// (its `name` input as a fallback) — what the person asked this subagent
    /// for, which reads far better than the bare `agentType`. Absent on older
    /// publishers and on a spawn that named neither.
    case subagent(
        id: Int, subagentId: String, agentType: String,
        status: AgentSubagentStatus, detail: String?, toolCalls: Int? = nil,
        title: String? = nil
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
        case let .narration(id, _, _, _): id
        case let .tool(id, _, _, _, _, _, _, _, _, _): id
        case let .userMessage(id, _, _): id
        case let .question(value): value.id
        case let .subagent(id, _, _, _, _, _, _): id
        case let .permission(id, _, _): id
        case let .compaction(id): id
        }
    }

    /// EXP-783: the same row under a new feed id. Used only by the older-page
    /// prepend, which folds a page through the ordinary reducer (ids from
    /// zero) and then renumbers it BELOW everything on screen, so no visible
    /// row's identity changes.
    public func withId(_ id: Int) -> AgentFeedItem {
        switch self {
        case let .narration(_, text, messageId, subagentId):
            .narration(id: id, text: text, messageId: messageId, subagentId: subagentId)
        case let .tool(
            _, name, detail, subagentId, callId, toolKind, settled, failed, diff, preview
        ):
            .tool(
                id: id, name: name, detail: detail, subagentId: subagentId,
                callId: callId, toolKind: toolKind, settled: settled, failed: failed,
                diff: diff, preview: preview
            )
        case let .userMessage(_, text, subagentId):
            .userMessage(id: id, text: text, subagentId: subagentId)
        case let .question(question):
            {
                var next = question
                next.id = id
                return .question(next)
            }()
        case let .subagent(_, subagentId, agentType, status, detail, toolCalls, title):
            .subagent(
                id: id, subagentId: subagentId, agentType: agentType,
                status: status, detail: detail, toolCalls: toolCalls, title: title
            )
        case let .permission(_, tool, detail):
            .permission(id: id, tool: tool, detail: detail)
        case .compaction:
            .compaction(id: id)
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
    /// subagent run. EXP-773: prose and human turns carry it too, so a
    /// subagent's whole conversation groups under its own row instead of
    /// interleaving into the main thread.
    public var subagentKey: String? {
        switch self {
        case let .tool(_, _, _, subagentId, _, _, _, _, _, _): return subagentId
        case let .subagent(_, subagentId, _, _, _, _, _): return subagentId
        case let .narration(_, _, _, subagentId): return subagentId
        case let .userMessage(_, _, subagentId): return subagentId
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
    /// EXP-847: what the spawn ASKED for (the Agent call's `description`), when
    /// a marker named one — the label every surface prefers over `agentType`.
    public let title: String?
    public let detail: String?
    /// A `completed` marker arrived.
    public let done: Bool
    /// Everything published under this subagent, in feed order — its tool
    /// calls plus (EXP-773) the prose it wrote and the turns addressed to it,
    /// so the run reads as a conversation instead of a list of calls. The
    /// lifecycle markers themselves stay out: they ARE the row.
    public let items: [AgentFeedItem]
    /// EXP-748: the highest count the subagent's own markers reported, if any.
    /// Replay evicts subagent tool events first, so `items` can undercount.
    public let reportedToolCalls: Int?

    public var id: Int { anchorId }
    /// EXP-847: the label every surface draws — what the spawn asked for, with
    /// the agent's TYPE as the fallback (older publishers send no title, and a
    /// spawn may name neither). Mirrored ×4.
    public var label: String { title ?? agentType }
    /// The reported count wins whenever it is higher than what is visible.
    public var toolCount: Int {
        max(items.filter(\.isTool).count, reportedToolCalls ?? 0)
    }
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
        reportedToolCalls: Int? = nil,
        title: String? = nil
    ) {
        self.anchorId = anchorId
        self.subagentId = subagentId
        self.agentType = agentType
        self.title = title
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

/// EXP-787 — what a transcript row is for the shared gap ladder. Three
/// classes, not seven row cases: the rhythm is about human turns, prose and
/// tool noise, and nothing finer than that ever changes the spacing.
public enum AgentRowClass: Equatable, Sendable {
    /// A human turn — the prose bubble and the slash-command pill alike.
    case turn
    /// Assistant prose and the cards that read as prose (asks, plans, the
    /// compaction marker).
    case prose
    /// Tool noise: single calls, collapsed runs, subagents, permission
    /// markers — and the trailing "Working…" row, which is the same weight.
    case tool
}

extension AgentFeedRow {
    /// The row's class in the gap ladder.
    public var rowClass: AgentRowClass {
        switch self {
        case .toolRun, .subagentRun: .tool
        case .ask: .prose
        case let .single(item):
            switch item {
            case .userMessage: .turn
            case .narration, .question, .compaction: .prose
            case .tool, .subagent, .permission: .tool
            }
        }
    }
}

/// The space ABOVE one transcript row (EXP-787), named rather than measured:
/// ExpCore cannot see ExpUI, so the session view maps these onto the shared
/// `DesignTokens.Transcript` gap tokens.
public enum AgentTranscriptGap: Equatable, Sendable {
    /// The first row of the transcript — nothing above it to space against.
    case none
    /// `gapTurn` — either side of a human turn.
    case turn
    /// `gapBlock` — between two prose rows.
    case block
    /// `gapTool` — where prose meets a tool row.
    case tool
    /// `gapDefault` — between two consecutive tool rows.
    case `default`
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

    /// EXP-820: a RE-answer of an already-acked step (the stepper's "go back")
    /// goes through here too — `acked` is left standing, so the step never
    /// rolls back into the current slot if this second frame goes unconfirmed;
    /// only its pending spinner and labels move.
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
    /// EXP-783: the transcript keeps the WHOLE run — the session view paints a
    /// WINDOW over it (`feedWindow`) and grows it upward, so keeping everything
    /// costs nothing per frame. These are safety ceilings on the app's memory,
    /// not a display cap, and they are sized to the device journal
    /// (JOURNAL_FILE_CAP) because that file is exactly what a replay reads
    /// back. The relay's ACTIVITY_LOG_CAP is deliberately NOT matched any
    /// more: it bounds the tail a joining viewer replays, and older pages are
    /// asked for (`history_page`) rather than pushed.
    ///
    /// Every number is the contract's `steerFeed` section (EXP-795): web,
    /// Android and the desktop read the same generated constants, so the
    /// budgets, the trim target and the window cannot drift per client.
    public static let feedByteCap = DomainContract.steerFeedByteCap
    /// The companion item ceiling — a run of tiny events would sit far under
    /// the byte budget while costing an array slot each.
    public static let feedItemCap = DomainContract.steerFeedItemCap
    /// How many of the run's newest rows the session view renders, and how
    /// much older transcript one "Load earlier" pulls in.
    public static let feedWindow = DomainContract.steerFeedWindow
    public static let feedWindowStep = DomainContract.steerFeedWindowStep
    /// EXP-783: events per `history_page` ask — the relay's schema rejects
    /// anything larger.
    public static let historyPageLimit = DomainContract.steerFeedHistoryPageMax

    /// What one row weighs against `feedByteCap`: the text it carries plus a
    /// flat per-item overhead standing in for the value itself. An estimate on
    /// purpose — this budget bounds memory, it does not account for it.
    public static func itemBytes(_ item: AgentFeedItem) -> Int {
        let overhead = DomainContract.steerFeedItemOverheadBytes
        switch item {
        case let .narration(_, text, _, _): return overhead + text.utf8.count
        case let .userMessage(_, text, _): return overhead + text.utf8.count
        case let .tool(_, name, detail, _, _, _, _, _, diff, _):
            // EXP-786: a folded per-call diff weighs too.
            return overhead + name.utf8.count + (detail?.utf8.count ?? 0) + (diff?.utf8.count ?? 0)
        case let .permission(_, tool, detail):
            return overhead + tool.utf8.count + (detail?.utf8.count ?? 0)
        case let .subagent(_, subagentId, agentType, _, detail, _, title):
            return overhead + subagentId.utf8.count + agentType.utf8.count
                + (detail?.utf8.count ?? 0) + (title?.utf8.count ?? 0)
        case let .question(question):
            return overhead + question.text.utf8.count
                + question.answers.reduce(0) { $0 + $1.utf8.count }
                + (question.header?.utf8.count ?? 0)
                + question.options.reduce(0) { $0 + $1.label.utf8.count + $1.key.utf8.count }
        case .compaction: return overhead
        }
    }

    /// Evict from the OLDEST end until the feed is inside both budgets, with
    /// the 90% hysteresis every client shares (`steer::feed::trim`): evicting
    /// down to the budget exactly would evict again on the very next event,
    /// and every eviction reallocates. The newest row always survives.
    /// Returns the surviving feed and its recomputed weight.
    public static func trim(
        feed: [AgentFeedItem], bytes: Int
    ) -> (feed: [AgentFeedItem], bytes: Int) {
        if bytes <= feedByteCap && feed.count <= feedItemCap { return (feed, bytes) }
        let percent = DomainContract.steerFeedTrimTargetPercent
        let byteTarget = feedByteCap * percent / 100
        let itemTarget = feedItemCap * percent / 100
        var remaining = bytes
        var dropTo = 0
        while dropTo + 1 < feed.count
            && (remaining > byteTarget || feed.count - dropTo > itemTarget) {
            remaining -= itemBytes(feed[dropTo])
            dropTo += 1
        }
        guard dropTo > 0 else { return (feed, bytes) }
        return (Array(feed[dropTo...]), max(0, remaining))
    }
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
    /// The mode chip's leading label, mirrored ×4 — the only chip label left,
    /// so the only one the clients have to agree on by hand.
    public static let configModeLabel = "Mode"
    /// EXP-772: the contract id of plan mode, and the label the compact
    /// switch that replaces a `plan` + one other pair carries. Byte-identical
    /// ×4.
    public static let planModeId = "plan"
    public static let planToggleLabel = "Plan"

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

    /// EXP-785: ACP's tool-call kinds — the contract's `toolKind` list.
    public static let toolKindValues = DomainContract.toolKindValues

    /// A wire `toolKind`, or nil for anything this build does not know.
    public static func toolKind(_ raw: Any?) -> String? {
        guard let value = raw as? String, toolKindValues.contains(value) else { return nil }
        return value
    }

    /// EXP-846: the Exponential-tool result preview a `tool_update` carries,
    /// or nil for every other call (and for a preview whose every field came
    /// back empty). A blank string counts as absent; the publisher already
    /// capped every one of them at 200 chars.
    public static func toolPreview(_ raw: Any?) -> AgentToolPreview? {
        guard let row = raw as? [String: Any] else { return nil }
        let preview = AgentToolPreview(
            id: string(row["id"]),
            identifier: string(row["identifier"]),
            title: string(row["title"]),
            url: string(row["url"]),
            count: (row["count"] as? NSNumber)?.intValue,
            status: string(row["status"])
        )
        return preview.isEmpty ? nil : preview
    }

    /// EXP-785/786: fold a `tool_update` into the NEWEST tool row whose
    /// `callId` matches — a settle (`status`), a per-call `diff`, an EXP-846
    /// result `preview`, or any mix. Never a row of its own. Nil = no row
    /// holds that id (evicted, or below the window, or a pre-EXP-785 row), and
    /// the caller keeps the feed as is. A `failed` after a `completed` wins; a
    /// status-less update carrying only a diff never settles the call.
    public static func applyToolUpdate(
        feed: [AgentFeedItem], event: [String: Any]
    ) -> [AgentFeedItem]? {
        guard let id = string(event["id"]),
              let at = feed.lastIndex(where: { item in
                  if case let .tool(_, _, _, _, callId, _, _, _, _, _) = item {
                      return callId == id
                  }
                  return false
              }),
              case let .tool(
                  rowId, name, detail, subagentId, callId, toolKind,
                  settled, failed, diff, preview
              ) = feed[at]
        else { return nil }
        var nextSettled = settled
        var nextFailed = failed
        if let status = event["status"] as? String, status == "completed" || status == "failed" {
            nextSettled = true
            nextFailed = status == "failed"
        }
        let nextDiff = string(event["diff"]) ?? diff
        // EXP-846: latest preview wins; an update without one keeps what the
        // row already shows (a settle and the result can arrive apart).
        let nextPreview = toolPreview(event["preview"]) ?? preview
        var next = feed
        next[at] = .tool(
            id: rowId, name: name, detail: detail, subagentId: subagentId,
            callId: callId, toolKind: toolKind,
            settled: nextSettled, failed: nextFailed, diff: nextDiff,
            preview: nextPreview
        )
        return next
    }

    // MARK: - Turn edges (EXP-848)

    /// Fold a `turn` activity event into the latest-wins slot. An unknown (or
    /// missing) `state` leaves it exactly as it was — the `applyCompaction`
    /// contract: a malformed frame must never flip the working indicator.
    public static func applyTurn(
        _ current: AgentTurnState, event: [String: Any]
    ) -> AgentTurnState {
        guard let raw = event["state"] as? String,
              let next = AgentTurnState(rawValue: raw) else { return current }
        return next
    }

    /// EXP-848 — the ONE working predicate, mirrored ×4 (web `working`,
    /// Android `agentWorking`, desktop `steer::working`).
    ///
    /// `turnState` is the engine's own edge, so nothing here infers activity
    /// from the feed's shape; everything else is a reason the run is NOT
    /// working even mid-turn: it is over, it is waiting on a human (a card, or
    /// the device-written `needs_input`), it is walled (`blocked`), or it is
    /// folding its context away (which has its own strip).
    public static func working(
        live: Bool,
        sessionEnded: Bool,
        turnState: AgentTurnState,
        awaitingInput: Bool,
        needsInput: Bool,
        blocked: Bool,
        compacting: Bool
    ) -> Bool {
        live && !sessionEnded && turnState == .started
            && !awaitingInput && !needsInput && !blocked && !compacting
    }

    // MARK: - The per-call diff (EXP-786)

    /// A publisher-cut diff ends in ONE metadata line saying how much it
    /// dropped (`\ 120 more lines truncated`). Split it off: the diff proper
    /// renders as a diff, the note as a muted footer — rendered as a diff LINE
    /// it would read as context the agent actually saw.
    ///
    /// Hand-mirrored from web `splitTruncatedDiff` (`lib/agent-feed.ts`),
    /// whose regex is `/(?:^|\n)\\ (\d+) more lines? truncated\s*$/`: only the
    /// LAST line counts, trailing whitespace is tolerated, and git's own
    /// `\ No newline at end of file` marker — same `\ ` prefix — is left in
    /// the diff where it belongs.
    public static func splitTruncatedDiff(_ diff: String) -> (diff: String, truncated: Int?) {
        // `\s*$`: the note may be followed by blank space and nothing else.
        var end = diff.endIndex
        while end > diff.startIndex, diff[diff.index(before: end)].isWhitespace {
            end = diff.index(before: end)
        }
        let body = diff[diff.startIndex..<end]
        // `(?:^|\n)`: the note is the whole string, or the last line of it.
        let lineStart = body.lastIndex(of: "\n").map(body.index(after:)) ?? body.startIndex
        guard let count = truncationCount(body[lineStart...]) else { return (diff, nil) }
        // The match SWALLOWS its leading newline, so the diff keeps no blank
        // last line.
        let cut = lineStart == body.startIndex ? body.startIndex : body.index(before: lineStart)
        return (String(body[body.startIndex..<cut]), count)
    }

    /// `\ 120 more lines truncated` → 120. Nil for anything else, git's
    /// `\ No newline at end of file` included.
    private static func truncationCount(_ line: Substring) -> Int? {
        guard line.hasPrefix("\\ ") else { return nil }
        let rest = line.dropFirst(2)
        // `\d+` is ASCII-only; a Devanagari numeral is not a line count.
        let digits = rest.prefix { $0.isASCII && $0.isNumber }
        guard !digits.isEmpty, let count = Int(digits) else { return nil }
        let tail = rest.dropFirst(digits.count)
        return tail == " more line truncated" || tail == " more lines truncated" ? count : nil
    }

    /// The footer under a cut diff. Locked ×4 with web `diffTruncationNote`.
    public static func diffTruncationNote(_ lines: Int) -> String {
        "\(lines) more line\(lines == 1 ? "" : "s") truncated"
    }

    /// EXP-784: the `rate_limit.status` values that CLEAR the slot.
    public static func rateLimitClears(_ status: String) -> Bool {
        let trimmed = status.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || trimmed.lowercased() == "ok"
    }

    /// Fold a `rate_limit` activity event. Nil CLEARS the slot — for an
    /// empty/`ok` status (the window lifted) AND for an unreadable payload: a
    /// stale "rate limited" banner beside a live run is the worse error.
    public static func applyRateLimit(
        _ current: AgentSessionRateLimit?, event: [String: Any]
    ) -> AgentSessionRateLimit? {
        guard let status = event["status"] as? String, !rateLimitClears(status) else { return nil }
        let resetsAt = (event["resetsAt"] as? NSNumber)?.intValue
        return AgentSessionRateLimit(
            status: status.trimmingCharacters(in: .whitespacesAndNewlines),
            resetsAt: (resetsAt ?? -1) >= 0 ? resetsAt : nil,
            message: string(event["message"])?.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    /// EXP-818/831: whether a rate-limit report is a WALL worth a banner —
    /// `rejected`, or a notice the agent itself wrote. Claude files
    /// `allowed_warning` on every turn past ~75% of a window while it keeps
    /// working; the Usage sheet carries that percentage. Mirrors web
    /// `rateLimitIsWall` / desktop `steer::rate_limit_is_wall`.
    public static func rateLimitIsWall(_ limit: AgentSessionRateLimit) -> Bool {
        if limit.status.trimmingCharacters(in: .whitespacesAndNewlines) == "rejected" {
            return true
        }
        return limit.message.map {
            !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        } ?? false
    }

    /// EXP-831: how long past its `resetsAt` a wall still renders. The
    /// engine clears the slot on the run's next activity or its next
    /// rate-limit event; until one arrives (and on a journal replayed after
    /// the fact) the clock is the only thing that can drop a banner whose
    /// reset has come and gone. One minute covers clock skew.
    public static let rateLimitExpiryGraceMs = 60_000

    /// EXP-831: whether the wall's reset is more than the grace behind `now`.
    /// A wall with no reset time never expires by the clock. Mirrored ×4.
    public static func rateLimitExpired(_ limit: AgentSessionRateLimit, now: Date) -> Bool {
        guard let resetsAt = limit.resetsAt else { return false }
        let nowMs = Int(now.timeIntervalSince1970 * 1000)
        return nowMs - resetsAt > rateLimitExpiryGraceMs
    }

    /// EXP-831: the banner's ONE gate — a wall whose reset has not passed.
    /// The view re-reads it on the model's 30s activity clock.
    public static func rateLimitBannerShows(_ limit: AgentSessionRateLimit, now: Date) -> Bool {
        rateLimitIsWall(limit) && !rateLimitExpired(limit, now: now)
    }

    /// EXP-784: the ONE line the rate-limit banner prints — the agent's
    /// message (or a plain "Rate limited" when it sent none) and, when the
    /// window names its end, ` · resets HH:MM` in LOCAL time. Pure so the
    /// clock format is testable against a pinned zone.
    public static func rateLimitCaption(
        _ limit: AgentSessionRateLimit, timeZone: TimeZone = .current
    ) -> String {
        let message = limit.message.flatMap { $0.isEmpty ? nil : $0 } ?? "Rate limited"
        guard let resetsAt = limit.resetsAt else { return message }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "HH:mm"
        let at = Date(timeIntervalSince1970: TimeInterval(resetsAt) / 1000)
        return "\(message) · resets \(formatter.string(from: at))"
    }

    // MARK: - Load earlier (EXP-783/796)

    /// EXP-796: whether the transcript offers "Load earlier". Rows the feed
    /// already holds above the window are always pageable; a page that only
    /// the DEVICE's journal has needs the relay to have said its replay was a
    /// tail, not to have run dry on it, AND an open viewer socket to ask on —
    /// the relay keeps a history room open after the device's replay, so
    /// pages keep flowing until the socket really closes, and a closed one
    /// can't carry the ask at all.
    public static func canLoadEarlier(
        windowStart: Int, historyTruncated: Bool, historyExhausted: Bool, connected: Bool
    ) -> Bool {
        windowStart > 0 || (historyTruncated && !historyExhausted && connected)
    }

    // MARK: - Inline card answers (EXP-820)

    /// The composer's placeholder. EXP-820: the composer HIDES while a card
    /// is pending (its free answer is an inline field on the card itself), so
    /// there is only the one generic prompt.
    public static let composerPlaceholder = "Message the agent…"
    /// EXP-820: the inline field a free-text row ("Type something.") expands
    /// into. Byte-identical ×4 (web `agent-session.tsx`).
    public static let freeTextPlaceholder = "Type your answer…"
    /// EXP-820: the inline field a plan's reject row ("No, keep planning")
    /// expands into — feedback that follows the deny as the next message.
    public static let planFeedbackPlaceholder = "Tell the agent what to change…"
    /// EXP-820: the text button under an earlier step being re-answered.
    public static let backToCurrentStepLabel = "Back to current step"
    /// The multi-select submit label, byte-identical ×4.
    public static let submitLabel = "Submit"

    /// EXP-820: the key of a plan card's reject row — the ONE option that
    /// expands into the inline feedback field. "No, keep planning" is last by
    /// contract ("Sends your next message back to planning"); `reject` is its
    /// wire id whenever the engine named one. nil for a non-plan or an
    /// option-less card.
    public static func planRejectKey(for question: AgentQuestion) -> String? {
        guard question.planMode, !question.options.isEmpty else { return nil }
        let reject = question.options.first(where: { $0.key == "reject" })
            ?? question.options[question.options.count - 1]
        return reject.key
    }

    /// EXP-820: whether a multi-question ask is OVER — nothing left to wait
    /// on, no earlier step to go back to. ONE rule ×4: a submit step exists
    /// AND is resolved, OR the ask has at most one numbered step and every
    /// numbered step is resolved (a lone-step ask has no review step), OR any
    /// step was dismissed (the engine tore the whole ask down).
    public static func askComplete(_ group: AgentAskGroup) -> Bool {
        if group.questions.contains(where: \.dismissed) { return true }
        if let submit = group.questions.first(where: \.isSubmitStep) {
            if submit.resolved { return true }
        }
        // The ask's own count, not the published one: a two-question ask
        // with its first step resolved is still waiting on the second.
        let numbered = group.questions.filter { !$0.isSubmitStep }
        let total = numbered.first?.total ?? numbered.count
        return total <= 1 && !numbered.isEmpty && numbered.allSatisfy(\.resolved)
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

    /// The composer's ONE chip (EXP-772): the agent's mode, or nil when the
    /// run advertises none — codex advertises an empty list, and that draws
    /// nothing at all rather than an inert badge.
    ///
    /// Exactly `plan` + one other mode collapses into the compact Plan switch;
    /// anything else stays a labelled picker over every advertised mode.
    /// Mirrored ×4.
    public static func modeChip(_ config: AgentSessionConfig?) -> AgentModeChip? {
        guard let config, !config.modes.isEmpty else { return nil }
        let current = config.modes.first { $0.id == config.currentMode }
        var toggle: AgentModeChip.PlanToggle?
        if config.modes.count == 2,
           let plan = config.modes.first(where: { $0.id == planModeId }),
           let other = config.modes.first(where: { $0.id != planModeId }) {
            toggle = AgentModeChip.PlanToggle(
                on: config.currentMode == plan.id, planId: plan.id, otherId: other.id
            )
        }
        return AgentModeChip(
            valueLabel: current?.label ?? config.currentMode ?? configDefaultValueLabel,
            values: config.modes.map { AgentConfigValue(id: $0.id, label: $0.label) },
            planToggle: toggle
        )
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

    /// EXP-772: fold a narration event into the row above it when both came
    /// out of the SAME assistant message.
    ///
    /// The engine's coalescer flushes one message in several `narration`
    /// events, which used to draw one bubble per flush and shred a paragraph
    /// into a column of fragments. Every event now carries the ACP `messageId`
    /// of its message, so a flush whose message is the one directly above
    /// APPENDS to that row (raw concatenation — the flushes are chunks of one
    /// string, not sentences). Anything in between (a tool call, a question,
    /// another subagent's prose) ends the run: the message really did resume
    /// after something happened, and that is worth its own bubble.
    ///
    /// nil = nothing to merge into, and the caller appends a fresh row.
    /// Mirrored ×4 (web `agent-feed.ts`, Android `AgentFeed.kt`, desktop
    /// `feed.rs`).
    public static func mergeNarration(
        _ feed: [AgentFeedItem], text: String, messageId: String?, subagentId: String?
    ) -> [AgentFeedItem]? {
        guard let messageId, !messageId.isEmpty else { return nil }
        guard case let .narration(id, existing, previousMessage, previousSubagent) = feed.last,
              previousMessage == messageId, previousSubagent == subagentId
        else { return nil }
        var out = feed
        out[out.count - 1] = .narration(
            id: id, text: existing + text, messageId: messageId, subagentId: subagentId
        )
        return out
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
    /// EXP-820: a by-id resolution matches an ALREADY-resolved card too — the
    /// engine re-publishes `question_resolved` for an earlier step that was
    /// re-answered, and its new answers replace the recorded ones.
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

    /// The gap ladder (EXP-787): the space above `cur`, chosen from the row
    /// before it. ONE derivation, mirrored on all four clients (web
    /// `lib/agent-feed.ts`, desktop `steer::feed`, Android `domain/AgentFeed`)
    /// — a human turn gets the widest air on EITHER side, two tool rows the
    /// tightest, prose meeting a tool row the step in between, and two prose
    /// rows the block gap. Order matters: the turn rule wins over everything.
    /// `prev == nil` is the first row, which has no gap at all.
    public static func transcriptGap(
        prev: AgentRowClass?, cur: AgentRowClass
    ) -> AgentTranscriptGap {
        guard let prev else { return .none }
        if prev == .turn || cur == .turn { return .turn }
        switch (prev == .tool, cur == .tool) {
        case (true, true): return .default
        case (true, false), (false, true): return .tool
        case (false, false): return .block
        }
    }

    /// Render rows over the flat feed — a projection only, the feed stays the
    /// state (and `activeQuestionIds` keeps operating on it): every card of one
    /// ask collapses into a stepper row, a subagent's markers and calls into
    /// its own group, and runs of ≥2 consecutive plain tool calls into a "N
    /// tool calls" row (EXP-97). Grouped items are pulled OUT of their in-place
    /// position into the row their group opened, so a late-arriving step (or a
    /// subagent call that lands behind an unrelated one) still joins its group.
    public static func rows(_ feed: [AgentFeedItem], from start: Int = 0) -> [AgentFeedRow] {
        var builders: [RowBuilder] = []
        var askAt: [String: Int] = [:]
        var subagentAt: [String: Int] = [:]
        // EXP-783: `start` restricts the projection to the rendered WINDOW.
        // The grouping state begins empty there, so a window that cuts through
        // a tool run, an ask or a subagent's calls opens a FRESH group at the
        // boundary — keyed on the first item the reader can actually see.
        // `start = 0` is the whole projection, item for item.
        var i = max(0, min(start, feed.count))
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
            // EXP-847: the FIRST marker that named a title wins — the spawn's
            // own `description` rides the `started` edge, and a completed edge
            // that carries none must not blank the row's label.
            var title: String?
            for item in builder.items {
                guard case let .subagent(_, _, type, status, mark, calls, named) = item
                else { continue }
                if !type.isEmpty { types.append(type) }
                if status == .completed { done = true }
                if let mark { detail = mark }
                if let calls { reported = max(reported ?? 0, calls) }
                if title == nil, let named, !named.isEmpty { title = named }
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
                // EXP-773: everything but the lifecycle markers, in order.
                items: builder.items.filter { if case .subagent = $0 { false } else { true } },
                reportedToolCalls: reported,
                title: title
            ))
        }
    }
}
