import ExpCore
import ExpUI
import Foundation

/// EXP-892 — the `#` autocomplete's server half, for every host that can go
/// async. The local half (`IssueRefLookup.search` ranks the synced rows with
/// the shared `IssueSearch` engine) reads off the main thread since #PERF-5
/// and lands a moment after the keystroke; this debounces
/// a team-scoped `issues.search` on the same query — full text over titles,
/// descriptions AND comment bodies, which the local rows cannot answer — and
/// hands the merged list back to the editor.
///
/// The merge is the shared one (`IssueSearch.mergeServerHits`): local order
/// first, then hits not already listed in the server's relevance order, deduped
/// by issue id, capped by the same limit. A hit whose id is local renders the
/// LOCAL row (live title, precise status); an unsynced one renders from the
/// hit's own fields.
///
/// A response is applied only while the menu still shows the query it answered
/// (`IssueEditorModel.activeIssueRefQuery`), so a slow round trip can never
/// repaint a newer keystroke's menu.
@MainActor
final class IssueRefAugmentor {
    /// SLOP-28 (EXP-1259 D4): the `#` search list takes the engine's DEFAULT
    /// limit, local + server hits — the old 8-row typeahead cap is gone.
    static let limit = IssueSearch.defaultLimit
    /// Coalesces keystrokes, same as the Search tab's debounce.
    private static let debounce: UInt64 = 250_000_000

    private let scope: IssueRefLookup.Scope
    private let db: DatabaseManager
    private let accountId: String
    private let issuesApi: IssuesApi
    private var task: Task<Void, Never>?
    private var localTask: Task<Void, Never>?
    /// The query whose local rows are loading or shown, and the one whose
    /// merged rows are shown (local rows never replace those).
    private var localQuery: String?
    private var mergedQuery: String?
    private weak var editor: IssueEditorModel?

    init(
        scope: IssueRefLookup.Scope,
        db: DatabaseManager,
        accountId: String,
        issuesApi: IssuesApi
    ) {
        self.scope = scope
        self.db = db
        self.accountId = accountId
        self.issuesApi = issuesApi
    }

    /// Point an editor's `#` typeahead at this scope: the instant local ranking
    /// plus the debounced server augmentation.
    func attach(to editor: IssueEditorModel) {
        self.editor = editor
        let scope = scope
        let db = db
        let accountId = accountId
        // #PERF-5: never a read on the main thread; the rows on screen stay
        // until this query's local rows land.
        editor.issueRefSearch = { [weak self, weak editor] query in
            self?.loadLocal(query)
            return editor?.issueRefCandidates ?? []
        }
        editor.issueRefLocalSearch = { query in
            await IssueRefLookup.search(
                query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
        }
        editor.onIssueRefQuery = { [weak self] query in self?.queryChanged(query) }
        // SLOP-28: the `#` sheet's server half — the same merge, awaited.
        editor.issueRefRemoteSearch = { [weak self] query in
            await self?.merged(query) ?? []
        }
    }

    /// The local ranking spliced with the team's full-text hits for `query`
    /// (the local rows alone when the server has nothing to add).
    func merged(_ query: String) async -> [IssueRefCandidate] {
        let local = await IssueRefLookup.search(
            query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
        guard !IssueSearch.normalizeQuery(query).isEmpty,
              let teamId = await IssueRefLookup.teamId(for: scope, db: db, accountId: accountId),
              let hits = try? await issuesApi.search(
                  accountId: accountId, teamId: teamId, query: query, limit: Self.limit),
              !hits.isEmpty
        else { return local }
        return await mergeHits(hits, local: local)
    }

    private func loadLocal(_ query: String) {
        // A fresh token (no query on screen yet) always loads.
        guard query != localQuery || editor?.activeIssueRefQuery == nil else { return }
        localQuery = query
        mergedQuery = nil
        localTask?.cancel()
        let (scope, db, accountId) = (scope, db, accountId)
        localTask = Task { [weak self] in
            let local = await IssueRefLookup.search(
                query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
            guard !Task.isCancelled, let self, self.mergedQuery != query else { return }
            self.editor?.offerIssueRefCandidates(local, for: query)
        }
    }

    private func queryChanged(_ query: String) {
        task?.cancel()
        task = nil
        // An empty `#` offers recent work; there is nothing to full-text search.
        guard !IssueSearch.normalizeQuery(query).isEmpty else { return }
        task = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.debounce)
            guard !Task.isCancelled, let self else { return }
            guard let teamId = await IssueRefLookup.teamId(for: scope, db: db, accountId: accountId),
                  let hits = try? await issuesApi.search(
                      accountId: accountId, teamId: teamId, query: query, limit: Self.limit)
            else { return }
            guard !Task.isCancelled, !hits.isEmpty else { return }
            await self.apply(hits, for: query)
        }
    }

    private func apply(_ hits: [SearchIssueHit], for query: String) async {
        guard let editor, editor.activeIssueRefQuery == query else { return }
        let local = await IssueRefLookup.search(
            query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
        let merged = await mergeHits(hits, local: local)
        guard !Task.isCancelled else { return }
        mergedQuery = query
        editor.offerIssueRefCandidates(merged, for: query)
    }

    private func mergeHits(_ hits: [SearchIssueHit], local: [IssueRefCandidate]) async -> [IssueRefCandidate] {
        let table = await IssueRefLookup.candidates(
            for: hits, scope: scope, db: db, accountId: accountId)
        // The issue being edited never offers itself — on either half.
        let exclude: Set<String> = {
            if case .issue(let id) = scope { return [id] }
            return []
        }()
        return IssueSearch.mergeServerHits(
            local: local,
            hits: hits,
            limit: Self.limit,
            exclude: exclude,
            // A locally ranked candidate always carries its row id; the
            // identifier fallback keeps hand-built candidates (tests, hosts
            // without a store read) merge-safe.
            localId: { $0.issueId ?? $0.identifier },
            hitId: { $0.id },
            resolve: { table[$0.id] }
        )
    }
}
