import ExpCore
import Foundation
import GRDB

/// Backs the Search tab: observes every issue + project of the active account
/// (local GRDB — no server round trip) and matches queries client-side over
/// identifier + title, mirroring the Android `SearchScreen`. The instant local
/// substring filter stays the fast path; a debounced server `issues.search`
/// (full-text over title + description + comments) augments it with issues the
/// local filter missed.
@MainActor @Observable
final class SearchViewModel {
    struct ResultGroup: Identifiable {
        let project: ProjectEntity
        let issues: [IssueEntity]
        var id: String { project.id }
    }

    var issues: [IssueEntity] = []
    var projects: [ProjectEntity] = []

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
    private var projectTask: Task<Void, Never>?
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
                    self?.issues = issues.filter { $0.archivedAt == nil }
                }
            } catch {}
        }

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
        // workspace-scoped — fan out one query per synced workspace (derived
        // from the observed projects; a workspace without projects has no
        // issues to find). Sorted for a deterministic merge order.
        let workspaceIds = Array(Set(projects.map(\.workspaceId))).sorted()
        let accountId = accountId
        let api = issuesApi

        searchTask = Task { [weak self] in
            // Debounce: coalesce keystrokes; a newer call cancels this task.
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled, !workspaceIds.isEmpty else { return }

            // Per-workspace failures just mean fewer server hits — never an
            // error surfaced to the user.
            let hits = await withTaskGroup(of: (String, [SearchIssueHit])?.self) { group in
                for workspaceId in workspaceIds {
                    group.addTask {
                        guard let hits = try? await api.search(accountId: accountId, workspaceId: workspaceId, query: trimmed) else {
                            return nil
                        }
                        return (workspaceId, hits)
                    }
                }
                var byWorkspace: [String: [SearchIssueHit]] = [:]
                for await result in group {
                    if let (workspaceId, workspaceHits) = result {
                        byWorkspace[workspaceId] = workspaceHits
                    }
                }
                // Relevance order preserved within each workspace.
                return workspaceIds.flatMap { byWorkspace[$0] ?? [] }
            }

            guard let self, !Task.isCancelled else { return }
            self.serverHits = hits
            self.serverHitsQuery = trimmed
        }
    }

    /// Substring match over identifier + title, newest activity first, capped
    /// at 50, grouped under project headers (groups ordered by their newest
    /// matching issue). Server full-text hits the local filter missed are
    /// appended after the local matches (deduped by id, relevance order):
    /// a hit whose id is in the local store renders the local row, otherwise
    /// a slim row built from the returned fields.
    func results(for query: String) -> [ResultGroup] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }

        let matches = issues
            .filter {
                $0.title.localizedCaseInsensitiveContains(trimmed)
                    || ($0.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .prefix(50)

        var order: [String] = []
        var byProject: [String: [IssueEntity]] = [:]
        var seenIds = Set<String>()
        for issue in matches {
            seenIds.insert(issue.id)
            if byProject[issue.projectId] == nil {
                order.append(issue.projectId)
                byProject[issue.projectId] = []
            }
            byProject[issue.projectId]?.append(issue)
        }

        // Augment with server hits — only while they belong to the query being
        // rendered, so a stale response never pollutes a newer keystroke.
        if serverHitsQuery == trimmed, !serverHits.isEmpty {
            let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            for hit in serverHits where !seenIds.contains(hit.id) {
                seenIds.insert(hit.id)
                let issue = issuesById[hit.id] ?? Self.placeholderEntity(from: hit)
                if byProject[issue.projectId] == nil {
                    order.append(issue.projectId)
                    byProject[issue.projectId] = []
                }
                byProject[issue.projectId]?.append(issue)
            }
        }

        let projectsById = Dictionary(projects.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return order.compactMap { projectId in
            guard let project = projectsById[projectId], let issues = byProject[projectId] else { return nil }
            return ResultGroup(project: project, issues: issues)
        }
    }

    /// A display-only stand-in for a server hit that has no local GRDB row
    /// (e.g. archived or not yet synced) — carries exactly the fields the
    /// result row renders (priority, identifier, status, title) plus the ids
    /// needed for grouping and navigation.
    private static func placeholderEntity(from hit: SearchIssueHit) -> IssueEntity {
        IssueEntity(
            id: hit.id,
            projectId: hit.projectId,
            number: nil,
            identifier: hit.identifier,
            title: hit.title,
            description: nil,
            status: hit.status,
            priority: hit.priority,
            assigneeId: nil,
            creatorId: nil,
            dueDate: nil,
            dueTime: nil,
            endTime: nil,
            sortOrder: nil,
            completedAt: nil,
            archivedAt: nil,
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
