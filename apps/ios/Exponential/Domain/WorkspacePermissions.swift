import Foundation
import GRDB

// Mirror of apps/web/src/hooks/use-workspace-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
struct WorkspacePermissions: Sendable {
    let isAuthed: Bool
    let isMember: Bool
    let isAdmin: Bool

    // Members and admins are moderators. Non-moderators in public workspaces
    // can only set title, description, and labels — never status, priority,
    // assignee, due date / time, or recurrence.
    var isModerator: Bool { isMember || isAdmin }

    let canCreate: Bool

    func canMutateIssue(creatorId: String?) -> Bool {
        guard isAuthed else { return false }
        if isMember { return true }
        if isAdmin { return true }
        if let workspaceIsPublic, workspaceIsPublic, creatorId == currentUserId {
            return true
        }
        return false
    }

    fileprivate let currentUserId: String?
    fileprivate let workspaceIsPublic: Bool?
}

extension WorkspacePermissions {
    static let denied = WorkspacePermissions(
        isAuthed: false,
        isMember: false,
        isAdmin: false,
        canCreate: false,
        currentUserId: nil,
        workspaceIsPublic: nil
    )

    static func resolve(
        workspace: WorkspaceEntity?,
        currentUserId: String?,
        isAdmin: Bool,
        dbPool: DatabasePool
    ) -> WorkspacePermissions {
        let isAuthed = currentUserId != nil
        guard let workspace else {
            return WorkspacePermissions(
                isAuthed: isAuthed,
                isMember: false,
                isAdmin: isAdmin,
                canCreate: false,
                currentUserId: currentUserId,
                workspaceIsPublic: nil
            )
        }

        let isMember: Bool = {
            guard let uid = currentUserId else { return false }
            return (try? dbPool.read { db in
                try WorkspaceMemberEntity
                    .filter(Column("workspace_id") == workspace.id)
                    .filter(Column("user_id") == uid)
                    .fetchCount(db) > 0
            }) ?? false
        }()

        let everyoneCanWrite =
            workspace.isPublic && workspace.publicWritePolicy == "everyone"
        let canCreate = isAuthed && (isMember || isAdmin || everyoneCanWrite)

        return WorkspacePermissions(
            isAuthed: isAuthed,
            isMember: isMember,
            isAdmin: isAdmin,
            canCreate: canCreate,
            currentUserId: currentUserId,
            workspaceIsPublic: workspace.isPublic
        )
    }
}
