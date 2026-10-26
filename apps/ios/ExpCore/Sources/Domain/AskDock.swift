import Foundation

/// THE ask cards ×4 (`packages/domain-contract/fixtures/ask-dock.json`, TS
/// reference `src/ask-dock.ts`): N-12: a question or plan card waiting on the
/// viewer is a DockCard of THE Dock (ExpCore `Dock`, fixtures/dock.json); the
/// composer never leaves. This keeps the cards' own words and models: the
/// row's label and title, the plan options ONE list ×4 (`planOptions`), a
/// thread's waiting cards. `AskDockTests` replays every fixture case.
public enum AskDock {
    public enum Kind: String, Equatable, Sendable {
        case plan, question
    }

    /// One card waiting on the viewer.
    public struct Pending: Equatable, Sendable, Identifiable {
        /// The card's id (a `messages.id`, or the session view's ask id).
        public let id: String
        public let kind: Kind
        /// The plan, or the question's text (markdown).
        public let text: String
        /// The asking session's name in a thread; nil in the session view.
        public let sessionName: String?
        /// Stack order, oldest first.
        public let createdAt: Double
        /// A plan's question count (a question: nil).
        public let questions: Int?

        public init(
            id: String, kind: Kind, text: String, sessionName: String?, createdAt: Double, questions: Int? = nil
        ) {
            self.id = id
            self.kind = kind
            self.text = text
            self.sessionName = sessionName
            self.createdAt = createdAt
            self.questions = questions
        }

        /// The Dock's source for this card.
        public var dockSource: Dock.Source {
            kind == .plan
                ? .plan(id: id, text: text, sessionName: sessionName, questions: questions ?? 0, createdAt: createdAt)
                : .question(id: id, text: text, sessionName: sessionName, createdAt: createdAt)
        }
    }

    public struct PlanOption: Equatable, Sendable, Identifiable {
        public let key: String
        public let label: String
        public let description: String?
        public let decision: String
        public let fresh: Bool
        public var id: String { key }
    }

    // MARK: - Fixture constants

    public enum Copy {
        public static let planReady = "Plan ready"
        public static let question = "Question"
        public static let back = "Back to all cards"
        public static let feedbackPlaceholder = "Tell the agent what to change…"
        public static let cancel = "Cancel"
        /// FEED-140: a plan the agent replaced with a newer one.
        public static let superseded = "Replaced by a newer plan"
    }

    public enum Icons {
        public static let plan = "coding-plan"
        public static let question = "ui-help"
    }

    /// THE plan card's options ×4 (the claude adapter's ExitPlanMode menu):
    /// Yes (the footer's primary) · Yes, and start with a fresh context (its
    /// menu) · No, keep planning (the secondary; opens the feedback field).
    public static let planOptions: [PlanOption] = [
        PlanOption(key: "approve", label: "Yes", description: nil, decision: "approved", fresh: false),
        PlanOption(
            key: "approve-fresh", label: "Yes, and start with a fresh context", description: nil,
            decision: "approved", fresh: true
        ),
        PlanOption(
            key: "reject", label: "No, keep planning",
            description: "Your feedback goes back with it", decision: "rejected", fresh: false
        ),
    ]

    /// The plan option that opens the feedback field.
    public static let planRejectKey = "reject"

    // MARK: - Model

    /// The option a decided plan card names as its answer.
    public static func planDecisionLabel(decision: String, fresh: Bool) -> String {
        // FEED-140: a plan the agent replaced with a newer one.
        if decision == "superseded" { return Copy.superseded }
        let wantsFresh = decision == "approved" && fresh
        return planOptions.first { $0.decision == decision && $0.fresh == wantsFresh }?.label ?? ""
    }

    public static func label(_ kind: Kind) -> String {
        kind == .plan ? Copy.planReady : Copy.question
    }

