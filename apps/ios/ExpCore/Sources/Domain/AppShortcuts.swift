import Foundation

/// #EXP-1638: the app icon's long-press menu, a 1:1 port of the TS reference
/// `packages/domain-contract/src/app-shortcuts.ts` (Android
/// `domain/AppShortcuts.kt`), locked by `fixtures/app-shortcuts.json`
/// (`AppShortcutsTests`). The fixture's `$comment` holds the rules.
public enum AppShortcuts {
    /// Fixture `copy`.
    public enum Copy {
        public static let newIssue = "New issue"
        public static let newChat = "New chat"
        public static let search = "Search"
    }

    /// Fixture `icons` (concepts, `packages/icons/icons.json`).
    public enum Icons {
        public static let newIssue = "nav-create-issue"
        public static let newChat = "action-chat"
        public static let search = "nav-search"
        public static let chat = "action-chat"
    }

    /// Fixture `statics` (in order).
    public static let statics = ["new-issue", "new-chat", "search"]
    public static let recentChatPrefix = "recent-chat:"
    public static let maxEntries = 4

    /// A recent chat (the share-targets.json direct-share filter's input).
    public typealias Chat = ShareTargets.Chat

    /// One item of the long-press menu.
    public struct Entry: Equatable, Sendable {
        public let id: String
        public let title: String
        /// A concept icon.
        public let icon: String

        public init(id: String, title: String, icon: String) {
            self.id = id
            self.title = title
            self.icon = icon
        }
    }

    /// The destination a shortcut id names.
    public enum Target: Equatable, Sendable {
        case newIssue
        case newChat
        case search
        case chat(String)
    }

    private static let staticEntries: [String: Entry] = [
        "new-issue": Entry(id: "new-issue", title: Copy.newIssue, icon: Icons.newIssue),
        "new-chat": Entry(id: "new-chat", title: Copy.newChat, icon: Icons.newChat),
        "search": Entry(id: "search", title: Copy.search, icon: Icons.search),
    ]

    /// The statics, then the most recent plain chats (no agent conversation,
    /// nothing encrypted), newest first, capped at `maxEntries`.
    public static func appShortcuts(chats: [Chat]) -> [Entry] {
        let fixed = statics.compactMap { staticEntries[$0] }
        let recent = chats
            .filter { $0.agentId == nil && !$0.sealed }
            .enumerated()
            .sorted { lhs, rhs in
                lhs.element.updatedAt != rhs.element.updatedAt
                    ? lhs.element.updatedAt > rhs.element.updatedAt
                    : lhs.offset < rhs.offset
            }
            .map { Entry(id: recentChatPrefix + $0.element.id, title: $0.element.title, icon: Icons.chat) }
        return Array((fixed + recent).prefix(maxEntries))
    }

    /// The destination a shortcut id names, nil = the normal start screen.
    public static func parseAppShortcut(_ id: String) -> Target? {
        switch id {
        case "new-issue": return .newIssue
        case "new-chat": return .newChat
        case "search": return .search
        default: break
        }
        guard id.hasPrefix(recentChatPrefix) else { return nil }
        let rest = String(id.dropFirst(recentChatPrefix.count))
        return rest.isEmpty ? nil : .chat(rest)
    }
}
