import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-1295: the Thread screen's synced inputs — ONE observation over the
/// root's digest (`ThreadRows.signature`, #EXP-1380), the rows re-read and
/// the view rebuilt off the main actor only when it moved, plus the 1 s
/// clock the session cards' elapsed reads. The root
/// arrives through the `sessions` shape; until it does the screen shows its
/// skeleton, and a root that never arrives (or vanishes) is `missing`.
@MainActor @Observable
final class ThreadScreenModel {
    private(set) var rows: ThreadRows?
    private(set) var missing = false
    private(set) var now = Date()
    /// EXP-1358: the thread view, built ONCE per emission and per clock
    /// tick. It was a computed property, and the screen read it four or five
    /// times per body pass (every message's card re-parsed each time).
    private(set) var view: SessionThreadView?
    /// Issue ids already on screen when the screen first drew them — an
    /// issue linked AFTER that fades into the strip with a ring once.
    private(set) var seenIssueIds: Set<String>?

    let rootId: String
    private let accountId: String
    private let db: DatabaseManager
    @ObservationIgnored private var observationTask: Task<Void, Never>?
    @ObservationIgnored private var clockTask: Task<Void, Never>?
    @ObservationIgnored private var missingTask: Task<Void, Never>?

    init(rootId: String, accountId: String, db: DatabaseManager) {
        self.rootId = rootId
        self.accountId = accountId
        self.db = db
    }

    func start() {
        startObserving()
        guard clockTask == nil else { return }
        // #EXP-1380 PERF: the clock ticks only while a session of the chat
        // is live, and a tick re-reads the live cards' elapsed alone (the
        // session cards and the suggested tasks' started items)
        // (`ThreadRows.ticked`): nothing is rebuilt, and a tick that moves
        // no caption publishes nothing. The Working… line keeps its own
        // timeline (`WorkingIndicatorRow`).
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled, let self else { return }
                if self.rows?.sessions.contains(where: \.isLive) == true { self.tick() }
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        clockTask?.cancel()
        clockTask = nil
        missingTask?.cancel()
        missingTask = nil
    }

    func isFresh(issueId: String) -> Bool {
        guard let seenIssueIds else { return false }
        return !seenIssueIds.contains(issueId)
    }

    /// #EXP-1380: the observation tracks the root's DIGEST
    /// (`ThreadRows.signature`), not the snapshot: an unrelated write (any
    /// other chat's message, another team's PR) re-reads a few aggregates
    /// and stops at `removeDuplicates`. A real change loads the snapshot
    /// and builds the view OFF the main actor; the main actor only swaps
    /// them in.
    private func startObserving() {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let rootId = rootId
        let observation = ValueObservation
            .tracking { db in try ThreadRows.signature(db: db, rootId: rootId) }
            .removeDuplicates()
        observationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await signature in observation.values(in: pool) {
                        guard signature != nil else {
                            self?.apply(nil)
                            continue
                        }
                        let snapshot = try await Self.snapshot(pool: pool, rootId: rootId)
                        guard !Task.isCancelled else { return }
                        self?.apply(snapshot)
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

    /// The snapshot and its view, read and built off the main actor.
    nonisolated private static func snapshot(
        pool: DatabasePool, rootId: String
    ) async throws -> (rows: ThreadRows, view: SessionThreadView, now: Date)? {
        guard let rows = try await pool.read({ db in try ThreadRows.load(db: db, rootId: rootId) }) else { return nil }
        return await Task.detached(priority: .userInitiated) {
            let now = Date()
            return (rows, rows.view(now: now), now)
        }.value
    }

    /// The clock: the live cards' elapsed move (session cards, started
    /// suggestions), nothing else.
    private func tick() {
        guard let rows, let view else { return }
        let now = Date()
        guard let next = rows.ticked(view, now: now) else { return }
        self.now = now
        self.view = next
    }

    private func apply(_ snapshot: (rows: ThreadRows, view: SessionThreadView, now: Date)?) {
        guard let snapshot else {
            // Not synced yet, or gone: a grace period before the empty state.
            if rows != nil {
                rows = nil
                view = nil
                missing = true
            } else if missingTask == nil {
                missingTask = Task { [weak self] in
                    try? await Task.sleep(for: .seconds(6))
                    guard !Task.isCancelled, let self, self.rows == nil else { return }
                    self.missing = true
                }
            }
            return
        }
        missingTask?.cancel()
        missingTask = nil
        missing = false
        let next = snapshot.rows
        if seenIssueIds == nil { seenIssueIds = Set(next.issues.map(\.id)) }
        rows = next
        now = snapshot.now
        // An equal view (a write that moved nothing on screen) is not
        // re-published.
        if view != snapshot.view { view = snapshot.view }
    }
}

extension SessionThreadView {
    /// The same view over other rows (the clock's elapsed patch).
    func replacingRows(_ rows: [ThreadRow]) -> SessionThreadView {
        SessionThreadView(
            rootId: rootId, title: title, isPrivate: isPrivate, focusId: focusId, tabs: tabs, rows: rows,
            composer: composer, live: live, liveSessionId: liveSessionId, busy: busy, needsYou: needsYou,
            mainSessionId: mainSessionId
        )
    }
}
