package com.exponential.app.data.api

import com.exponential.app.data.db.WorkspaceMemberEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class JoinWorkspaceInput(val workspaceId: String)

@Serializable
data class JoinWorkspaceResult(val member: WorkspaceMemberEntity)

@Serializable
data class UpdateRoleInput(val memberId: String, val role: String)

@Serializable
data class UpdateRoleResult(val member: WorkspaceMemberEntity)

@Serializable
data class RemoveMemberInput(val memberId: String)

@Serializable
data class OkResult(val ok: Boolean = false, val success: Boolean = false)

@Singleton
class WorkspaceMembersApi @Inject constructor(private val trpc: TrpcClient) {
    /// Self-service join — the server restricts it to PUBLIC workspaces
    /// (private workspaces require an invite) and it's idempotent, so a retry
    /// after a flaky response is safe. Joining is what makes a public board
    /// sync for this user; the changed membership rotates every Electric
    /// shape's where clause, so the pipelines 409-refetch on their next poll.
    suspend fun join(accountId: String, workspaceId: String): WorkspaceMemberEntity =
        trpc.mutation(
            accountId,
            path = "workspaceMembers.join",
            input = JoinWorkspaceInput(workspaceId),
            inputSerializer = JoinWorkspaceInput.serializer(),
            outputSerializer = JoinWorkspaceResult.serializer(),
        ).member

    suspend fun updateRole(accountId: String, memberId: String, role: String): WorkspaceMemberEntity =
        trpc.mutation(
            accountId,
            path = "workspaceMembers.updateRole",
            input = UpdateRoleInput(memberId, role),
            inputSerializer = UpdateRoleInput.serializer(),
            outputSerializer = UpdateRoleResult.serializer(),
        ).member

    suspend fun remove(accountId: String, memberId: String) {
        trpc.mutation(
            accountId,
            path = "workspaceMembers.remove",
            input = RemoveMemberInput(memberId),
            inputSerializer = RemoveMemberInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }
}
