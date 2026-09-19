import ExpCore
import Foundation
import GRDB

/// EXP-893: what the Work screen is ABOUT — an issue, or a session. A session
/// subject learns its issue off the synced row (a batch, action or chat run
/// has none, so it stays issue-less).
enum WorkSubject: Hashable {
    case issue(id: String)
    case session(id: String)
}

/// EXP-893: the Work screen's synced inputs that are neither the issue's own
/// view model nor the steer socket — the subject's rows of mine on the issue
/// (EXP-886's run switcher, moved here from `AgentSessionModel`), the bound
/// session row for a session subject, and the devices rows that name each
/// run's machine and decide whether an ended run can be resumed.
///
/// Pure derivations (`WorkFaces.codingTarget`, `PastRuns.issueRuns`,
/// `RunResume.target`) run over these rows; the screen holds the face.
@MainActor @Observable
final class WorkSubjectModel {

    /// One run of mine on the issue: the session plus its machine's CURRENT
    /// label (the live devices row, not the start-time snapshot).
    struct IssueRun: Identifiable {
        let session: CodingSessionEntity
        let device: SessionDevicePresentation
        var id: String { session.id }
    }

    /// The issue the subject is about — the issue subject's own id, or the
    /// session subject's `issue_id` once its row synced. Nil for an
    /// issue-less run.
    private(set) var issueId: String?
    /// A session subject's own synced row. Nil for an issue subject, and once
    /// the row is gone (swept, or out of this client's scope).
    private(set) var boundSession: CodingSessionEntity?
    /// Whether a session subject's row has been READ at least once — before
    /// that the screen shows nothing rather than an issue-less shell.
    private(set) var boundResolved = false
    /// The row the screen currently SHOWS, observed by id — a resumed or
    /// switched run's continuation is neither the bound row nor (for an
    /// issue-less subject) one of the issue's runs, so it needs its own read.
    private(set) var shownRow: CodingSessionEntity?
    private var shownRowId: String?
    /// The issue's runs of mine (`PastRuns.issueRuns`: live first, then
    /// newest end first, uncapped), each with its host machine as it presents
    /// now — the switcher's rows. Empty for an issue-less subject.
    private(set) var issueRuns: [IssueRun] = []
    /// EXP-876: the issues the shown BATCH run covers — what NAMES an
    /// issue-less run in this screen's header, where "Batch run" told two
    /// batches apart no better than it did in the list. Empty for every other
    /// subject; observed only while a batch is shown.
    private(set) var batchIssues: [IssueEntity] = []
    /// The 30s liveness clock — `codingTarget` and `resumeDevice` re-decide
    /// on it, since a heartbeat STOPPING is the absence of a write.
    private(set) var now = Date()

    private let accountId: String
    private let subject: WorkSubject
    private let currentUserId: String?
    private let db: DatabaseManager

    private var runRows: [CodingSessionEntity] = []
    private var deviceRows: [DeviceEntity] = []
    private var boundObservationTask: Task<Void, Never>?
    /// EXP-876: the covered-issue observation and the key it is armed for
    /// (the stored ids plus the branch — nothing else can change the set).
    private var batchObservationTask: Task<Void, Never>?
    private var batchKey: String?
    private var shownObservationTask: Task<Void, Never>?
    private var runObservationTask: Task<Void, Never>?
    private var deviceObservationTask: Task<Void, Never>?
    private var clockTask: Task<Void, Never>?

    init(accountId: String, subject: WorkSubject, currentUserId: String?, db: DatabaseManager) {
        self.accountId = accountId
        self.subject = subject
        self.currentUserId = currentUserId
        self.db = db
        if case let .issue(id) = subject { issueId = id }
    }

    // MARK: - Reads

    /// The synced row for a run this screen may show — one of the issue's own
    /// runs, or the bound session itself.
    func session(id: String) -> CodingSessionEntity? {
        if let row = runRows.first(where: { $0.id == id }) { return row }
        if boundSession?.id == id { return boundSession }
        if shownRow?.id == id { return shownRow }
        return nil
    }

    /// Point the shown-row observation at `id` (the screen's `shownSessionId`).
    func observeShown(id: String?) {
        guard shownRowId != id else { return }
        shownRowId = id
        shownObservationTask?.cancel()
        shownObservationTask = nil
        shownRow = nil
        startObservingShown()
    }

    /// The run the screen shows by default (`WorkFaces.codingTarget`): the
    /// bound run when it is mine and live, else my newest live run on the
    /// issue, else my newest run. An issue-less session subject shows itself.
    func codingTarget(boundId: String?) -> CodingSessionEntity? {
        guard let issueId else { return boundSession }
        return WorkFaces.codingTarget(
            runRows, issueId: issueId, boundId: boundId, me: currentUserId, now: now
        )
    }

