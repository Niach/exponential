import Foundation

// #FEED-237 (Danny: "like whatsapp"): a person's OWN message, one bubble at
// the bottom of the transcript from the press, its delivery mark (the clock,
// one check, two checks, the red mark) and its menu. Pure and locked by
// `packages/domain-contract/fixtures/message-delivery.json`, whose `$comment`
// holds the rules; mirrors: TS `src/message-delivery.ts`, desktop
// `domain::message_delivery`, Android `domain/MessageDelivery.kt`.
public enum MessageDelivery {
    /// genUI's `messageStatus`.
    public enum State: String, Equatable, Sendable, CaseIterable {
        case sending
        case sent
        case read
        case failed
    }

    /// An outbox entry's own state.
    public enum LocalState: String, Equatable, Sendable, Codable {
        case sending
        case failed
    }

    public enum Action: String, Equatable, Sendable {
        case retry
        case delete
        case cancel
    }

    public enum Tone: String, Equatable, Sendable {
        case muted
        case info
        case danger
    }

    /// Fixture `copy`.
    public enum Copy {
        public static func status(_ state: State) -> String {
            switch state {
            case .sending: return "Sending"
            case .sent: return "Sent"
            case .read: return "Read"
            case .failed: return "Not sent"
            }
        }

        public static func menu(_ action: Action) -> String {
            switch action {
            case .retry: return "Retry"
            case .delete: return "Delete"
            case .cancel: return "Cancel sending"
            }
        }

        /// The `sessions.unqueue` CONFLICT toast (the device consumed it).
        public static let cancelFailed = "Couldn't remove the message"
    }

    /// Fixture `marks`: the icon concept and the tone per state.
    public static func mark(_ state: State) -> (icon: String, tone: Tone) {
        switch state {
        case .sending: return ("ui-clock", .muted)
        case .sent: return ("ui-check", .muted)
        case .read: return ("ui-read", .info)
        case .failed: return ("ui-error", .danger)
        }
    }

    public static let markSize: Double = 14
    public static let matchWindowMs: Double = 60_000

    /// Fixture `testIds`.
    public enum TestIds {
        public static let status = "message-status"
        public static let menu = "message-menu"
    }

    public struct StateInput: Equatable, Sendable {
        /// The outbox entry's state; nil = a synced row alone.
        public let local: LocalState?
        /// The server holds the row (an entry's messageId, or a synced row).
        public let stored: Bool
        public let deliveredAt: String?
        public let consumedAt: String?
        /// The bubble is the live transcript's echo of the turn.
        public let echoed: Bool

        public init(local: LocalState?, stored: Bool, deliveredAt: String?, consumedAt: String?, echoed: Bool) {
            self.local = local
            self.stored = stored
            self.deliveredAt = deliveredAt
            self.consumedAt = consumedAt
            self.echoed = echoed
        }
    }

    /// First match wins: echoed or consumed = read, delivered = sent, an
    /// unstored failed entry = failed, anything else = the clock.
    public static func deliveryState(_ input: StateInput) -> State {
        if input.echoed || input.consumedAt != nil { return .read }
        if input.deliveredAt != nil { return .sent }
        if input.local == .failed && !input.stored { return .failed }
        return .sending
    }

    public struct OutboxEntry: Equatable, Sendable {
        /// A fresh UUID per press; also the `sessions.send` `clientMessageId`.
        public let clientId: String
        public let text: String
        public let createdAt: String
        public let state: LocalState
        /// The row the server stored it as (nil until the send answers).
        public let messageId: String?

        public init(clientId: String, text: String, createdAt: String, state: LocalState, messageId: String?) {
            self.clientId = clientId
            self.text = text
            self.createdAt = createdAt
            self.state = state
            self.messageId = messageId
        }
    }

    public struct OutboxRow: Equatable, Sendable {
        public let id: String
        public let authorKind: String
        public let authorId: String?
        public let body: String
        public let createdAt: String

        public init(id: String, authorKind: String, authorId: String?, body: String, createdAt: String) {
            self.id = id
            self.authorKind = authorKind
            self.authorId = authorId
            self.body = body
            self.createdAt = createdAt
        }
    }

    public struct Merge: Equatable, Sendable {
        public let drop: [String]
        public let keep: [String]

        public init(drop: [String], keep: [String]) {
            self.drop = drop
            self.keep = keep
        }
    }

    private static func millis(_ stamp: String) -> Double {
        (WireTimestamps.parse(stamp)?.timeIntervalSince1970 ?? 0) * 1000
    }

    /// Which outbox entries a synced row replaced (`drop`) and which still
    /// draw (`keep`, oldest first, ties by clientId).
    public static func outboxMerge(viewerId: String, outbox: [OutboxEntry], rows: [OutboxRow]) -> Merge {
        let entries = outbox.sorted { a, b in
            let at = millis(a.createdAt), bt = millis(b.createdAt)
            return at != bt ? at < bt : a.clientId < b.clientId
        }
        let rowIds = Set(rows.map(\.id))
        var matched = Set<String>()
        var dropped = Set<String>()
        // By id first: an acked entry's row is its row, whatever its text.
        for entry in entries {
            if let messageId = entry.messageId, rowIds.contains(messageId) {
                dropped.insert(entry.clientId)
                matched.insert(messageId)
            }
        }
        let own = rows
            .filter { $0.authorKind == "user" && $0.authorId == viewerId }
            .sorted { a, b in
                let at = millis(a.createdAt), bt = millis(b.createdAt)
                return at != bt ? at < bt : a.id < b.id
            }
        for entry in entries {
            if dropped.contains(entry.clientId) || entry.messageId != nil || entry.state == .failed { continue }
            let from = millis(entry.createdAt) - matchWindowMs
            guard let row = own.first(where: { candidate in
                !matched.contains(candidate.id) && candidate.body == entry.text && millis(candidate.createdAt) >= from
            }) else { continue }
            matched.insert(row.id)
            dropped.insert(entry.clientId)
        }
        return Merge(
            drop: entries.filter { dropped.contains($0.clientId) }.map(\.clientId),
            keep: entries.filter { !dropped.contains($0.clientId) }.map(\.clientId)
        )
    }

    /// The bubble's menu, in order (empty = no menu).
    public static func bubbleActions(state: State, stored: Bool) -> [Action] {
        switch state {
        case .failed: return [.retry, .delete]
        case .read: return []
        case .sending, .sent: return stored ? [.cancel] : []
        }
    }
}
