import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-1295: the Thread screen's synced inputs — ONE observation over the
/// root's rows (`ThreadRows.load`), re-read on every write to any table it
/// touches, plus the 1 s clock the session cards' elapsed reads. The root
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
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled, let self else { return }
                // Only a live tree's elapsed moves.
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

    private func startObserving() {
        guard observationTask == nil, let pool = try? db.pool(forAccountId: accountId) else { return }
        let rootId = rootId
        let observation = ValueObservation.tracking { db in
            try ThreadRows.load(db: db, rootId: rootId)
        }
        observationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        self?.apply(rows)
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

    /// The clock: the elapsed captions move, the view is rebuilt once.
    private func tick() {
        now = Date()
        view = rows?.view(now: now)
    }

    private func apply(_ next: ThreadRows?) {
        guard let next else {
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
        if seenIssueIds == nil { seenIssueIds = Set(next.issues.map(\.id)) }
        rows = next
        tick()
    }
}
