package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/notifications.ts — inbox mark-read.
@Serializable
private data class NotificationIdInput(@SerialName("id") val id: String)

@Serializable
private object NotificationsEmptyInput

@Singleton
class NotificationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun markRead(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "notifications.markRead",
            input = NotificationIdInput(id),
            inputSerializer = NotificationIdInput.serializer(),
        )
    }

    suspend fun markAllRead(accountId: String) {
        trpc.mutationUnit(
            accountId,
            path = "notifications.markAllRead",
            input = NotificationsEmptyInput,
            inputSerializer = NotificationsEmptyInput.serializer(),
        )
    }
}
