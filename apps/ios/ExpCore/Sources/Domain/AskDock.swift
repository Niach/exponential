import Foundation

/// THE ask dock ×4 (`packages/domain-contract/fixtures/ask-dock.json`, TS
/// reference `src/ask-dock.ts`): the question/plan cards waiting on the
/// viewer take the composer's place, in the session view AND the thread.
/// One pending card = the big card (the composer hides); several = compact
/// cards stacked in the composer's slot, a tap grows one into the big card
/// (only that one answers), a back arrow returns to the stack. The plan
/// card's options are ONE list ×4 (`planOptions`). `AskDockTests` replays
/// every fixture case and pins the constants below to the fixture.
public enum AskDock {
    public enum Kind: String, Equatable, Sendable {
        case plan, question
    }

    public enum Mode: String, Equatable, Sendable {
        case none, single, stack, focused
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

        public init(id: String, kind: Kind, text: String, sessionName: String?, createdAt: Double) {
            self.id = id
            self.kind = kind
            self.text = text
            self.sessionName = sessionName
            self.createdAt = createdAt
        }
    }

    /// One compact card of the stack.
    public struct Row: Equatable, Sendable, Identifiable {
        public let id: String
        public let kind: Kind
        /// The header label ("Plan ready" / "Question").
        public let label: String
        /// The first plain line of the text (the row truncates it).
        public let title: String
        /// The asking session's name, else nil.
        public let caption: String?

        public init(id: String, kind: Kind, label: String, title: String, caption: String?) {
            self.id = id
            self.kind = kind
            self.label = label
            self.title = title
            self.caption = caption
        }
    }

    public struct Layout: Equatable, Sendable {
        public let mode: Mode
        /// Whether the composer yields its slot to the dock.
        public let composerHidden: Bool
        /// The back arrow (only a focused card out of a stack).
        public let back: Bool
        /// The big card's id (`single` | `focused`), else nil.
        public let cardId: String?
        /// The compact cards (`stack` only), oldest first.
        public let rows: [Row]

        public init(mode: Mode, composerHidden: Bool, back: Bool, cardId: String?, rows: [Row]) {
            self.mode = mode
            self.composerHidden = composerHidden
            self.back = back
            self.cardId = cardId
            self.rows = rows
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
    }

    public enum TestIds {
        public static let dock = "ask-dock"
        public static let row = "ask-dock-row"
        public static let back = "ask-dock-back"
        public static let card = "ask-dock-card"
    }

    /// The big card's body cap, as a fraction of the viewport height.
    public static let bodyMaxHeightFraction: Double = 0.4
    /// The gap between stacked compact cards (pt).
    public static let stackGap: Double = 6

    /// THE plan card's options ×4 (the claude adapter's ExitPlanMode menu):
    /// Yes (primary) · Yes, and start with a fresh context · No, keep
    /// planning (opens the feedback field).
    public static let planOptions: [PlanOption] = [
        PlanOption(key: "approve", label: "Yes", description: nil, decision: "approved", fresh: false),
        PlanOption(
            key: "approve-fresh", label: "Yes, and start with a fresh context", description: nil,
            decision: "approved", fresh: true
        ),
        PlanOption(
            key: "reject", label: "No, keep planning",
            description: "Sends your next message back to planning", decision: "rejected", fresh: false
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

    public static func dock(pending input: [Pending], focusedId: String?) -> Layout {
        let pending = input.sorted { a, b in
            a.createdAt != b.createdAt ? a.createdAt < b.createdAt : a.id < b.id
        }
        guard let first = pending.first else {
            return Layout(mode: .none, composerHidden: false, back: false, cardId: nil, rows: [])
        }
        if pending.count == 1 {
            return Layout(mode: .single, composerHidden: true, back: false, cardId: first.id, rows: [])
        }
        if let focused = pending.first(where: { $0.id == focusedId }) {
            return Layout(mode: .focused, composerHidden: true, back: true, cardId: focused.id, rows: [])
        }
        return Layout(
            mode: .stack,
            composerHidden: true,
            back: false,
            cardId: nil,
            rows: pending.map { p in
                let title = title(p.text)
                return Row(
                    id: p.id, kind: p.kind, label: label(p.kind),
                    title: title.isEmpty ? label(p.kind) : title,
                    caption: p.sessionName
                )
            }
        )
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
                    createdAt: Double(index)
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
