package com.exponential.app.domain

import com.exponential.app.data.db.TeamEntity

// Mirror of apps/web/src/hooks/use-team-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
//
// Permissions are membership-only (EXP-180 removed public boards — nothing
// is anonymously readable on any client): any member moderates and any
// member can create. Owner-only controls key off `isOwner`.
data class TeamPermissions(
    val isAuthed: Boolean,
    val isMember: Boolean,
    val isOwner: Boolean,
    val isAdmin: Boolean,
    val canCreate: Boolean,
) {
    val isModerator: Boolean get() = isMember || isAdmin

    fun canMutateIssue(creatorId: String?): Boolean = isModerator

    companion object {
        val Denied = TeamPermissions(
            isAuthed = false,
            isMember = false,
            isOwner = false,
            isAdmin = false,
            canCreate = false,
        )

        fun resolve(
            team: TeamEntity?,
            currentUserId: String?,
            isAdmin: Boolean,
            isMember: Boolean,
            memberRole: String? = null,
        ): TeamPermissions {
            val isAuthed = currentUserId != null
            if (team == null) {
                return TeamPermissions(
                    isAuthed = isAuthed,
                    isMember = false,
                    isOwner = false,
                    isAdmin = isAdmin,
                    canCreate = false,
                )
            }
            return TeamPermissions(
                isAuthed = isAuthed,
                isMember = isMember,
                isOwner = memberRole == "owner",
                isAdmin = isAdmin,
                canCreate = isAuthed && (isMember || isAdmin),
            )
        }
    }
}
