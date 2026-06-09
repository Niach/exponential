package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The agent submits plans via the
// same router; this Android surface exposes the human-side actions (approve /
// request-changes / retry / answer a question). There is no `getState` read
// anymore: every field the Plan Panel needs (plan text, question, revision,
// approval, lastError) is synced locally via the `agent_runs` Electric shape.
@Serializable
private data class IssueIdInput(@SerialName("issueId") val issueId: String)

@Serializable
private data class AnswerInput(
    @SerialName("issueId") val issueId: String,
    @SerialName("answer") val answer: String,
)

@Singleton
class AgentPlanApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun approvePlan(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "agentPlan.approvePlan",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
        )
    }

    suspend fun requestChanges(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "agentPlan.requestChanges",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
        )
    }

    suspend fun retry(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "agentPlan.retry",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
        )
    }

    /** Record the human's answer to the agent's open question. The server clears
     *  the question, flips the issue to `drafting`, and records an agent_answer
     *  event. */
    suspend fun answerQuestion(accountId: String, issueId: String, answer: String) {
        trpc.mutationUnit(
            accountId,
            path = "agentPlan.answerQuestion",
            input = AnswerInput(issueId, answer),
            inputSerializer = AnswerInput.serializer(),
        )
    }
}
