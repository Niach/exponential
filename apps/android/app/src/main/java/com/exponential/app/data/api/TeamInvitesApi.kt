package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// Accept-only surface: inviting members is a web-only flow (EXP-216 — the
// store builds must never reach the seat-cap billing copy), but invite LINKS
// still open in the app, so previewing + accepting stays.

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

@Singleton
class TeamInvitesApi @Inject constructor(private val trpc: TrpcClient) {
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
