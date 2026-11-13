import Foundation

/// #FEED-219: every composer's synced DRAFT. Pure and locked by
/// `packages/domain-contract/fixtures/composer-draft.json` (`keys`,
/// `unparsable`, `writes`, `fields`, `debounceMs`), mirrored from
/// `src/composer-draft.ts`; the fixture's `$comment` holds the rules.
public enum ComposerDraft {
    public enum Kind: String, CaseIterable, Equatable, Sendable {
        case comment
        case reply
        case session
        case agent
        case app
        case newChat
    }

    /// The target a composer writes to.
    public struct Target: Equatable, Hashable, Sendable {
        public let kind: Kind
        public let id: String

        public init(kind: Kind, id: String) {
            self.kind = kind
            self.id = id
        }

        public static func comment(_ issueId: String) -> Target { Target(kind: .comment, id: issueId) }
        public static func reply(_ topCommentId: String) -> Target { Target(kind: .reply, id: topCommentId) }
        public static func session(_ sessionId: String) -> Target { Target(kind: .session, id: sessionId) }
        public static func agent(_ agentId: String) -> Target { Target(kind: .agent, id: agentId) }
        public static func app(_ appId: String) -> Target { Target(kind: .app, id: appId) }
        public static func newChat(_ teamId: String) -> Target { Target(kind: .newChat, id: teamId) }

        public var key: String { "\(kind.rawValue):\(id)" }
    }

    /// The fixture's debounce after the last keystroke.
    public static let debounceMs = 800

    public static func key(_ target: Target) -> String { target.key }

    /// An unknown kind or a missing id = nil (a client ignores that row).
    public static func parse(_ key: String) -> Target? {
        guard let colon = key.firstIndex(of: ":"), colon != key.startIndex else { return nil }
        let id = String(key[key.index(after: colon)...])
        guard !id.isEmpty, let kind = Kind(rawValue: String(key[..<colon])) else { return nil }
        return Target(kind: kind, id: id)
    }

    public enum Write: Equatable, Sendable {
        case upsert(String)
        case delete
    }

    /// What writing the field's text does to the row: empty or whitespace
    /// only deletes it, anything else upserts it verbatim.
    public static func write(_ text: String) -> Write {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .delete : .upsert(text)
    }

    public struct FieldInput: Equatable, Sendable {
        /// The field's text right now.
        public var field: String
        /// The person edited the field since the composer opened.
        public var edited: Bool
        /// This client's own write, not yet echoed back by the shape.
        public var local: String?
        /// The synced row's text (nil = no row).
        public var synced: String?

        public init(field: String, edited: Bool, local: String?, synced: String?) {
            self.field = field
            self.edited = edited
            self.local = local
            self.synced = synced
        }
    }

    /// What the field shows.
    public static func field(_ input: FieldInput) -> String {
        if input.edited { return input.field }
        return input.local ?? input.synced ?? ""
    }
}
