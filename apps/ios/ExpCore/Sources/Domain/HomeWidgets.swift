import Foundation

/// #EXP-1830: the home screen widgets, a 1:1 port of the TS reference
/// `packages/domain-contract/src/home-widgets.ts` (Android
/// `domain/HomeWidgets.kt`), locked by `fixtures/home-widgets.json`
/// (`HomeWidgetsTests`). The fixture's `$comment` holds the rules.
///
/// Foundation only (with `SharedAppGroup`): the widget extension compiles
/// this file on its own, without the rest of ExpCore (no GRDB). The app
/// writes `Snapshot` into the app group; the widgets read it and fold it here.
public enum HomeWidgets {
    // MARK: - Fixture constants

    public enum Kind: String, CaseIterable, Sendable {
        case board
        case myIssues = "my-issues"
        case needsYou = "needs-you"
    }

    public enum Copy {
        public static let myIssues = "My issues"
        public static let needsYou = "Needs you"
        public static let emptyIssues = "No open issues"
        public static let emptyNeedsYou = "Nothing needs you"
        public static let signedOut = "Sign in to Exponential"
        public static let sealedRow = "Encrypted chat"
        public static let pickBoard = "Board"
        public static let pickTeams = "Teams"
        public static let boardGallery = "Board"
        public static let boardGalleryDescription = "A board's open issues."
        public static let myIssuesGalleryDescription = "Your open issues."
        public static let needsYouGalleryDescription = "Plans and questions waiting on you."
    }

    /// Fixture `icons` (concepts, `packages/icons/icons.json`).
    public enum Icons {
        public static let myIssues = "nav-my-issues"
        public static let needsYou = "nav-needs-you"
        public static let plan = "coding-plan"
        public static let question = "ui-help"
        public static let pr = "pr-open"
    }

    public static let openCategories = ["started", "unstarted", "backlog"]
    public static let categoryOrder = ["started", "unstarted", "backlog"]

    public enum Capacity {
        public static let small = 3
        public static let medium = 3
        public static let large = 8
    }

    public enum Look {
        public static let padding: Double = 14
        public static let headerGap: Double = 8
        public static let rowHeight: Double = 28
        public static let glyph: Double = 16
    }

    /// contract `issuePriority.displayOrder` (urgent first, none last;
    /// `IssuePriority.displayOrder`, restated so this file stands alone).
    public static let priorityOrder = ["urgent", "high", "medium", "low", "none"]

    // MARK: - Issue lists

    public struct Status: Codable, Equatable, Sendable {
        public let id: String
        public let category: String
        public let color: String
        public let sortOrder: Double

        public init(id: String, category: String, color: String, sortOrder: Double) {
            self.id = id
            self.category = category
            self.color = color
            self.sortOrder = sortOrder
        }
    }

    public struct Issue: Codable, Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        public let statusId: String
        public let boardId: String
        public let teamId: String
        public let priority: String
        public let sortOrder: Double
        public let assigneeId: String?
        public let updatedAt: String