    private static let blockMarker = try! NSRegularExpression(
        pattern: #"^(#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+|\[[ xX]\]\s+)"#
    )
    private static let link = try! NSRegularExpression(pattern: #"\[([^\]]*)\]\([^)]*\)"#)

    private static func stripMarker(_ line: String) -> String {
        let range = NSRange(line.startIndex..., in: line)
        return blockMarker.stringByReplacingMatches(in: line, range: range, withTemplate: "")
    }

    /// The first plain line of a markdown body: fenced code blocks skipped
    /// whole, block markers (heading, quote, list, task box) stripped, links
    /// reduced to their text, `**` / `__` / backticks dropped.
    public static func title(_ text: String) -> String {
        var fenced = false
        for raw in text.components(separatedBy: "\n") {
            var line = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            if line.hasPrefix("```") || line.hasPrefix("~~~") {
                fenced.toggle()
                continue
            }
            if fenced { continue }
            var stripped = stripMarker(line)
            while stripped != line {
                line = stripped
                stripped = stripMarker(line)
            }
            line = link.stringByReplacingMatches(
                in: line, range: NSRange(line.startIndex..., in: line), withTemplate: "$1"
            )
            line = line
                .replacingOccurrences(of: "**", with: "")
                .replacingOccurrences(of: "__", with: "")
                .replacingOccurrences(of: "`", with: "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !line.isEmpty { return line }
        }
        return ""
    }

    /// A thread's waiting cards, in Conversation order: every `plan` row with
    /// no decision and every unanswered `question` row, of ANY session in the
    /// tree. `sessionName` names the asking session (nil = unnamed).
    public static func threadPendingAsks(
        _ rows: [ThreadRow], sessionName: (String) -> String?
    ) -> [Pending] {
        var pending: [Pending] = []
        for (index, row) in rows.enumerated() {
            switch row {
            case let .plan(card) where card.decision == nil:
                pending.append(Pending(
                    id: card.id, kind: .plan, text: card.plan, sessionName: sessionName(card.sessionId),
                    createdAt: Double(index), questions: card.questions.count
                ))
            case let .question(card) where !card.answered:
                pending.append(Pending(
                    id: card.id, kind: .question, text: card.question, sessionName: sessionName(card.sessionId),
                    createdAt: Double(index)
                ))
            default:
                break
            }
        }
        return pending
    }

    // MARK: - EXP-1398: THE session view's cards ×4

    public enum SessionCardKind: String, Equatable, Sendable {
        case plan, question, surface

        /// The display row of the call that posts a card of this kind.
        public var postingCall: String {
            switch self {
            case .plan: "session_self_plan"
            case .question: "session_self_ask"
            case .surface: "session_self_show"
            }
        }
    }

    public struct SessionCard: Equatable, Sendable {
        /// The card's `messages.id`.
        public let id: String
        public let kind: SessionCardKind
        /// A plan with no decision / an unanswered question (Dock + answerable).
        public let waiting: Bool

        public init(id: String, kind: SessionCardKind, waiting: Bool) {
            self.id = id
            self.kind = kind
            self.waiting = waiting
        }
    }

    /// The session view's cards: its own conversation's `plan` / `question`
    /// cards a TOOL posted (no `questionId`) and its `surface` cards, oldest
    /// first (createdAt, then id). Each draws inline; a waiting one also docks.
    public static func sessionCards(_ messages: [TreeMessage]) -> [SessionCard] {
        var cards: [(card: SessionCard, at: Double)] = []
        for message in messages {
            guard let card = message.card else { continue }
            let engine = card.questionId != nil
            let at = SessionTree.ms(message.createdAt) ?? 0
            switch card.kind {
            case "plan" where !engine:
                cards.append((SessionCard(id: message.id, kind: .plan, waiting: card.decision == nil), at))
            case "question" where !engine:
                cards.append((SessionCard(id: message.id, kind: .question, waiting: message.answeredAt == nil), at))
            case "surface":
                cards.append((SessionCard(id: message.id, kind: .surface, waiting: false), at))
            default:
                break
            }
        }
        cards.sort { $0.at != $1.at ? $0.at < $1.at : $0.card.id < $1.card.id }
        return cards.map(\.card)
    }

    /// A feed call as `anchorSessionCards` reads it.
    public struct FeedCall: Equatable, Sendable {
        public let id: Int
        public let name: String?
        public let settled: Bool
        public let failed: Bool

        public init(id: Int, name: String?, settled: Bool, failed: Bool) {
            self.id = id
            self.name = name
            self.settled = settled
            self.failed = failed
        }
    }

    /// The feed's tool calls, as `anchorSessionCards` reads them.
    public static func feedCalls(_ feed: [AgentFeedItem]) -> [FeedCall] {
        feed.compactMap { item in
            guard case let .tool(id, name, _, _, _, _, settled, failed, _, _, _, _, _) = item else { return nil }
            return FeedCall(id: id, name: name, settled: settled, failed: failed)
        }
    }

    /// Where each session card draws: right after the SETTLED, unfailed call
    /// that posted it, paired newest back PER KIND; a card with no call left =
    /// `unanchored` (card order, the transcript's head). `anchors` keys = feed
    /// item ids, each list oldest first.
    public static func anchorSessionCards(
        _ calls: [FeedCall], cards: [SessionCard]
    ) -> (anchors: [Int: [String]], unanchored: [String]) {
        // EXP-1358: nothing to anchor = no walk over the whole feed.
        guard !cards.isEmpty else { return ([:], []) }
        var byRow: [String: [Int]] = [:]
        for call in calls where call.settled && !call.failed {
            guard let name = call.name, let display = ExpToolDisplay.resolve(toolName: name) else { continue }
            byRow[display.row, default: []].append(call.id)
        }
        var used: [String: Int] = [:]
        var anchors: [Int: [String]] = [:]
        var unanchored: [String] = []
        for card in cards.reversed() {
            let row = card.kind.postingCall
            let skip = used[row] ?? 0
            used[row] = skip + 1
            let ids = byRow[row] ?? []
            let index = ids.count - 1 - skip
            if index >= 0 {
                anchors[ids[index], default: []].insert(card.id, at: 0)
            } else {
                unanchored.insert(card.id, at: 0)
            }
        }
        return (anchors, unanchored)
    }

    /// A thread's card captions: each session TAB's name (nested ones
    /// included); the root (no tab) = nil.
    public static func threadSessionNames(_ tabs: [ThreadTab]) -> (String) -> String? {
        var names: [String: String] = [:]
        for tab in tabs {
            guard case let .session(sessionId, name, _, nested) = tab else { continue }
            names[sessionId] = name
            for child in nested { names[child.sessionId] = child.name }
        }
        return { names[$0] }
    }
}
