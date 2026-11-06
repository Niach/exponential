import Foundation

/// #EXP-1642 (packages/domain-contract/fixtures/system-search.json, its
/// `$comment` is the spec; TS reference src/system-search.ts, Android
/// `domain/SystemSearch.kt`): issues and chats in the phone's own search.
/// The app's `SystemSearchIndexer` writes these items into CoreSpotlight, one
/// domain per signed-in account; opening a result routes `parse(id)` under the
/// domain's account. Never a description, comment or message; never an
/// encrypted chat.
public enum SystemSearch {
    public static let issueSubtitle = "{identifier} · {board}"
    public static let chatSubtitle = "Chat"
    public static let issuePrefix = "issue:"
    public static let chatPrefix = "chat:"
    public static let domainPrefix = "account:"
    public static let debounceMs = 2000

    public struct Caps: Equatable, Sendable {
        public let maxIssues: Int
        public let maxChats: Int

        public init(maxIssues: Int, maxChats: Int) {
            self.maxIssues = maxIssues
            self.maxChats = maxChats
        }
    }

    /// The fixture's tiny caps (the replay).
    public static let fixtureCaps = Caps(maxIssues: 2, maxChats: 2)
    /// What the app indexes.
    public static let productionCaps = Caps(maxIssues: 1000, maxChats: 200)

    public struct Board: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    public struct Issue: Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        public let boardId: String
        public let updatedAt: String
        public let trashed: Bool
        public let archived: Bool

        public init(
            id: String, identifier: String, title: String, boardId: String, updatedAt: String,
            trashed: Bool = false, archived: Bool = false
        ) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.boardId = boardId
            self.updatedAt = updatedAt
            self.trashed = trashed
            self.archived = archived
        }
    }

    public struct Item: Equatable, Sendable {
        public let id: String
        public let title: String
        public let subtitle: String
        public let keywords: [String]

        public init(id: String, title: String, subtitle: String, keywords: [String]) {
            self.id = id
            self.title = title
            self.subtitle = subtitle
            self.keywords = keywords
        }
    }

    public enum Target: Equatable, Sendable {
        case issue(String)
        case chat(String)
    }

    /// One account's domain (a sign-out deletes it and nothing else).
    public static func domain(accountId: String) -> String { domainPrefix + accountId }

    /// The account a domain names; nil = not one of ours.
    public static func accountId(domain: String) -> String? {
        guard domain.hasPrefix(domainPrefix) else { return nil }
        let rest = String(domain.dropFirst(domainPrefix.count))
        return rest.isEmpty ? nil : rest
    }

    /// The index of one account: live issues, then plain chats, each capped.
    public static func items(
        boards: [Board], issues: [Issue], chats: [ShareTargets.Chat], caps: Caps = fixtureCaps
    ) -> [Item] {
        let names = Dictionary(boards.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        let issueItems = newestFirst(issues.filter { !$0.trashed && !$0.archived }, \.updatedAt)
            .prefix(caps.maxIssues)
            .map { issue -> Item in
                let board = names[issue.boardId]
                return Item(
                    id: issuePrefix + issue.id,
                    title: issue.title,
                    subtitle: board.map {
                        issueSubtitle.replacingOccurrences(of: "{identifier}", with: issue.identifier)
                            .replacingOccurrences(of: "{board}", with: $0)
                    } ?? issue.identifier,
                    keywords: board.map { [issue.identifier, $0] } ?? [issue.identifier]
                )
            }
        let chatItems = newestFirst(chats.filter { ($0.agentId ?? "").isEmpty && !$0.sealed }, \.updatedAt)
            .prefix(caps.maxChats)
            .map { Item(id: chatPrefix + $0.id, title: $0.title, subtitle: chatSubtitle, keywords: []) }
        return Array(issueItems) + Array(chatItems)
    }

    /// The destination a result id names; nil = the normal start screen.
    public static func parse(_ id: String) -> Target? {
        if id.hasPrefix(issuePrefix) {
            let rest = String(id.dropFirst(issuePrefix.count))
            return rest.isEmpty ? nil : .issue(rest)
        }
        if id.hasPrefix(chatPrefix) {
            let rest = String(id.dropFirst(chatPrefix.count))
            return rest.isEmpty ? nil : .chat(rest)
        }
        return nil
    }

    /// A stable newest-first sort (the reference's `Array.prototype.sort`).
    private static func newestFirst<T>(_ rows: [T], _ key: KeyPath<T, String>) -> [T] {
        rows.enumerated().sorted { a, b in
            let lhs = a.element[keyPath: key], rhs = b.element[keyPath: key]
            return lhs != rhs ? lhs > rhs : a.offset < b.offset
        }.map(\.element)
    }
}
