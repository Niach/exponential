package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * `codingSessions.mergePr` (EXP-734): the counterpart of `issues.mergePr` for
 * a run whose pull request links NO issue — an action or chat run that opened
 * one via MCP `exponential_pr_open({repositoryId, head})`. Nothing is
 * completed or moved: the server merges the PR, flips the session row's
 * `pr_state` to `merged` and (unless the team keeps sessions on merge) ends
 * the run, all of which arrive through Electric sync.
 */
@Serializable
data class MergeSessionPrInput(
    @SerialName("sessionId") val sessionId: String,
)

@Singleton
class CodingSessionsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * Squash-merge the run's own PR via the GitHub App. Errors match
     * `issues.mergePr` (conflicts, branch protection, GitHub App failures), so
     * callers keep rendering them through [com.exponential.app.domain.MergeFailure].
     */
    suspend fun mergePr(accountId: String, sessionId: String) {
        trpc.mutationUnit(
            accountId,
            path = "codingSessions.mergePr",
            input = MergeSessionPrInput(sessionId),
            inputSerializer = MergeSessionPrInput.serializer(),
        )
    }
}
