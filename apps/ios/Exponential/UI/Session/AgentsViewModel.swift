import ExpCore
import Foundation
import GRDB

/// Backs the Agents tab: every running coding session in the active account
/// (the synced `coding_sessions` shape), joined to its issue for display.
/// Desktop is the only session runner — this list is the mobile window into
/// what is coding right now.
@MainActor @Observable
final class AgentsViewModel {
    struct Row: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        var id: String { session.id }
    }

    var rows: [Row] = []

    private let accountId: String
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var sessionTask: Task<Void, Never>?
    private var issueTask: Task<Void, Never>?

    private var sessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("status") == DomainContract.codingSessionStatusRunning)
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.sessions = sessions
                    self?.rebuild()
                }
            } catch {}
        }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                    self?.rebuild()
                }
            } catch {}
        }
    }

    func stopObserving() {
        sessionTask?.cancel()
        sessionTask = nil
        issueTask?.cancel()
        issueTask = nil
    }

    private func rebuild() {
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        rows = sessions
            .sorted { $0.startedAt > $1.startedAt }
            // issueId is nil for release-scoped orchestrator sessions (EXP-56)
            // — those rows render without an issue link.
            .map { Row(session: $0, issue: $0.issueId.flatMap { issuesById[$0] }) }
    }
}
