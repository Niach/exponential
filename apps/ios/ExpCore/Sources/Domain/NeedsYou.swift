import Foundation

/// #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/fixtures/needs-you.json`,
/// TS reference `src/needs-you.ts`, desktop `domain::needs_you`, Android
/// `domain/NeedsYou.kt`): every plan and question waiting on the viewer.
/// `NeedsYouTests` replays every fixture case.
public enum NeedsYou {
    // MARK: - Fixture constants

    public static let route = "/t/$teamSlug/needs-you"

    public enum Copy {
        public static let entry = "Needs you"
        public static let empty = "Nothing needs you"
    }

    public enum Prefix {
        public static let question = "Question: "
    }

    public enum TestIds {
        public static let entry = "nav-needs-you"
        public static let row = "needs-you-row"
        public static let list = "needs-you"
    }

    // MARK: - Input

    public struct SessionInput: Equatable, Sendable {
        public let id: String
        public let rootId: String
        /// The session's display name (session-title; an agent conversation = the agent's).
        public let name: String
        public let status: String
        public let agentId: String?
        public let userId: String?

        public init(id: String, rootId: String, name: String, status: String, agentId: String?, userId: String?) {
            self.id = id
            self.rootId = rootId
            self.name = name
            self.status = status
            self.agentId = agentId
            self.userId = userId
        }
    }

    public struct MessageInput: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorSessionId: String?
        public let body: String
        /// Wire timestamps, as the digests carry them.
        public let createdAt: String
        public let answeredAt: String?
        public let card: TreeMessageCard?

        public init(
            id: String, sessionId: String, authorSessionId: String?, body: String, createdAt: String,
            answeredAt: String?, card: TreeMessageCard?
        ) {
            self.id = id
            self.sessionId = sessionId
            self.authorSessionId = authorSessionId
            self.body = body
            self.createdAt = createdAt
            self.answeredAt = answeredAt
            self.card = card
        }
    }

    // MARK: - Output

    /// `plan` = the chat's Plan tab; `card` = its card in the Conversation.
    public enum Opens: String, Equatable, Sendable {
        case plan, card
    }

    public struct Row: Equatable, Sendable, Identifiable {
        /// The waiting card's message: the origin's for a mirror it may read.
        public let messageId: String
        public let sessionId: String
        public let rootId: String
        public let kind: AskDock.Kind
        public let opens: Opens
        public let title: String
        public let chat: String
        public let meta: String
        public let age: String
        public let createdAt: String
        public var id: String { messageId }

        public init(
            messageId: String, sessionId: String, rootId: String, kind: AskDock.Kind, opens: Opens, title: String,
            chat: String, meta: String, age: String, createdAt: String
        ) {
            self.messageId = messageId
            self.sessionId = sessionId
            self.rootId = rootId
            self.kind = kind
            self.opens = opens
            self.title = title
            self.chat = chat
            self.meta = meta
            self.age = age
            self.createdAt = createdAt
        }
    }

    // MARK: - The fold

    private static func ms(_ wire: String) -> Double { SessionTree.ms(wire) ?? 0 }

    private static func rowOf(_ message: MessageInput, sessions: [String: SessionInput], now: Double) -> Row? {
        guard let card = message.card, let session = sessions[message.sessionId],
              let root = sessions[session.rootId]
        else { return nil }
        let age = ResultsTab.age(ms(message.createdAt), now: now)
        if card.kind == "plan" || card.kind == "question" {
            guard WaitingCards.cardState(card, answeredAt: message.answeredAt) == .waiting else { return nil }
            let plan = card.kind == "plan"
            let question = AskDock.title(card.question ?? "")
            return Row(
                messageId: message.id,
                sessionId: session.id,
                rootId: root.id,
                kind: plan ? .plan : .question,
                opens: plan ? .plan : .card,
                title: plan ? PlanPage.planRowLabel(card.plan ?? "") : question.isEmpty ? AskDock.label(.question) : question,
                chat: root.name,
                meta: plan ? PlanPage.countsLine(PlanPage.counts(card)) : "",
                age: age,
                createdAt: message.createdAt
            )
        }
        guard card.kind == "ask", message.answeredAt == nil else { return nil }
        // A mirror: the origin's own ids where the viewer may read it, else the
        // mirror's, with the neutral copy (chats.json `askPreview`).
        let originSession = (card.sessionId ?? message.authorSessionId).flatMap { sessions[$0] }
        let preview = Chats.askPreview(
            TreeMessage(
                id: message.id, sessionId: message.sessionId, authorKind: "agent", authorId: nil, body: message.body,
                card: card, answeredAt: message.answeredAt, consumedAt: nil, createdAt: message.createdAt,
                authorSessionId: message.authorSessionId
            ),
            visible: Set(sessions.keys)
        )
        let plan = preview.kind == .plan
        let title = preview.text.hasPrefix(Prefix.question)
            ? String(preview.text.dropFirst(Prefix.question.count))
            : preview.text
        if let originSession, let messageId = card.messageId {
            return Row(
                messageId: messageId,
                sessionId: originSession.id,
                rootId: originSession.rootId,
                kind: plan ? .plan : .question,
                opens: plan ? .plan : .card,
                title: title,
                chat: sessions[originSession.rootId]?.name ?? root.name,
                meta: "",
                age: age,
                createdAt: message.createdAt
            )
        }
        return Row(
            messageId: message.id,
            sessionId: session.id,
            rootId: root.id,
            kind: plan ? .plan : .question,
            opens: .card,
            title: title,
            chat: root.name,
            meta: "",
            age: age,
            createdAt: message.createdAt
        )
    }

    /// THE fold: one row per waiting card, once, newest first.
    public static func rows(
        now: Double, me: String?, sessions: [SessionInput], messages: [MessageInput]
    ) -> [Row] {
        // Another member's agent conversation is theirs (chats.json `listAgents`).
        let live = sessions.filter { $0.status != "ended" && ($0.agentId == nil || $0.userId == me) }
        let sessionsById = Dictionary(live.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        var rows: [String: Row] = [:]
        var seen = Set<String>()
        for message in messages where seen.insert(message.id).inserted {
            guard let row = rowOf(message, sessions: sessionsById, now: now) else { continue }
            // The card itself beats its mirror's copy of it.
            if message.id == row.messageId || rows[row.messageId] == nil { rows[row.messageId] = row }
        }
        return rows.values.sorted { a, b in
            let at = ms(a.createdAt), bt = ms(b.createdAt)
            return at != bt ? at > bt : a.messageId > b.messageId
        }
    }
}
