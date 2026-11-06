import Foundation

/// #EXP-1640 (packages/domain-contract/fixtures/app-intents.json, its
/// `$comment` is the spec; TS reference src/app-intents.ts, Android
/// `domain/AppIntents.kt`): Siri, Shortcuts, the Action button and Apple
/// Intelligence. The app's `CreateIssueIntent` and `AskAgentIntent` run these
/// on the ACTIVE account through the app's own create and send paths.
public enum AppIntentsModel {
    public enum Copy {
        /// `{identifier}`.
        public static let created = "Created {identifier}"
        public static let askTitle = "What's the issue?"
        public static let noBoard = "Open Exponential and create a board first"
        /// `{name}`.
        public static let noAgent = "No agent called {name}"
        /// `{agent}`.
        public static let sent = "Sent to {agent}"
        public static let signedOut = "Sign in to Exponential first"
        public static let createIssueTitle = "Create issue"
        public static let askAgentTitle = "Ask agent"

        public static func created(identifier: String) -> String {
            created.replacingOccurrences(of: "{identifier}", with: identifier)
        }

        public static func noAgent(name: String) -> String {
            noAgent.replacingOccurrences(of: "{name}", with: name)
        }

        public static func sent(agent: String) -> String {
            sent.replacingOccurrences(of: "{agent}", with: agent)
        }
    }

    /// The App Shortcut phrases, `${applicationName}` / `${agent}` as the
    /// fixture writes them (the provider spells them as `\(.applicationName)`).
    public enum Phrases {
        public static let createIssue = ["Create an issue in ${applicationName}", "New ${applicationName} issue"]
        public static let askAgent = ["Ask ${agent} in ${applicationName}", "Ask ${applicationName}"]
    }

    public static let titleMax = 256

    /// A spoken or typed issue title; nil = ask again.
    public static func issueTitle(_ text: String) -> String? {
        let title = text.split(whereSeparator: { $0.isWhitespace || $0.isNewline })
            .joined(separator: " ")
        guard !title.isEmpty else { return nil }
        // Code points, like the reference's Array.from.
        return String(String.UnicodeScalarView(title.unicodeScalars.prefix(titleMax)))
    }

    public struct Board: Equatable, Sendable {
        public let id: String
        public let sortOrder: Double

        public init(id: String, sortOrder: Double) {
            self.id = id
            self.sortOrder = sortOrder
        }
    }

    /// The board a new issue lands on; nil = no board.
    public static func board(boards: [Board], lastBoardId: String?, pickedId: String?) -> String? {
        let ids = Set(boards.map(\.id))
        if let pickedId, ids.contains(pickedId) { return pickedId }
        if let lastBoardId, ids.contains(lastBoardId) { return lastBoardId }
        return boards.enumerated()
            .min { a, b in a.element.sortOrder != b.element.sortOrder ? a.element.sortOrder < b.element.sortOrder : a.offset < b.offset }?
            .element.id
    }

    public struct Agent: AgentRoleOrdered, Equatable, Sendable {
        public let id: String
        public let name: String
        public let sortOrder: Double
        public let role: String?
        public let ownerId: String?

        public init(id: String, name: String, sortOrder: Double, role: String?, ownerId: String?) {
            self.id = id
            self.name = name
            self.sortOrder = sortOrder
            self.role = role
            self.ownerId = ownerId
        }

        public var agentId: String { id }
        public var agentName: String { name }
        public var agentRole: String? { role }
        public var agentOwnerId: String? { ownerId }
        public var agentSortOrder: Double { sortOrder }
    }

    /// The agent a spoken name means: exact, then prefix, in agent order;
    /// none spoken = the first (the member's Assistant).
    public static func matchAgent<T: AgentRoleOrdered>(agents: [T], me: String?, spoken: String) -> String? {
        let ordered = AgentOrdering.orderedAgents(agents, me: me)
        let wanted = spoken.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !wanted.isEmpty else { return ordered.first?.agentId }
        if let exact = ordered.first(where: { $0.agentName.lowercased() == wanted }) { return exact.agentId }
        return ordered.first { $0.agentName.lowercased().hasPrefix(wanted) }?.agentId
    }
}
