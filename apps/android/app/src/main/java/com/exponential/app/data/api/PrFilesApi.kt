package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// One changed file in an issue's PR. Mirrors the web PullFile (github-pr.ts).
@Serializable
data class PullFile(
    val filename: String,
    val status: String,
    val additions: Int = 0,
    val deletions: Int = 0,
    val patch: String? = null,
)

@Serializable
data class PrFilesResult(
    val repo: String? = null,
    val prNumber: Int? = null,
    val files: List<PullFile> = emptyList(),
)

@Serializable
private data class PrFilesInput(@SerialName("issueId") val issueId: String)

@Singleton
class PrFilesApi @Inject constructor(private val trpc: TrpcClient) {

    // Live GitHub fetch via the server (issues.prFiles query) — not synced.
    suspend fun get(accountId: String, issueId: String): PrFilesResult =
        trpc.query(
            accountId,
            path = "issues.prFiles",
            input = PrFilesInput(issueId),
            inputSerializer = PrFilesInput.serializer(),
            outputSerializer = PrFilesResult.serializer(),
        )
}
