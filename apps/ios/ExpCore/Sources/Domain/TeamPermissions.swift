import Foundation
import GRDB

// Mirror of apps/web/src/hooks/use-team-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
//
// Team membership is a simple invite-only binary (no self-service join),
// so there are no "privileged member" / public-team special cases: any
// member is a moderator and can create/mutate.
public struct TeamPermissions: Sendable, Equatable {
    public let isAuthed: Bool
    public let isMember: Bool
    public let isOwner: Bool
    /// The GLOBAL/instance admin flag (console access). EXP-557: it grants NO
    /// team capabilities — non-members get no mutation affordances, admin or
    /// not (mirrors the server's resolveTeamAccess).
    public let isAdmin: Bool

    /// Members moderate. There is no longer a lesser "participant" tier —
    /// membership is invite-only again.
    public var isModerator: Bool { isMember }

    public let canCreate: Bool

    public func canMutateIssue(creatorId: String?) -> Bool {
        guard isAuthed else { return false }
        return isMember
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
        guard let team else {
            return resolve(team: nil, currentUserId: currentUserId, isAdmin: isAdmin, memberRole: nil)
        }
        let memberRole: String? = {
            guard let uid = currentUserId else { return nil }
            return try? dbPool.read { db in
                try Self.memberRole(teamId: team.id, userId: uid, db: db)
            }
        }()
        return resolve(team: team, currentUserId: currentUserId, isAdmin: isAdmin, memberRole: memberRole)
    }

    /// EXP-1358: the same rule inside an OPEN read (a `ValueObservation`'s
    /// tracked fetch), so a view model never blocks the main actor on a
    /// synchronous pool read while the observation that woke it still holds
    /// the readers.
    public static func resolve(
        team: TeamEntity?,
        currentUserId: String?,
        isAdmin: Bool,
        db: Database
    ) throws -> TeamPermissions {
        let memberRole: String? = try {
            guard let team, let uid = currentUserId else { return nil }
            return try Self.memberRole(teamId: team.id, userId: uid, db: db)
        }()
        return resolve(team: team, currentUserId: currentUserId, isAdmin: isAdmin, memberRole: memberRole)
    }

    private static func memberRole(teamId: String, userId: String, db: Database) throws -> String? {
        try TeamMemberEntity
            .filter(Column("team_id") == teamId)
            .filter(Column("user_id") == userId)
            .fetchOne(db)?
            .role
    }

    private static func resolve(
        team: TeamEntity?,
        currentUserId: String?,
        isAdmin: Bool,
        memberRole: String?
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
        let isMember = memberRole != nil
        let isOwner = memberRole == "owner"
        let canCreate = isMember

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


/// EXP-1358: everything an issue surface derives from a team's membership,
/// read in ONE tracked fetch: the permissions, whether the members shape has
/// reached up-to-date (the "Syncing team…" banner), the human member count
/// (solo teams hide the assignee affordances) and the member rows themselves
/// (the assignee / @-mention vocabulary). Observing this instead of the raw
/// tables keeps every derivation off the main actor and lets equal snapshots
/// be dropped before they re-render anything.
public struct TeamPermissionsSnapshot: Sendable, Equatable {
    public var team: TeamEntity?
    public var permissions: TeamPermissions
    public var membersLive: Bool
    public var humanCount: Int
    public var members: [TeamMemberEntity]

    public static let empty = TeamPermissionsSnapshot(
        team: nil, permissions: .denied, membersLive: false, humanCount: 0, members: []
    )

    /// Only "pending" while membership genuinely hasn't landed — a live
    /// members shape with no matching row means the viewer really isn't a
    /// member, which is a real read-only state.
    public var permissionsPending: Bool {
        permissions.isAuthed && !permissions.isMember && !membersLive
    }

    /// Solo team ⇒ nothing to reassign to. EXACTLY one, not `<= 1`: a count
    /// of 0 means the member list hasn't synced yet (never a genuine empty
    /// team — the viewer is a member). Same contract as web's issue-list.tsx.
    public var singleMemberTeam: Bool { humanCount == 1 }

    /// The snapshot for `teamId` (nil = no team resolvable: denied).
    public static func read(
        db: Database,
        teamId: String?,
        currentUserId: String?,
        isAdmin: Bool
    ) throws -> TeamPermissionsSnapshot {
        let members = try TeamMemberEntity.fetchAll(db)
        let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
        guard let teamId else {
            return TeamPermissionsSnapshot(
                team: nil,
                permissions: try TeamPermissions.resolve(team: nil, currentUserId: currentUserId, isAdmin: isAdmin, db: db),
                membersLive: live,
                humanCount: 0,
                members: members
            )
        }
        let team = try TeamEntity.fetchOne(db, key: teamId)
        let own = members.filter { $0.teamId == teamId }
        return TeamPermissionsSnapshot(
            team: team,
            permissions: try TeamPermissions.resolve(team: team, currentUserId: currentUserId, isAdmin: isAdmin, db: db),
            membersLive: live,
            humanCount: own.count,
            members: members
        )
    }
}
