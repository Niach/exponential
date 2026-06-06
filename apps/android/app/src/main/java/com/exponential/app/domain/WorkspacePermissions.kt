package com.exponential.app.domain

import com.exponential.app.data.db.WorkspaceEntity

// Mirror of apps/web/src/hooks/use-workspace-permissions.ts. Server enforces
// these rules too — the helper exists so the UI can disable controls a viewer
// can't change instead of bouncing them on submit.
data class WorkspacePermissions(
    val isAuthed: Boolean,
    val isMember: Boolean,
    val isOwner: Boolean,
    val isAdmin: Boolean,
    val canCreate: Boolean,
    private val currentUserId: String?,
    private val workspaceIsPublic: Boolean,
) {
    // Members and admins are moderators. Non-moderators in public workspaces
    // can only set title, description, and labels.
    val isModerator: Boolean get() = isMember || isAdmin

    fun canMutateIssue(creatorId: String?): Boolean {
        if (!isAuthed) return false
        if (isMember) return true
        if (isAdmin) return true
        if (workspaceIsPublic && creatorId != null && creatorId == currentUserId) return true
        return false
    }

    // Mirrors assertIssueAccess(..., "approve_plan") in
    // apps/web/src/lib/auth/access.ts: only the issue creator or a workspace
    // owner can approve agent plans.
    fun canApprovePlan(creatorId: String?): Boolean {
        if (!isAuthed) return false
        if (creatorId != null && creatorId == currentUserId) return true
        return isOwner
    }

    companion object {
        val Denied = WorkspacePermissions(
            isAuthed = false,
            isMember = false,
            isOwner = false,
            isAdmin = false,
            canCreate = false,
            currentUserId = null,
            workspaceIsPublic = false,
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
                    currentUserId = currentUserId,
                    workspaceIsPublic = false,
                )
            }
            val everyoneCanWrite =
                workspace.isPublic && workspace.publicWritePolicy == "everyone"
            val canCreate = isAuthed && (isMember || isAdmin || everyoneCanWrite)
            return WorkspacePermissions(
                isAuthed = isAuthed,
                isMember = isMember,
                isOwner = memberRole == "owner",
                isAdmin = isAdmin,
                canCreate = canCreate,
                currentUserId = currentUserId,
                workspaceIsPublic = workspace.isPublic,
            )
        }
    }
}
