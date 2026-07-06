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
    // Public-workspace membership is an open self-service join, so a plain member
    // there is a participant, not a moderator — only an owner (or an instance
    // admin) moderates a public workspace. Private workspaces are unchanged: any
    // member moderates. Mirrors the server's isWorkspaceModerator / assertIssueAccess.
    private val isPrivilegedMember: Boolean
        get() = isMember && (!workspaceIsPublic || isOwner)

    val isModerator: Boolean get() = isPrivilegedMember || isAdmin

    fun canMutateIssue(creatorId: String?): Boolean {
        if (!isAuthed) return false
        if (isPrivilegedMember) return true
        if (isAdmin) return true
        // A non-privileged authed user may still mutate issues they created in a
        // public workspace.
        if (workspaceIsPublic && creatorId != null && creatorId == currentUserId) return true
        return false
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
