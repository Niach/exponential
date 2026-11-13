import Foundation

/// EXP-1635: sharing INTO Exponential, a 1:1 port of the TS reference
/// `packages/domain-contract/src/share-targets.ts` (Android
/// `domain/ShareTargets.kt`), locked by `fixtures/share-targets.json`
/// (`ShareTargetsTests`). The fixture's `$comment` holds the rules. Foundation
/// only: the share extension compiles it.
public enum ShareTargets {
    /// Fixture `copy`.
    public enum Copy {
        public static let title = "Share to"
        public static let newIssue = "New issue"
        public static let newChat = "New chat"
        public static let newChatSubtitle = "Temporary session"
        public static let agents = "Agents"
        public static let imagesOne = "1 image"
        public static let imagesMany = "{n} images"

        /// The preview line's label for `count` images.
        public static func images(_ count: Int) -> String {
            count == 1 ? imagesOne : imagesMany.replacingOccurrences(of: "{n}", with: "\(count)")
        }
    }

    /// Fixture `ids`.
    public enum Ids {
        public static let newIssue = "new-issue"
        public static let newChat = "new-chat"
    }

    /// Fixture `icons` (concepts, `packages/icons/icons.json`).
    public enum Icons {
        public static let newIssue = "nav-create-issue"
        public static let newChat = "ui-icon-placeholder"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let page = "share-targets"
        public static let preview = "share-targets-preview"
        public static func row(_ id: String) -> String { "share-target-\(id)" }
    }

    /// Fixture `directShare`.
    public enum DirectShare {
        public static let agentPrefix = "share-agent:"
        public static let chatPrefix = "share-chat:"
        public static let maxEntries = 4
    }

    /// A recent chat. `agentId` = the agent a conversation session belongs
    /// to: never a recent chat row.
    public struct Chat: Equatable, Sendable {
        public let id: String
        public let title: String
        public let updatedAt: String
        public let agentId: String?
        /// A tree key or a sealed title: never handed to the OS.
        public let sealed: Bool

        public init(id: String, title: String, updatedAt: String, agentId: String?, sealed: Bool = false) {
            self.id = id
            self.title = title
            self.updatedAt = updatedAt
            self.agentId = agentId
            self.sealed = sealed
        }
    }

    /// One row of the system share sheet.
    public struct Entry: Equatable, Sendable {
        public let id: String
        public let title: String

        public init(id: String, title: String) {
            self.id = id
            self.title = title
        }
    }

    /// The destination a direct-share id names.
    public enum Target: Equatable, Sendable {
        case agent(String)
        case chat(String)
    }

    /// The Share to page: the fixed rows, then the agents in agent row order.
    public static func shareTargets<Agent: AgentRoleOrdered>(
        agents: [Agent], me: String?
    ) -> (rows: [String], agents: [Agent]) {
        ([Ids.newIssue, Ids.newChat], AgentOrdering.orderedAgents(agents, me: me))
    }

    /// What a shared payload leaves in the composer.
    public static func shareChatText(text: String?, subject: String?, url: String?, hasImages: Bool) -> String {
        let text = trimmed(text)
        let url = trimmed(url)
        let body = !url.isEmpty && !text.contains(url)
            ? [text, url].filter { !$0.isEmpty }.joined(separator: "\n\n")
            : text
        let subject = trimmed(subject)
        if hasImages || subject.isEmpty || body.contains(subject) { return body }
        return [subject, body].filter { !$0.isEmpty }.joined(separator: "\n\n")
    }

    /// The system share sheet's rows: agents, then recent chats, capped.
    public static func directShareEntries<Agent: AgentRoleOrdered>(
        agents: [Agent], chats: [Chat], me: String?
    ) -> [Entry] {
        let agentRows = AgentOrdering.orderedAgents(agents, me: me).map {
            Entry(id: DirectShare.agentPrefix + $0.agentId, title: $0.agentName)
        }
        let chatRows = chats
            .filter { $0.agentId == nil && !$0.sealed }
            .enumerated()
            .sorted { lhs, rhs in
                lhs.element.updatedAt != rhs.element.updatedAt
                    ? lhs.element.updatedAt > rhs.element.updatedAt
                    : lhs.offset < rhs.offset
            }
            .map { Entry(id: DirectShare.chatPrefix + $0.element.id, title: $0.element.title) }
        return Array((agentRows + chatRows).prefix(DirectShare.maxEntries))
    }

    /// The destination a direct-share id names, nil = fall back to step 1.
    public static func parseDirectShareId(_ id: String) -> Target? {
        if id.hasPrefix(DirectShare.agentPrefix) {
            let rest = String(id.dropFirst(DirectShare.agentPrefix.count))
            return rest.isEmpty ? nil : .agent(rest)
        }
        if id.hasPrefix(DirectShare.chatPrefix) {
            let rest = String(id.dropFirst(DirectShare.chatPrefix.count))
            return rest.isEmpty ? nil : .chat(rest)
        }
        return nil
    }

    /// New issue's boards on the picked team (#EXP-1823), and the one it opens on.
    public static func shareIssueBoards<Board>(
        teamId: String?,
        boards: [Board],
        lastBoardId: String?,
        id: (Board) -> String,
        team: (Board) -> String
    ) -> (boards: [Board], board: Board?) {
        let own = teamId.map { teamId in boards.filter { team($0) == teamId } } ?? []
        return (own, own.first { id($0) == lastBoardId } ?? own.first)
    }

    private static func trimmed(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }
}
