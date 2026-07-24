import Foundation

// Deliberately its own file rather than part of IssuesApi.swift: the Share
// Extension compiles a curated copy of that file into its OWN module (see
// Project.swift's shareExtensionSources) and links neither GRDB nor the entity
// layer, so anything in IssuesApi.swift that mentions IssueEntity would break
// the extension build. The DTO stays Foundation-only over there; the mapping
// lives here, where the entities do.
public extension FetchedIssue {
    /// The server-read row in the exact shape the Electric-synced `issues`
    /// table stores, so it can be written into the local store and observed
    /// like any synced row (EXP-264). Field-for-field: `issues.get` pins the
    /// same column allowlist as the issues shape proxy.
    func entity() -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: boardId,
            number: number,
            identifier: identifier,
            title: title,
            description: description,
            status: status,
            priority: priority,
            assigneeId: assigneeId,
            creatorId: creatorId,
            source: source,
            dueDate: dueDate,
            dueTime: dueTime,
            endTime: endTime,
            sortOrder: sortOrder,
            completedAt: completedAt,
            archivedAt: archivedAt,
            duplicateOfId: duplicateOfId,
            prUrl: prUrl,
            prNumber: prNumber,
            prState: prState,
            branch: branch,
            prMergedAt: prMergedAt,
            createdAt: createdAt,
            updatedAt: updatedAt
        )
    }
}
