import ExpUI
import ExpCore
import Foundation
import GRDB

/// One review entry (EXP-131): the open PR(s) awaiting review. A batch coding
/// run links several issues to ONE `prUrl`, so those issues collapse into a
/// single entry; an issue with no `prUrl` (shouldn't normally happen for an
/// open PR, but be defensive) keys on its own id so it still renders once.
struct ReviewEntry: Identifiable {
    /// `prUrl` when present, else `issue:<id>` — the grouping key.
    let id: String
    /// The issues sharing this PR, newest first. `representative` is the first.
    let issues: [IssueEntity]

    var representative: IssueEntity { issues[0] }
    var isBatch: Bool { issues.count > 1 }
    var prUrl: String? { representative.prUrl }
    var prNumber: Int? { representative.prNumber }
    var branch: String? { representative.branch }
    /// Identifiers of every linked issue, newest first — mirrors `issues`
    /// (for the batch row subtitle).
    var identifiers: [String] { issues.compactMap { $0.identifier } }
}

/// One board's review entries — Reviews groups by board like the other
/// cross-board lists group by status.
struct ReviewGroup: Identifiable {
    let board: BoardEntity
    let entries: [ReviewEntry]
    var id: String { board.id }
}

/// "Reviews" (EXP-131): every issue in the ACTIVE team with an open PR,
/// collapsed to one entry per distinct PR (a batch PR appears once, not N
/// times), grouped by board. Mirrors `MyIssuesViewModel`'s GRDB observation
/// pattern — two independent, cancellable loops over issues + boards.
@MainActor @Observable
final class ReviewsViewModel {
    var issues: [IssueEntity] = []
    var boards: [BoardEntity] = []

    private let accountId: String
    private let db: DatabaseManager

    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        // Only issues with an OPEN PR are review candidates.
        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("pr_state") == DomainContract.prStateOpen)
                .fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues.filter { $0.archivedAt == nil }
                }
            } catch {}
        }

        // Boards resolve each entry's board (name/section) and scope the
        // list to the active team (issues carry no team_id).
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

    /// Review entries grouped by board, scoped to `teamId`. Entries
    /// within a board are newest-first; board sections follow the sidebar's
    /// `sortOrder`. Empty when no team is active.
    func groups(teamId: String?) -> [ReviewGroup] {
        guard let teamId else { return [] }

        let teamBoards = boards.filter { $0.teamId == teamId }
        let boardById = Dictionary(uniqueKeysWithValues: teamBoards.map { ($0.id, $0) })
        let candidates = issues.filter { boardById[$0.boardId] != nil }

        // Collapse issues sharing a prUrl into one entry (fall back to the issue
        // id when prUrl is absent). Preserve first-seen order for determinism.
        var buckets: [String: [IssueEntity]] = [:]
        var keyOrder: [String] = []
        for issue in candidates {
            let key = (issue.prUrl?.isEmpty == false) ? issue.prUrl! : "issue:\(issue.id)"
            if buckets[key] == nil { keyOrder.append(key); buckets[key] = [] }
            buckets[key]?.append(issue)
        }

        let entries: [ReviewEntry] = keyOrder.compactMap { key in
            guard let bucket = buckets[key], !bucket.isEmpty else { return nil }
            // Newest first inside the entry — representative is the newest issue.
            let sorted = bucket.sorted { Self.newerFirst($0, $1) }
            return ReviewEntry(id: key, issues: sorted)
        }

        // Group entries by their representative's board.
        var byBoard: [String: [ReviewEntry]] = [:]
        for entry in entries {
            byBoard[entry.representative.boardId, default: []].append(entry)
        }

        return teamBoards
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
            .compactMap { board in
                guard let boardEntries = byBoard[board.id], !boardEntries.isEmpty else { return nil }
                let ordered = boardEntries.sorted {
                    Self.newerFirst($0.representative, $1.representative)
                }
                return ReviewGroup(board: board, entries: ordered)
            }
    }

    /// Newest-first by `createdAt` (Postgres wire text compares chronologically,
    /// the IssueSorting precedent), id as the deterministic tie-break.
    private static func newerFirst(_ a: IssueEntity, _ b: IssueEntity) -> Bool {
        if a.createdAt != b.createdAt { return a.createdAt > b.createdAt }
        return a.id > b.id
    }
}
