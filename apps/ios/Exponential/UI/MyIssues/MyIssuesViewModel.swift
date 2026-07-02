import ExpUI
import ExpCore
import Foundation
import GRDB

/// "My Issues" — the fixed cross-project view (masterplan §5a): every issue in
/// the active account assigned to the signed-in user, grouped by status.
/// Mirrors `IssueListViewModel`'s GRDB observations minus the project
/// predicate; no filter bar / saved views by design (fixed built-in view).
@MainActor @Observable
final class MyIssuesViewModel {
    var issues: [IssueEntity] = []
    var projects: [ProjectEntity] = []

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository
    // Both observation loops are stored and cancelled individually: a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    private var issueTask: Task<Void, Never>?
    private var projectTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, auth: AuthRepository) {
        self.accountId = accountId
        self.db = db
        self.auth = auth
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        guard let userId = auth.userId else { return }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("assignee_id") == userId)
                .fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues.filter { $0.archivedAt == nil }
                }
            } catch {}
        }

        // Projects resolve each row's project prefix/name (rows span projects).
        let projectObservation = ValueObservation.tracking { db in
            try ProjectEntity.fetchAll(db)
        }
        projectTask = Task { [weak self] in
            do {
                for try await projects in projectObservation.values(in: pool) {
                    self?.projects = projects
                }
            } catch {}
        }
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        projectTask?.cancel()
        projectTask = nil
    }

    func issuesForStatus(_ status: IssueStatus) -> [IssueEntity] {
        issues
            .filter { IssueStatus.from($0.status) == status }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    func project(forId id: String) -> ProjectEntity? {
        projects.first { $0.id == id }
    }
}
