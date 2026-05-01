package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class RegisterPushTokenInput(val token: String, val platform: String)

@Serializable
data class UnregisterPushTokenInput(val token: String)

@Serializable
data class PushTokenAck(val ok: Boolean)

@Singleton
class PushTokensApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun register(token: String) {
        trpc.mutation(
            path = "pushTokens.register",
            input = RegisterPushTokenInput(token = token, platform = "android"),
            inputSerializer = RegisterPushTokenInput.serializer(),
            outputSerializer = PushTokenAck.serializer(),
        )
    }

    suspend fun unregister(token: String) {
        trpc.mutation(
            path = "pushTokens.unregister",
            input = UnregisterPushTokenInput(token),
            inputSerializer = UnregisterPushTokenInput.serializer(),
            outputSerializer = PushTokenAck.serializer(),
        )
    }
}