    /// The machine a Resume of an ENDED run would go to (`RunResume.target`,
    /// the ×4 rule), or nil when none can take it.
    func resumeDevice(for session: CodingSessionEntity) -> SteerDevice? {
        let presented = deviceRows.map {
            SteerDevice(entity: $0, now: now, currentUserId: currentUserId)
        }
        return RunResume.target(for: session, devices: presented, currentUserId: currentUserId)
    }

    /// The host machine's current presentation for a run.
    func device(for session: CodingSessionEntity) -> SessionDevicePresentation {
        SessionDevicePresentation.resolve(session: session, devices: deviceRows, now: now)
    }

    // MARK: - Lifecycle

    /// Re-armed on every appear (a pushed child stops it on disappear).
    func start() {
        startObservingBound()
        startObservingShown()
        startObservingRuns()
        startObservingDevices()
        startClock()
    }

    func stop() {
        boundObservationTask?.cancel()
        boundObservationTask = nil
        batchObservationTask?.cancel()
        batchObservationTask = nil
        batchKey = nil
        shownObservationTask?.cancel()
        shownObservationTask = nil
        runObservationTask?.cancel()
        runObservationTask = nil
        deviceObservationTask?.cancel()
        deviceObservationTask = nil
        clockTask?.cancel()
        clockTask = nil
    }

    private func startObservingBound() {
        guard boundObservationTask == nil else { return }
        guard case let .session(sessionId) = subject else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.filter(Column("id") == sessionId).fetchOne(db)
        }
        boundObservationTask = Task { [weak self] in
            // The GRDB stream is one-shot (EXP-410): re-subscribe on error so
            // a busy database never freezes the row at its last value.
            while !Task.isCancelled {
                do {
                    for try await row in observation.values(in: pool) {
                        guard let self else { return }
                        self.boundSession = row
                        self.boundResolved = true
                        // EXP-876: the header names an issue-less bound run
                        // off its covered issues; the shown-row observation
                        // re-points this the moment the reader switches runs.
                        if self.shownRow == nil { self.observeBatch(for: row) }
                        if let issueId = row?.issueId, self.issueId != issueId {
                            self.issueId = issueId
                            self.runObservationTask?.cancel()
                            self.runObservationTask = nil
                            self.startObservingRuns()
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

    private func startObservingShown() {
        guard shownObservationTask == nil, let shownRowId else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity.filter(Column("id") == shownRowId).fetchOne(db)
        }
        shownObservationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await row in observation.values(in: pool) {
                        guard let self else { return }
                        self.shownRow = row
                        // EXP-876: follow the shown run — a switch between
                        // two batches re-points the covered-issue pool.
                        self.observeBatch(for: row)
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

    /// EXP-876: keep the covered-issue pool pointed at `session` — the issues
    /// a BATCH run is NAMED by, read by id (`batch_issue_ids`, what the run
    /// stored at start; EXP-972 dropped the branch-mates fallback). A
    /// non-batch subject clears it and observes nothing.
    private func observeBatch(for session: CodingSessionEntity?) {
        let batch = session.flatMap { BatchRun.isBatch($0) ? $0 : nil }
        let ids = BatchRun.issueIds(batch?.batchIssueIds)
        let key = ids.joined(separator: "\u{0}")
        guard batchKey != key else { return }
        batchKey = key
        batchObservationTask?.cancel()
        batchObservationTask = nil
        batchIssues = []
        guard !ids.isEmpty else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(ids.contains(Column("id")))
                .fetchAll(db)
        }
        batchObservationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        self.batchIssues = rows
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

    /// The issue's runs of mine. SQL narrows to the issue + user;
    /// `PastRuns.issueRuns` is the ×4 rule and re-applies the predicate and
    /// the ordering on the way out.
    private func startObservingRuns() {
        guard runObservationTask == nil else { return }
        guard let issueId, let currentUserId, !currentUserId.isEmpty else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("issue_id") == issueId)
                .filter(Column("user_id") == currentUserId)
                .fetchAll(db)
        }
        runObservationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        self.runRows = rows
                        self.rebuildIssueRuns()
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

    private func startObservingDevices() {
        guard deviceObservationTask == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try DeviceEntity.fetchAll(db)
        }
        deviceObservationTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    for try await rows in observation.values(in: pool) {
                        guard let self else { return }
                        self.deviceRows = rows
                        // A machine rename repaints the switcher's bylines.
                        self.rebuildIssueRuns()
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

    private func startClock() {
        guard clockTask == nil else { return }
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                guard let self, !Task.isCancelled else { return }
                self.now = Date()
                self.rebuildIssueRuns()
            }
        }
    }

    /// Re-derive the switcher's entries: the rows and the device labels move
    /// independently, so both observers call this.
    private func rebuildIssueRuns() {
        let now = Date()
        issueRuns = PastRuns.issueRuns(runRows, issueId: issueId, userId: currentUserId).map { row in
            IssueRun(
                session: row,
                device: SessionDevicePresentation.resolve(session: row, devices: deviceRows, now: now)
            )
        }
    }
}
