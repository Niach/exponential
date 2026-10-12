import ExpCore
import Foundation
import GRDB

/// EXP-1284: the tab the Thread screen shows — its CHILD STRIP is the pager
/// on phones (`Conversation` · one tab per issue · `Guide`; the "+" is a
/// trailing button, never a page). Hashable so it can ride `AppRoute`.
enum ThreadPage: Hashable {
    case conversation
    case issue(String)
    case guide
}

/// EXP-1284: everything one thread's view reads off the local store — the
/// thread, its items, the runs and issues they point at (plus the issues the
/// runs NAME: a run's own issue, a batch's covered ones) and the device rows
/// that label each run's machine. Read in ONE GRDB pass (`load`), so a
/// thread, its list row and an issue's Threads chip all derive from the same
/// snapshot.
struct ThreadRows: Equatable, Sendable {
    let thread: ThreadEntity
    let items: [ThreadItemEntity]
    /// The thread's runs, start order (Run 1 first).
    let sessions: [CodingSessionEntity]
    /// Every issue the thread or its runs name.
    let issues: [IssueEntity]
    let devices: [DeviceEntity]

    static func == (lhs: ThreadRows, rhs: ThreadRows) -> Bool {
        lhs.thread.id == rhs.thread.id
            && lhs.thread.updatedAt == rhs.thread.updatedAt
            && lhs.thread.title == rhs.thread.title
            && lhs.thread.results == rhs.thread.results
            && lhs.items.map(\.id) == rhs.items.map(\.id)
            && lhs.sessions.map(\.updatedAt) == rhs.sessions.map(\.updatedAt)
            && lhs.sessions.map(\.id) == rhs.sessions.map(\.id)
            && lhs.issues.map(\.updatedAt) == rhs.issues.map(\.updatedAt)
            && lhs.issues.map(\.id) == rhs.issues.map(\.id)
            && lhs.devices.map(\.label) == rhs.devices.map(\.label)
    }

    /// The thread's issue rows in link order (items that resolved).
    var linkedIssues: [IssueEntity] {
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return items
            .filter { $0.kind == DomainContract.threadItemKindIssue }
            .compactMap { $0.issueId.flatMap { byId[$0] } }
    }

    func session(id: String) -> CodingSessionEntity? {
        sessions.first { $0.id == id }
    }

    func issue(id: String) -> IssueEntity? {
        issues.first { $0.id == id }
    }

    /// The one item behind an issue chip (what Unlink removes, what the
    /// fresh-child ring times).
    func item(issueId: String) -> ThreadItemEntity? {
        items.first { $0.kind == DomainContract.threadItemKindIssue && $0.issueId == issueId }
    }

    /// The run's machine as it presents now (the live devices row's label).
    func device(for session: CodingSessionEntity, now: Date = Date()) -> SessionDevicePresentation {
        SessionDevicePresentation.resolve(session: session, devices: devices, now: now)
    }

    /// The run's NAME, the session identity subject (`sessionRowTitle`: the
    /// issue's title, an action's name, a chat's agent title, a batch's).
    func name(of session: CodingSessionEntity) -> String {
        sessionRowTitle(
            issue: session.issueId.flatMap { id in issues.first { $0.id == id } },
            session: session,
            batchIssues: BatchRun.issues(session, issues: issues)
        )
    }

    /// The shared model's runs (`ThreadView.swift`).
    var viewRuns: [ThreadViewRun] {
        sessions.map { session in
            ThreadViewRun(session, name: name(of: session), deviceLabel: device(for: session).label)
        }
    }

    /// THE thread view (fixture-locked ×4).
    var view: ThreadViewModel {
        ThreadViews.build(
            thread: ThreadViewThread(thread),
            items: items.map { ThreadViewItem($0) },
            issues: issues.map { ThreadViewIssue($0) },
            runs: viewRuns
        )
    }

