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

// EXP-1126: `users.signInMethods` — apps/web/src/lib/auth/sign-in-methods.ts
// is the source of truth. `kind` stays a String (apple|google|oidc|password) so
// a future kind decodes instead of failing the whole payload.
@Serializable
data class SignInProviderDto(
    val id: String,
    val name: String,
    val kind: String,
    val available: Boolean = true,
    val linked: Boolean = false,
    val linkedAt: String? = null,
)

@Serializable
data class SignInPasskeyDto(
    val id: String,
    val name: String? = null,
    val createdAt: String? = null,
    val backedUp: Boolean = false,
)

@Serializable
data class SignInMethodsDto(
    val email: String,
    val emailVerified: Boolean = false,
    val emailOtpEnabled: Boolean = false,
    val passwordEnabled: Boolean = false,
    val passkeyEnabled: Boolean = false,
    val providers: List<SignInProviderDto> = emptyList(),
    val passkeys: List<SignInPasskeyDto> = emptyList(),
    val waysIn: Int = 0,
)

@Serializable
data class SignInLinkTicketDto(
    val ticket: String,
    val expiresInSeconds: Int = 120,
)

@Serializable
private object SignInMethodsEmptyInput

@Serializable
internal data class UnlinkSignInMethodInput(@SerialName("providerId") val providerId: String)

@Serializable
internal data class DeletePasskeyInput(@SerialName("id") val id: String)

@Serializable
internal data class MintSignInLinkTicketInput(@SerialName("provider") val provider: String)

/**
 * EXP-1126: a linked row may be unlinked only while another way in remains —
 * the server refuses the last one (PRECONDITION_FAILED); this is the same rule
 * the UI disables the button on. A row that is no way in (a provider no longer
 * offered) always goes (EXP-1209).
 */
fun canUnlink(methods: SignInMethodsDto, provider: SignInProviderDto): Boolean =
    provider.linked && (!provider.available || methods.waysIn > 1)

/** Removing a passkey follows the same last-way-in rule as unlinking. */
fun canRemovePasskey(methods: SignInMethodsDto): Boolean = methods.waysIn > 1

// `users.timezone` — the caller's stored IANA zone (null = never set).
@Serializable
data class TimezoneDto(val timezone: String? = null)

@Singleton
class UsersApi @Inject constructor(private val trpc: TrpcClient) {

    /** The account's stored timezone (null = unset, read as "UTC" — web parity). */
    suspend fun timezone(accountId: String): String? =
        trpc.query(
            accountId,
            path = "users.timezone",
            input = SignInMethodsEmptyInput,
            inputSerializer = SignInMethodsEmptyInput.serializer(),
            outputSerializer = TimezoneDto.serializer(),
        ).timezone

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

    /** EXP-1126: every way the account can sign in on this instance. */
    suspend fun signInMethods(accountId: String): SignInMethodsDto =
        trpc.query(
            accountId,
            path = "users.signInMethods",
            input = SignInMethodsEmptyInput,
            inputSerializer = SignInMethodsEmptyInput.serializer(),
            outputSerializer = SignInMethodsDto.serializer(),
        )

    /** Refused with PRECONDITION_FAILED when it is the account's last way in. */
    suspend fun unlinkSignInMethod(accountId: String, providerId: String) {
        trpc.mutationUnit(
            accountId,
            path = "users.unlinkSignInMethod",
            input = UnlinkSignInMethodInput(providerId),
            inputSerializer = UnlinkSignInMethodInput.serializer(),
        )
    }

    /** Refused with PRECONDITION_FAILED when it is the account's last way in. */
    suspend fun deletePasskey(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "users.deletePasskey",
            input = DeletePasskeyInput(id),
            inputSerializer = DeletePasskeyInput.serializer(),
        )
    }

    /**
     * A single-use, 2-minute ticket naming this session + [provider]; the
     * browser handoff's link mode (`mobile-oauth-start?link=`) redeems it.
     */
    suspend fun mintSignInLinkTicket(accountId: String, provider: String): SignInLinkTicketDto =
        trpc.mutation(
            accountId,
            path = "users.mintSignInLinkTicket",
            input = MintSignInLinkTicketInput(provider),
            inputSerializer = MintSignInLinkTicketInput.serializer(),
            outputSerializer = SignInLinkTicketDto.serializer(),
        )
}
