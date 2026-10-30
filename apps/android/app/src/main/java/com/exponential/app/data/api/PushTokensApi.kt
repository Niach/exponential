package com.exponential.app.data.api

import io.ktor.http.HttpStatusCode
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

/**
 * A sealed registration (EXP-1435): the token, the push relay capability this
 * install minted for the server, and the X25519 key the server seals for. The
 * client never posts a bare token (NEW-14).
 */
@Serializable
data class RegisterPushTokenInput(
    val token: String,
    val platform: String,
    val capability: String,
    val pushPublicKey: String,
)

@Serializable
data class UnregisterPushTokenInput(val token: String)

@Serializable
data class PushTokenAck(val ok: Boolean)

/** `pushTokens.relay`: the push relay base URL devices register with; null = none. */
@Serializable
data class PushRelayInfo(val url: String? = null)

@Serializable
private object PushRelayEmptyInput

@Singleton
class PushTokensApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun register(
        accountId: String,
        token: String,
        capability: String,
        pushPublicKey: String,
    ) {
        trpc.mutation(
            accountId,
            path = "pushTokens.register",
            input = RegisterPushTokenInput(
                token = token,
                platform = "android",
                capability = capability,
                pushPublicKey = pushPublicKey,
            ),
            inputSerializer = RegisterPushTokenInput.serializer(),
            outputSerializer = PushTokenAck.serializer(),
        )
    }

    suspend fun unregister(accountId: String, token: String) {
        trpc.mutation(
            accountId,
            path = "pushTokens.unregister",
            input = UnregisterPushTokenInput(token),
            inputSerializer = UnregisterPushTokenInput.serializer(),
            outputSerializer = PushTokenAck.serializer(),
        )
    }

    /**
     * The relay this account's server pushes through (EXP-1435). A server
     * older than the procedure answers tRPC 404: that is "no relay" too, and
     * without one the device registers nothing. Any other failure throws.
     */
    suspend fun relayUrl(accountId: String): String? = try {
        trpc.query(
            accountId,
            path = "pushTokens.relay",
            input = PushRelayEmptyInput,
            inputSerializer = PushRelayEmptyInput.serializer(),
            outputSerializer = PushRelayInfo.serializer(),
        ).url?.trimEnd('/')?.takeIf { it.isNotBlank() }
    } catch (err: TrpcException) {
        if (err.status == HttpStatusCode.NotFound) null else throw err
    }
}
