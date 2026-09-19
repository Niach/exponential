import ExpCore
import Foundation
import GRDB

/// Backs the Search tab: observes every issue + board of the active account
/// (local GRDB — no server round trip) and ranks queries client-side with the
/// shared `IssueSearch` engine (EXP-892 — the ONE algorithm web, iOS, Android
/// and desktop run). The instant local ranking stays the fast path; a debounced
/// server `issues.search` (full-text over title + description + comments)
/// augments it with issues the local ranking missed.
@MainActor @Observable
final class SearchViewModel {
    /// EXP-922: ONE flat ranked row — the web sheet's and the desktop
    /// palette's shape. The board rides ALONG the issue (it draws the row's
    /// sub-line) instead of banding the list into per-board sections, so the
    /// four surfaces list the same rows in the same single relevance order.
    struct Result: Identifiable {
        let issue: IssueEntity
        let board: BoardEntity?
        var id: String { issue.id }
    }

    /// How many ranked issues the tab renders at once, local + server hits —
    /// EXP-922: the ONE limit every search surface on every client uses.
    private static let resultLimit = IssueSearch.defaultLimit

    var issues: [IssueEntity] = []
    var boards: [BoardEntity] = []

    // Server-search augmentation: relevance-ordered hits for `serverHitsQuery`.
    // `results(for:)` only merges them while the rendered query still matches,
    // so stale hits never bleed into a newer keystroke's results.
    private var serverHits: [SearchIssueHit] = []
    private var serverHitsQuery = ""

    private let accountId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var issueTask: Task<Void, Never>?
    private var boardTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, issuesApi: IssuesApi) {
        self.accountId = accountId
        self.db = db
        self.issuesApi = issuesApi
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                }
            } catch {}
        }

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
        searchTask?.cancel()
        searchTask = nil
    }

    /// Debounced server search — the view calls this on every query change.
    /// Never blocks typing: local substring results render immediately from
    /// `results(for:)`; server hits land later and augment them. Errors fall
    /// back to local-only silently.
    func queryChanged(_ query: String) {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        searchTask?.cancel()
        searchTask = nil

        guard !trimmed.isEmpty else {
            serverHits = []
            serverHitsQuery = ""
            return
        }

        // The Search tab spans the whole account, but `issues.search` is
        // team-scoped — fan out one query per synced team (derived
        // from the observed boards; a team without boards has no
        // issues to find). Sorted for a deterministic merge order.
        let teamIds = Array(Set(boards.map(\.teamId))).sorted()
        let accountId = accountId
        let api = issuesApi

        searchTask = Task { [weak self] in
            // Debounce: coalesce keystrokes; a newer call cancels this task.
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled, !teamIds.isEmpty else { return }

            // Per-team failures just mean fewer server hits — never an
            // error surfaced to the user.
            let hits = await withTaskGroup(of: (String, [SearchIssueHit])?.self) { group in
                for teamId in teamIds {
                    group.addTask {
                        guard let hits = try? await api.search(
                            accountId: accountId,
                            teamId: teamId,
                            query: trimmed,
                            limit: Self.resultLimit
                        ) else {
                            return nil
                        }
                        return (teamId, hits)
                    }
                }
                var byTeam: [String: [SearchIssueHit]] = [:]
                for await result in group {
                    if let (teamId, teamHits) = result {
                        byTeam[teamId] = teamHits
                    }
                }
                // Relevance order preserved within each team.
                return teamIds.flatMap { byTeam[$0] ?? [] }
            }

            guard let self, !Task.isCancelled else { return }
            self.serverHits = hits
            self.serverHitsQuery = trimmed
        }
    }

    /// EXP-892: the shared `IssueSearch` engine ranks the synced rows (the ONE
    /// algorithm every client runs — an exact identifier hit, then undone
    /// before done (EXP-922), then identifier before title before description,
    /// recency breaking ties), capped at `resultLimit`. Server full-text hits
    /// the local ranking missed are appended after the local matches (deduped
    /// by id, relevance order): a hit whose id is in the local store renders
    /// the local row, otherwise a slim row built from the returned fields.
    ///
    /// EXP-922: ONE flat list, no board bands — the ranking already decides
    /// the order, and grouping fought it (a board's band jumped to wherever
    /// its best hit landed and dragged its weaker hits up with it).
    func results(for query: String) -> [Result] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }

        let local = IssueSearch.rank(
            issues,
            query: trimmed,
            limit: Self.resultLimit,
            projection: \.searchRow
        )
        // Augment with server hits — only while they belong to the query being
        // rendered, so a stale response never pollutes a newer keystroke.
        let ranked: [IssueEntity]
        if serverHitsQuery == trimmed, !serverHits.isEmpty {
            let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            ranked = IssueSearch.mergeServerHits(
                local: local,
                hits: serverHits,
                limit: Self.resultLimit,
                localId: \.id,
                hitId: \.id,
                // The Search tab spans the account, so an unsynced hit still
                // renders — from its own returned fields.
                resolve: { hit in issuesById[hit.id] ?? Self.placeholderEntity(from: hit) }
            )
        } else {
            ranked = local
        }

        let boardsById = Dictionary(boards.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return ranked.map { Result(issue: $0, board: boardsById[$0.boardId]) }
    }

    /// A display-only stand-in for a server hit that has no local GRDB row
    /// (e.g. not yet synced) — carries exactly the fields the
    /// result row renders (priority, identifier, status, title) plus the ids
    /// needed for grouping and navigation.
    private static func placeholderEntity(from hit: SearchIssueHit) -> IssueEntity {
        IssueEntity(
            id: hit.id,
            boardId: hit.boardId,
            number: nil,
            identifier: hit.identifier,
            title: hit.title,
            description: nil,
            status: hit.status,
            priority: hit.priority,
            assigneeId: nil,
            creatorId: nil,
            source: nil,
            dueDate: nil,
            sortOrder: nil,
            completedAt: nil,
            duplicateOfId: nil,
            prUrl: nil,
            prNumber: nil,
            prState: nil,
            branch: nil,
            prMergedAt: nil,
            createdAt: "",
            updatedAt: ""
        )
    }
}
