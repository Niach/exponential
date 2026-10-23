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

    /// EXP-1280: the Exponential tools that post a card (NEW-4: the display
    /// rows of `session_self` op plan / op ask; web `CARD_TOOLS`).
    public static let cardTools: Set<String> = ["session_self_plan", "session_self_ask"]

    /// Where the session view paints each tool-posted card (web
    /// `anchorToolCards`): right after the settled call that posted it. Calls
    /// and cards pair from the NEWEST back (the transcript may start
    /// mid-history; a failed call posted nothing); a card with no call on
    /// screen = `unanchored` (it predates the window). `cardIds` oldest first;
    /// `anchors` keys = feed item ids, each list oldest first.
    public static func anchorToolCards(
        feed: [AgentFeedItem], cardIds: [String]
    ) -> (anchors: [Int: [String]], unanchored: [String]) {
        // EXP-1358: nothing to anchor = no walk over the whole feed (the
        // session body called this on every render, cards or not).
        guard !cardIds.isEmpty else { return ([:], []) }
        let calls: [Int] = feed.compactMap { item in
            guard case let .tool(id, name, _, _, _, _, settled, failed, _, _, _, _, _) = item,
                  settled, !failed,
                  let display = ExpToolDisplay.resolve(toolName: name),
                  cardTools.contains(display.row)
            else { return nil }
            return id
        }
        var anchors: [Int: [String]] = [:]
        var unanchored: [String] = []
        for k in 0..<cardIds.count {
            let card = cardIds[cardIds.count - 1 - k]
            let callIndex = calls.count - 1 - k
            guard callIndex >= 0 else {
                unanchored.insert(card, at: 0)
                continue
            }
            anchors[calls[callIndex], default: []].insert(card, at: 0)
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
