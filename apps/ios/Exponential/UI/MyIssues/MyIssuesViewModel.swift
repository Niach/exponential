import ExpUI
import ExpCore
import Foundation
import GRDB

/// "My Issues" — the fixed cross-board view (masterplan §5a): every issue in
/// the active account assigned to the signed-in user, grouped by status.
/// Mirrors `IssueListViewModel`'s GRDB observations minus the board
/// predicate; no filter bar / saved views by design (fixed built-in view).
@MainActor @Observable
final class MyIssuesViewModel {
    var issues: [IssueEntity] = []
    var boards: [BoardEntity] = []

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository
    // Both observation loops are stored and cancelled individually: a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?

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

        // Boards resolve each row's board prefix/name (rows span boards).
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        boardTask = Task { [weak self] in
            do {
                for try await boards in boardObservation.values(in: pool) {
                    self?.boards = boards
                }
            } catch {}
        }
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        boardTask?.cancel()
        boardTask = nil
    }

    func issuesForStatus(_ status: IssueStatus) -> [IssueEntity] {
        // Canonical in-group ordering (EXP-38) — same comparator as the
        // board board, so "Assigned to you" matches every other surface.
        IssueSorting.sorted(
            issues.filter { IssueStatus.from($0.status) == status },
            status: status
        )
    }

    func board(forId id: String) -> BoardEntity? {
        boards.first { $0.id == id }
    }
}
