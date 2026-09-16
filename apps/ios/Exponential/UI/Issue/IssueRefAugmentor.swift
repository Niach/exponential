import ExpCore
import ExpUI
import Foundation

/// EXP-892 — the `#` autocomplete's server half, for every host that can go
/// async. The local half stays instant and synchronous (`IssueRefLookup.search`
/// ranks the synced rows with the shared `IssueSearch` engine); this debounces
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
    /// The `#` menu shows at most this many rows, local + server hits.
    static let limit = 8
    /// Coalesces keystrokes, same as the Search tab's debounce.
    private static let debounce: UInt64 = 250_000_000

    private let scope: IssueRefLookup.Scope
    private let db: DatabaseManager
    private let accountId: String
    private let issuesApi: IssuesApi
    private var task: Task<Void, Never>?
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
        editor.issueRefSearch = { query in
            IssueRefLookup.search(
                query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
        }
        editor.onIssueRefQuery = { [weak self] query in self?.queryChanged(query) }
    }

    private func queryChanged(_ query: String) {
        task?.cancel()
        task = nil
        // An empty `#` offers recent work; there is nothing to full-text search.
        guard !IssueSearch.normalizeQuery(query).isEmpty else { return }
        task = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.debounce)
            guard !Task.isCancelled, let self else { return }
            guard let teamId = IssueRefLookup.teamId(for: scope, db: db, accountId: accountId),
                  let hits = try? await issuesApi.search(
                      accountId: accountId, teamId: teamId, query: query, limit: Self.limit)
            else { return }
            guard !Task.isCancelled, !hits.isEmpty else { return }
            self.apply(hits, for: query)
        }
    }

    private func apply(_ hits: [SearchIssueHit], for query: String) {
        guard let editor, editor.activeIssueRefQuery == query else { return }
        let local = IssueRefLookup.search(
            query, scope: scope, db: db, accountId: accountId, limit: Self.limit)
        let table = IssueRefLookup.candidates(
            for: hits, scope: scope, db: db, accountId: accountId)
        // The issue being edited never offers itself — on either half.
        let exclude: Set<String> = {
            if case .issue(let id) = scope { return [id] }
            return []
        }()
        let merged = IssueSearch.mergeServerHits(
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
        editor.offerIssueRefCandidates(merged, for: query)
    }
}
