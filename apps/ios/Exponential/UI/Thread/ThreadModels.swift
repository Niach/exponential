import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-1284: the Thread screen's synced inputs — ONE observation over the
/// thread's rows (`ThreadRows.load`), re-read on every write to any table it
/// touches, plus the 30 s liveness clock (a heartbeat STOPPING is the absence
/// of a write). The shape delivers the thread on its own; a deep link to a
/// thread the shapes have not delivered yet reads `threads.get` once and
/// mirrors the rows in (`ThreadDto.entity()`).
@MainActor @Observable
final class ThreadScreenModel {
    private(set) var rows: ThreadRows?
    /// Whether the thread is known to be MISSING: not in the store and
    /// `threads.get` refused it (or it vanished after it was shown) — the
    /// `thread-not-found` state. Skeleton until either the rows or this.
    private(set) var missing = false
    private(set) var now = Date()
    /// Item ids already on screen when the screen first drew them — a child
    /// linked AFTER that fades in with a ring once (brief §1).
    private(set) var seenItemIds: Set<String>?

    let threadId: String
    private let accountId: String
    private let db: DatabaseManager
    @ObservationIgnored private var observationTask: Task<Void, Never>?
    @ObservationIgnored private var clockTask: Task<Void, Never>?
    @ObservationIgnored private var fetchedOnce = false

    init(threadId: String, accountId: String, db: DatabaseManager) {
        self.threadId = threadId
        self.accountId = accountId
        self.db = db
    }

    var view: ThreadViewModel? { rows?.view }

    func start(api: ThreadsApi) {
        startObserving(api: api)
        guard clockTask == nil else { return }
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard !Task.isCancelled else { return }
                self?.now = Date()
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        clockTask?.cancel()
        clockTask = nil
    }

    /// Whether `itemId` was linked while the screen was up (its first render
    /// is the fade-in with a ring), else false.
    func isFresh(_ item: ThreadItemEntity) -> Bool {
        guard let seenItemIds, !seenItemIds.contains(item.id) else { return false }
        return ThreadViews.isFresh(createdAt: item.createdAt, now: Date())
    }