        public init(
            id: String, identifier: String, title: String, statusId: String, boardId: String, teamId: String,
            priority: String, sortOrder: Double, assigneeId: String?, updatedAt: String
        ) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.statusId = statusId
            self.boardId = boardId
            self.teamId = teamId
            self.priority = priority
            self.sortOrder = sortOrder
            self.assigneeId = assigneeId
            self.updatedAt = updatedAt
        }
    }

    public struct IssueRow: Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        public let statusCategory: String
        public let statusColor: String

        public init(id: String, identifier: String, title: String, statusCategory: String, statusColor: String) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.statusCategory = statusCategory
            self.statusColor = statusColor
        }
    }

    /// `count` = every matching row (the header's), `rows` = the first `max`.
    public struct List<Row: Equatable & Sendable>: Equatable, Sendable {
        public let count: Int
        public let rows: [Row]

        public init(count: Int, rows: [Row]) {
            self.count = count
            self.rows = rows
        }
    }

    private static func openIssues(_ issues: [Issue], _ statuses: [Status]) -> [(issue: Issue, status: Status)] {
        let byId = Dictionary(statuses.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return issues.compactMap { issue in
            guard let status = byId[issue.statusId], openCategories.contains(status.category) else { return nil }
            return (issue, status)
        }
    }

    private static func row(_ pair: (issue: Issue, status: Status)) -> IssueRow {
        IssueRow(
            id: pair.issue.id, identifier: pair.issue.identifier, title: pair.issue.title,
            statusCategory: pair.status.category, statusColor: pair.status.color
        )
    }

    private static func rank(_ list: [String], _ value: String) -> Int {
        list.firstIndex(of: value) ?? list.count
    }

    /// One board's open issues in its reading order: category, status
    /// sortOrder, issue sortOrder, identifier.
    public static func boardWidget(boardId: String, statuses: [Status], issues: [Issue], max: Int) -> List<IssueRow> {
        let rows = openIssues(issues.filter { $0.boardId == boardId }, statuses).sorted { a, b in
            let ca = rank(categoryOrder, a.status.category), cb = rank(categoryOrder, b.status.category)
            if ca != cb { return ca < cb }
            if a.status.sortOrder != b.status.sortOrder { return a.status.sortOrder < b.status.sortOrder }
            if a.issue.sortOrder != b.issue.sortOrder { return a.issue.sortOrder < b.issue.sortOrder }
            return a.issue.identifier < b.issue.identifier
        }
        return List(count: rows.count, rows: rows.prefix(Swift.max(0, max)).map(row))
    }

    /// The viewer's open issues in the picked teams (none = every team):
    /// priority, updatedAt desc, identifier.
    public static func myIssuesWidget(
        me: String, teamIds: [String], statuses: [Status], issues: [Issue], max: Int
    ) -> List<IssueRow> {
        let mine = issues.filter { $0.assigneeId == me && (teamIds.isEmpty || teamIds.contains($0.teamId)) }
        let rows = openIssues(mine, statuses).sorted { a, b in
            let pa = rank(priorityOrder, a.issue.priority), pb = rank(priorityOrder, b.issue.priority)
            if pa != pb { return pa < pb }
            if a.issue.updatedAt != b.issue.updatedAt { return a.issue.updatedAt > b.issue.updatedAt }
            return a.issue.identifier < b.issue.identifier
        }
        return List(count: rows.count, rows: rows.prefix(Swift.max(0, max)).map(row))
    }

    // MARK: - Needs you

    /// One `NeedsYou.Row` as far as the widget reads it.
    public struct NeedsYouInput: Codable, Equatable, Sendable {
        public let messageId: String
        public let sessionId: String
        public let rootId: String
        /// `plan` | `question` | `pr`.
        public let kind: String
        /// `plan` | `card` | `results` | `chat`.
        public let opens: String
        public let title: String
        public let age: String

        public init(
            messageId: String, sessionId: String, rootId: String, kind: String, opens: String, title: String,
            age: String
        ) {
            self.messageId = messageId
            self.sessionId = sessionId
            self.rootId = rootId
            self.kind = kind
            self.opens = opens
            self.title = title
            self.age = age
        }
    }

    public struct NeedsYouRow: Equatable, Sendable {
        public let messageId: String
        public let sessionId: String
        public let rootId: String
        public let opens: String
        /// A concept icon.
        public let icon: String
        public let title: String
        public let age: String

        public init(
            messageId: String, sessionId: String, rootId: String, opens: String, icon: String, title: String,
            age: String
        ) {
            self.messageId = messageId
            self.sessionId = sessionId
            self.rootId = rootId
            self.opens = opens
            self.icon = icon
            self.title = title
            self.age = age
        }
    }

    private static let kindIcon = ["plan": Icons.plan, "question": Icons.question, "pr": Icons.pr]

    /// THE Needs you rows (needs-you.json order), an encrypted chat's text
    /// withheld.
    public static func needsYouWidget(rows: [NeedsYouInput], sealedRootIds: [String], max: Int) -> List<NeedsYouRow> {
        let sealed = Set(sealedRootIds)
        return List(count: rows.count, rows: rows.prefix(Swift.max(0, max)).map { row in
            NeedsYouRow(
                messageId: row.messageId,
                sessionId: row.sessionId,
                rootId: row.rootId,
                opens: row.opens,
                icon: kindIcon[row.kind] ?? Icons.question,
                title: sealed.contains(row.rootId) ? Copy.sealedRow : row.title,
                age: row.age
            )
        })
    }

    /// results-tab.json's age (`ResultsTab.age`), restated so a widget's
    /// timeline re-reads it as the clock moves.
    public static func age(_ createdAtMs: Double, now: Double) -> String {
        let minutes = Int((Swift.max(0, now - createdAtMs) / 60_000).rounded(.down))
        if minutes < 1 { return "now" }
        if minutes < 60 { return "\(minutes)m" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours)h" }
        return "\(hours / 24)d"
    }

    // MARK: - The snapshot (app → app group → widgets)

    /// What the app writes for the widgets: the ACTIVE account's open issues,
    /// boards, teams and needs-you rows. Glyphs are asset names the app
    /// resolved (the widget carries the same imagesets, no registry).
    public struct Snapshot: Codable, Equatable, Sendable {
        public struct Board: Codable, Equatable, Sendable {
            public let id: String
            public let teamId: String
            public let name: String
            public let glyph: String
            public let color: String?
            public let sortOrder: Double

            public init(id: String, teamId: String, name: String, glyph: String, color: String?, sortOrder: Double) {
                self.id = id
                self.teamId = teamId
                self.name = name
                self.glyph = glyph
                self.color = color
                self.sortOrder = sortOrder
            }
        }

        public struct Team: Codable, Equatable, Sendable {
            public let id: String
            public let name: String

            public init(id: String, name: String) {
                self.id = id
                self.name = name
            }
        }

        public struct NeedsYouItem: Codable, Equatable, Sendable {
            public let row: NeedsYouInput
            public let createdAtMs: Double

            public init(row: NeedsYouInput, createdAtMs: Double) {
                self.row = row
                self.createdAtMs = createdAtMs
            }
        }

        /// The header and row glyphs (asset names) by concept.
        public struct Glyphs: Codable, Equatable, Sendable {
            public let myIssues: String
            public let needsYou: String
            public let plan: String
            public let question: String
            public let pr: String

            public init(myIssues: String, needsYou: String, plan: String, question: String, pr: String) {
                self.myIssues = myIssues
                self.needsYou = needsYou
                self.plan = plan
                self.question = question
                self.pr = pr
            }

            public func glyph(concept: String) -> String {
                switch concept {
                case Icons.plan: plan
                case Icons.pr: pr
                case Icons.myIssues: myIssues
                case Icons.needsYou: needsYou
                default: question
                }
            }
        }

        /// nil = signed out.
        public let me: String?
        public let teams: [Team]
        public let boards: [Board]
        public let statuses: [Status]
        /// Every open issue (a status in `openCategories`).
        public let issues: [Issue]
        /// A status's glyph (asset name: the category's, started by its clock).
        public let statusGlyphs: [String: String]
        public let needsYou: [NeedsYouItem]
        public let sealedRootIds: [String]
        public let glyphs: Glyphs?

        public init(
            me: String?, teams: [Team], boards: [Board], statuses: [Status], issues: [Issue],
            statusGlyphs: [String: String], needsYou: [NeedsYouItem], sealedRootIds: [String], glyphs: Glyphs?
        ) {
            self.me = me
            self.teams = teams
            self.boards = boards
            self.statuses = statuses
            self.issues = issues
            self.statusGlyphs = statusGlyphs
            self.needsYou = needsYou
            self.sealedRootIds = sealedRootIds
            self.glyphs = glyphs
        }

        public static let signedOut = Snapshot(
            me: nil, teams: [], boards: [], statuses: [], issues: [], statusGlyphs: [:], needsYou: [],
            sealedRootIds: [], glyphs: nil
        )

        /// The needs-you rows with their ages at `now`.
        public func needsYouRows(now: Date) -> [NeedsYouInput] {
            let nowMs = now.timeIntervalSince1970 * 1000
            return needsYou.map { item in
                let row = item.row
                return NeedsYouInput(
                    messageId: row.messageId, sessionId: row.sessionId, rootId: row.rootId, kind: row.kind,
                    opens: row.opens, title: row.title, age: HomeWidgets.age(item.createdAtMs, now: nowMs)
                )
            }
        }
    }

    /// PRIVACY: the snapshot's needs-you items with every encrypted chat's
    /// text already `copy.sealedRow`, so no card text of a sealed root ever
    /// lands in the app group (the widget's own sealing stays as a second line).
    public static func sealed(
        _ items: [Snapshot.NeedsYouItem], sealedRootIds: [String]
    ) -> [Snapshot.NeedsYouItem] {
        let sealed = Set(sealedRootIds)
        return items.map { item in
            let row = item.row
            guard sealed.contains(row.rootId) else { return item }
            return Snapshot.NeedsYouItem(
                row: NeedsYouInput(
                    messageId: row.messageId, sessionId: row.sessionId, rootId: row.rootId, kind: row.kind,
                    opens: row.opens, title: Copy.sealedRow, age: row.age
                ),
                createdAtMs: item.createdAtMs
            )
        }
    }

    private static let snapshotFile = "home-widgets.json"

    private static var snapshotURL: URL? {
        FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: SharedAppGroup.suiteName)?
            .appendingPathComponent(snapshotFile)
    }

    /// The last snapshot the app wrote, nil = none yet (never signed in here).
    public static func readSnapshot() -> Snapshot? {
        guard let url = snapshotURL, let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(Snapshot.self, from: data)
    }

    /// Writes the snapshot; false = unchanged (nothing to reload).
    @discardableResult
    public static func writeSnapshot(_ snapshot: Snapshot) -> Bool {
        guard let url = snapshotURL else { return false }
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        guard let data = try? encoder.encode(snapshot) else { return false }
        if let old = try? Data(contentsOf: url), old == data { return false }
        return (try? data.write(to: url, options: .atomic)) != nil
    }

    // MARK: - Taps (app-shortcuts.json OPENING: the back stack rule)

    /// The `exponential://` links the widgets open; `AppNavigator` routes them.
    public enum Links {
        public static let scheme = "exponential"

        public static func board(_ id: String) -> URL { url("board/\(id)") }
        public static var myIssues: URL { url("my-issues") }
        public static var needsYou: URL { url("needs-you") }
        public static func issue(_ id: String) -> URL { url("issue/\(id)") }
        /// A needs-you row: its root opened by `opens` on `message`.
        public static func needsYouRow(_ row: NeedsYouRow) -> URL {
            var components = URLComponents()
            components.scheme = scheme
            components.host = "session"
            components.path = "/\(row.rootId)"
            components.queryItems = [
                URLQueryItem(name: "opens", value: row.opens),
                URLQueryItem(name: "message", value: row.messageId),
            ]
            return components.url ?? url("needs-you")
        }
        /// A signed-out widget: the app on its start screen.
        public static var app: URL { url("home") }

        private static func url(_ rest: String) -> URL {
            URL(string: "\(scheme)://\(rest)")!
        }
    }
}
