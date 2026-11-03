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

/** FEED-108: a roster row without an invite (`teamMembers.addPlaceholder`). */
@Serializable
data class AddPlaceholderInput(val teamId: String, val email: String, val name: String? = null)

@Serializable
data class AddedPlaceholder(val name: String = "")

@Serializable
data class AddPlaceholderResult(val member: AddedPlaceholder = AddedPlaceholder())

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

    /** FEED-108: the placeholder member, no mail, no seat; returns its name. */
    suspend fun addPlaceholder(accountId: String, teamId: String, email: String, name: String?): String =
        trpc.mutation(
            accountId,
            path = "teamMembers.addPlaceholder",
            input = AddPlaceholderInput(teamId, email, name),
            inputSerializer = AddPlaceholderInput.serializer(),
            outputSerializer = AddPlaceholderResult.serializer(),
        ).member.name

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
