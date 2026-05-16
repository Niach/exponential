package com.exponential.app.data.api

import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.WorkspaceInviteEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class CreateInviteInput(val workspaceId: String, val role: String = "member")

@Serializable
data class CreateInviteResult(val invite: WorkspaceInviteEntity, val token: String)

@Serializable
data class AcceptInviteInput(val token: String)

@Serializable
data class AcceptInviteResult(
    val workspace: WorkspaceEntity,
    val alreadyMember: Boolean = false,
)

@Serializable
data class ListInvitesInput(val workspaceId: String)

@Serializable
data class ListInvitesResult(val invites: List<WorkspaceInviteEntity>)

@Serializable
data class RevokeInviteInput(val id: String)

@Serializable
data class GetByTokenInput(val token: String)

@Serializable
data class InvitePreview(
    val id: String,
    val workspaceId: String,
    val role: String,
    val acceptedAt: String? = null,
    val expiresAt: String,
    val workspaceName: String,
)

@Serializable
data class GetByTokenResult(val invite: InvitePreview)

@Singleton
class WorkspaceInvitesApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun create(workspaceId: String, role: String = "member"): CreateInviteResult =
        trpc.mutation(
            path = "workspaceInvites.create",
            input = CreateInviteInput(workspaceId, role),
            inputSerializer = CreateInviteInput.serializer(),
            outputSerializer = CreateInviteResult.serializer(),
        )

    suspend fun accept(token: String): AcceptInviteResult =
        trpc.mutation(
            path = "workspaceInvites.accept",
            input = AcceptInviteInput(token),
            inputSerializer = AcceptInviteInput.serializer(),
            outputSerializer = AcceptInviteResult.serializer(),
        )

    suspend fun list(workspaceId: String): List<WorkspaceInviteEntity> =
        trpc.mutation(
            path = "workspaceInvites.list",
            input = ListInvitesInput(workspaceId),
            inputSerializer = ListInvitesInput.serializer(),
            outputSerializer = ListInvitesResult.serializer(),
        ).invites

    suspend fun revoke(id: String) {
        trpc.mutation(
            path = "workspaceInvites.revoke",
            input = RevokeInviteInput(id),
            inputSerializer = RevokeInviteInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }

    suspend fun getByToken(token: String): InvitePreview =
        trpc.mutation(
            path = "workspaceInvites.getByToken",
            input = GetByTokenInput(token),
            inputSerializer = GetByTokenInput.serializer(),
            outputSerializer = GetByTokenResult.serializer(),
        ).invite
}
