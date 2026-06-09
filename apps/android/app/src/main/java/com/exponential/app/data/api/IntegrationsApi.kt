package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class GoogleStatus(
    val connected: Boolean,
    val scope: String? = null,
    val connectedAt: String? = null,
)

@Serializable
data class BackfillResult(val ok: Boolean = false, val scheduled: Int = 0)

// One repo the user's GitHub App can connect (mirrors web InstallationRepo).
// `private` is a Kotlin keyword so it's mapped via @SerialName.
@Serializable
data class GithubPickerRepo(
    val fullName: String,
    @SerialName("private") val isPrivate: Boolean = false,
    val defaultBranch: String = "main",
    val installationId: Long = 0,
)

// Result of integrations.github.repos — install state + the repo list.
@Serializable
data class GithubReposResult(
    val configured: Boolean = false,
    val installed: Boolean = false,
    val installUrl: String? = null,
    val repos: List<GithubPickerRepo> = emptyList(),
    val hasMore: Boolean = false,
)

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

    // Repos the user's GitHub App is installed on, for the connect-repo picker.
    suspend fun githubRepos(accountId: String): GithubReposResult =
        trpc.query(
            accountId,
            path = "integrations.github.repos",
            input = IntegrationsEmptyInput,
            inputSerializer = IntegrationsEmptyInput.serializer(),
            outputSerializer = GithubReposResult.serializer(),
        )
}
