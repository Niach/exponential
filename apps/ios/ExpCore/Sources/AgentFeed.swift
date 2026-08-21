import Foundation

// Steer protocol v2 (EXP-249) — the pure core of the "Agent session" activity
// feed: the wire's question / subagent / permission shapes, the render grouping
// over the flat feed, and the per-card answer lock. Foundation only (ExpCore
// rule) and free of SwiftUI, so ExpCoreTests can drive all of it. The view
// model (Exponential/UI/Session/AgentSessionModel.swift) owns only the socket.

/// One answer choice of a `question` activity event. `key` is what a steering
/// client submits: protocol v2 puts it inside the semantic `answer` frame (the
/// desktop maps keys onto its own picker), older desktops take it as the raw
/// TUI keystroke.
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
    /// Protocol-v2 stable question id. nil = a legacy desktop that only
    /// screen-scraped the TUI: no `answer` frame and no explicit resolution, so
    /// the trailing-run heuristics below still apply to it.
    public let wireId: String?
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
        wireId: String? = nil,
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

    /// Answerable through the semantic `answer` frame (protocol v2).
    public var isSemantic: Bool { wireId != nil }

    /// The ask's final review/submit step: it belongs to an ask but carries no
    /// step position of its own.
    public var isSubmitStep: Bool { askId != nil && index == nil }

    /// Key of the per-card answer lock — the wire id when there is one, else a
    /// local stand-in so legacy cards lock against double taps too.
    public var lockKey: String { wireId ?? "local:\(id)" }

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
    case subagent(
        id: Int, subagentId: String, agentType: String,
        status: AgentSubagentStatus, detail: String?
    )
    /// A permission prompt the agent hit (protocol v2) — INFORMATIONAL: the
    /// desktop's own TUI owns the approval, there is nothing to answer here.
    case permission(id: Int, tool: String, detail: String?)

    public var id: Int {
        switch self {
        case let .narration(id, _): id
        case let .tool(id, _, _, _): id
        case let .userMessage(id, _): id
        case let .question(value): value.id
        case let .subagent(id, _, _, _, _): id
        case let .permission(id, _, _): id
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
        case let .subagent(_, subagentId, _, _, _): return subagentId
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

    public var id: Int { anchorId }
    public var toolCount: Int { items.count }
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
        items: [AgentFeedItem]
    ) {
        self.anchorId = anchorId
        self.subagentId = subagentId
        self.agentType = agentType
        self.detail = detail
        self.done = done
        self.items = items
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
    /// The desktop's plan-picker resolution narration (steer/src/activity.rs) —
    /// the legacy, pre-`question_resolved` signal that a pending plan approval
    /// was answered. Still emitted alongside the semantic events so old clients
    /// keep working; new clients only fall back to it for cards with no wire id.
    public static let planResolvedNarration = "Plan approval answered."
    /// The desktop's answered-question narration prefix (legacy path).
    public static let questionAnsweredPrefix = "Question answered: "
    /// The desktop's dismissed-question narration (legacy path).
    public static let questionDismissedNarration = "Question dismissed."
    /// Client-side feed cap — old events fall off the top. Matches the relay's
    /// ACTIVITY_LOG_CAP so a full replay never truncates.
    public static let feedCap = 2000
    /// `subagent.agentType` when the desktop's hook payload carried none — old
    /// desktop builds also stamp it onto the COMPLETED edge, so it is a
    /// sentinel the label selection skips past, never a type to prefer
    /// (EXP-350).
    public static let subagentFallbackType = "agent"

    /// Ids of the question items still answerable.
    ///
    /// Protocol-v2 cards (a wire id) are answerable until the desktop retires
    /// them with `question_resolved` — no screen-scraping heuristics apply.
    /// LEGACY cards keep the EXP-174 rule: the TRAILING consecutive question
    /// run, plus any plan-approval card with no resolution signal after it
    /// (plan pickers are published from the live terminal grid the moment they
    /// appear, so lagging transcript flushes can land BEHIND a picker that is
    /// still on screen). Mirrors the Android `activeQuestionIds`.
    public static func activeQuestionIds(_ feed: [AgentFeedItem]) -> Set<Int> {
        var ids = Set<Int>()
        // Still inside the trailing consecutive question run.
        var trailing = true
        // A resolution signal lies after the current position.
        var retired = false
        // The scan runs the whole feed (no early exit): v2 cards resolve
        // explicitly, so an unresolved one stays active however far back it is.
        for item in feed.reversed() {
            switch item {
            case let .question(question) where question.isSemantic:
                if !question.resolved { ids.insert(question.id) }
                // Still a question event for the LEGACY cards before it —
                // seeing one proves the picker moved on.
                trailing = false
                retired = true
            case let .question(question):
                if question.resolved {
                    // An answered/dismissed card is itself a resolution signal
                    // and is never active.
                    trailing = false
                    retired = true
                } else {
                    if trailing || (question.planMode && !retired) { ids.insert(question.id) }
                    retired = true
                }
            // A human message breaks the trailing run but does NOT retire a
            // plan card — steering a message mid-plan leaves the picker up
            // (web parity, EXP-249).
            case .userMessage:
                trailing = false
            case let .narration(_, text):
                trailing = false
                if text.trimmingCharacters(in: .whitespacesAndNewlines) == planResolvedNarration {
                    retired = true
                }
            case .tool, .subagent, .permission:
                trailing = false
            }
        }
        return ids
    }

    /// Whether the desktop publishes question identities. From the first such
    /// card on, its legacy resolution narrations are duplicates of the semantic
    /// events (it emits both so pre-EXP-249 clients keep working) and must be
    /// SWALLOWED instead of rendered.
    public static func hasSemanticQuestions(_ feed: [AgentFeedItem]) -> Bool {
        feed.contains { $0.question?.isSemantic == true }
    }

    /// Fold a legacy `Question answered: <answer>` narration into the EARLIEST
    /// unanswered non-plan LEGACY card (answers arrive in question order, so
    /// earliest-first keeps multi-question asks aligned). nil when no card is
    /// waiting — the caller renders the narration so the answer is never lost.
    /// v2 cards are never touched: they resolve through `question_resolved`.
    public static func attachQuestionAnswer(
        _ feed: [AgentFeedItem], answer: String
    ) -> [AgentFeedItem]? {
        guard let index = feed.firstIndex(where: { item in
            guard let question = item.question else { return false }
            return !question.isSemantic && !question.planMode && !question.resolved
        }), let question = feed[index].question else { return nil }
        var updated = question
        updated.resolved = true
        updated.answers = [answer]
        var out = feed
        out[index] = .question(updated)
        return out
    }

    /// Retire every pending non-plan LEGACY card (the ask was dismissed).
    /// nil when nothing changed.
    public static func dismissPendingQuestions(_ feed: [AgentFeedItem]) -> [AgentFeedItem]? {
        var out = feed
        var changed = false
        for i in out.indices {
            guard let question = out[i].question,
                  !question.isSemantic, !question.planMode, !question.resolved
            else { continue }
            var updated = question
            updated.resolved = true
            out[i] = .question(updated)
            changed = true
        }
        return changed ? out : nil
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
        guard let wireId = question.wireId,
              let index = feed.firstIndex(where: { $0.question?.wireId == wireId }),
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
    /// nil when no card matches (evicted, legacy producer) — the caller
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
            for item in builder.items {
                guard case let .subagent(_, _, type, status, markerDetail) = item else { continue }
                if !type.isEmpty { types.append(type) }
                if status == .completed { done = true }
                if let markerDetail { detail = markerDetail }
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
                items: builder.items.filter(\.isTool)
            ))
        }
    }
}
