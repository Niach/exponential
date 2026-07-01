package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

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
