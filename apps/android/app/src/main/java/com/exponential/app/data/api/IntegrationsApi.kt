package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class GoogleStatus(
    val connected: Boolean,
    val scope: String? = null,
    val connectedAt: String? = null,
)

@Serializable
data class BackfillResult(val ok: Boolean = false, val scheduled: Int = 0)

@Serializable
private object IntegrationsEmptyInput

@Singleton
class IntegrationsApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun googleStatus(accountId: String): GoogleStatus =
        trpc.query(
            accountId,
            path = "integrations.google.status",
            input = IntegrationsEmptyInput,
            inputSerializer = IntegrationsEmptyInput.serializer(),
            outputSerializer = GoogleStatus.serializer(),
        )

    suspend fun googleDisconnect(accountId: String) {
        trpc.mutation(
            accountId,
            path = "integrations.google.disconnect",
            input = IntegrationsEmptyInput,
            inputSerializer = IntegrationsEmptyInput.serializer(),
            outputSerializer = OkResult.serializer(),
        )
    }

    suspend fun googleBackfill(accountId: String): BackfillResult =
        trpc.mutation(
            accountId,
            path = "integrations.google.backfill",
            input = IntegrationsEmptyInput,
            inputSerializer = IntegrationsEmptyInput.serializer(),
            outputSerializer = BackfillResult.serializer(),
        )
}
