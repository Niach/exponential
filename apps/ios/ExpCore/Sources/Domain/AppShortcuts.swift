import Foundation

/// #EXP-1638 / #EXP-1830: the app icon's long-press menu, a 1:1 port of the
/// TS reference `packages/domain-contract/src/app-shortcuts.ts` (Android
/// `domain/AppShortcuts.kt`), locked by `fixtures/app-shortcuts.json`
/// (`AppShortcutsTests`). The fixture's `$comment` holds the rules.
public enum AppShortcuts {
    /// Fixture `copy`.
    public enum Copy {
        public static let newIssue = "New issue"
        public static let newChat = "New chat"
        public static let search = "Search"
    }

    /// Fixture `icons` (concepts, `packages/icons/icons.json`; `assistant` =
    /// THE brand mark).
    public enum Icons {
        public static let newIssue = "nav-create-issue"
        public static let newChat = "action-chat"
        public static let search = "nav-search"
        public static let chat = "action-chat"
        public static let assistant = "assistant"
        public static let agent = "bot"
    }

    /// Fixture `statics` (in order).
    public static let statics = ["new-issue", "new-chat", "search"]
    public static let recentChatPrefix = "recent-chat:"
    public static let agentPrefix = "agent:"
    public static let maxEntries = 4

    /// A chat root (the share-targets.json chat; `sealed` = a tree key or a
    /// sealed title).
    public typealias Chat = ShareTargets.Chat

    /// An agent the viewer may chat with (the synced definition).
    public struct Agent: Equatable, Sendable {
        public let id: String
        public let name: String
        /// A stored registry icon, nil = `Icons.agent`.
        public let icon: String?
        public let color: String?

        public init(id: String, name: String, icon: String?, color: String?) {
            self.id = id
            self.name = name
            self.icon = icon
            self.color = color
        }
    }

    /// The viewer's Assistant (agent-settings.json `assistant`).
    public struct Assistant: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    public struct Input: Equatable, Sendable {
        public let assistant: Assistant?
        public let agents: [Agent]
        /// The viewer's session pins, in sidebar order.
        public let pins: [String]
        public let chats: [Chat]

        public init(assistant: Assistant?, agents: [Agent], pins: [String], chats: [Chat]) {
            self.assistant = assistant
            self.agents = agents
            self.pins = pins
            self.chats = chats
        }
    }

    /// One item of the long-press menu.
    public struct Entry: Equatable, Sendable {
        public let id: String
        public let title: String
        /// A concept icon (`assistant` = the brand mark), or an agent's
        /// registry icon.
        public let icon: String
        public let color: String?

        public init(id: String, title: String, icon: String, color: String? = nil) {
            self.id = id
            self.title = title
            self.icon = icon
            self.color = color
        }
    }

    /// The destination a shortcut id names.
    public enum Target: Equatable, Sendable {
        case newIssue
        case newChat
        case search
        case chat(String)
        /// THE agent's conversation (the caller's DM; none yet = the empty DM).
        case agent(String)
    }

    private static let staticEntries: [String: Entry] = [
        "new-issue": Entry(id: "new-issue", title: Copy.newIssue, icon: Icons.newIssue),
        "new-chat": Entry(id: "new-chat", title: Copy.newChat, icon: Icons.newChat),
        "search": Entry(id: "search", title: Copy.search, icon: Icons.search),
    ]

    private static func assistantEntry(_ assistant: Assistant) -> Entry {
        Entry(id: agentPrefix + assistant.id, title: assistant.name, icon: Icons.assistant)
    }

    /// #EXP-1830: the chat entries: the Assistant, the session pins in pin
    /// order, then the recent chats newest first, deduplicated by id. An own
    /// agent conversation is its agent's entry; an unknown agent's and an
    /// encrypted plain chat are none.
    public static func chatEntries(_ input: Input) -> [Entry] {
        let agents = Dictionary(input.agents.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        func entry(_ chat: Chat) -> Entry? {
            if let agentId = chat.agentId {
                guard let agent = agents[agentId] else { return nil }
                if let assistant = input.assistant, agent.id == assistant.id { return assistantEntry(assistant) }
                return Entry(id: agentPrefix + agent.id, title: agent.name, icon: agent.icon ?? Icons.agent, color: agent.color)
            }
            if chat.sealed { return nil }
            return Entry(id: recentChatPrefix + chat.id, title: chat.title, icon: Icons.chat)
        }
        let byId = Dictionary(input.chats.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let pinned = input.pins.compactMap { byId[$0] }
        // Stable: equal times keep the input order.
        let recent = input.chats.enumerated()
            .sorted { lhs, rhs in
                lhs.element.updatedAt != rhs.element.updatedAt
                    ? lhs.element.updatedAt > rhs.element.updatedAt
                    : lhs.offset < rhs.offset
            }
            .map(\.element)
        var out: [Entry] = []
        var seen = Set<String>()
        func push(_ item: Entry?) {
            guard let item, seen.insert(item.id).inserted else { return }
            out.append(item)
        }
        if let assistant = input.assistant { push(assistantEntry(assistant)) }
        for chat in pinned { push(entry(chat)) }
        for chat in recent { push(entry(chat)) }
        return out
    }

    /// The statics, then the chat entries, capped at `maxEntries`.
    public static func appShortcuts(_ input: Input) -> [Entry] {
        let fixed = statics.compactMap { staticEntries[$0] }
        return Array((fixed + chatEntries(input)).prefix(maxEntries))
    }

    /// The destination a shortcut id names, nil = the normal start screen.
    public static func parseAppShortcut(_ id: String) -> Target? {
        switch id {
        case "new-issue": return .newIssue
        case "new-chat": return .newChat
        case "search": return .search
        default: break
        }
        if id.hasPrefix(agentPrefix) {
            let rest = String(id.dropFirst(agentPrefix.count))
            return rest.isEmpty ? nil : .agent(rest)
        }
        guard id.hasPrefix(recentChatPrefix) else { return nil }
        let rest = String(id.dropFirst(recentChatPrefix.count))
        return rest.isEmpty ? nil : .chat(rest)
    }
}
