import ExpCore
import Foundation
import GRDB

/// Backs the Search tab: observes every issue + project of the active account
/// (local GRDB — no server round trip) and matches queries client-side over
/// identifier + title, mirroring the Android `SearchScreen`.
@MainActor @Observable
final class SearchViewModel {
    struct ResultGroup: Identifiable {
        let project: ProjectEntity
        let issues: [IssueEntity]
        var id: String { project.id }
    }

    var issues: [IssueEntity] = []
    var projects: [ProjectEntity] = []

    private let accountId: String
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var issueTask: Task<Void, Never>?
    private var projectTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
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
    }

    /// Substring match over identifier + title, newest activity first, capped
    /// at 50, grouped under project headers (groups ordered by their newest
    /// matching issue).
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
        for issue in matches {
            if byProject[issue.projectId] == nil {
                order.append(issue.projectId)
                byProject[issue.projectId] = []
            }
            byProject[issue.projectId]?.append(issue)
        }

        let projectsById = Dictionary(projects.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return order.compactMap { projectId in
            guard let project = projectsById[projectId], let issues = byProject[projectId] else { return nil }
            return ResultGroup(project: project, issues: issues)
        }
    }
}
