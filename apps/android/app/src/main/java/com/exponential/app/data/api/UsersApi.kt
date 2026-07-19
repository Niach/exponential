package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/users.ts — self-service account management.
@Serializable
private data class ConfirmInput(@SerialName("confirm") val confirm: Boolean)

@Singleton
class UsersApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * Permanently delete the signed-in user's account on this server (store
     * policy: account deletion must be initiable in-app). The server cascades
     * sessions, memberships, authored content, and solo teams; callers
     * must follow up with local sign-out + cache wipe.
     */
    suspend fun deleteAccount(accountId: String) {
        trpc.mutationUnit(
            accountId,
            path = "users.deleteAccount",
            input = ConfirmInput(confirm = true),
            inputSerializer = ConfirmInput.serializer(),
        )
    }
}
