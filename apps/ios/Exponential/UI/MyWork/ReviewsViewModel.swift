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

/// EXP-734: one AGENT RUN's own open pull request — the chore PR an action or
/// chat run opened through `exponential_pr_open({repositoryId, head})`, which
/// links no issue at all and so appears in no board group.
struct RunReviewEntry: Identifiable {
    let session: CodingSessionEntity
    /// The run's own name: its action, or "Chat" for a chat run.
    let title: String
    var id: String { session.id }
    var prUrl: String? { session.prUrl }
    var prNumber: Int? { session.prNumber }
    var branch: String? { session.branch }
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
    /// EXP-734: issue-less runs whose OWN pull request is open — the chore PRs
    /// no board group can ever show.
    var runSessions: [CodingSessionEntity] = []

    private let accountId: String
    private let db: DatabaseManager

    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var sessionTask: Task<Void, Never>?

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
                    self?.issues = issues
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

        // EXP-734: an action or chat run's own PR links no issue, so it can
        // only be found on the session row the server stamped it on.
        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("issue_id") == nil)
                .filter(Column("pr_state") == DomainContract.prStateOpen)
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.runSessions = sessions
                }
            } catch {}
        }
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        boardTask?.cancel()
        boardTask = nil
        sessionTask?.cancel()
        sessionTask = nil
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

    /// EXP-734: the team's agent runs parking their OWN open pull request, one
    /// entry per distinct prUrl (newest run wins), newest first. Sessions carry
    /// `team_id`, so no board scope is needed.
    func runEntries(teamId: String?) -> [RunReviewEntry] {
        guard let teamId else { return [] }
        var byPrUrl: [String: CodingSessionEntity] = [:]
        for session in runSessions where session.teamId == teamId {
            guard session.hasOpenPr, let prUrl = session.prUrl else { continue }
            if let current = byPrUrl[prUrl], Self.newerFirst(current, session) { continue }
            byPrUrl[prUrl] = session
        }
        return byPrUrl.values
            .sorted { Self.newerFirst($0, $1) }
            .map {
                RunReviewEntry(session: $0, title: $0.actionName ?? "Chat")
            }
    }

    /// Newest-first by `startedAt`, id as the deterministic tie-break — the
    /// issue rule applied to session rows.
    private static func newerFirst(_ a: CodingSessionEntity, _ b: CodingSessionEntity) -> Bool {
        if a.startedAt != b.startedAt { return a.startedAt > b.startedAt }
        return a.id > b.id
    }

    /// Newest-first by `createdAt` (Postgres wire text compares chronologically,
    /// the IssueSorting precedent), id as the deterministic tie-break.
    private static func newerFirst(_ a: IssueEntity, _ b: IssueEntity) -> Bool {
        if a.createdAt != b.createdAt { return a.createdAt > b.createdAt }
        return a.id > b.id
    }
}
