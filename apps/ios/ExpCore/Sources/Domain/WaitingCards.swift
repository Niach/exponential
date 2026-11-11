import Foundation

/// #EXP-1561 / #EXP-1562: WAITING question and plan cards ×4
/// (`packages/domain-contract/fixtures/waiting-cards.json`, TS reference
/// `src/waiting-cards.ts`): a card's state (waiting / answered / dismissed /
/// superseded) and the chat header's waiting chips. `WaitingCardsTests`
/// replays `stateCases`, `chipCases` and `targetCases`, comparing the WHOLE value.
public enum WaitingCards {
    public enum State: String, Equatable, Sendable {
        case waiting, answered, dismissed, superseded
    }

    public enum Layout: String, Equatable, Sendable {
        case hidden, chips, button
    }

    public enum Copy {
        public static let dismiss = "Dismiss"
        public static let dismissed = "Dismissed"
        public static let waiting = "Waiting on you"
        public static let waitingCount = "{count} waiting"
    }

    public enum Icons {
        public static let plan = "coding-plan"
        public static let question = "ui-help"
        public static let dismiss = "ui-close"
        public static let waiting = "ui-help"

        public static func of(_ kind: AskDock.Kind) -> String {
            kind == .plan ? plan : question
        }
    }

    public enum Constants {
        public static let labelMax = 24
        public static let ellipsis = "\u{2026}"
        public static let highlightMs = 1600
    }

    public enum TestIds {
        public static let dismiss = "card-dismiss"
        public static let dismissed = "card-dismissed"
        public static let chip = "waiting-chip"
        public static let button = "waiting-button"
        public static let menu = "waiting-menu"
    }

    /// One header chip.
    public struct Chip: Equatable, Sendable, Identifiable {
        public let id: String
        public let kind: AskDock.Kind
        /// The kind's concept icon.
        public let icon: String
        public let label: String

        public init(id: String, kind: AskDock.Kind, icon: String, label: String) {
            self.id = id
            self.kind = kind
            self.icon = icon
            self.label = label
        }
    }

    public struct Chips: Equatable, Sendable {
        public let layout: Layout
        public let count: Int
        public let chips: [Chip]

        public init(layout: Layout, count: Int, chips: [Chip]) {
            self.layout = layout
            self.count = count
            self.chips = chips
        }
    }

    /// A question/plan card's state; nil = not a question or plan.
    public static func cardState(_ card: TreeMessageCard?, answeredAt: String?) -> State? {
        guard let card, card.kind == "plan" || card.kind == "question" else { return nil }
        if card.dismissed || card.decision == "dismissed" { return .dismissed }
        if card.kind == "plan" {
            if card.decision == "superseded" { return .superseded }
            return card.decision != nil ? .answered : .waiting
        }
        return answeredAt != nil || card.answer != nil ? .answered : .waiting
    }

    /// A title clipped to `labelMax` characters (longer = the first
    /// labelMax − 1, trailing spaces trimmed, + the ellipsis).
    public static func clipLabel(_ title: String) -> String {
        let scalars = Array(title.unicodeScalars)
        guard scalars.count > Constants.labelMax else { return title }
        var head = String(String.UnicodeScalarView(scalars.prefix(Constants.labelMax - 1)))
        while head.last?.isWhitespace == true { head.removeLast() }
        return head + Constants.ellipsis
    }

    /// The header's waiting chips over the WAITING cards in Conversation
    /// order: hidden when none waits, ONE button on phones, else one chip
    /// per card.
    public static func chips(_ cards: [AskDock.Pending], phone: Bool) -> Chips {
        let chips = cards.map { card in
            let title = clipLabel(AskDock.title(card.text))
            return Chip(
                id: card.id, kind: card.kind, icon: Icons.of(card.kind),
                label: title.isEmpty ? AskDock.label(card.kind) : title
            )
        }
        let layout: Layout = chips.isEmpty ? .hidden : phone ? .button : .chips
        return Chips(layout: layout, count: chips.count, chips: chips)
    }

    /// #EXP-1785: what a picked chip (or menu row) opens.
    public enum Open: String, Equatable, Sendable {
        case plan, card, session
    }

    public struct Target: Equatable, Sendable {
        public let open: Open
        public let sessionId: String

        public init(open: Open, sessionId: String) {
            self.open = open
            self.sessionId = sessionId
        }
    }

    /// A picked card: its kind and its asking session.
    public struct TargetCard: Equatable, Sendable {
        public let kind: AskDock.Kind
        public let sessionId: String

        public init(kind: AskDock.Kind, sessionId: String) {
            self.kind = kind
            self.sessionId = sessionId
        }
    }

    /// #EXP-1785: where a pick goes: a plan = the plan page; the root's
    /// question = the Chat tab at its card; a question of a session under
    /// the root = that session opened in place at its card.
    public static func waitingTarget(card: TargetCard, rootId: String) -> Target {
        if card.kind == .plan { return Target(open: .plan, sessionId: card.sessionId) }
        return Target(open: card.sessionId == rootId ? .card : .session, sessionId: card.sessionId)
    }

    /// The phone button's accessible name.
    public static func waitingCountLabel(_ count: Int) -> String {
        Copy.waitingCount.replacingOccurrences(of: "{count}", with: String(count))
    }
}
