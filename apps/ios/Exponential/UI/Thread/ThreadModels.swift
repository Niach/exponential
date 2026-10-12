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
    /// Whether the thread has been READ at least once (skeleton until then).
    private(set) var resolved = false
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
        let observation = ValueObservation.tracking { db in
            try ThreadRows.load(db: db, threadId: threadId)
        }
        observationTask = Task { [weak self] in
            // The GRDB stream is one-shot (EXP-410): re-subscribe on error so
            // a busy database never freezes the thread at its last value.
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        if self.seenItemIds == nil, let rows {
                            self.seenItemIds = Set(rows.items.map(\.id))
                        }
                        self.rows = rows
                        self.resolved = true
                        if rows == nil { await self.fetchMissing(api: api, pool: pool) }
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
    /// re-reads it. A refusal leaves the screen on its empty state.
    private func fetchMissing(api: ThreadsApi, pool: DatabasePool) async {
        guard !fetchedOnce else { return }
        fetchedOnce = true
        guard let detail = try? await api.get(accountId: accountId, id: threadId) else { return }
        try? await pool.write { db in
            try detail.thread.entity().save(db)
            for item in detail.items { try item.entity().save(db) }
        }
    }
}

/// EXP-1284: the Agent page's thread lists (brief §5) — every synced thread,
/// built into its view, split by `partitionThreadList` into Running (a live
/// run of mine, across every member team, like the Agent tab's dot) and
/// Recent (the ACTIVE team's other threads, newest activity first).
@MainActor @Observable
final class ThreadListModel {
    struct Entry: Identifiable, ThreadListEntry {
        let rows: ThreadRows
        let view: ThreadViewModel
        let mineLive: Bool
        var id: String { rows.thread.id }
        var threadId: String { rows.thread.id }
        var updatedAt: String { rows.thread.updatedAt }
        var teamId: String { rows.thread.teamId }
    }

    private(set) var running: [Entry] = []
    private(set) var recent: [Entry] = []
    private(set) var loaded = false

    /// The active team — Recent is scoped to it (Running spans every team).
    var activeTeamId: String? {
        didSet { if oldValue != activeTeamId { rebuild() } }
    }

    private let accountId: String
    private let userId: String?
    private let db: DatabaseManager
    private var all: [ThreadRows] = []
    @ObservationIgnored private var observationTask: Task<Void, Never>?
    @ObservationIgnored private var clockTask: Task<Void, Never>?

    init(accountId: String, userId: String?, db: DatabaseManager) {
        self.accountId = accountId
        self.userId = userId
        self.db = db
    }

    func start() {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try ThreadRows.load(db: db, teamIds: nil)
        }
        observationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        self.all = rows
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
        let now = Date()
        let entries = all.map { rows in
            Entry(rows: rows, view: rows.view, mineLive: rows.mineLive(userId: userId, now: now))
        }
        let parts = ThreadViews.partition(entries)
        running = parts.running
        // A thread with nothing in it yet (no issue, no run) has nothing to
        // show on Home.
        recent = parts.recent.filter { entry in
            (activeTeamId == nil || entry.teamId == activeTeamId) && !entry.rows.items.isEmpty
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
