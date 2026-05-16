package com.exponential.app.data.api

import com.exponential.app.data.db.WorkspaceMemberEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

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
    suspend fun updateRole(memberId: String, role: String): WorkspaceMemberEntity =
        trpc.mutation(
            path = "workspaceMembers.updateRole",
            input = UpdateRoleInput(memberId, role),
            inputSerializer = UpdateRoleInput.serializer(),
            outputSerializer = UpdateRoleResult.serializer(),
        ).member

    suspend fun remove(memberId: String) {
        trpc.mutation(
            path = "workspaceMembers.remove",
            input = RemoveMemberInput(memberId),
            inputSerializer = RemoveMemberInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }
}
