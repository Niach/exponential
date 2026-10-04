package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// One changed file in an issue's PR. Mirrors the web PullFile (github-pr.ts).
@Serializable
data class PullFile(
    val filename: String,
    /** EXP-912: where a renamed/copied file came from — GitHub's raw key. */
    @SerialName("previous_filename") val previousFilename: String? = null,
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

/**
 * EXP-1154: the pull request's title + body as GitHub holds them
 * (`issues.prDescription`) — the Results face's fallback for an issue whose
 * open PR has no run report. Every field is null when no PR is linked.
 */
@Serializable
data class PrDescription(
    val repo: String? = null,
    val prNumber: Int? = null,
    val url: String? = null,
    val title: String? = null,
    val body: String? = null,
    val state: String? = null,
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

    // EXP-1154: the live GitHub title + body (`issues.prDescription`).
    suspend fun description(accountId: String, issueId: String): PrDescription =
        trpc.query(
            accountId,
            path = "issues.prDescription",
            input = PrFilesInput(issueId),
            inputSerializer = PrFilesInput.serializer(),
            outputSerializer = PrDescription.serializer(),
        )
}