    /// The viewer owns a LIVE run in it (heartbeat-live, like the Agent
    /// tab's dot) — `partitionThreadList`'s Running.
    func mineLive(userId: String?, now: Date = Date()) -> Bool {
        guard let userId else { return false }
        return sessions.contains { $0.userId == userId && CodingSessionLiveness.isLive($0, now: now) }
    }

    /// The agent of the newest live run (the list row's working mark).
    var liveAgent: String? {
        sessions.last { $0.status != DomainContract.codingSessionStatusEnded }?.agent
    }

    /// What the thread's Guide child merges (`ThreadViews.mergeTarget`).
    var mergeTarget: MergeTarget? {
        ThreadViews.mergeTarget(runs: sessions, issues: linkedIssues)
    }

    // MARK: - Reads

    /// One thread, or nil when its row is not (yet) synced.
    static func load(db: Database, threadId: String) throws -> ThreadRows? {
        try load(db: db, threadIds: [threadId]).first
    }

    /// Several threads in ONE pass, in the given id order (missing ones
    /// dropped).
    static func load(db: Database, threadIds: [String]) throws -> [ThreadRows] {
        guard !threadIds.isEmpty else { return [] }
        let threads = try ThreadEntity.filter(threadIds.contains(Column("id"))).fetchAll(db)
        return try assemble(db: db, threads: threads, order: threadIds)
    }

    /// Every thread in `teamIds` (nil = every synced team), newest activity
    /// first.
    static func load(db: Database, teamIds: [String]?) throws -> [ThreadRows] {
        var request = ThreadEntity.order(Column("updated_at").desc, Column("id").asc)
        if let teamIds {
            request = request.filter(teamIds.contains(Column("team_id")))
        }
        let threads = try request.fetchAll(db)
        return try assemble(db: db, threads: threads, order: threads.map(\.id))
    }

    private static func assemble(
        db: Database, threads: [ThreadEntity], order: [String]
    ) throws -> [ThreadRows] {
        guard !threads.isEmpty else { return [] }
        let threadIds = threads.map(\.id)
        let items = try ThreadItemEntity
            .filter(threadIds.contains(Column("thread_id")))
            .order(Column("sort_order").asc, Column("created_at").asc, Column("id").asc)
            .fetchAll(db)
        let sessionIds = Set(items.compactMap(\.sessionId))
        let sessions: [CodingSessionEntity] = try CodingSessionEntity
            .filter(Array(sessionIds).contains(Column("id")))
            .fetchAll(db)
        var issueIds = Set(items.compactMap(\.issueId))
        for session in sessions {
            if let issueId = session.issueId { issueIds.insert(issueId) }
            issueIds.formUnion(BatchRun.issueIds(session.batchIssueIds))
        }
        let issues: [IssueEntity] = try IssueEntity
            .filter(Array(issueIds).contains(Column("id")))
            .fetchAll(db)
        let devices = try DeviceEntity.fetchAll(db)

        let itemsByThread = Dictionary(grouping: items, by: \.threadId)
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let threadsById = Dictionary(threads.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })

        return order.compactMap { id in
            guard let thread = threadsById[id] else { return nil }
            let own = itemsByThread[id] ?? []
            let runs = own.compactMap { $0.sessionId.flatMap { sessionsById[$0] } }
                .sorted { a, b in
                    let ta = WireTimestamps.parse(a.startedAt)?.timeIntervalSince1970 ?? 0
                    let tb = WireTimestamps.parse(b.startedAt)?.timeIntervalSince1970 ?? 0
                    return ta != tb ? ta < tb : a.id < b.id
                }
            var named = Set(own.compactMap(\.issueId))
            for run in runs {
                if let issueId = run.issueId { named.insert(issueId) }
                named.formUnion(BatchRun.issueIds(run.batchIssueIds))
            }
            return ThreadRows(
                thread: thread,
                items: own,
                sessions: runs,
                issues: named.compactMap { issuesById[$0] }.sorted { $0.id < $1.id },
                devices: devices
            )
        }
    }
}
