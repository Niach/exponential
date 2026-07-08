package com.exponential.app.domain

import com.exponential.app.data.db.WorkspaceEntity

// Mirror of apps/web/src/hooks/use-workspace-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
//
// Since project types landed (public boards are read-only projects, not public
// workspaces), permissions collapse to membership-only: any member moderates
// and any member can create. Anonymous public-board viewing is web-only — the
// mobile app only syncs workspaces the user is a member of.
data class WorkspacePermissions(
    val isAuthed: Boolean,
    val isMember: Boolean,
    val isOwner: Boolean,
    val isAdmin: Boolean,
    val canCreate: Boolean,
) {
    val isModerator: Boolean get() = isMember || isAdmin

    fun canMutateIssue(creatorId: String?): Boolean = isModerator

    companion object {
        val Denied = WorkspacePermissions(
            isAuthed = false,
            isMember = false,
            isOwner = false,
            isAdmin = false,
            canCreate = false,
        )

        fun resolve(
            workspace: WorkspaceEntity?,
            currentUserId: String?,
            isAdmin: Boolean,
            isMember: Boolean,
            memberRole: String? = null,
        ): WorkspacePermissions {
            val isAuthed = currentUserId != null
            if (workspace == null) {
                return WorkspacePermissions(
                    isAuthed = isAuthed,
                    isMember = false,
                    isOwner = false,
                    isAdmin = isAdmin,
                    canCreate = false,
                )
            }
            return WorkspacePermissions(
                isAuthed = isAuthed,
                isMember = isMember,
                isOwner = memberRole == "owner",
                isAdmin = isAdmin,
                canCreate = isAuthed && (isMember || isAdmin),
            )
        }
    }
}
