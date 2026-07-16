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
    /// Sorted identifiers of every linked issue (for the batch row subtitle).
    var identifiers: [String] { issues.compactMap { $0.identifier } }
}

/// One project's review entries — Reviews groups by project like the other
/// cross-project lists group by status.
struct ReviewGroup: Identifiable {
    let project: ProjectEntity
    let entries: [ReviewEntry]
    var id: String { project.id }
}

/// "Reviews" (EXP-131): every issue in the ACTIVE workspace with an open PR,
/// collapsed to one entry per distinct PR (a batch PR appears once, not N
/// times), grouped by project. Mirrors `MyIssuesViewModel`'s GRDB observation
/// pattern — two independent, cancellable loops over issues + projects.
@MainActor @Observable
final class ReviewsViewModel {
    var issues: [IssueEntity] = []
    var projects: [ProjectEntity] = []

    private let accountId: String
    private let db: DatabaseManager

    private var issueTask: Task<Void, Never>?
    private var projectTask: Task<Void, Never>?

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

        // Projects resolve each entry's project (name/section) and scope the
        // list to the active workspace (issues carry no workspace_id).
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

    /// Review entries grouped by project, scoped to `workspaceId`. Entries
    /// within a project are newest-first; project sections follow the sidebar's
    /// `sortOrder`. Empty when no workspace is active.
    func groups(workspaceId: String?) -> [ReviewGroup] {
        guard let workspaceId else { return [] }

        let workspaceProjects = projects.filter { $0.workspaceId == workspaceId }
        let projectById = Dictionary(uniqueKeysWithValues: workspaceProjects.map { ($0.id, $0) })
        let candidates = issues.filter { projectById[$0.projectId] != nil }

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

        // Group entries by their representative's project.
        var byProject: [String: [ReviewEntry]] = [:]
        for entry in entries {
            byProject[entry.representative.projectId, default: []].append(entry)
        }

        return workspaceProjects
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
            .compactMap { project in
                guard let projectEntries = byProject[project.id], !projectEntries.isEmpty else { return nil }
                let ordered = projectEntries.sorted {
                    Self.newerFirst($0.representative, $1.representative)
                }
                return ReviewGroup(project: project, entries: ordered)
            }
    }

    /// Newest-first by `createdAt` (Postgres wire text compares chronologically,
    /// the IssueSorting precedent), id as the deterministic tie-break.
    private static func newerFirst(_ a: IssueEntity, _ b: IssueEntity) -> Bool {
        if a.createdAt != b.createdAt { return a.createdAt > b.createdAt }
        return a.id > b.id
    }
}
