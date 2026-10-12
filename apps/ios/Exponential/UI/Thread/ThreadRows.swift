import ExpCore
import Foundation
import GRDB
import Observation

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

    /// T9/T-N5: the status the model reads = the HEARTBEAT rule the session
    /// lists use (`CodingSessionLiveness.isLive`): a live row keeps its
    /// status (a sweep end that the host heartbeats back reads `running`), a
    /// dead heartbeat reads `ended` — so nothing calls a crashed host's
    /// session live, and the reply box resumes it instead of sending into it.
    static func liveStatus(_ session: CodingSessionEntity, now: Date) -> String {
        guard CodingSessionLiveness.isLive(session, now: now) else {
            return DomainContract.codingSessionStatusEnded
        }
        return session.status == DomainContract.codingSessionStatusEnded
            ? DomainContract.codingSessionStatusRunning
            : session.status
    }

    /// The shared model's runs (`ThreadView.swift`), status through the
    /// heartbeat rule.
    func runsForView(now: Date = Date()) -> [ThreadViewRun] {
        sessions.map { session in
            let live = Self.liveStatus(session, now: now) != DomainContract.codingSessionStatusEnded
            return ThreadViewRun(
                id: session.id,
                name: name(of: session),
                userId: session.userId,
                deviceLabel: device(for: session, now: now).label,
                status: Self.liveStatus(session, now: now),
                needsInput: live && session.needsInput,
                agentBusy: live && session.agentBusy,
                issueId: session.issueId,
                parentSessionId: session.parentSessionId,
                resumedFromId: session.resumedFromId,
                startedAt: session.startedAt,
                endedAt: session.endedAt
            )
        }
    }

    var viewRuns: [ThreadViewRun] { runsForView() }

    /// THE thread view (fixture-locked ×4).
    func buildView(now: Date) -> ThreadViewModel {
        ThreadViews.build(
            thread: ThreadViewThread(thread),
            items: items.map { ThreadViewItem($0) },
            issues: issues.map { ThreadViewIssue($0) },
            runs: runsForView(now: now)
        )
    }

    var view: ThreadViewModel { buildView(now: Date()) }

    /// The viewer owns a LIVE session in it (heartbeat-live, like the Agent
    /// tab's dot) — `partitionThreadList`'s Running.
    func mineLive(userId: String?, now: Date = Date()) -> Bool {
        guard let userId else { return false }
        return sessions.contains { $0.userId == userId && CodingSessionLiveness.isLive($0, now: now) }
    }

    /// T6: the viewer has a session in it or created it — Recent lists only
    /// these (`partitionThreadList`).
    func mine(userId: String?) -> Bool {
        guard let userId else { return false }
        return thread.createdBy == userId || sessions.contains { $0.userId == userId }
    }

    /// The agent of the newest live session (the list row's working mark).
    func newestLiveAgent(now: Date = Date()) -> String? {
        sessions.last { CodingSessionLiveness.isLive($0, now: now) }?.agent
    }

    var liveAgent: String? { newestLiveAgent() }

    /// T8/T-N6: the PR the thread's Guide targets (`threadPrTarget` ×4: the
    /// newest OPEN PR on its sessions and issues, else the newest merged).
    var prTarget: ThreadViews.PrTarget? {
        ThreadViews.prTarget(sessions: sessions, issues: linkedIssues)
    }

    /// T-N8: the thread's Guide, else its newest session's row.
    var guide: String? {
        ThreadViews.threadGuide(threadResults: thread.results, sessions: sessions)
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

/// EXP-1284 (review T11/T-N7/T-N9): asks the thread page already on the
/// stack to show one child — the navigator pops back to it instead of
/// pushing a second page of the same thread, then lands it here: a start →
/// its Conversation, a "New issue in this thread" → the new issue's tab.
/// A reference box like `PushRouteAction` (ONE instance in the navigator's
/// state); `nonce` makes a repeated request for the same page a new one.
@Observable
final class ThreadLanding: @unchecked Sendable {
    struct Request: Equatable {
        let threadId: String
        let page: ThreadPage
        let nonce: UUID
    }

    private(set) var request: Request?

    init() {}

    func land(threadId: String, page: ThreadPage) {
        request = Request(threadId: threadId, page: page, nonce: UUID())
    }
}
