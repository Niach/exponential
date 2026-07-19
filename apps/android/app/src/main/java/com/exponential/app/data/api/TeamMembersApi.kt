package com.exponential.app.data.api

import com.exponential.app.data.db.TeamMemberEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class UpdateRoleInput(val memberId: String, val role: String)

@Serializable
data class UpdateRoleResult(val member: TeamMemberEntity)

@Serializable
data class RemoveMemberInput(val memberId: String)

@Serializable
data class OkResult(val ok: Boolean = false, val success: Boolean = false)

@Singleton
class TeamMembersApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun updateRole(accountId: String, memberId: String, role: String): TeamMemberEntity =
        trpc.mutation(
            accountId,
            path = "teamMembers.updateRole",
            input = UpdateRoleInput(memberId, role),
            inputSerializer = UpdateRoleInput.serializer(),
            outputSerializer = UpdateRoleResult.serializer(),
        ).member

    suspend fun remove(accountId: String, memberId: String) {
        trpc.mutation(
            accountId,
            path = "teamMembers.remove",
            input = RemoveMemberInput(memberId),
            inputSerializer = RemoveMemberInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }
}
