import Foundation
import GRDB

// Mirror of apps/web/src/hooks/use-workspace-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
public struct WorkspacePermissions: Sendable {
    public let isAuthed: Bool
    public let isMember: Bool
    public let isOwner: Bool
    public let isAdmin: Bool

    // Members and admins are moderators. Non-moderators in public workspaces
    // can only set title, description, and labels — never status, priority,
    // assignee, due date / time, or recurrence.
    public var isModerator: Bool { isMember || isAdmin }

    public let canCreate: Bool

    public func canMutateIssue(creatorId: String?) -> Bool {
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
    public static let denied = WorkspacePermissions(
        isAuthed: false,
        isMember: false,
        isOwner: false,
        isAdmin: false,
        canCreate: false,
        currentUserId: nil,
        workspaceIsPublic: nil
    )

    public static func resolve(
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
                isOwner: false,
                isAdmin: isAdmin,
                canCreate: false,
                currentUserId: currentUserId,
                workspaceIsPublic: nil
            )
        }

        let memberRole: String? = {
            guard let uid = currentUserId else { return nil }
            return try? dbPool.read { db in
                try WorkspaceMemberEntity
                    .filter(Column("workspace_id") == workspace.id)
                    .filter(Column("user_id") == uid)
                    .fetchOne(db)?
                    .role
            }
        }()
        let isMember = memberRole != nil
        let isOwner = memberRole == "owner"

        let everyoneCanWrite =
            workspace.isPublic && workspace.publicWritePolicy == "everyone"
        let canCreate = isAuthed && (isMember || isAdmin || everyoneCanWrite)

        return WorkspacePermissions(
            isAuthed: isAuthed,
            isMember: isMember,
            isOwner: isOwner,
            isAdmin: isAdmin,
            canCreate: canCreate,
            currentUserId: currentUserId,
            workspaceIsPublic: workspace.isPublic
        )
    }
}
