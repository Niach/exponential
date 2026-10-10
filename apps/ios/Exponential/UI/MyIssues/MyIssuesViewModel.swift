import ExpUI
import ExpCore
import Foundation
import GRDB

/// "My issues" — the fixed cross-board view (masterplan §5a): every issue in
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
    /// P14: every synced `issue_statuses` row (all teams) — each issue
    /// resolves against its own team's rows.
    var statusRows: [IssueStatusEntity] = []
    /// Every synced label and issue↔label row — the rows' label dots.
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    /// Folded group ids (session-only, like the board list).
    var collapsedStatuses: Set<String> = []

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
    private var statusTask: Task<Void, Never>?
    private var labelTask: Task<Void, Never>?
    private var issueLabelTask: Task<Void, Never>?

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

        // Boards resolve each issue's TEAM (rows span boards and teams), so
        // the issue resolves against its own team's status rows.
        let boardObservation = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        boardTask = Task { [weak self] in
            do {
                for try await boards in boardObservation.values(in: pool) {
                    guard let self else { return }
                    self.boards = boards
                    self.rebuildRows()
                }
            } catch {}
        }

        // P14: the team status rows the groups resolve against.
        let statusObservation = ValueObservation.tracking { db in
            try IssueStatusEntity.fetchAll(db)
        }
        statusTask = Task { [weak self] in
            do {
                for try await rows in statusObservation.values(in: pool) {
                    guard let self else { return }
                    self.statusRows = rows
                    self.rebuildRows()
                }
            } catch {}
        }

        // The rows' label dots (the board list's own reads).
        let labelObservation = ValueObservation.tracking { db in
            try LabelEntity.fetchAll(db)
        }
        labelTask = Task { [weak self] in
            do {
                for try await labels in labelObservation.values(in: pool) {
                    self?.labels = labels
                }
            } catch {}
        }
        let issueLabelObservation = ValueObservation.tracking { db in
            try IssueLabelEntity.fetchAll(db)
        }
        issueLabelTask = Task { [weak self] in
            do {
                for try await issueLabels in issueLabelObservation.values(in: pool) {
                    self?.issueLabels = issueLabels
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
        statusTask?.cancel()
        statusTask = nil
        labelTask?.cancel()
        labelTask = nil
        issueLabelTask?.cancel()
        issueLabelTask = nil
    }

    // MARK: - Rendered rows (EXP-980)

    /// One rendered group: a resolved team status (merged across teams by
    /// category + name, `CrossTeamStatusGroups`) and the rows under it.
    struct RenderGroup: Identifiable {
        let id: String
        let status: ResolvedIssueStatus
        let rows: [NestedIssueRow]
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
        let teamOfBoard = Dictionary(
            boards.map { ($0.id, $0.teamId) }, uniquingKeysWith: { a, _ in a }
        )
        let groups = CrossTeamStatusGroups.groups(
            issues: issues,
            teamIdOf: { teamOfBoard[$0.boardId] },
            statusRows: statusRows
        )
        let sorted = groups.map(\.issues)
        let byId = Dictionary(
            sorted.flatMap { $0 }.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        let nested = IssueNesting.nestIssueRows(
            groups: sorted.map { $0.map(\.id) },
            relations: relations,
            identifierOf: { byId[$0]?.identifier ?? $0 }
        )
        renderGroups = zip(groups, nested).compactMap { group, rows in
            let mapped = rows.compactMap { row in
                byId[row.id].map { NestedIssueRow(issue: $0, depth: row.depth) }
            }
            return mapped.isEmpty ? nil : RenderGroup(id: group.id, status: group.status, rows: mapped)
        }
        blockCounts = IssueGraph.blockCounts(relations: relations, issues: relationIssues)
    }

    /// The status a row draws: its own team's resolved row (P14).
    func resolved(_ issue: IssueEntity) -> ResolvedIssueStatus {
        let teamId = boards.first { $0.id == issue.boardId }?.teamId
        let team = teamId.map { id in
            IssueStatusResolver.teamStatusesOrFallback(statusRows.filter { $0.teamId == id })
        } ?? IssueStatusResolver.builtinFallbackTeam
        return IssueStatusResolver.resolve(issue, team: team)
    }

    /// Up to the row's label dots, in the board list's order.
    func labelsFor(issueId: String) -> [LabelEntity] {
        let labelIds = issueLabels.filter { $0.issueId == issueId }.map(\.labelId)
        return labels.filter { labelIds.contains($0.id) }
    }

    func toggleStatusCollapsed(_ groupId: String) {
        if collapsedStatuses.contains(groupId) {
            collapsedStatuses.remove(groupId)
        } else {
            collapsedStatuses.insert(groupId)
        }
    }
}
