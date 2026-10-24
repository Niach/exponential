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
            statusId: statusId,
            priority: priority,
            assigneeId: assigneeId,
            creatorId: creatorId,
            source: source,
            dueDate: dueDate,
            sortOrder: sortOrder,
            completedAt: completedAt,
            duplicateOfId: duplicateOfId,
            estimate: estimate,
            draftId: draftId,
            createdAt: createdAt,
            updatedAt: updatedAt,
            assigneeAgentId: assigneeAgentId,
            creatorAgentId: creatorAgentId
        )
    }
}

public extension IssueEntity {
    /// The same row with a replaced description. The create response is
    /// captured BEFORE the draft images are uploaded and the markdown is
    /// patched, so mirroring it verbatim would store the image-stripped text
    /// until sync lands the patched row (EXP-596).
    func replacingDescription(_ description: String?) -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: boardId,
            number: number,
            identifier: identifier,
            title: title,
            description: description,
            status: status,
            statusId: statusId,
            priority: priority,
            assigneeId: assigneeId,
            creatorId: creatorId,
            source: source,
            dueDate: dueDate,
            sortOrder: sortOrder,
            completedAt: completedAt,
            duplicateOfId: duplicateOfId,
            estimate: estimate,
            draftId: draftId,
            createdAt: createdAt,
            updatedAt: updatedAt,
            assigneeAgentId: assigneeAgentId,
            creatorAgentId: creatorAgentId
        )
    }
}

// #EXP-1383: the two-column assignee writes. Here, not in IssuesApi.swift:
// the Share Extension compiles that file without the domain layer.
public extension UpdateIssueInput {
    /// An assignee write sends BOTH columns, the cleared one as an explicit
    /// null (agent-settings.json `assignee`).
    init(id: String, assignee: IssueAssignee) {
        self.init(id: id, assigneeId: assignee.assigneeId)
        assigneeAgentId = assignee.assigneeAgentId
        if assignee.assigneeId == nil { explicitNulls.insert(CodingKeys.assigneeId.rawValue) }
        if assignee.assigneeAgentId == nil { explicitNulls.insert(CodingKeys.assigneeAgentId.rawValue) }
    }
}

public extension BulkUpdateIssuesInput {
    /// The bulk assignee write, both columns, the cleared one null.
    init(ids: [String], assignee: IssueAssignee) {
        self.init(ids: ids, assigneeId: assignee.assigneeId)
        assigneeAgentId = assignee.assigneeAgentId
        if assignee.assigneeId == nil { explicitNulls.insert(CodingKeys.assigneeId.rawValue) }
        if assignee.assigneeAgentId == nil { explicitNulls.insert(CodingKeys.assigneeAgentId.rawValue) }
    }
}
