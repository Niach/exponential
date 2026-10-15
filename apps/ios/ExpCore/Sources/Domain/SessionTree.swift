import Foundation

// EXP-1291 Threads v2: the session tree's view models — a thread IS a root
// session (`kind` thread), a team agent a root of `kind` agent, children are
// sub-sessions. A 1:1 port of the TS reference
// `packages/domain-contract/src/session-tree.ts` (desktop
// `domain::session_tree`, Android `domain/SessionTree.kt`), byte-locked by the
// fixtures `SessionTreeTests` replays:
//   fixtures/session-card.json   → `SessionTree.sessionCard`
//   fixtures/session-thread.json → `SessionTree.buildSessionThread`
//   fixtures/threads-list.json   → `SessionTree.partitionThreads` + `listAgents` + `threadRowMarkState`
// The fixtures' `$comment`s hold the rules. Pure: callers map their synced
// rows into the `Tree*` inputs.

// MARK: - Inputs

/// A usage wall (`sessions.blocked` jsonb): set = the session is walled.
public struct TreeSessionBlocked: Equatable, Sendable {
    public let window: String?
    public let resetsAt: String?

    public init(window: String? = nil, resetsAt: String? = nil) {
        self.window = window
        self.resetsAt = resetsAt
    }

    /// The synced jsonb TEXT; nil, `null` or a non-object = not walled.
    public static func parse(_ raw: String?) -> TreeSessionBlocked? {
        guard let object = SessionTree.jsonObject(raw) else { return nil }
        return TreeSessionBlocked(
            window: object["window"] as? String,
            resetsAt: object["resetsAt"] as? String
        )
    }
}

/// One `sessions.guide` entry, as far as the tree reads it (the Guide tab's
/// dot). The Guide tab itself renders through `SessionResults`.
public struct TreeGuideEntry: Equatable, Sendable {
    public let text: String?
    public let label: String?

    public init(text: String? = nil, label: String? = nil) {
        self.text = text
        self.label = label
    }

    /// The synced jsonb TEXT; nil/`null`/a non-array = no guide.
    public static func parseList(_ raw: String?) -> [TreeGuideEntry]? {
        guard let raw, let data = raw.data(using: .utf8),
              let entries = (try? JSONSerialization.jsonObject(with: data)) as? [Any]
        else { return nil }
        return entries.compactMap { entry in
            guard let object = entry as? [String: Any] else { return nil }
            return TreeGuideEntry(
                text: object["text"] as? String,
                label: object["label"] as? String
            )
        }
    }
}

/// One `sessions` row (root or child).
public struct TreeSession: Equatable, Sendable, Identifiable {
    public let id: String
    public let teamId: String
    /// nil = a root.
    public let parentId: String?
    /// The root's id (a root names itself).
    public let rootId: String
    /// Contract `sessionKindValues`: thread | agent.
    public let kind: String
    public let userId: String
    /// Contract `sessionStatusValues`: queued | running | idle | ended.
    public let status: String
    public let busy: Bool
    public let needsInput: Bool
    public let blocked: TreeSessionBlocked?
    public let shared: Bool
    public let title: String?
    public let guide: [TreeGuideEntry]?
    public let actionName: String?
    /// Agent facets (kind agent).
    public let name: String?
    public let icon: String?
    public let color: String?
    public let sortOrder: Double
    public let startedAt: String?
    public let endedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// EXP-1316: the steer device id of the machine that runs it (the
    /// thread row's device glyph); nil when unknown.
    public let deviceId: String?

