import Foundation
import GRDB

// Mirror of apps/web/src/hooks/use-workspace-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
//
// Public boards moved to a per-project `type='feedback'`; workspace membership
// is once again a simple binary (no self-service public join), so the old
// "privileged member" / public-workspace special cases collapse: any member is
// a moderator and can create/mutate.
public struct WorkspacePermissions: Sendable {
    public let isAuthed: Bool
    public let isMember: Bool
    public let isOwner: Bool
    public let isAdmin: Bool

    /// Members and admins moderate. There is no longer a lesser "participant"
    /// tier — membership is invite-only again.
    public var isModerator: Bool { isMember || isAdmin }

    public let canCreate: Bool

    public func canMutateIssue(creatorId: String?) -> Bool {
        guard isAuthed else { return false }
        return isMember || isAdmin
    }

    fileprivate let currentUserId: String?
}

extension WorkspacePermissions {
    public static let denied = WorkspacePermissions(
        isAuthed: false,
        isMember: false,
        isOwner: false,
        isAdmin: false,
        canCreate: false,
        currentUserId: nil
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
                currentUserId: currentUserId
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
        let canCreate = isAuthed && (isMember || isAdmin)

        return WorkspacePermissions(
            isAuthed: isAuthed,
            isMember: isMember,
            isOwner: isOwner,
            isAdmin: isAdmin,
            canCreate: canCreate,
            currentUserId: currentUserId
        )
    }
}
