import Foundation

// #EXP-1464: what a conversation's SESSION rows do when tapped ×4 (a
// thread's Conversation and a team agent's conversation alike). A session
// card whose session waits on a question or plan EXPANDS in place into that
// card; every other row naming a session (a session card, a `Report from`
// line, an event line such as `… was not picked up`) OPENS it by
// inbox-route.json `sessionTarget`. fixtures/thread-row-action.json; mirrors:
// TS `src/thread-row-action.ts`, desktop `domain::thread_row_action`, Android
// `domain/ThreadRowAction.kt`.
public enum ThreadRowAction {
    /// Fixture `copy`.
    public enum Copy {
        public static let openChat = "Open chat"
        public static let answered = "Answered"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let openChat = "thread-open-chat"
        public static let expanded = "thread-row-expanded"
        public static let event = "thread-event"
    }

    public enum Action: String, Equatable, Sendable {
        case expand
        case open
        case none
    }

    public struct Input: Equatable, Sendable {
        /// The Conversation row's type (`session`, `report`, `event`, …).
        public let type: String
        /// The session the row names (nil = none or not synced).
        public let sessionId: String?
        /// The conversation's own root (a row about it does nothing).
        public let rootId: String
        /// That session's card state (session-card.json), nil = unknown.
        public let state: SessionState?
        /// A watchdog or usage flag stands behind its `needs_you` (no card).
        public let blocked: Bool
        /// Its waiting card already is a row of this conversation.
        public let inline: Bool
        /// The viewer answered the card this row expanded (client-held).
        public let answered: Bool

        public init(
            type: String, sessionId: String?, rootId: String, state: SessionState?,
            blocked: Bool, inline: Bool, answered: Bool
        ) {
            self.type = type
            self.sessionId = sessionId
            self.rootId = rootId
            self.state = state
            self.blocked = blocked
            self.inline = inline
            self.answered = answered
        }
    }

    public struct Result: Equatable, Sendable {
        public let action: Action
        public let sessionId: String?
        /// Only a row that does something carries the chevron (and press).
        public let chevron: Bool
        /// Overrides the card's state word (`Copy.answered`), nil = its own.
        public let kindWord: String?
        /// Overrides the card's tone with the word, nil = its own.
        public let tone: String?

        public init(action: Action, sessionId: String?, chevron: Bool, kindWord: String?, tone: String?) {
            self.action = action
            self.sessionId = sessionId
            self.chevron = chevron
            self.kindWord = kindWord
            self.tone = tone
        }

        public static let none = Result(action: .none, sessionId: nil, chevron: false, kindWord: nil, tone: nil)
    }

    private static let openingTypes: Set<String> = ["session", "report", "event"]

    public static func threadRowAction(_ input: Input) -> Result {
        guard let sessionId = input.sessionId, !sessionId.isEmpty, sessionId != input.rootId,
              openingTypes.contains(input.type)
        else { return .none }
        if input.type == "session", input.state == .needsYou, !input.blocked, !input.inline {
            return Result(action: .expand, sessionId: sessionId, chevron: true, kindWord: nil, tone: nil)
        }
        let answered = input.type == "session" && input.answered
            && input.state != nil && input.state != .needsYou && input.state != .done
        return Result(
            action: .open,
            sessionId: sessionId,
            chevron: true,
            kindWord: answered ? Copy.answered : nil,
            tone: answered ? "muted" : nil
        )
    }

    /// Whether `sessionId`'s waiting card already is a row of `rows` (an
    /// unanswered question or an undecided plan of that session).
    public static func waitingCardInline(_ rows: [ThreadRow], sessionId: String) -> Bool {
        rows.contains { row in
            switch row {
            case let .question(card): card.sessionId == sessionId && !card.answered
            case let .plan(card): card.sessionId == sessionId && card.decision == nil
            default: false
            }
        }
    }

    /// The card an expanding row opens: the session's NEWEST (createdAt,
    /// ties by the higher id) waiting `question` (answeredAt nil) or `plan`
    /// (no decision; `superseded` is one) message id, else nil.
    public static func sessionWaitingCard(sessionId: String, messages: [TreeMessage]) -> String? {
        var best: (id: String, at: Date)?
        for message in messages where message.sessionId == sessionId {
            guard let card = message.card else { continue }
            let waiting: Bool = switch card.kind {
            case "question": message.answeredAt == nil
            case "plan": card.decision == nil
            default: false
            }
            guard waiting else { continue }
            let at = WireTimestamps.parse(message.createdAt) ?? .distantPast
            if let current = best, at < current.at || (at == current.at && message.id <= current.id) { continue }
            best = (message.id, at)
        }
        return best?.id
    }
}
