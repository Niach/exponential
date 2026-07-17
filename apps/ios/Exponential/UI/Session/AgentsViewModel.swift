import ExpCore
import Foundation
import GRDB

/// Backs the Agents tab: every running coding session in the active account
/// (the synced `coding_sessions` shape), joined to its issue for display.
/// Desktop is the only session runner — this list is the mobile window into
/// what is coding right now.
@MainActor @Observable
final class AgentsViewModel {
    struct Row: Identifiable {
        let session: CodingSessionEntity
        let issue: IssueEntity?
        var id: String { session.id }
    }

    var rows: [Row] = []

    private let accountId: String
    private let db: DatabaseManager
    // Stored and cancelled individually — a single wrapper task would not
    // propagate cancellation into unstructured inner loops, and the view
    // re-arms on every appear.
    private var sessionTask: Task<Void, Never>?
    private var issueTask: Task<Void, Never>?
    private var projectTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?

    private var sessions: [CodingSessionEntity] = []
    private var issues: [IssueEntity] = []
    // Observed so the Start-coding picker can resolve repo-backed projects
    // (EXP-156) — not used by the running-session list itself.
    private var projects: [ProjectEntity] = []

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        let sessionObservation = ValueObservation.tracking { db in
            try CodingSessionEntity
                .filter(Column("status") == DomainContract.codingSessionStatusRunning)
                .fetchAll(db)
        }
        sessionTask = Task { [weak self] in
            do {
                for try await sessions in sessionObservation.values(in: pool) {
                    self?.sessions = sessions
                    self?.rebuild()
                }
            } catch {}
        }

        let issueObservation = ValueObservation.tracking { db in
            try IssueEntity.fetchAll(db)
        }
        issueTask = Task { [weak self] in
            do {
                for try await issues in issueObservation.values(in: pool) {
                    self?.issues = issues
                    self?.rebuild()
                }
            } catch {}
        }

        // Projects back the Start-coding picker's eligibility filter — the
        // running-session list doesn't rebuild on these.
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

        // GRDB only re-fires on writes — this minute clock re-applies the
        // staleness filter so a phantom row's entry clears once its liveness
        // window elapses without any sync delta (EXP-153).
        livenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                guard let self, !Task.isCancelled else { return }
                self.rebuild()
            }
        }
    }

    func stopObserving() {
        sessionTask?.cancel()
        sessionTask = nil
        issueTask?.cancel()
        issueTask = nil
        projectTask?.cancel()
        projectTask = nil
        livenessTask?.cancel()
        livenessTask = nil
    }

    /// Candidate issues for the Agents-tab Start-coding sheet (EXP-156): every
    /// eligible issue in `workspaceId` (nil = across all synced workspaces),
    /// recency-ordered, no preselection. Same eligibility as the issue-detail
    /// card minus the current-issue exemption. Reads the already-observed
    /// projects/issues (no DB round-trip).
    func startCandidates(workspaceId: String?) -> [StartCodingSheet.IssueOption] {
        // Repo-backed, non-archived projects only — projectId → repositoryId.
        var repoByProject: [String: String] = [:]
        for project in projects where project.archivedAt == nil {
            if let workspaceId, project.workspaceId != workspaceId { continue }
            if let repoId = project.repositoryId {
                repoByProject[project.id] = repoId
            }
        }
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        return issues
            .filter { row in
                guard repoByProject[row.projectId] != nil else { return false }
                if row.archivedAt != nil { return false }
                if terminal.contains(row.status) { return false }
                if row.prState == DomainContract.prStateMerged { return false }
                return true
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { row in
                StartCodingSheet.IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoByProject[row.projectId]
                )
            }
    }

    private func rebuild() {
        let issuesById = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        rows = sessions
            // Heartbeat-stale rows render as absent (EXP-153).
            .filter { CodingSessionLiveness.isLive($0) }
            .sorted { $0.startedAt > $1.startedAt }
            // issueId is nil for a desktop batch (multi-issue) run's session
            // — those rows render without an issue link.
            .map { Row(session: $0, issue: $0.issueId.flatMap { issuesById[$0] }) }
    }
}
