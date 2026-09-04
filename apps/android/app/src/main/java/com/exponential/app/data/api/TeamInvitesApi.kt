package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// Invite links: mint (EXP-725), preview and accept. Creating one is owner-only
// server-side and the app only OFFERS it while the team has free seats — at
// the cap the control is removed entirely, so a store build never renders the
// seat-cap billing copy (EXP-216).

@Serializable
data class AcceptInviteInput(val token: String)

@Serializable
data class AcceptInviteResult(
    val team: TeamEntity,
    val alreadyMember: Boolean = false,
)

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

// `role` always rides explicitly rather than leaning on the server default —
// the wire shape is the contract, and a member invite is what every client
// mints. The result also carries the invite row and `emailDelivered`; only the
// token is decoded here (the app never types an address into the invite).
@Serializable
data class CreateInviteInput(val teamId: String, val role: String = "member")

@Serializable
data class CreateInviteResult(val token: String)

@Singleton
class TeamInvitesApi @Inject constructor(private val trpc: TrpcClient) {

    /** Mint a shareable invite link token (owner-only server-side). */
    suspend fun create(accountId: String, teamId: String): String =
        trpc.mutation(
            accountId,
            path = "teamInvites.create",
            input = CreateInviteInput(teamId),
            inputSerializer = CreateInviteInput.serializer(),
            outputSerializer = CreateInviteResult.serializer(),
        ).token

    suspend fun accept(accountId: String, token: String): AcceptInviteResult =
        trpc.mutation(
            accountId,
            path = "teamInvites.accept",
            input = AcceptInviteInput(token),
            inputSerializer = AcceptInviteInput.serializer(),
            outputSerializer = AcceptInviteResult.serializer(),
        )

    suspend fun getByToken(accountId: String, token: String): InvitePreview =
        trpc.query(
            accountId,
            path = "teamInvites.getByToken",
            input = GetByTokenInput(token),
            inputSerializer = GetByTokenInput.serializer(),
            outputSerializer = GetByTokenResult.serializer(),
        ).invite
}
