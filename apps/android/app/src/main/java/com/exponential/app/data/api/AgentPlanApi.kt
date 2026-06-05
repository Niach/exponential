package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The agent submits plans via the
// same router; this Android surface exposes the human-side actions (approve /
// request-changes / retry / answer a question) plus the structured plan/question
// read (`getState`) that backs the native Plan Panel.
@Serializable
private data class IssueIdInput(@SerialName("issueId") val issueId: String)

@Serializable
private data class AnswerInput(
    @SerialName("issueId") val issueId: String,
    @SerialName("answer") val answer: String,
)

/// The structured agent plan/question state for an issue. The plan/question TEXT
/// is server-only (not synced via Electric), so it's fetched on demand. Keys
/// match the camelCase tRPC payload.
@Serializable
data class AgentPlanState(
    val planText: String? = null,
    val question: String? = null,
    val questionAskedAt: String? = null,
    val state: String? = null,
    val revision: Int = 0,
    val approvedAt: String? = null,
)

@Singleton
class AgentPlanApi @Inject constructor(private val trpc: TrpcClient) {

    /** Read the structured plan/question text + state for an issue (tRPC query). */
    suspend fun getState(accountId: String, issueId: String): AgentPlanState =
        trpc.query(
            accountId,
            path = "agentPlan.getState",
            input = IssueIdInput(issueId),
            inputSerializer = IssueIdInput.serializer(),
            outputSerializer = AgentPlanState.serializer(),
        )

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
