package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/users.ts — self-service account management.
@Serializable
private data class ConfirmInput(@SerialName("confirm") val confirm: Boolean)

@Serializable
private data class SetTimezoneInput(
    @SerialName("timezone") val timezone: String,
    @SerialName("onlyIfUnset") val onlyIfUnset: Boolean,
)

@Singleton
class UsersApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * EXP-452: claim the device's IANA timezone for the account. With
     * [onlyIfUnset] this is the same best-effort post-login claim the web and
     * desktop apps make — an explicit pick in settings always wins. The daily
     * digest's send hour is read in `users.timezone`, so an account that only
     * ever signs in on mobile would otherwise stay NULL and have its digest
     * silently scheduled in UTC.
     */
    suspend fun setTimezone(accountId: String, timezone: String, onlyIfUnset: Boolean) {
        trpc.mutationUnit(
            accountId,
            path = "users.setTimezone",
            input = SetTimezoneInput(timezone, onlyIfUnset),
            inputSerializer = SetTimezoneInput.serializer(),
        )
    }

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
