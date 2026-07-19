import Foundation
import GRDB

// Mirror of apps/web/src/hooks/use-team-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
//
// Team membership is a simple invite-only binary (no self-service join),
// so there are no "privileged member" / public-team special cases: any
// member is a moderator and can create/mutate.
public struct TeamPermissions: Sendable {
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

extension TeamPermissions {
    public static let denied = TeamPermissions(
        isAuthed: false,
        isMember: false,
        isOwner: false,
        isAdmin: false,
        canCreate: false,
        currentUserId: nil
    )

    public static func resolve(
        team: TeamEntity?,
        currentUserId: String?,
        isAdmin: Bool,
        dbPool: DatabasePool
    ) -> TeamPermissions {
        let isAuthed = currentUserId != nil
        guard let team else {
            return TeamPermissions(
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
                try TeamMemberEntity
                    .filter(Column("team_id") == team.id)
                    .filter(Column("user_id") == uid)
                    .fetchOne(db)?
                    .role
            }
        }()
        let isMember = memberRole != nil
        let isOwner = memberRole == "owner"
        let canCreate = isAuthed && (isMember || isAdmin)

        return TeamPermissions(
            isAuthed: isAuthed,
            isMember: isMember,
            isOwner: isOwner,
            isAdmin: isAdmin,
            canCreate: canCreate,
            currentUserId: currentUserId
        )
    }
}
