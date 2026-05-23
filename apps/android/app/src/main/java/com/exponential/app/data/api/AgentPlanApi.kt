package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The daemon submits plans via
// the same router; this Android surface only exposes the human-side
// approve / request-changes / retry actions.
@Serializable
private data class IssueIdInput(@SerialName("issueId") val issueId: String)

@Singleton
class AgentPlanApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun approvePlan(issueId: String) {
        trpc.mutation(
            path = "agentPlan.approvePlan",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
            outputSerializer = JsonElement.serializer(),
        )
    }

    suspend fun requestChanges(issueId: String) {
        trpc.mutation(
            path = "agentPlan.requestChanges",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
            outputSerializer = JsonElement.serializer(),
        )
    }

    suspend fun retry(issueId: String) {
        trpc.mutation(
            path = "agentPlan.retry",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
            outputSerializer = JsonElement.serializer(),
        )
    }
}