    public init(
        id: String,
        teamId: String,
        parentId: String?,
        rootId: String,
        kind: String,
        userId: String,
        status: String,
        busy: Bool,
        needsInput: Bool,
        blocked: TreeSessionBlocked?,
        shared: Bool,
        title: String?,
        guide: [TreeGuideEntry]?,
        actionName: String?,
        name: String?,
        icon: String?,
        color: String?,
        sortOrder: Double,
        startedAt: String?,
        endedAt: String?,
        createdAt: String,
        updatedAt: String,
        deviceId: String? = nil
    ) {
        self.deviceId = deviceId
        self.id = id
        self.teamId = teamId
        self.parentId = parentId
        self.rootId = rootId
        self.kind = kind
        self.userId = userId
        self.status = status
        self.busy = busy
        self.needsInput = needsInput
        self.blocked = blocked
        self.shared = shared
        self.title = title
        self.guide = guide
        self.actionName = actionName
        self.name = name
        self.icon = icon
        self.color = color
        self.sortOrder = sortOrder
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

/// One `session_issues` link (root ↔ issue).
public struct TreeSessionIssue: Equatable, Sendable {
    public let sessionId: String
    public let issueId: String
    public let sortOrder: Double

    public init(sessionId: String, issueId: String, sortOrder: Double) {
        self.sessionId = sessionId
        self.issueId = issueId
        self.sortOrder = sortOrder
    }
}

public struct TreeIssue: Equatable, Sendable, Identifiable {
    public let id: String
    public let identifier: String
    public let title: String

    public init(id: String, identifier: String, title: String) {
        self.id = id
        self.identifier = identifier
        self.title = title
    }
}

/// One question-card option (EXP-1322): the answer IS the label; the
/// description (nil when absent) renders under it. The synced card carries a
/// bare string or `{label, description?}`; both normalise here.
public struct TreeQuestionOption: Equatable, Sendable {
    public let label: String
    public let description: String?

    public init(label: String, description: String? = nil) {
        self.label = label
        self.description = description
    }
}

extension TreeQuestionOption: ExpressibleByStringLiteral {
    public init(stringLiteral value: String) {
        self.init(label: value)
    }
}

/// A message's `card` jsonb (contract `messageCardKindValues`), read
/// tolerantly: every field the rows use, nil when absent or mistyped.
public struct TreeMessageCard: Equatable, Sendable {
    public let kind: String?
    /// session | thread_started.
    public let sessionId: String?
    /// question.
    public let question: String?
    public let options: [TreeQuestionOption]
    public let answer: String?
    /// plan.
    public let plan: String?
    public let decision: String?
    /// subagent.
    public let title: String?
    public let status: String?
    /// picture.
    public let attachmentId: String?
    public let width: Double?
    public let height: Double?
    public let caption: String?
    /// event (contract `messageEventKindValues`).
    public let event: String?
    public let issueId: String?
    public let pullRequestId: String?

    public init(
        kind: String?,
        sessionId: String? = nil,
        question: String? = nil,
        options: [TreeQuestionOption] = [],
        answer: String? = nil,
        plan: String? = nil,
        decision: String? = nil,
        title: String? = nil,
        status: String? = nil,
        attachmentId: String? = nil,
        width: Double? = nil,
        height: Double? = nil,
        caption: String? = nil,
        event: String? = nil,
        issueId: String? = nil,
        pullRequestId: String? = nil
    ) {
        self.kind = kind
        self.sessionId = sessionId
        self.question = question
        self.options = options
        self.answer = answer
        self.plan = plan
        self.decision = decision
        self.title = title
        self.status = status
        self.attachmentId = attachmentId
        self.width = width
        self.height = height
        self.caption = caption
        self.event = event
        self.issueId = issueId
        self.pullRequestId = pullRequestId
    }

    /// A decoded JSON object (JSONSerialization).
    public init(object: [String: Any]) {
        func text(_ key: String) -> String? { object[key] as? String }
        // `String(card.x ?? '')` in the reference: a scalar reads as text.
        func loose(_ key: String) -> String? {
            switch object[key] {
            case let string as String: string
            case let number as NSNumber: number.stringValue
            default: nil
            }
        }
        func number(_ key: String) -> Double? {
            guard let value = object[key] as? NSNumber,
                  CFGetTypeID(value) != CFBooleanGetTypeID()
            else { return nil }
            return value.doubleValue
        }
        self.init(
            kind: text("kind"),
            sessionId: text("sessionId"),
            question: loose("question"),
            options: (object["options"] as? [Any])?.compactMap { option in
                switch option {
                case let string as String: TreeQuestionOption(label: string)
                case let entry as [String: Any]:
                    (entry["label"] as? String).map {
                        let description = entry["description"] as? String
                        return TreeQuestionOption(label: $0, description: description?.isEmpty == false ? description : nil)
                    }
                default: nil
                }
            } ?? [],
            answer: text("answer"),
            plan: loose("plan"),
            decision: text("decision"),
            title: loose("title"),
            status: loose("status"),
            attachmentId: text("attachmentId"),
            width: number("width"),
            height: number("height"),
            caption: text("caption"),
            event: loose("event"),
            issueId: text("issueId"),
            pullRequestId: text("pullRequestId")
        )
    }

    /// The synced jsonb TEXT; nil/`null`/a non-object = no card.
    public static func parse(_ raw: String?) -> TreeMessageCard? {
        SessionTree.jsonObject(raw).map(TreeMessageCard.init(object:))
    }
}

/// One `messages` row.
public struct TreeMessage: Equatable, Sendable, Identifiable {
    public let id: String
    public let sessionId: String
    /// Contract `messageAuthorKindValues`: user | agent | system.
    public let authorKind: String
    public let authorId: String?
    public let body: String
    public let card: TreeMessageCard?
    public let answeredAt: String?
    public let consumedAt: String?
    public let createdAt: String

    public init(
        id: String,
        sessionId: String,
        authorKind: String,
        authorId: String?,
        body: String,
        card: TreeMessageCard?,
        answeredAt: String?,
        consumedAt: String?,
        createdAt: String
    ) {
        self.id = id
        self.sessionId = sessionId
        self.authorKind = authorKind
        self.authorId = authorId
        self.body = body
        self.card = card
        self.answeredAt = answeredAt
        self.consumedAt = consumedAt
        self.createdAt = createdAt
    }
}

public struct TreePullRequest: Equatable, Sendable, Identifiable {
    public let id: String
    public let number: Int

    public init(id: String, number: Int) {
        self.id = id
        self.number = number
    }
}

// MARK: - Outputs

/// One state word per session: the card's kind word, icon and tone.
public enum SessionState: String, Equatable, Sendable, CaseIterable {
    case queued
    case working
    case waiting
    case needsYou = "needs_you"
    case blocked
    case ended

    /// Fixture `copy.states`.
    public var word: String {
        switch self {
        case .queued: "Queued"
        case .working: "Working"
        case .waiting: "Waiting"
        case .needsYou: "Needs you"
        case .blocked: "Usage limit"
        case .ended: "Ended"
        }
    }

    /// Fixture `icons` (concept names).
    public var icon: String {
        switch self {
        case .queued: "ui-clock"
        case .working: "coding-running"
        case .waiting: "coding-assistant"
        case .needsYou: "coding-needs-input"
        case .blocked: "notification-session-blocked"
        case .ended: "coding-ended"
        }
    }

    /// Fixture `tones`: muted | foreground | amber | rose.
    public var tone: String {
        switch self {
        case .working: "foreground"
        case .needsYou: "amber"
        case .blocked: "rose"
        case .queued, .waiting, .ended: "muted"
        }
    }
}

/// THE session card = the Work card's header row (EXP-1240 `WorkCardView`):
/// icon, name, the muted state word, trailing elapsed, a spinner while
/// `running`. Tapping it opens that session's raw view.
public struct SessionCard: Equatable, Sendable {
    public let sessionId: String
    /// Concept name.
    public let icon: String
    public let name: String
    public let kindWord: String
    public let state: SessionState
    public let tone: String
    public let elapsed: String?
    public let running: Bool
    public let testId: String

    public init(
        sessionId: String,
        icon: String,
        name: String,
        kindWord: String,
        state: SessionState,
        tone: String,
        elapsed: String?,
        running: Bool,
        testId: String
    ) {
        self.sessionId = sessionId
        self.icon = icon
        self.name = name
        self.kindWord = kindWord
        self.state = state
        self.tone = tone
        self.elapsed = elapsed
        self.running = running
        self.testId = testId
    }
}

/// The thread header's tab strip (phones: the pager).
public enum ThreadTab: Equatable, Sendable {
    case conversation
    case issue(issueId: String, identifier: String)
    /// The "+" (link/file an issue).
    case add
    /// EXP-1316: one chip per session at the VIEWED level (the root's run +
    /// its direct children, or the focused session + ITS direct children).
    /// `markState` = `threadRowMarkState` of that session, `.ended` once it
    /// ended.
    case session(sessionId: String, name: String, markState: SessionState)
    /// `dot` while the root's guide holds any text or picture.
    case guide(dot: Bool)
}

/// One Conversation row.
public enum ThreadRow: Equatable, Sendable {
    /// `author` = user | agent | system; a person's row is `queued` until
    /// the device consumed it.
    case message(id: String, author: String, authorId: String?, body: String, queued: Bool)
    /// `id` = the root's id for its own card, else the card message's id.
    case session(id: String, card: SessionCard)
    /// `custom` = a free-text answer is offered beside the options (always,
    /// EXP-1316: THE session view's card).
    case question(id: String, question: String, options: [TreeQuestionOption], custom: Bool, answer: String?, answered: Bool)
    case plan(id: String, plan: String, decision: String?)
    case subagent(id: String, title: String, status: String)
    case picture(id: String, attachmentId: String, width: Double?, height: Double?, caption: String?)
    /// One muted line.
    case event(id: String, event: String, text: String)

    public var id: String {
        switch self {
        case let .message(id, _, _, _, _),
             let .session(id, _),
             let .question(id, _, _, _, _, _),
             let .plan(id, _, _),
             let .subagent(id, _, _),
             let .picture(id, _, _, _, _),
             let .event(id, _, _):
            id
        }
    }
}

/// The composer below a thread: THE session composer box.
public struct ThreadComposer: Equatable, Sendable {
    /// `sessions.send({sessionId})` target (the root).
    public let sessionId: String
    public let placeholder: String

    public init(sessionId: String, placeholder: String) {
        self.sessionId = sessionId
        self.placeholder = placeholder
    }
}

public struct SessionThreadView: Equatable, Sendable {
    public let rootId: String
    public let title: String
    /// A lock glyph beside the title (`!root.shared`).
    public let isPrivate: Bool
    /// The focused session (nil = the thread itself).
    public let focusId: String?
    public let tabs: [ThreadTab]
    public let rows: [ThreadRow]
    public let composer: ThreadComposer
    public let live: Bool
    public let busy: Bool
    public let needsYou: Bool

    public init(
        rootId: String,
        title: String,
        isPrivate: Bool,
        focusId: String?,
        tabs: [ThreadTab],
        rows: [ThreadRow],
        composer: ThreadComposer,
        live: Bool,
        busy: Bool,
        needsYou: Bool
    ) {
        self.rootId = rootId
        self.title = title
        self.isPrivate = isPrivate
        self.focusId = focusId
        self.tabs = tabs
        self.rows = rows
        self.composer = composer
        self.live = live
        self.busy = busy
        self.needsYou = needsYou
    }
}

public struct ThreadsListRow: Equatable, Sendable, Identifiable {
    public let sessionId: String
    public let teamId: String
    public let title: String
    public let isPrivate: Bool
    public let busy: Bool
    public let needsYou: Bool
    /// The tree's newest `updatedAt` (wire text).
    public let activityAt: String
    /// EXP-1316: the ROOT's device (the row's trailing device glyph,
    /// resolved against the synced devices rows); nil = none.
    public let deviceId: String?

    public var id: String { sessionId }

    public init(
        sessionId: String,
        teamId: String,
        title: String,
        isPrivate: Bool,
        busy: Bool,
        needsYou: Bool,
        activityAt: String,
        deviceId: String? = nil
    ) {
        self.deviceId = deviceId
        self.sessionId = sessionId
        self.teamId = teamId
        self.title = title
        self.isPrivate = isPrivate
        self.busy = busy
        self.needsYou = needsYou
        self.activityAt = activityAt
    }
}

public struct ThreadsListGroup: Equatable, Sendable, Identifiable {
    public enum Key: String, Equatable, Sendable, CaseIterable {
        case needsYou = "needs_you"
        case running
        case recent

        /// Fixture `copy.groups`.
        public var title: String {
            switch self {
            case .needsYou: "Needs you"
            case .running: "Running"
            case .recent: "Recent"
            }
        }
    }

    public let key: Key
    public let title: String
    public let rows: [ThreadsListRow]

    public var id: String { key.rawValue }

    public init(key: Key, title: String, rows: [ThreadsListRow]) {
        self.key = key
        self.title = title
        self.rows = rows
    }
}

/// EXP-1309: a Threads row's ONE quiet trailing word (web
/// `THREAD_ROW_WORDS`); `SessionTree.threadRowWord` picks it.
public enum ThreadRowWord: String, Equatable, Sendable, CaseIterable {
    case stack
    case readyToMerge
    case merged
    case question
    /// An agent row waiting on a person.
    case needsInput

    public var text: String {
        switch self {
        case .readyToMerge: "ready to merge"
        case .stack: "stack"
        case .merged: "merged"
        case .question: "question"
        case .needsInput: "needs input"
        }
    }
}

/// A pull request as far as the stack test reads it.
public struct StackPr: Equatable, Sendable {
    public let id: String
    public let state: String
    public let basePullRequestId: String?

    public init(id: String, state: String, basePullRequestId: String?) {
        self.id = id
        self.state = state
        self.basePullRequestId = basePullRequestId
    }

    public init(_ pr: PullRequestEntity) {
        self.init(id: pr.id, state: pr.state, basePullRequestId: pr.basePullRequestId)
    }
}

/// One row of the Agents band (icon + color like a board row).
public struct AgentRow: Equatable, Sendable, Identifiable {
    public let sessionId: String
    public let teamId: String
    public let name: String
    /// Icon NAME (contract `boardIconValues`).
    public let icon: String
    /// `#rrggbb`.
    public let color: String
    public let busy: Bool
    public let needsYou: Bool

    public var id: String { sessionId }

    public init(
        sessionId: String,
        teamId: String,
        name: String,
        icon: String,
        color: String,
        busy: Bool,
        needsYou: Bool
    ) {
        self.sessionId = sessionId
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.busy = busy
        self.needsYou = needsYou
    }
}

// MARK: - The rules

public enum SessionTree {
    /// fixtures/session-card.json `copy` + `testIds`.
    public enum CardCopy {
        public static let session = "Session"
        public static let testId = "session-card"
    }

    /// fixtures/session-thread.json `copy` + `titleMax` + `testIds`.
    public enum ThreadCopy {
        public static let untitled = "Untitled"
        public static let placeholder = "Reply…"
        public static let conversation = "Conversation"
        public static let guide = "Guide"
        /// `{issue}` = the identifier, `{pr}` = `#<number>`.
        public static let events: [String: String] = [
            "issue_linked": "Linked {issue}",
            "issue_filed": "Filed {issue}",
            "issue_unlinked": "Unlinked {issue}",
            "pr_opened": "Opened PR {pr}",
            "pr_merged": "Merged PR {pr}",
            "pr_closed": "Closed PR {pr}",
            "session_ended": "Session ended",
        ]
        public static let titleMax = 80
        /// `titles`: `{action}`, `{issue}`, `{more}`.
        public static let titleAction = "Run {action}"
        public static let titleIssue = "Implement {issue}"
        public static let titleIssues = "Implement {issue} + {more}"
        public static let tabsTestId = "thread-tabs"
        public static let conversationTestId = "thread-conversation"
        public static let composerTestId = "thread-composer"
        /// EXP-1306: the Guide's stack band.
        public static let stack = "Stack"
        public static let guideStackTestId = "thread-guide-stack"
        public static let guideMergeTestId = "thread-guide-merge"
        /// EXP-1307: the header's `…` on the Guide (`guideMenuEntries`).
        public static let guideMenuTestId = "thread-guide-menu"
    }

    /// fixtures/threads-list.json `copy` + `agentDefaults` + `testIds`
    /// (group titles = `ThreadsListGroup.Key.title`, words =
    /// `ThreadRowWord.text`).
    public enum ListCopy {
        public static let title = "Threads"
        /// The rail's one band (web + IDE).
        public static let rail = "Active"
        public static let agents = "Agents"
        public static let empty = "No sessions yet"
        public static let agentDefaultIcon = "bot"
        public static let agentDefaultColor = "#6366f1"
        public static let listTestId = "threads-list"
        public static let rowTestId = "threads-row"
        public static let agentRowTestId = "agents-row"
    }

    /// The work card's elapsed ladder: `37s`, `2m 04s`, `1h 03m`.
    public static func formatElapsed(ms durationMs: Double) -> String {
        let total = max(0, Int((durationMs / 1000).rounded(.down)))
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let seconds = total % 60
        func pad(_ value: Int) -> String { value < 10 ? "0\(value)" : "\(value)" }
        if hours > 0 { return "\(hours)h \(pad(minutes))m" }
        if minutes > 0 { return "\(minutes)m \(pad(seconds))s" }
        return "\(seconds)s"
    }

    public static func sessionState(_ session: TreeSession) -> SessionState {
        if session.status == "ended" { return .ended }
        if session.needsInput { return .needsYou }
        if session.blocked != nil { return .blocked }
        if session.status == "queued" { return .queued }
        if session.busy { return .working }
        return .waiting
    }

    /// EXP-1312: a thread row's lead outside the Threads tab (Home's Running
    /// band) ×4: needs you beats working, else the bare mark
    /// (`threads-list.json` `markCases`).
    public static func threadRowMarkState(needsYou: Bool, busy: Bool) -> SessionState {
        if needsYou { return .needsYou }
        if busy { return .working }
        return .waiting
    }

    /// EXP-1316: a session chip's lead: that session's row mark, `.ended`
    /// once it ended.
    public static func sessionTabMarkState(_ session: TreeSession) -> SessionState {
        if session.status == "ended" { return .ended }
        return threadRowMarkState(needsYou: session.needsInput, busy: session.busy)
    }

    /// Sessions of one tree in creation order (ties by id): `Session N` names.
    public static func treeOrder(_ sessions: [TreeSession]) -> [TreeSession] {
        sessions.sorted { a, b in
            let ma = ms(a.createdAt) ?? 0, mb = ms(b.createdAt) ?? 0
            if ma != mb { return ma < mb }
            return a.id < b.id
        }
    }

    /// THE session card. `ordinal` = its 1-based place in the tree's
    /// creation order (names an untitled session `Session N`).
    public static func sessionCard(_ session: TreeSession, ordinal: Int, now: Date) -> SessionCard {
        let state = sessionState(session)
        let start = ms(session.startedAt)
        let end = state == .ended ? (ms(session.endedAt) ?? ms(session.updatedAt)) : ms(now)
        var elapsed: String?
        if let start, let end, state != .queued { elapsed = formatElapsed(ms: end - start) }
        let title = session.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return SessionCard(
            sessionId: session.id,
            icon: state.icon,
            name: title.isEmpty ? "\(CardCopy.session) \(ordinal)" : title,
            kindWord: state.word,
            state: state,
            tone: state.tone,
            elapsed: elapsed,
            running: state == .working,
            testId: CardCopy.testId
        )
    }

    /// EXP-1316: a thread's title, never renamed by a person: an agent's
    /// `name`; else `Run <action>`; else `Implement <IDENT>` (the first
    /// synced issue by sortOrder, `+ <n-1>` with more); else the stored
    /// title (the CLI's auto-name); else the first line of the root's first
    /// person message (capped `titleMax`); else `Untitled`.
    public static func threadTitle(
        root: TreeSession,
        issues: [TreeIssue],
        messages: [TreeMessage]
    ) -> String {
        func trimmed(_ value: String?) -> String {
            value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        }
        let name = trimmed(root.name)
        if root.kind == "agent", !name.isEmpty { return name }
        let action = trimmed(root.actionName)
        if !action.isEmpty { return ThreadCopy.titleAction.replacingOccurrences(of: "{action}", with: action) }
        if let first = issues.first {
            return issues.count > 1
                ? ThreadCopy.titleIssues
                    .replacingOccurrences(of: "{issue}", with: first.identifier)
                    .replacingOccurrences(of: "{more}", with: String(issues.count - 1))
                : ThreadCopy.titleIssue.replacingOccurrences(of: "{issue}", with: first.identifier)
        }
        let stored = trimmed(root.title)
        if !stored.isEmpty { return stored }
        let firstMessage = messages
            .filter { $0.sessionId == root.id && $0.authorKind == "user" }
            .enumerated()
            .min { a, b in
                // A stable oldest-first pick (the reference's sort is stable).
                let ma = ms(a.element.createdAt) ?? 0, mb = ms(b.element.createdAt) ?? 0
                return ma != mb ? ma < mb : a.offset < b.offset
            }?.element
        if let firstMessage, let line = firstLine(firstMessage.body, max: ThreadCopy.titleMax) {
            return line
        }
        return ThreadCopy.untitled
    }

    /// The thread screen: the header tabs, the root's conversation as rows
    /// (its own session card first, then its messages oldest first) and the
    /// composer target (the root). `focus` = a session of the tree whose
    /// level the session chips show (itself + its direct children).
    public static func buildSessionThread(
        root: TreeSession,
        sessions: [TreeSession],
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage],
        pullRequests: [TreePullRequest],
        now: Date,
        focus: String? = nil
    ) -> SessionThreadView {
        var tree = treeOrder(sessions.filter { $0.rootId == root.id || $0.id == root.id })
        if !tree.contains(where: { $0.id == root.id }) { tree.insert(root, at: 0) }
        var ordinals: [String: Int] = [:]
        for (index, session) in tree.enumerated() { ordinals[session.id] = index + 1 }
        var byId: [String: TreeSession] = [:]
        for session in sessions { byId[session.id] = session }
        byId[root.id] = root
        let linked = rootIssues(root.id, sessionIssues: sessionIssues, issues: issues)
        var issueMap: [String: TreeIssue] = [:]
        for issue in issues { issueMap[issue.id] = issue }
        var prMap: [String: TreePullRequest] = [:]
        for pr in pullRequests { prMap[pr.id] = pr }
        let own = messages
            .filter { $0.sessionId == root.id }
            .sorted { a, b in
                let ma = ms(a.createdAt) ?? 0, mb = ms(b.createdAt) ?? 0
                if ma != mb { return ma < mb }
                return a.id < b.id
            }
        let title = threadTitle(root: root, issues: linked, messages: own)
        let focused = focus.flatMap { id in tree.first { $0.id == id } }
        let level = focused ?? root
        let sessionTabs: [ThreadTab] = ([level] + tree.filter { $0.parentId == level.id && $0.id != level.id })
            .map { session in
                .session(
                    sessionId: session.id,
                    name: sessionCard(session, ordinal: ordinals[session.id] ?? 1, now: now).name,
                    markState: sessionTabMarkState(session)
                )
            }

        var rows: [ThreadRow] = [.session(id: root.id, card: sessionCard(root, ordinal: 1, now: now))]
        for message in own {
            guard let card = message.card else {
                let author = message.authorKind == "user" || message.authorKind == "agent"
                    ? message.authorKind
                    : "system"
                if message.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { continue }
                rows.append(.message(
                    id: message.id,
                    author: author,
                    authorId: message.authorId,
                    body: message.body,
                    queued: author == "user" && message.consumedAt == nil
                ))
                continue
            }
            switch card.kind {
            case "session", "thread_started":
                guard let targetId = card.sessionId, let target = byId[targetId] else { break }
                rows.append(.session(
                    id: message.id,
                    card: sessionCard(target, ordinal: ordinals[target.id] ?? 1, now: now)
                ))
            case "question":
                rows.append(.question(
                    id: message.id,
                    question: card.question ?? "",
                    options: card.options,
                    custom: true,
                    answer: card.answer,
                    answered: message.answeredAt != nil
                ))
            case "plan":
                rows.append(.plan(id: message.id, plan: card.plan ?? "", decision: card.decision))
            case "subagent":
                rows.append(.subagent(id: message.id, title: card.title ?? "", status: card.status ?? ""))
            case "picture":
                guard let attachmentId = card.attachmentId else { break }
                rows.append(.picture(
                    id: message.id,
                    attachmentId: attachmentId,
                    width: card.width,
                    height: card.height,
                    caption: card.caption
                ))
            case "event":
                if let text = eventText(card, issues: issueMap, pullRequests: prMap) {
                    rows.append(.event(id: message.id, event: card.event ?? "", text: text))
                }
            default:
                break
            }
        }

        let guideDot = (root.guide ?? []).contains { entry in
            !(entry.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || entry.label != nil
        }
        let live = tree.filter { $0.status != "ended" }
        return SessionThreadView(
            rootId: root.id,
            title: title,
            isPrivate: !root.shared,
            focusId: focused?.id,
            tabs: [.conversation]
                + linked.map { .issue(issueId: $0.id, identifier: $0.identifier) }
                + [.add] + sessionTabs + [.guide(dot: guideDot)],
            rows: rows,
            composer: ThreadComposer(sessionId: root.id, placeholder: ThreadCopy.placeholder),
            live: !live.isEmpty,
            busy: live.contains { $0.busy },
            needsYou: live.contains { $0.needsInput }
        )
    }

    /// The Threads list: the roots of kind `thread` in scope (`teamId` = the
    /// ACTIVE team; nil = every team, phones band rows per team), grouped
    /// Needs you · Running · Recent, each by the tree's newest `updatedAt`
    /// desc. Empty groups are omitted.
    public static func partitionThreads(
        sessions: [TreeSession],
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue],
        messages: [TreeMessage],
        teamId: String?
    ) -> [ThreadsListGroup] {
        let roots = sessions.filter {
            $0.parentId == nil && $0.kind == "thread" && (teamId == nil || $0.teamId == teamId)
        }
        var byRoot: [String: [TreeSession]] = [:]
        for session in sessions { byRoot[session.rootId, default: []].append(session) }
        var buckets: [ThreadsListGroup.Key: [ThreadsListRow]] = [:]
        for root in roots {
            let tree = byRoot[root.id] ?? [root]
            let live = tree.filter { $0.status != "ended" }
            let needsYou = live.contains { $0.needsInput }
            let running = live.contains { $0.status == "queued" || $0.status == "running" }
            let activity = tree.reduce(root.updatedAt) { latest, session in
                (ms(session.updatedAt) ?? 0) > (ms(latest) ?? 0) ? session.updatedAt : latest
            }
            let title = threadTitle(
                root: root,
                issues: rootIssues(root.id, sessionIssues: sessionIssues, issues: issues),
                messages: messages
            )
            let row = ThreadsListRow(
                sessionId: root.id,
                teamId: root.teamId,
                title: title,
                isPrivate: !root.shared,
                busy: live.contains { $0.busy },
                needsYou: needsYou,
                activityAt: activity,
                deviceId: root.deviceId
            )
            buckets[needsYou ? .needsYou : running ? .running : .recent, default: []].append(row)
        }
        return ThreadsListGroup.Key.allCases.compactMap { key in
            let rows = (buckets[key] ?? []).sorted { a, b in
                let ma = ms(a.activityAt) ?? 0, mb = ms(b.activityAt) ?? 0
                if ma != mb { return ma > mb }
                return a.sessionId < b.sessionId
            }
            return rows.isEmpty ? nil : ThreadsListGroup(key: key, title: key.title, rows: rows)
        }
    }

    /// The Agents band: one row per agent root of the team (nil = every
    /// team) by `sortOrder`, then name, then id. An agent's spinner/needs-you
    /// dot covers its own row only.
    public static func listAgents(_ sessions: [TreeSession], teamId: String?) -> [AgentRow] {
        sessions
            .filter {
                $0.parentId == nil && $0.kind == "agent" && (teamId == nil || $0.teamId == teamId)
            }
            .sorted { a, b in
                if a.sortOrder != b.sortOrder { return a.sortOrder < b.sortOrder }
                let byName = (a.name ?? "").localizedCompare(b.name ?? "")
                if byName != .orderedSame { return byName == .orderedAscending }
                return a.id < b.id
            }
            .map {
                AgentRow(
                    sessionId: $0.id,
                    teamId: $0.teamId,
                    name: $0.name ?? "",
                    icon: $0.icon ?? ListCopy.agentDefaultIcon,
                    color: $0.color ?? ListCopy.agentDefaultColor,
                    busy: $0.busy,
                    needsYou: $0.needsInput
                )
            }
    }

    /// The caller's own working sessions counted per team — the board
    /// switcher SHEET's per-team dots (EXP-1075/EXP-1210). A session counts
    /// while it is queued or running; `needsInput` while any own non-ended
    /// session waits on a person. No resolved user owns nothing.
    public struct TeamLiveSessions: Equatable, Sendable {
        public var count: Int
        public var needsInput: Bool

        public init(count: Int = 0, needsInput: Bool = false) {
            self.count = count
            self.needsInput = needsInput
        }
    }

    public static func liveByTeam(_ sessions: [TreeSession], userId: String?) -> [String: TeamLiveSessions] {
        guard let userId, !userId.isEmpty else { return [:] }
        var byTeam: [String: TeamLiveSessions] = [:]
        for session in sessions where session.userId == userId && session.status != "ended" {
            let working = session.status == "queued" || session.status == "running"
            guard working || session.needsInput else { continue }
            var entry = byTeam[session.teamId] ?? TeamLiveSessions()
            if working { entry.count += 1 }
            if session.needsInput { entry.needsInput = true }
            byTeam[session.teamId] = entry
        }
        return byTeam
    }

    /// EXP-1306: the ONE pull request a thread's Guide speaks for (its stack
    /// rail + the bottom Merge capsule): the newest open one, else the newest
    /// merged one (createdAt, ties by the higher number); a closed PR never
    /// counts.
    public static func guidePullRequest(_ pullRequests: [PullRequestEntity]) -> PullRequestEntity? {
        func newest(_ state: String) -> PullRequestEntity? {
            pullRequests.filter { $0.state == state }.reduce(nil) { best, pr in
                guard let best else { return pr }
                let diff = (ms(pr.createdAt) ?? 0) - (ms(best.createdAt) ?? 0)
                return diff > 0 || (diff == 0 && pr.number > best.number) ? pr : best
            }
        }
        return newest(DomainContract.prStateOpen) ?? newest(DomainContract.prStateMerged)
    }

    /// EXP-1307: one entry of the thread header's `…` on the Guide.
    public enum GuideMenuEntry: String, Sendable, Equatable {
        case openGithub = "open-github"
        case closePr = "close-pr"
    }

    /// EXP-1307: the thread header's `…` on the Guide: Open on GitHub whenever
    /// the Guide has a PR (`state` = its state, nil = none), Close PR while it
    /// is open and the viewer is a member; empty = no trigger.
    public static func guideMenuEntries(state: String?, member: Bool) -> [GuideMenuEntry] {
        guard let state else { return [] }
        return state == DomainContract.prStateOpen && member ? [.openGithub, .closePr] : [.openGithub]
    }

    /// EXP-1318: an open PR of the thread sits in an open stack: based on
    /// another open PR, or another open PR is based on it. `teamPrs` = the
    /// team's PRs (a stack edge may cross threads). Fixture `stackCases`.
    public static func threadInStack(_ prs: [StackPr], teamPrs: [StackPr]) -> Bool {
        let open = DomainContract.prStateOpen
        let openIds = Set(teamPrs.filter { $0.state == open }.map(\.id))
        return prs.filter { $0.state == open }.contains { pr in
            (pr.basePullRequestId.map(openIds.contains) ?? false)
                || teamPrs.contains { $0.state == open && $0.basePullRequestId == pr.id }
        }
    }

    /// EXP-1309/1318: ONE quiet trailing word for a Threads row ×4 (the lock
    /// rides beside it): an open PR in a stack → `stack`; an open PR while
    /// nothing works → `readyToMerge` (working = no word); no open PR but a
    /// merged one → `merged`; else waiting on a person → `question`; else
    /// none. Fixture `wordCases`.
    public static func threadRowWord(
        prStates: [String],
        busy: Bool,
        needsYou: Bool,
        inStack: Bool
    ) -> ThreadRowWord? {
        if prStates.contains(DomainContract.prStateOpen) {
            if inStack { return .stack }
            return busy ? nil : .readyToMerge
        }
        if prStates.contains(DomainContract.prStateMerged) { return .merged }
        return needsYou ? .question : nil
    }

    /// The synced-row form: `prs` = the root's own pull requests, `teamPrs`
    /// = its team's.
    public static func threadRowWord(
        busy: Bool,
        needsYou: Bool,
        prs: [PullRequestEntity],
        teamPrs: [PullRequestEntity]
    ) -> ThreadRowWord? {
        threadRowWord(
            prStates: prs.map(\.state),
            busy: busy,
            needsYou: needsYou,
            inStack: threadInStack(prs.map(StackPr.init), teamPrs: teamPrs.map(StackPr.init))
        )
    }

    /// The rail's Active band (`ListCopy.rail`): every Needs you row, every
    /// Running row, then the Recent rows whose word still asks for a merge
    /// (`readyToMerge` or `stack`), in group order. Fixture `railCases`.
    public static func railThreads(
        _ groups: [ThreadsListGroup],
        wordOf: (ThreadsListRow) -> ThreadRowWord?
    ) -> [ThreadsListRow] {
        groups.flatMap { group -> [ThreadsListRow] in
            guard group.key == .recent else { return group.rows }
            return group.rows.filter { row in
                let word = wordOf(row)
                return word == .readyToMerge || word == .stack
            }
        }
    }

    /// EXP-1306 (EXP-820 ×4): a question or plan card waiting on the viewer
    /// takes the session composer's place.
    public static func composerYieldsToCard(live: Bool, pendingAsk: Bool) -> Bool {
        live && pendingAsk
    }

    // MARK: - Helpers

    private static func rootIssues(
        _ rootId: String,
        sessionIssues: [TreeSessionIssue],
        issues: [TreeIssue]
    ) -> [TreeIssue] {
        var byId: [String: TreeIssue] = [:]
        for issue in issues { byId[issue.id] = issue }
        return sessionIssues
            .enumerated()
            .filter { $0.element.sessionId == rootId }
            .sorted { a, b in
                a.element.sortOrder != b.element.sortOrder
                    ? a.element.sortOrder < b.element.sortOrder
                    : a.offset < b.offset
            }
            .compactMap { byId[$0.element.issueId] }
    }

    private static func eventText(
        _ card: TreeMessageCard,
        issues: [String: TreeIssue],
        pullRequests: [String: TreePullRequest]
    ) -> String? {
        guard let template = ThreadCopy.events[card.event ?? ""] else { return nil }
        if template.contains("{issue}") {
            guard let issue = card.issueId.flatMap({ issues[$0] }) else { return nil }
            return template.replacingOccurrences(of: "{issue}", with: issue.identifier)
        }
        if template.contains("{pr}") {
            guard let pr = card.pullRequestId.flatMap({ pullRequests[$0] }) else { return nil }
            return template.replacingOccurrences(of: "{pr}", with: "#\(pr.number)")
        }
        return template
    }

    /// First non-empty, non-image line of a person's text, capped with `…`.
    private static func firstLine(_ body: String, max: Int) -> String? {
        let line = body
            .components(separatedBy: "\n")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty && !$0.hasPrefix("![") }
        guard let line else { return nil }
        guard line.count > max else { return line }
        let head = String(line.prefix(max - 1))
        let trimmed = head.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        return "\(trimmed)…"
    }

    /// Epoch milliseconds of a wire timestamp; nil when missing or unreadable.
    static func ms(_ value: String?) -> Double? {
        guard let value, !value.isEmpty, let date = WireTimestamps.parse(value) else { return nil }
        return ms(date)
    }

    static func ms(_ date: Date) -> Double {
        (date.timeIntervalSince1970 * 1000).rounded()
    }

    static func jsonObject(_ raw: String?) -> [String: Any]? {
        guard let raw, let data = raw.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }
}
