package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.TeamInviteEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// `email` is optional (EXP-188 invite-by-email): when set the server persists
// it on the invite and mails the link. explicitNulls=false omits it when null,
// so token-only invites keep the old wire shape.
@Serializable
data class CreateInviteInput(
    val teamId: String,
    val role: String = "member",
    val email: String? = null,
)

// `emailDelivered`: null = no email requested; false = requested but the
// send failed (surface the link so the owner can share it by hand).
@Serializable
data class CreateInviteResult(
    val invite: TeamInviteEntity,
    val token: String,
    val emailDelivered: Boolean? = null,
)

@Serializable
data class AcceptInviteInput(val token: String)

@Serializable
data class AcceptInviteResult(
    val team: TeamEntity,
    val alreadyMember: Boolean = false,
)

@Serializable
data class ListInvitesInput(val teamId: String)

@Serializable
data class ListInvitesResult(val invites: List<TeamInviteEntity>)

@Serializable
data class RevokeInviteInput(val id: String)

@Serializable
data class GetByTokenInput(val token: String)

@Serializable
data class InvitePreview(
    val id: String,
    val teamId: String,
    val role: String,
    val acceptedAt: String? = null,
    val expiresAt: String,
    val teamName: String,
)

@Serializable
data class GetByTokenResult(val invite: InvitePreview)

@Singleton
class TeamInvitesApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun create(
        accountId: String,
        teamId: String,
        role: String = "member",
        email: String? = null,
    ): CreateInviteResult =
        trpc.mutation(
            accountId,
            path = "teamInvites.create",
            input = CreateInviteInput(teamId, role, email),
            inputSerializer = CreateInviteInput.serializer(),
            outputSerializer = CreateInviteResult.serializer(),
        )

    suspend fun accept(accountId: String, token: String): AcceptInviteResult =
        trpc.mutation(
            accountId,
            path = "teamInvites.accept",
            input = AcceptInviteInput(token),
            inputSerializer = AcceptInviteInput.serializer(),
            outputSerializer = AcceptInviteResult.serializer(),
        )

    suspend fun list(accountId: String, teamId: String): List<TeamInviteEntity> =
        trpc.query(
            accountId,
            path = "teamInvites.list",
            input = ListInvitesInput(teamId),
            inputSerializer = ListInvitesInput.serializer(),
            outputSerializer = ListInvitesResult.serializer(),
        ).invites

    suspend fun revoke(accountId: String, id: String) {
        trpc.mutation(
            accountId,
            path = "teamInvites.revoke",
            input = RevokeInviteInput(id),
            inputSerializer = RevokeInviteInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }

    suspend fun getByToken(accountId: String, token: String): InvitePreview =
        trpc.query(
            accountId,
            path = "teamInvites.getByToken",
            input = GetByTokenInput(token),
            inputSerializer = GetByTokenInput.serializer(),
            outputSerializer = GetByTokenResult.serializer(),
        ).invite
}
