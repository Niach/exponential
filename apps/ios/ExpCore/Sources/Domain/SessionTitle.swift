import Foundation

// EXP-1341: THE display title of any session ×4 (never `Session N`). A 1:1
// port of the TS reference `packages/domain-contract/src/session-title.ts`
// (desktop `domain::session_title`, Android `domain/SessionTitle.kt`), locked
// by fixtures/session-title.json (`SessionTitleTests`), whose `$comment`
// holds the rungs: the explicit `title` → the session's issues (the
// session's OWN rows, root or child alike, NEW-1) → the first prompt's usable
// line → `Untitled`.

public enum SessionTitle {
    /// fixtures/session-title.json `titles` + `titleMax` + `untitled`.
    public enum Copy {
        /// `{identifier}`, `{title}`.
        public static let issue = "#{identifier} {title}"
        /// `{identifier}`, `{title}`, `{more}`.
        public static let issues = "#{identifier} {title} + {more}"
        public static let titleMax = 80
        public static let untitled = "Untitled"
    }

    public struct Session: Equatable, Sendable {
        public let id: String
        public let rootId: String
        public let title: String?

        public init(id: String, rootId: String, title: String?) {
            self.id = id
            self.rootId = rootId
            self.title = title
        }

        public init(_ session: TreeSession) {
            self.init(
                id: session.id,
                rootId: session.rootId,
                title: session.title
            )
        }
    }

    public struct Message: Equatable, Sendable {
        public let id: String
        /// NEW-8: the session its conversation belongs to (keyed through the
        /// `conversations` shape), never the row's own legacy `session_id`.
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

    /// EXP-1358: the lookups every title rung reads, built ONCE per list
    /// (the issue map, each session's links, each session's input messages
    /// oldest first with their timestamps parsed once). `partitionThreads`
    /// used to rebuild all of these for EVERY row — O(rows × messages) on
    /// the main actor on every sync tick.
    public struct Index: Sendable {
        let issuesById: [String: TreeIssue]
        /// Each session's OWN links (NEW-1: the row sits on its holder), in link order.
        let ownLinks: [String: [TreeSessionIssue]]
        /// Each session's candidate first prompts, oldest first — `firstPrompt`'s
        /// filter, already sorted.
        let inputsBySession: [String: [Message]]

        public init(links: [TreeSessionIssue], issues: [TreeIssue], messages: [Message]) {
            var issuesById: [String: TreeIssue] = [:]
            for issue in issues { issuesById[issue.id] = issue }
            self.issuesById = issuesById
            let ordered = links.sorted { a, b in
                a.sortOrder != b.sortOrder ? a.sortOrder < b.sortOrder : a.issueId < b.issueId
            }
            ownLinks = Dictionary(grouping: ordered, by: \.sessionId)
            // The input filter, then ONE sort per session on pre-parsed stamps.
            var stamped: [String: [(ms: Double, message: Message)]] = [:]
            for message in messages where !message.hasCard {
                let isInput: Bool
                if message.authorKind == "user" {
                    isInput = true
                } else if let author = message.authorSessionId, !author.isEmpty {
                    isInput = author != message.sessionId
                } else {
                    isInput = false
                }
                guard isInput else { continue }
                stamped[message.sessionId, default: []].append((SessionTree.ms(message.createdAt) ?? 0, message))
            }
            inputsBySession = stamped.mapValues { entries in
                entries.sorted { a, b in a.ms != b.ms ? a.ms < b.ms : a.message.id < b.message.id }
                    .map(\.message)
            }
        }

        func sessionIssues(id: String) -> [TreeIssue] {
            (ownLinks[id] ?? []).compactMap { issuesById[$0.issueId] }
        }

        func firstPrompt(sessionId: String) -> String? {
            for message in inputsBySession[sessionId] ?? [] {
                if let line = SessionTitle.firstLine(message.body) { return line }
            }
            return nil
        }
    }

    /// The session's OWN links (root or child alike, NEW-1), in link order
    /// (`sortOrder`, then `issueId`); unknown issues are skipped.
    public static func sessionIssues(
        id: String,
        links: [TreeSessionIssue],
        issues: [TreeIssue]
    ) -> [TreeIssue] {
        Index(links: links, issues: issues, messages: []).sessionIssues(id: id)
    }

    /// The first prompt's usable line: a person's text or another session's
    /// input, oldest first.
    public static func firstPrompt(sessionId: String, messages: [Message]) -> String? {
        Index(links: [], issues: [], messages: messages).firstPrompt(sessionId: sessionId)
    }

    /// THE display title (fixture `session-title.json` rungs).
    public static func title(
        _ session: Session,
        links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [Message]
    ) -> String {
        title(session, index: Index(links: links, issues: issues, messages: messages))
    }

    /// THE display title off a prebuilt `Index` (one index, many rows).
    public static func title(_ session: Session, index: Index) -> String {
        let stored = trimmed(session.title)
        if !stored.isEmpty { return stored }
        let own = index.sessionIssues(id: session.id)
        if !own.isEmpty { return issueLabel(own) }
        if let prompt = index.firstPrompt(sessionId: session.id) {
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
        let index = Index(links: links, issues: issues, messages: messages)
        var out: [String: String] = [:]
        for session in sessions {
            out[session.id] = title(session, index: index)
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

    /// The tree rows' index (EXP-1358): build once, title every row.
    public static func index(
        sessionIssues links: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage]
    ) -> Index {
        Index(links: links, issues: issues, messages: messages.map(Message.init))
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
    static func firstLine(_ body: String) -> String? {
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
