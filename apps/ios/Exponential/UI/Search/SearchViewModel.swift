import ExpCore
import Foundation
import GRDB

/// Backs the Search page: observes the ACTIVE team's issues (#EXP-1382,
/// search-list.json TEAM SCOPE — the phone shell has one active team) and
/// ranks queries with the shared `IssueSearch` engine (EXP-892 — the ONE
/// algorithm web, iOS, Android and desktop run). A debounced server
/// `issues.search` for that team (full-text over title + description +
/// comments) augments it with issues the local ranking missed.
///
/// TYPING never waits on the rank: every snapshot is PREPARED once
/// (`IssueSearch.Index`, built on GRDB's background queue), and a keystroke
/// ranks it in a debounced task off the main actor; the view only reads the
/// published `results`.
@MainActor @Observable
final class SearchViewModel {
    /// EXP-922: ONE flat ranked row — the web sheet's and the desktop
    /// palette's shape, in the single relevance order.
    struct Result: Identifiable, Sendable {
        let issue: IssueEntity
        var id: String { issue.id }
    }

    /// How many ranked issues the page renders at once, local + server hits —
    /// EXP-922: the ONE limit every search surface on every client uses.
    nonisolated private static let resultLimit = IssueSearch.defaultLimit
    /// The keystroke coalescing window before a rank runs.
    private static let rankDebounce: Duration = .milliseconds(120)
    /// The coalescing window before the server is asked.
    private static let serverDebounce: Duration = .milliseconds(250)

    /// What the page renders — always state, never computed in a body.
    private(set) var results: [Result] = []

    /// A rank is still owed for the typed query (the debounce, the detached
    /// rank, or the index not built yet): the page keeps its rows and never
    /// shows the empty state meanwhile (feedback-states.json: no false
    /// "No issues match" while the work is pending).
    var isRanking: Bool {
        !query.isEmpty && teamId != nil && (index == nil || localQuery != query)
    }

    private var teamId: String?
    private var query = ""
    /// The active team's issues, prepared once per snapshot.
    private var index: IssueSearch.Index<IssueEntity>?
    // The local rank for `localQuery` and the server's relevance-ordered
    // hits for `serverHitsQuery`; `publish` merges them only while both
    // belong to the current query, so a stale half never bleeds in.
    private var local: [IssueEntity] = []
    private var localQuery = ""
    private var serverHits: [SearchIssueHit] = []
    private var serverHitsQuery = ""

    private let accountId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private var issueTask: Task<Void, Never>?
    private var rankTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, issuesApi: IssuesApi) {
        self.accountId = accountId
        self.db = db
        self.issuesApi = issuesApi
    }

    /// Points the page at `teamId` (the shell's active team) and observes its
    /// issues; a switch re-runs the query against the new team.
    func startObserving(teamId: String?) {
        stopObserving() // restartable: the view re-arms on every appear
        if teamId != self.teamId {
            self.teamId = teamId
            index = nil
            local = []
            localQuery = ""
            serverHits = []
            serverHitsQuery = ""
            results = []
        }
        guard let teamId, let pool = try? db.pool(forAccountId: accountId) else { return }

        // The index is built in `map`, which GRDB runs off the main actor;
        // the box hands each build the previous one so unchanged rows keep
        // their prepared entry.
        let previous = IndexBox()
        let observation = ValueObservation.tracking { db in
            try IssueEntity
                .filter(sql: "board_id IN (SELECT id FROM boards WHERE team_id = ?)", arguments: [teamId])
                .fetchAll(db)
        }
        .removeDuplicates()
        .map { issues in
            let index = IssueSearch.Index(issues, reusing: previous.index, projection: \.searchRow)
            previous.index = index
            return index
        }
        issueTask = Task { [weak self] in
            do {
                for try await index in observation.values(in: pool) {
                    guard let self else { return }
                    self.index = index
                    // A synced change re-ranks the query on screen at once.
                    self.scheduleRank(debounce: false)
                }
            } catch {}
        }
        queryChanged(query)
    }

    func stopObserving() {
        issueTask?.cancel()
        issueTask = nil
        rankTask?.cancel()
        rankTask = nil
        searchTask?.cancel()
        searchTask = nil
    }

    /// The view calls this on every keystroke. Both the rank and the server
    /// search are debounced and cancelled by the next keystroke.
    func queryChanged(_ query: String) {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        self.query = trimmed
        searchTask?.cancel()
        searchTask = nil

        guard !trimmed.isEmpty else {
            rankTask?.cancel()
            rankTask = nil
            local = []
            localQuery = ""
            serverHits = []
            serverHitsQuery = ""
            results = []
            return
        }
        scheduleRank(debounce: true)

        // ONE request, scoped to the active team (`issues.search` takes the
        // team id); failures just mean no server hits.
        guard let teamId else { return }
        let accountId = accountId
        let api = issuesApi
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: Self.serverDebounce)
            guard !Task.isCancelled else { return }
            let hits = (try? await api.search(
                accountId: accountId,
                teamId: teamId,
                query: trimmed,
                limit: Self.resultLimit
            )) ?? []
            guard let self, !Task.isCancelled, self.query == trimmed, self.teamId == teamId else { return }
            self.serverHits = hits
            self.serverHitsQuery = trimmed
            self.publish()
        }
    }

    /// Ranks the current query over the prepared pool in a detached task
    /// (off the main actor), cancelling the previous rank.
    private func scheduleRank(debounce: Bool) {
        rankTask?.cancel()
        rankTask = nil
        let query = query
        guard !query.isEmpty, let index else { return }
        rankTask = Task { [weak self] in
            if debounce {
                try? await Task.sleep(for: Self.rankDebounce)
                guard !Task.isCancelled else { return }
            }
            let ranked = await Task.detached(priority: .userInitiated) {
                index.rank(query: query, limit: Self.resultLimit)
            }.value
            guard let self, !Task.isCancelled, self.query == query, self.index === index else { return }
            self.local = ranked
            self.localQuery = query
            self.publish()
        }
    }

    /// EXP-892: the local rank first (an exact identifier hit, then undone
    /// before done, then identifier before title before description, recency
    /// breaking ties), then the server's full-text hits it missed (deduped by
    /// id, relevance order): a hit in the local store renders the local row,
    /// otherwise a slim row built from the returned fields. Capped at
    /// `resultLimit`; both halves are ≤ that, so this is cheap on the main
    /// actor.
    private func publish() {
        guard !query.isEmpty, localQuery == query else { return }
        let ranked: [IssueEntity]
        if serverHitsQuery == query, !serverHits.isEmpty {
            let index = index
            ranked = IssueSearch.mergeServerHits(
                local: local,
                hits: serverHits,
                limit: Self.resultLimit,
                localId: \.id,
                hitId: \.id,
                resolve: { hit in index?.value(id: hit.id) ?? Self.placeholderEntity(from: hit) }
            )
        } else {
            ranked = local
        }
        results = ranked.map { Result(issue: $0) }
    }

    /// A display-only stand-in for a server hit that has no local GRDB row
    /// (e.g. not yet synced) — carries exactly the fields the result row
    /// renders (priority, identifier, status, title) plus the ids needed for
    /// navigation.
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
            createdAt: "",
            updatedAt: ""
        )
    }
}

/// The previous snapshot's index, carried between `map` calls (GRDB runs
/// them one at a time on its own queue).
private final class IndexBox: @unchecked Sendable {
    var index: IssueSearch.Index<IssueEntity>?
}