    private func startObserving(api: ThreadsApi) {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let threadId = threadId
        // T14: the read also says whether any `thread_items` row names the
        // id — the items and the thread arrive on separate shapes, so an
        // item first means the thread row is on its way, not missing.
        let observation = ValueObservation.tracking { db -> ThreadRead in
            let rows = try ThreadRows.load(db: db, threadId: threadId)
            if rows != nil { return ThreadRead(rows: rows, named: true) }
            let named = try ThreadItemEntity.filter(Column("thread_id") == threadId).fetchCount(db) > 0
            return ThreadRead(rows: rows, named: named)
        }
        observationTask = Task { [weak self] in
            // The GRDB stream is one-shot (EXP-410): re-subscribe on error so
            // a busy database never freezes the thread at its last value.
            while !Task.isCancelled {
                do {
                    for try await read in observation.values(in: pool) {
                        guard let self else { return }
                        let rows = read.rows
                        if self.seenItemIds == nil, let rows {
                            self.seenItemIds = Set(rows.items.map(\.id))
                        }
                        self.rows = rows
                        if rows != nil {
                            self.missing = false
                        } else {
                            // ONE `threads.get` before concluding anything;
                            // mirrored rows re-emit through this stream. Only
                            // a refusal (or a later miss) with NO item naming
                            // the id is not-found (T14: skeleton until then).
                            let coming = await self.fetchMissing(api: api, pool: pool)
                            self.missing = !coming && !read.named
                        }
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
    }

    /// A thread the shapes have not delivered (a deep link racing the sync):
    /// one `threads.get`, mirrored into the store; the observation then
    /// re-reads it. Returns whether rows are on their way; a refusal (or a
    /// second miss) = false, the screen's `thread-not-found` state.
    private func fetchMissing(api: ThreadsApi, pool: DatabasePool) async -> Bool {
        guard !fetchedOnce else { return false }
        fetchedOnce = true
        guard let detail = try? await api.get(accountId: accountId, id: threadId) else { return false }
        do {
            try await pool.write { db in
                try detail.thread.entity().save(db)
                for item in detail.items { try item.entity().save(db) }
            }
            return true
        } catch {
            return false
        }
    }
}

/// One read of the thread screen: its rows, and whether any item names it.
struct ThreadRead: Sendable {
    let rows: ThreadRows?
    let named: Bool
}

/// EXP-1284: the Agent page's thread lists (brief §5, review T6/T4) —
/// every synced thread of EVERY member team (phones band the rows per team),
/// built into its view and split by `partitionThreadList(entries, teamId:
/// nil)` (fixture `partitions`): Running = a live session of mine, Recent =
/// my other threads (a session of mine, or created by me). T4: a live
/// session of mine whose thread item has not synced (or whose placement
/// failed) is never hidden: it lists under Running as a plain session row.
@MainActor @Observable
final class ThreadListModel {
    struct Entry: Identifiable, ThreadListEntry {
        let rows: ThreadRows
        let view: ThreadViewModel
        let mineLive: Bool
        let mine: Bool
        let liveAgent: String?
        var id: String { rows.thread.id }
        var threadId: String { rows.thread.id }
        var updatedAt: String { rows.thread.updatedAt }
        var teamId: String { rows.thread.teamId }
    }

    /// A live session of mine in no thread (yet): drawn as THE session row.
    struct Orphan: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        let batchIssues: [IssueEntity]
        let device: SessionDevicePresentation
        let deviceIcon: String
        var id: String { session.id }
        var teamId: String { session.teamId }
    }

    /// One GRDB pass: every thread, plus my live-status sessions that no
    /// thread item names, their issues and the device rows.
    struct Snapshot: Sendable {
        let threads: [ThreadRows]
        let orphanSessions: [CodingSessionEntity]
        let orphanIssues: [IssueEntity]
        let devices: [DeviceEntity]
    }

    private(set) var running: [Entry] = []
    private(set) var recent: [Entry] = []
    private(set) var orphans: [Orphan] = []
    private(set) var loaded = false

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    private var snapshot: Snapshot?
    @ObservationIgnored private var observationTask: Task<Void, Never>?
    @ObservationIgnored private var clockTask: Task<Void, Never>?

    init(accountId: String, userId: String?, db: DatabaseManager) {
        self.accountId = accountId
        self.userId = userId
        self.db = db
    }

    func start() {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let userId = userId
        let observation = ValueObservation.tracking { db -> Snapshot in
            let threads = try ThreadRows.load(db: db, teamIds: nil)
            guard let userId else {
                return Snapshot(threads: threads, orphanSessions: [], orphanIssues: [], devices: [])
            }
            let threaded = Set(try String.fetchAll(
                db, sql: "SELECT \"session_id\" FROM \"thread_items\" WHERE \"session_id\" IS NOT NULL"
            ))
            let mineLiveStatus = try CodingSessionEntity
                .filter(Column("user_id") == userId)
                .filter(
                    [DomainContract.codingSessionStatusRunning, DomainContract.codingSessionStatusInReview]
                        .contains(Column("status"))
                        || Column("ended_by") == DomainContract.codingSessionEndedByStale
                )
                .fetchAll(db)
                .filter { !threaded.contains($0.id) }
            var issueIds = Set<String>()
            for session in mineLiveStatus {
                if let issueId = session.issueId { issueIds.insert(issueId) }
                issueIds.formUnion(BatchRun.issueIds(session.batchIssueIds))
            }
            let issues: [IssueEntity] = try IssueEntity
                .filter(Array(issueIds).contains(Column("id")))
                .fetchAll(db)
            return Snapshot(
                threads: threads,
                orphanSessions: mineLiveStatus,
                orphanIssues: issues,
                devices: try DeviceEntity.fetchAll(db)
            )
        }
        observationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await snapshot in observation.values(in: pool) {
                        guard let self else { return }
                        self.snapshot = snapshot
                        self.loaded = true
                        self.rebuild()
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
        // GRDB only re-fires on writes: a heartbeat STOPPING is the absence
        // of one, so the liveness rule re-runs on a clock too.
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                guard !Task.isCancelled else { return }
                self?.rebuild()
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        clockTask?.cancel()
        clockTask = nil
    }

    private func rebuild() {
        guard let snapshot else { return }
        let now = Date()
        let entries = snapshot.threads.map { rows in
            Entry(
                rows: rows,
                view: rows.buildView(now: now),
                mineLive: rows.mineLive(userId: userId, now: now),
                mine: rows.mine(userId: userId),
                liveAgent: rows.newestLiveAgent(now: now)
            )
        }
        // Phones: every team, banded per team by the view.
        let parts = ThreadViews.partition(entries, teamId: nil)
        running = parts.running
        recent = parts.recent
        let steerDevices = snapshot.devices.map {
            SteerDevice(entity: $0, now: now, currentUserId: userId)
        }
        orphans = snapshot.orphanSessions
            .filter { CodingSessionLiveness.isLive($0, now: now) }
            .sorted { a, b in
                let ta = WireTimestamps.parse(a.startedAt)?.timeIntervalSince1970 ?? 0
                let tb = WireTimestamps.parse(b.startedAt)?.timeIntervalSince1970 ?? 0
                return ta != tb ? ta > tb : a.id < b.id
            }
            .map { session in
                Orphan(
                    session: session,
                    issue: session.issueId.flatMap { id in snapshot.orphanIssues.first { $0.id == id } },
                    batchIssues: BatchRun.issues(session, issues: snapshot.orphanIssues),
                    device: SessionDevicePresentation.resolve(
                        session: session, devices: snapshot.devices, now: now
                    ),
                    deviceIcon: SessionListRow.deviceIcon(session, devices: steerDevices)
                )
            }
    }
}

/// EXP-1284: the threads holding one issue (brief §6: the issue page's
/// `Threads` row), newest activity first.
@MainActor @Observable
final class IssueThreadsModel {
    struct Item: Identifiable {
        let rows: ThreadRows
        let view: ThreadViewModel
        var id: String { rows.thread.id }
    }

    private(set) var threads: [Item] = []

    let issueId: String
    private let accountId: String
    private let db: DatabaseManager
    @ObservationIgnored private var observationTask: Task<Void, Never>?

    init(accountId: String, issueId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
    }

    func start() {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let issueId = issueId
        let observation = ValueObservation.tracking { db in
            let ids = try ThreadQueries.threadsHoldingIssue(db: db, issueId: issueId).map(\.id)
            return try ThreadRows.load(db: db, threadIds: ids)
        }
        observationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        self.threads = rows.map { Item(rows: $0, view: $0.view) }
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
    }
}

/// EXP-1284: the ONE thread a run sits in (`ThreadQueries.threadOfSession`),
/// observed per run id — what a run's screens read to offer the thread
/// (brief §7) and to show the THREAD's Guide (`guideOfRun`, brief §8).
/// `follow` re-points it; nil = the run's item has not synced (or a legacy
/// run predating threads).
@MainActor @Observable
final class RunThreadLookup {
    private(set) var sessionId: String?
    private(set) var thread: ThreadEntity?

    private let accountId: String
    private let db: DatabaseManager
    @ObservationIgnored private var task: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    /// The thread of `sessionId` (nil clears). Re-arms only on a change, or
    /// after `stop`.
    func follow(_ sessionId: String?) {
        guard sessionId != self.sessionId || task == nil else { return }
        task?.cancel()
        task = nil
        self.sessionId = sessionId
        if thread != nil { thread = nil }
        guard let sessionId, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try ThreadQueries.threadOfSession(db: db, sessionId: sessionId)
        }
        task = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await thread in observation.values(in: pool) {
                        guard let self else { return }
                        self.thread = thread
                    }
                    return
                } catch is CancellationError {
                    return
                } catch {
                    try? await Task.sleep(for: .seconds(1))
                }
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }
}
