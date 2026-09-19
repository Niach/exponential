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
    /// EXP-980: every synced `issue_relations` row — `parent` rows nest the
    /// list, `blocks` rows badge it.
    var relations: [IssueRelationEntity] = []
    /// The issues at either end of a `blocks` row. An issue nobody assigned to
    /// the reader still counts as a blocker, so this pool is wider than the
    /// list itself.
    var relationIssues: [IssueEntity] = []

    private let accountId: String
    private let db: DatabaseManager
    private let auth: AuthRepository
    // Every observation loop is stored and cancelled individually: a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var relationTask: Task<Void, Never>?

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
                    guard let self else { return }
                    self.issues = issues
                    self.rebuildRows()
                }
            } catch {}
        }

        // EXP-980: the relation rows the list nests (`parent`) and badges
        // (`blocks`) with, plus the issues at either end of a `blocks` row —
        // the same tracked read the board list opens.
        let relationObservation = ValueObservation.tracking {
            db -> ([IssueRelationEntity], [IssueEntity]) in
            let relations = try IssueRelationEntity.fetchAll(db)
            let ids = Array(Set(
                relations
                    .filter { $0.type == IssueRelationType.blocks.rawValue }
                    .flatMap { [$0.issueId, $0.relatedIssueId] }
            ))
            let issues = ids.isEmpty
                ? []
                : try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            return (relations, issues)
        }
        relationTask = Task { [weak self] in
            do {
                for try await (relations, issues) in relationObservation.values(in: pool) {
                    guard let self else { return }
                    self.relations = relations
                    self.relationIssues = issues
                    self.rebuildRows()
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
        relationTask?.cancel()
        relationTask = nil
    }

    // MARK: - Rendered rows (EXP-980)

    /// One rendered group: an anchor status and the rows displayed under it.
    struct RenderGroup: Identifiable {
        let status: IssueStatus
        let rows: [NestedIssueRow]
        var id: String { status.rawValue }
    }

    /// The list, nested and ready to draw — recomputed once per incoming
    /// change, over ALL groups at once (a sub-issue follows its root out of
    /// its own status group).
    private(set) var renderGroups: [RenderGroup] = []

    /// EXP-980: the per-row blocks badge numbers.
    private(set) var blockCounts: [String: IssueGraph.BlockCounts] = [:]

    /// The mini-graph a row's badge opens.
    func blockGraph(forIssueId issueId: String) -> IssueGraph.Graph {
        IssueGraph.blockGraph(
            subjectIds: [issueId], relations: relations, issues: relationIssues
        )
    }

    private func rebuildRows() {
        let statuses = IssueStatus.displayOrder
        let sorted = statuses.map { issuesForStatus($0) }
        let byId = Dictionary(
            sorted.flatMap { $0 }.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        let nested = IssueNesting.nestIssueRows(
            groups: sorted.map { $0.map(\.id) },
            relations: relations,
            identifierOf: { byId[$0]?.identifier ?? $0 }
        )
        renderGroups = zip(statuses, nested).compactMap { status, rows in
            let mapped = rows.compactMap { row in
                byId[row.id].map { NestedIssueRow(issue: $0, depth: row.depth) }
            }
            return mapped.isEmpty ? nil : RenderGroup(status: status, rows: mapped)
        }
        blockCounts = IssueGraph.blockCounts(relations: relations, issues: relationIssues)
    }

    /// EXP-314: "Assigned to you" spans TEAMS, and status rows are
    /// team-scoped — grouping by row id would split one "In Progress" into a
    /// group per team. Cross-team surfaces therefore keep ANCHOR-enum
    /// grouping; only the per-board list groups by resolved status row.
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
