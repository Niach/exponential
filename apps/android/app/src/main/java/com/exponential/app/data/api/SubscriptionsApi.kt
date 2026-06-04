package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/subscriptions.ts — per-issue subscribe toggle.
@Serializable
private data class SubIssueIdInput(@SerialName("issueId") val issueId: String)

@Singleton
class SubscriptionsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun subscribe(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "subscriptions.subscribe",
            input = SubIssueIdInput(issueId),
            inputSerializer = SubIssueIdInput.serializer(),
        )
    }

    suspend fun unsubscribe(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "subscriptions.unsubscribe",
            input = SubIssueIdInput(issueId),
            inputSerializer = SubIssueIdInput.serializer(),
        )
    }
}
