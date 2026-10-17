import Foundation

// EXP-1341: THE display title of any session ×4 (never `Session N`). A 1:1
// port of the TS reference `packages/domain-contract/src/session-title.ts`
// (desktop `domain::session_title`, Android `domain/SessionTitle.kt`), locked
// by fixtures/session-title.json (`SessionTitleTests`), whose `$comment`
// holds the rungs: the explicit name (`name`, else `title`) → the session's
// issues (a root's links, a child's ASSIGNED links) → `Run <action>` → the
// first prompt's usable line → `Untitled`.

public enum SessionTitle {
    /// fixtures/session-title.json `titles` + `titleMax` + `untitled`.
    public enum Copy {
        /// `{identifier}`, `{title}`.
        public static let issue = "#{identifier} {title}"
        /// `{identifier}`, `{title}`, `{more}`.
        public static let issues = "#{identifier} {title} + {more}"
        /// `{action}`.
        public static let action = "Run {action}"
        public static let titleMax = 80
        public static let untitled = "Untitled"
    }

    public struct Session: Equatable, Sendable {
        public let id: String
        public let rootId: String
        public let name: String?
        public let title: String?
        public let actionName: String?

        public init(id: String, rootId: String, name: String?, title: String?, actionName: String?) {
            self.id = id
            self.rootId = rootId
            self.name = name
            self.title = title
            self.actionName = actionName
        }

        public init(_ session: TreeSession) {
            self.init(
                id: session.id,
                rootId: session.rootId,
                name: session.name,
                title: session.title,
                actionName: session.actionName
            )
        }
    }

    public struct Message: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorKind: String
        public let authorSessionId: String?
        public let body: String
        public let hasCard: Bool
        public let createdAt: String

        public init(
            id: String,
            sessionId: String,
            authorKind: String,
            authorSessionId: String?,
            body: String,
            hasCard: Bool,
            createdAt: String
        ) {
            self.id = id
            self.sessionId = sessionId
            self.authorKind = authorKind
            self.authorSessionId = authorSessionId
            self.body = body
            self.hasCard = hasCard
            self.createdAt = createdAt
        }

        public init(_ message: TreeMessage) {
            self.init(
                id: message.id,
                sessionId: message.sessionId,
                authorKind: message.authorKind,
                authorSessionId: message.authorSessionId,
                body: message.body,
                hasCard: message.card != nil,
                createdAt: message.createdAt
            )
        }
    }

    /// A root's links, or the links ASSIGNED to a child, in link order
    /// (`sortOrder`, then `issueId`); unknown issues are skipped.
    public static func sessionIssues(
        id: String,
        rootId: String,
        links: [TreeSessionIssue],
        issues: [TreeIssue]
    ) -> [TreeIssue] {
        var byId: [String: TreeIssue] = [:]
        for issue in issues { byId[issue.id] = issue }
        let isRoot = id == rootId
        return links
            .filter { isRoot ? $0.sessionId == id : $0.assignedSessionId == id }
            .sorted { a, b in
                a.sortOrder != b.sortOrder ? a.sortOrder < b.sortOrder : a.issueId < b.issueId
            }
            .compactMap { byId[$0.issueId] }
    }

    /// The first prompt's usable line: a person's text or another session's
    /// input, oldest first.
    public static func firstPrompt(sessionId: String, messages: [Message]) -> String? {
        let inputs = messages
            .filter { message in
                guard message.sessionId == sessionId, !message.hasCard else { return false }
                if message.authorKind == "user" { return true }
                guard let author = message.authorSessionId, !author.isEmpty else { return false }
                return author != sessionId
            }
            .sorted { a, b in
                let ma = SessionTree.ms(a.createdAt) ?? 0, mb = SessionTree.ms(b.createdAt) ?? 0
                return ma != mb ? ma < mb : a.id < b.id
            }
        for message in inputs {
            if let line = firstLine(message.body) { return line }
        }
        return nil
    }

    /// THE display title (fixture `session-title.json` rungs).
    public static func title(
        _ session: Session,
        links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [Message]
    ) -> String {
        let name = trimmed(session.name)
        if !name.isEmpty { return name }
        let stored = trimmed(session.title)
        if !stored.isEmpty { return stored }
        let own = sessionIssues(id: session.id, rootId: session.rootId, links: links, issues: issues)
        if !own.isEmpty { return issueLabel(own) }
        let action = trimmed(session.actionName)
        if !action.isEmpty { return Copy.action.replacingOccurrences(of: "{action}", with: action) }
        if let prompt = firstPrompt(sessionId: session.id, messages: messages) {
            return clip(prompt, Copy.titleMax)
        }
        return Copy.untitled
    }

    /// Every given session's title, keyed by id (the tree's card names).
    public static func titles(
        _ sessions: [Session],
        links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [Message]
    ) -> [String: String] {
        var out: [String: String] = [:]
        for session in sessions {
            out[session.id] = title(session, links: links, issues: issues, messages: messages)
        }
        return out
    }

    // MARK: - Tree-row conveniences

    public static func title(
        _ session: TreeSession,
        sessionIssues links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage]
    ) -> String {
        title(Session(session), links: links, issues: issues, messages: messages.map(Message.init))
    }

    public static func titles(
        _ sessions: [TreeSession],
        sessionIssues links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage]
    ) -> [String: String] {
        titles(sessions.map(Session.init), links: links, issues: issues, messages: messages.map(Message.init))
    }

    // MARK: - Helpers

    private static func trimmed(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    /// Cut to `max` characters, ending in `…`.
    static func clip(_ text: String, _ max: Int) -> String {
        guard text.count > max else { return text }
        let head = String(text.prefix(Swift.max(0, max - 1)))
        let trimmedHead = head.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        return "\(trimmedHead)…"
    }

    /// The first usable line of a body: non-blank, not an image embed.
    private static func firstLine(_ body: String) -> String? {
        body
            .components(separatedBy: "\n")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty && !$0.hasPrefix("![") }
    }

    private static func issueLabel(_ issues: [TreeIssue]) -> String {
        let first = issues[0]
        let template = (issues.count > 1 ? Copy.issues : Copy.issue)
            .replacingOccurrences(of: "{identifier}", with: first.identifier)
            .replacingOccurrences(of: "{more}", with: String(issues.count - 1))
        let title = first.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let room = Copy.titleMax - template.replacingOccurrences(of: "{title}", with: "").count
        return template.replacingOccurrences(of: "{title}", with: clip(title, Swift.max(1, room)))
    }
}
