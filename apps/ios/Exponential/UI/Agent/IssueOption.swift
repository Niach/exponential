import ExpCore
import Foundation

/// One eligible issue the Agent page composer can put on a run (EXP-156,
/// top-level since EXP-825 — it used to be nested in the Start-coding sheet).
/// `repositoryId` drives the single-repository-per-run validation (all
/// checked issues must share one).
struct IssueOption: Identifiable, Sendable, Equatable {
    let id: String
    let identifier: String?
    let title: String
    let repositoryId: String?
    // Wire status/priority strings, so the picker rows and the chips can
    // render the same status/priority glyphs as the issue list (EXP-173). No
    // defaults: a producer that forgets them must fail to compile, not
    // silently render every row as Backlog/no-priority.
    let status: String?
    let priority: String?

    /// The ×4 eligibility rule: a repo-backed board, a non-terminal status
    /// (custom statuses anchor to one of the enum values, EXP-314) and no
    /// merged PR. `exempt` ids skip the issue-level checks — a seeded issue
    /// the user opened the composer FROM must always be on offer, whatever
    /// its status (the issue detail's rule since EXP-156).
    static func build(
        issues: [IssueEntity],
        boards: [BoardEntity],
        teamId: String?,
        exempt: Set<String> = []
    ) -> [IssueOption] {
        // Repo-backed boards only — boardId → repositoryId.
        var repoByBoard: [String: String] = [:]
        for board in boards {
            if let teamId, board.teamId != teamId { continue }
            if let repoId = board.repositoryId {
                repoByBoard[board.id] = repoId
            }
        }
        let terminal: Set<String> = [
            IssueStatus.done.rawValue,
            IssueStatus.cancelled.rawValue,
            IssueStatus.duplicate.rawValue,
        ]
        return issues
            .filter { row in
                guard repoByBoard[row.boardId] != nil else { return false }
                if exempt.contains(row.id) { return true }
                if terminal.contains(row.status) { return false }
                if row.prState == DomainContract.prStateMerged { return false }
                return true
            }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { row in
                IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoByBoard[row.boardId],
                    status: row.status,
                    priority: row.priority
                )
            }
    }
}
